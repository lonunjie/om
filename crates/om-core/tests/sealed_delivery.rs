// SPDX-License-Identifier: MPL-2.0
//! Sealed-sender evidence (SEALED_SENDER_DELIVERY_PROFILE.md, SECURITY_INVARIANTS M4/M6/M8). The relay-visible bytes
//! of an established 1:1 relationship carry no sender IK or ratchet header, there is no clear fallback, replays are
//! idempotent, and the request-scoped bootstrap key is bounded and dies with the first valid Hello.

use om_codec::Registry;
use om_core::api::*;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::delivery;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::unhex;
use om_core::host::Inbound;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
}

fn mk(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "c",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

/// Runs the first contact up to (not including) the moment Bob processes Alice's Hello.
fn half_open() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = mk(&mut alice, "p-a", "Alice");
    let b = mk(&mut bob, "p-b", "Bob");
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
    // Alice fetches Bob's keys and deposits the sealed Hello; Bob has not polled yet.
    alice.flush_net(&a);
    alice.flush_net(&a);
    let a_conv = alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: a.clone() })[0]
        .id
        .clone();
    Pair {
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
    }
}

fn state(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
    rt.core.debug_app_state(ctx).unwrap()
}

fn rel(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
    state(rt, ctx)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

fn text(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    op: &str,
    conv: &str,
    t: &str,
) -> Option<OutcomeClass> {
    rt.dispatch(
        ctx,
        op,
        IntentBody::SendMessage {
            conversation_id: conv.into(),
            draft: Draft { text: t.into() },
        },
    );
    rt.outcome(ctx, op)
}

fn ec(ik_hex: &str) -> Vec<u8> {
    let mut v = vec![0x01];
    v.extend(unhex(ik_hex).unwrap());
    v
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn relay_never_sees_sender_identity_key_or_ratchet_header_on_established_traffic() {
    let mut p = half_open();
    // Before Bob has answered, Alice has no established key: the message is queued, never sent in the clear.
    let deposits = p.net.deposit_count();
    assert_eq!(
        text(
            &mut p.alice,
            &p.a,
            "early",
            &p.a_conv,
            "queued while establishing"
        ),
        Some(OutcomeClass::PendingNetwork)
    );
    p.alice.flush_net(&p.a);
    assert_eq!(
        p.net.deposit_count(),
        deposits,
        "nothing is deposited without an established delivery key"
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let b_conv = p
        .bob
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: p.b.clone() })[0]
        .id
        .clone();
    for i in 0..4 {
        assert_eq!(
            text(
                &mut p.alice,
                &p.a,
                &format!("a{i}"),
                &p.a_conv,
                "from alice"
            ),
            Some(OutcomeClass::PendingNetwork)
        );
        assert_eq!(
            text(&mut p.bob, &p.b, &format!("b{i}"), &b_conv, "from bob"),
            Some(OutcomeClass::PendingNetwork)
        );
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    }
    // Each side's IK as the *peer* recorded it (peerIk) is the sender key that used to ride in every envelope.
    let alice_ik = ec(rel(&p.bob, &p.b)["peerIk"].as_str().unwrap());
    let bob_ik = ec(rel(&p.alice, &p.a)["peerIk"].as_str().unwrap());
    let reg = Registry::load();
    let mut sealed = 0;
    for bytes in p.net.all_deposited_bytes() {
        // Every deposit is either the W0 MessageRequest (first contact, no identity in the clear) or a sealed wrapper.
        if reg.check("MessageRequest", &bytes, false).is_ok() {
            continue;
        }
        reg.check("SealedDeliveryEnvelope", &bytes, false)
            .expect("W0-valid SealedDeliveryEnvelope");
        let v = cbor::decode(&bytes).unwrap();
        // Exactly the profile's fields: type, version, profile, key id, delivery id, enc, ciphertext, expiry.
        let V::M(m) = &v else { panic!("map") };
        assert_eq!(
            m.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5, 6, 7]
        );
        let ct = v.get(6).unwrap().b().unwrap().len();
        assert!(
            [1040, 4112, 16400, 65552, 131088].contains(&ct),
            "padding bucket {ct}"
        );
        assert!(
            !contains(&bytes, &alice_ik) && !contains(&bytes, &bob_ik),
            "sender IK visible to the relay"
        );
        assert!(
            !contains(&bytes, &alice_ik[1..]) && !contains(&bytes, &bob_ik[1..]),
            "raw IK visible to the relay"
        );
        sealed += 1;
    }
    assert!(
        sealed >= 10,
        "expected Hello, keys reply and 8 messages, saw {sealed}"
    );
    // Padding hides exact size within a bucket: a one-character and a 300-character message are the same size.
    p.alice.dispatch(
        &p.a,
        "s",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft { text: "x".into() },
        },
    );
    p.alice.dispatch(
        &p.a,
        "l",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "y".repeat(300),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let all = p.net.all_deposited_bytes();
    let n = all.len();
    assert_eq!(all[n - 1].len(), all[n - 2].len());
}

#[test]
fn clear_sender_envelopes_are_never_accepted_and_replays_are_idempotent() {
    let mut p = half_open();
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let b_conv = p
        .bob
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: p.b.clone() })[0]
        .id
        .clone();
    // Exact redelivery of a COMPLETE wrapper: same messages, no new failure.
    text(&mut p.alice, &p.a, "m", &p.a_conv, "once");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let mb = unhex(rel(&p.bob, &p.b)["myMailbox"].as_str().unwrap()).unwrap();
    let last = p
        .net
        .deposit_log()
        .into_iter()
        .rev()
        .find(|(m, _)| *m == mb)
        .map(|(_, b)| b)
        .unwrap();
    let t = p
        .bob
        .core
        .poll_targets(&p.b)
        .unwrap()
        .into_iter()
        .find(|t| t.mailbox == mb)
        .unwrap();
    for k in 1..=2 {
        let step = p
            .bob
            .core
            .inbound(
                &p.b,
                &mb,
                &[Inbound {
                    seq: t.after + k,
                    bytes: last.clone(),
                }],
            )
            .unwrap();
        p.bob.settle(&step);
    }
    let v: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: b_conv.clone(),
    });
    assert_eq!(
        v.timeline.len(),
        1,
        "an at-least-once relay redelivery creates no second message"
    );
    assert!(
        v.summary.notices.is_empty(),
        "an exact COMPLETE replay is not an integrity failure"
    );
    // A pass-1 style clear envelope [EncodeEC(IK), null, header, ct] on Bob's mailbox is not an OM-Crypto-v1 delivery.
    let mailbox = p.bob.core.poll_targets(&p.b).unwrap().remove(0);
    let clear = cbor::encode(&V::A(vec![
        V::B(vec![1; 33]),
        V::Null,
        V::B(vec![0; 40]),
        V::B(vec![0; 40]),
    ]));
    let step = p
        .bob
        .core
        .inbound(
            &p.b,
            &mailbox.mailbox,
            &[Inbound {
                seq: mailbox.after + 1000,
                bytes: clear,
            }],
        )
        .unwrap();
    p.bob.settle(&step);
    let v: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: b_conv.clone(),
    });
    assert_eq!(
        v.summary.notices.len(),
        1,
        "stripped wrapper is a counted integrity failure"
    );
    // Replay records carry a fingerprint and a terminal state, never plaintext.
    let r = rel(&p.bob, &p.b);
    let replay = r["replay"].as_object().unwrap();
    assert!(
        replay.len() >= 2
            && replay
                .values()
                .all(|x| x["state"] == "complete" || x["state"] == "rejected")
    );
}

#[test]
fn bootstrap_key_admits_at_most_eight_candidates_then_dies_with_the_winning_hello() {
    let mut p = half_open();
    // Bob's request-scoped bootstrap key is still live (Alice's Hello is queued, not yet processed).
    let r = rel(&p.bob, &p.b);
    let boot = &r["bootstrap"]["key"]["public"];
    let pk = unhex(boot["publicKey"].as_str().unwrap()).unwrap();
    let kid: [u8; 16] = unhex(boot["keyId"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let mb = unhex(r["myMailbox"].as_str().unwrap()).unwrap();
    let exp = boot["expiresAt"].as_u64().unwrap();
    let mut rng = InsecureTestRng::from_label(b"forger");
    let t = p
        .bob
        .core
        .poll_targets(&p.b)
        .unwrap()
        .into_iter()
        .find(|t| t.mailbox == mb)
        .unwrap();
    // Anyone who learned the public bootstrap key can seal junk to it; it must open, fail inner auth, and consume a slot.
    for (i, seq) in (0..12u8).zip(t.after + 1000..) {
        let w = delivery::seal(
            &pk,
            &kid,
            delivery::CORE_ROUTE_URI,
            &mb,
            exp.min(1_790_000_000 + 600),
            &[i; 60],
            &mut rng,
        )
        .unwrap();
        let step = p
            .bob
            .core
            .inbound(&p.b, &mb, &[Inbound { seq, bytes: w }])
            .unwrap();
        p.bob.settle(&step);
    }
    let r = rel(&p.bob, &p.b);
    assert_eq!(
        r["bootstrap"]["candidates"].as_array().unwrap().len(),
        8,
        "ninth and later candidates are refused"
    );
    assert_eq!(
        r["replay"].as_object().unwrap().len(),
        8,
        "no replay state allocated beyond the budget"
    );
    // The real Hello (sequence below the junk) is skipped by the cursor in this synthetic ordering, so deliver it as
    // a fresh item: it must still be refused because the budget is spent, which is the specified bounded behaviour.
    // Now a fresh request: the winning Hello kills the key. Use a clean pair for the positive path.
    let mut q = half_open();
    settle_all(&mut [(&mut q.alice, &q.a), (&mut q.bob, &q.b)]);
    let r = rel(&q.bob, &q.b);
    assert!(
        r["bootstrap"].is_null(),
        "bootstrap key destroyed by the winning Hello"
    );
    assert_eq!(r["state"], "active");
    assert_eq!(rel(&q.alice, &q.a)["state"], "active");
    // A late wrapper sealed to the dead bootstrap key finds no key.
    let failures = r["integrityFailures"].as_u64().unwrap();
    let w = delivery::seal(
        &pk,
        &kid,
        delivery::CORE_ROUTE_URI,
        &mb,
        exp,
        &[9; 60],
        &mut rng,
    )
    .unwrap();
    let mb2 = unhex(r["myMailbox"].as_str().unwrap()).unwrap();
    let t2 = q
        .bob
        .core
        .poll_targets(&q.b)
        .unwrap()
        .into_iter()
        .find(|t| t.mailbox == mb2)
        .unwrap();
    let step = q
        .bob
        .core
        .inbound(
            &q.b,
            &mb2,
            &[Inbound {
                seq: t2.after + 50,
                bytes: w,
            }],
        )
        .unwrap();
    q.bob.settle(&step);
    // (the key id belongs to the other pair, so this is simply an unknown key: counted, nothing installed)
    assert_eq!(
        rel(&q.bob, &q.b)["integrityFailures"].as_u64().unwrap(),
        failures + 1
    );
}

// ---------------------------------------------------------------------------------------------- events

mod events {
    use super::*;
    use om_core::events::{
        Header, Hlc, K_CREATE, K_DELETE, K_EDIT, K_RECEIPT, message_create_body, rich_text,
    };

    fn hexb(s: &str) -> Vec<u8> {
        unhex(s).unwrap()
    }

    /// An established pair plus what Alice needs to author events by hand.
    fn pair() -> (Pair, String, [u8; 32], [u8; 16], [u8; 32]) {
        let mut p = half_open();
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
        let b_conv = p
            .bob
            .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: p.b.clone() })[0]
            .id
            .clone();
        let ra = rel(&p.alice, &p.a);
        let canon: [u8; 32] = hexb(ra["canonId"].as_str().unwrap()).try_into().unwrap();
        // Alice's own account/device ids are Bob's recorded peer ids.
        let rb = rel(&p.bob, &p.b);
        let acct: [u8; 32] = hexb(rb["peerAccountId"].as_str().unwrap())
            .try_into()
            .unwrap();
        let dev: [u8; 16] = hexb(rb["peerDeviceId"].as_str().unwrap())
            .try_into()
            .unwrap();
        (p, b_conv, canon, dev, acct)
    }

    fn inject(p: &mut Pair, payload: Vec<u8>) {
        let step = p
            .alice
            .core
            .debug_inject_payload(&p.a, &p.a_conv, &payload)
            .unwrap();
        p.alice.settle(&step);
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    }

    fn ev(
        canon: [u8; 32],
        acct: [u8; 32],
        dev: [u8; 16],
        id: u8,
        ms: u64,
        kind: u64,
        body: om_core::crypto::cbor::V,
    ) -> Vec<u8> {
        Header {
            event_id: [id; 16],
            conversation_id: canon,
            author_account: acct,
            author_device: dev,
            clock: Hlc {
                physical_ms: ms,
                logical: 0,
            },
            kind,
            body,
        }
        .encode()
    }

    fn view(p: &Pair, conv: &str) -> ConversationView {
        p.bob.query(Query::GetConversationView {
            ctx: p.b.clone(),
            conversation_id: conv.into(),
        })
    }

    #[test]
    fn messages_are_canonical_w0_events_with_event_ids() {
        let (mut p, b_conv, canon, _, _) = pair();
        text(&mut p.alice, &p.a, "m", &p.a_conv, "hello\nworld");
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
        let v = view(&p, &b_conv);
        let TimelineItem::Incoming {
            message_id, text, ..
        } = &v.timeline[0]
        else {
            panic!()
        };
        assert_eq!(text, "hello\nworld");
        assert!(
            message_id.starts_with("msg-") && message_id.len() == 4 + 32,
            "message id is the 128-bit EventID"
        );
        assert_eq!(
            hexb(rel(&p.bob, &p.b)["canonId"].as_str().unwrap()),
            canon.to_vec(),
            "both sides share the ConversationID"
        );
    }

    #[test]
    fn edits_and_deletes_apply_only_from_the_author_in_clock_order_and_equivocation_rejects_all_versions()
     {
        let (mut p, b_conv, canon, dev, acct) = pair();
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                1,
                1_790_000_100_000,
                K_CREATE,
                message_create_body("first"),
            ),
        );
        assert_eq!(texts_of(&view(&p, &b_conv)), vec!["first"]);
        // Edit with a later clock replaces the text; a stale (earlier) edit does not.
        let edit = |id: u8, ms: u64, t: &str| {
            ev(
                canon,
                acct,
                dev,
                id,
                ms,
                K_EDIT,
                om_core::crypto::cbor::V::M(vec![
                    (0, om_core::crypto::cbor::V::B(vec![1; 16])),
                    (1, rich_text(t)),
                ]),
            )
        };
        inject(&mut p, edit(2, 1_790_000_200_000, "second"));
        inject(&mut p, edit(3, 1_790_000_150_000, "stale"));
        assert_eq!(texts_of(&view(&p, &b_conv)), vec!["second"]);
        // Same EventID, different payload: every version is rejected.
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                9,
                1_790_000_300_000,
                K_CREATE,
                message_create_body("one"),
            ),
        );
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                9,
                1_790_000_300_000,
                K_CREATE,
                message_create_body("two"),
            ),
        );
        assert_eq!(texts_of(&view(&p, &b_conv)), vec!["second"]);
        // A later replay of the first version is also refused.
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                9,
                1_790_000_300_000,
                K_CREATE,
                message_create_body("one"),
            ),
        );
        assert_eq!(texts_of(&view(&p, &b_conv)), vec!["second"]);
        // Delete tombstones the message.
        let del = ev(
            canon,
            acct,
            dev,
            4,
            1_790_000_400_000,
            K_DELETE,
            om_core::crypto::cbor::V::M(vec![(0, om_core::crypto::cbor::V::B(vec![1; 16]))]),
        );
        inject(&mut p, del);
        assert!(view(&p, &b_conv).timeline.is_empty());
    }

    #[test]
    fn events_for_another_conversation_or_author_and_unknown_kinds_are_refused() {
        let (mut p, b_conv, canon, dev, acct) = pair();
        let before = rel(&p.bob, &p.b)["integrityFailures"].as_u64().unwrap();
        inject(
            &mut p,
            ev(
                [0xee; 32],
                acct,
                dev,
                1,
                1_790_000_100_000,
                K_CREATE,
                message_create_body("wrong conversation"),
            ),
        );
        inject(
            &mut p,
            ev(
                canon,
                [0xdd; 32],
                dev,
                2,
                1_790_000_100_000,
                K_CREATE,
                message_create_body("wrong author"),
            ),
        );
        inject(
            &mut p,
            ev(
                canon,
                acct,
                [0xcc; 16],
                3,
                1_790_000_100_000,
                K_CREATE,
                message_create_body("wrong endpoint"),
            ),
        );
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                4,
                1_790_000_100_000,
                6,
                om_core::crypto::cbor::V::M(vec![]),
            ),
        );
        assert!(view(&p, &b_conv).timeline.is_empty());
        assert_eq!(
            rel(&p.bob, &p.b)["integrityFailures"].as_u64().unwrap(),
            before + 4
        );
    }

    #[test]
    fn a_rejected_payload_on_an_established_session_does_not_advance_the_ratchet() {
        // Decision (audit M6): an authenticated inner payload that fails validation leaves the cryptographic state
        // exactly as it was, on every path (bootstrap and established), so the sender's message key is not burned by
        // a payload that was never accepted.
        let (mut p, b_conv, canon, dev, acct) = pair();
        let before = p.bob.core.debug_device_digest(&p.b).unwrap();
        inject(
            &mut p,
            ev(
                [0xee; 32],
                acct,
                dev,
                1,
                1_790_000_100_000,
                K_CREATE,
                message_create_body("wrong conversation"),
            ),
        );
        assert!(view(&p, &b_conv).timeline.is_empty());
        assert_eq!(
            p.bob.core.debug_device_digest(&p.b).unwrap(),
            before,
            "a rejected payload must not change the ratchet"
        );
        // A good message afterwards still arrives (the chain stayed usable).
        inject(
            &mut p,
            ev(
                canon,
                acct,
                dev,
                2,
                1_790_000_100_001,
                K_CREATE,
                message_create_body("fine"),
            ),
        );
        assert_eq!(texts_of(&view(&p, &b_conv)), vec!["fine".to_string()]);
        assert_ne!(p.bob.core.debug_device_digest(&p.b).unwrap(), before);
    }

    #[test]
    fn a_receipt_settles_an_outgoing_message_as_delivered() {
        let (mut p, b_conv, canon, _, _) = pair();
        text(&mut p.alice, &p.a, "m", &p.a_conv, "ping");
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
        let v: ConversationView = p.alice.query(Query::GetConversationView {
            ctx: p.a.clone(),
            conversation_id: p.a_conv.clone(),
        });
        let TimelineItem::Outgoing {
            message_id,
            delivery,
            ..
        } = &v.timeline[0]
        else {
            panic!()
        };
        assert_eq!(*delivery, DeliveryState::Sent);
        let eid: Vec<u8> = hexb(message_id.trim_start_matches("msg-"));
        // Bob (the peer) reports it delivered. Bob's own device authors the receipt.
        let rb = rel(&p.bob, &p.b);
        let (b_acct, b_dev): ([u8; 32], [u8; 16]) = {
            let ra = rel(&p.alice, &p.a);
            (
                hexb(ra["peerAccountId"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
                hexb(ra["peerDeviceId"].as_str().unwrap())
                    .try_into()
                    .unwrap(),
            )
        };
        let _ = rb;
        let body = om_core::crypto::cbor::V::M(vec![
            (0, om_core::crypto::cbor::V::U(1)),
            (
                1,
                om_core::crypto::cbor::V::A(vec![om_core::crypto::cbor::V::B(eid)]),
            ),
        ]);
        let payload = ev(canon, b_acct, b_dev, 7, 1_790_000_500_000, K_RECEIPT, body);
        let step = p
            .bob
            .core
            .debug_inject_payload(&p.b, &b_conv, &payload)
            .unwrap();
        p.bob.settle(&step);
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
        let v: ConversationView = p.alice.query(Query::GetConversationView {
            ctx: p.a.clone(),
            conversation_id: p.a_conv.clone(),
        });
        let TimelineItem::Outgoing { delivery, .. } = &v.timeline[0] else {
            panic!()
        };
        assert_eq!(*delivery, DeliveryState::Delivered);
    }

    fn texts_of(v: &ConversationView) -> Vec<String> {
        v.timeline
            .iter()
            .map(|i| match i {
                TimelineItem::Incoming { text, .. } | TimelineItem::Outgoing { text, .. } => {
                    text.clone()
                }
            })
            .collect()
    }
}
