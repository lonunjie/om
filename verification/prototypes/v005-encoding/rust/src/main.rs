//! Feasibility prototype for V-005. Not product code. Not for production.
//!
//! v005 fixtures FILE          encode the verification/cbor foundation fixtures
//! v005 vectors FILE           decode/validate/re-encode every W0 object vector, print results JSON
//! v005 lenient TYPE FILE...   read CDDL-tool-generated examples (key order not enforced)
//! v005 bench FILE             encode/decode cost of the vectors marked bench=true

mod cbor;
mod schema;

use cbor::{decode, encode, from_diag, hex, unhex, Limits, Value};
use schema::Registry;
use serde_json::{json, Map, Value as J};
use sha2::{Digest, Sha256};
use std::time::Instant;

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
                        out.extend(std::iter::repeat(b).take(o["len"].as_u64().unwrap() as usize));
                    }
                    _ => panic!("bad hex segment"),
                }
            }
            out
        }
        _ => panic!("bad hex"),
    }
}

fn bytes_of(v: Option<&Value>) -> &[u8] {
    match v {
        Some(Value::Bytes(b)) => b,
        _ => panic!("expected bytes"),
    }
}

fn uint_of(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Uint(n)) => *n,
        _ => panic!("expected uint"),
    }
}

fn ds_sig(label: &str, profile: u64, role: u64, signer: &[u8], body: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text(label.into()),
        Value::Uint(profile),
        Value::Uint(role),
        Value::Bytes(signer.to_vec()),
        Value::Bytes(body.to_vec()),
    ]))
}

fn ds_hash(label: &str, body: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![Value::Text(label.into()), Value::Bytes(body.to_vec())]))
}

/// Signature inputs and hashes computed independently from the decoded object.
fn derived(reg: &Registry, name: &str, top: &Value, top_bytes: &[u8]) -> Vec<J> {
    let t = &reg.types[name];
    let mut out = Vec::new();
    let body: Vec<u8> = if let Some(label) = &t.sig_label {
        let body = bytes_of(top.get(1)).to_vec();
        if let Some(Value::Array(sigs)) = top.get(2) {
            for s in sigs {
                let role = uint_of(s.get(2));
                out.push(json!({"kind": "sig_input", "hex": hex(&ds_sig(label, uint_of(s.get(1)), role, bytes_of(s.get(0)), &body))}));
            }
        }
        body
    } else {
        top_bytes.to_vec()
    };
    if let Some(label) = &t.hash_label {
        let hi = ds_hash(label, &body);
        out.push(json!({"kind": "hash", "input_hex": hex(&hi), "sha256": hex(&Sha256::digest(&hi))}));
    }
    if name == "AsyncKeyBundle" {
        let b = decode(&body, Limits::OM, 1 << 20).unwrap();
        let dev = bytes_of(b.get(2));
        let profile = uint_of(b.get(5));
        for k in 7..=11u64 {
            let items: Vec<&Value> = match b.get(k) {
                Some(Value::Array(a)) => a.iter().collect(),
                Some(x) => vec![x],
                None => vec![],
            };
            for it in items {
                if it.get(3).is_none() {
                    continue;
                }
                let kind = uint_of(it.get(1));
                if kind != 3 && kind != 4 {
                    continue;
                }
                let context = Value::Array(vec![
                    Value::Uint(1),
                    b.get(2).unwrap().clone(),
                    b.get(5).unwrap().clone(),
                    b.get(6).unwrap().clone(),
                    b.get(12).unwrap().clone(),
                    Value::Uint(kind),
                    it.get(0).unwrap().clone(),
                    it.get(2).unwrap().clone(),
                ]);
                out.push(json!({"kind": "sig_input", "hex": hex(&ds_sig("OM-PREKEY-SIG-v1", profile, 3, dev, &encode(&context)))}));
            }
        }
    }
    out
}

fn cmd_fixtures(path: &str) {
    let f: J = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut out = Map::new();
    for x in f.as_array().unwrap() {
        out.insert(x["id"].as_str().unwrap().into(), J::String(hex(&encode(&from_diag(&x["value"])))));
    }
    println!("{}", J::Object(out));
}

fn cmd_vectors(path: &str) {
    let reg = Registry::load();
    let doc: J = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut out = Map::new();
    for v in doc["vectors"].as_array().unwrap() {
        let id = v["id"].as_str().unwrap();
        let name = v["object"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        let r = match reg.check(name, &bytes, false) {
            Err(e) => json!({"verdict": "reject", "error": e}),
            Ok(val) => {
                let mut o = Map::new();
                o.insert("verdict".into(), json!("accept"));
                o.insert("error".into(), J::Null);
                o.insert("reencode_equal".into(), json!(encode(&val) == bytes));
                if let Some(dv) = v.get("value") {
                    o.insert("value_encode_equal".into(), json!(encode(&from_diag(dv)) == bytes));
                }
                if let Some(bv) = v.get("body_value") {
                    o.insert("body_value_encode_equal".into(), json!(encode(&from_diag(bv)) == bytes_of(val.get(1))));
                }
                o.insert("derived".into(), J::Array(derived(&reg, name, &val, &bytes)));
                J::Object(o)
            }
        };
        out.insert(id.into(), r);
    }
    println!("{}", J::Object(out));
}

fn cmd_lenient(name: &str, files: &[String]) {
    let reg = Registry::load();
    let mut out = Map::new();
    for f in files {
        let b = std::fs::read(f).unwrap();
        let r = match reg.check(name, &b, true) {
            Ok(_) => json!("accept"),
            Err(e) => json!(e),
        };
        out.insert(std::path::Path::new(f).file_name().unwrap().to_string_lossy().into(), r);
    }
    println!("{}", J::Object(out));
}

fn time_ns<F: FnMut()>(mut f: F) -> f64 {
    // calibrate to ~15 ms per batch, take the median of 7 batches
    let mut n = 1u64;
    loop {
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        if t.elapsed().as_millis() >= 15 || n >= 1 << 24 {
            break;
        }
        n *= 2;
    }
    let mut samples: Vec<f64> = (0..7)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..n {
                f();
            }
            t.elapsed().as_nanos() as f64 / n as f64
        })
        .collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[3]
}

fn cmd_bench(path: &str) {
    let reg = Registry::load();
    let doc: J = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut rows = Vec::new();
    for v in doc["vectors"].as_array().unwrap() {
        if v["bench"] != J::Bool(true) {
            continue;
        }
        let name = v["object"].as_str().unwrap();
        let bytes = expand_hex(&v["hex"]);
        let val = reg.check(name, &bytes, false).expect("bench vector must be valid");
        let max = reg.types[name].max_bytes;
        let decode_ns = time_ns(|| {
            std::hint::black_box(decode(std::hint::black_box(&bytes), Limits::OM, max).unwrap());
        });
        let validate_ns = time_ns(|| {
            std::hint::black_box(reg.check(name, std::hint::black_box(&bytes), false).unwrap());
        });
        let encode_ns = time_ns(|| {
            std::hint::black_box(encode(std::hint::black_box(&val)));
        });
        let derive_ns = time_ns(|| {
            std::hint::black_box(derived(&reg, name, &val, &bytes));
        });
        rows.push(json!({
            "id": v["id"], "bytes": bytes.len(),
            "decode_ns": decode_ns.round(), "decode_validate_ns": validate_ns.round(),
            "encode_ns": encode_ns.round(), "sig_hash_inputs_ns": derive_ns.round()
        }));
    }
    println!("{}", J::Array(rows));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("fixtures") => cmd_fixtures(&a[2]),
        Some("vectors") => cmd_vectors(&a[2]),
        Some("lenient") => cmd_lenient(&a[2], &a[3..]),
        Some("bench") => cmd_bench(&a[2]),
        _ => {
            eprintln!("usage: v005 fixtures|vectors|lenient|bench ...");
            std::process::exit(2)
        }
    }
}
