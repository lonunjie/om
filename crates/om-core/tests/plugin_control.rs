// SPDX-License-Identifier: MPL-2.0
//! `plugins` slice 4: the control, release and grant engine against the ADR-0266 corpus
//! (`verification/prototypes/plugins-wire/vectors.json`): the chain, rollback, fork, tombstone, ceiling, manifest and
//! grant vectors that the codec replay leaves to this engine.
use om_codec::Value;
use om_codec::cbor::encode;
use om_core::plugin_control::*;
use serde_json::Value as J;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/plugins-wire/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn map(mut m: Vec<(Value, Value)>) -> Value {
    m.sort_by_key(|(k, _)| encode(k));
    m.sort_by_key(|(k, _)| encode(k).len());
    Value::Map(m)
}

/// The prototypes' JSON conventions: `{"$bytes": hex}`, arrays, and a plain object is a map with unsigned-integer keys.
fn to_value(x: &J) -> Value {
    match x {
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => Value::Uint(n.as_u64().unwrap_or_else(|| panic!("number {n}"))),
        J::String(s) => Value::Text(s.clone()),
        J::Array(a) => Value::Array(a.iter().map(to_value).collect()),
        J::Object(o) if o.len() == 1 && o.contains_key("$bytes") => {
            Value::Bytes(om_codec::cbor::unhex(o["$bytes"].as_str().unwrap()))
        }
        J::Object(o) => map(o
            .iter()
            .map(|(k, v)| (Value::Uint(k.parse().expect("integer key")), to_value(v)))
            .collect()),
        J::Null => panic!("null"),
    }
}

fn body(v: &J) -> Vec<u8> {
    encode(&to_value(v))
}

/// The body inside a signed envelope, taken through the product registry.
fn inner(object: &str, wire: &[u8]) -> Vec<u8> {
    let v = om_codec::Registry::load_w4()
        .check(object, wire, false)
        .unwrap_or_else(|e| panic!("{object}: {e}"));
    match v.get(1) {
        Some(Value::Bytes(b)) => b.clone(),
        _ => panic!("no body"),
    }
}

fn positive(id: &str) -> Vec<u8> {
    let c = corpus();
    let v = c["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("{id}"))
        .clone();
    inner(
        v["object"].as_str().unwrap(),
        &om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap()),
    )
}

fn negatives(object: &str) -> Vec<J> {
    corpus()["negative"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["object"] == object)
        .cloned()
        .collect()
}

/// The negative's own body, or `None` when the product codec already refuses the envelope.
fn neg_body(v: &J) -> Option<Vec<u8>> {
    let wire = om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap());
    let r = om_codec::Registry::load_w4().check(v["object"].as_str().unwrap(), &wire, false);
    match r.ok()?.get(1) {
        Some(Value::Bytes(b)) => Some(b.clone()),
        _ => None,
    }
}

fn approvers(st: &PluginState) -> Vec<Vec<u8>> {
    st.keys.keys().cloned().collect()
}

fn chain_of(prior: PluginState) -> PluginChain {
    let anchor = prior.hash;
    PluginChain::from_verified_head(prior, anchor)
}

#[test]
fn the_honest_chain_genesis_then_a_request_beyond_the_ceiling_then_a_revocation() {
    let g = PluginState::from_body(&positive("P02-control-genesis")).unwrap();
    let mut chain = PluginChain::new(g.plugin_id, g.hash);
    assert_eq!(
        chain.accept(g.clone(), &approvers(&g)),
        Ok(ControlOutcome::Accepted)
    );
    let up =
        PluginState::from_body(&positive("P15-control-add-new-cap-request-but-disabled")).unwrap();
    assert_eq!(
        chain.accept(up.clone(), &approvers(&up)),
        Ok(ControlOutcome::Accepted)
    );
    let rel2 = &up.releases[&2];
    assert!(
        rel2.requested.contains(&2) && !rel2.ceiling.contains(&2),
        "the new capability is requested, not carried"
    );
    let rv = PluginState::from_body(&positive("P19-publisher-revokes-release")).unwrap();
    assert_eq!(
        chain.accept(rv.clone(), &approvers(&rv)),
        Ok(ControlOutcome::Accepted)
    );
    assert!(chain.release_revoked(1));
    assert!(!chain.release_revoked(2));
    assert_eq!(chain.head().unwrap().version, 3);
}

#[test]
fn a_genesis_that_is_not_the_pinned_anchor_is_refused() {
    let g = PluginState::from_body(&positive("P02-control-genesis")).unwrap();
    let mut chain = PluginChain::new(g.plugin_id, [0u8; 32]);
    assert_eq!(
        chain.accept(g.clone(), &approvers(&g)),
        Err(ControlError::MissingPrev)
    );
}

#[test]
fn every_stateful_control_negative_is_refused_by_the_engine() {
    let (mut checked, mut refused_by_codec) = (0, 0);
    for v in negatives("PluginControlState") {
        let class = v["error_class"].as_str().unwrap();
        if !["stale_chain", "forked_chain", "capability_escalation"].contains(&class) {
            continue;
        }
        let id = v["id"].as_str().unwrap();
        let Some(nb) = neg_body(&v) else {
            refused_by_codec += 1;
            continue;
        };
        let next = PluginState::from_body(&nb).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        let mut chain = match v.get("prior_value") {
            Some(p) => chain_of(PluginState::from_body(&body(p)).unwrap()),
            // no prior: the vector is an invalid genesis on its own
            None => PluginChain::new(next.plugin_id, next.hash),
        };
        let who = match v.get("prior_value") {
            Some(p) => approvers(&PluginState::from_body(&body(p)).unwrap()),
            None => approvers(&next),
        };
        let got = chain.accept(next, &who);
        assert!(
            matches!(got, Err(_) | Ok(ControlOutcome::Forked)),
            "{id} ({class}) was accepted: {got:?}"
        );
        checked += 1;
    }
    assert!(
        checked + refused_by_codec >= 11,
        "{checked} {refused_by_codec}"
    );
}

#[test]
fn a_revoked_release_cannot_come_back_and_a_revoking_successor_is_accepted_even_after_a_fork() {
    let g = PluginState::from_body(&positive("P02-control-genesis")).unwrap();
    let up =
        PluginState::from_body(&positive("P15-control-add-new-cap-request-but-disabled")).unwrap();
    let rv = PluginState::from_body(&positive("P19-publisher-revokes-release")).unwrap();
    let mut chain = PluginChain::new(g.plugin_id, g.hash);
    chain.accept(g.clone(), &approvers(&g)).unwrap();
    chain.accept(up.clone(), &approvers(&up)).unwrap();
    chain.accept(rv.clone(), &approvers(&rv)).unwrap();
    // an attempt to offer the earlier (active) shape again is a rollback
    assert_eq!(
        chain.accept(up, &approvers(&g)),
        Err(ControlError::Rollback)
    );
    assert!(chain.release_revoked(1));
}

#[test]
fn update_decisions_refuse_rollback_unless_pinned_and_refuse_revoked_releases() {
    let up =
        PluginState::from_body(&positive("P15-control-add-new-cap-request-but-disabled")).unwrap();
    let carry = update_decision(&up, 2, 1, None, false).unwrap();
    assert_eq!(
        carry
            .disabled_until_granted
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(!carry.ceiling.contains(&2));
    assert_eq!(
        update_decision(&up, 1, 2, None, false),
        Err(UpdateError::Rollback)
    );
    assert!(
        update_decision(&up, 1, 2, Some(1), false).is_ok(),
        "an explicit local pin permits it"
    );
    assert_eq!(
        update_decision(&up, 1, 2, Some(1), true),
        Err(UpdateError::Revoked)
    );
    assert_eq!(
        update_decision(&up, 9, 2, None, false),
        Err(UpdateError::UnknownRelease)
    );
}

#[test]
fn a_manifest_is_authorized_only_by_the_release_that_lists_its_digest() {
    let up =
        PluginState::from_body(&positive("P15-control-add-new-cap-request-but-disabled")).unwrap();
    let m1 = Manifest::from_body(&positive("P01-manifest-genesis")).unwrap();
    let m2 = Manifest::from_body(&positive("P14-manifest-update-request-extra")).unwrap();
    assert!(
        m1.matches_release(&up),
        "release 1 is listed with this manifest's digest"
    );
    assert!(m2.matches_release(&up), "release 2 likewise");
    let g = PluginState::from_body(&positive("P02-control-genesis")).unwrap();
    assert!(
        !m2.matches_release(&g),
        "a manifest the control state does not list is not authorized"
    );
    for v in negatives("PluginPackageManifest") {
        let class = v["error_class"].as_str().unwrap();
        let id = v["id"].as_str().unwrap();
        let m = neg_body(&v).map(|b| Manifest::from_body(&b));
        if let (Some(Ok(m)), Some(c)) = (m, v.get("control_value")) {
            let head = PluginState::from_body(&body(c)).unwrap();
            assert!(!m.matches_release(&head), "{id} ({class})");
        }
    }
}

fn grant(id: &str) -> Grant {
    Grant::from_body(&positive(id)).unwrap()
}

#[test]
fn grants_chain_by_incarnation_revocation_is_terminal_and_expiry_only_reduces() {
    let g1 = grant("P03-device-grant");
    let g2 = grant("P18-device-revokes-grant");
    let mut t = GrantTable::new();
    assert_eq!(
        t.accept(g1.clone(), false),
        Err(GrantError::IssuerNotAuthorized)
    );
    t.accept(g1.clone(), true).unwrap();
    assert!(t.live(&g1.incarnation, 50));
    assert!(!t.live(&g1.incarnation, 100_000), "expired");
    assert!(!t.live(&g1.incarnation, 5), "not yet issued");
    t.accept(g2.clone(), true).unwrap();
    assert!(!t.live(&g1.incarnation, 50), "revoked");
    // the first version cannot be replayed over the revoked head, and nothing revives it
    assert_eq!(t.accept(g1, true), Err(GrantError::Resurrection));
}

#[test]
fn every_stateful_grant_negative_is_refused_or_not_live() {
    let mut checked = 0;
    for v in negatives("PluginCapabilityGrant") {
        let class = v["error_class"].as_str().unwrap();
        let id = v["id"].as_str().unwrap();
        if !["stale_chain", "forked_chain", "grant_expired"].contains(&class) {
            continue;
        }
        let Some(gb) = neg_body(&v) else {
            continue;
        };
        let g = Grant::from_body(&gb).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        let mut t = GrantTable::new();
        if let Some(p) = v.get("prior_value") {
            t.resume(Grant::from_body(&body(p)).unwrap_or_else(|e| panic!("{id} prior: {e:?}")));
            assert!(t.accept(g, true).is_err(), "{id} ({class}) was accepted");
        } else {
            let now = v["now"].as_u64().unwrap();
            t.accept(g.clone(), true).unwrap();
            assert!(!t.live(&g.incarnation, now), "{id} still live at {now}");
        }
        checked += 1;
    }
    assert!(checked >= 5, "{checked}"); // N40, N45, N46, N47, N102
}

#[test]
fn a_capability_beyond_the_ceiling_needs_a_grant_for_that_release_while_older_grants_carry_forward()
{
    let up =
        PluginState::from_body(&positive("P15-control-add-new-cap-request-but-disabled")).unwrap();
    let scope = (1u64, vec![0x35u8; 16]);
    let mut t = GrantTable::new();
    // P03 grants capability 1 for release 1: inside release 2's ceiling, so it carries forward.
    t.accept(grant("P03-device-grant"), true).unwrap();
    assert!(capability_enabled(&t, &up, 2, 1, &scope, 50));
    // capability 2 is requested by release 2 but is outside its ceiling: disabled until the fresh grant for release 2
    assert!(!capability_enabled(&t, &up, 2, 2, &scope, 50));
    t.accept(grant("P16-device-approves-extra-capability"), true)
        .unwrap();
    assert!(capability_enabled(&t, &up, 2, 2, &scope, 50));
    // a capability the release never requested is never enabled
    assert!(!capability_enabled(&t, &up, 2, 9, &scope, 50));
}
