// SPDX-License-Identifier: MPL-2.0
//! The second-language codec's vectors (`verification/prototypes/lan-frame-py`, written from `lan.cddl` alone) against the
//! Rust codec: every case the Python side accepts must decode here to the same frame and re-encode to the same bytes; every
//! case it refuses must be refused here too. The reason strings are local to each codec and are not compared.
use om_codec::cbor::{Value, unhex};
use om_core::lan_frame::{decode, encode_frame};
use serde_json::Value as J;
use std::path::PathBuf;

fn vectors() -> Vec<J> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/lan-frame-py/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn the_rust_codec_agrees_with_the_independent_python_vectors() {
    let all = vectors();
    let (mut accepted, mut refused) = (0, 0);
    let mut disagreements = Vec::new();
    for v in &all {
        let name = v["name"].as_str().unwrap();
        let bytes = unhex(v["hex"].as_str().unwrap());
        let got = decode(&bytes);
        if v["accept"].as_bool().unwrap() {
            accepted += 1;
            match got {
                Err(e) => disagreements.push(format!("{name}: python accepts, rust refuses ({e})")),
                Ok(f) => {
                    if encode_frame(&f).as_deref() != Some(bytes.as_slice()) {
                        disagreements.push(format!("{name}: re-encoded bytes differ"));
                    }
                }
            }
        } else {
            refused += 1;
            if got.is_ok() {
                disagreements.push(format!("{name}: python refuses, rust accepts"));
            }
        }
    }
    let _ = Value::Uint(0);
    assert!(accepted >= 28 && refused >= 198, "{accepted}/{refused}");
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}
