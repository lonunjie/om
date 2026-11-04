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
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
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
            draft: Draft { text: text.into() },
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
            draft: Draft { text: "hi".into() },
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
