// SPDX-License-Identifier: MPL-2.0
//! Replays the V-024 two-codec vectors (`verification/prototypes/v024-lan-wire/vectors.json`, ADR-0158 / `spec/cddl/lan.cddl`)
//! through the product codec: every positive vector must be accepted by the registered struct and re-encode to the exact
//! frozen bytes, every negative vector must be refused. The Python and Node prototype codecs replay the same file
//! (`run.py`), so this is the third decoder on one corpus.

use om_codec::cbor::{Value, encode, hex};
use om_codec::schema::Registry;
use serde_json::Value as J;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/v024-lan-wire/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn to_value(x: &J) -> Value {
    if let Some(n) = x.as_u64() {
        return Value::Uint(n);
    }
    let h = x["hex"]
        .as_str()
        .expect("vector value is an integer or {hex}");
    Value::Bytes(om_codec::cbor::unhex(h))
}

/// The pairs exactly as the prototypes encode them: keys sorted by their canonical encoding (numeric for these small keys).
fn encode_pairs(pairs: &J) -> Vec<u8> {
    let mut m: Vec<(Value, Value)> = pairs
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| (to_value(&kv[0]), to_value(&kv[1])))
        .collect();
    m.sort_by_key(|(k, _)| match k {
        Value::Uint(n) => *n,
        _ => unreachable!(),
    });
    encode(&Value::Map(m))
}

fn struct_name(t: &str) -> &'static str {
    match t {
        "invite" => "LanInviteRendezvousV1",
        "signal" => "LanSignalEnvelopeV1",
        "frame" => "LanFrameV1",
        other => panic!("unknown vector type {other}"),
    }
}

fn check(reg: &Registry, t: &str, bytes: &[u8]) -> Result<Value, &'static str> {
    if t == "frame" {
        reg.check_lan_frame(bytes, 131_068)
    } else {
        reg.check_struct(struct_name(t), bytes, 131_068)
    }
}

#[test]
fn v024_vectors_replay_through_the_product_codec() {
    let reg = Registry::load_w1();
    let doc = corpus();
    assert_eq!(doc["profile"], "LanDeliveryProfile-v1");
    let (pos, neg) = (
        doc["positives"].as_array().unwrap(),
        doc["negatives"].as_array().unwrap(),
    );
    assert!(pos.len() >= 9 && neg.len() >= 8, "corpus shrank");
    let mut bad = Vec::new();
    for v in pos {
        let id = v["id"].as_str().unwrap();
        let bytes = encode_pairs(&v["value"]);
        if hex(&bytes) != v["expected"].as_str().unwrap() {
            bad.push(format!("{id}: encoding differs from the frozen bytes"));
        }
        match check(&reg, v["type"].as_str().unwrap(), &bytes) {
            Ok(val) if encode(&val) == bytes => {}
            Ok(_) => bad.push(format!("{id}: re-encode differs")),
            Err(e) => bad.push(format!("{id}: refused with {e}")),
        }
    }
    for v in neg {
        let id = v["id"].as_str().unwrap();
        let bytes = encode_pairs(&v["value"]);
        if check(&reg, v["type"].as_str().unwrap(), &bytes).is_ok() {
            bad.push(format!("{id}: accepted but must be refused"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The two prototype codecs name `missing`/`unknown` as `fields` and a bad length as `size`; the product codec uses its stable classes.
#[test]
fn frame_shape_errors_are_the_stable_classes() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let class = |id: &str| {
        let v = doc["negatives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        reg.check_lan_frame(&encode_pairs(&v["value"]), 131_068)
            .map(|_| ())
            .unwrap_err()
    };
    assert_eq!(class("frame-missing"), "missing_field");
    assert_eq!(class("frame-unknown"), "unknown_field");
    assert_eq!(class("chunk-empty"), "value_range");
}
