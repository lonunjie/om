// SPDX-License-Identifier: MPL-2.0
//! Signature-input and hash-input derivation for decoded W0 objects (DS-SIG / DS-HASH from
//! `spec/registries/om-domain-separation.json`). Used by the conformance replay and by callers that need the
//! exact bytes a signature covers.

use crate::cbor::{Limits, Value, decode, encode, hex};
use crate::schema::Registry;
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

pub fn bytes_of(v: Option<&Value>) -> &[u8] {
    match v {
        Some(Value::Bytes(b)) => b,
        _ => panic!("expected bytes"),
    }
}

pub fn uint_of(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Uint(n)) => *n,
        _ => panic!("expected uint"),
    }
}

pub fn ds_sig(label: &str, profile: u64, role: u64, signer: &[u8], body: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text(label.into()),
        Value::Uint(profile),
        Value::Uint(role),
        Value::Bytes(signer.to_vec()),
        Value::Bytes(body.to_vec()),
    ]))
}

pub fn ds_hash(label: &str, body: &[u8]) -> Vec<u8> {
    encode(&Value::Array(vec![
        Value::Text(label.into()),
        Value::Bytes(body.to_vec()),
    ]))
}

/// Signature inputs and hashes (the object must already have passed `Registry::check`; this panics otherwise) computed independently from the decoded object.
pub fn derived(reg: &Registry, name: &str, top: &Value, top_bytes: &[u8]) -> Vec<J> {
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
        out.push(
            json!({"kind": "hash", "input_hex": hex(&hi), "sha256": hex(&Sha256::digest(&hi))}),
        );
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
