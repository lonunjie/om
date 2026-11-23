// SPDX-License-Identifier: MPL-2.0
//! The services and bridges typed queries through the JSON bridge. Compiled only with `--features scope-engines`.
#![cfg(feature = "scope-engines")]
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_wasm::bridge::Bridge;
use serde_json::{Value, json};

fn call(v: Value) -> Value {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
}

#[test]
fn fidelity_plans_round_trip_through_json() {
    let r = call(
        json!({"cmd":"bridge_fidelity_plan","semantics":[1],"maxAttachmentBytes":null,"op":{"kind":"reply","targetMapped":true}}),
    );
    assert_eq!(r["ok"]["carry"], "unlinked_message");
    assert_eq!(r["ok"]["flags"], json!([3]));
    let r = call(
        json!({"cmd":"bridge_fidelity_plan","semantics":[1,2,3,4,5,6,7,8,9],"maxAttachmentBytes":1000,"op":{"kind":"edit","sourceTracked":true,"policyAllows":true}}),
    );
    assert_eq!(
        (r["ok"]["carry"].clone(), r["ok"]["flags"].clone()),
        (json!("native"), json!([1]))
    );
    let r = call(
        json!({"cmd":"bridge_fidelity_plan","semantics":[1],"maxAttachmentBytes":null,"op":{"kind":"delete"}}),
    );
    assert_eq!(r["ok"]["claimsErasure"], false);
    assert!(
        call(json!({"cmd":"bridge_fidelity_plan","semantics":[1],"op":{"kind":"nope"}}))["err"]
            .is_object()
    );
}

#[test]
fn broker_requests_follow_grants_taint_and_policy() {
    let ask = |tainted: bool, policy: Value, name: &str| {
        call(json!({"cmd":"service_broker_request","granted":["fetch_url_via_scoped_network_provider"],"tainted":tainted,"policy":policy,"name":name}))["ok"].clone()
    };
    let room_no = json!({"kind":"room","allowed":false});
    assert_eq!(
        ask(
            false,
            room_no.clone(),
            "fetch_url_via_scoped_network_provider"
        )["allowed"],
        true
    );
    assert_eq!(
        ask(
            true,
            room_no.clone(),
            "fetch_url_via_scoped_network_provider"
        )["refusal"],
        "tainted_egress"
    );
    assert_eq!(
        ask(
            true,
            json!({"kind":"one_to_one","me":true,"peer":true}),
            "fetch_url_via_scoped_network_provider"
        )["allowed"],
        true
    );
    assert_eq!(ask(false, room_no.clone(), "shell")["refusal"], "forbidden");
    assert_eq!(
        ask(false, room_no.clone(), "create_draft")["refusal"],
        "not_granted"
    );
    assert_eq!(ask(false, room_no, "rm -rf")["refusal"], "unknown_tool");
}
