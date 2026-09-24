// SPDX-License-Identifier: Apache-2.0
//! Official NIST ACVP ML-KEM (FIPS 203) known-answer tests against the ML-KEM layer used by
//! the prototype (`libcrux-ml-kem`). Input: `internalProjection.json` for ML-KEM-keyGen-FIPS203
//! and ML-KEM-encapDecap-FIPS203 from github.com/usnistgov/ACVP-Server (pinned by run.sh).
//! Only ML-KEM-768 groups are exercised (the OM-Crypto-v1 parameter set).

use crate::unhex;
use libcrux_ml_kem::mlkem768 as m;
use serde_json::{json, Value};

fn hx(t: &Value, k: &str) -> Vec<u8> {
    unhex(t[k].as_str().unwrap_or("")).unwrap_or_default()
}

pub fn run(keygen_json: &str, encdec_json: &str) -> Value {
    let kg: Value = serde_json::from_str(keygen_json).expect("keyGen json");
    let ed: Value = serde_json::from_str(encdec_json).expect("encapDecap json");
    let mut counts = std::collections::BTreeMap::<String, (u64, u64)>::new();
    let mut failures = vec![];
    let mut tally = |name: &str, tc: &Value, ok: bool, failures: &mut Vec<Value>| {
        let e = counts.entry(name.to_string()).or_default();
        e.0 += 1;
        if ok {
            e.1 += 1;
        } else {
            failures.push(json!({"group": name, "tcId": tc["tcId"]}));
        }
    };
    for g in kg["testGroups"].as_array().unwrap() {
        if g["parameterSet"] != "ML-KEM-768" {
            continue;
        }
        for t in g["tests"].as_array().unwrap() {
            let mut seed = [0u8; 64];
            seed[..32].copy_from_slice(&hx(t, "d"));
            seed[32..].copy_from_slice(&hx(t, "z"));
            let kp = m::generate_key_pair(seed);
            let ok = kp.public_key().as_slice().as_slice() == hx(t, "ek").as_slice()
                && kp.private_key().as_slice().as_slice() == hx(t, "dk").as_slice();
            tally("keyGen", t, ok, &mut failures);
        }
    }
    for g in ed["testGroups"].as_array().unwrap() {
        if g["parameterSet"] != "ML-KEM-768" {
            continue;
        }
        let func = g["function"].as_str().unwrap_or("").to_string();
        for t in g["tests"].as_array().unwrap() {
            let ok = match func.as_str() {
                "encapsulation" => {
                    let pk = m::MlKem768PublicKey::try_from(hx(t, "ek").as_slice()).unwrap();
                    let mm: [u8; 32] = hx(t, "m").try_into().unwrap();
                    let (c, k) = m::encapsulate(&pk, mm);
                    c.as_slice().as_slice() == hx(t, "c").as_slice() && k.as_slice() == hx(t, "k").as_slice()
                }
                "decapsulation" => {
                    let sk = m::MlKem768PrivateKey::try_from(hx(t, "dk").as_slice()).unwrap();
                    let c = m::MlKem768Ciphertext::try_from(hx(t, "c").as_slice()).unwrap();
                    m::decapsulate(&sk, &c).as_slice() == hx(t, "k").as_slice()
                }
                "encapsulationKeyCheck" => {
                    let want = t["testPassed"].as_bool().unwrap();
                    let got = m::MlKem768PublicKey::try_from(hx(t, "ek").as_slice())
                        .map(|pk| m::validate_public_key(&pk))
                        .unwrap_or(false);
                    got == want
                }
                "decapsulationKeyCheck" => {
                    let want = t["testPassed"].as_bool().unwrap();
                    let dummy = m::MlKem768Ciphertext::from([0u8; 1088]);
                    let got = m::MlKem768PrivateKey::try_from(hx(t, "dk").as_slice())
                        .map(|sk| m::validate_private_key(&sk, &dummy))
                        .unwrap_or(false);
                    got == want
                }
                _ => continue,
            };
            tally(&func, t, ok, &mut failures);
        }
    }
    let total: u64 = counts.values().map(|c| c.0).sum();
    let passed: u64 = counts.values().map(|c| c.1).sum();
    json!({
        "implementation": "libcrux-ml-kem 0.0.10 (mlkem768)",
        "parameter_set": "ML-KEM-768",
        "groups": counts.iter().map(|(k, (n, p))| (k.clone(), json!({"cases": n, "passed": p}))).collect::<serde_json::Map<_, _>>(),
        "total": total,
        "passed": passed,
        "failures": failures,
        "all_passed": total > 0 && total == passed,
    })
}
