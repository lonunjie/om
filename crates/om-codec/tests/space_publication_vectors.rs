// SPDX-License-Identifier: MPL-2.0
//! Replays the V-008 two-codec vectors (`verification/prototypes/v008-space-wire/vectors.json`, ADR-0160,
//! `spec/cddl/space-publication.cddl`) through the product codec: SpaceControlState (24), SpaceDelegatedIntent (25),
//! PublicationControlState (26), PublicationEvent (27) and the topic and moderation ConversationEvent kinds. Every positive
//! vector must be accepted and re-encode to the identical bytes; every negative vector must be refused. The Python and Node
//! prototype codecs replay the same file, so this is the third decoder on one corpus.

use om_codec::cbor::{Value, encode};
use om_codec::schema::Registry;
use serde_json::Value as J;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/v008-space-wire/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// The prototypes' JSON conventions: `{"$bytes": hex}`, `{"$set": [..]}` (an array), `{"$map": [[k, v], ..]}`, and a
/// plain object is a map with unsigned-integer keys.
fn to_value(x: &J) -> Value {
    match x {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => Value::Uint(n.as_u64().expect("unsigned integer")),
        J::String(s) => Value::Text(s.clone()),
        J::Array(a) => Value::Array(a.iter().map(to_value).collect()),
        J::Object(o) if o.len() == 1 && o.contains_key("$bytes") => {
            Value::Bytes(om_codec::cbor::unhex(o["$bytes"].as_str().unwrap()))
        }
        J::Object(o) if o.len() == 1 && o.contains_key("$set") => {
            Value::Array(o["$set"].as_array().unwrap().iter().map(to_value).collect())
        }
        J::Object(o) if o.len() == 1 && o.contains_key("$map") => map(o["$map"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kv| (to_value(&kv[0]), to_value(&kv[1])))
            .collect()),
        J::Object(o) => map(o
            .iter()
            .map(|(k, v)| (Value::Uint(k.parse().expect("integer key")), to_value(v)))
            .collect()),
    }
}

/// Canonical map order: the prototypes sort by encoded key (shorter first, then bytewise).
fn map(mut m: Vec<(Value, Value)>) -> Value {
    m.sort_by_key(|(k, _)| encode(k));
    m.sort_by_key(|(k, _)| encode(k).len());
    Value::Map(m)
}

/// The prototypes' placeholder signature wrapper: `{1: bstr(body), 2: [{0: 16x0x10, 1: 1, 2: role, 3: 64x0x20}]}`.
fn envelope(body: &[u8], role: u64) -> Vec<u8> {
    let sig = map(vec![
        (Value::Uint(0), Value::Bytes(vec![0x10; 16])),
        (Value::Uint(1), Value::Uint(1)),
        (Value::Uint(2), Value::Uint(role)),
        (Value::Uint(3), Value::Bytes(vec![0x20; 64])),
    ]);
    encode(&map(vec![
        (Value::Uint(1), Value::Bytes(body.to_vec())),
        (Value::Uint(2), Value::Array(vec![sig])),
    ]))
}

fn role_of(object: &str) -> u64 {
    match object {
        "SpaceControlState" | "SpaceDelegatedIntent" | "PublicationControlState" => 3,
        "PublicationEvent" => 7,
        other => panic!("no signing role for {other}"),
    }
}

fn wire_of(v: &J) -> Vec<u8> {
    let body = encode(&to_value(&v["value"]));
    if v["envelope"].as_bool() == Some(true) {
        envelope(&body, role_of(v["object"].as_str().unwrap()))
    } else {
        body
    }
}

fn check(reg: &Registry, v: &J, bytes: &[u8]) -> Result<Value, &'static str> {
    let object = v["object"].as_str().unwrap();
    // Unsigned bodies and signed envelopes both go through the registered top-level type.
    reg.check(object, bytes, false)
}

#[test]
fn v008_vectors_replay_through_the_product_codec() {
    let reg = Registry::load_w3();
    let doc = corpus();
    let (pos, neg) = (
        doc["positive"].as_array().unwrap(),
        doc["negative"].as_array().unwrap(),
    );
    assert!(pos.len() >= 9 && neg.len() >= 10, "corpus shrank");
    let mut bad = Vec::new();
    for v in pos {
        let id = v["id"].as_str().unwrap();
        let wire = wire_of(v);
        match check(&reg, v, &wire) {
            Ok(val) if encode(&val) == wire => {}
            Ok(_) => bad.push(format!("{id}: re-encode differs")),
            Err(e) => bad.push(format!("{id}: refused with {e}")),
        }
    }
    for v in neg {
        let id = v["id"].as_str().unwrap();
        if check(&reg, v, &wire_of(v)).is_ok() {
            bad.push(format!("{id}: accepted but must be refused"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// ADR-0168 F-16: the publication signing role is a W3 value and is valid only on the types that list it.
#[test]
fn the_publication_signing_role_is_scoped_to_publication_events() {
    let doc = corpus();
    let post = doc["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "publication-post")
        .unwrap();
    let wire = wire_of(post);
    // A W0 or W1 decoder does not know role 7 at all.
    assert_eq!(
        Registry::load()
            .check("PublicationEvent", &wire, false)
            .map(|_| ()),
        Err("value_range")
    );
    assert_eq!(
        Registry::load_w1()
            .check("PublicationEvent", &wire, false)
            .map(|_| ()),
        Err("value_range")
    );
    assert!(
        Registry::load_w3()
            .check("PublicationEvent", &wire, false)
            .is_ok()
    );
    // The same role on a Space object is refused even by the W3 decoder: the type does not list it.
    let space = doc["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "space-minimal")
        .unwrap();
    let wrong = envelope(&encode(&to_value(&space["value"])), 7);
    assert_eq!(
        Registry::load_w3()
            .check("SpaceControlState", &wrong, false)
            .map(|_| ()),
        Err("value_range")
    );
}
