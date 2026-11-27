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

fn session() -> impl FnMut(Value) -> Value {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    move |v| serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
}

#[test]
fn lan_frames_round_trip_through_the_bridge_and_hostile_ones_are_refused() {
    let mut c = session();
    let t = "07".repeat(16);
    let e = c(
        json!({"cmd":"lan_frame_encode","frame":{"kind":"chunk","transferId":t,"offset":16384,"bytes":"ab".repeat(100)}}),
    );
    let hex = e["ok"]["frame"].as_str().unwrap().to_string();
    let d = c(json!({"cmd":"lan_frame_decode","bytes":hex}));
    assert_eq!(
        (d["ok"]["kind"].clone(), d["ok"]["offset"].clone()),
        (json!("chunk"), json!(16384))
    );
    let big = c(
        json!({"cmd":"lan_frame_encode","frame":{"kind":"chunk","transferId":t,"offset":0,"bytes":"ab".repeat(16385)}}),
    );
    assert_eq!(big["ok"], json!({"refused":"value_range"}));
    assert!(c(json!({"cmd":"lan_frame_decode","bytes":"a0"}))["ok"]["refused"].is_string());
    assert_eq!(
        c(json!({"cmd":"lan_frame_decode","bytes":"zz"}))["ok"],
        json!({"refused":"malformed"})
    );
}

#[test]
fn lan_fetch_requests_round_trip_through_the_bridge_and_a_bad_capability_size_is_refused() {
    let mut c = session();
    let r = "09".repeat(16);
    let e = c(
        json!({"cmd":"lan_frame_encode","frame":{"kind":"fetch_blob","requestId":r,"capability":"aa".repeat(24),"blobId":"bb".repeat(32),"ciphertextDigest":"cc".repeat(32)}}),
    );
    let hex = e["ok"]["frame"].as_str().unwrap().to_string();
    let d = c(json!({"cmd":"lan_frame_decode","bytes":hex}));
    assert_eq!(d["ok"]["kind"], "fetch_blob");
    assert_eq!(d["ok"]["blobId"], "bb".repeat(32));
    let k = c(
        json!({"cmd":"lan_frame_encode","frame":{"kind":"fetch_async_key","requestId":r,"deviceId":"0a".repeat(16),"profile":1,"capability":"dd".repeat(32)}}),
    );
    let d = c(json!({"cmd":"lan_frame_decode","bytes":k["ok"]["frame"]}));
    assert_eq!(
        (d["ok"]["kind"].clone(), d["ok"]["profile"].clone()),
        (json!("fetch_async_key"), json!(1))
    );
    // A capability outside 16..64 bytes is refused by the codec on the way back in.
    let bad = c(
        json!({"cmd":"lan_frame_encode","frame":{"kind":"fetch_blob","requestId":r,"capability":"aa".repeat(8),"blobId":"bb".repeat(32),"ciphertextDigest":"cc".repeat(32)}}),
    );
    assert!(bad["ok"]["refused"].is_string());
}
