// SPDX-License-Identifier: MPL-2.0
//! ADR-0192 device-link ceremony vectors, replayed against `om_core::crypto::link_ceremony`.
//! The vectors are made by `verification/prototypes/device-link-ceremony/` (independent pure-Python construction,
//! cross-checked with Node); this test shows the Rust construction agrees byte for byte.
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use flate2::read::GzDecoder;
use om_core::crypto::link_ceremony as lc;
use serde_json::Value;
use std::io::Read;

fn vectors() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/vectors/device-link-ceremony-vectors.json.gz"
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

fn a32(v: &Value) -> [u8; 32] {
    hx(v).try_into().unwrap()
}

#[test]
fn the_rust_construction_matches_the_independent_vectors() {
    let v = vectors();
    let (i, e) = (&v["inputs"], &v["expected"]);
    let expiry = i["expiry_s"].as_u64().unwrap();
    let (home, a_cap, b_recv) = (hx(&i["home_uri"]), hx(&i["a_cap"]), hx(&i["b_recv"]));
    assert_eq!(lc::context(expiry, &home, &a_cap, &b_recv), hx(&e["ctx"]));
    let k = lc::keys(expiry, &home, &hx(&i["secret"]), &a_cap, &b_recv);
    assert_eq!(k.id, a32(&e["ceremony_id"]));
    assert_eq!(lc::prk(&k.id, &hx(&i["secret"])), a32(&e["prk"]));
    assert_eq!(k.req, a32(&e["k_req"]));
    assert_eq!(k.resp, a32(&e["k_resp"]));
    assert_eq!(k.code, a32(&e["k_code"]));
    assert_eq!(k.bind, a32(&e["k_bind"]));
    assert_eq!(lc::aead_ad(&k.id, lc::ROLE_REQ), hx(&e["ad_req"]));
    assert_eq!(lc::aead_ad(&k.id, lc::ROLE_RESP), hx(&e["ad_resp"]));

    let request = hx(&i["request"]);
    let m = lc::code_mac(&k.code, &request);
    assert_eq!(m, a32(&e["code_mac"]));
    assert_eq!(lc::code_digits(&m), e["code"].as_str().unwrap());
    assert_eq!(
        lc::verification_code(&k.code, &request),
        e["code"].as_str().unwrap()
    );

    let rd = om_core::crypto::sha256(&[&request]);
    assert_eq!(rd, a32(&e["request_digest"]));
    let nd = om_core::crypto::sha256(&[&hx(&i["notice"])]);
    assert_eq!(nd, a32(&e["notice_digest"]));
    let bind = lc::binding(
        &k.bind,
        &rd,
        &hx(&i["approver"]),
        &a32(&i["state_hash"]),
        &nd,
        i["roster_count"].as_u64().unwrap(),
        i["history_count"].as_u64().unwrap(),
    );
    assert_eq!(bind, a32(&e["binding"]));
    assert!(lc::tag_eq(&bind, &a32(&e["binding"])));

    let nonce: [u8; 24] = hx(&i["nonce"]).try_into().unwrap();
    let pt = hx(&i["plaintext"]);
    for (role, key, want) in [
        (lc::ROLE_REQ, &k.req, "sealed_req"),
        (lc::ROLE_RESP, &k.resp, "sealed_resp"),
    ] {
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &pt,
                    aad: &lc::aead_ad(&k.id, role),
                },
            )
            .unwrap();
        assert_eq!(ct, hx(&e[want]), "{want}");
    }
    assert_eq!(lc::recv_mailbox(&b_recv).to_vec(), hx(&e["mailbox"]));
}

#[test]
fn code_digits_keep_leading_zeros_and_wrap_at_the_modulus() {
    for c in v_cases() {
        let m: [u8; 32] = hx(&c["m"]).try_into().unwrap();
        assert_eq!(lc::code_digits(&m), c["digits"].as_str().unwrap());
        assert_eq!(lc::code_grouped(&m), c["grouped"].as_str().unwrap());
    }
}

fn v_cases() -> Vec<Value> {
    vectors()["code_digits"].as_array().unwrap().clone()
}

#[test]
fn every_bound_input_changes_every_key() {
    let v = vectors();
    let (i, e) = (&v["inputs"], &v["expected"]);
    for c in v["context_changes"].as_array().unwrap() {
        for f in ["k_req", "k_resp", "k_code", "k_bind"] {
            assert_ne!(c[f], e[f], "{} {f}", c["name"]);
        }
    }
    // The Rust side: changing the home, expiry, either capability or the version-bearing context moves the keys.
    let expiry = i["expiry_s"].as_u64().unwrap();
    let (home, a_cap, b_recv, s) = (
        hx(&i["home_uri"]),
        hx(&i["a_cap"]),
        hx(&i["b_recv"]),
        hx(&i["secret"]),
    );
    let base = lc::keys(expiry, &home, &s, &a_cap, &b_recv);
    for other in [
        lc::keys(expiry, b"https://other.example", &s, &a_cap, &b_recv),
        lc::keys(expiry + 1, &home, &s, &a_cap, &b_recv),
        lc::keys(expiry, &home, &s, &[0u8; 32], &b_recv),
        lc::keys(expiry, &home, &s, &a_cap, &[0u8; 32]),
    ] {
        assert_ne!(other.id, base.id);
        assert_ne!(other.code, base.code);
        assert_ne!(other.bind, base.bind);
        assert_ne!(other.req, base.req);
        assert_ne!(other.resp, base.resp);
    }
    // The negatives hold on the Rust side too: a code MAC is not a binding, and a changed field changes the tag.
    let request = hx(&i["request"]);
    let rd = om_core::crypto::sha256(&[&request]);
    let nd = om_core::crypto::sha256(&[&hx(&i["notice"])]);
    let sh = a32(&i["state_hash"]);
    let ap = hx(&i["approver"]);
    let n = &v["negatives"];
    let b = |r: &[u8; 32], a: &[u8], s: &[u8; 32], n: &[u8; 32], ro, hi| {
        lc::binding(&base.bind, r, a, s, n, ro, hi)
    };
    assert_eq!(b(&rd, &ap, &sh, &nd, 2, 3), a32(&e["binding"]));
    assert_eq!(
        b(&om_core::crypto::sha256(&[b"other"]), &ap, &sh, &nd, 2, 3),
        a32(&n["binding_other_request"])
    );
    assert_eq!(
        b(&rd, &ap, &om_core::crypto::sha256(&[b"s2"]), &nd, 2, 3),
        a32(&n["binding_other_state"])
    );
    assert_eq!(
        b(&rd, &ap, &sh, &om_core::crypto::sha256(&[b"n2"]), 2, 3),
        a32(&n["binding_other_notice"])
    );
    assert_eq!(b(&rd, &ap, &sh, &nd, 3, 3), a32(&n["binding_other_roster"]));
    assert_eq!(
        b(&rd, &ap, &sh, &nd, 2, 4),
        a32(&n["binding_other_history"])
    );
    assert_eq!(
        b(&rd, &[8u8; 16], &sh, &nd, 2, 3),
        a32(&n["binding_other_approver"])
    );
    let mut reenc = request.clone();
    reenc.push(0);
    assert_eq!(
        lc::code_mac(&base.code, &reenc),
        a32(&n["reencoded_request_code_mac"])
    );
    assert_ne!(
        a32(&n["code_mac_replayed_as_binding_differs"]),
        a32(&e["binding"])
    );
    assert!(!lc::tag_eq(
        &a32(&n["code_mac_replayed_as_binding_differs"]),
        &a32(&e["binding"])
    ));
}
