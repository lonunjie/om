// SPDX-License-Identifier: MPL-2.0
//! End-to-end Core scenarios over the loopback provider: onboarding, invite and introduction, 1:1 sessions with
//! ratcheting across restarts, commit-before-release, unknown network outcomes, profile namespacing and the
//! conservative failure paths. The sans-IO Core is driven exactly as a PlatformAdapter host drives it.

mod common;

use om_core::api::*;
use om_core::host::CoreError;
use om_core::runtime::{Fault, LoopbackNetwork, Runtime, settle_all};

fn create_profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "create-1",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    assert_eq!(rt.outcome(&ctx, "create-1"), Some(OutcomeClass::Committed));
    rt.flush_net(&ctx); // publish key material
    ctx
}

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

/// Alice invites, Bob accepts the link, Alice accepts the introduction; both end with an active conversation.
fn connected() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    alice.dispatch(&a, "inv-1", IntentBody::CreateInvite);
    alice.flush_net(&a);
    assert_eq!(alice.outcome(&a, "inv-1"), Some(OutcomeClass::Committed));
    let invite: Option<InviteView> = alice.query(Query::GetInvite {
        ctx: a.clone(),
        operation_id: "inv-1".into(),
    });
    let link = invite.expect("invite view").link;
    bob.dispatch(&b, "acc-1", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(bob.outcome(&b, "acc-1"), Some(OutcomeClass::Committed));
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    assert_eq!(intros.len(), 1);
    assert_eq!(intros[0].display_name, "Bob");
    alice.dispatch(
        &a,
        "intro-1",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(alice.outcome(&a, "intro-1"), Some(OutcomeClass::Committed));
    let al: Vec<ConversationSummary> = alice.query(Query::GetConversationList { ctx: a.clone() });
    let bl: Vec<ConversationSummary> = bob.query(Query::GetConversationList { ctx: b.clone() });
    assert_eq!((al.len(), bl.len()), (1, 1));
    assert_eq!(
        (al[0].display_name.as_str(), bl[0].display_name.as_str()),
        ("Bob", "Alice")
    );
    Pair {
        net,
        a_conv: al[0].id.clone(),
        b_conv: bl[0].id.clone(),
        alice,
        bob,
        a,
        b,
    }
}

fn timeline(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<TimelineItem> {
    let v: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    });
    v.timeline
}

fn texts(t: &[TimelineItem]) -> Vec<String> {
    t.iter()
        .map(|i| match i {
            TimelineItem::Outgoing { text, .. } => format!("out:{text}"),
            TimelineItem::Incoming { text, .. } => format!("in:{text}"),
        })
        .collect()
}

#[test]
fn onboarding_creates_identity_and_a_display_once_recovery_key() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("solo", &net);
    let ctx = rt.open("me");
    rt.dispatch(
        &ctx,
        "c1",
        IntentBody::CreateIdentityProfile {
            label: "  Me  ".into(),
        },
    );
    assert_eq!(rt.outcome(&ctx, "c1"), Some(OutcomeClass::Committed));
    let key: Option<RecoveryKeyView> = rt.query(Query::GetRecoveryKey {
        ctx: ctx.clone(),
        key_operation_id: "c1".into(),
    });
    let key = key.expect("key is shown once");
    assert_eq!(key.groups.len(), 14);
    assert!(key.groups.iter().all(|g| g.len() == 4));
    let profiles: Vec<ProfileSummary> = rt.query(Query::GetProfileList);
    assert_eq!(
        profiles,
        vec![ProfileSummary {
            id: "me".into(),
            label: "Me".into()
        }]
    );
    let rs: RecoveryStatusView = rt.query(Query::GetRecoveryStatus { ctx: ctx.clone() });
    assert!(!rs.recovery_key_saved);
    rt.dispatch(
        &ctx,
        "c2",
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "c1".into(),
        },
    );
    assert_eq!(rt.outcome(&ctx, "c2"), Some(OutcomeClass::Committed));
    let gone: Option<RecoveryKeyView> = rt.query(Query::GetRecoveryKey {
        ctx: ctx.clone(),
        key_operation_id: "c1".into(),
    });
    assert!(gone.is_none(), "Core never shows the key again");
    let rs: RecoveryStatusView = rt.query(Query::GetRecoveryStatus { ctx: ctx.clone() });
    assert!(rs.recovery_key_saved);
    assert_eq!(
        rs.policy,
        RecoveryPolicyState::Pending,
        "pass 1 must not claim recovery is on"
    );
    // A second identity in the same profile, or an invalid label, is refused.
    rt.dispatch(
        &ctx,
        "c3",
        IntentBody::CreateIdentityProfile {
            label: "Again".into(),
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "c3"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Conflict
        })
    );
    let ctx2 = rt.open("other");
    rt.dispatch(
        &ctx2,
        "c4",
        IntentBody::CreateIdentityProfile {
            label: "   ".into(),
        },
    );
    assert_eq!(
        rt.outcome(&ctx2, "c4"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    let devices: DevicesView = rt.query(Query::GetDevices { ctx });
    assert_eq!(devices.devices.len(), 1);
    assert!(devices.devices[0].is_current && devices.devices[0].actions.is_empty());
}

#[test]
fn invite_introduction_and_two_way_messaging_with_ratcheting() {
    let mut p = connected();
    p.alice.dispatch(
        &p.a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "hello bob".into(),
            },
        },
    );
    assert_eq!(
        p.alice.outcome(&p.a, "m1"),
        Some(OutcomeClass::PendingNetwork)
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(p.alice.outcome(&p.a, "m1"), Some(OutcomeClass::Committed));
    assert_eq!(
        texts(&timeline(&p.bob, &p.b, &p.b_conv)),
        vec!["in:hello bob"]
    );
    // Bob replies on the same session; Alice replies again (new DH ratchet steps in each direction).
    for (i, (who, text)) in [
        (1, "hi alice"),
        (0, "how are you"),
        (1, "fine"),
        (1, "and you"),
        (0, "great"),
    ]
    .into_iter()
    .enumerate()
    {
        let op = format!("r{i}");
        if who == 1 {
            p.bob.dispatch(
                &p.b,
                &op,
                IntentBody::SendMessage {
                    conversation_id: p.b_conv.clone(),
                    draft: Draft { text: text.into() },
                },
            );
        } else {
            p.alice.dispatch(
                &p.a,
                &op,
                IntentBody::SendMessage {
                    conversation_id: p.a_conv.clone(),
                    draft: Draft { text: text.into() },
                },
            );
        }
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    }
    assert_eq!(
        texts(&timeline(&p.alice, &p.a, &p.a_conv)),
        vec![
            "out:hello bob",
            "in:hi alice",
            "out:how are you",
            "in:fine",
            "in:and you",
            "out:great"
        ]
    );
    assert_eq!(
        texts(&timeline(&p.bob, &p.b, &p.b_conv)),
        vec![
            "in:hello bob",
            "out:hi alice",
            "in:how are you",
            "out:fine",
            "out:and you",
            "in:great"
        ]
    );
    // The relay only ever saw ciphertext: no plaintext fragment appears in anything deposited.
    for bytes in p.net.all_deposited_bytes() {
        for needle in ["hello bob", "hi alice", "Alice", "Bob"] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                "relay saw {needle}"
            );
        }
    }
    let ConversationView { summary, .. } = p.alice.query(Query::GetConversationView {
        ctx: p.a.clone(),
        conversation_id: p.a_conv.clone(),
    });
    assert!(summary.notices.is_empty());
}

#[test]
fn first_contact_request_is_a_w0_valid_message_request() {
    let net = LoopbackNetwork::default();
    let (mut alice, mut bob) = (Runtime::new("alice", &net), Runtime::new("bob", &net));
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
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
    bob.flush_net(&b); // resolve
    bob.flush_net(&b); // deposit request
    let reg = om_codec::Registry::load();
    let request = net
        .all_deposited_bytes()
        .into_iter()
        .find(|b| reg.check("MessageRequest", b, false).is_ok());
    assert!(
        request.is_some(),
        "the request on the relay validates as the frozen W0 MessageRequest"
    );
    // Alice has not accepted: nothing is pinned, no conversation exists for her, only an introduction request.
    alice.poll(&a);
    let list: Vec<ConversationSummary> = alice.query(Query::GetConversationList { ctx: a.clone() });
    assert!(list.is_empty());
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a });
    assert_eq!(intros.len(), 1);
}

#[test]
fn state_survives_restart_and_messaging_continues() {
    let mut p = connected();
    p.alice.dispatch(
        &p.a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "before restart".into(),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let a = p.alice.restart().remove(0);
    let b = p.bob.restart().remove(0);
    assert_ne!(
        a.generation, p.a.generation,
        "a restart is a new generation"
    );
    assert_eq!(
        texts(&timeline(&p.bob, &b, &p.b_conv)),
        vec!["in:before restart"]
    );
    p.bob.dispatch(
        &b,
        "m2",
        IntentBody::SendMessage {
            conversation_id: p.b_conv.clone(),
            draft: Draft {
                text: "after restart".into(),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &a), (&mut p.bob, &b)]);
    assert_eq!(
        texts(&timeline(&p.alice, &a, &p.a_conv)),
        vec!["out:before restart", "in:after restart"]
    );
    // Pending operations are reconstructed from the durable log (UI-13).
    let ops = p.alice.core.pending_operations(&a).unwrap();
    assert!(
        ops.iter()
            .any(|o| o.operation_id == "m1" && o.outcome == OutcomeClass::Committed)
    );
}

#[test]
fn a_failed_host_commit_releases_nothing() {
    let mut p = connected();
    p.alice.store.fail_next_commit();
    let step = p
        .alice
        .core
        .dispatch(&Intent {
            ctx: p.a.clone(),
            operation_id: "m1".into(),
            body: IntentBody::SendMessage {
                conversation_id: p.a_conv.clone(),
                draft: Draft {
                    text: "never persisted".into(),
                },
            },
        })
        .unwrap();
    assert!(!p.alice.settle(&step), "the store refused the commit");
    assert!(
        timeline(&p.alice, &p.a, &p.a_conv).is_empty(),
        "the staged message is not visible"
    );
    assert!(
        p.alice.core.net_ops(&p.a).unwrap().is_empty(),
        "no network effect was released"
    );
    assert_eq!(p.alice.outcome(&p.a, "m1"), None);
    // The exact same logical operation can be dispatched again and now succeeds, with the ratchet un-advanced.
    p.alice.dispatch(
        &p.a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "persisted".into(),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(
        texts(&timeline(&p.bob, &p.b, &p.b_conv)),
        vec!["in:persisted"]
    );
}

#[test]
fn timeout_after_send_is_reconciled_not_rejected_and_not_duplicated() {
    let mut p = connected();
    p.alice.dispatch(
        &p.a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "once".into(),
            },
        },
    );
    p.net.inject(Fault::TimeoutAfterEffect); // delivered, but the ack is lost
    p.alice.flush_net(&p.a);
    assert_eq!(
        p.alice.outcome(&p.a, "m1"),
        Some(OutcomeClass::PendingReconciliation)
    );
    let t = timeline(&p.alice, &p.a, &p.a_conv);
    assert!(matches!(
        &t[0],
        TimelineItem::Outgoing {
            delivery: DeliveryState::PendingReconciliation,
            ..
        }
    ));
    // The process restarts while the outcome is unknown; the exact envelope is retried, the op id never changes.
    let a = p.alice.restart().remove(0);
    assert_eq!(
        p.alice.outcome(&a, "m1"),
        Some(OutcomeClass::PendingReconciliation)
    );
    settle_all(&mut [(&mut p.alice, &a), (&mut p.bob, &p.b)]);
    assert_eq!(p.alice.outcome(&a, "m1"), Some(OutcomeClass::Committed));
    assert_eq!(
        texts(&timeline(&p.bob, &p.b, &p.b_conv)),
        vec!["in:once"],
        "the redelivered envelope is processed once"
    );
    let v: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv.clone(),
    });
    assert!(
        v.summary.notices.is_empty(),
        "a benign redelivery is not an integrity failure"
    );
    // An unavailable relay (definitely not sent) stays PendingNetwork and then succeeds.
    p.bob.dispatch(
        &p.b,
        "m2",
        IntentBody::SendMessage {
            conversation_id: p.b_conv.clone(),
            draft: Draft {
                text: "later".into(),
            },
        },
    );
    p.net.inject(Fault::Unavailable);
    p.bob.flush_net(&p.b);
    assert_eq!(
        p.bob.outcome(&p.b, "m2"),
        Some(OutcomeClass::PendingNetwork)
    );
    settle_all(&mut [(&mut p.alice, &a), (&mut p.bob, &p.b)]);
    assert_eq!(p.bob.outcome(&p.b, "m2"), Some(OutcomeClass::Committed));
}

#[test]
fn replayed_relay_items_and_forged_envelopes_change_nothing() {
    let mut p = connected();
    p.alice.dispatch(
        &p.a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "real".into(),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    // A hostile relay re-serves an old item with a lower-or-equal sequence: the committed cursor ignores it.
    let before = timeline(&p.bob, &p.b, &p.b_conv);
    for t in p.bob.core.poll_targets(&p.b).unwrap() {
        let replay = [om_core::host::Inbound {
            seq: 1,
            bytes: vec![0xde, 0xad],
        }];
        let step = p.bob.core.inbound(&p.b, &t.mailbox, &replay).unwrap();
        p.bob.settle(&step);
    }
    assert_eq!(timeline(&p.bob, &p.b, &p.b_conv), before);
    // A forged envelope at a fresh sequence is counted as an integrity failure and delivers nothing.
    let t = p.bob.core.poll_targets(&p.b).unwrap().remove(0);
    let forged = [om_core::host::Inbound {
        seq: t.after + 100,
        bytes: vec![0x84, 0x41, 0x00, 0xf6, 0x40, 0x40],
    }];
    let step = p.bob.core.inbound(&p.b, &t.mailbox, &forged).unwrap();
    p.bob.settle(&step);
    assert_eq!(timeline(&p.bob, &p.b, &p.b_conv), before);
    let v: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv.clone(),
    });
    assert_eq!(v.summary.notices.len(), 1);
    assert_eq!(
        v.summary.notices[0].code,
        SecurityNoticeCode::IntegrityFailure
    );
}

#[test]
fn profiles_are_namespaced_and_contexts_cannot_be_retargeted() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("multi", &net);
    let p1 = create_profile(&mut rt, "one", "One");
    let p2 = create_profile(&mut rt, "two", "Two");
    let profiles: Vec<ProfileSummary> = rt.query(Query::GetProfileList);
    assert_eq!(profiles.len(), 2);
    rt.dispatch(&p1, "inv", IntentBody::CreateInvite);
    // Profile two cannot see profile one's invite, operation or state.
    let seen: Option<InviteView> = rt.query(Query::GetInvite {
        ctx: p2.clone(),
        operation_id: "inv".into(),
    });
    assert!(seen.is_none());
    assert_eq!(rt.outcome(&p2, "inv"), None);
    assert_eq!(
        rt.core
            .pending_operations(&p2)
            .unwrap()
            .iter()
            .filter(|o| o.operation_id == "inv")
            .count(),
        0
    );
    // Distinct, unlinkable key material per profile (no hidden shared identifier).
    let (s1, s2) = (rt.store.raw("one"), rt.store.raw("two"));
    assert!(s1.iter().all(|a| s2.iter().all(|b| a != b)));
    // A stale or forged context fails closed with an error, not an outcome.
    let forged = ProfileExecutionContext {
        profile_id: "one".into(),
        generation: "gen-forged-999".into(),
    };
    assert!(matches!(
        rt.core.query(&Query::GetDevices {
            ctx: forged.clone()
        }),
        Err(CoreError::ProfileAccess(_))
    ));
    assert!(matches!(
        rt.core.dispatch(&Intent {
            ctx: forged,
            operation_id: "x".into(),
            body: IntentBody::CreateInvite
        }),
        Err(CoreError::ProfileAccess(_))
    ));
    // Switching is a presentation switch: it commits for an open profile and is not_found for an unknown one.
    rt.dispatch(
        &p1,
        "sw1",
        IntentBody::SwitchIdentityProfile {
            target_profile_id: "two".into(),
        },
    );
    rt.dispatch(
        &p1,
        "sw2",
        IntentBody::SwitchIdentityProfile {
            target_profile_id: "nope".into(),
        },
    );
    assert_eq!(rt.outcome(&p1, "sw1"), Some(OutcomeClass::Committed));
    assert_eq!(
        rt.outcome(&p1, "sw2"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
    // After close and reopen the old context is stale.
    rt.core.close_profile("one");
    assert!(rt.core.query(&Query::GetDevices { ctx: p1 }).is_err());
}

#[test]
fn operation_ids_are_idempotent_and_never_reused_for_another_intent() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("idem", &net);
    let ctx = create_profile(&mut rt, "me", "Me");
    let h1 = rt.dispatch(&ctx, "inv", IntentBody::CreateInvite);
    let n = rt.core.net_ops(&ctx).unwrap().len();
    let h2 = rt.dispatch(&ctx, "inv", IntentBody::CreateInvite);
    assert_eq!(h1, h2);
    assert_eq!(
        rt.core.net_ops(&ctx).unwrap().len(),
        n,
        "no second semantic operation"
    );
    let err = rt.core.dispatch(&Intent {
        ctx,
        operation_id: "inv".into(),
        body: IntentBody::ClearAdaptiveNavigation,
    });
    assert!(matches!(err, Err(CoreError::OperationIdConflict)));
}

#[test]
fn destructive_and_unbuilt_intents_fail_closed() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("guard", &net);
    let ctx = create_profile(&mut rt, "me", "Me");
    rt.dispatch(
        &ctx,
        "no-token",
        IntentBody::RevokeDevice {
            device_id: "d".into(),
            confirmation: serde_json::Value::Null,
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "no-token"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unauthorized
        })
    );
    rt.dispatch(&ctx, "unbuilt", IntentBody::OfferDeviceLink);
    assert_eq!(
        rt.outcome(&ctx, "unbuilt"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unsupported
        })
    );
    let confirmation =
        common::confirmation(&mut rt, &ctx, "StartRecovery", "identity:recovery_key");
    rt.dispatch(
        &ctx,
        "rec",
        IntentBody::StartRecovery {
            scope: RecoveryScope::Identity,
            custodian: RecoveryCustodianCode::RecoveryKey,
            confirmation,
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "rec"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unsupported
        })
    );
    rt.dispatch(
        &ctx,
        "link",
        IntentBody::RequestDeviceLink {
            link: "om:link/aaaaaaaaaaaaaaaaaaaa".into(),
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "link"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unsupported
        })
    );
    rt.dispatch(
        &ctx,
        "bad-msg",
        IntentBody::SendMessage {
            conversation_id: "nope".into(),
            draft: Draft { text: "x".into() },
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "bad-msg"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
}

#[test]
fn invite_failure_paths_are_conservative() {
    let net = LoopbackNetwork::default();
    let (mut alice, mut bob) = (Runtime::new("alice", &net), Runtime::new("bob", &net));
    let a = create_profile(&mut alice, "pa", "Alice");
    let b = create_profile(&mut bob, "pb", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    // Malformed link grammar.
    for (i, bad) in [
        "",
        "om:invite/short",
        "https://x/y",
        "om:invite/!!!!!!!!!!!!!!!!!!!!!!",
    ]
    .into_iter()
    .enumerate()
    {
        bob.dispatch(
            &b,
            &format!("bad{i}"),
            IntentBody::AcceptInvitation { link: bad.into() },
        );
        assert_eq!(
            bob.outcome(&b, &format!("bad{i}")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Invalid
            }),
            "{bad}"
        );
    }
    // Your own invite is not acceptable.
    alice.dispatch(
        &a,
        "self",
        IntentBody::AcceptInvitation { link: link.clone() },
    );
    assert_eq!(
        alice.outcome(&a, "self"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    // The introduction service substituted the descriptor: the link's committed digests catch it.
    let other = Runtime::new("mallory", &net);
    drop(other);
    let mut mallory = Runtime::new("mallory", &net);
    let m = create_profile(&mut mallory, "pm", "Mallory");
    mallory.dispatch(&m, "minv", IntentBody::CreateInvite);
    // Replace Alice's stored descriptor with Mallory's (same invite id as Alice's link).
    let alice_token = link.trim_start_matches("om:invite/").to_string();
    let raw = om_core_test_unb64(&alice_token);
    let mallory_desc = mallory
        .core
        .net_ops(&m)
        .unwrap()
        .into_iter()
        .find_map(|o| match o.call {
            om_core::host::NetCall::PutInvite { descriptor, .. } => Some(descriptor),
            _ => None,
        });
    let swap = om_core::host::NetCall::PutInvite {
        invite_id: raw[..16].to_vec(),
        descriptor: mallory_desc.unwrap(),
        expires_at: u64::MAX,
    };
    assert_eq!(net.perform(&swap), om_core::host::NetResult::Ok);
    bob.dispatch(
        &b,
        "acc-swapped",
        IntentBody::AcceptInvitation { link: link.clone() },
    );
    bob.flush_net(&b);
    assert_eq!(
        bob.outcome(&b, "acc-swapped"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::IntegrityFailure
        })
    );
    assert!(
        bob.query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: b.clone() })
            .is_empty(),
        "nothing was pinned or created"
    );
    // Revoked invite: the service no longer has it.
    alice.dispatch(
        &a,
        "rev",
        IntentBody::RevokeInvite {
            invite_id: "inv".into(),
        },
    );
    alice.flush_net(&a);
    assert_eq!(alice.outcome(&a, "rev"), Some(OutcomeClass::Committed));
    net.revoke_all_invites();
    bob.dispatch(&b, "acc-revoked", IntentBody::AcceptInvitation { link });
    bob.flush_net(&b);
    assert_eq!(
        bob.outcome(&b, "acc-revoked"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
}

fn om_core_test_unb64(s: &str) -> Vec<u8> {
    // Same alphabet as the invite link; decoded independently here so the test does not trust the Core helper.
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        acc = acc << 6 | alphabet.iter().position(|&a| a == c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    out
}

#[test]
fn blocking_a_person_stops_delivery_locally() {
    let mut p = connected();
    let person = p
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: p.a.clone() })[0]
        .person_id
        .clone();
    let confirmation = common::confirmation(&mut p.alice, &p.a, "BlockPerson", &person);
    p.alice.dispatch(
        &p.a,
        "blk",
        IntentBody::BlockPerson {
            person_id: person,
            scope: BlockScope::Local,
            confirmation,
        },
    );
    assert_eq!(p.alice.outcome(&p.a, "blk"), Some(OutcomeClass::Committed));
    p.bob.dispatch(
        &p.b,
        "m1",
        IntentBody::SendMessage {
            conversation_id: p.b_conv.clone(),
            draft: Draft {
                text: "are you there".into(),
            },
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert!(timeline(&p.alice, &p.a, &p.a_conv).is_empty());
    p.alice.dispatch(
        &p.a,
        "m2",
        IntentBody::SendMessage {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "blocked".into(),
            },
        },
    );
    assert_eq!(
        p.alice.outcome(&p.a, "m2"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unavailable
        })
    );
}

mod props {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum Act {
        AliceSends(u8),
        BobSends(u8),
        Pump,
        RestartAlice,
        RestartBob,
        RelayHiccup,
    }

    fn act() -> impl Strategy<Value = Act> {
        prop_oneof![
            (0u8..40).prop_map(Act::AliceSends),
            (0u8..40).prop_map(Act::BobSends),
            Just(Act::Pump),
            Just(Act::RestartAlice),
            Just(Act::RestartBob),
            Just(Act::RelayHiccup),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]
        /// Whatever the interleaving of sends, restarts, lost acks and polling, every message is delivered exactly
        /// once and in per-sender order, and no benign duplicate is reported as an integrity failure.
        #[test]
        fn delivery_is_exactly_once_and_ordered(acts in proptest::collection::vec(act(), 1..24)) {
            let mut p = connected();
            let (mut a, mut b) = (p.a.clone(), p.b.clone());
            let (mut sent_a, mut sent_b) = (vec![], vec![]);
            for (i, act) in acts.iter().enumerate() {
                match act {
                    Act::AliceSends(n) => {
                        let t = format!("a{i}-{n}");
                        p.alice.dispatch(&a, &format!("oa{i}"), IntentBody::SendMessage { conversation_id: p.a_conv.clone(), draft: Draft { text: t.clone() } });
                        sent_a.push(t);
                    }
                    Act::BobSends(n) => {
                        let t = format!("b{i}-{n}");
                        p.bob.dispatch(&b, &format!("ob{i}"), IntentBody::SendMessage { conversation_id: p.b_conv.clone(), draft: Draft { text: t.clone() } });
                        sent_b.push(t);
                    }
                    Act::Pump => { p.alice.pump(&a); p.bob.pump(&b); }
                    Act::RestartAlice => a = p.alice.restart().remove(0),
                    Act::RestartBob => b = p.bob.restart().remove(0),
                    Act::RelayHiccup => p.net.inject(Fault::TimeoutAfterEffect),
                }
            }
            settle_all(&mut [(&mut p.alice, &a), (&mut p.bob, &b)]);
            let at = timeline(&p.alice, &a, &p.a_conv);
            let bt = timeline(&p.bob, &b, &p.b_conv);
            let from = |t: &[TimelineItem], incoming: bool| -> Vec<String> {
                t.iter().filter_map(|i| match (i, incoming) {
                    (TimelineItem::Incoming { text, .. }, true) | (TimelineItem::Outgoing { text, .. }, false) => Some(text.clone()),
                    _ => None,
                }).collect()
            };
            prop_assert_eq!(from(&bt, true), sent_a.clone());
            prop_assert_eq!(from(&at, true), sent_b.clone());
            prop_assert_eq!(from(&at, false), sent_a);
            prop_assert_eq!(from(&bt, false), sent_b);
            let sa: ConversationView = p.alice.query(Query::GetConversationView { ctx: a.clone(), conversation_id: p.a_conv.clone() });
            let sb: ConversationView = p.bob.query(Query::GetConversationView { ctx: b.clone(), conversation_id: p.b_conv.clone() });
            prop_assert!(sa.summary.notices.is_empty() && sb.summary.notices.is_empty());
        }
    }
}
