// SPDX-License-Identifier: MPL-2.0
//! `ChangePrivacyPolicy` (ADR-0181 section 8, UI-11): the person's own protection level changes only with a nonce Core issued
//! for that exact level, unspent, unexpired, and issued while the level still had the rank it has now.

mod common;

use common::{confirmation, create_profile};
use om_core::CONFIRMATION_TTL_MS;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime};

fn setup() -> (Runtime, ProfileExecutionContext) {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("owner", &net);
    let ctx = create_profile(&mut rt, "p", "Owner");
    (rt, ctx)
}

fn level(rt: &Runtime, ctx: &ProfileExecutionContext) -> PrivacyPolicyLevel {
    rt.query(Query::GetPrivacyLevel { ctx: ctx.clone() })
}

fn change(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    op: &str,
    policy: PrivacyPolicyLevel,
    token: serde_json::Value,
) -> Option<OutcomeClass> {
    rt.dispatch(
        ctx,
        op,
        IntentBody::ChangePrivacyPolicy {
            policy,
            confirmation: token,
        },
    );
    rt.outcome(ctx, op)
}

const COMMITTED: Option<OutcomeClass> = Some(OutcomeClass::Committed);
const UNAUTHORIZED: Option<OutcomeClass> = Some(OutcomeClass::Rejected {
    reason: RejectReason::Unauthorized,
});
const CONFLICT: Option<OutcomeClass> = Some(OutcomeClass::Rejected {
    reason: RejectReason::Conflict,
});

/// Move to `to` through the whole two-step path, as a person's tap does.
fn set(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str, to: PrivacyPolicyLevel) {
    let code = serde_json::to_value(to).unwrap();
    let token = confirmation(rt, ctx, "ChangePrivacyPolicy", code.as_str().unwrap());
    assert_eq!(change(rt, ctx, op, to, token), COMMITTED, "{op}");
}

#[test]
fn a_new_profile_is_recommended_and_tightening_applies_with_its_nonce() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    assert_eq!(level(&rt, &ctx), Recommended);
    set(&mut rt, &ctx, "up", Hardened);
    assert_eq!(level(&rt, &ctx), Hardened);
    set(&mut rt, &ctx, "up2", Maximum);
    assert_eq!(level(&rt, &ctx), Maximum);
}

#[test]
fn a_loosening_runs_only_with_its_own_fresh_nonce() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    set(&mut rt, &ctx, "up", Maximum);
    // No nonce, a made-up nonce, and a nonce for a different level change nothing.
    for (n, bogus) in [
        serde_json::json!({}),
        serde_json::json!({ "confirmed": true }),
        serde_json::json!({ "nonce": "00".repeat(16) }),
        serde_json::json!(null),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            change(&mut rt, &ctx, &format!("b{n}"), Recommended, bogus),
            UNAUTHORIZED,
            "{n}"
        );
    }
    let other = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "hardened");
    assert_eq!(
        change(&mut rt, &ctx, "wrong", Recommended, other),
        UNAUTHORIZED
    );
    assert_eq!(level(&rt, &ctx), Maximum);
    set(&mut rt, &ctx, "down", Recommended);
    assert_eq!(level(&rt, &ctx), Recommended);
}

#[test]
fn a_replayed_nonce_is_refused_and_changes_nothing() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    let token = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "hardened");
    assert_eq!(
        change(&mut rt, &ctx, "a", Hardened, token.clone()),
        COMMITTED
    );
    set(&mut rt, &ctx, "back", Recommended);
    assert_eq!(change(&mut rt, &ctx, "b", Hardened, token), UNAUTHORIZED);
    assert_eq!(level(&rt, &ctx), Recommended);
}

#[test]
fn an_expired_nonce_is_refused() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    let token = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "maximum");
    rt.tick(CONFIRMATION_TTL_MS + 1);
    assert_eq!(change(&mut rt, &ctx, "late", Maximum, token), UNAUTHORIZED);
    assert_eq!(level(&rt, &ctx), Recommended);
}

#[test]
fn a_nonce_issued_for_a_tightening_cannot_apply_after_the_level_moved() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    // Issued while Recommended: "hardened" is a tightening.
    let stale = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "hardened");
    set(&mut rt, &ctx, "up", Maximum);
    // Now "hardened" would be a loosening; the old nonce never confirmed that.
    assert_eq!(change(&mut rt, &ctx, "stale", Hardened, stale), CONFLICT);
    assert_eq!(level(&rt, &ctx), Maximum);
    // The nonce was spent by the attempt.
    let again = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "hardened");
    assert_eq!(change(&mut rt, &ctx, "fresh", Hardened, again), COMMITTED);
    assert_eq!(level(&rt, &ctx), Hardened);
}

#[test]
fn choosing_the_level_in_use_changes_nothing_and_an_unknown_level_gets_no_nonce() {
    use PrivacyPolicyLevel::*;
    let (mut rt, ctx) = setup();
    set(&mut rt, &ctx, "same", Recommended);
    assert_eq!(level(&rt, &ctx), Recommended);
    rt.dispatch(
        &ctx,
        "bad",
        IntentBody::IssueConfirmation {
            intent_kind: "ChangePrivacyPolicy".into(),
            target: "paranoid".into(),
        },
    );
    assert_eq!(
        rt.outcome(&ctx, "bad"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
}

#[test]
fn the_level_survives_a_restart() {
    use PrivacyPolicyLevel::*;
    let (mut rt, mut ctx) = setup();
    set(&mut rt, &ctx, "up", Maximum);
    ctx = rt.restart().remove(0);
    assert_eq!(level(&rt, &ctx), Maximum);
    // A nonce issued before the restart still names the rank it was issued against.
    let token = confirmation(&mut rt, &ctx, "ChangePrivacyPolicy", "recommended");
    ctx = rt.restart().remove(0);
    assert_eq!(
        change(&mut rt, &ctx, "after", Recommended, token),
        COMMITTED
    );
    assert_eq!(level(&rt, &ctx), Recommended);
}
