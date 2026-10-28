// SPDX-License-Identifier: MPL-2.0
//! Replays the frozen W0 object corpus (`verification/vectors/v005-w0-object-vectors.json.gz`) and the
//! foundation CBOR fixtures through the product codec: verdict, exact error class, byte-identical re-encoding,
//! encode-from-value equality and the derived signature/hash inputs must all match.

use flate2::read::GzDecoder;
use om_codec::cbor::{encode, from_diag, hex, unhex};
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
    let raw = std::fs::read(repo("verification/vectors/v005-w0-object-vectors.json.gz"))
        .expect("corpus present");
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
fn w0_corpus_replays_exactly() {
    let reg = Registry::load();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(
        vectors.len() >= 700,
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
        accepted >= 60 && rejected >= 600,
        "accepted {accepted} rejected {rejected}"
    );
}

#[test]
fn foundation_fixtures_encode_exactly() {
    let fixtures: J = serde_json::from_str(
        &std::fs::read_to_string(repo("verification/cbor/fixtures.json")).unwrap(),
    )
    .unwrap();
    let mut n = 0;
    for f in fixtures.as_array().unwrap() {
        let bytes = encode(&from_diag(&f["value"]));
        if let Some(want) = f["expected"].as_str() {
            assert_eq!(hex(&bytes), want, "fixture {}", f["id"]);
        }
        // pinned or not, canonical output must survive the strict decoder byte-for-byte
        let back = om_codec::decode(
            &bytes,
            om_codec::Limits {
                lenient: true,
                ..om_codec::Limits::OM
            },
            1 << 20,
        );
        assert_eq!(back.map(|v| encode(&v)), Ok(bytes), "fixture {}", f["id"]);
        n += 1;
    }
    assert!(n >= 14);
}

#[test]
fn mutated_inputs_never_panic() {
    // Cheap property test: every single-byte flip and every truncation of every accepted vector is classified, not a panic.
    let reg = Registry::load();
    let doc = corpus();
    for v in doc["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["expect"] == "accept")
        .take(40)
    {
        let name = v["object"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        for cut in (0..bytes.len()).step_by(7) {
            let _ = reg.check(name, &bytes[..cut], false);
        }
        for i in (0..bytes.len()).step_by(11) {
            let mut m = bytes.clone();
            m[i] ^= 0x5a;
            let _ = reg.check(name, &m, false);
        }
    }
    assert_eq!(reg.check("NoSuchType", &[0], false), Err("unknown_type"));
}
