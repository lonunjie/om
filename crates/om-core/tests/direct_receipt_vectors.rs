// SPDX-License-Identifier: MPL-2.0
//! ADR-0194 direct-transfer receipt vectors, replayed against `om_core::crypto::direct_receipt`.
//! The vectors are made by `verification/prototypes/direct-receipt/` (independent pure-Python construction, recomputed
//! with Node); this test shows the Rust construction agrees byte for byte and that every negative gives another tag.
use flate2::read::GzDecoder;
use om_core::attach::ciphertext_digest;
use om_core::crypto::direct_receipt as dr;
use serde_json::Value;
use std::io::Read;

fn vectors() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/vectors/direct-transfer-receipt-vectors.json.gz"
    );
    let mut s = String::new();
    GzDecoder::new(&std::fs::read(p).unwrap()[..])
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}

fn hx(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn a<const N: usize>(v: &Value) -> [u8; N] {
    hx(v).try_into().unwrap()
}

#[test]
fn the_rust_construction_matches_the_independent_vectors() {
    let v = vectors();
    let cases = v["constructions"].as_array().unwrap();
    assert!(cases.len() >= 4);
    for c in cases {
        let id = c["id"].as_str().unwrap();
        let (key, blob, tid) = (
            a::<32>(&c["content_key"]),
            a::<32>(&c["blob_id"]),
            a::<16>(&c["transfer_id"]),
        );
        let ct = hx(&c["ciphertext"]);
        assert_eq!(ct.len() as u64, c["total_bytes"].as_u64().unwrap(), "{id}");
        let digest = ciphertext_digest(&ct);
        assert_eq!(digest, a::<32>(&c["ciphertext_digest"]), "{id} digest");
        assert_eq!(
            dr::receipt_key(&key, &digest, &blob),
            a::<32>(&c["k_rcpt"]),
            "{id} key"
        );
        assert_eq!(
            dr::possession_hash(&tid, &ct),
            a::<32>(&c["h_pos"]),
            "{id} h_pos"
        );
        assert_eq!(
            dr::receipt_tag(&key, &digest, &blob, &tid, &ct),
            a::<32>(&c["receipt_tag"]),
            "{id} tag"
        );
    }
    assert!(
        cases
            .iter()
            .any(|c| c["total_bytes"].as_u64().unwrap() > 16_384)
    );
}

#[test]
fn every_changed_input_gives_another_tag() {
    let v = vectors();
    let base = v["constructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "4096-bytes")
        .unwrap();
    let want = a::<32>(&base["receipt_tag"]);
    let negs = v["negatives"].as_array().unwrap();
    assert!(negs.len() >= 9);
    for n in negs {
        let id = n["id"].as_str().unwrap();
        let (key, blob, tid, pos_tid, dig) = (
            a::<32>(&n["content_key"]),
            a::<32>(&n["blob_id"]),
            a::<16>(&n["transfer_id"]),
            a::<16>(&n["pos_transfer_id"]),
            a::<32>(&n["ciphertext_digest"]),
        );
        let ct = hx(&n["ciphertext"]);
        let h_pos = dr::possession_hash(&pos_tid, &ct);
        let got = dr::tag_from_parts(
            &key,
            &dig,
            &blob,
            &tid,
            n["total_bytes"].as_u64().unwrap(),
            &h_pos,
        );
        assert_eq!(
            got,
            a::<32>(&n["receipt_tag"]),
            "{id}: the Rust value is the vector's"
        );
        assert_ne!(got, want, "{id} must not verify as the original");
    }
    // a resumed transfer is confirmed like a fresh one: same transfer id, same object, same tag
    assert_eq!(a::<32>(&v["resumed"]["receipt_tag"]), want);
}
