// SPDX-License-Identifier: MPL-2.0
//! ADR-0194: the peer's final ack is only a claim; delivery needs an authenticated receipt bound to the transfer and its
//! digest. Sender and receiver state machines over a loopback channel; the engine paths are in `direct_engine.rs`.

use om_core::attach::{self, AttachmentRef};
use om_core::crypto::direct_receipt::receipt_tag;
use om_core::direct::*;

const READ_CAP: [u8; 32] = [5; 32];

fn make(len: usize) -> (AttachmentRef, Vec<u8>) {
    let plain: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
    let (key, blob_id) = ([7u8; 32], [9u8; 32]);
    let ct = attach::seal(&key, &blob_id, &plain);
    let r = AttachmentRef {
        blob_id,
        media_type: "application/octet-stream".into(),
        plaintext_size: len as u64,
        ciphertext_digest: attach::ciphertext_digest(&ct),
        key,
        locations: vec![],
        filename: None,
        width: None,
        height: None,
        duration_ms: None,
        read_cap: Some(READ_CAP.to_vec()),
    };
    (r, ct)
}

fn sender(r: &AttachmentRef, ct: &[u8]) -> DirectSender {
    DirectSender::new(
        r.blob_id,
        r.ciphertext_digest,
        READ_CAP.to_vec(),
        ct.to_vec(),
        r.key,
    )
    .unwrap()
}

/// Run the exchange until quiet. Frames the receiver sends back that are receipts are collected, not delivered, so a test
/// decides what reaches the sender. `cut` drops the channel after that many chunks.
fn run(
    s: &mut DirectSender,
    f: &mut DirectFetch,
    first: DirectFrame,
    mut ids: u8,
    cut: Option<usize>,
) -> Vec<DirectFrame> {
    let mut receipts = vec![];
    let mut to_s = vec![first];
    let mut chunks = 0;
    let mut guard = 0;
    while !to_s.is_empty() && guard < 100_000 {
        guard += 1;
        let mut to_f = vec![];
        for fr in to_s.drain(..) {
            ids = ids.wrapping_add(1);
            to_f.extend(s.handle(&fr, [ids; 16]));
        }
        to_f.extend(s.pump(0));
        for fr in to_f {
            if matches!(fr, DirectFrame::Chunk { .. }) {
                chunks += 1;
                if cut == Some(chunks) {
                    return receipts;
                }
            }
            for back in f.handle(&fr) {
                if matches!(back, DirectFrame::Receipt { .. }) {
                    receipts.push(back);
                } else {
                    to_s.push(back);
                }
            }
        }
    }
    receipts
}

/// A finished, committed exchange: the sender holds a claim, the receipt is returned (not yet delivered).
fn claimed(
    len: usize,
) -> (
    AttachmentRef,
    Vec<u8>,
    DirectSender,
    DirectFetch,
    DirectFrame,
) {
    let (r, ct) = make(len);
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    assert!(
        run(&mut s, &mut f, req, 0, None).is_empty(),
        "no receipt before the commit"
    );
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    let mut rc = f.committed();
    assert_eq!(rc.len(), 1);
    (r, ct, s, f, rc.remove(0))
}

fn tid(fr: &DirectFrame) -> [u8; 16] {
    match fr {
        DirectFrame::Receipt { transfer_id, .. } => *transfer_id,
        _ => panic!("not a receipt"),
    }
}

#[test]
fn a_final_ack_alone_is_a_claim_and_the_object_is_kept() {
    let (r, ct, mut s, _f, _rc) = claimed(100_000);
    assert_eq!(s.durability(), Durability::PeerClaim);
    // A new fetch still gets the object: nothing was wiped at the claim, and the claim is not downgraded.
    let req = DirectFrame::FetchBlob {
        request_id: [3; 16],
        read_cap: READ_CAP.to_vec(),
        blob_id: r.blob_id,
        digest: r.ciphertext_digest,
    };
    let out = s.handle(&req, [4; 16]);
    assert!(
        matches!(&out[..], [DirectFrame::Begin { total_bytes, .. }] if *total_bytes == ct.len() as u64)
    );
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
}

#[test]
fn a_forged_final_ack_never_becomes_delivered() {
    let (r, ct) = make(50_000);
    let mut s = sender(&r, &ct);
    let req = DirectFrame::FetchBlob {
        request_id: [1; 16],
        read_cap: READ_CAP.to_vec(),
        blob_id: r.blob_id,
        digest: r.ciphertext_digest,
    };
    let Some(DirectFrame::Begin { transfer_id, .. }) = s.handle(&req, [2; 16]).pop() else {
        panic!("begin")
    };
    // An ack of the whole length before `object_end` changes nothing.
    s.handle(
        &DirectFrame::Ack {
            transfer_id,
            offset: ct.len() as u64,
        },
        [0; 16],
    );
    s.pump(0); // sends the end
    assert_eq!(s.state(), OfferState::Transferring);
    s.handle(
        &DirectFrame::Ack {
            transfer_id,
            offset: ct.len() as u64,
        },
        [0; 16],
    );
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    // No receipt exists: the state can never be Confirmed by acks, whatever their number.
    for _ in 0..3 {
        s.handle(
            &DirectFrame::Ack {
                transfer_id,
                offset: ct.len() as u64,
            },
            [0; 16],
        );
    }
    assert_ne!(s.state(), OfferState::Confirmed);
}

#[test]
fn a_valid_receipt_confirms_and_drops_the_object_and_a_replay_does_nothing() {
    let (r, _ct, mut s, _f, rc) = claimed(100_000);
    assert!(s.handle(&rc, [0; 16]).is_empty());
    assert_eq!(s.state(), OfferState::Confirmed);
    assert_eq!(s.durability(), Durability::PeerLocalCopyConfirmed);
    assert!(
        s.handle(&rc, [0; 16]).is_empty(),
        "a replay changes nothing"
    );
    assert_eq!(s.state(), OfferState::Confirmed);
    let req = DirectFrame::FetchBlob {
        request_id: [3; 16],
        read_cap: READ_CAP.to_vec(),
        blob_id: r.blob_id,
        digest: r.ciphertext_digest,
    };
    assert!(matches!(
        s.handle(&req, [4; 16])[..],
        [DirectFrame::Error {
            code: ErrorCode::Unavailable,
            ..
        }]
    ));
}

#[test]
fn forged_replayed_and_misbound_tags_are_ignored_without_failing_the_transfer() {
    let (r, ct, mut s, _f, rc) = claimed(100_000);
    let id = tid(&rc);
    let forged = |tag: [u8; 32]| DirectFrame::Receipt {
        transfer_id: id,
        tag,
    };
    // random tag, and a tag under another key, blob id, digest, transfer id and object
    let mut other_ct = ct.clone();
    other_ct[40] ^= 1;
    let bad = [
        [0u8; 32],
        receipt_tag(&[8; 32], &r.ciphertext_digest, &r.blob_id, &id, &ct),
        receipt_tag(&r.key, &r.ciphertext_digest, &[1; 32], &id, &ct),
        receipt_tag(&r.key, &[2; 32], &r.blob_id, &id, &ct),
        receipt_tag(&r.key, &r.ciphertext_digest, &r.blob_id, &[0xee; 16], &ct),
        receipt_tag(&r.key, &r.ciphertext_digest, &r.blob_id, &id, &other_ct),
        receipt_tag(
            &r.key,
            &r.ciphertext_digest,
            &r.blob_id,
            &id,
            &ct[..ct.len() - 1],
        ),
    ];
    for t in bad {
        assert!(
            s.handle(&forged(t), [0; 16]).is_empty(),
            "no error, no failure"
        );
        assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    }
    // a valid tag for another transfer id is not accepted for the claimed one, and the right one still works
    let other_id = receipt_tag(&r.key, &r.ciphertext_digest, &r.blob_id, &[0xee; 16], &ct);
    s.handle(
        &DirectFrame::Receipt {
            transfer_id: [0xee; 16],
            tag: other_id,
        },
        [0; 16],
    );
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    s.handle(&rc, [0; 16]);
    assert_eq!(s.state(), OfferState::Confirmed);
}

#[test]
fn invalid_receipt_comparisons_are_bounded_for_one_transfer() {
    let (_r, _ct, mut s, _f, rc) = claimed(20_000);
    let bad = DirectFrame::Receipt {
        transfer_id: tid(&rc),
        tag: [1; 32],
    };
    for _ in 0..MAX_BAD_RECEIPTS {
        s.handle(&bad, [0; 16]);
    }
    // This transfer's comparison window closes; recovery requires a fresh transfer ID.
    s.handle(&rc, [0; 16]);
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
}

/// One full fetch on a fresh channel: the receiver already holds the object, so only the exchange runs. Returns the
/// receiver's receipt (not delivered) and leaves the sender holding a new claim.
fn claim_again(s: &mut DirectSender, f: &mut DirectFetch, ids: u8) -> DirectFrame {
    let req = f.request([ids; 16]).unwrap();
    let out = s.handle(&req, [ids.wrapping_add(1); 16]);
    let ack = f.handle(&out[0]);
    s.handle(&ack[0], [0; 16]);
    let end = s.pump(0);
    let back = f.handle(&end[0]);
    s.handle(&back[0], [0; 16]);
    back[1].clone()
}

#[test]
fn asking_again_does_not_renew_the_whole_object_hash_budget() {
    let (_r, _ct, mut s, mut f, rc) = claimed(20_000);
    let mut latest = rc;
    for round in 0..MAX_RECEIPT_HASHES {
        let bad = DirectFrame::Receipt {
            transfer_id: tid(&latest),
            tag: [round + 1; 32],
        };
        for _ in 0..MAX_BAD_RECEIPTS {
            s.handle(&bad, [0; 16]);
        }
        latest = claim_again(&mut s, &mut f, 20 + round);
        assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    }
    // Each distinct claim costs at most one whole-object hash; the offer-wide budget is now exhausted.
    s.handle(&latest, [0; 16]);
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
}

#[test]
fn a_receipt_with_no_claim_is_ignored() {
    let (r, ct) = make(30_000);
    let mut s = sender(&r, &ct);
    let tag = receipt_tag(&r.key, &r.ciphertext_digest, &r.blob_id, &[1; 16], &ct);
    s.handle(
        &DirectFrame::Receipt {
            transfer_id: [1; 16],
            tag,
        },
        [0; 16],
    );
    assert_eq!(s.state(), OfferState::Waiting);
}

#[test]
fn the_receiver_sends_no_receipt_until_the_host_reports_the_commit() {
    let (r, ct) = make(100_000);
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    assert!(run(&mut s, &mut f, req, 0, None).is_empty());
    assert_eq!(f.state(), FetchState::Complete);
    assert!(f.take_receipt(&[1; 16]).is_none());
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    assert_eq!(f.committed().len(), 1);
    assert_eq!(f.state(), FetchState::Committed);
    assert_eq!(f.open().unwrap().len(), 100_000);
}

#[test]
fn resume_from_the_middle_is_confirmed_like_a_fresh_transfer() {
    let (r, ct) = make(300_000);
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    run(&mut s, &mut f, req, 0, Some(5));
    assert!(f.held() > 0 && f.held() < ct.len() as u64);
    assert_eq!(s.state(), OfferState::Transferring);
    let req = f.request([2; 16]).unwrap();
    assert!(run(&mut s, &mut f, req, 100, None).is_empty());
    assert_eq!(
        s.state(),
        OfferState::PeerClaimsReceived,
        "an initial offset never claims"
    );
    let rc = f.committed().remove(0);
    s.handle(&rc, [0; 16]);
    assert_eq!(s.state(), OfferState::Confirmed);
}

#[test]
fn a_lost_receipt_is_recovered_by_asking_again_with_the_whole_total_held() {
    let (r, ct, mut s, mut f, lost) = claimed(100_000);
    drop(lost); // the receipt never reached the sender
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    // a fresh channel: the receiver already holds everything and acks the full total right after `object_begin`
    let req = f.request([7; 16]).unwrap();
    let out = s.handle(&req, [8; 16]);
    let DirectFrame::Begin { transfer_id, .. } = out[0] else {
        panic!("begin")
    };
    let ack = f.handle(&out[0]);
    assert!(matches!(&ack[..], [DirectFrame::Ack { offset, .. }] if *offset == ct.len() as u64));
    s.handle(&ack[0], [0; 16]);
    let end = s.pump(0);
    assert!(
        matches!(&end[..], [DirectFrame::End { .. }]),
        "no chunk at all, then the end"
    );
    let back = f.handle(&end[0]);
    assert!(matches!(&back[0], DirectFrame::Ack { .. }));
    let DirectFrame::Receipt {
        transfer_id: rid, ..
    } = back[1]
    else {
        panic!("receipt")
    };
    assert_eq!(rid, transfer_id, "the fresh transfer id, never the old tag");
    s.handle(&back[0], [0; 16]);
    s.handle(&back[1], [0; 16]);
    assert_eq!(s.state(), OfferState::Confirmed);
    let _ = r;
}

#[test]
fn a_receipt_from_another_transfer_cannot_be_replayed() {
    let (r, _ct, mut s, mut f, old) = claimed(60_000);
    let req = f.request([7; 16]).unwrap();
    let out = s.handle(&req, [8; 16]);
    let ack = f.handle(&out[0]);
    s.handle(&ack[0], [0; 16]);
    let end = s.pump(0);
    // the old receipt names the old transfer id: with the claim now moved to the new transfer it does not verify
    let back = f.handle(&end[0]);
    s.handle(&back[0], [0; 16]); // the final ack: the claim now names the new transfer
    s.handle(&old, [0; 16]);
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    s.handle(&back[1], [0; 16]);
    assert_eq!(s.state(), OfferState::Confirmed);
    let _ = r;
}

#[test]
fn a_sender_that_answers_a_receipt_with_an_error_keeps_the_verified_copy() {
    let (_r, _ct, _s, mut f, rc) = claimed(60_000);
    // an older sender answers an unknown frame with an error under an id of its own
    assert!(
        f.handle(&DirectFrame::Error {
            id: [0xab; 16],
            code: ErrorCode::Protocol
        })
        .is_empty()
    );
    assert_eq!(f.state(), FetchState::Committed);
    assert!(f.take_receipt(&tid(&rc)).is_none(), "receipts stop");
    assert_eq!(
        f.open().unwrap().len(),
        60_000,
        "the verified object is never discarded"
    );
}

#[test]
fn a_verified_receiver_is_never_failed_by_odd_frames() {
    let (_r, _ct, _s, mut f, _rc) = claimed(60_000);
    for fr in [
        DirectFrame::Chunk {
            transfer_id: [1; 16],
            offset: 0,
            bytes: vec![1],
        },
        DirectFrame::Begin {
            transfer_id: [9; 16],
            total_bytes: 3,
            sha256: [0; 32],
        },
        DirectFrame::End {
            transfer_id: [9; 16],
        },
    ] {
        f.handle(&fr);
        assert_eq!(f.state(), FetchState::Committed);
    }
    assert!(f.open().is_ok());
}

#[test]
fn eight_wrong_receipts_can_recover_on_a_fresh_transfer() {
    let (_r, _ct, mut s, mut f, rc) = claimed(100_000);
    for _ in 0..MAX_BAD_RECEIPTS {
        s.handle(
            &DirectFrame::Receipt {
                transfer_id: tid(&rc),
                tag: [0; 32],
            },
            [0; 16],
        );
    }
    assert_eq!(s.state(), OfferState::PeerClaimsReceived);
    let req = f.request([7; 16]).unwrap();
    let receipts = run(&mut s, &mut f, req, 20, None);
    assert_eq!(receipts.len(), 1);
    assert_ne!(tid(&receipts[0]), tid(&rc));
    s.handle(&receipts[0], [0; 16]);
    assert_eq!(s.state(), OfferState::Confirmed);
}
