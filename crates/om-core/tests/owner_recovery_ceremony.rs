// SPDX-License-Identifier: MPL-2.0
//! The recovery approval construction (ADR-0203 sections 2 and 3) against the independent vectors in
//! `verification/vectors/owner-recovery-vectors.json.gz`: context, transcript, keys, associated data, polling id,
//! comparison code, reply binding, evidence digest and the offer link, plus the refusals the vectors list.

mod common;

use flate2::read::GzDecoder;
use om_core::crypto::recovery_approval as ra;
use om_core::crypto::unhex;
use serde_json::Value as J;
use std::io::Read;

fn corpus() -> J {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/owner-recovery-vectors.json.gz");
    let raw = std::fs::read(p).expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn h(v: &J) -> Vec<u8> {
    unhex(v.as_str().unwrap()).unwrap()
}

#[test]
fn the_construction_matches_the_independent_vectors() {
    let c = &corpus()["ceremony"];
    let offer = ra::decode_offer(c["offer"].as_str().unwrap()).expect("the vector offer parses");
    assert_eq!(
        ra::encode_offer(&offer).unwrap(),
        c["offer"].as_str().unwrap()
    );
    let ctx = ra::context(
        offer.expiry_s.into(),
        offer.uri.as_bytes(),
        &offer.a_cap,
        &offer.b_recv,
    );
    assert_eq!(ctx, h(&c["context"]));
    let k = ra::keys(
        offer.expiry_s.into(),
        offer.uri.as_bytes(),
        &offer.secret,
        &offer.a_cap,
        &offer.b_recv,
    );
    assert_eq!(k.id.to_vec(), h(&c["transcript"]));
    assert_eq!(k.req.to_vec(), h(&c["keys"]["req"]));
    assert_eq!(k.resp.to_vec(), h(&c["keys"]["resp"]));
    assert_eq!(k.code.to_vec(), h(&c["keys"]["code"]));
    assert_eq!(k.bind.to_vec(), h(&c["keys"]["bind"]));
    assert_eq!(ra::aead_ad(&k.id, 1), h(&c["ad"][0]));
    assert_eq!(ra::aead_ad(&k.id, 2), h(&c["ad"][1]));
    assert_eq!(ra::mailbox(&offer.b_recv).to_vec(), h(&c["mailbox"]));
}

#[test]
fn evidence_digest_and_descriptor_replay_and_the_binding_covers_the_exact_proposal() {
    let c = &corpus()["ceremony"];
    let offer = ra::decode_offer(c["offer"].as_str().unwrap()).unwrap();
    let k = ra::keys(
        offer.expiry_s.into(),
        offer.uri.as_bytes(),
        &offer.secret,
        &offer.a_cap,
        &offer.b_recv,
    );
    let desc_bytes = h(&c["descriptor"]);
    let d = ra::Descriptor::from_bytes(&desc_bytes).expect("the vector descriptor");
    assert_eq!(d.to_bytes(), desc_bytes);
    let evidence = h(&c["evidence"]);
    assert_eq!(ra::evidence_digest(&evidence), d.evidence_digest);
    assert_eq!(d.evidence_len as usize, evidence.len());
    assert_eq!(d.chunks, 2, "3,2xx,xxx bytes in two 50,000-byte slices");
    // The binding binds the ceremony, the exact proposal and the descriptor: change any one and it differs.
    let want = h(&c["binding"]);
    let vectors = corpus();
    let proposal_hex = vectors["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "RecoveryApprovalProposal/v1")
        .unwrap()["hex"]
        .as_str()
        .unwrap()
        .to_string();
    let proposal = unhex(&proposal_hex).unwrap();
    assert!(
        ra::Proposal::from_bytes(&proposal).is_some(),
        "the vector proposal is a strict proposal"
    );
    let tag = ra::reply_tag(&k.bind, &k.id, &proposal, &desc_bytes);
    assert_eq!(tag.to_vec(), want, "the independent HMAC");
    let mut other = proposal.clone();
    *other.last_mut().unwrap() ^= 1;
    assert_ne!(ra::reply_tag(&k.bind, &k.id, &other, &desc_bytes), tag);
    let mut other_desc = d.clone();
    other_desc.target[0] ^= 1;
    assert_ne!(
        ra::reply_tag(&k.bind, &k.id, &proposal, &other_desc.to_bytes()),
        tag
    );
    assert_eq!(
        ra::code_digits(&k.code, &proposal),
        c["code"].as_str().unwrap()
    );
}

#[test]
fn the_other_ceremony_cannot_be_read_as_this_one_in_either_direction() {
    let c = &corpus()["ceremony"];
    // An enrollment link is not an approval offer: never reinterpreted.
    let crossed = c["refusals"][0]["offer"].as_str().unwrap();
    assert!(crossed.starts_with("om:link/"));
    assert_eq!(ra::decode_offer(crossed), Err(ra::OfferError::Prefix));
    // An unsupported version of this ceremony is refused (the refusal an old reader of version 1 would also give).
    let this = c["offer"].as_str().unwrap();
    assert!(this.starts_with("om:recover-approve/"));
    let mut token = common::b64_decode(&this[ra::OFFER_PREFIX.len()..]);
    token[0] = 2;
    let s = format!("{}{}", ra::OFFER_PREFIX, b64url(&token));
    assert_eq!(ra::decode_offer(&s), Err(ra::OfferError::Version));
    // A damaged token is refused whole.
    let mut cut = this.to_string();
    cut.truncate(cut.len() - 3);
    assert!(ra::decode_offer(&cut).is_err());
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for ch in b.chunks(3) {
        let n = (ch[0] as u32) << 16
            | (*ch.get(1).unwrap_or(&0) as u32) << 8
            | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..=ch.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}
