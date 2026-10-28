// SPDX-License-Identifier: MPL-2.0
//! The JSON bridge end to end on the native target (the wasm build runs the same code; see packages/om-core-wasm).
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_wasm::bridge::Bridge;
use serde_json::{Value, json};

fn call(b: &mut Bridge<InsecureTestRng>, v: Value) -> Value {
    serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
}

fn home() -> Value {
    json!({ "tier": "T1", "observers": ["host_sees_ciphertext"], "migrationAvailable": true, "clientOriginChoice": ["default_origin"] })
}

#[test]
fn json_commands_drive_the_core() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"bridge"),
        || 1_790_000_000_000,
    );
    assert_eq!(
        call(&mut b, json!({ "cmd": "init", "home": home() })),
        json!({ "ok": null })
    );
    let ctx = call(
        &mut b,
        json!({ "cmd": "open_profile", "profileId": "me", "records": [] }),
    )["ok"]
        .clone();
    assert_eq!(ctx["profileId"], "me");
    let step = call(
        &mut b,
        json!({ "cmd": "dispatch", "intent": { "ctx": ctx, "operationId": "op1", "kind": "CreateIdentityProfile", "label": "Me" } }),
    );
    let commit = &step["ok"]["commit"];
    assert!(
        commit["writes"].as_array().is_some_and(|w| w.len() == 1),
        "one atomic record write: {step}"
    );
    assert_eq!(
        step["ok"]["handle"],
        json!({ "operationId": "op1", "profileId": "me" })
    );
    // Nothing is visible before the host commits.
    let out = call(
        &mut b,
        json!({ "cmd": "outcome", "ctx": ctx, "operationId": "op1" }),
    );
    assert_eq!(out["ok"], Value::Null);
    let rel = call(
        &mut b,
        json!({ "cmd": "finish_commit", "commitId": commit["id"], "ok": true }),
    );
    assert_eq!(rel["ok"]["changedOperations"], json!(["op1"]));
    let out = call(
        &mut b,
        json!({ "cmd": "outcome", "ctx": ctx, "operationId": "op1" }),
    );
    assert_eq!(out["ok"], json!({ "kind": "Committed" }));
    let key = call(
        &mut b,
        json!({ "cmd": "query", "query": { "kind": "GetRecoveryKey", "ctx": ctx, "keyOperationId": "op1" } }),
    );
    assert_eq!(key["ok"]["groups"].as_array().map(Vec::len), Some(14));
    let list = call(
        &mut b,
        json!({ "cmd": "query", "query": { "kind": "GetProfileList" } }),
    );
    assert_eq!(list["ok"], json!([{ "id": "me", "label": "Me" }]));
    let ops = call(&mut b, json!({ "cmd": "net_ops", "ctx": ctx }));
    assert_eq!(ops["ok"][0]["call"]["call"], "publish_key_material");
}

#[test]
fn malformed_and_hostile_requests_return_errors_not_panics() {
    let mut b = Bridge::new(|| InsecureTestRng::from_label(b"bridge2"), || 0);
    for bad in [
        &b""[..],
        b"{",
        b"[]",
        b"{\"cmd\":\"nope\"}",
        b"{\"cmd\":\"query\",\"query\":{\"kind\":\"GetProfileList\"}}",
        &[0xff, 0xfe],
    ] {
        let r: Value = serde_json::from_slice(&b.handle(bad)).unwrap();
        assert!(r.get("err").is_some(), "{r}");
    }
    call(&mut b, json!({ "cmd": "init", "home": home() }));
    assert_eq!(
        call(&mut b, json!({ "cmd": "init", "home": home() }))["err"]["code"],
        "invalid",
        "init is once only"
    );
    let stale = json!({ "profileId": "x", "generation": "gen-0" });
    assert_eq!(
        call(&mut b, json!({ "cmd": "net_ops", "ctx": stale }))["err"]["code"],
        "profile_access"
    );
    assert_eq!(
        call(
            &mut b,
            json!({ "cmd": "finish_commit", "commitId": 99, "ok": true })
        )["err"]["code"],
        "invalid"
    );
}
