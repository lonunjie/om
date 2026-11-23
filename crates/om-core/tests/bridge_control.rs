// SPDX-License-Identifier: MPL-2.0
//! `bridges` slice 1: bridge control (hash chain, no self-bootstrap, endpoint rotation, rollback) and typed authorship
//! against the ADR-0264 vectors. The chain engine is the service one; these tests pin that a bridge state decodes into it
//! under the bridge hash label and that a bridge-authored event cannot be mistaken for an account's or a service's.
use om_codec::cbor::unhex;
use om_codec::{Registry, Value};
use om_core::bridge_control::*;
use om_core::service_control::*;
use std::collections::BTreeSet;

const BRIDGE: [u8; 32] = [0x33; 32];
const ENDPOINT: [u8; 16] = [0x55; 16];

fn doc() -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/bridges-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn vector(id: &str) -> (String, Vec<u8>) {
    let d = doc();
    let v = d["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"));
    (
        v["object"].as_str().unwrap().to_string(),
        unhex(v["wire_hex"].as_str().unwrap()),
    )
}
fn state(id: &str) -> BridgeState {
    let (obj, wire) = vector(id);
    let Some(Value::Bytes(body)) = Registry::load_w4()
        .check(&obj, &wire, false)
        .unwrap()
        .get(1)
        .cloned()
    else {
        panic!("no envelope body")
    };
    state_from_body(&body).unwrap()
}
fn controller() -> Principal {
    (1, vec![0x11; 32])
}
fn event() -> Vec<u8> {
    vector("ok-import-with-head-and-mapping").1
}
fn chain() -> BridgeChain {
    let mut c = BridgeChain::new(BRIDGE);
    assert_eq!(
        c.accept(state("ok-bridgecontrolstate"), &[controller()]),
        Ok(ControlOutcome::Accepted)
    );
    c
}

#[test]
fn bridge_states_decode_hash_chain_and_advance() {
    let g = state("ok-bridgecontrolstate");
    assert_eq!(g.service_id, BRIDGE);
    assert_eq!(g.version, 1);
    assert!(g.prev.is_none());
    let mut c = chain();
    // version two names the genesis hash computed under the bridge label (a second codec produced the vector)
    let v2 = state("ok-control-version-two");
    assert_eq!(v2.prev, Some(g.hash));
    assert_eq!(c.accept(v2, &[controller()]), Ok(ControlOutcome::Accepted));
    assert_eq!(c.head().unwrap().version, 2);
}

#[test]
fn a_bridge_cannot_bootstrap_or_advance_its_own_control() {
    let mut c = BridgeChain::new(BRIDGE);
    // the bridge itself, or its endpoint, approving genesis is self-bootstrap
    for approver in [(3, BRIDGE.to_vec()), (3, ENDPOINT.to_vec())] {
        assert_eq!(
            c.accept(state("ok-bridgecontrolstate"), &[controller(), approver]),
            Err(ControlError::SelfBootstrap)
        );
    }
    // no approval at threshold
    assert_eq!(
        c.accept(state("ok-bridgecontrolstate"), &[]),
        Err(ControlError::Threshold)
    );
    let mut c = chain();
    assert_eq!(
        c.accept(state("ok-control-version-two"), &[(1, vec![0x99; 32])]),
        Err(ControlError::Threshold)
    );
}

#[test]
fn rollback_and_unknown_predecessor_are_refused() {
    let mut c = chain();
    assert_eq!(
        c.accept(state("ok-control-version-two"), &[controller()]),
        Ok(ControlOutcome::Accepted)
    );
    assert_eq!(
        c.accept(state("ok-bridgecontrolstate"), &[controller()]),
        Err(ControlError::Rollback)
    );
    let mut empty = BridgeChain::new(BRIDGE);
    assert_eq!(
        empty.accept(state("ok-control-version-two"), &[controller()]),
        Err(ControlError::UnknownPrev)
    );
}

#[test]
fn endpoint_rotation_is_atomic_and_a_revoked_endpoint_stays_dead() {
    let mut c = chain();
    let g = c.head().unwrap().clone();
    let new = Endpoint {
        id: vec![0x56; 16],
        state: ACTIVE,
        profile: 1,
        key: vec![0x52; 40],
        mode: 0,
        host_ref: None,
    };
    let mut next = g.migrate_endpoint(&ENDPOINT, new).unwrap();
    // the caller encodes the new state, which gives it its own hash
    next.hash = [0xEE; 32];
    assert_eq!(
        c.accept(next, &[controller()]),
        Ok(ControlOutcome::Accepted)
    );
    assert!(c.is_revoked(&ENDPOINT));
    assert!(!c.endpoint_usable(&ENDPOINT));
    assert!(c.endpoint_usable(&[0x56; 16]));
    // the old event can no longer be accepted as sent by the revoked endpoint
    assert_eq!(
        validate_bridge_event(&event(), &ENDPOINT, &c),
        Err(BridgeAuthorError::EndpointNotUsable)
    );
}

#[test]
fn a_bridge_event_needs_an_active_endpoint_and_is_always_bridged() {
    let c = chain();
    let a = validate_bridge_event(&event(), &ENDPOINT, &c).unwrap();
    assert_eq!(a.bridge, BRIDGE);
    assert!(a.bridged());
    // an unknown endpoint, and a chain for another BridgeID
    assert_eq!(
        validate_bridge_event(&event(), &[0x77; 16], &c),
        Err(BridgeAuthorError::EndpointNotUsable)
    );
    let mut other = BridgeChain::new([0x34; 32]);
    let mut g = state("ok-bridgecontrolstate");
    g.service_id = [0x34; 32];
    other.accept(g, &[controller()]).unwrap();
    assert_eq!(
        validate_bridge_event(&event(), &ENDPOINT, &other),
        Err(BridgeAuthorError::EndpointNotUsable)
    );
}

#[test]
fn a_native_or_unprovenanced_event_is_not_a_bridge_event() {
    let c = chain();
    // an account-authored native event
    assert_eq!(
        validate_bridge_event(&vector("ok-w0-native-event").1, &ENDPOINT, &c),
        Err(BridgeAuthorError::NotBridgeAuthor)
    );
    // a bridge-authored event without provenance is refused by the codec before authorship is considered
    let d = doc();
    let neg = d["negative"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "bad-bridge-no-provenance")
        .unwrap();
    assert_eq!(
        validate_bridge_event(&unhex(neg["wire_hex"].as_str().unwrap()), &ENDPOINT, &c),
        Err(BridgeAuthorError::Malformed)
    );
}

#[test]
fn bridges_and_services_never_count_toward_a_human_quorum() {
    let endpoints: BTreeSet<Vec<u8>> = [ENDPOINT.to_vec()].into();
    let approvers = vec![
        (1, vec![1; 32]),
        (3, BRIDGE.to_vec()),
        (2, vec![9; 32]),
        (1, ENDPOINT.to_vec()),
        (1, vec![2; 32]),
    ];
    assert_eq!(
        human_quorum(&approvers, &endpoints),
        vec![(1, vec![1; 32]), (1, vec![2; 32])]
    );
}
