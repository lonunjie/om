// SPDX-License-Identifier: MPL-2.0
//! Signed AccountControlState in the engine: genesis, device views derived from the head, SuspendDevice and
//! RevokeDevice as real authorized transitions, and a multi-generation chain accepted by a peer at first contact.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

fn profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
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

fn devices(rt: &Runtime, ctx: &ProfileExecutionContext) -> DevicesView {
    rt.query(Query::GetDevices { ctx: ctx.clone() })
}

fn outcome(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    op: &str,
    body: IntentBody,
) -> Option<OutcomeClass> {
    rt.dispatch(ctx, op, body);
    rt.outcome(ctx, op)
}

fn rejected(r: RejectReason) -> Option<OutcomeClass> {
    Some(OutcomeClass::Rejected { reason: r })
}

#[test]
fn a_lone_device_has_a_signed_genesis_and_can_neither_suspend_nor_remove_itself() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("solo", &net);
    let ctx = profile(&mut rt, "p", "Solo");
    let v = devices(&rt, &ctx);
    assert_eq!(v.control_state, AccountControlStateCode::Normal);
    assert_eq!(v.devices.len(), 1);
    let me = v.devices[0].clone();
    assert!(me.is_current && me.actions.is_empty());
    assert!(
        me.cautions
            .contains(&DeviceCautionCode::LastVotingController)
    );
    // Self-targeting, unknown and malformed targets are refused with distinct reasons.
    for (op, id, want) in [
        ("s-self", me.id.clone(), RejectReason::Conflict),
        ("s-none", "00".repeat(16), RejectReason::NotFound),
        ("s-bad", "zz".into(), RejectReason::Invalid),
    ] {
        let confirmation = nonce(&mut rt, &ctx, "SuspendDevice", &id);
        assert_eq!(
            outcome(
                &mut rt,
                &ctx,
                op,
                IntentBody::SuspendDevice {
                    device_id: id,
                    confirmation
                }
            ),
            rejected(want),
            "{op}"
        );
    }
    let confirmation = nonce(&mut rt, &ctx, "RevokeDevice", &me.id);
    assert_eq!(
        outcome(
            &mut rt,
            &ctx,
            "r-self",
            IntentBody::RevokeDevice {
                device_id: me.id.clone(),
                confirmation
            }
        ),
        rejected(RejectReason::Conflict),
        "section 13: never leave zero voting controllers"
    );
    // Nothing changed: still generation 1 (device list identical).
    assert_eq!(devices(&rt, &ctx).devices, vec![me]);
}

#[test]
fn removing_a_messaging_device_is_a_signed_transition_and_is_terminal() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("owner", &net);
    let ctx = profile(&mut rt, "p", "Owner");
    let (id, step) = rt.core.debug_link_messaging_device(&ctx).unwrap();
    rt.settle(&step);
    let v = devices(&rt, &ctx);
    let phone = v
        .devices
        .iter()
        .find(|d| d.id == id)
        .expect("linked device is listed")
        .clone();
    assert_eq!(
        (phone.role, phone.state),
        (DeviceRole::Messaging, DeviceStateCode::Active)
    );
    assert_eq!(
        phone.actions,
        vec![DeviceActionCode::Revoke],
        "one controller may remove a messaging device"
    );
    // Messaging devices are removed, never quarantined (section 4.1).
    let confirmation = nonce(&mut rt, &ctx, "SuspendDevice", &id);
    assert_eq!(
        outcome(
            &mut rt,
            &ctx,
            "sus",
            IntentBody::SuspendDevice {
                device_id: id.clone(),
                confirmation
            }
        ),
        rejected(RejectReason::Conflict)
    );
    // Without a confirmation token a destructive intent never runs (UI-11).
    assert_eq!(
        outcome(
            &mut rt,
            &ctx,
            "no-token",
            IntentBody::RevokeDevice {
                device_id: id.clone(),
                confirmation: serde_json::Value::Null
            }
        ),
        rejected(RejectReason::Unauthorized)
    );
    // A token must be a token (an object): `false`, `0`, `""` and `[]` are not confirmations (audit M2).
    for (n, bogus) in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
        serde_json::json!([]),
        serde_json::json!("confirmed"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            outcome(
                &mut rt,
                &ctx,
                &format!("bogus-{n}"),
                IntentBody::RevokeDevice {
                    device_id: id.clone(),
                    confirmation: bogus
                }
            ),
            rejected(RejectReason::Unauthorized),
            "case {n}"
        );
    }
    let confirmation = nonce(&mut rt, &ctx, "RevokeDevice", &id);
    assert_eq!(
        outcome(
            &mut rt,
            &ctx,
            "rev",
            IntentBody::RevokeDevice {
                device_id: id.clone(),
                confirmation
            }
        ),
        Some(OutcomeClass::Committed)
    );
    let v = devices(&rt, &ctx);
    let gone = v.devices.iter().find(|d| d.id == id).unwrap();
    assert_eq!(gone.state, DeviceStateCode::Revoked);
    assert!(gone.actions.is_empty());
    // Terminal: the same DeviceID cannot be removed (or anything else done to it) again.
    let confirmation = nonce(&mut rt, &ctx, "RevokeDevice", &id);
    assert_eq!(
        outcome(
            &mut rt,
            &ctx,
            "rev2",
            IntentBody::RevokeDevice {
                device_id: id,
                confirmation
            }
        ),
        rejected(RejectReason::NotFound)
    );
}

#[test]
fn the_chain_survives_restart_and_a_multi_generation_head_is_accepted_by_a_peer() {
    let net = LoopbackNetwork::default();
    let (mut alice, mut bob) = (Runtime::new("alice", &net), Runtime::new("bob", &net));
    let a = profile(&mut alice, "pa", "Alice");
    let b = profile(&mut bob, "pb", "Bob");
    let (id, step) = alice.core.debug_link_messaging_device(&a).unwrap();
    alice.settle(&step);
    let confirmation = nonce(&mut alice, &a, "RevokeDevice", &id);
    assert_eq!(
        outcome(
            &mut alice,
            &a,
            "rev",
            IntentBody::RevokeDevice {
                device_id: id,
                confirmation
            }
        ),
        Some(OutcomeClass::Committed)
    );
    // Restart: the chain is re-verified from stored bytes; the view is unchanged.
    let before = devices(&alice, &a);
    let ctxs = alice.restart();
    assert_eq!(devices(&alice, &ctxs[0]), before);
    let a = ctxs[0].clone();
    // Generation-3 head: Alice's binding carries the verified chain and Bob accepts it at first contact.
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
    assert_eq!(bob.outcome(&b, "acc"), Some(OutcomeClass::Committed));
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    assert_eq!(intros.len(), 1, "Alice received Bob's request");
    alice.dispatch(
        &a,
        "ok",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    for (rt, ctx) in [(&alice, &a), (&bob, &b)] {
        let list: Vec<ConversationSummary> =
            rt.query(Query::GetConversationList { ctx: ctx.clone() });
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].relationship,
            RelationshipStateCode::Active,
            "Bob verified Alice's generation-3 chain and binding, and Alice bound Bob's"
        );
    }
}

#[test]
fn a_record_signed_by_another_intro_identity_than_the_linked_one_is_refused() {
    // A hostile introduction service swaps in a different (even validly signed) record: the link commits to the
    // intro identity's key digest, so the invitee refuses (ADR-0181 section 2).
    let net = LoopbackNetwork::default();
    let (mut alice, mut mallory, mut bob) = (
        Runtime::new("alice", &net),
        Runtime::new("mallory", &net),
        Runtime::new("bob", &net),
    );
    let a = profile(&mut alice, "pa", "Alice");
    let m = profile(&mut mallory, "pm", "Mallory");
    let b = profile(&mut bob, "pb", "Bob");
    for (rt, ctx, op) in [(&mut alice, &a, "ia"), (&mut mallory, &m, "im")] {
        rt.dispatch(ctx, op, IntentBody::CreateInvite);
        rt.flush_net(ctx);
    }
    let link_a = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "ia".into(),
        })
        .unwrap()
        .link;
    // Swap the stored descriptor behind Alice's invite id with Mallory's.
    let id_a = om_core::crypto::hex(&unb64(&link_a)[..16]);
    let id_m = {
        let l = mallory
            .query::<Option<InviteView>>(Query::GetInvite {
                ctx: m.clone(),
                operation_id: "im".into(),
            })
            .unwrap()
            .link;
        om_core::crypto::hex(&unb64(&l)[..16])
    };
    net.swap_invites(
        &om_core::crypto::unhex(&id_a).unwrap(),
        &om_core::crypto::unhex(&id_m).unwrap(),
    );
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link: link_a });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(
        bob.outcome(&b, "acc"),
        rejected(RejectReason::IntegrityFailure)
    );
}

fn unb64(link: &str) -> Vec<u8> {
    let t = link.trim_start_matches("om:invite/");
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in t.bytes() {
        let v = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
            .iter()
            .position(|&x| x == ch)
            .unwrap() as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    out
}
