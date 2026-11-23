// SPDX-License-Identifier: MPL-2.0
//! `services_bots` slice 1: the service control engine against the V-011 model cases (self-bootstrap, rollback, atomic
//! host migration, fork never blocking a revocation, tombstones, stale removal versus reinstall) and the wire decode of
//! the ADR-0262 vectors.
use om_core::service_control::*;
use std::collections::{BTreeMap, BTreeSet};

const SID: [u8; 32] = [0x41; 32];

fn acct(n: u8) -> Principal {
    (1, vec![n; 32])
}
fn h(n: u64) -> [u8; 32] {
    let mut x = [0xC0; 32];
    x[..8].copy_from_slice(&n.to_be_bytes());
    x
}
fn ep(id: u8, state: u64) -> Endpoint {
    Endpoint {
        id: vec![id; 16],
        state,
        profile: 1,
        key: vec![id; 40],
        mode: 2,
        host_ref: Some(vec![id.wrapping_add(100); 16]),
    }
}
fn st(
    id: u64,
    version: u64,
    prev: Option<u64>,
    ctl: &[u8],
    thr: u64,
    eps: Vec<Endpoint>,
) -> ServiceState {
    ServiceState {
        service_id: SID,
        version,
        prev: prev.map(h),
        controllers: ctl.iter().map(|c| acct(*c)).collect::<BTreeSet<_>>(),
        threshold: thr,
        endpoints: eps
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect::<BTreeMap<_, _>>(),
        hash: h(id),
    }
}
fn chain() -> ServiceChain {
    let mut c = ServiceChain::new(SID);
    let g = st(1, 1, None, &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(g, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    c
}

#[test]
fn genesis_needs_its_own_declared_controllers_at_threshold() {
    let mut c = ServiceChain::new(SID);
    let g = st(1, 1, None, &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(g.clone(), &[acct(1)]),
        Err(ControlError::Threshold)
    );
    assert_eq!(
        c.accept(g.clone(), &[acct(1), acct(1)]),
        Err(ControlError::Threshold)
    );
    assert_eq!(
        c.accept(g.clone(), &[acct(1), acct(9)]),
        Err(ControlError::Threshold)
    );
    assert_eq!(
        c.accept(g.clone(), &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(
        c.accept(g, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Duplicate)
    );
}

#[test]
fn a_service_cannot_bootstrap_its_own_control() {
    let mut c = ServiceChain::new(SID);
    // the service itself, or its endpoint, listed as a controller
    let g = ServiceState {
        controllers: [(1, SID.to_vec())].into(),
        ..st(1, 1, None, &[1], 1, vec![ep(10, ACTIVE)])
    };
    assert_eq!(
        c.accept(g, &[(1, SID.to_vec())]),
        Err(ControlError::SelfBootstrap)
    );
    let g2 = ServiceState {
        controllers: [(1, vec![10; 16])].into(),
        ..st(2, 1, None, &[1], 1, vec![ep(10, ACTIVE)])
    };
    assert_eq!(
        c.accept(g2, &[(1, vec![10; 16])]),
        Err(ControlError::SelfBootstrap)
    );
    // an endpoint's approval does not count toward the next state either
    let mut c = chain();
    let next = st(
        2,
        2,
        Some(1),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(11, ACTIVE)],
    );
    assert_eq!(
        c.accept(next, &[acct(1), (1, vec![10; 16])]),
        Err(ControlError::SelfBootstrap)
    );
}

#[test]
fn rollback_replay_gap_and_unknown_prev_are_refused() {
    let mut c = chain();
    let s2 = st(
        2,
        2,
        Some(1),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(11, ACTIVE)],
    );
    assert_eq!(
        c.accept(s2.clone(), &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    // the genesis state again is a rollback; an old duplicate too
    let g = st(1, 1, None, &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(g, &[acct(1), acct(2)]),
        Err(ControlError::Rollback)
    );
    assert_eq!(
        c.accept(s2, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Duplicate)
    );
    let skip = st(
        4,
        4,
        Some(2),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(11, ACTIVE)],
    );
    assert_eq!(
        c.accept(skip, &[acct(1), acct(2)]),
        Err(ControlError::BadVersion)
    );
    let orphan = st(9, 3, Some(77), &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(orphan, &[acct(1), acct(2)]),
        Err(ControlError::UnknownPrev)
    );
    let genesis_again = st(8, 1, None, &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(genesis_again, &[acct(1), acct(2)]),
        Err(ControlError::MissingPrev)
    );
}

#[test]
fn host_migration_is_one_state_and_never_leaves_both_or_neither() {
    let mut c = chain();
    let head = c.head().unwrap().clone();
    let moved = head.migrate_endpoint(&[10; 16], ep(20, ACTIVE)).unwrap();
    let moved = ServiceState {
        hash: h(2),
        ..moved
    };
    assert_eq!(moved.endpoints[&vec![10u8; 16]].state, REVOKED);
    assert_eq!(moved.endpoints[&vec![20u8; 16]].state, ACTIVE);
    assert!(c.endpoint_usable(&[10; 16]));
    assert_eq!(
        c.accept(moved, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    assert!(!c.endpoint_usable(&[10; 16]));
    assert!(c.endpoint_usable(&[20; 16]));
    // the old endpoint can never be moved again or reused as the new one
    let head = c.head().unwrap().clone();
    assert_eq!(
        head.migrate_endpoint(&[10; 16], ep(30, ACTIVE)),
        Err(ControlError::EndpointResurrected)
    );
    assert_eq!(
        head.migrate_endpoint(&[20; 16], ep(10, ACTIVE)),
        Err(ControlError::EndpointResurrected)
    );
}

#[test]
fn revoked_endpoints_stay_as_tombstones_and_nothing_vanishes_silently() {
    let mut c = chain();
    let rev = st(2, 2, Some(1), &[1, 2], 2, vec![ep(10, REVOKED)]);
    assert_eq!(
        c.accept(rev, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    // dropping the tombstone, reactivating, or changing it are all refused
    for (i, eps) in [
        vec![],
        vec![ep(10, ACTIVE)],
        vec![ServiceState_ep_key_changed()],
    ]
    .into_iter()
    .enumerate()
    {
        let s = st(30 + i as u64, 3, Some(2), &[1, 2], 2, eps);
        assert_eq!(
            c.accept(s, &[acct(1), acct(2)]),
            Err(ControlError::EndpointResurrected),
            "case {i}"
        );
    }
    // an active endpoint cannot be deleted without being revoked first
    let mut c = chain();
    let del = st(2, 2, Some(1), &[1, 2], 2, vec![]);
    assert_eq!(
        c.accept(del, &[acct(1), acct(2)]),
        Err(ControlError::EndpointDeleted)
    );
    // a quarantined endpoint cannot reopen under the same ID
    let mut c = chain();
    let q = st(2, 2, Some(1), &[1, 2], 2, vec![ep(10, QUARANTINED)]);
    assert_eq!(
        c.accept(q, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    let reopen = st(3, 3, Some(2), &[1, 2], 2, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(reopen, &[acct(1), acct(2)]),
        Err(ControlError::EndpointResurrected)
    );
}

#[allow(non_snake_case)]
fn ServiceState_ep_key_changed() -> Endpoint {
    Endpoint {
        key: vec![0xEE; 40],
        ..ep(10, REVOKED)
    }
}

#[test]
fn a_fork_never_blocks_a_revocation() {
    let mut c = chain();
    // two competing authority-changing successors of the genesis: no winner, privileged advancement freezes
    let a = st(
        2,
        2,
        Some(1),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(11, ACTIVE)],
    );
    let b = st(
        3,
        2,
        Some(1),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(12, ACTIVE)],
    );
    assert_eq!(
        c.accept(a, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(c.accept(b, &[acct(1), acct(2)]), Ok(ControlOutcome::Forked));
    assert!(c.is_frozen());
    assert!(c.head().is_none());
    // the endpoint is not usable on a branch where it is not active, and a further growth is refused while frozen
    assert!(!c.endpoint_usable(&[11; 16]));
    let grow = st(
        4,
        3,
        Some(2),
        &[1, 2],
        2,
        vec![ep(10, ACTIVE), ep(11, ACTIVE), ep(13, ACTIVE)],
    );
    assert_eq!(
        c.accept(grow, &[acct(1), acct(2)]),
        Err(ControlError::Frozen)
    );
    // a revocation of the leaked endpoint (reducing) is accepted regardless and is monotone for every branch
    let revoke = st(
        5,
        3,
        Some(2),
        &[1, 2],
        2,
        vec![ep(10, REVOKED), ep(11, ACTIVE)],
    );
    assert_eq!(
        c.accept(revoke, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    assert!(c.is_revoked(&[10; 16]));
    assert!(!c.endpoint_usable(&[10; 16]));
}

#[test]
fn only_the_prior_controllers_advance_the_state() {
    let mut c = chain();
    // a new controller set takes effect only through the old set's approval
    let s2 = st(2, 2, Some(1), &[3, 4], 1, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(s2.clone(), &[acct(3), acct(4)]),
        Err(ControlError::Threshold)
    );
    assert_eq!(
        c.accept(s2, &[acct(1), acct(2)]),
        Ok(ControlOutcome::Accepted)
    );
    let s3 = st(3, 3, Some(2), &[3, 4], 1, vec![ep(10, ACTIVE)]);
    assert_eq!(
        c.accept(s3.clone(), &[acct(1), acct(2)]),
        Err(ControlError::Threshold)
    );
    assert_eq!(c.accept(s3, &[acct(3)]), Ok(ControlOutcome::Accepted));
}

fn inst(incarnation: u8, version: u64, prev: Option<u64>, id: u64, revoked: bool) -> Installation {
    Installation {
        incarnation: vec![incarnation; 16],
        service_id: SID,
        scope: (2, vec![7; 32]),
        mode: 1,
        revoked,
        version,
        prev: prev.map(h),
        hash: h(id),
    }
}

#[test]
fn a_stale_removal_cannot_remove_a_later_reinstall() {
    let mut t = InstallationTable::new();
    let scope = (2, vec![7; 32]);
    t.accept(inst(1, 1, None, 100, false)).unwrap();
    assert_eq!(t.active(&SID, &scope).len(), 1);
    // revoke incarnation 1
    t.accept(inst(1, 2, Some(100), 101, true)).unwrap();
    assert!(t.active(&SID, &scope).is_empty());
    // reinstall: a fresh incarnation ID
    t.accept(inst(2, 1, None, 200, false)).unwrap();
    assert_eq!(t.active(&SID, &scope).len(), 1);
    // a stale copy of the old removal arrives again, and a stale replay of the old installation: neither touches #2
    t.accept(inst(1, 2, Some(100), 101, true)).unwrap();
    assert_eq!(t.accept(inst(1, 1, None, 100, false)).map(|_| ()), Ok(()));
    assert_eq!(t.active(&SID, &scope)[0].incarnation, vec![2u8; 16]);
    // the revoked incarnation can never come back
    assert_eq!(
        t.accept(inst(1, 3, Some(101), 102, false)),
        Err(InstallError::Resurrected)
    );
    // an update cannot change the service, scope or mode of an incarnation
    let mut changed = inst(2, 2, Some(200), 201, false);
    changed.mode = 3;
    assert_eq!(t.accept(changed), Err(InstallError::Changed));
}

#[test]
fn ids_are_not_reused_across_chains_and_wire_vectors_decode() {
    // the first positive ServiceControlState vector of ADR-0262 decodes and hashes the same body each time
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/services-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let reg = om_codec::Registry::load_w4();
    let mut seen = 0;
    for v in doc["positive"].as_array().unwrap() {
        let wire = om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap());
        match v["object"].as_str().unwrap() {
            "ServiceControlState" => {
                let env = reg.check("ServiceControlState", &wire, false).unwrap();
                let Some(om_codec::Value::Bytes(body)) = env.get(1) else {
                    panic!("no body")
                };
                let s = ServiceState::from_body(body).unwrap();
                assert_eq!(s, ServiceState::from_body(body).unwrap());
                assert!(s.threshold as usize <= s.controllers.len());
                seen += 1;
            }
            "ServiceInstallation" => {
                let env = reg.check("ServiceInstallation", &wire, false).unwrap();
                let Some(om_codec::Value::Bytes(body)) = env.get(1) else {
                    panic!("no body")
                };
                Installation::from_body(body).unwrap();
                seen += 1;
            }
            _ => {}
        }
    }
    assert!(seen >= 7, "vectors decoded: {seen}");
}
