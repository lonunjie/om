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

#[test]
fn route_offers_cross_the_bridge_validated() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"bridge-routes"),
        || 1_790_000_000_000,
    );
    call(&mut b, json!({ "cmd": "init", "home": home() }));
    let ok = json!({ "mailbox": "07".repeat(16), "relayUri": "https://home.test/", "capability": "09".repeat(32) });
    let bad = json!({ "mailbox": "07".repeat(15), "relayUri": "https://home.test/", "capability": "09".repeat(32) });
    let legacy = json!({ "mailbox": "08".repeat(16), "relayUri": "om-core:mailbox/v0", "capability": "09".repeat(32) });
    assert_eq!(
        call(
            &mut b,
            json!({ "cmd": "supply_routes", "routes": [ok, bad, legacy] })
        ),
        json!({ "ok": 1 })
    );
    assert!(
        call(
            &mut b,
            json!({ "cmd": "supply_routes", "routes": [{ "mailbox": "zz" }] })
        )["err"]
            .is_object()
    );
}

#[test]
fn backup_commands_round_trip_and_name_each_refusal() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"backup"),
        || 1_790_000_000_000,
    );
    call(&mut b, json!({ "cmd": "init", "home": home() }));
    let ctx = call(
        &mut b,
        json!({ "cmd": "open_profile", "profileId": "me", "records": [] }),
    )["ok"]
        .clone();
    let key = "42".repeat(32);
    let plan = call(&mut b, json!({ "cmd": "backup_plan" }))["ok"].clone();
    assert_eq!(plan["kdf"]["p"], 1);
    assert_eq!(plan["salt"].as_str().unwrap().len(), 32);
    let file = call(
        &mut b,
        json!({ "cmd": "backup_export", "ctx": ctx, "plan": plan, "key": key }),
    )["ok"]
        .as_str()
        .unwrap()
        .to_string();
    let head = call(&mut b, json!({ "cmd": "backup_inspect", "file": file }));
    assert_eq!(head["ok"]["version"], 1);
    assert_eq!(head["ok"]["salt"], plan["salt"]);

    let wrong = call(
        &mut b,
        json!({ "cmd": "backup_import", "ctx": ctx, "file": file, "key": "43".repeat(32) }),
    );
    assert_eq!(wrong["err"]["code"], "backup_wrong_password");
    let junk = call(
        &mut b,
        json!({ "cmd": "backup_inspect", "file": "00".repeat(200) }),
    );
    assert_eq!(junk["err"]["code"], "backup_not_a_backup");
    let cut = call(
        &mut b,
        json!({ "cmd": "backup_inspect", "file": &file[..100] }),
    );
    assert_eq!(cut["err"]["code"], "backup_truncated");
    let ok = call(
        &mut b,
        json!({ "cmd": "backup_import", "ctx": ctx, "file": file, "key": key }),
    );
    assert_eq!(ok["ok"]["summary"]["messagesAdded"], 0);
    assert_eq!(
        call(
            &mut b,
            json!({ "cmd": "backup_export", "ctx": ctx, "plan": plan, "key": "00" })
        )["err"]["code"],
        "invalid"
    );
}

/// The host makes a returned commit durable (here: reports success) before the next call.
fn finish(b: &mut Bridge<InsecureTestRng>, r: &Value) {
    if let Some(id) = r["ok"]["commit"]["id"].as_u64() {
        call(
            b,
            json!({ "cmd": "finish_commit", "commitId": id, "ok": true }),
        );
    }
}

#[test]
fn attachments_are_prepared_refused_and_opened_through_the_bridge() {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"bridge-att"),
        || 1_790_000_000_000,
    );
    call(&mut b, json!({ "cmd": "init", "home": home() }));
    let ctx = call(
        &mut b,
        json!({ "cmd": "open_profile", "profileId": "me", "records": [] }),
    )["ok"]
        .clone();
    let meta = json!({ "mediaType": "image/png" });
    // No advertised store: a typed refusal, nothing else.
    let r = call(
        &mut b,
        json!({ "cmd": "prepare_attachment", "ctx": ctx, "plaintext": "0102", "meta": meta }),
    );
    finish(&mut b, &r);
    assert_eq!(r["ok"]["refused"], json!({ "error": "no_blob_store" }));
    call(
        &mut b,
        json!({ "cmd": "set_blob_limits", "limits": { "maxBlobBytes": 1024 } }),
    );
    // Over the advertised limit: refused with the largest size, before anything is sealed.
    let big = "00".repeat(2000);
    let r = call(
        &mut b,
        json!({ "cmd": "prepare_attachment", "ctx": ctx, "plaintext": big, "meta": meta }),
    );
    finish(&mut b, &r);
    assert_eq!(
        r["ok"]["refused"],
        json!({ "error": "too_large", "max": 1008 }),
        "{}",
        r.to_string().chars().take(300).collect::<String>()
    );
    // Within it: sealed, with a commit to make durable and ciphertext 16 bytes longer.
    let r = call(
        &mut b,
        json!({ "cmd": "prepare_attachment", "ctx": ctx, "plaintext": "00".repeat(100), "meta": meta }),
    );
    finish(&mut b, &r);
    let prepared = &r["ok"]["prepared"];
    assert_eq!(prepared["ciphertextLen"], 116);
    assert!(
        r["ok"]["commit"]["writes"]
            .as_array()
            .is_some_and(|w| !w.is_empty())
    );
    // Core can reproduce the same ciphertext from the plaintext, and refuses other plaintext.
    let again = call(
        &mut b,
        json!({ "cmd": "restage_attachment", "ctx": ctx, "blobId": prepared["blobId"], "plaintext": "00".repeat(100) }),
    );
    assert_eq!(again["ok"]["ciphertext"], prepared["ciphertext"]);
    let wrong = call(
        &mut b,
        json!({ "cmd": "restage_attachment", "ctx": ctx, "blobId": prepared["blobId"], "plaintext": "01".repeat(100) }),
    );
    assert_eq!(
        wrong["ok"]["refused"],
        json!({ "error": "integrity_failure" })
    );
    // A message that does not exist has no attachment to fetch or open.
    let f = call(
        &mut b,
        json!({ "cmd": "attachment_fetch", "ctx": ctx, "messageId": "msg-x", "index": 0 }),
    );
    assert_eq!(f["ok"]["refused"], json!({ "error": "not_found" }));
}
