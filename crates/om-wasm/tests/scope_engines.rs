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

#[test]
fn topics_run_through_one_bridge_instance() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    let mut go = |v: Value| -> Value {
        serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
    };
    let id = "01".repeat(16);
    let ev = "aa".repeat(32);
    let st = json!({"clock": 1, "event": "bb".repeat(32)});
    let r = go(
        json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"create","id":id,"title":"general","at":st}}),
    );
    assert_eq!(r["ok"]["refused"], "not_authorized");
    let r = go(
        json!({"cmd":"topics_do","caps":["room.create_topic"],"actor":"ann","op":{"kind":"create","id":id,"title":"general","at":st}}),
    );
    assert_eq!(r["ok"]["applied"], true);
    let r = go(
        json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"post","mode":"forum","topLevel":true,"event":ev,"author":"ann","topic":null}}),
    );
    assert_eq!(r["ok"]["refused"], "topic_required");
    let r = go(
        json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"post","mode":"forum","topLevel":true,"event":ev,"author":"ann","topic":id}}),
    );
    assert_eq!(r["ok"]["applied"], true);
    let r = go(
        json!({"cmd":"topics_do","caps":["room.manage_topics"],"actor":"bob","op":{"kind":"move","event":ev,"topic":null,"at":{"clock":5,"event":"cc".repeat(32)}}}),
    );
    assert_eq!(r["ok"]["applied"], true);
    let v = go(json!({"cmd":"topics_view","topic":id,"message":ev}));
    assert_eq!(v["ok"]["topic"]["title"], "general");
    assert_eq!(v["ok"]["placement"], json!({"topic": null}));
    let r = go(
        json!({"cmd":"topics_do","caps":[],"actor":"x","op":{"kind":"create","id":"zz","title":"t","at":st}}),
    );
    assert_eq!(r["ok"]["refused"], "malformed");
}

#[test]
fn moderation_runs_through_the_bridge_and_stale_lifts_are_noops() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    let mut go = |v: Value| -> Value {
        serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
    };
    let inc = |n: u8| format!("{n:02x}").repeat(16);
    assert_eq!(
        go(
            json!({"cmd":"moderation_do","caps":[],"actor":"x","op":{"kind":"admit","who":"bob","incarnation":inc(3),"governance":false}})
        )["ok"]["applied"],
        true
    );
    let ban = |c: &str, t: &str, n: u8| json!({"cmd":"moderation_do","caps":[c],"actor":"mod","op":{"kind":"ban","target":t,"incarnation":inc(n),"governanceAuthority":false}});
    assert_eq!(
        go(ban("room.remove_member", "bob", 9))["ok"]["refused"],
        "not_authorized"
    );
    assert_eq!(go(ban("room.ban_member", "bob", 9))["ok"]["applied"], true);
    assert_eq!(
        go(json!({"cmd":"moderation_view","who":"bob"}))["ok"],
        json!({"member": false, "banned": true, "timedOut": false})
    );
    let unban = |n: u8| json!({"cmd":"moderation_do","caps":["room.ban_member"],"actor":"mod","op":{"kind":"unban","target":"bob","incarnation":inc(n)}});
    assert_eq!(
        go(unban(1))["ok"],
        json!({"applied": false, "noop": true}),
        "a stale incarnation changes nothing"
    );
    assert_eq!(
        go(json!({"cmd":"moderation_view","who":"bob"}))["ok"]["banned"],
        true
    );
    assert_eq!(go(unban(9))["ok"]["applied"], true);
}

#[test]
fn first_contact_never_activates_without_both_bindings() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    let mut go = |v: Value| -> Value {
        serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
    };
    let step = |k: Value| json!({"cmd":"lan_first_contact","step":k});
    assert_eq!(
        go(step(json!({"kind":"ticket","verified":true})))["ok"]["refused"],
        "out_of_order"
    );
    assert_eq!(
        go(step(json!({"kind":"reach"})))["ok"]["state"],
        "reachable"
    );
    assert_eq!(
        go(step(json!({"kind":"ticket","verified":false})))["ok"]["refused"],
        "not_verified"
    );
    for k in [
        json!({"kind":"ticket","verified":true}),
        json!({"kind":"request","valid":true}),
        json!({"kind":"own_binding","verified":true}),
    ] {
        go(step(k));
    }
    let half = go(step(json!({"kind":"state"})));
    assert_eq!(
        (
            half["ok"]["state"].clone(),
            half["ok"]["asyncKeyFetchAllowed"].clone()
        ),
        (json!("half_bound"), json!(false))
    );
    let done = go(step(json!({"kind":"peer_binding","verified":true})));
    assert_eq!(
        (
            done["ok"]["state"].clone(),
            done["ok"]["deviceEligible"].clone()
        ),
        (json!("active"), json!(true))
    );
}

#[test]
fn a_delegated_intent_is_checked_against_the_binding() {
    let r = call(
        json!({"cmd":"space_delegated_intent_check","body":"a0","binding":{"spaceId":"aa".repeat(32),"capabilities":["room.remove_member"],"freshnessBound":60,"acceptsSubdelegation":false},"heldHead":"bb".repeat(32),"heldVersion":1,"heldHeadAge":0,"viaSubdelegation":false}),
    );
    assert_eq!(r["ok"]["refusal"], "malformed");
    let r = call(
        json!({"cmd":"space_delegated_intent_check","body":"a0","binding":{"spaceId":"aa".repeat(32),"capabilities":["room.manage_governance"],"freshnessBound":60,"acceptsSubdelegation":false},"heldHead":"bb".repeat(32),"heldVersion":1,"heldHeadAge":0,"viaSubdelegation":false}),
    );
    assert_eq!(r["ok"]["refused"], "binding_protected");
}
