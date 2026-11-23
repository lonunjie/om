// SPDX-License-Identifier: MPL-2.0
//! Deterministic mutation sweep over every W4 wire-freeze corpus that has a hex form (services, bridges, plugins).
//! For each positive vector it flips every byte, truncates at every length and appends junk, then runs the product
//! decoder. The decoder must never panic, and anything it still accepts must re-encode to exactly the bytes it was given
//! (no lenient parse). This is a bounded, reproducible sweep, not a replacement for coverage-guided fuzzing.

use om_codec::cbor::unhex;
use om_codec::schema::Registry;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

const CORPORA: &[&str] = &["services-wire", "bridges-wire", "plugins-wire"];

fn positives(dir: &str) -> Vec<(String, String, Vec<u8>)> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes")
        .join(dir)
        .join("vectors.json");
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    j["positive"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| {
            Some((
                v["id"].as_str()?.to_string(),
                v["object"].as_str()?.to_string(),
                unhex(v["wire_hex"].as_str()?),
            ))
        })
        .collect()
}

#[test]
fn mutated_wire_never_panics_and_accepted_bytes_round_trip() {
    let reg = Registry::load_w4();
    let (mut cases, mut accepted) = (0u64, 0u64);
    for dir in CORPORA {
        for (id, object, wire) in positives(dir) {
            let mut variants: Vec<Vec<u8>> = Vec::new();
            for i in 0..wire.len() {
                for x in [0x01u8, 0x80, 0xff] {
                    let mut m = wire.clone();
                    m[i] ^= x;
                    variants.push(m);
                }
                variants.push(wire[..i].to_vec());
            }
            for junk in [&[0u8][..], &[0xff, 0xff][..], &[0xa0][..]] {
                let mut m = wire.clone();
                m.extend_from_slice(junk);
                variants.push(m);
            }
            for v in variants {
                cases += 1;
                let r = catch_unwind(AssertUnwindSafe(|| reg.check(&object, &v, false)));
                let r = r.unwrap_or_else(|_| panic!("decoder panicked on {dir}/{id}"));
                if let Ok(val) = r {
                    accepted += 1;
                    // accepted means canonical: re-encoding the decoded value reproduces the input exactly
                    assert_eq!(
                        om_codec::cbor::encode(&val),
                        v,
                        "{dir}/{id} accepted a non-canonical form"
                    );
                }
            }
        }
    }
    assert!(cases > 10_000, "sweep too small: {cases}");
    eprintln!("mutation sweep: {cases} cases, {accepted} still accepted (all canonical)");
}
