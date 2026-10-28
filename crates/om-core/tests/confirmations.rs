// SPDX-License-Identifier: MPL-2.0
//! M2 (audit 2026-10-28) as ADR-0181 section 8 settles it: a destructive intent runs only with a confirmation nonce
//! Core issued for exactly that intent kind and target in that profile, unspent and unexpired.

mod common;

use common::{confirmation, create_profile};
use om_core::CONFIRMATION_TTL_MS;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime};
use serde_json::json;

fn setup() -> (Runtime, ProfileExecutionContext, String) {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("owner", &net);
    let ctx = create_profile(&mut rt, "p", "Owner");
    let (id, step) = rt.core.debug_link_messaging_device(&ctx).unwrap();
    rt.settle(&step);
    (rt, ctx, id)
}

fn revoke(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    op: &str,
    id: &str,
    token: serde_json::Value,
) -> Option<OutcomeClass> {
    rt.dispatch(
        ctx,
        op,
        IntentBody::RevokeDevice {
            device_id: id.into(),
            confirmation: token,
        },
    );
    rt.outcome(ctx, op)
}

const UNAUTHORIZED: Option<OutcomeClass> = Some(OutcomeClass::Rejected {
    reason: RejectReason::Unauthorized,
});

#[test]
fn only_a_core_issued_nonce_for_this_kind_and_target_confirms() {
    let (mut rt, ctx, id) = setup();
    // The old UI-minted shapes are no longer confirmations, whatever they contain.
    for (n, bogus) in [
        json!({}),
        json!({ "confirmed": true }),
        json!({ "nonce": 7 }),
        json!({ "nonce": "" }),
        json!({ "nonce": "00".repeat(16) }),
        json!(null),
        json!(true),
        json!("nonce"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            revoke(&mut rt, &ctx, &format!("b{n}"), &id, bogus),
            UNAUTHORIZED,
            "case {n}"
        );
    }
    // A nonce issued for another kind, or for another target, is refused.
    let wrong_kind = confirmation(&mut rt, &ctx, "SuspendDevice", &id);
    assert_eq!(revoke(&mut rt, &ctx, "k", &id, wrong_kind), UNAUTHORIZED);
    let wrong_target = confirmation(&mut rt, &ctx, "RevokeDevice", &"11".repeat(16));
    assert_eq!(revoke(&mut rt, &ctx, "t", &id, wrong_target), UNAUTHORIZED);
    // The device is still there: nothing ran.
    let v: DevicesView = rt.query(Query::GetDevices { ctx: ctx.clone() });
    assert!(
        v.devices
            .iter()
            .any(|d| d.id == id && d.state != DeviceStateCode::Revoked)
    );
    // The matching nonce confirms, once.
    let good = confirmation(&mut rt, &ctx, "RevokeDevice", &id);
    assert_eq!(
        revoke(&mut rt, &ctx, "ok", &id, good.clone()),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(
        revoke(&mut rt, &ctx, "again", &id, good),
        UNAUTHORIZED,
        "a nonce is single use"
    );
}

#[test]
fn a_mismatched_presentation_spends_the_nonce() {
    let (mut rt, ctx, id) = setup();
    let other = "22".repeat(16);
    let nonce = confirmation(&mut rt, &ctx, "RevokeDevice", &id);
    // Presented for the wrong target first: refused, and the nonce is gone, so the right target cannot reuse it.
    assert_eq!(
        revoke(&mut rt, &ctx, "wrong", &other, nonce.clone()),
        UNAUTHORIZED
    );
    assert_eq!(revoke(&mut rt, &ctx, "right", &id, nonce), UNAUTHORIZED);
}

#[test]
fn a_nonce_expires_and_is_bound_to_its_profile() {
    let (mut rt, ctx, id) = setup();
    let stale = confirmation(&mut rt, &ctx, "RevokeDevice", &id);
    rt.tick(CONFIRMATION_TTL_MS + 1);
    assert_eq!(
        revoke(&mut rt, &ctx, "late", &id, stale),
        UNAUTHORIZED,
        "expired nonces are refused"
    );
    // The query no longer returns an expired nonce either.
    rt.dispatch(
        &ctx,
        "q",
        IntentBody::IssueConfirmation {
            intent_kind: "RevokeDevice".into(),
            target: id.clone(),
        },
    );
    rt.tick(CONFIRMATION_TTL_MS + 1);
    let none: Option<ConfirmationView> = rt.query(Query::GetConfirmation {
        ctx: ctx.clone(),
        operation_id: "q".into(),
    });
    assert!(none.is_none());

    // A nonce minted in one profile means nothing in another.
    let other_ctx = create_profile(&mut rt, "p2", "Second");
    let (id2, step) = rt.core.debug_link_messaging_device(&other_ctx).unwrap();
    rt.settle(&step);
    let foreign = confirmation(&mut rt, &ctx, "RevokeDevice", &id2);
    assert_eq!(
        revoke(&mut rt, &other_ctx, "x", &id2, foreign),
        UNAUTHORIZED
    );
}

#[test]
fn issuing_is_limited_to_destructive_kinds_and_bounded() {
    let (mut rt, ctx, id) = setup();
    for (n, (kind, target)) in [
        ("SendMessage", "x"),
        ("Nope", "x"),
        ("RevokeDevice", ""),
        ("RevokeDevice", "a\nb"),
    ]
    .into_iter()
    .enumerate()
    {
        rt.dispatch(
            &ctx,
            &format!("i{n}"),
            IntentBody::IssueConfirmation {
                intent_kind: kind.into(),
                target: target.into(),
            },
        );
        assert_eq!(
            rt.outcome(&ctx, &format!("i{n}")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Invalid
            }),
            "{kind}/{target:?}"
        );
    }
    // At most 16 nonces are outstanding: the oldest is dropped.
    let first = confirmation(&mut rt, &ctx, "RevokeDevice", &id);
    for _ in 0..16 {
        confirmation(&mut rt, &ctx, "RevokeDevice", &id);
    }
    assert_eq!(revoke(&mut rt, &ctx, "old", &id, first), UNAUTHORIZED);
    let st = rt.core.debug_app_state(&ctx).unwrap();
    assert_eq!(st["confirmations"].as_array().unwrap().len(), 16);
}

#[test]
fn every_destructive_kind_checks_the_nonce_including_the_unbuilt_ones() {
    let (mut rt, ctx, _) = setup();
    let bare = json!({ "nonce": "ab".repeat(16) });
    let cases: Vec<(&str, IntentBody)> = vec![
        (
            "BlockPerson",
            IntentBody::BlockPerson {
                person_id: "p".into(),
                scope: BlockScope::Local,
                confirmation: bare.clone(),
            },
        ),
        (
            "SuspendDevice",
            IntentBody::SuspendDevice {
                device_id: "d".into(),
                confirmation: bare.clone(),
            },
        ),
        (
            "ChangePrivacyPolicy",
            IntentBody::ChangePrivacyPolicy {
                policy: PrivacyPolicyLevel::Hardened,
                confirmation: bare.clone(),
            },
        ),
        (
            "ApproveDeviceLink",
            IntentBody::ApproveDeviceLink {
                request_id: "r".into(),
                confirmation: bare.clone(),
            },
        ),
        (
            "StartRecovery",
            IntentBody::StartRecovery {
                scope: RecoveryScope::Identity,
                custodian: RecoveryCustodianCode::RecoveryKey,
                confirmation: bare.clone(),
            },
        ),
        (
            "VetoRecovery",
            IntentBody::VetoRecovery {
                request_id: "r".into(),
                confirmation: bare,
            },
        ),
    ];
    for (kind, body) in cases {
        let op = format!("u-{kind}");
        rt.dispatch(&ctx, &op, body);
        assert_eq!(rt.outcome(&ctx, &op), UNAUTHORIZED, "{kind}");
    }
}

#[test]
fn targets_are_the_documented_strings() {
    let body = IntentBody::StartRecovery {
        scope: RecoveryScope::History,
        custodian: RecoveryCustodianCode::SyncedPasskey,
        confirmation: json!({}),
    };
    assert_eq!(
        body.confirmation_target().as_deref(),
        Some("history:synced_passkey")
    );
    let body = IntentBody::ChangePrivacyPolicy {
        policy: PrivacyPolicyLevel::Maximum,
        confirmation: json!({}),
    };
    assert_eq!(body.confirmation_target().as_deref(), Some("maximum"));
    assert_eq!(IntentBody::CreateInvite.confirmation_target(), None);
}
