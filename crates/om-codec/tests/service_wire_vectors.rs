// SPDX-License-Identifier: MPL-2.0
//! Replays the services_bots wire-freeze corpus (`verification/prototypes/services-wire/vectors.json`, ADR-0262,
//! `spec/cddl/service.cddl`) through the product codec. The corpus was written with an independent Python codec; this is
//! the second decoder on it. Every positive vector must be accepted by a W4 decoder and re-encode to the identical
//! bytes. Every negative vector whose error class is structural (field, range, canonical form, set order, null, signing
//! role on the envelope, W0 rejection of key 13) must be refused by the codec; the chain and authorization classes
//! (stale and forked chains and the rules that need a validated head) belong to the service control engine (slice 1) and
//! are only counted here, so a corpus change cannot slip by unnoticed.

use om_codec::cbor::{encode, unhex};
use om_codec::schema::Registry;
use serde_json::Value as J;
use std::path::PathBuf;

fn corpus() -> J {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/services-wire/vectors.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

const STRUCTURAL: &[&str] = &[
    "missing_field",
    "unknown_field",
    "value_range",
    "non_canonical",
    "set_order",
    "null_forbidden",
    "w0_receiver_rejection",
];

/// Negatives that om-codec still accepts (cross-field rules not yet in `service_rules`). Empty since the rules landed;
/// kept so any future gap is pinned exactly.
const PENDING_CROSS_FIELD: &[&str] = &[];

fn registry_for(v: &J) -> Registry {
    match v["receiver_wave"].as_str() {
        Some("W0") => Registry::load(),
        _ => Registry::load_w4(),
    }
}

#[test]
fn services_vectors_replay_through_the_product_codec() {
    let doc = corpus();
    let (pos, neg) = (
        doc["positive"].as_array().unwrap(),
        doc["negative"].as_array().unwrap(),
    );
    assert!(pos.len() >= 21 && neg.len() >= 80, "corpus shrank");
    let mut bad = Vec::new();
    for v in pos {
        let id = v["id"].as_str().unwrap();
        let wire = unhex(v["wire_hex"].as_str().unwrap());
        assert_eq!(wire, unhex(v["canonical_hex"].as_str().unwrap()), "{id}");
        match registry_for(v).check(v["object"].as_str().unwrap(), &wire, false) {
            Ok(val) if encode(&val) == wire => {}
            Ok(_) => bad.push(format!("{id}: re-encode differs")),
            Err(e) => bad.push(format!("{id}: refused with {e}")),
        }
    }
    let mut semantic = 0;
    for v in neg {
        let id = v["id"].as_str().unwrap();
        let class = v["error_class"].as_str().unwrap();
        let wire = unhex(v["wire_hex"].as_str().unwrap());
        let got = registry_for(v).check(v["object"].as_str().unwrap(), &wire, false);
        if STRUCTURAL.contains(&class) {
            let pending = PENDING_CROSS_FIELD.contains(&id);
            if got.is_ok() != pending {
                bad.push(format!(
                    "{id}: {class} {}",
                    if pending {
                        "now refused: drop it from PENDING_CROSS_FIELD"
                    } else {
                        "accepted but must be refused"
                    }
                ));
            }
        } else {
            semantic += 1;
        }
    }
    assert!(bad.is_empty(), "{} mismatches: {bad:#?}", bad.len());
    println!("semantic negatives left to the control engine: {semantic}");
}
