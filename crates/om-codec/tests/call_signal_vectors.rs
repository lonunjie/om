// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0218 call-signal vectors (`verification/vectors/call-signal-vectors.json.gz`: CallSignal,
//! object type 44, W2, plus the CallKeyPackage struct vectors) through the product codec: verdict, exact error
//! class, byte-identical re-encoding, encode-from-value equality and, for accepted objects, the derived signature
//! inputs must match. The two prototype codecs agree on the same file (call-wire/run.sh).
//! The product codec also covers what the prototypes cannot: byte strings above their 64 KiB string limit.

use flate2::read::GzDecoder;
use om_codec::cbor::{Value, encode, from_diag, unhex};
use om_codec::derive::derived;
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
    let raw = std::fs::read(repo("verification/vectors/call-signal-vectors.json.gz"))
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
fn call_signal_corpus_replays_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 90, "corpus unexpectedly small: {}", vectors.len());
    let mut bad = Vec::new();
    let (mut accepted, mut rejected) = (0, 0);
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let name = v["object"].as_str().unwrap();
        assert_eq!(name, "CallSignal", "{id}");
        let bytes = expand_hex(&v["hex"]);
        match (v["expect"].as_str().unwrap(), reg.check(name, &bytes, false)) {
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
                let got: Vec<J> = derived(&reg, name, &val, &bytes);
                let want = v["derived"].as_array().unwrap();
                let strip = |x: &J| pick(x, &["kind", "hex", "input_hex", "sha256"]);
                let g: Vec<_> = got.iter().map(strip).collect();
                let w: Vec<_> = want.iter().map(strip).collect();
                if g != w {
                    bad.push(format!("{id}: derived signature/hash inputs differ"));
                }
            }
            (want, got) => bad.push(format!("{id}: expected {want}, got {:?}", got.map(|_| "accept"))),
        }
    }
    assert!(bad.is_empty(), "{} mismatches, first: {:#?}", bad.len(), &bad[..bad.len().min(10)]);
    assert!(accepted >= 35 && rejected >= 55, "accepted {accepted} rejected {rejected}");
}

/// CallKeyPackage is not a top-level object; the product codec checks it as a registered struct.
#[test]
fn key_package_structs_replay_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let structs = doc["structs"].as_array().unwrap();
    assert!(structs.len() >= 10, "only {} struct vectors", structs.len());
    let mut bad = Vec::new();
    for v in structs {
        let id = v["id"].as_str().unwrap();
        let name = v["struct"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        match (v["expect"].as_str().unwrap(), reg.check_struct(name, &bytes, 32_768)) {
            ("reject", Err(e)) => {
                if Some(e) != v["error"].as_str() {
                    bad.push(format!("{id}: error {e} != {}", v["error"]));
                }
            }
            ("accept", Ok(val)) => {
                if encode(&val) != bytes {
                    bad.push(format!("{id}: re-encode differs"));
                }
            }
            (want, got) => bad.push(format!("{id}: expected {want}, got {:?}", got.map(|_| "accept"))),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The signal is at most 32,768 bytes, proven at the exact edge on the product codec (the prototype codecs cap every
/// string at 64 KiB). An object over it is refused before any field is read.
#[test]
fn signal_size_boundary_on_the_product_codec() {
    let reg = Registry::load_w1();
    let build = |pad: usize| {
        encode(&from_diag(&serde_json::json!({"$map": [
            [0, 44], [1, 1], [2, 4], [3, {"$bytes": "11".repeat(16)}], [4, 0], [8, 6],
            [23, {"$map": [["org.example.padding", {"$map": [[0, false], [1, {"$bytes": "00".repeat(pad)}]]}]]}]
        ]})))
    };
    let mut pad = 32_768 - build(0).len();
    while build(pad).len() > 32_768 {
        pad -= 1;
    }
    while build(pad + 1).len() <= 32_768 {
        pad += 1;
    }
    assert_eq!(build(pad).len(), 32_768);
    assert!(reg.check("CallSignal", &build(pad), false).is_ok());
    assert_eq!(
        reg.check("CallSignal", &build(pad + 1), false).map(|_: Value| ()),
        Err("size_limit")
    );
}

/// No time field travels: every unassigned key is `unknown_field`, and key 23 is the only extension point. Keys 10 to 15
/// are held for group calls and are refused here.
#[test]
fn signal_refuses_every_unassigned_key() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let min = doc["vectors"].as_array().unwrap().iter().find(|v| v["id"] == "CallSignal/end-hangup").unwrap();
    let bytes = expand_hex(&min["hex"]);
    let val = reg.check("CallSignal", &bytes, false).unwrap();
    for key in (10u64..=22).chain(24..=40) {
        let Value::Map(mut m) = val.clone() else { panic!("map") };
        m.push((Value::Uint(key), Value::Uint(1_790_208_000)));
        m.sort_by_key(|a| encode(&a.0));
        assert_eq!(
            reg.check("CallSignal", &encode(&Value::Map(m)), false).map(|_: Value| ()),
            Err("unknown_field"),
            "key {key}"
        );
    }
}
