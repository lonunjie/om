// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0199 recovery-record wire vectors (`verification/vectors/recovery-record-vectors.json.gz`:
//! RecoveryRecordEnvelope 35, RecoveryRecord 36, RecoveryRedirect 37, RecoveryKit 38, all W1) through the product codec:
//! verdict, exact error class and byte-identical re-encoding, plus encode-from-value equality. The two prototype codecs
//! and two CDDL tools agree on the same file (recovery-record-wire/run.sh). The product codec also covers what the
//! prototypes cannot: byte strings above their 64 KiB string limit, up to the registered 1 MiB object bounds.

use flate2::read::GzDecoder;
use om_codec::cbor::{Value, encode, from_diag, unhex};
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
    let raw = std::fs::read(repo("verification/vectors/recovery-record-vectors.json.gz"))
        .expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

#[test]
fn recovery_record_corpus_replays_exactly() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(
        vectors.len() >= 190,
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
        accepted >= 35 && rejected >= 150,
        "accepted {accepted} rejected {rejected}"
    );
}

fn uint(n: u64) -> Value {
    Value::Uint(n)
}

fn envelope(ct_len: usize) -> Vec<u8> {
    encode(&Value::Map(vec![
        (uint(0), uint(35)),
        (uint(1), uint(1)),
        (uint(2), uint(1)),
        (uint(3), Value::Bytes(vec![7; 12])),
        (uint(4), Value::Bytes(vec![9; ct_len])),
    ]))
}

/// What the prototypes cannot reach: the envelope bounds above 64 KiB (16,400 to 1,048,016 ciphertext bytes).
#[test]
fn envelope_bounds_above_the_prototype_string_limit() {
    let reg = Registry::load_w1();
    for (len, ok) in [
        (16_399, false),
        (16_400, true),
        (65_552, true),
        (262_160, true),
        (1_048_016, true),
        (1_048_017, false),
    ] {
        let r = reg.check("RecoveryRecordEnvelope", &envelope(len), false);
        assert_eq!(r.is_ok(), ok, "ciphertext length {len}: {r:?}");
    }
}

/// A record plaintext padded to each of the four buckets is accepted; one byte more than the largest is not.
#[test]
fn plaintext_bounds_up_to_the_largest_bucket() {
    let reg = Registry::load_w1();
    let doc = corpus();
    let typical = doc["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "RecoveryRecord/typical")
        .unwrap();
    let base = expand_hex(&typical["hex"]);
    let Value::Map(mut pairs) = om_codec::decode(&base, om_codec::Limits::OM, 1_048_000).unwrap()
    else {
        panic!("map")
    };
    for bucket in [16_384usize, 65_536, 262_144, 1_048_000] {
        // key 22 pad: item = key byte + bstr head + payload; the head is 1, 2, 3 or 5 bytes by payload size
        let l = [1usize, 2, 3, 5]
            .into_iter()
            .find_map(|hdr| {
                let l = bucket - base.len() - 1 - hdr;
                let h = match l {
                    0..=23 => 1,
                    24..=255 => 2,
                    256..=65_535 => 3,
                    _ => 5,
                };
                (h == hdr).then_some(l)
            })
            .expect("a pad length reaches the bucket");
        let mut p = pairs.clone();
        p.push((uint(22), Value::Bytes(vec![0; l])));
        let out = encode(&Value::Map(p));
        assert_eq!(out.len(), bucket);
        assert!(
            reg.check("RecoveryRecord", &out, false).is_ok(),
            "bucket {bucket}"
        );
    }
    pairs.push((uint(22), Value::Bytes(vec![0; 1_048_000])));
    let over = encode(&Value::Map(pairs));
    assert_eq!(reg.check("RecoveryRecord", &over, false), Err("size_limit"));
}
