// SPDX-License-Identifier: MPL-2.0
//! Direct transfer of an encrypted attachment over a loopback byte channel (ADR-0150, ADR-0158 sections 6 and 8, AT-06):
//! BlobID and ciphertext-digest binding, contiguous 16 KiB chunks, backpressure, resume after a drop, refusal states.

use om_core::attach::{self, AttachError, AttachmentRef};
use om_core::direct::*;
use om_core::host::BlobLimits;

const READ_CAP: [u8; 32] = [5; 32];

fn make(len: usize) -> (AttachmentRef, Vec<u8>, Vec<u8>) {
    let plain: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
    let (key, blob_id) = ([7u8; 32], [9u8; 32]);
    let ct = attach::seal(&key, &blob_id, &plain);
    let r = AttachmentRef {
        blob_id,
        media_type: "application/octet-stream".into(),
        plaintext_size: len as u64,
        ciphertext_digest: attach::ciphertext_digest(&ct),
        key,
        locations: vec![], // direct only: the schema allows no locations
        filename: None,
        width: None,
        height: None,
        duration_ms: None,
        read_cap: Some(READ_CAP.to_vec()),
    };
    (r, plain, ct)
}

fn sender(r: &AttachmentRef, ct: &[u8]) -> DirectSender {
    DirectSender::new(
        r.blob_id,
        r.ciphertext_digest,
        READ_CAP.to_vec(),
        ct.to_vec(),
    )
    .unwrap()
}

/// Deliver frames both ways until quiet. `tamper` may alter a frame on the wire; `cut_after_chunks` drops the channel.
fn run(
    s: &mut DirectSender,
    f: &mut DirectFetch,
    first: DirectFrame,
    mut ids: u8,
    cut_after_chunks: Option<usize>,
    tamper: &mut dyn FnMut(&mut DirectFrame),
) -> usize {
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
        for mut fr in to_f {
            if matches!(fr, DirectFrame::Chunk { .. }) {
                chunks += 1;
                if cut_after_chunks == Some(chunks) {
                    return chunks;
                }
            }
            tamper(&mut fr);
            to_s.extend(f.handle(&fr));
        }
    }
    chunks
}

fn no_tamper(_: &mut DirectFrame) {}

#[test]
fn large_file_goes_direct_bound_to_blobid_and_digest() {
    let (r, plain, ct) = make(5 * 1024 * 1024 + 123); // above a 4 MiB spool bound
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    let chunks = run(&mut s, &mut f, req, 0, None, &mut no_tamper);
    assert_eq!(chunks, ct.len().div_ceil(MAX_CHUNK));
    assert_eq!(f.state(), FetchState::Complete);
    assert_eq!(f.ciphertext().unwrap(), ct.as_slice());
    assert_eq!(f.open().unwrap(), plain);
    assert_eq!(s.state(), OfferState::Delivered);
    assert_eq!(f.durability(), Durability::LocalReceiptOnly);
}

#[test]
fn drop_then_resume_from_the_held_offset() {
    let (r, plain, ct) = make(300_000);
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    run(&mut s, &mut f, req, 0, Some(5), &mut no_tamper);
    // the sender emitted all chunks into the pump in one go, the channel dropped after 5 were delivered
    assert_eq!(f.state(), FetchState::Receiving);
    let held = f.held();
    assert_eq!(held, 4 * MAX_CHUNK as u64);
    // fresh channel: new request, resumed from `held`
    let req = f.request([2; 16]).unwrap();
    let chunks = run(&mut s, &mut f, req, 100, None, &mut no_tamper);
    assert_eq!(
        chunks,
        ((ct.len() as u64 - held) as usize).div_ceil(MAX_CHUNK)
    );
    assert_eq!(f.open().unwrap(), plain);
    assert_eq!(s.state(), OfferState::Delivered);
}

#[test]
fn one_flipped_bit_never_yields_plaintext() {
    let (r, _plain, ct) = make(100_000);
    let mut s = sender(&r, &ct);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    let mut seen = 0;
    run(&mut s, &mut f, req, 0, None, &mut |fr| {
        if let DirectFrame::Chunk { bytes, .. } = fr {
            seen += 1;
            if seen == 3 {
                bytes[10] ^= 1;
            }
        }
    });
    assert_eq!(f.state(), FetchState::Failed(AttachError::IntegrityFailure));
    assert_eq!(f.open(), Err(AttachError::IntegrityFailure));
    assert!(f.ciphertext().is_none());
    assert_eq!(f.held(), 0, "a failed object is wiped");
}

#[test]
fn a_sender_serving_other_bytes_is_caught_by_the_committed_digest() {
    // Wire digest and length are self-consistent, but the object is not the one the message committed to.
    let (r, _, _) = make(50_000);
    let (_, _, other_ct) = make(50_000);
    let mut other = other_ct.clone();
    other[0] ^= 0xff; // same length, different bytes
    let mut s = DirectSender::new(
        r.blob_id,
        attach::ciphertext_digest(&other),
        READ_CAP.to_vec(),
        other,
    )
    .unwrap();
    // it answers a fetch that names *its* digest; the recipient's ref has another, so use a ref with the sender's digest
    // check instead: the honest recipient asks with the committed digest and is refused outright.
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    run(&mut s, &mut f, req, 0, None, &mut no_tamper);
    assert_ne!(f.state(), FetchState::Complete);
    assert!(f.open().is_err());
}

#[test]
fn wrong_capability_blob_id_or_digest_gets_one_uniform_refusal() {
    let (r, _, ct) = make(1000);
    let mut s = sender(&r, &ct);
    let base = DirectFrame::FetchBlob {
        request_id: [1; 16],
        read_cap: READ_CAP.to_vec(),
        blob_id: r.blob_id,
        digest: r.ciphertext_digest,
    };
    let bad = [
        DirectFrame::FetchBlob {
            request_id: [1; 16],
            read_cap: vec![6; 32],
            blob_id: r.blob_id,
            digest: r.ciphertext_digest,
        },
        DirectFrame::FetchBlob {
            request_id: [1; 16],
            read_cap: READ_CAP.to_vec(),
            blob_id: [0; 32],
            digest: r.ciphertext_digest,
        },
        DirectFrame::FetchBlob {
            request_id: [1; 16],
            read_cap: READ_CAP.to_vec(),
            blob_id: r.blob_id,
            digest: [0; 32],
        },
    ];
    for b in &bad {
        assert_eq!(
            s.handle(b, [2; 16]),
            vec![DirectFrame::Error {
                id: [1; 16],
                code: ErrorCode::Unavailable
            }]
        );
    }
    assert_eq!(s.state(), OfferState::Waiting);
    assert!(matches!(
        s.handle(&base, [2; 16])[0],
        DirectFrame::Begin { .. }
    ));
}

#[test]
fn receiver_enforces_committed_length_framing_and_chunk_bounds() {
    let (r, _, ct) = make(40_000);
    let begin = |total: u64| DirectFrame::Begin {
        transfer_id: [3; 16],
        total_bytes: total,
        sha256: [0; 32],
    };
    // declared total differs from plaintext_size + 16: refused before any byte is accepted
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    f.request([1; 16]);
    f.handle(&begin(u64::MAX));
    assert!(matches!(f.state(), FetchState::Failed(_)));

    let ok_begin = begin(ct.len() as u64);
    let chunk = |off: u64, n: usize| DirectFrame::Chunk {
        transfer_id: [3; 16],
        offset: off,
        bytes: vec![0; n],
    };
    for bad in [chunk(5, 10), chunk(0, MAX_CHUNK + 1), chunk(0, 0)] {
        let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
        f.request([1; 16]);
        f.handle(&ok_begin);
        f.handle(&bad);
        assert!(matches!(f.state(), FetchState::Failed(_)), "{bad:?}");
    }
    // a chunk before any request or begin is refused too
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    f.handle(&chunk(0, 10));
    assert!(matches!(f.state(), FetchState::Failed(_)));
}

#[test]
fn backpressure_stops_at_high_water_and_resumes_at_low_water() {
    let (r, _, ct) = make(2 * 1024 * 1024);
    let mut s = sender(&r, &ct);
    let fetch = DirectFrame::FetchBlob {
        request_id: [1; 16],
        read_cap: READ_CAP.to_vec(),
        blob_id: r.blob_id,
        digest: r.ciphertext_digest,
    };
    let DirectFrame::Begin { transfer_id, .. } = s.handle(&fetch, [2; 16])[0].clone() else {
        panic!()
    };
    assert!(
        s.pump(0).is_empty(),
        "nothing until the receiver's first ack"
    );
    s.handle(
        &DirectFrame::Ack {
            transfer_id,
            offset: 0,
        },
        [0; 16],
    );
    let first = s.pump(0);
    assert_eq!(first.len(), HIGH_WATER / MAX_CHUNK);
    // still paused above low water
    assert!(s.pump(LOW_WATER + 1).is_empty());
    assert!(!s.pump(LOW_WATER).is_empty());
}

#[test]
fn offers_are_withdrawn_and_nothing_is_served_after() {
    let (r, _, ct) = make(1000);
    let mut s = sender(&r, &ct);
    s.withdraw();
    assert_eq!(s.state(), OfferState::Withdrawn);
    let mut f = DirectFetch::new(&r, [1; 16]).unwrap();
    let req = f.request([1; 16]).unwrap();
    run(&mut s, &mut f, req, 0, None, &mut no_tamper);
    assert_eq!(f.state(), FetchState::Idle, "refused: may try again later");
    assert!(f.open().is_err());
}

#[test]
fn sender_and_reference_validation_fail_closed() {
    let (mut r, _, ct) = make(1000);
    assert!(
        DirectSender::new(r.blob_id, [0; 32], READ_CAP.to_vec(), ct.clone()).err()
            == Some(AttachError::IntegrityFailure)
    );
    assert!(DirectSender::new(r.blob_id, r.ciphertext_digest, vec![1; 8], ct).is_err());
    r.read_cap = None;
    assert_eq!(
        DirectFetch::new(&r, [0; 16]).err(),
        Some(AttachError::NotFound)
    );
}

#[test]
fn too_big_for_the_spool_is_refused_before_sending_and_offered_direct() {
    let spool = Some(BlobLimits {
        max_blob_bytes: 4 * 1024 * 1024,
        max_spool_bytes: 0,
        max_blobs: 0,
    });
    assert!(matches!(
        plan_size(spool, 6 * 1024 * 1024),
        SizePlan::OfferDirect { .. }
    ));
    assert_eq!(plan_size(spool, 1024), SizePlan::Spool);
}
