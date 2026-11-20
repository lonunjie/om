// SPDX-License-Identifier: MPL-2.0
//! Attachment send and receive against the loopback BlobStore (STORAGE_AND_BLOB_ARCHITECTURE, ADR-0150, ADR-0148,
//! ADR-0149, ADR-0155): client-side encryption, digest verified before decrypt, typed refusals before sending, uncertain
//! outcomes reconciled by exact retry, and the voice-note and location-card presentation extensions.

mod common;

use om_core::api::*;
use om_core::attach::{AttachError, AttachmentRef, Meta};
use om_core::crypto::cbor::{self, V};
use om_core::crypto::{hex, unhex};
use om_core::events::{self, Header, Hlc};
use om_core::host::{BlobLimits, NetCall};
use om_core::present::{self, EXT_LOCATION, EXT_VOICE};
use om_core::runtime::{Fault, LoopbackNetwork, Runtime, StoreLimits, settle_all};

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn active_pair() -> Pair {
    active_pair_with_expressive(false, false)
}

fn active_pair_with_expressive(alice_enabled: bool, bob_enabled: bool) -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    if alice_enabled {
        alice.core.enable_expressive_media();
    }
    if bob_enabled {
        bob.core.enable_expressive_media();
    }
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let conv = |rt: &Runtime, c: &ProfileExecutionContext| {
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: c.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        l[0].id.clone()
    };
    let (a_conv, b_conv) = (conv(&alice, &a), conv(&bob, &b));
    Pair {
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

fn view(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<TimelineItem> {
    rt.query::<ConversationView>(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    })
    .timeline
}

fn parts(it: &TimelineItem) -> (&str, &str, &[AttachmentView], &Option<PresentationView>) {
    match it {
        TimelineItem::Outgoing {
            message_id,
            text,
            attachments,
            presentation,
            ..
        }
        | TimelineItem::Incoming {
            message_id,
            text,
            attachments,
            presentation,
            ..
        } => (message_id, text, attachments, presentation),
    }
}

fn png(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

fn meta(t: &str) -> Meta {
    Meta {
        media_type: t.into(),
        filename: Some("holiday.png".into()),
        ..Default::default()
    }
}

fn input(p: &PreparedAttachment) -> AttachmentInput {
    AttachmentInput {
        blob_id: p.blob_id.clone(),
        voice_note: None,
    }
}

fn send(p: &mut Pair, op: &str, text: &str, atts: Vec<AttachmentInput>) {
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendAttachments {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: text.into(),
                reply_to: None,
            },
            attachments: atts,
        },
    );
}

fn settle(p: &mut Pair) {
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

fn app(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
    rt.core.debug_app_state(ctx).unwrap()
}

/// The content key of Alice's `n`th outgoing message attachment, read from her private state.
fn key_of(p: &Pair, n: usize) -> [u8; 32] {
    let st = app(&p.alice, &p.a);
    let m = st["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| !m["attachments"].as_array().unwrap().is_empty())
        .unwrap()
        .clone();
    let r = unhex(m["attachments"][n]["reference"].as_str().unwrap()).unwrap();
    AttachmentRef::from_v(&cbor::decode(&r).unwrap())
        .unwrap()
        .key
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

// ---------------------------------------------------------------------------------------------- round trip

#[test]
fn an_attachment_is_encrypted_on_the_device_stored_and_opened_by_the_peer() {
    let mut p = active_pair();
    let plain = png(3000);
    let prep = p
        .alice
        .prepare_attachment(&p.a, &plain, meta("image/png"))
        .unwrap();
    assert_eq!(prep.ciphertext_len, 3016);
    send(&mut p, "s1", "look", vec![input(&prep)]);
    // Nothing is sent as a message before the blob is stored: the upload goes first.
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::PendingNetwork)
    );
    settle(&mut p);
    assert_eq!(p.alice.outcome(&p.a, "s1"), Some(OutcomeClass::Committed));
    assert_eq!(p.net.blob_count(), 1);

    // The sender's view: ours, stored for later (not "delivered"), with the store's own expiry.
    let av = view(&p.alice, &p.a, &p.a_conv);
    let (mid, text, atts, _) = parts(&av[0]);
    assert_eq!(text, "look");
    assert_eq!(atts[0].state, AttachmentState::StoredForLater);
    assert!(atts[0].stored_until.unwrap() > 0);
    assert_eq!(
        (atts[0].size, atts[0].media_type.as_str()),
        (3000, "image/png")
    );
    let mid = mid.to_string();

    // The receiver's view: remote, nothing fetched on its own.
    let bv = view(&p.bob, &p.b, &p.b_conv);
    let (bmid, btext, batts, pres) = parts(&bv[0]);
    assert_eq!((btext, bmid), ("look", mid.as_str()));
    assert_eq!(batts.len(), 1);
    assert_eq!(batts[0].state, AttachmentState::Remote);
    assert_eq!(batts[0].filename.as_deref(), Some("holiday.png"));
    assert!(pres.is_none());
    let opened = p.bob.fetch_attachment(&p.b, &mid, 0).unwrap();
    assert_eq!(opened.plaintext, plain);
    assert_eq!(opened.media_type, "image/png");

    // The BlobStore saw ciphertext only; the relay stream carries neither the plaintext, the key nor a file name.
    let key = key_of(&p, 0);
    for ct in p.net.blob_ciphertexts() {
        assert!(!contains(&ct, &plain[..64]), "plaintext reached the store");
        assert!(!contains(&ct, &key));
        assert!(!contains(&ct, b"holiday"));
    }
    for dep in p.net.all_deposited_bytes() {
        assert!(!contains(&dep, &key), "content key left the E2EE event");
        assert!(!contains(&dep, &plain[..64]));
        assert!(!contains(&dep, b"holiday"));
    }
    // Our own other devices see the same: the message is also a self-sync copy target (single device here: none).
}

#[test]
fn an_attachment_only_message_has_no_text() {
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(500), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "", vec![input(&prep)]);
    settle(&mut p);
    let bv = view(&p.bob, &p.b, &p.b_conv);
    let (_, text, atts, _) = parts(&bv[0]);
    assert_eq!((text, atts.len()), ("", 1));
}

#[test]
fn bytes_that_do_not_match_the_digest_never_decrypt() {
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(2000), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    settle(&mut p);
    let mid = parts(&view(&p.bob, &p.b, &p.b_conv)[0]).0.to_string();
    p.net.corrupt_blobs();
    assert_eq!(
        p.bob
            .fetch_attachment(&p.b, &mid, 0)
            .map(|o| o.plaintext.len()),
        Err(AttachError::IntegrityFailure)
    );
    // Direct: a truncated or extended object, or bytes of another blob, is refused by Core itself.
    let good = p
        .alice
        .core
        .restage_attachment(&p.a, &prep.blob_id, &png(2000))
        .unwrap();
    assert!(p.bob.core.open_attachment(&p.b, &mid, 0, &good).is_ok());
    assert_eq!(
        p.bob
            .core
            .open_attachment(&p.b, &mid, 0, &good[..good.len() - 1]),
        Err(AttachError::IntegrityFailure)
    );
    let mut longer = good.clone();
    longer.push(0);
    assert_eq!(
        p.bob.core.open_attachment(&p.b, &mid, 0, &longer),
        Err(AttachError::IntegrityFailure)
    );
    // The store forgot the object: not found, nothing else.
    p.net.expire_blobs();
    assert_eq!(
        p.bob
            .fetch_attachment(&p.b, &mid, 0)
            .map(|o| o.plaintext.len()),
        Err(AttachError::NotFound)
    );
}

#[test]
fn a_restaged_ciphertext_is_identical_and_wrong_plaintext_is_refused() {
    let mut p = active_pair();
    let plain = png(1234);
    let prep = p
        .alice
        .prepare_attachment(&p.a, &plain, meta("image/png"))
        .unwrap();
    let again = p
        .alice
        .core
        .restage_attachment(&p.a, &prep.blob_id, &plain)
        .unwrap();
    assert_eq!(again, prep.ciphertext);
    let mut other = plain.clone();
    other[5] ^= 1;
    assert_eq!(
        p.alice.core.restage_attachment(&p.a, &prep.blob_id, &other),
        Err(AttachError::IntegrityFailure)
    );
}

// ---------------------------------------------------------------------------------------------- refusals before sending

#[test]
fn an_object_over_the_advertised_limit_is_refused_before_anything_is_sent() {
    let mut p = active_pair();
    let before = (p.net.deposit_count(), p.net.blob_put_count());
    let err = p
        .alice
        .prepare_attachment(&p.a, &png(5 * 1024 * 1024), meta("image/png"))
        .map(|_| ())
        .unwrap_err();
    assert_eq!(
        err,
        AttachError::TooLarge {
            max: 4 * 1024 * 1024 - 16
        }
    );
    assert_eq!(err.reject_reason(), RejectReason::TooLarge);
    assert!(!err.reject_reason().retryable());
    // exactly at the limit is fine
    assert!(
        p.alice
            .prepare_attachment(&p.a, &png(4 * 1024 * 1024 - 16), meta("image/png"))
            .is_ok()
    );
    assert_eq!((p.net.deposit_count(), p.net.blob_put_count()), before);
    // no inline fallback exists: the timeline is untouched
    assert!(view(&p.alice, &p.a, &p.a_conv).is_empty());
}

#[test]
fn limits_that_shrank_after_preparation_refuse_the_send_with_a_typed_reason() {
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(100_000), meta("image/png"))
        .unwrap();
    p.alice.set_blob_limits(Some(BlobLimits {
        max_blob_bytes: 50_000,
        max_spool_bytes: 0,
        max_blobs: 0,
    }));
    send(&mut p, "s1", "x", vec![input(&prep)]);
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::TooLarge
        })
    );
    assert!(view(&p.alice, &p.a, &p.a_conv).is_empty());
    assert_eq!(p.net.blob_put_count(), 0);
}

#[test]
fn no_advertised_blob_store_means_no_attachments_and_text_still_works() {
    let mut p = active_pair();
    p.alice.set_blob_limits(None);
    assert_eq!(
        p.alice
            .prepare_attachment(&p.a, &png(10), meta("image/png"))
            .map(|_| ()),
        Err(AttachError::NoBlobStore)
    );
    p.alice.dispatch(
        &p.a,
        "t",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "hi".into(),
                reply_to: None,
            },
        },
    );
    settle(&mut p);
    assert_eq!(parts(&view(&p.bob, &p.b, &p.b_conv)[0]).1, "hi");
}

#[test]
fn a_store_that_refuses_the_object_is_a_typed_rejection_and_no_message_leaves() {
    let mut p = active_pair();
    // Core believes 4 MiB; the store only takes 1 KiB (payload_too_large at the store).
    p.net.set_store_limits(StoreLimits {
        max_blob_bytes: 1024,
        ..Default::default()
    });
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(5000), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    let before = p.net.deposit_count();
    settle(&mut p);
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::TooLarge
        })
    );
    let av = view(&p.alice, &p.a, &p.a_conv);
    assert!(matches!(
        &av[0],
        TimelineItem::Outgoing {
            delivery: DeliveryState::Rejected {
                reason: RejectReason::TooLarge
            },
            ..
        }
    ));
    assert_eq!(p.net.deposit_count(), before, "no message was deposited");
    assert!(view(&p.bob, &p.b, &p.b_conv).is_empty());
}

#[test]
fn a_full_store_is_quota_and_the_exact_retry_reuses_the_same_blob() {
    let mut p = active_pair();
    p.net.set_store_limits(StoreLimits {
        max_blobs: 0,
        ..Default::default()
    });
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(800), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    settle(&mut p);
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Quota
        })
    );
    let mid = parts(&view(&p.alice, &p.a, &p.a_conv)[0]).0.to_string();
    p.net.set_store_limits(StoreLimits::default());
    p.alice.dispatch(
        &p.a,
        "r1",
        IntentBody::RetryDelivery {
            message_id: mid.clone(),
        },
    );
    settle(&mut p);
    assert_eq!(p.alice.outcome(&p.a, "r1"), Some(OutcomeClass::Committed));
    let bv = view(&p.bob, &p.b, &p.b_conv);
    assert_eq!(
        parts(&bv[0]).2[0].blob_id,
        prep.blob_id,
        "BlobID is stable across the retry"
    );
    assert!(p.bob.fetch_attachment(&p.b, &mid, 0).is_ok());
    assert_eq!(p.net.blob_count(), 1);
}

// ---------------------------------------------------------------------------------------------- uncertain outcomes

#[test]
fn a_timeout_after_the_put_is_pending_reconciliation_and_the_retry_is_the_same_put() {
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(900), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    let first = match &p.alice.core.net_ops(&p.a).unwrap()[0].call {
        NetCall::PutBlob {
            blob_id,
            owner_cap,
            request_id,
            ..
        } => (blob_id.clone(), owner_cap.clone(), request_id.clone()),
        c => panic!("{c:?}"),
    };
    p.net.inject(Fault::TimeoutAfterEffect);
    p.alice.flush_net(&p.a);
    // It may have been stored: never a rejection, and the message did not go out.
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::PendingReconciliation)
    );
    assert_eq!(p.net.blob_count(), 1);
    assert!(matches!(
        &view(&p.alice, &p.a, &p.a_conv)[0],
        TimelineItem::Outgoing {
            delivery: DeliveryState::PendingReconciliation,
            ..
        }
    ));
    // The very same call is still owed, byte for byte.
    match &p.alice.core.net_ops(&p.a).unwrap()[0].call {
        NetCall::PutBlob {
            blob_id,
            owner_cap,
            request_id,
            ..
        } => {
            assert_eq!(
                (blob_id.clone(), owner_cap.clone(), request_id.clone()),
                first
            );
        }
        c => panic!("{c:?}"),
    }
    settle(&mut p);
    assert_eq!(p.alice.outcome(&p.a, "s1"), Some(OutcomeClass::Committed));
    assert_eq!(p.net.blob_count(), 1, "one object, not two");
    assert_eq!(p.net.blob_put_count(), 1);
    assert_eq!(view(&p.bob, &p.b, &p.b_conv).len(), 1);
}

#[test]
fn an_unreachable_store_is_pending_network_and_survives_a_restart() {
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(700), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    p.net.inject(Fault::Unavailable);
    p.alice.flush_net(&p.a);
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::PendingNetwork)
    );
    // The process dies; the identities were persisted before the first effect.
    let ctxs = p.alice.restart();
    p.a = ctxs[0].clone();
    settle(&mut p);
    assert_eq!(p.alice.outcome(&p.a, "s1"), Some(OutcomeClass::Committed));
    assert_eq!(
        parts(&view(&p.bob, &p.b, &p.b_conv)[0]).2[0].blob_id,
        prep.blob_id
    );
    assert_eq!(p.net.blob_put_count(), 1);
}

#[test]
fn a_store_answering_with_another_digest_fails_closed() {
    // A receipt whose digest is not ours means the store holds other bytes: the message is refused, nothing is sent.
    let mut p = active_pair();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(600), meta("image/png"))
        .unwrap();
    send(&mut p, "s1", "x", vec![input(&prep)]);
    let op = p.alice.core.net_ops(&p.a).unwrap().remove(0);
    let bad = om_core::host::BlobReceipt {
        read_cap: vec![1; 32],
        locations: vec!["https://blobs.loopback.test/v1/blobs/00".into()],
        digest: [9; 32],
        expires_at: Some(1_790_100_000),
    }
    .to_bytes();
    let step = p
        .alice
        .core
        .net_result(&p.a, op.id, om_core::host::NetResult::Bytes { bytes: bad })
        .unwrap();
    p.alice.settle(&step);
    assert_eq!(
        p.alice.outcome(&p.a, "s1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::IntegrityFailure
        })
    );
    assert!(view(&p.bob, &p.b, &p.b_conv).is_empty());
}

// ---------------------------------------------------------------------------------------------- voice note

fn voice_input(p: &PreparedAttachment, waveform: Option<Vec<u8>>) -> AttachmentInput {
    AttachmentInput {
        blob_id: p.blob_id.clone(),
        voice_note: Some(VoiceNoteInput {
            waveform: waveform.map(|w| hex(&w)),
        }),
    }
}

fn audio_meta() -> Meta {
    Meta {
        media_type: "audio/ogg".into(),
        duration_ms: Some(4200),
        ..Default::default()
    }
}

#[test]
fn a_voice_note_is_an_audio_attachment_with_a_bounded_waveform_hint() {
    let mut p = active_pair();
    let wave: Vec<u8> = (0..64).map(|i| (i * 4) as u8).collect();
    let prep = p
        .alice
        .prepare_attachment(&p.a, &png(2400), audio_meta())
        .unwrap();
    send(
        &mut p,
        "v",
        "",
        vec![voice_input(&prep, Some(wave.clone()))],
    );
    settle(&mut p);
    let bv = view(&p.bob, &p.b, &p.b_conv);
    let (mid, _, atts, pres) = parts(&bv[0]);
    assert_eq!(atts[0].duration_ms, Some(4200));
    let vn = pres.as_ref().unwrap().voice_note.as_ref().unwrap();
    assert_eq!(
        (vn.attachment_index, vn.waveform.as_deref()),
        (0, Some(&wave[..]))
    );
    assert!(p.bob.fetch_attachment(&p.b, mid, 0).is_ok());
}

#[test]
fn a_voice_note_must_be_audio_with_a_duration_and_a_sane_waveform() {
    let mut p = active_pair();
    let audio = p
        .alice
        .prepare_attachment(&p.a, &png(300), audio_meta())
        .unwrap();
    let image = p
        .alice
        .prepare_attachment(&p.a, &png(300), meta("image/png"))
        .unwrap();
    let no_dur = p
        .alice
        .prepare_attachment(
            &p.a,
            &png(300),
            Meta {
                media_type: "audio/ogg".into(),
                ..Default::default()
            },
        )
        .unwrap();
    for (op, att) in [
        ("a", voice_input(&image, None)),
        ("b", voice_input(&no_dur, None)),
        ("c", voice_input(&audio, Some(vec![1; 3]))),
        ("d", voice_input(&audio, Some(vec![1; 300]))),
    ] {
        send(&mut p, op, "", vec![att]);
        assert_eq!(
            p.alice.outcome(&p.a, op),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Invalid
            }),
            "{op}"
        );
    }
    assert!(view(&p.alice, &p.a, &p.a_conv).is_empty());
}

// ---------------------------------------------------------------------------------------------- hostile senders

struct Author {
    canon: [u8; 32],
    acct: [u8; 32],
    dev: [u8; 16],
}

fn author(p: &Pair) -> Author {
    let st = app(&p.bob, &p.b);
    let r = st["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    let h = |k: &str| unhex(r[k].as_str().unwrap()).unwrap();
    Author {
        canon: h("canonId").try_into().unwrap(),
        acct: h("peerAccountId").try_into().unwrap(),
        dev: h("peerDeviceId").try_into().unwrap(),
    }
}

/// Alice's Core sends an arbitrary, correctly authenticated event (a sender that builds its own events).
fn alice_sends(p: &mut Pair, payload: &[u8]) {
    let step = p
        .alice
        .core
        .debug_inject_payload(&p.a, &p.a_conv, payload)
        .unwrap();
    p.alice.settle(&step);
    settle(p);
}

fn create_event(
    au: &Author,
    id: u8,
    ms: u64,
    text: Option<&str>,
    atts: Vec<V>,
    ext: Option<V>,
) -> Vec<u8> {
    Header {
        event_id: [id; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: ms,
            logical: 0,
        },
        kind: events::K_CREATE,
        body: events::message_create_body_with(text, atts),
    }
    .encode_with(ext)
}

fn some_ref(media: &str, dur: Option<u64>) -> AttachmentRef {
    AttachmentRef {
        blob_id: [3; 32],
        media_type: media.into(),
        plaintext_size: 100,
        ciphertext_digest: [4; 32],
        key: [5; 32],
        locations: vec![
            "https://blobs.loopback.test/v1/blobs/aa".into(),
            "http://evil.example/x".into(),
        ],
        filename: Some("../../etc/passwd".into()),
        width: None,
        height: None,
        duration_ms: dur,
        read_cap: Some(vec![6; 32]),
    }
}

fn ext_entry(id: &str, critical: bool, value: V) -> V {
    V::TM(vec![(
        id.into(),
        V::M(vec![(0, V::Bool(critical)), (1, value)]),
    )])
}

#[test]
fn malformed_presentation_extensions_are_ignored_and_the_message_still_arrives() {
    let mut p = active_pair();
    let au = author(&p);
    // a 3-sample waveform on audio; a voice note on an image; a voice note on an index that does not exist
    let cases: Vec<(u8, AttachmentRef, V)> = vec![
        (
            1,
            some_ref("audio/ogg", Some(1000)),
            V::M(vec![(0, V::U(0)), (1, V::B(vec![1; 3]))]),
        ),
        (2, some_ref("image/png", None), V::M(vec![(0, V::U(0))])),
        (
            3,
            some_ref("audio/ogg", Some(1000)),
            V::M(vec![(0, V::U(5))]),
        ),
        (4, some_ref("audio/ogg", None), V::M(vec![(0, V::U(0))])),
    ];
    for (id, r, voice) in cases {
        let ev = create_event(
            &au,
            id,
            1_790_000_001_000 + id as u64,
            Some("m"),
            vec![r.to_v()],
            Some(ext_entry(EXT_VOICE, false, voice)),
        );
        alice_sends(&mut p, &ev);
    }
    let bv = view(&p.bob, &p.b, &p.b_conv);
    assert_eq!(bv.len(), 4, "every message arrived");
    for it in &bv {
        let (_, text, atts, pres) = parts(it);
        assert_eq!((text, atts.len()), ("m", 1));
        assert!(
            pres.is_none(),
            "an unusable extension renders as the plain message"
        );
        // untrusted fields were cleaned or dropped, never trusted
        assert_eq!(atts[0].filename.as_deref(), Some("passwd"));
    }
    // the plain-http location was dropped; the https one is kept for fetch planning
    match p
        .bob
        .core
        .attachment_fetch(&p.b, parts(&bv[0]).0, 0)
        .unwrap()
    {
        NetCall::GetBlob { locations, .. } => {
            assert_eq!(
                locations,
                vec!["https://blobs.loopback.test/v1/blobs/aa".to_string()]
            );
        }
        c => panic!("{c:?}"),
    }
}

#[test]
fn an_unknown_critical_extension_or_a_critical_registered_one_rejects_the_event() {
    let mut p = active_pair();
    let au = author(&p);
    let r = some_ref("audio/ogg", Some(10)).to_v();
    let evs = [
        create_event(
            &au,
            1,
            1_790_000_001_000,
            Some("a"),
            vec![r.clone()],
            Some(ext_entry("om.critical.future", true, V::U(1))),
        ),
        create_event(
            &au,
            2,
            1_790_000_002_000,
            Some("b"),
            vec![r.clone()],
            Some(ext_entry(EXT_VOICE, true, V::M(vec![(0, V::U(0))]))),
        ),
    ];
    for e in evs {
        alice_sends(&mut p, &e);
    }
    assert!(view(&p.bob, &p.b, &p.b_conv).is_empty());
    // an unknown NON-critical extension is ignored
    let ok = create_event(
        &au,
        3,
        1_790_000_003_000,
        Some("c"),
        vec![r],
        Some(ext_entry("om.vendor.thing", false, V::U(1))),
    );
    alice_sends(&mut p, &ok);
    assert_eq!(view(&p.bob, &p.b, &p.b_conv).len(), 1);
}

#[test]
fn a_message_with_neither_content_nor_attachments_or_a_repeated_blob_is_refused() {
    let mut p = active_pair();
    let au = author(&p);
    let r = some_ref("image/png", None).to_v();
    alice_sends(
        &mut p,
        &create_event(&au, 1, 1_790_000_001_000, None, vec![], None),
    );
    alice_sends(
        &mut p,
        &create_event(
            &au,
            2,
            1_790_000_002_000,
            Some("x"),
            vec![r.clone(), r],
            None,
        ),
    );
    assert!(view(&p.bob, &p.b, &p.b_conv).is_empty());
}

// ---------------------------------------------------------------------------------------------- location card

fn loc_intent(p: &Pair, lat: i32, lon: i32, note: Option<&str>) -> IntentBody {
    IntentBody::SendLocation {
        conversation_id: p.a_conv.clone(),
        lat_e7: lat,
        lon_e7: lon,
        accuracy_m: Some(25),
        label: Some("Café".into()),
        note: note.map(str::to_string),
    }
}

#[test]
fn a_location_card_carries_exactly_one_canonical_token_that_every_client_shows() {
    let mut p = active_pair();
    let intent = loc_intent(&p, 107_202_000, 1_225_621_000, Some("meet here"));
    p.alice.dispatch(&p.a, "loc", intent);
    settle(&mut p);
    assert_eq!(p.alice.outcome(&p.a, "loc"), Some(OutcomeClass::Committed));
    let bv = view(&p.bob, &p.b, &p.b_conv);
    let (_, text, atts, pres) = parts(&bv[0]);
    assert_eq!(text, "meet here\ngeo:10.7202000,122.5621000");
    assert!(atts.is_empty());
    let card = pres.as_ref().unwrap().location_card.as_ref().unwrap();
    assert_eq!(
        (card.lat_e7, card.lon_e7, card.accuracy_m),
        (107_202_000, 1_225_621_000, Some(25))
    );
    assert_eq!(card.geo, "geo:10.7202000,122.5621000");
    assert_eq!(card.label.as_deref(), Some("Café"));
    // the sender sees the card too
    assert!(parts(&view(&p.alice, &p.a, &p.a_conv)[0]).3.is_some());
}

#[test]
fn a_location_with_a_note_naming_another_point_or_out_of_range_is_refused() {
    let mut p = active_pair();
    for (op, lat, lon, note) in [
        ("a", 1, 1, Some("or geo:5.0000000,5.0000000")),
        ("b", 900_000_001, 0, None),
        ("c", 0, -1_800_000_001, None),
    ] {
        let intent = loc_intent(&p, lat, lon, note);
        p.alice.dispatch(&p.a, op, intent);
        assert_eq!(
            p.alice.outcome(&p.a, op),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Invalid
            }),
            "{op}"
        );
    }
    assert!(view(&p.alice, &p.a, &p.a_conv).is_empty());
}

fn edit_event(au: &Author, target: &[u8], ms: u64, text: &str) -> Vec<u8> {
    Header {
        event_id: [0xE1; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: ms,
            logical: 0,
        },
        kind: events::K_EDIT,
        body: V::M(vec![
            (0, V::B(target.to_vec())),
            (1, events::rich_text(text)),
        ]),
    }
    .encode()
}

#[test]
fn an_edit_that_removes_or_changes_the_token_suppresses_the_card() {
    for (edited, expect_card) in [
        ("never mind", false),
        ("geo:10.7202000,122.5621000 again", true),
        ("now geo:11.0000000,122.5621000", false),
        (
            "geo:10.7202000,122.5621000 and geo:0.0000000,0.0000000",
            false,
        ),
    ] {
        let mut p = active_pair();
        let intent = loc_intent(&p, 107_202_000, 1_225_621_000, None);
        p.alice.dispatch(&p.a, "loc", intent);
        settle(&mut p);
        let mid = parts(&view(&p.bob, &p.b, &p.b_conv)[0]).0.to_string();
        let target = unhex(mid.trim_start_matches("msg-")).unwrap();
        let au = author(&p);
        alice_sends(&mut p, &edit_event(&au, &target, 1_790_000_900_000, edited));
        let bv = view(&p.bob, &p.b, &p.b_conv);
        let (_, text, _, pres) = parts(&bv[0]);
        assert_eq!(text, edited);
        assert_eq!(
            pres.as_ref()
                .and_then(|x| x.location_card.as_ref())
                .is_some(),
            expect_card,
            "{edited}"
        );
    }
}

#[test]
fn a_card_whose_fallback_does_not_match_its_point_is_not_drawn() {
    let mut p = active_pair();
    let au = author(&p);
    let loc = present::Location {
        lat_e7: 107_202_000,
        lon_e7: 1_225_621_000,
        accuracy_m: None,
        label: Some("x".into()),
    };
    let ext = present::extensions_value(&present::Extensions {
        voice: None,
        location: Some(loc),
    })
    .unwrap();
    // no token at all; a different point; the right point plus a second one
    for (id, text) in [
        (1u8, "somewhere"),
        (2, "geo:10.7202001,122.5621000"),
        (3, "geo:10.7202000,122.5621000 geo:1.0000000,1.0000000"),
    ] {
        alice_sends(
            &mut p,
            &create_event(
                &au,
                id,
                1_790_000_001_000 + id as u64,
                Some(text),
                vec![],
                Some(ext.clone()),
            ),
        );
    }
    let bv = view(&p.bob, &p.b, &p.b_conv);
    assert_eq!(bv.len(), 3);
    for it in &bv {
        assert!(parts(it).3.is_none());
    }
    let _ = EXT_LOCATION;
}

// ---------------------------------------------------------------------------------------------- W1 expressive media

use om_core::expressive::{
    self as expressive, AssetRef, BlobPolicy, CachePolicy, Install, Kind, Opened, Presentation,
    Release, SendFrom, Source, Target,
};
use om_core::host::Step;

const PACK: [u8; 32] = [0x72; 32];
const WEBP_HEADER: &[u8] = b"RIFF\x12\0\0\0WEBPVP8L\x06\0\0\0\x2f\0\0\0\0\0";

fn expressive_manifest(kind: Kind) -> (Vec<u8>, Vec<u8>) {
    let blob_id = [12; 32];
    let key = [23; 32];
    let loop_bytes = expressive_animation();
    let (plaintext, duration_ms) = if kind == Kind::LoopAnimation {
        (loop_bytes.as_slice(), Some(68))
    } else {
        (WEBP_HEADER, None)
    };
    let ct = om_core::attach::seal(&key, &blob_id, plaintext);
    let asset = AssetRef {
        attachment: AttachmentRef {
            blob_id,
            key,
            media_type: "image/webp".into(),
            plaintext_size: plaintext.len() as u64,
            ciphertext_digest: om_core::attach::ciphertext_digest(&ct),
            locations: vec![],
            filename: None,
            read_cap: None,
            width: Some(1),
            height: Some(1),
            duration_ms,
        },
        presentation: Presentation::webp(kind, "Accessible label".into(), Some("Fallback".into())),
    };
    let manifest = cbor::encode(&V::M(vec![
        (0, V::U(45)),
        (1, V::U(1)),
        (2, V::B(PACK.to_vec())),
        (3, V::U(1)),
        (5, V::T("Local pack".into())),
        (7, V::A(vec![V::U(4)])),
        (
            8,
            V::A(vec![V::M(vec![
                (0, V::T("item".into())),
                (1, V::U(kind as u64)),
                (2, asset.value()),
                (5, V::T("Item label".into())),
            ])]),
        ),
        (9, V::M(vec![])),
    ]));
    (manifest, ct)
}

fn expressive_prepare(
    p: &mut Pair,
    kind: Kind,
    policy: BlobPolicy,
) -> (PreparedAttachment, Vec<u8>) {
    let (bytes, ct) = expressive_manifest(kind);
    let id = Release::parse(&bytes).unwrap().id();
    let (ok, commit) = p
        .alice
        .core
        .install_content_pack(
            &p.a,
            Install {
                release: &bytes,
                expected_release: id,
                confirmed_release: id,
                control: None,
                policy: CachePolicy::Lazy,
                favourites: &[],
                cached: &[],
            },
        )
        .unwrap();
    ok.unwrap();
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));
    let (prepared, step) = p
        .alice
        .core
        .prepare_expressive(&p.a, PACK, "item", &ct, policy)
        .unwrap();
    assert!(p.alice.settle(&step));
    let prepared = prepared.unwrap();
    p.net.stage_blob(
        &unhex(&prepared.blob_id).unwrap(),
        prepared.ciphertext.clone(),
    );
    (prepared, ct)
}

fn expressive_send(p: &mut Pair, op: &str, target: Target, blob: &PreparedAttachment) {
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendExpressive {
            conversation_id: p.a_conv.clone(),
            target,
            asset: SendFrom::Prepared {
                blob_id: blob.blob_id.clone(),
            },
        },
    );
    assert_eq!(
        p.alice.outcome(&p.a, op),
        Some(OutcomeClass::PendingNetwork)
    );
}

#[test]
fn expressive_sticker_survives_pack_removal_and_restart_with_same_blob() {
    let mut p = active_pair_with_expressive(true, true);
    let (first, pack_ct) = expressive_prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh);
    let (second, _) = expressive_prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh);
    assert_ne!(first.blob_id, second.blob_id);
    assert_ne!(first.ciphertext, second.ciphertext);
    expressive_send(&mut p, "s1", Target::Attachment, &first);
    expressive_send(&mut p, "s2", Target::Attachment, &second);
    let st = app(&p.alice, &p.a);
    let messages = st["relationships"][&p.a_conv]["messages"]
        .as_array()
        .unwrap();
    let keys: Vec<_> = messages
        .iter()
        .filter_map(|m| {
            let r = unhex(m["attachments"][0]["reference"].as_str()?).ok()?;
            Some(
                AttachmentRef::from_v(&cbor::decode(&r).unwrap())
                    .unwrap()
                    .key,
            )
        })
        .collect();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1]);
    let (ok, commit) = p.alice.core.remove_content_pack(&p.a, PACK).unwrap();
    ok.unwrap();
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));
    assert!(p.alice.core.content_pack(&p.a, PACK).unwrap().is_none());
    p.a = p.alice.restart().remove(0);
    p.alice.core.enable_expressive_media();
    assert!(p.alice.core.content_pack(&p.a, PACK).unwrap().is_none());
    assert_ne!(pack_ct, first.ciphertext);
    settle(&mut p);
    assert_eq!(p.net.blob_put_count(), 2);
    assert!(p.bob.core.content_pack(&p.b, PACK).unwrap().is_none());
    let items = view(&p.bob, &p.b, &p.b_conv);
    assert_eq!(items.len(), 2);
    let (id, text, atts, _) = parts(&items[0]);
    assert_eq!(text, "Fallback");
    assert_eq!(atts[0].blob_id, first.blob_id);
    assert_eq!(
        p.bob
            .core
            .expressive_presentation(&p.b, id, Source::Attachment(0))
            .unwrap()
            .alt_text,
        "Accessible label"
    );
    assert!(matches!(
        p.bob.core.open_expressive(&p.b, id, Source::Attachment(0), &first.ciphertext),
        Ok(Opened::Renderable { plaintext, .. }) if plaintext == WEBP_HEADER,
    ));
    let mut observed = om_core::expressive::DecodeFacts {
        width: 1,
        height: 1,
        frame_count: Some(1),
    };
    let accepts = |facts: &om_core::expressive::DecodeFacts| {
        p.bob.core.check_expressive_decode(
            &p.b,
            id,
            Source::Attachment(0),
            &first.ciphertext,
            Some(facts),
        )
    };
    assert!(accepts(&observed));
    observed.frame_count = None; // bitmap-only platforms cannot observe the track count
    assert!(accepts(&observed));
    observed.frame_count = Some(2);
    assert!(!accepts(&observed));
    observed.frame_count = Some(1);
    observed.width = 2;
    assert!(!accepts(&observed));
    let mut tampered = first.ciphertext;
    tampered[0] ^= 1;
    assert_eq!(
        p.bob
            .core
            .open_expressive(&p.b, id, Source::Attachment(0), &tampered),
        Err(AttachError::IntegrityFailure),
    );
}

#[test]
fn expressive_picker_state_is_profile_local_and_commit_gated() {
    use om_core::expressive::PickerChange;
    let mut p = active_pair_with_expressive(true, true);
    expressive_prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh);
    let release = p
        .alice
        .core
        .content_pack(&p.a, PACK)
        .unwrap()
        .unwrap()
        .release_id;
    assert!(p.bob.core.content_packs(&p.b).unwrap().is_empty());

    let (result, commit) = p
        .alice
        .core
        .update_expressive_picker(
            &p.a,
            PACK,
            release,
            "item",
            PickerChange::Favourite { present: true },
        )
        .unwrap();
    result.unwrap();
    assert!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .favourites
            .is_empty()
    );
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));

    let (result, commit) = p
        .alice
        .core
        .update_expressive_picker(&p.a, PACK, release, "item", PickerChange::Recent)
        .unwrap();
    result.unwrap();
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));
    let stale = p
        .alice
        .core
        .update_expressive_picker(&p.a, PACK, [0; 32], "item", PickerChange::Recent)
        .unwrap();
    assert_eq!(stale.0, Err("release_id_mismatch"));
    assert!(stale.1.is_none());

    p.a = p.alice.restart().remove(0);
    assert!(
        p.alice
            .core
            .expressive_picker(&p.a, PACK)
            .unwrap()
            .is_none()
    );
    p.alice.core.enable_expressive_media();
    let picker = p.alice.core.expressive_picker(&p.a, PACK).unwrap().unwrap();
    assert!(picker.pack.favourites.contains("item"));
    assert_eq!(picker.recents, ["item"]);
}

#[test]
fn expressive_w0_device_gets_fallback_and_no_w1_slot() {
    for bob_supports_media in [false, true] {
        let mut p = active_pair_with_expressive(true, bob_supports_media);
        if bob_supports_media {
            // The primary receiver understands W1. Its second, ACS-committed device is W0
            // and has no route yet, so checking only reachable or primary devices would fail.
            let (legacy, step) = p.bob.core.debug_link_messaging_device(&p.b).unwrap();
            assert!(p.bob.settle(&step));
            settle(&mut p);
            let alice = app(&p.alice, &p.a);
            let credentials: Vec<_> = alice["relationships"][&p.a_conv]["peerCredentials"]
                .as_array()
                .unwrap()
                .iter()
                .map(|raw| {
                    om_core::crypto::binding::Credential::from_body(
                        &unhex(raw.as_str().unwrap()).unwrap(),
                    )
                    .unwrap()
                })
                .collect();
            assert!(
                credentials
                    .iter()
                    .any(|c| hex(&c.device_id) == legacy && c.capabilities.is_empty())
            );
            assert!(
                credentials
                    .iter()
                    .any(|c| c.capabilities.iter().any(|s| s == expressive::INLINE))
            );
        }
        let (emoji, _) = expressive_prepare(&mut p, Kind::CustomEmoji, BlobPolicy::Fresh);
        expressive_send(&mut p, "inline", Target::Inline, &emoji);
        settle(&mut p);
        let items = view(&p.bob, &p.b, &p.b_conv);
        let (id, text, atts, _) = parts(&items[0]);
        assert_eq!(text, "Fallback");
        assert!(atts.is_empty());
        assert!(
            p.bob
                .core
                .expressive_presentation(&p.b, id, Source::Inline(0))
                .is_none()
        );
        assert_eq!(p.net.blob_put_count(), 0);
    }
}

#[test]
fn expressive_inline_edit_and_reaction_follow_normal_mutation_registers() {
    let mut p = active_pair_with_expressive(true, true);
    let (emoji, _) = expressive_prepare(&mut p, Kind::CustomEmoji, BlobPolicy::Fresh);
    expressive_send(&mut p, "inline", Target::Inline, &emoji);
    settle(&mut p);
    let items = view(&p.bob, &p.b, &p.b_conv);
    let (id, text, atts, _) = parts(&items[0]);
    let id = id.to_owned();
    assert_eq!(text, "Fallback");
    assert!(atts.is_empty());
    assert!(matches!(
        p.bob
            .core
            .open_expressive(&p.b, &id, Source::Inline(0), &emoji.ciphertext),
        Ok(Opened::Renderable { .. })
    ));

    let (edit, _) = expressive_prepare(&mut p, Kind::CustomEmoji, BlobPolicy::Fresh);
    expressive_send(
        &mut p,
        "edit",
        Target::Edit {
            message_id: id.clone(),
        },
        &edit,
    );
    settle(&mut p);
    assert!(matches!(
        p.bob
            .core
            .open_expressive(&p.b, &id, Source::Inline(0), &edit.ciphertext),
        Ok(Opened::Renderable { .. })
    ));
    assert_eq!(
        p.bob
            .core
            .open_expressive(&p.b, &id, Source::Inline(0), &emoji.ciphertext),
        Err(AttachError::IntegrityFailure)
    );

    let (reaction, _) = expressive_prepare(&mut p, Kind::CustomEmoji, BlobPolicy::Fresh);
    let key = "🙂".to_string();
    expressive_send(
        &mut p,
        "reaction",
        Target::Reaction {
            message_id: id.clone(),
            key: key.clone(),
        },
        &reaction,
    );
    settle(&mut p);
    let source = Source::Reaction {
        key: &key,
        mine: false,
    };
    assert!(matches!(
        p.bob
            .core
            .open_expressive(&p.b, &id, source.clone(), &reaction.ciphertext),
        Ok(Opened::Renderable { .. })
    ));
    p.alice.dispatch(
        &p.a,
        "remove-reaction",
        IntentBody::SetReaction {
            conversation_id: p.a_conv.clone(),
            message_id: id.clone(),
            reaction: key.clone(),
            present: false,
        },
    );
    settle(&mut p);
    assert!(
        p.bob
            .core
            .expressive_presentation(&p.b, &id, source)
            .is_none()
    );
    assert_eq!(view(&p.bob, &p.b, &p.b_conv).len(), 1);
}

#[test]
fn expressive_shared_blob_is_explicit_and_reuses_the_blob() {
    let mut p = active_pair_with_expressive(true, true);
    assert_eq!(BlobPolicy::default(), BlobPolicy::Fresh);
    assert!(BlobPolicy::Fresh.metadata_exposure().is_none());
    let exposure = BlobPolicy::SharedBlob.metadata_exposure().unwrap();
    assert!(
        exposure.contains("BlobStore")
            && exposure.contains("Recipients")
            && exposure.contains("link conversations")
    );
    let (first, _) = expressive_prepare(&mut p, Kind::Sticker, BlobPolicy::SharedBlob);
    expressive_send(&mut p, "first", Target::Attachment, &first);
    settle(&mut p);
    let sent = view(&p.alice, &p.a, &p.a_conv);
    let (id, _, _, _) = parts(&sent[0]);
    p.alice.dispatch(
        &p.a,
        "reuse",
        IntentBody::SendExpressive {
            conversation_id: p.a_conv.clone(),
            target: Target::Attachment,
            asset: SendFrom::Shared {
                conversation_id: p.a_conv.clone(),
                message_id: id.into(),
            },
        },
    );
    settle(&mut p);
    let items = view(&p.bob, &p.b, &p.b_conv);
    assert_eq!(items.len(), 2);
    assert_eq!(p.net.blob_put_count(), 1);
    for item in &items {
        let (id, _, atts, _) = parts(item);
        assert_eq!(atts[0].blob_id, first.blob_id);
        assert!(
            p.bob
                .core
                .expressive_presentation(&p.b, id, Source::Attachment(0))
                .unwrap()
                .shared_blob
        );
    }
}

#[test]
fn expressive_unknown_profile_is_inert_and_unknown_kind_follows_the_wire_ruling() {
    let mut p = active_pair_with_expressive(true, true);
    let au = author(&p);
    let r = some_ref("image/webp", None);
    let ext = ext_entry(
        expressive::EXTENSION,
        false,
        V::M(vec![
            (0, V::U(0)),
            (1, V::U(1)),
            (2, V::T("future.profile".into())),
            (3, V::T("Future alt".into())),
        ]),
    );
    let bytes = create_event(
        &au,
        63,
        1_790_000_000_100,
        Some("fallback"),
        vec![r.to_v()],
        Some(ext),
    );
    alice_sends(&mut p, &bytes);
    let items = view(&p.bob, &p.b, &p.b_conv);
    let (id, _, _, _) = parts(&items[0]);
    assert_eq!(
        p.bob
            .core
            .open_expressive(&p.b, id, Source::Attachment(0), &[]),
        Ok(Opened::Placeholder {
            alt_text: "Future alt".into()
        })
    );
    assert_eq!(
        p.bob.core.expressive_fetch(&p.b, id, Source::Attachment(0)),
        Err(AttachError::Unsupported)
    );

    let mut fields = match r.to_v() {
        V::M(m) => m,
        _ => unreachable!(),
    };
    fields.extend([
        (7, V::U(1)),
        (8, V::U(1)),
        (11, V::U(5)),
        (12, V::T(expressive::WEBP.into())),
        (13, V::T("Alt".into())),
    ]);
    let reaction = Header {
        event_id: [64; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: 1_790_000_000_101,
            logical: 0,
        },
        kind: events::K_REACTION,
        body: V::M(vec![
            (0, V::B(vec![63; 16])),
            (1, V::T("🙂".into())),
            (2, V::Bool(true)),
            (3, V::M(fields)),
        ]),
    }
    .encode_full(None, Some(V::A(vec![V::T(expressive::REACTION.into())])));
    assert_eq!(
        expressive::decode_event(&reaction, true).unwrap_err(),
        "value_range"
    );
    alice_sends(&mut p, &reaction);
    assert!(
        p.bob
            .core
            .expressive_presentation(
                &p.b,
                id,
                Source::Reaction {
                    key: "🙂",
                    mine: false
                }
            )
            .is_none()
    );
}

#[test]
fn expressive_pack_activation_is_atomic_at_the_host_write() {
    use om_core::crypto::sig::{self, DeviceSigner, ROLE_CONTROLLER};

    let mut p = active_pair_with_expressive(true, false);
    let (bytes, ciphertext) = expressive_manifest(Kind::Sticker);
    let first = Release::parse(&bytes).unwrap().id();
    let signer = DeviceSigner {
        ed_seed: [3; 32],
        mldsa_seed: [4; 32],
    };
    let key = signer.public();
    let key_id = &sig::ds_hash(
        "OM-CONTENT-PACK-CONTROLLER-KEY-ID-v1",
        &cbor::encode(&V::A(vec![V::U(1), V::B(key.clone())])),
    )[..16];
    let control = |id, version, previous| {
        let mut fields = vec![
            (0, V::U(46)),
            (1, V::U(1)),
            (2, V::B(PACK.to_vec())),
            (3, V::U(version)),
            (
                5,
                V::A(vec![V::M(vec![
                    (0, V::B(key_id.to_vec())),
                    (1, V::U(1)),
                    (2, V::B(key.clone())),
                ])]),
            ),
            (6, V::U(1)),
            (7, sig::digest(&id)),
            (8, V::U(version)),
        ];
        if let Some(parent) = previous {
            fields.push((4, sig::digest(&parent)));
        }
        let body = cbor::encode(&V::M(fields));
        let msg = sig::ds_sig(
            "OM-CONTENT-PACK-CONTROL-SIG-v1",
            1,
            ROLE_CONTROLLER,
            key_id,
            &body,
        );
        let signed = sig::envelope(
            body.clone(),
            vec![sig::signature_entry(
                key_id,
                1,
                ROLE_CONTROLLER,
                signer.sign_with(&msg, [5; 32]),
            )],
        );
        let hash = sig::ds_hash("OM-CONTENT-PACK-CONTROL-HASH-v1", &body);
        (signed, hash)
    };
    let (genesis, hash) = control(first, 1, None);
    let cached = [expressive::CachedBlob {
        blob_id: [12; 32],
        ciphertext: &ciphertext,
    }];
    fn input<'a>(
        release: &'a [u8],
        id: [u8; 32],
        control: Option<&'a [u8]>,
        cached: &'a [expressive::CachedBlob<'a>],
    ) -> Install<'a> {
        Install {
            release,
            expected_release: id,
            confirmed_release: id,
            control,
            policy: CachePolicy::Full,
            favourites: &[],
            cached,
        }
    }
    let out = p
        .alice
        .core
        .install_content_pack(&p.a, input(&bytes, first, Some(&genesis), &cached))
        .unwrap();
    out.0.unwrap();
    assert!(p.alice.settle(&Step {
        commit: out.1,
        handle: None
    }));
    let pinned = p.alice.core.content_pack(&p.a, PACK).unwrap().unwrap();
    assert_eq!(pinned.signed_by, [<[u8; 16]>::try_from(key_id).unwrap()]);
    assert_eq!(pinned.cached_blobs, 1);

    let V::M(fields) = cbor::decode(&bytes).unwrap() else {
        unreachable!()
    };
    let successor = |seq, previous| {
        let mut fields = fields.clone();
        fields.iter_mut().find(|(k, _)| *k == 3).unwrap().1 = V::U(seq);
        fields.iter_mut().find(|(k, _)| *k == 5).unwrap().1 = V::T("Next release".into());
        fields.push((4, sig::digest(&previous)));
        cbor::encode(&V::M(fields))
    };
    let next = successor(2, first);
    let second = Release::parse(&next).unwrap().id();
    let (signed, _) = control(second, 2, Some(hash));

    let refusal = p
        .alice
        .core
        .install_content_pack(&p.a, input(&next, second, None, &cached))
        .unwrap();
    assert_eq!(refusal.0, Err("updates_frozen"));
    assert!(refusal.1.is_none());

    for (invalid, reason) in [
        (successor(3, first), "sequence_skip"),
        (successor(2, [7; 32]), "previous_release_mismatch"),
    ] {
        let id = Release::parse(&invalid).unwrap().id();
        let bad = p
            .alice
            .core
            .install_content_pack(&p.a, input(&invalid, id, Some(&signed), &cached))
            .unwrap();
        assert_eq!(bad.0, Err(reason));
        assert!(bad.1.is_none());
    }

    // Kill after staging, then refuse the lone atomic pack write, then restart after success.
    let step = |rt: &mut Runtime, ctx: &ProfileExecutionContext| {
        let (result, commit) = rt
            .core
            .install_content_pack(ctx, input(&next, second, Some(&signed), &cached))
            .unwrap();
        result.unwrap();
        let commit = commit.unwrap();
        assert_eq!(commit.writes.len(), 1);
        assert_eq!(commit.writes[0].key, "expressive-packs");
        Step {
            commit: Some(commit),
            handle: None,
        }
    };
    let _uncommitted = step(&mut p.alice, &p.a);
    assert_eq!(
        p.alice.core.content_pack(&p.a, PACK).unwrap().as_ref(),
        Some(&pinned)
    );
    p.a = p.alice.restart().remove(0);
    p.alice.core.enable_expressive_media();
    assert_eq!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .release_id,
        first
    );

    let write = step(&mut p.alice, &p.a);
    p.alice.store.fail_next_commit();
    assert!(!p.alice.settle(&write));
    assert_eq!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .release_id,
        first
    );
    p.a = p.alice.restart().remove(0);
    p.alice.core.enable_expressive_media();
    assert_eq!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .release_id,
        first
    );

    let write = step(&mut p.alice, &p.a);
    assert!(p.alice.settle(&write));
    let retry = p
        .alice
        .core
        .install_content_pack(&p.a, input(&next, second, Some(&signed), &cached))
        .unwrap();
    assert_eq!(retry, (Ok(()), None));
    p.a = p.alice.restart().remove(0);
    assert_eq!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .release_id,
        second
    );
    assert!(p.bob.core.content_pack(&p.b, PACK).unwrap().is_none());
}

#[test]
fn expressive_install_requires_confirmation_and_rejects_blobs_outside_the_pack() {
    let mut p = active_pair_with_expressive(true, false);
    let (bytes, ct) = expressive_manifest(Kind::Sticker);
    let release_id = Release::parse(&bytes).unwrap().id();
    let outside = [expressive::CachedBlob {
        blob_id: [99; 32],
        ciphertext: &ct,
    }];
    let matching = [expressive::CachedBlob {
        blob_id: [12; 32],
        ciphertext: &ct,
    }];
    let favourites = ["item".into()];
    let install = |confirmed, cached, policy| Install {
        release: &bytes,
        expected_release: release_id,
        confirmed_release: confirmed,
        control: None,
        policy,
        favourites: &favourites,
        cached,
    };
    for (confirmed, cache, policy, reason) in [
        (
            [0; 32],
            &matching[..],
            CachePolicy::Full,
            "confirmation_required",
        ),
        (
            release_id,
            &outside[..],
            CachePolicy::Lazy,
            "blob_outside_pack_or_policy",
        ),
        (
            release_id,
            &[][..],
            CachePolicy::FavouritesOnly,
            "cache_incomplete",
        ),
    ] {
        let (result, commit) = p
            .alice
            .core
            .install_content_pack(&p.a, install(confirmed, cache, policy))
            .unwrap();
        assert_eq!(result, Err(reason));
        assert!(commit.is_none());
    }
    let (result, commit) = p
        .alice
        .core
        .install_content_pack(
            &p.a,
            install(release_id, &matching, CachePolicy::FavouritesOnly),
        )
        .unwrap();
    result.unwrap();
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));
    let installed = p.alice.core.content_pack(&p.a, PACK).unwrap().unwrap();
    assert_eq!(installed.cached_blobs, 1);
    assert!(installed.favourites.contains("item"));
    let other = p.alice.open("p-other");
    assert!(p.alice.core.content_pack(&other, PACK).unwrap().is_none());
    let (result, commit) = p
        .alice
        .core
        .evict_content_pack_blob(&p.a, PACK, [12; 32])
        .unwrap();
    result.unwrap();
    assert!(p.alice.settle(&Step {
        commit,
        handle: None
    }));
    assert_eq!(
        p.alice
            .core
            .content_pack(&p.a, PACK)
            .unwrap()
            .unwrap()
            .cached_blobs,
        0
    );

    let (result, commit) = p
        .bob
        .core
        .install_content_pack(
            &p.b,
            install(release_id, &matching, CachePolicy::FavouritesOnly),
        )
        .unwrap();
    assert_eq!(result, Err("expressive_disabled"));
    assert!(commit.is_none());
}

fn expressive_animation() -> Vec<u8> {
    let mut bytes =
        b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x02\0\0\0\0\0\0\0\0\0ANIM\x06\0\0\0\0\0\0\0\0\0".to_vec();
    for _ in 0..2 {
        bytes.extend_from_slice(b"ANMF\x1e\0\0\0");
        let mut frame = [0; 16];
        frame[12] = 34;
        bytes.extend_from_slice(&frame);
        bytes.extend_from_slice(&WEBP_HEADER[12..]);
    }
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn expressive_loop_is_duration_bound_on_the_normal_blob_path() {
    let mut p = active_pair_with_expressive(true, true);
    let (animation, _) = expressive_prepare(&mut p, Kind::LoopAnimation, BlobPolicy::Fresh);
    expressive_send(&mut p, "loop", Target::Attachment, &animation);
    settle(&mut p);
    let items = view(&p.bob, &p.b, &p.b_conv);
    let (id, _, _, _) = parts(&items[0]);
    let result = p
        .bob
        .core
        .open_expressive(&p.b, id, Source::Attachment(0), &animation.ciphertext);
    assert!(
        matches!(result, Ok(Opened::Renderable { facts, .. }) if facts.duration_ms == Some(68))
    );

    let (pack, ciphertext) = expressive_manifest(Kind::LoopAnimation);
    let mut asset = Release::parse(&pack).unwrap().item("item").unwrap().clone();
    asset.attachment.duration_ms = Some(69);
    assert_eq!(
        asset.open(&ciphertext),
        Ok(Opened::Placeholder {
            alt_text: "Accessible label".into()
        })
    );
}
