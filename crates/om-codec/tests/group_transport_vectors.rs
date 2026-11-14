// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0189 group carriage vectors (`verification/vectors/group-transport-vectors.json.gz`:
//! GroupRouteRecord 32, GroupTransportFrame 33 and ConversationEvent kinds 15 to 17) through the product codec under
//! the W1 event-body overlay: verdict, exact error class, byte-identical re-encoding, encode-from-value equality and the
//! derived signature inputs must match. The two prototype codecs agree on the same file (group-transport-wire/run.sh).
//! The product codec also covers the one place the prototypes cannot: byte strings above their 64 KiB string limit.

use flate2::read::GzDecoder;
use om_codec::cbor::{Value, encode, from_diag, unhex};
use om_codec::derive::{bytes_of, derived};
use om_codec::schema::Registry;
use serde_json::Value as J;
use std::io::Read;
use std::path::PathBuf;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn expand_hex(h: &J) -> Vec<u8> {
    match h {
        J::String(s) => unhex(s),
        J::Array(parts) => {
            let mut out = Vec::new();
            for p in parts {
                match p {
                    J::String(s) => out.extend(unhex(s)),
                    J::Object(o) => {
                        let b = unhex(o["fill"].as_str().unwrap())[0];
                        out.extend(std::iter::repeat_n(b, o["len"].as_u64().unwrap() as usize));
                    }
                    _ => panic!("bad hex segment"),
                }
            }
            out
        }
        _ => panic!("bad hex"),
    }
}

fn corpus() -> J {
    let raw = std::fs::read(repo("verification/vectors/group-transport-vectors.json.gz"))
        .expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn pick(x: &J, keys: &[&str]) -> Vec<(String, J)> {
    keys.iter()
        .filter_map(|k| x.get(*k).map(|v| ((*k).to_string(), v.clone())))
        .collect()
}

#[test]
fn group_transport_corpus_replays_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(
        vectors.len() >= 120,
        "corpus unexpectedly small: {}",
        vectors.len()
    );
    let mut bad = Vec::new();
    let (mut accepted, mut rejected) = (0, 0);
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let name = v["object"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        match (
            v["expect"].as_str().unwrap(),
            reg.check(name, &bytes, false),
        ) {
            ("reject", Err(e)) => {
                rejected += 1;
                if Some(e) != v["error"].as_str() {
                    bad.push(format!("{id}: error {e} != {}", v["error"]));
                }
            }
            ("accept", Ok(val)) => {
                accepted += 1;
                if encode(&val) != bytes {
                    bad.push(format!("{id}: re-encode differs"));
                }
                if let Some(dv) = v.get("value")
                    && encode(&from_diag(dv)) != bytes
                {
                    bad.push(format!("{id}: value does not encode to the bytes"));
                }
                if let Some(bv) = v.get("body_value")
                    && encode(&from_diag(bv)) != bytes_of(val.get(1))
                {
                    bad.push(format!("{id}: body_value does not encode to the body"));
                }
                let got: Vec<J> = derived(&reg, name, &val, &bytes);
                let want = v["derived"].as_array().unwrap();
                let strip = |x: &J| pick(x, &["kind", "hex", "input_hex", "sha256"]);
                let g: Vec<_> = got.iter().map(strip).collect();
                let w: Vec<_> = want.iter().map(strip).collect();
                if g != w {
                    bad.push(format!("{id}: derived signature/hash inputs differ"));
                }
            }
            (want, got) => bad.push(format!(
                "{id}: expected {want}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches, first: {:#?}",
        bad.len(),
        &bad[..bad.len().min(10)]
    );
    assert!(
        accepted >= 20 && rejected >= 100,
        "accepted {accepted} rejected {rejected}"
    );
}

/// Kinds 15 to 17 are W1 enum values (ADR-0168 F-16): a W0 decoder rejects every event of those kinds, the W1
/// overlay accepts them and types their bodies.
#[test]
fn group_event_kinds_are_w1_only() {
    let w0 = Registry::load();
    let w1 = Registry::load_w1();
    let mut seen = 0;
    for v in corpus()["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        if v["object"] != "ConversationEvent" {
            continue;
        }
        let bytes = expand_hex(&v["hex"]);
        assert_eq!(
            w0.check("ConversationEvent", &bytes, false)
                .map(|_: Value| ()),
            Err("value_range"),
            "{id} must be rejected by a W0 decoder"
        );
        assert_eq!(
            w1.check("ConversationEvent", &bytes, false).is_ok(),
            v["expect"] == "accept",
            "{id}"
        );
        seen += 1;
    }
    assert!(seen >= 35, "only {seen} event vectors");
}

/// ADR-0189 1.2 / 1.6: a frame body is at most 131,000 bytes and the whole frame at most 131,068. The prototype codecs
/// cap every string at 64 KiB, so this boundary is proven here on the product codec only.
#[test]
fn frame_body_boundary_on_the_product_codec() {
    let reg = Registry::load_w1();
    let frame = |kind: u64, body_len: usize| {
        encode(&from_diag(&serde_json::json!({"$map": [
            [0, 33], [1, 1], [2, kind], [3, {"$bytes": "00".repeat(body_len)}]
        ]})))
    };
    assert!(
        reg.check("GroupTransportFrame", &frame(3, 131_000), false)
            .is_ok()
    );
    assert_eq!(
        reg.check("GroupTransportFrame", &frame(3, 131_001), false)
            .map(|_: Value| ()),
        Err("value_range")
    );
    assert_eq!(
        reg.check("GroupTransportFrame", &frame(3, 0), false)
            .map(|_: Value| ()),
        Err("value_range")
    );
}
