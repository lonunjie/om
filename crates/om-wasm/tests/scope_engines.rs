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

fn session() -> impl FnMut(Value) -> Value {
    let mut b = Bridge::new(
        || InsecureTestRng::from_label(b"scope"),
        || 1_790_000_000_000,
    );
    move |v| serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
}

fn head(version: u64, hash: &str, state: u64) -> Value {
    json!({"serviceId": "61".repeat(32), "version": version, "hash": hash.repeat(32),
           "endpoints": [{"id": "10".repeat(16), "state": state}]})
}

#[test]
fn a_room_service_keeps_its_state_under_a_handle() {
    let mut c = session();
    let svc = "61".repeat(32);
    let ep = "10".repeat(16);
    let ok = |v: &Value| v["ok"].clone();
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"open","quorum":2,"sequencingTimeout":300}})
        )),
        json!({"done": true})
    );
    let humans = json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}]);
    let install = |caps: Value, ap: &Value| json!({"cmd":"room_services_do","room":"r1","caps":caps,"op":{"kind":"install","head":head(1,"01",1),"incarnation":"05","filterClasses":[1],"approvers":ap}});
    assert_eq!(
        ok(&c(install(json!([]), &humans))),
        json!({"refused": "not_authorized"})
    );
    let one = json!([{"kind":1,"id":"01".repeat(32)},{"kind":2,"id":"09".repeat(32)}]);
    assert_eq!(
        ok(&c(install(json!(["room.install_service"]), &one))),
        json!({"refused": "quorum_not_met"})
    );
    assert_eq!(
        ok(&c(install(json!(["room.install_service"]), &humans))),
        json!({"done": true})
    );
    let v = ok(&c(
        json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"view","service":svc}}),
    ));
    assert_eq!(
        (
            v["installed"].clone(),
            v["canReadAllContent"].clone(),
            v["filterClasses"].clone()
        ),
        (json!(true), json!(true), json!([1]))
    );
    // A head that revokes the endpoint stops it; a second Room handle is untouched; rollback is refused.
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"observe_head","head":head(2,"02",3),"now":10}})
        )),
        json!({"done": true})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"must_stop","service":svc,"endpoint":ep}})
        )),
        json!({"mustStopSending": true})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"observe_head","head":head(1,"01",1),"now":11}})
        )),
        json!({"refused": "rollback"})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"may_commit_removal","service":svc,"endpoint":ep,"isCoordinator":false,"now":20}})
        )),
        json!({"refused": "too_early"})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"may_commit_removal","service":svc,"endpoint":ep,"isCoordinator":false,"now":310}})
        )),
        json!({"done": true})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"removal_committed","service":svc,"endpoint":ep}})
        )),
        json!({"applied": true})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"room_services_do","room":"zz","caps":[],"op":{"kind":"view","service":svc}})
        )),
        json!({"refused": "unknown_handle"})
    );
}

#[test]
fn a_bridge_room_decides_every_export() {
    let mut c = session();
    let b = "61".repeat(32);
    let ep = "10".repeat(16);
    let ok = |v: &Value| v["ok"].clone();
    c(
        json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"open","quorum":2,"sequencingTimeout":300,"freshnessBound":600}}),
    );
    let mapping = json!({"incarnation":"05".repeat(16),"namespace":"test.net","direction":3,"allowedClasses":[1,2]});
    let admit = |caps: Value, ap: Value| json!({"cmd":"bridge_room_do","room":"b1","caps":caps,"op":{"kind":"admit","head":head(1,"01",1),"mapping":mapping,"approvers":ap,"now":0}});
    // Another bridge principal (kind 3) must not count toward the human quorum.
    let mixed = json!([{"kind":1,"id":"01".repeat(32)},{"kind":3,"id":b}]);
    assert_eq!(
        ok(&c(admit(json!(["room.add_bridge"]), mixed))),
        json!({"refused": "quorum_not_met"})
    );
    assert_eq!(
        ok(&c(admit(
            json!(["room.install_service"]),
            json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}])
        ))),
        json!({"refused": "not_authorized"})
    );
    assert_eq!(
        ok(&c(admit(
            json!(["room.add_bridge"]),
            json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}])
        ))),
        json!({"done": true})
    );
    let export = |class: u8, now: u64| json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"may_export","bridge":b,"endpoint":ep,"class":class,"now":now}});
    assert_eq!(ok(&c(export(1, 100))), json!({"allowed": true}));
    assert_eq!(ok(&c(export(9, 100)))["blocked"], "class_not_allowed");
    assert_eq!(ok(&c(export(1, 601)))["blocked"], "stale_mapping");
    assert_eq!(
        ok(&c(
            json!({"cmd":"bridge_room_do","room":"b1","caps":["room.add_bridge"],"op":{"kind":"revoke_mapping","bridge":b}})
        )),
        json!({"done": true})
    );
    assert_eq!(ok(&c(export(1, 100)))["blocked"], "revoked");
    assert_eq!(
        ok(&c(
            json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"import_author_ok","authorKind":1}})
        )),
        json!({"allowed": false})
    );
    assert_eq!(
        ok(&c(
            json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"import_author_ok","authorKind":3}})
        )),
        json!({"allowed": true})
    );
}

#[test]
fn a_lan_transfer_confirms_only_on_an_authenticated_receipt() {
    let mut c = session();
    let ok = |v: &Value| v["ok"].clone();
    let (blob, dig) = ("aa".repeat(32), "bb".repeat(32));
    c(
        json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"sender_open","blobId":blob,"digest":dig,"total":40000}}),
    );
    c(
        json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"receiver_open","total":40000,"restoredOffset":null}}),
    );
    let mut off = 0u64;
    loop {
        let r = ok(&c(
            json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"next_chunk"}}),
        ));
        if r["chunk"].is_null() {
            break;
        }
        let len = r["chunk"]["len"].as_u64().unwrap();
        assert_eq!(r["chunk"]["offset"], off);
        let a = ok(&c(
            json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"accept","offset":off,"len":len}}),
        ));
        off += len;
        assert_eq!(a["acknowledged"], off);
    }
    assert_eq!(off, 40000);
    c(
        json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"transport_ack","upTo":40000}}),
    );
    let v = ok(&c(
        json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"view"}}),
    ));
    assert_eq!(v["confirmed"], false);
    let rc = |auth: bool, d: &str| json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"receipt","authenticated":auth,"blobId":blob,"digest":d,"length":40000}});
    assert_eq!(
        ok(&c(rc(false, &dig))),
        json!({"refused": "receipt_not_authenticated"})
    );
    assert_eq!(
        ok(&c(rc(true, &"cc".repeat(32)))),
        json!({"refused": "receipt_mismatch"})
    );
    assert_eq!(ok(&c(rc(true, &dig))), json!({"done": true}));
    let bad = ok(&c(
        json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"accept","offset":1,"len":5}}),
    ));
    assert!(bad["refused"] == "bad_chunk" || bad["refused"] == "overrun");
}

#[test]
fn a_space_binding_needs_an_official_link_and_dies_with_the_unlink() {
    let mut c = session();
    let ok = |v: &Value| v["ok"].clone();
    let create = |caps: Value| json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"create_binding","spaceId":"aa".repeat(32),"governance":[{"kind":1,"id":"01".repeat(32)}],"capabilities":caps,"freshnessBound":60,"acceptsSubdelegation":false}});
    assert_eq!(
        ok(&c(create(json!(["room.remove_member"])))),
        json!({"refused": "unknown_handle"})
    );
    c(json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"propose"}}));
    assert_eq!(
        ok(&c(create(json!(["room.remove_member"])))),
        json!({"refused": "link_not_official"})
    );
    let v = ok(&c(
        json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"child_accepts"}}),
    ));
    assert_eq!(v["official"], true);
    assert_eq!(
        ok(&c(create(json!(["room.manage_governance"])))),
        json!({"refused": "binding_protected"})
    );
    assert_eq!(
        ok(&c(create(json!([])))),
        json!({"refused": "empty_capabilities"})
    );
    assert_eq!(
        ok(&c(create(json!(["room.remove_member"])))),
        json!({"done": true})
    );
    let v = ok(&c(
        json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"unlink"}}),
    ));
    assert_eq!(
        (v["official"].clone(), v["bound"].clone()),
        (json!(false), json!(false))
    );
    // Re-accepting does not restore the binding.
    let v = ok(&c(
        json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"child_accepts"}}),
    ));
    assert_eq!(v["bound"], false);
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
