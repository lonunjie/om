// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0190 recovery-notice vectors (`verification/vectors/recovery-notice-vectors.json.gz`: RecoveryNotice,
//! object type 34, W1, plus the RecoveryDossierEntry struct vectors) through the product codec: verdict, exact error
//! class, byte-identical re-encoding, encode-from-value equality and, for accepted objects, the derived signature
//! inputs must match. The two prototype codecs agree on the same file (recovery-notice-wire/run.sh).
//! The product codec also covers what the prototypes cannot: byte strings above their 64 KiB string limit.

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
    let raw = std::fs::read(repo("verification/vectors/recovery-notice-vectors.json.gz"))
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
fn recovery_notice_corpus_replays_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(
        vectors.len() >= 100,
        "corpus unexpectedly small: {}",
        vectors.len()
    );
    let mut bad = Vec::new();
    let (mut accepted, mut rejected) = (0, 0);
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let name = v["object"].as_str().unwrap();
        assert_eq!(name, "RecoveryNotice", "{id}");
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
        accepted >= 25 && rejected >= 70,
        "accepted {accepted} rejected {rejected}"
    );
}

/// The RecoveryDossierEntry is not a top-level object; the product codec checks it as a registered struct
/// (`check_struct`, no grammar change), the way the `FrameBody*` structs are checked.
#[test]
fn dossier_entry_structs_replay_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let structs = doc["structs"].as_array().unwrap();
    assert!(structs.len() >= 15, "only {} struct vectors", structs.len());
    let mut bad = Vec::new();
    for v in structs {
        let id = v["id"].as_str().unwrap();
        let name = v["struct"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        match (
            v["expect"].as_str().unwrap(),
            reg.check_struct(name, &bytes, 131_068),
        ) {
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
            (want, got) => bad.push(format!(
                "{id}: expected {want}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// ADR-0190 section 2 step 2: the notice is at most 131,068 bytes (one sealed bucket), proven here at the exact edge
/// (the prototype codecs cap every string at 64 KiB). An object over it is refused before any field is read.
#[test]
fn notice_size_boundary_on_the_product_codec() {
    let reg = Registry::load_w1();
    let policy = "00".repeat(40);
    let req = {
        // any canonical SignedEnvelope of a RecoveryRequest: take it from the corpus minimal vector
        let doc = corpus();
        let v = doc["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == "RecoveryNotice/minimal")
            .unwrap()
            .clone();
        let b = expand_hex(&v["hex"]);
        let val = reg.check("RecoveryNotice", &b, false).unwrap();
        bytes_of(val.get(3)).to_vec()
    };
    let build = |pad: usize| {
        encode(&from_diag(&serde_json::json!({"$map": [
            [0, 34], [1, 1],
            [2, {"$bytes": policy}],
            [3, {"$bytes": req.iter().map(|b| format!("{b:02x}")).collect::<String>()}],
            [23, {"$map": [["org.example.padding", {"$map": [[0, false], [1, {"$bytes": "00".repeat(pad)}]]}]]}]
        ]})))
    };
    // find the pad that makes the object exactly 131,068 bytes
    let mut pad = 131_068 - build(0).len();
    while build(pad).len() > 131_068 {
        pad -= 1;
    }
    while build(pad + 1).len() <= 131_068 {
        pad += 1;
    }
    assert_eq!(build(pad).len(), 131_068);
    assert!(reg.check("RecoveryNotice", &build(pad), false).is_ok());
    assert_eq!(
        reg.check("RecoveryNotice", &build(pad + 1), false)
            .map(|_: Value| ()),
        Err("size_limit")
    );
}

/// A time field never travels: every unassigned key, including the plausible time names, is `unknown_field`, and
/// key 23 is the only extension point (ADR-0190 sections 1 and 6).
#[test]
fn notice_refuses_every_unassigned_key() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let min = doc["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "RecoveryNotice/minimal")
        .unwrap();
    let bytes = expand_hex(&min["hex"]);
    let val = reg.check("RecoveryNotice", &bytes, false).unwrap();
    for key in (8u64..=22).chain(24..=40) {
        let Value::Map(mut m) = val.clone() else {
            panic!("map")
        };
        m.push((Value::Uint(key), Value::Uint(1_790_208_000)));
        m.sort_by_key(|a| encode(&a.0));
        assert_eq!(
            reg.check("RecoveryNotice", &encode(&Value::Map(m)), false)
                .map(|_: Value| ()),
            Err("unknown_field"),
            "key {key}"
        );
    }
}
