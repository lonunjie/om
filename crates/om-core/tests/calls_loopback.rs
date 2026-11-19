// SPDX-License-Identifier: MPL-2.0
mod common;

use om_core::api::*;
use om_core::calls::{CallReason, CallState, CallView, MediaState};
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Pair {
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
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

fn settle(p: &mut Pair) {
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

fn view(rt: &Runtime, ctx: &ProfileExecutionContext) -> Option<CallView> {
    rt.query(Query::GetCallView { ctx: ctx.clone() })
}

fn start(p: &mut Pair, from_alice: bool, op: &str) -> CallView {
    let (rt, ctx, conv) = if from_alice {
        (&mut p.alice, &p.a, &p.a_conv)
    } else {
        (&mut p.bob, &p.b, &p.b_conv)
    };
    rt.core.enable_calls();
    let chat: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.clone(),
    });
    rt.dispatch(
        ctx,
        op,
        IntentBody::StartCall {
            target: chat.summary.person_id,
            media: CallMedia::Audio,
            sdp: Some(b"prepared offer".to_vec()),
        },
    );
    view(rt, ctx).unwrap()
}

fn end(p: &mut Pair, from_alice: bool, id: &str, op: &str) {
    let (rt, ctx) = if from_alice {
        (&mut p.alice, &p.a)
    } else {
        (&mut p.bob, &p.b)
    };
    rt.dispatch(
        ctx,
        op,
        IntentBody::EndCall {
            call_session_id: id.into(),
        },
    );
}

#[test]
fn authenticated_answer_installs_directional_keys_and_hangup_wipes_them() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    assert_eq!(call.state, CallState::Starting);
    settle(&mut p);
    let incoming = view(&p.bob, &p.b).unwrap();
    assert_eq!(incoming.call_session_id, call.call_session_id);
    assert_eq!(incoming.state, CallState::Incoming);
    assert_eq!(view(&p.alice, &p.a).unwrap().state, CallState::Starting);
    assert!(
        p.bob
            .core
            .take_call_keys(&p.b, &call.call_session_id)
            .unwrap()
            .is_none()
    );
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"prepared answer".to_vec(),
            media_flags: 1,
        },
    );
    settle(&mut p);
    let a = view(&p.alice, &p.a).unwrap();
    let b = view(&p.bob, &p.b).unwrap();
    assert_eq!(a.state, CallState::Connecting);
    assert_eq!(b.state, CallState::Connecting);
    let ka = p
        .alice
        .core
        .take_call_keys(&p.a, &a.call_session_id)
        .unwrap()
        .unwrap();
    let kb = p
        .bob
        .core
        .take_call_keys(&p.b, &b.call_session_id)
        .unwrap()
        .unwrap();
    assert!(ka.send() == kb.receive());
    assert!(ka.receive() == kb.send());
    assert!(ka.send() != ka.receive());
    for record in p
        .alice
        .store
        .raw(&p.a.profile_id)
        .into_iter()
        .chain(p.bob.store.raw(&p.b.profile_id))
    {
        for key in [ka.send(), ka.receive()] {
            let hex = om_core::crypto::hex(key);
            assert!(!record.windows(key.len()).any(|window| window == key));
            assert!(
                !record
                    .windows(hex.len())
                    .any(|window| window == hex.as_bytes())
            );
        }
    }
    assert!(
        p.alice
            .core
            .take_call_keys(&p.a, &a.call_session_id)
            .unwrap()
            .is_none()
    );
    let step = p
        .alice
        .core
        .call_media_state(&p.a, &a.call_session_id, MediaState::Active)
        .unwrap();
    assert!(p.alice.settle(&step));
    assert_eq!(view(&p.alice, &p.a).unwrap().state, CallState::Active);
    end(&mut p, true, &call.call_session_id, "hangup");
    settle(&mut p);
    assert_eq!(view(&p.bob, &p.b).unwrap().reason, Some(CallReason::Hangup));
    assert!(
        p.bob
            .core
            .take_call_keys(&p.b, &call.call_session_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn decline_and_simultaneous_dial() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "first");
    settle(&mut p);
    p.bob.dispatch(
        &p.b,
        "decline",
        IntentBody::DeclineCall {
            call_session_id: call.call_session_id,
        },
    );
    settle(&mut p);
    assert_eq!(
        view(&p.alice, &p.a).unwrap().reason,
        Some(CallReason::Declined)
    );
    let a = start(&mut p, true, "glare-a");
    let b = start(&mut p, false, "glare-b");
    settle(&mut p);
    let av = view(&p.alice, &p.a).unwrap();
    let bv = view(&p.bob, &p.b).unwrap();
    assert_eq!(av.call_session_id, a.call_session_id.min(b.call_session_id));
    assert_eq!(bv.call_session_id, av.call_session_id);
    assert!(matches!(
        (av.state, bv.state),
        (CallState::Starting, CallState::Incoming) | (CallState::Incoming, CallState::Starting)
    ));
}

#[test]
fn restart_drops_unsent_offers_and_does_not_resurrect_a_ringing_call() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let first = start(&mut p, true, "unsent");
    p.a = p.alice.restart().remove(0);
    assert!(view(&p.alice, &p.a).is_none());
    settle(&mut p);
    assert!(view(&p.bob, &p.b).is_none());
    assert!(
        p.alice
            .core
            .take_call_keys(&p.a, &first.call_session_id)
            .unwrap()
            .is_none()
    );
    let _ = start(&mut p, true, "second");
    settle(&mut p);
    assert_eq!(view(&p.bob, &p.b).unwrap().state, CallState::Incoming);
    p.b = p.bob.restart().remove(0);
    p.bob.core.enable_calls();
    settle(&mut p);
    assert!(view(&p.bob, &p.b).is_none());
}

#[test]
fn disabled_contact_is_silent_and_the_caller_expires() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    p.bob.dispatch(
        &p.b,
        "off",
        IntentBody::SetContactAllowCalls {
            conversation_id: p.b_conv.clone(),
            allowed: false,
        },
    );
    p.b = p.bob.restart().remove(0);
    p.bob.core.enable_calls();
    let _ = start(&mut p, true, "start");
    settle(&mut p);
    assert!(view(&p.bob, &p.b).is_none());
    p.alice.tick(44_999);
    let step = p.alice.core.call_tick(&p.a).unwrap();
    assert!(p.alice.settle(&step));
    assert_eq!(view(&p.alice, &p.a).unwrap().state, CallState::Starting);
    p.alice.tick(1);
    let step = p.alice.core.call_tick(&p.a).unwrap();
    assert!(p.alice.settle(&step));
    assert_eq!(
        view(&p.alice, &p.a).unwrap().reason,
        Some(CallReason::Timeout)
    );
}

fn deposits(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<om_core::host::NetOp> {
    rt.core
        .net_ops(ctx)
        .unwrap()
        .into_iter()
        .filter(|op| matches!(op.call, om_core::host::NetCall::Deposit { .. }))
        .collect()
}

#[test]
fn replay_after_restart_never_rings_again() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let first = start(&mut p, true, "start");
    let before = deposits(&p.alice, &p.a);
    assert_eq!(before.len(), 1);
    let targets = p.bob.core.poll_targets(&p.b).unwrap();
    let relay = targets
        .iter()
        .find(|t| t.kind == om_core::host::MailboxKind::Relay)
        .unwrap();
    settle(&mut p);
    assert_eq!(
        view(&p.bob, &p.b).unwrap().call_session_id,
        first.call_session_id
    );
    p.b = p.bob.restart().remove(0);
    p.bob.core.enable_calls();
    let om_core::host::NetCall::Deposit { bytes, .. } = &before[0].call else {
        unreachable!()
    };
    let result = p
        .bob
        .core
        .inbound(
            &p.b,
            &relay.mailbox,
            &[om_core::host::Inbound {
                seq: relay.after + 100,
                bytes: bytes.clone(),
            }],
        )
        .unwrap();
    assert!(p.bob.settle(&result));
    assert!(view(&p.bob, &p.b).is_none());
}

#[test]
fn delayed_deposit_has_an_authenticated_deadline_and_is_missed_without_ringing() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    let ops = deposits(&p.alice, &p.a);
    let om_core::host::NetCall::Deposit { bytes, .. } = &ops[0].call else {
        unreachable!()
    };
    let expiry = om_core::crypto::cbor::decode(bytes)
        .unwrap()
        .get(7)
        .unwrap()
        .u()
        .unwrap();
    assert_eq!(expiry, p.alice.clock.get() / 1000 + 45);
    assert_eq!(p.alice.flush_net(&p.a), 1);
    p.bob.tick(45_000);
    assert!(p.bob.poll(&p.b) > 0);
    assert!(view(&p.bob, &p.b).is_none());
    let notices: Vec<om_core::calls::CallNotice> =
        p.bob.query(Query::GetCallNotices { ctx: p.b.clone() });
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].call_session_id, call.call_session_id);
    assert_eq!(notices[0].reason, CallReason::Timeout);

    // Reopening the encrypted local vault retains the notice but does not restore a live call.
    p.b = p.bob.restart().remove(0);
    p.bob.core.enable_calls();
    assert!(view(&p.bob, &p.b).is_none());
    let reopened: Vec<om_core::calls::CallNotice> =
        p.bob.query(Query::GetCallNotices { ctx: p.b.clone() });
    assert_eq!(reopened, notices);
}

#[test]
fn busy_and_offer_spam_do_not_replace_the_existing_call() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let first = start(&mut p, true, "start");
    settle(&mut p);
    // The old caller is gone. A new offer must not take over Bob's incoming call.
    p.a = p.alice.restart().remove(0);
    let second = start(&mut p, true, "start-again");
    settle(&mut p);
    assert_ne!(first.call_session_id, second.call_session_id);
    assert_eq!(view(&p.alice, &p.a).unwrap().reason, Some(CallReason::Busy));
    assert_eq!(
        view(&p.bob, &p.b).unwrap().call_session_id,
        first.call_session_id
    );
    p.bob.dispatch(
        &p.b,
        "decline-first",
        IntentBody::DeclineCall {
            call_session_id: first.call_session_id,
        },
    );
    settle(&mut p);
    for n in 0..2 {
        let next = start(&mut p, true, &format!("start-{n}"));
        settle(&mut p);
        p.bob.dispatch(
            &p.b,
            &format!("decline-{n}"),
            IntentBody::DeclineCall {
                call_session_id: next.call_session_id,
            },
        );
        settle(&mut p);
    }
    let prior = view(&p.bob, &p.b);
    for n in 0..2 {
        let next = start(&mut p, true, &format!("fold-{n}"));
        settle(&mut p);
        assert_eq!(view(&p.bob, &p.b), prior);
        end(&mut p, true, &next.call_session_id, &format!("cancel-{n}"));
        settle(&mut p);
    }
    let notices: Vec<om_core::calls::CallNotice> =
        p.bob.query(Query::GetCallNotices { ctx: p.b.clone() });
    assert_eq!(notices.iter().filter(|n| n.folded).count(), 1);
}

#[test]
fn one_contact_leaves_replay_capacity_for_another_after_restart() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    for n in 0..70 {
        let attempt = start(&mut p, true, &format!("budget-{n}"));
        settle(&mut p);
        end(&mut p, true, &attempt.call_session_id, &format!("end-{n}"));
        settle(&mut p);
    }
    p.b = p.bob.restart().remove(0);
    p.bob.core.enable_calls();
    let saved = common::app_state(&p.bob, &p.b);
    let seen = saved["callOffersSeen"].as_array().unwrap();
    assert!(seen.len() <= 64);

    let mut carol = Runtime::new("carol", &p.bob.net);
    let c = common::create_profile(&mut carol, "p-c", "Carol");
    common::invite(&mut p.bob, &p.b, "carol-invite");
    let invite: Option<InviteView> = p.bob.query(Query::GetInvite {
        ctx: p.b.clone(),
        operation_id: "carol-invite".into(),
    });
    carol.dispatch(
        &c,
        "accept",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
    settle_all(&mut [(&mut carol, &c), (&mut p.bob, &p.b)]);
    let intro = common::intros(&mut p.bob, &p.b).pop().unwrap();
    p.bob.dispatch(
        &p.b,
        "carol-intro",
        IntentBody::AcceptIntroduction {
            request_id: intro.id,
        },
    );
    settle_all(&mut [(&mut carol, &c), (&mut p.bob, &p.b)]);
    let contacts: Vec<ConversationSummary> =
        carol.query(Query::GetConversationList { ctx: c.clone() });
    carol.core.enable_calls();
    carol.dispatch(
        &c,
        "ring",
        IntentBody::StartCall {
            target: contacts[0].person_id.clone(),
            media: CallMedia::Audio,
            sdp: Some(b"prepared offer".to_vec()),
        },
    );
    settle_all(&mut [(&mut carol, &c), (&mut p.bob, &p.b)]);
    assert_eq!(view(&p.bob, &p.b).unwrap().state, CallState::Incoming);
}

#[test]
fn unsupported_receiver_and_ordinary_context_never_turn_a_signal_into_chat() {
    let mut p = active_pair();
    let call = start(&mut p, true, "start");
    settle(&mut p); // Bob leaves the capability disabled.
    assert!(view(&p.bob, &p.b).is_none());
    p.bob.core.enable_calls();
    let id = om_core::crypto::unhex(&call.call_session_id).unwrap();
    use om_core::crypto::cbor::{self, V};
    let wrong_context_offer = cbor::encode(&V::M(vec![
        (0, V::U(44)),
        (1, V::U(1)),
        (2, V::U(1)),
        (3, V::B(id)),
        (4, V::U(0)),
        (5, V::U(1)),
        (6, V::B(b"offer in the wrong context".to_vec())),
        (7, V::M(vec![(0, V::B(vec![20; 32])), (1, V::U(1))])),
    ]));
    let step = p
        .alice
        .core
        .debug_inject_payload(&p.a, &p.a_conv, &wrong_context_offer)
        .unwrap();
    assert!(p.alice.settle(&step));
    p.alice.dispatch(
        &p.a,
        "ordinary",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "still a chat".into(),
                reply_to: None,
            },
        },
    );
    settle(&mut p);
    assert!(view(&p.bob, &p.b).is_none());
    let chat: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv,
    });
    assert!(
        matches!(&chat.timeline[..], [TimelineItem::Incoming { text, .. }] if text == "still a chat")
    );
}

#[test]
fn a_refused_commit_releases_neither_offer_nor_call_view() {
    let mut p = active_pair();
    p.alice.core.enable_calls();
    let chat: ConversationView = p.alice.query(Query::GetConversationView {
        ctx: p.a.clone(),
        conversation_id: p.a_conv.clone(),
    });
    let intent = Intent {
        ctx: p.a.clone(),
        operation_id: "commit-refused".into(),
        body: IntentBody::StartCall {
            target: chat.summary.person_id,
            media: CallMedia::Audio,
            sdp: Some(b"never released".to_vec()),
        },
    };
    let step = p.alice.core.dispatch(&intent).unwrap();
    assert!(step.commit.is_some());
    assert!(view(&p.alice, &p.a).is_none());
    assert!(deposits(&p.alice, &p.a).is_empty());
    p.alice.store.fail_next_commit();
    assert!(!p.alice.settle(&step));
    assert!(view(&p.alice, &p.a).is_none());
    assert!(deposits(&p.alice, &p.a).is_empty());
    let step = p.alice.core.dispatch(&intent).unwrap();
    assert!(p.alice.settle(&step));
    assert_eq!(deposits(&p.alice, &p.a).len(), 1);
}

#[test]
fn outgoing_admission_refuses_a_person_without_an_established_relationship() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    alice.core.enable_calls();
    alice.dispatch(
        &a,
        "unknown",
        IntentBody::StartCall {
            target: "unknown".into(),
            media: CallMedia::Audio,
            sdp: Some(b"offer".to_vec()),
        },
    );
    assert_eq!(
        alice.outcome(&a, "unknown"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unauthorized
        })
    );
    assert!(deposits(&alice, &a).is_empty());
    assert!(view(&alice, &a).is_none());
}

#[test]
fn blocked_peer_does_not_get_a_reply_or_a_ring() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let chat: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv.clone(),
    });
    let confirmation =
        common::confirmation(&mut p.bob, &p.b, "BlockPerson", &chat.summary.person_id);
    p.bob.dispatch(
        &p.b,
        "block",
        IntentBody::BlockPerson {
            person_id: chat.summary.person_id,
            scope: BlockScope::Local,
            confirmation,
        },
    );
    assert_eq!(p.bob.outcome(&p.b, "block"), Some(OutcomeClass::Committed));
    let _call = start(&mut p, true, "start");
    p.alice.flush_net(&p.a);
    assert!(p.bob.poll(&p.b) > 0);
    assert!(view(&p.bob, &p.b).is_none());
    assert!(deposits(&p.bob, &p.b).is_empty());
}

#[test]
fn legacy_ratchet_reader_skips_call_ciphertext_and_still_opens_chat() {
    use om_core::crypto::{
        Error, binding::RelationshipContext, cbor, delivery,
        rng::insecure_test_rng::InsecureTestRng, unhex,
    };
    use om_core::host::NetCall;
    use om_core::state::ProfileState;

    let mut p = active_pair();
    let mut legacy =
        ProfileState::decode("legacy".into(), &p.bob.store.records(&p.b.profile_id)).unwrap();
    let rel = &legacy.app.relationships[&p.b_conv];
    let peer = RelationshipContext::new(unhex(&rel.peer_account_id).unwrap().try_into().unwrap());
    let open = |op: &om_core::host::NetOp| {
        let NetCall::Deposit {
            mailbox,
            relay_uri,
            bytes,
        } = &op.call
        else {
            unreachable!()
        };
        let wrapper = cbor::decode(bytes).unwrap();
        let key_id: [u8; 16] = wrapper.get(3).unwrap().b().unwrap().try_into().unwrap();
        let key = rel
            .my_delivery
            .iter()
            .find(|k| unhex(&k.public.key_id).unwrap() == key_id)
            .unwrap();
        delivery::open(
            bytes,
            &unhex(&key.seed).unwrap().try_into().unwrap(),
            &key_id,
            relay_uri.as_deref().unwrap(),
            mailbox,
            1_790_000_000,
        )
        .unwrap()
        .inner_envelope
    };
    start(&mut p, true, "dial");
    let ciphertext = open(&deposits(&p.alice, &p.a)[0]);
    let legacy = legacy.device.as_mut().unwrap();
    let mut rng = InsecureTestRng::from_label(b"old client");
    let before = legacy.to_bytes();
    assert!(matches!(
        legacy.decrypt(&peer, &ciphertext, &mut rng),
        Err(Error::DecryptFailed)
    ));
    assert!(legacy.to_bytes() == before);
    p.alice.flush_net(&p.a);
    p.alice.dispatch(
        &p.a,
        "chat",
        IntentBody::SendMessage {
            conversation_id: p.a_conv,
            draft: Draft {
                text: "ordinary chat still decrypts".into(),
                reply_to: None,
            },
        },
    );
    let ciphertext = open(&deposits(&p.alice, &p.a)[0]);
    let (_, pt, _) = legacy.decrypt(&peer, &ciphertext, &mut rng).unwrap();
    let event = om_core::events::parse(&pt).unwrap();
    assert_eq!(
        om_core::events::plain_text(event.body.get(0).unwrap()).as_deref(),
        Some("ordinary chat still decrypts")
    );
}

#[test]
fn peer_call_sequence_rejects_older_messages_and_reserved_kinds() {
    use om_core::crypto::{
        cbor::{self, V},
        unhex,
    };
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    settle(&mut p);
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"answer".to_vec(),
            media_flags: 1,
        },
    );
    settle(&mut p);
    let id = unhex(&call.call_session_id).unwrap();
    let inject = |rt: &mut Runtime, kind: u64, seq: u64, field| {
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(44)),
            (1, V::U(1)),
            (2, V::U(kind)),
            (3, V::B(id.clone())),
            (4, V::U(seq)),
            field,
        ]));
        let step = rt
            .core
            .debug_call_payload(&p.a, &p.a_conv, &payload)
            .unwrap();
        assert!(rt.settle(&step));
    };
    inject(&mut p.alice, 6, 5, (9, V::U(1)));
    inject(&mut p.alice, 4, 4, (8, V::U(6)));
    inject(&mut p.alice, 4, 5, (8, V::U(6)));
    inject(&mut p.alice, 8, 9, (8, V::U(6)));
    inject(&mut p.alice, 4, 9, (9, V::U(0)));
    p.alice.flush_net(&p.a);
    p.bob.poll(&p.b);
    let current = view(&p.bob, &p.b).unwrap();
    assert_eq!(current.state, CallState::Connecting);
    assert_eq!(current.peer_media_flags, 1);
    inject(&mut p.alice, 4, 6, (8, V::U(6)));
    settle(&mut p);
    assert_eq!(view(&p.bob, &p.b).unwrap().reason, Some(CallReason::Hangup));
}

fn connect(p: &mut Pair) -> String {
    p.bob.core.enable_calls();
    let call = start(p, true, "start");
    settle(p);
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"answer".to_vec(),
            media_flags: 1,
        },
    );
    settle(p);
    for (rt, ctx) in [(&mut p.alice, &p.a), (&mut p.bob, &p.b)] {
        rt.core
            .take_call_keys(ctx, &call.call_session_id)
            .unwrap()
            .unwrap();
        let step = rt
            .core
            .call_media_state(ctx, &call.call_session_id, MediaState::Active)
            .unwrap();
        assert!(rt.settle(&step));
        assert_eq!(view(rt, ctx).unwrap().state, CallState::Active);
    }
    call.call_session_id
}

/// Perform the one listed deposit for real and report its result to Core.
fn perform_one(rt: &mut Runtime, ctx: &ProfileExecutionContext) {
    let ops = deposits(rt, ctx);
    assert_eq!(ops.len(), 1, "exactly one signal in flight on the leg");
    let result = rt.net.perform(&ops[0].call);
    let step = rt.core.net_result(ctx, ops[0].id, result).unwrap();
    assert!(rt.settle(&step));
}

#[test]
fn adr_0228_one_signal_in_flight_per_leg_and_media_updates_coalesce() {
    let mut p = active_pair();
    let id = connect(&mut p);
    p.alice.dispatch(
        &p.a,
        "ice",
        IntentBody::UpdateCallIce {
            call_session_id: id.clone(),
            sdp: b"restart".to_vec(),
        },
    );
    for (n, flags) in [(0, 0u8), (1, 1), (2, 3)] {
        p.alice.dispatch(
            &p.a,
            &format!("media{n}"),
            IntentBody::SetCallMedia {
                call_session_id: id.clone(),
                media_flags: flags,
            },
        );
    }
    // Only the ice_update is released; the media updates wait behind it.
    assert_eq!(deposits(&p.alice, &p.a).len(), 1);
    perform_one(&mut p.alice, &p.a);
    // The three media updates coalesced into one, carrying the latest value.
    perform_one(&mut p.alice, &p.a);
    assert!(deposits(&p.alice, &p.a).is_empty());
    p.bob.poll(&p.b);
    let bob = view(&p.bob, &p.b).unwrap();
    assert_eq!(bob.peer_media_flags, 3);
    assert_eq!(bob.state, CallState::Active);
    assert_eq!(
        p.bob
            .core
            .call_description(&p.b, &id, true)
            .unwrap()
            .unwrap()
            .sdp,
        b"restart"
    );
}

#[test]
fn adr_0228_an_unknown_result_keeps_the_leg_busy_and_retries_the_same_bytes() {
    let mut p = active_pair();
    let id = connect(&mut p);
    p.alice.dispatch(
        &p.a,
        "ice",
        IntentBody::UpdateCallIce {
            call_session_id: id.clone(),
            sdp: b"restart".to_vec(),
        },
    );
    p.alice.dispatch(
        &p.a,
        "media",
        IntentBody::SetCallMedia {
            call_session_id: id.clone(),
            media_flags: 3,
        },
    );
    let first = deposits(&p.alice, &p.a).remove(0);
    let step = p
        .alice
        .core
        .net_result(
            &p.a,
            first.id,
            om_core::host::NetResult::Err {
                error: om_core::host::NetError::Timeout,
            },
        )
        .unwrap();
    assert!(p.alice.settle(&step));
    // Backing off: nothing is listed, and the media update is not released past the unknown signal.
    assert!(deposits(&p.alice, &p.a).is_empty());
    p.alice.tick(2_000);
    let again = deposits(&p.alice, &p.a);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, first.id);
    assert_eq!(
        again[0].call, first.call,
        "identical bytes and operation id"
    );
    perform_one(&mut p.alice, &p.a);
    perform_one(&mut p.alice, &p.a);
}

#[test]
fn adr_0228_a_lost_answer_response_after_durable_deposit_still_connects() {
    use om_core::runtime::Fault;
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    settle(&mut p);
    p.bob.net.inject(Fault::TimeoutAfterEffect);
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"answer".to_vec(),
            media_flags: 1,
        },
    );
    p.bob.flush_net(&p.b);
    let b = view(&p.bob, &p.b).unwrap();
    assert_eq!(b.state, CallState::Connecting);
    assert!(
        b.delivery_unsure,
        "visible as still trying, not as a refusal"
    );
    assert_eq!(b.reason, None);
    settle(&mut p);
    assert_eq!(view(&p.alice, &p.a).unwrap().state, CallState::Connecting);
    p.bob.tick(2_000);
    settle(&mut p);
    let b = view(&p.bob, &p.b).unwrap();
    assert!(!b.delivery_unsure);
    assert_eq!(b.state, CallState::Connecting);
    assert_eq!(view(&p.alice, &p.a).unwrap().state, CallState::Connecting);
}

#[test]
fn adr_0228_a_lost_answer_before_deposit_ends_both_sides_without_sticking() {
    use om_core::runtime::Fault;
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    settle(&mut p);
    // Every attempt of the answer is lost before it reaches the Home.
    for _ in 0..3 {
        p.bob.net.inject(Fault::TimeoutBeforeEffect);
    }
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"answer".to_vec(),
            media_flags: 1,
        },
    );
    for _ in 0..3 {
        p.bob.flush_net(&p.b);
        p.bob.tick(8_000);
    }
    let step = p.bob.core.call_tick(&p.b).unwrap();
    assert!(p.bob.settle(&step));
    // Callee: bounded by the 30 s connecting watchdog (24 s elapsed above).
    assert_eq!(view(&p.bob, &p.b).unwrap().state, CallState::Connecting);
    p.bob.tick(6_000);
    let step = p.bob.core.call_tick(&p.b).unwrap();
    assert!(p.bob.settle(&step));
    assert_eq!(view(&p.bob, &p.b).unwrap().reason, Some(CallReason::Failed));
    // Caller: still ringing, then rings out.
    p.alice.tick(45_000);
    let step = p.alice.core.call_tick(&p.a).unwrap();
    assert!(p.alice.settle(&step));
    assert_eq!(
        view(&p.alice, &p.a).unwrap().reason,
        Some(CallReason::Timeout)
    );
}

#[test]
fn adr_0228_an_adapter_that_never_reports_media_is_ended_at_thirty_seconds() {
    let mut p = active_pair();
    p.bob.core.enable_calls();
    let call = start(&mut p, true, "start");
    settle(&mut p);
    p.bob.dispatch(
        &p.b,
        "answer",
        IntentBody::AcceptCall {
            call_session_id: call.call_session_id.clone(),
            sdp: b"answer".to_vec(),
            media_flags: 1,
        },
    );
    settle(&mut p);
    for (rt, ctx) in [(&mut p.alice, &p.a), (&mut p.bob, &p.b)] {
        rt.core
            .take_call_keys(ctx, &call.call_session_id)
            .unwrap()
            .unwrap();
        rt.tick(29_999);
        let step = rt.core.call_tick(ctx).unwrap();
        assert!(rt.settle(&step));
        let v = view(rt, ctx).unwrap();
        assert_eq!(v.state, CallState::Connecting);
        assert!(v.wake_at_ms.is_some());
        rt.tick(1);
        let step = rt.core.call_tick(ctx).unwrap();
        assert!(rt.settle(&step));
        let v = view(rt, ctx).unwrap();
        assert_eq!(
            (v.state, v.reason),
            (CallState::Failed, Some(CallReason::Failed))
        );
        assert!(
            rt.core
                .take_call_keys(ctx, &call.call_session_id)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn adr_0228_core_bounds_an_ice_restart_exchange_and_ends_after_two_more() {
    let mut p = active_pair();
    let id = connect(&mut p);
    let interrupt = |p: &mut Pair| {
        let step = p
            .alice
            .core
            .call_media_state(&p.a, &id, MediaState::Interrupted)
            .unwrap();
        assert!(p.alice.settle(&step));
    };
    let expire = |p: &mut Pair| {
        p.alice.tick(30_000);
        let step = p.alice.core.call_tick(&p.a).unwrap();
        assert!(p.alice.settle(&step));
        view(&p.alice, &p.a).unwrap()
    };
    interrupt(&mut p);
    let v = view(&p.alice, &p.a).unwrap();
    assert_eq!((v.state, v.restart_attempt), (CallState::Reconnecting, 0));
    assert!(v.wake_at_ms.is_some());
    // A restart that completes closes the exchange.
    let step = p
        .alice
        .core
        .call_media_state(&p.a, &id, MediaState::Active)
        .unwrap();
    assert!(p.alice.settle(&step));
    let v = expire(&mut p);
    assert_eq!((v.state, v.restart_attempt), (CallState::Active, 0));
    interrupt(&mut p);
    let v = expire(&mut p);
    assert_eq!((v.state, v.restart_attempt), (CallState::Reconnecting, 1));
    let v = expire(&mut p);
    assert_eq!((v.state, v.restart_attempt), (CallState::Reconnecting, 2));
    let v = expire(&mut p);
    assert_eq!(
        (v.state, v.reason),
        (CallState::Failed, Some(CallReason::Failed))
    );
}
