// SPDX-License-Identifier: MPL-2.0
//! `communities_publications` slice 5: V-016 race and ABA cases against product code. A decision alone changes nothing,
//! bans end only by a committed lift, stale reductions never hit a later incarnation.
use om_core::room_moderation::*;
use std::collections::BTreeSet;

fn caps(c: &[&str]) -> BTreeSet<String> {
    c.iter().map(|s| s.to_string()).collect()
}
const MOD: &[u8] = b"mod";
const GOV: &[u8] = b"gov";
const BOB: &[u8] = b"bob";
fn room() -> RoomModeration {
    let mut r = RoomModeration::new();
    r.admit(GOV.to_vec(), [1; 16], true).unwrap();
    r.admit(MOD.to_vec(), [2; 16], false).unwrap();
    r.admit(BOB.to_vec(), [3; 16], false).unwrap();
    r
}
fn all() -> BTreeSet<String> {
    caps(&[REMOVE, BAN, TIMEOUT, RESTRICT, REDACT])
}

#[test]
fn every_action_needs_its_capability_and_never_targets_self() {
    let mut r = room();
    assert_eq!(
        r.remove(&caps(&[]), MOD, BOB, [3; 16], false),
        Err(ModError::NotAuthorized)
    );
    assert_eq!(
        r.ban(&caps(&[REMOVE]), MOD, BOB, [9; 16], false),
        Err(ModError::NotAuthorized)
    );
    assert_eq!(
        r.timeout(&caps(&[BAN]), MOD, BOB, [9; 16], false),
        Err(ModError::NotAuthorized)
    );
    assert_eq!(
        r.redact(&caps(&[]), MOD, BOB, [5; 32]),
        Err(ModError::NotAuthorized)
    );
    assert_eq!(
        r.ban(&all(), MOD, MOD, [9; 16], false),
        Err(ModError::SelfAction)
    );
    assert_eq!(
        r.redact(&all(), MOD, MOD, [5; 32]),
        Err(ModError::SelfAction)
    );
}

#[test]
fn a_moderator_cannot_touch_governance_without_governance_authority() {
    let mut r = room();
    assert_eq!(
        r.remove(&all(), MOD, GOV, [1; 16], false),
        Err(ModError::GovernanceProtected)
    );
    assert_eq!(
        r.ban(&all(), MOD, GOV, [9; 16], false),
        Err(ModError::GovernanceProtected)
    );
    assert_eq!(
        r.remove(&all(), MOD, GOV, [1; 16], true),
        Ok(Effect::Applied)
    );
}

#[test]
fn a_ban_removes_blocks_admission_and_ends_only_by_a_committed_lift() {
    let mut r = room();
    r.ban(&all(), MOD, BOB, [9; 16], false).unwrap();
    assert!(r.is_banned(BOB) && !r.is_member(BOB));
    assert_eq!(r.admit(BOB.to_vec(), [4; 16], false), Err(ModError::Banned));
    // nothing but a lift ends it: re-banning is a duplicate, a self-lift is refused
    assert_eq!(
        r.ban(&all(), MOD, BOB, [8; 16], false),
        Err(ModError::Duplicate)
    );
    assert_eq!(
        r.unban(&all(), BOB, BOB, [9; 16]),
        Err(ModError::SelfAction)
    );
    assert!(r.is_banned(BOB));
    assert_eq!(r.unban(&all(), MOD, BOB, [9; 16]), Ok(Effect::Applied));
    r.admit(BOB.to_vec(), [4; 16], false).unwrap();
    assert!(r.is_member(BOB));
}

#[test]
fn a_stale_lift_or_remove_never_hits_a_later_incarnation() {
    let mut r = room();
    r.ban(&all(), MOD, BOB, [9; 16], false).unwrap();
    r.unban(&all(), MOD, BOB, [9; 16]).unwrap();
    r.ban(&all(), MOD, BOB, [10; 16], false).unwrap();
    // a late, rebased unban of the first ban targets a dead instance: no-op, the new ban stands
    assert_eq!(r.unban(&all(), MOD, BOB, [9; 16]), Ok(Effect::NoOp));
    assert!(r.is_banned(BOB));
    assert_eq!(r.ban_incarnation(BOB), Some([10; 16]));
    // a stale remove of an old member incarnation leaves the re-admitted member alone
    r.unban(&all(), MOD, BOB, [10; 16]).unwrap();
    r.admit(BOB.to_vec(), [4; 16], false).unwrap();
    assert_eq!(r.remove(&all(), MOD, BOB, [3; 16], false), Ok(Effect::NoOp));
    assert!(r.is_member(BOB));
    assert_eq!(
        r.remove(&all(), MOD, BOB, [4; 16], false),
        Ok(Effect::Applied)
    );
}

#[test]
fn timeouts_and_restrictions_end_only_by_lift() {
    let mut r = room();
    r.timeout(&all(), MOD, BOB, [6; 16], false).unwrap();
    r.restrict(&all(), MOD, BOB, 1, [7; 16], false).unwrap();
    assert!(r.is_timed_out(BOB) && r.is_restricted(BOB, 1) && !r.is_restricted(BOB, 2));
    assert_eq!(
        r.timeout(&all(), MOD, BOB, [8; 16], false),
        Err(ModError::Duplicate)
    );
    assert_eq!(r.untimeout(&all(), MOD, BOB, [0; 16]), Ok(Effect::NoOp));
    assert!(r.is_timed_out(BOB));
    assert_eq!(r.untimeout(&all(), MOD, BOB, [6; 16]), Ok(Effect::Applied));
    assert_eq!(r.lift(&all(), MOD, BOB, 1, [7; 16]), Ok(Effect::Applied));
    assert!(!r.is_timed_out(BOB) && !r.is_restricted(BOB, 1));
    assert_eq!(
        r.timeout(&all(), MOD, b"nobody", [1; 16], false),
        Err(ModError::UnknownMember)
    );
}

#[test]
fn a_decision_alone_changes_nothing_and_its_public_form_drops_evidence() {
    let r = room();
    let mut log = DecisionLog::default();
    let d = Decision {
        decision_id: [1; 16],
        actor: MOD.to_vec(),
        action: "ban".into(),
        subject: BOB.to_vec(),
        target_event: None,
        reason_code: Some("spam".into()),
        starts_at: Some(1),
        expires_at: Some(2),
        reporter: Some(b"reporter".to_vec()),
        evidence_commitment: Some([5; 32]),
    };
    log.record(d.clone());
    assert_eq!(log.all().len(), 1);
    assert!(
        !r.is_banned(BOB) && r.is_member(BOB),
        "a record never changes Room state"
    );
    let p = d.public_view();
    assert_eq!(p.reason_code.as_deref(), Some("spam"));
    let shown = format!("{p:?}");
    assert!(!shown.contains("reporter") && !shown.contains("evidence"));
}

#[test]
fn redact_suppresses_another_principals_message() {
    let mut r = room();
    r.redact(&all(), MOD, BOB, [5; 32]).unwrap();
    assert!(r.is_redacted(&[5; 32]) && !r.is_redacted(&[6; 32]));
}
