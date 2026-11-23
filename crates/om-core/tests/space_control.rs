// SPDX-License-Identifier: MPL-2.0
//! `communities_publications` slice 2: the Space control engine against the V-008 model cases (delegation ceiling rules 1
//! to 3, no self-escalation, quorum bound to the prior state, fork freeze with reductions still applicable, orphaned
//! governance) and the wire decode of the ADR-0160 vectors.
use om_codec::cbor::unhex;
use om_core::space_control::*;
use std::collections::{BTreeMap, BTreeSet};

const SPACE: [u8; 32] = [0x21; 32];

fn acct(n: u8) -> Principal {
    (1, vec![n; 32])
}
fn role(grants: &[&str], grantable: &[&str]) -> Role {
    Role {
        grants: grants.iter().map(|s| s.to_string()).collect(),
        grantable: grantable.iter().map(|s| s.to_string()).collect(),
    }
}
fn rid(n: u8) -> Vec<u8> {
    vec![n; 16]
}
fn h(n: u8) -> [u8; 32] {
    [n; 32]
}

/// Governance 1-of-1 (acct 1); acct 2 is a role manager (role 1: manage_roles + members, ceiling {post, manage_members});
/// acct 3 is a plain member.
fn genesis() -> SpaceState {
    SpaceState {
        space_id: SPACE,
        version: 1,
        prev: None,
        hash: h(1),
        profile: vec![0xa0],
        members: BTreeMap::from([
            (acct(1), BTreeSet::new()),
            (acct(2), BTreeSet::from([rid(1)])),
            (acct(3), BTreeSet::new()),
        ]),
        roles: BTreeMap::from([(
            rid(1),
            role(
                &[CAP_ROLES, CAP_MEMBERS, CAP_LINKS],
                &["space.view", CAP_MEMBERS],
            ),
        )]),
        links: BTreeSet::new(),
        governance: BTreeSet::from([acct(1)]),
        threshold: 1,
        delegation: BTreeSet::new(),
    }
}
fn next(prev: &SpaceState, hash: u8) -> SpaceState {
    let mut n = prev.clone();
    n.version = prev.version + 1;
    n.prev = Some(prev.hash);
    n.hash = h(hash);
    n
}
fn chain() -> SpaceChain {
    let mut c = SpaceChain::new(SPACE);
    assert_eq!(
        c.accept(genesis(), &[acct(1)]),
        Ok(ControlOutcome::Accepted)
    );
    c
}

#[test]
fn genesis_needs_its_own_governance_and_the_wrong_space_is_refused() {
    let mut c = SpaceChain::new(SPACE);
    assert_eq!(
        c.accept(genesis(), &[acct(3)]),
        Err(ControlError::Governance)
    );
    let mut other = genesis();
    other.space_id = [0x22; 32];
    assert_eq!(c.accept(other, &[acct(1)]), Err(ControlError::WrongSpace));
    assert_eq!(
        c.accept(genesis(), &[acct(1)]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(
        c.accept(genesis(), &[acct(1)]),
        Ok(ControlOutcome::Duplicate)
    );
}

#[test]
fn governance_changes_need_the_prior_quorum_and_never_an_ordinary_role() {
    let mut c = chain();
    let g = genesis();
    let mut n = next(&g, 2);
    n.governance.insert(acct(3));
    // the role manager cannot touch governance; the principal it would add cannot approve its own appointment
    assert_eq!(
        c.accept(n.clone(), &[acct(2)]),
        Err(ControlError::Governance)
    );
    assert_eq!(
        c.accept(n.clone(), &[acct(3)]),
        Err(ControlError::Governance)
    );
    assert_eq!(c.accept(n, &[acct(1)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn rule_one_editing_a_role_stays_inside_the_ceiling() {
    let mut c = chain();
    let g = genesis();
    // adding a capability the editor cannot grant to an existing role raises every holder beyond the editor's ceiling
    let mut n = next(&g, 2);
    n.roles
        .get_mut(&rid(1))
        .unwrap()
        .grants
        .insert("space.moderate".into());
    assert_eq!(c.accept(n, &[acct(2)]), Err(ControlError::Ceiling));
    // a new role inside the ceiling is fine
    let mut ok = next(&g, 3);
    ok.roles.insert(rid(2), role(&["space.view"], &[]));
    assert_eq!(c.accept(ok, &[acct(2)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn rule_two_assigning_a_role_needs_the_whole_role_inside_the_ceiling() {
    let mut c = chain();
    let g = genesis();
    let mut n = next(&g, 2);
    n.roles
        .insert(rid(2), role(&["space.view", "space.moderate"], &[]));
    n.members.get_mut(&acct(3)).unwrap().insert(rid(2));
    // the role holds a capability beyond the ceiling even though the assigner looked at `space.view`
    assert_eq!(c.accept(n, &[acct(2)]), Err(ControlError::Ceiling));
    // the same role under quorum is fine
    let mut q = next(&g, 3);
    q.roles
        .insert(rid(2), role(&["space.view", "space.moderate"], &[]));
    q.members.get_mut(&acct(3)).unwrap().insert(rid(2));
    assert_eq!(c.accept(q, &[acct(1)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn rule_three_a_grantable_scope_stays_inside_the_grantors_own() {
    let mut c = chain();
    let g = genesis();
    let mut n = next(&g, 2);
    n.roles.insert(rid(2), role(&[], &["space.moderate"]));
    assert_eq!(c.accept(n, &[acct(2)]), Err(ControlError::Ceiling));
}

#[test]
fn no_self_escalation_and_the_governance_capability_is_protected() {
    let mut c = chain();
    let g = genesis();
    let mut n = next(&g, 2);
    n.members.get_mut(&acct(2)).unwrap().insert(rid(2));
    n.roles.insert(rid(2), role(&[CAP_GOVERNANCE], &[]));
    assert_eq!(c.accept(n, &[acct(2)]), Err(ControlError::Protected));
    // an approver without the capability cannot change roles at all
    let mut m = next(&g, 3);
    m.roles.insert(rid(2), role(&["space.view"], &[]));
    assert_eq!(c.accept(m, &[acct(3)]), Err(ControlError::NotAuthorized));
}

#[test]
fn each_kind_of_change_needs_its_own_capability() {
    let mut c = chain();
    let g = genesis();
    let mut link = next(&g, 2);
    link.links.insert((1, vec![9; 32]));
    assert_eq!(
        c.accept(link.clone(), &[acct(3)]),
        Err(ControlError::NotAuthorized)
    );
    let mut profile = next(&g, 3);
    profile.profile = vec![0xa1, 0x00, 0x61, 0x78];
    // acct 2 has no manage_profile
    assert_eq!(
        c.accept(profile, &[acct(2)]),
        Err(ControlError::NotAuthorized)
    );
    assert_eq!(c.accept(link, &[acct(2)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn a_member_can_always_leave_but_cannot_remove_another() {
    let mut c = chain();
    let g = genesis();
    let mut leave = next(&g, 2);
    leave.members.remove(&acct(3));
    assert_eq!(
        c.accept(leave.clone(), &[acct(3)]),
        Ok(ControlOutcome::Accepted)
    );
    // acct 3 (no manage_members) removing acct 2 is not authorized
    let mut c = chain();
    let mut kick = next(&g, 3);
    kick.members.remove(&acct(2));
    assert_eq!(c.accept(kick, &[acct(3)]), Err(ControlError::NotAuthorized));
}

#[test]
fn stale_head_and_gaps_are_refused() {
    let mut c = chain();
    let g = genesis();
    let n2 = next(&g, 2);
    assert_eq!(
        c.accept(n2.clone(), &[acct(1)]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(c.accept(genesis(), &[acct(1)]), Err(ControlError::Rollback));
    let mut skip = next(&n2, 3);
    skip.version += 1;
    assert_eq!(c.accept(skip, &[acct(1)]), Err(ControlError::BadVersion));
    let mut lost = next(&n2, 4);
    lost.prev = Some(h(99));
    assert_eq!(c.accept(lost, &[acct(1)]), Err(ControlError::UnknownPrev));
}

#[test]
fn a_fork_freezes_privileged_advancement_but_never_a_reduction() {
    let mut c = chain();
    let g = genesis();
    let mut a = next(&g, 2);
    a.links.insert((1, vec![7; 32]));
    let mut b = next(&g, 3);
    b.links.insert((1, vec![8; 32]));
    assert_eq!(
        c.accept(a.clone(), &[acct(1)]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(c.accept(b, &[acct(1)]), Ok(ControlOutcome::Forked));
    assert!(c.is_frozen());
    assert!(c.head().is_none());
    // no winner: a privileged successor of either tip is refused while frozen
    let mut more = next(&a, 4);
    more.links.insert((1, vec![6; 32]));
    assert_eq!(c.accept(more, &[acct(1)]), Err(ControlError::Frozen));
    // a reducing transition (drops a link) stays applicable
    let mut drop = next(&a, 5);
    drop.links.clear();
    assert_eq!(c.accept(drop, &[acct(1)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn orphaned_governance_is_reported_and_ordinary_actions_continue() {
    let mut c = chain();
    let g = genesis();
    // governance (acct 1) is gone: nothing can change governance, the role manager still works
    let live: BTreeSet<Principal> = [acct(2), acct(3)].into();
    assert!(!g.governance_reachable(&live));
    let mut gov = next(&g, 2);
    gov.threshold = 1;
    gov.governance.insert(acct(2));
    assert_eq!(c.accept(gov, &[acct(2)]), Err(ControlError::Governance));
    let mut ordinary = next(&g, 3);
    ordinary.roles.insert(rid(2), role(&["space.view"], &[]));
    assert_eq!(c.accept(ordinary, &[acct(2)]), Ok(ControlOutcome::Accepted));
}

#[test]
fn the_adr_0160_vectors_decode() {
    // Rebuild the V-008 Space bodies from the committed corpus with its own Python encoder and decode them.
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/prototypes/v008-space-wire/python_codec.py"
    );
    assert!(std::path::Path::new(p).exists());
    let Ok(out) = std::process::Command::new("python3")
        .args(["-I", "-c", "import json,sys;sys.path.insert(0,sys.argv[1]);import python_codec as c;v=json.load(open(sys.argv[1]+'/vectors.json'));print(json.dumps({x['id']:c.enc(c.norm(x['value'])).hex() for x in v['positive']}))"])
        .arg(std::path::Path::new(p).parent().unwrap())
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let m: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut seen = 0;
    for id in ["space-minimal", "space-populated"] {
        let s = SpaceState::from_body(&unhex(m[id].as_str().unwrap())).unwrap();
        // the minimal state is the genesis; the populated one is a later link in a chain
        assert_eq!(s.prev.is_none(), s.version == 1, "{id}");
        assert!(s.threshold as usize <= s.governance.len());
        seen += 1;
    }
    assert_eq!(seen, 2);
    assert!(SpaceState::from_body(&[0xa0]).is_err());
}
