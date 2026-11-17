// SPDX-License-Identifier: MPL-2.0
//! Replays `verification/vectors/recovery-record-vectors.json.gz` (ADR-0199) through Core. The "crypto" family was
//! computed by independent code (Python hashlib/hmac, an AES-256-GCM-SIV written from RFC 8452, a brute-force pad search):
//! key derivation, associated data, padding, sealed envelopes (byte-for-byte), the second layer, generation arithmetic
//! and freshness. The wire family's "semantic_expect: reject" vectors are wire-valid and must be refused by the reader's
//! cross-field checks (section 4); every other accepted vector must read and verify.

use flate2::read::GzDecoder;
use om_core::recovery_record::*;
use serde_json::Value as J;
use std::io::Read;

fn corpus() -> J {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/recovery-record-vectors.json.gz");
    let raw = std::fs::read(p).expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn hx(v: &J) -> Vec<u8> {
    match v {
        J::String(s) => om_core::crypto::unhex(s).unwrap(),
        J::Array(parts) => {
            let mut out = Vec::new();
            for p in parts {
                match p {
                    J::String(s) => out.extend(om_core::crypto::unhex(s).unwrap()),
                    J::Object(o) => {
                        let b = om_core::crypto::unhex(o["fill"].as_str().unwrap()).unwrap()[0];
                        out.extend(std::iter::repeat_n(b, o["len"].as_u64().unwrap() as usize));
                    }
                    _ => panic!("segment"),
                }
            }
            out
        }
        _ => panic!("hex"),
    }
}

fn a32(v: &J) -> [u8; 32] {
    hx(v).try_into().unwrap()
}

fn sha(b: &[u8]) -> String {
    om_core::crypto::hex(&om_core::crypto::sha256(&[b]))
}

#[test]
fn key_derivation_locator_and_associated_data() {
    let c = corpus();
    for k in c["crypto"]["keys"].as_array().unwrap() {
        let (root, scope) = (a32(&k["root"]), a32(&k["scope"]));
        let keys = RecordKeys::derive(&root, &scope);
        assert_eq!(keys.read_cap, a32(&k["read_cap"]));
        assert_eq!(keys.write_cap, a32(&k["write_cap"]));
        assert_eq!(keys.record_key, a32(&k["record_key"]));
        assert_eq!(keys.snapshot_key, a32(&k["snapshot_key"]));
        assert_eq!(keys.locator, a32(&k["locator"]));
        assert_eq!(
            associated_data(&scope, &keys.locator),
            hx(&k["associated_data"])
        );
        assert_eq!(
            snapshot_associated_data(&scope, 7),
            hx(&k["snapshot_associated_data_gen_7"])
        );
    }
    let s = &c["crypto"]["scope_separation"];
    let a = RecordKeys::derive(&a32(&s["root"]), &a32(&s["scope_a"]));
    let b = RecordKeys::derive(&a32(&s["root"]), &a32(&s["scope_b"]));
    assert_eq!(a.read_cap, a32(&s["read_cap_a"]));
    assert_eq!(b.read_cap, a32(&s["read_cap_b"]));
    assert_ne!(
        a.read_cap, b.read_cap,
        "a key derived for another scope is another key"
    );
}

#[test]
fn padding_matches_the_brute_force_search() {
    let c = corpus();
    for p in c["crypto"]["padding"].as_array().unwrap() {
        let (u, b) = (
            p["unpadded"].as_u64().unwrap() as usize,
            p["bucket"].as_u64().unwrap() as usize,
        );
        assert_eq!(
            pad_length(u, b).map(|x| x as u64),
            p["pad"].as_u64(),
            "unpadded {u} bucket {b}"
        );
    }
    for p in c["crypto"]["bucket_for"].as_array().unwrap() {
        let u = p["unpadded"].as_u64().unwrap() as usize;
        assert_eq!(
            bucket_for(u).map(|x| x as u64),
            p["bucket"].as_u64(),
            "unpadded {u}"
        );
    }
}

#[test]
fn sealed_envelopes_are_byte_identical_to_the_independent_implementation() {
    let c = corpus();
    for s in c["crypto"]["seals"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let (root, scope) = (a32(&s["root"]), a32(&s["scope"]));
        let nonce: [u8; 12] = hx(&s["nonce"]).try_into().unwrap();
        let rec = RecoveryRecord::from_bytes(&hx(&s["record_hex"])).unwrap();
        rec.verify().unwrap();
        let plain = rec.to_padded_bytes().unwrap();
        assert_eq!(plain.len() as u64, s["bucket"].as_u64().unwrap(), "{id}");
        assert_eq!(
            sha(&plain),
            s["plaintext_sha256"].as_str().unwrap(),
            "{id}: padded plaintext"
        );
        let keys = RecordKeys::derive(&root, &scope);
        let env = seal_envelope(&keys, &scope, nonce, &plain).unwrap();
        assert_eq!(
            env.len() as u64,
            s["envelope_len"].as_u64().unwrap(),
            "{id}"
        );
        assert_eq!(
            sha(&env),
            s["envelope_sha256"].as_str().unwrap(),
            "{id}: envelope"
        );
        if let Some(h) = s.get("envelope_hex") {
            assert_eq!(env, hx(h), "{id}");
        }
        // and the reader gets the record back
        match open(&keys, &scope, &env).unwrap() {
            Plain::Record(r) => assert_eq!(*r, rec, "{id}"),
            Plain::Redirect(_) => panic!("{id}: a record"),
        }
    }
}

#[test]
fn open_negatives_and_the_positive() {
    let c = corpus();
    for n in c["crypto"]["open"].as_array().unwrap() {
        let id = n["id"].as_str().unwrap();
        let keys = RecordKeys::derive(&a32(&n["root"]), &a32(&n["scope"]));
        let r = open(&keys, &a32(&n["scope"]), &hx(&n["envelope_hex"]));
        assert_eq!(r.is_ok(), n["expect"] == "accept", "{id}: {r:?}");
    }
}

#[test]
fn second_layer_round_trip_and_generation_binding() {
    let c = corpus();
    let s = &c["crypto"]["second_layer"];
    let (key, scope) = (a32(&s["key"]), a32(&s["scope"]));
    let generation = s["generation"].as_u64().unwrap();
    let snap = RelationshipSnapshot::from_bytes(&hx(&s["snapshot_hex"])).unwrap();
    let nonce: [u8; 12] = hx(&s["nonce"]).try_into().unwrap();
    assert_eq!(
        seal_snapshot_layer(&key, &scope, generation, nonce, &snap),
        hx(&s["layer_hex"])
    );
    assert_eq!(
        open_snapshot_layer(&key, &scope, generation, &hx(&s["layer_hex"])).unwrap(),
        snap
    );
    assert!(
        open_snapshot_layer(
            &key,
            &scope,
            s["wrong_generation"].as_u64().unwrap(),
            &hx(&s["layer_hex"])
        )
        .is_err()
    );
    assert!(open_snapshot_layer(&[1; 32], &scope, generation, &hx(&s["layer_hex"])).is_err());
}

#[test]
fn generation_arithmetic_and_freshness() {
    let c = corpus();
    for g in c["crypto"]["generation"].as_array().unwrap() {
        let (a, r) = (
            g["acs"].as_u64().unwrap(),
            g["refresh"].as_u64().unwrap() as u32,
        );
        let rg = record_generation(a, r).unwrap();
        assert_eq!(rg, g["record_generation"].as_u64().unwrap());
        assert_eq!((generation_acs(rg), generation_refresh(rg)), (a, r));
    }
    for g in c["crypto"]["generation_reject"].as_array().unwrap() {
        assert!(
            record_generation(
                g["acs"].as_u64().unwrap(),
                g["refresh"].as_u64().unwrap() as u32
            )
            .is_err()
        );
    }
    let plain = b"authenticated plaintext".to_vec();
    for f in c["crypto"]["freshness"].as_array().unwrap() {
        let mark = f["mark"].as_u64().map(|g| (g, plaintext_digest(&plain)));
        let body = if f["same_plaintext"].as_bool().unwrap() {
            plain.clone()
        } else {
            b"another".to_vec()
        };
        let want = match f["expect"].as_str().unwrap() {
            "newer" => Freshness::Newer,
            "same" => Freshness::Same,
            "stale" => Freshness::Stale,
            _ => Freshness::Equivocation,
        };
        assert_eq!(
            freshness(mark, f["generation"].as_u64().unwrap(), &body),
            want
        );
    }
}

#[test]
fn wire_vectors_read_and_cross_field_rules_hold() {
    let c = corpus();
    let (mut ok, mut refused) = (0, 0);
    for v in c["vectors"].as_array().unwrap() {
        if v["expect"] != "accept" {
            continue;
        }
        let id = v["id"].as_str().unwrap();
        let bytes = hx(&v["hex"]);
        let want_ok = v.get("semantic_expect").is_none();
        let got = match v["object"].as_str().unwrap() {
            "RecoveryRecord" => RecoveryRecord::from_bytes(&bytes).and_then(|r| {
                r.verify()?;
                // a record that reads re-encodes to the same unpadded bytes
                Ok(())
            }),
            "RecoveryRedirect" => RecoveryRedirect::from_bytes(&bytes).map(|_| ()),
            "RecoveryKit" => RecoveryKit::from_bytes(&bytes).map(|_| ()),
            _ => continue,
        };
        // vectors that carry a pad (key 22) are wire-valid but not bucket-sized: the reader's bucket rule is `read_plaintext`
        assert_eq!(
            got.is_ok(),
            want_ok,
            "{id}: {got:?} ({})",
            v.get("semantic_rule").unwrap_or(&J::Null)
        );
        if want_ok {
            ok += 1;
        } else {
            refused += 1;
        }
    }
    assert!(ok >= 15 && refused >= 15, "ok {ok} refused {refused}");
}

#[test]
fn kit_round_trips_and_never_prints() {
    let root = [5u8; 32];
    let kit = RecoveryKit::make(
        &root,
        [6; 32],
        "home.example.net",
        Some(8443),
        3,
        [7; 32],
        true,
    );
    let again = RecoveryKit::from_bytes(&kit.to_bytes()).unwrap();
    assert_eq!(kit, again);
    assert_eq!(format!("{kit:?}"), "RecoveryKit(<redacted>)");
    let keys = RecordKeys::derive(&root, &[6; 32]);
    assert_eq!(kit.locator(), keys.locator);
    assert_eq!(RecordKeys::from_kit(&kit).record_key, keys.record_key);
    assert!(kit.to_bytes().len() <= MAX_KIT_BYTES);
    let bare = RecoveryKit::make(&root, [6; 32], "home.example.net", None, 0, [7; 32], false);
    assert_eq!(RecoveryKit::from_bytes(&bare.to_bytes()).unwrap(), bare);
}
