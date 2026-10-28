// SPDX-License-Identifier: MPL-2.0
//! MessageRequest HPKE for OM-Crypto-v1 (ADR-0131 rule 9), implemented from the specifications and
//! checked against the draft's own published test vectors (no OM-invented hybrid):
//!
//! * draft-ietf-hpke-pq-05 §4: MLKEM768-X25519 (kem_id 0x647a); DeriveKeyPair via SHAKE256.LabeledDerive;
//! * draft-irtf-cfrg-concrete-hybrid-kems-03 §4.2 and draft-irtf-cfrg-hybrid-kems-12 §5.1/§5.5/App. A:
//!   CG framework, PRG SHAKE256, KDF SHA3-256, label "\.//^\", EncapsDerand;
//! * draft-ietf-hpke-hpke-03 §4.4/§5.1 (= RFC 9180 for a two-stage KDF): HKDF-SHA256 key schedule, base mode;
//! * RFC 8439 ChaCha20Poly1305.
//!
//! OM binding (ADR-0135 §5): info = OM-CBOR ["OM-MESSAGE-REQUEST-HPKE-INFO-v1", crypto_profile,
//! ticket_id, request_key_digest], aad = empty, one Seal per MessageRequest (sequence 0).

use crate::crypto::Error;
use crate::crypto::cbor::{self, V};
use crate::crypto::sig::digest;
use chacha20poly1305::ChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use hkdf::Hkdf;
use libcrux_ml_kem::mlkem768 as mlkem;
use sha2::Sha256;
use sha3::{Digest, Sha3_256};
use shake::Shake256;
use shake::digest::{ExtendableOutput, Update, XofReader};
use x25519_dalek::{PublicKey, StaticSecret};

pub const KEM_ID: u16 = 0x647a;
pub const KDF_ID: u16 = 0x0001;
pub const AEAD_ID: u16 = 0x0003;
pub const LABEL: &[u8] = b"\\.//^\\";
pub const NPK: usize = 1184 + 32;
pub const NENC: usize = 1088 + 32;
pub const INFO_LABEL: &str = "OM-MESSAGE-REQUEST-HPKE-INFO-v1";

fn kem_suite() -> Vec<u8> {
    let mut s = b"KEM".to_vec();
    s.extend_from_slice(&KEM_ID.to_be_bytes());
    s
}

fn hpke_suite() -> Vec<u8> {
    let mut s = b"HPKE".to_vec();
    for x in [KEM_ID, KDF_ID, AEAD_ID] {
        s.extend_from_slice(&x.to_be_bytes());
    }
    s
}

fn shake256(parts: &[&[u8]], out: &mut [u8]) {
    let mut h = Shake256::default();
    for p in parts {
        h.update(p);
    }
    h.finalize_xof().read(out);
}

fn lp(x: &[u8]) -> Vec<u8> {
    let mut o = (x.len() as u16).to_be_bytes().to_vec();
    o.extend_from_slice(x);
    o
}

/// SHAKE256.LabeledDerive(ikm, label, context, L) with suite_id = "KEM" || 0x647a.
fn labeled_derive(ikm: &[u8], label: &[u8], context: &[u8], out: &mut [u8]) {
    let l = (out.len() as u16).to_be_bytes();
    shake256(
        &[ikm, b"HPKE-v1", &kem_suite(), &lp(label), &l, context],
        out,
    );
}

struct Expanded {
    ek_pq: Vec<u8>,
    ek_t: [u8; 32],
    dk_pq: mlkem::MlKem768KeyPair,
    dk_t: [u8; 32],
}

fn expand(seed: &[u8; 32]) -> Expanded {
    let mut full = [0u8; 96];
    shake256(&[seed], &mut full);
    let kp = mlkem::generate_key_pair(full[..64].try_into().unwrap());
    let dk_t: [u8; 32] = full[64..].try_into().unwrap();
    Expanded {
        ek_pq: kp.public_key().as_slice().to_vec(),
        ek_t: PublicKey::from(&StaticSecret::from(dk_t)).to_bytes(),
        dk_pq: kp,
        dk_t,
    }
}

fn combine(ss_pq: &[u8], ss_t: &[u8; 32], ct_t: &[u8; 32], ek_t: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha3_256::new();
    Digest::update(&mut h, ss_pq);
    Digest::update(&mut h, ss_t);
    Digest::update(&mut h, ct_t);
    Digest::update(&mut h, ek_t);
    Digest::update(&mut h, LABEL);
    h.finalize().into()
}

/// HPKE DeriveKeyPair(ikm) -> (sk = 32-byte seed, pk = ek_PQ || ek_T).
pub fn derive_key_pair(ikm: &[u8]) -> ([u8; 32], Vec<u8>) {
    let mut seed = [0u8; 32];
    labeled_derive(ikm, b"DeriveKeyPair", b"", &mut seed);
    (seed, public_key(&seed))
}

pub fn public_key(seed: &[u8; 32]) -> Vec<u8> {
    let e = expand(seed);
    let mut pk = e.ek_pq;
    pk.extend_from_slice(&e.ek_t);
    pk
}

/// EncapsDerand(ek, randomness[64]) -> (shared_secret, enc).
pub fn encaps_derand(pk: &[u8], rnd: &[u8; 64]) -> Result<([u8; 32], Vec<u8>), Error> {
    if pk.len() != NPK {
        return Err(Error::InvalidPqKey);
    }
    let ek = mlkem::MlKem768PublicKey::try_from(&pk[..1184]).map_err(|_| Error::InvalidPqKey)?;
    if !mlkem::validate_public_key(&ek) {
        return Err(Error::InvalidPqKey);
    }
    let ek_t: [u8; 32] = pk[1184..].try_into().unwrap();
    let (ct_pq, ss_pq) = mlkem::encapsulate(&ek, rnd[..32].try_into().unwrap());
    let sk_e: [u8; 32] = rnd[32..].try_into().unwrap();
    let ct_t = PublicKey::from(&StaticSecret::from(sk_e)).to_bytes();
    let ss_t = StaticSecret::from(sk_e)
        .diffie_hellman(&PublicKey::from(ek_t))
        .to_bytes();
    let mut enc = ct_pq.as_slice().to_vec();
    enc.extend_from_slice(&ct_t);
    Ok((combine(ss_pq.as_slice(), &ss_t, &ct_t, &ek_t), enc))
}

pub fn decaps(seed: &[u8; 32], enc: &[u8]) -> Result<[u8; 32], Error> {
    if enc.len() != NENC {
        return Err(Error::InvalidPqCiphertext);
    }
    let e = expand(seed);
    let ct = mlkem::MlKem768Ciphertext::try_from(&enc[..1088])
        .map_err(|_| Error::InvalidPqCiphertext)?;
    let ss_pq = mlkem::decapsulate(e.dk_pq.private_key(), &ct);
    let ct_t: [u8; 32] = enc[1088..].try_into().unwrap();
    let ss_t = StaticSecret::from(e.dk_t)
        .diffie_hellman(&PublicKey::from(ct_t))
        .to_bytes();
    Ok(combine(ss_pq.as_slice(), &ss_t, &ct_t, &e.ek_t))
}

fn labeled_extract(salt: &[u8], label: &[u8], ikm: &[u8]) -> [u8; 32] {
    let mut l = b"HPKE-v1".to_vec();
    l.extend_from_slice(&hpke_suite());
    l.extend_from_slice(label);
    l.extend_from_slice(ikm);
    let (prk, _) = Hkdf::<Sha256>::extract(Some(salt), &l);
    prk.into()
}

fn labeled_expand(prk: &[u8; 32], label: &[u8], info: &[u8], out: &mut [u8]) {
    let mut i = (out.len() as u16).to_be_bytes().to_vec();
    i.extend_from_slice(b"HPKE-v1");
    i.extend_from_slice(&hpke_suite());
    i.extend_from_slice(label);
    i.extend_from_slice(info);
    Hkdf::<Sha256>::from_prk(prk)
        .expect("prk")
        .expand(&i, out)
        .expect("len");
}

#[derive(Clone, Debug)]
pub struct Context {
    pub key: [u8; 32],
    pub base_nonce: [u8; 12],
    pub exporter_secret: [u8; 32],
    pub seq: u64,
}

pub fn key_schedule(shared_secret: &[u8; 32], info: &[u8]) -> Context {
    let psk_id_hash = labeled_extract(b"", b"psk_id_hash", b"");
    let info_hash = labeled_extract(b"", b"info_hash", info);
    let mut ctx = vec![0u8];
    ctx.extend_from_slice(&psk_id_hash);
    ctx.extend_from_slice(&info_hash);
    let secret = labeled_extract(shared_secret, b"secret", b"");
    let mut c = Context {
        key: [0; 32],
        base_nonce: [0; 12],
        exporter_secret: [0; 32],
        seq: 0,
    };
    labeled_expand(&secret, b"key", &ctx, &mut c.key);
    labeled_expand(&secret, b"base_nonce", &ctx, &mut c.base_nonce);
    labeled_expand(&secret, b"exp", &ctx, &mut c.exporter_secret);
    c
}

impl Context {
    pub fn nonce(&self) -> [u8; 12] {
        let mut n = self.base_nonce;
        for (i, b) in self.seq.to_be_bytes().iter().enumerate() {
            n[4 + i] ^= b;
        }
        n
    }
    pub fn seal(&mut self, aad: &[u8], pt: &[u8]) -> Vec<u8> {
        let c = ChaCha20Poly1305::new_from_slice(&self.key).expect("key");
        let ct = c
            .encrypt(&self.nonce().into(), Payload { msg: pt, aad })
            .expect("seal");
        self.seq += 1;
        ct
    }
    pub fn open(&mut self, aad: &[u8], ct: &[u8]) -> Result<Vec<u8>, Error> {
        let c = ChaCha20Poly1305::new_from_slice(&self.key).expect("key");
        let pt = c
            .decrypt(&self.nonce().into(), Payload { msg: ct, aad })
            .map_err(|_| Error::DecryptFailed)?;
        self.seq += 1;
        Ok(pt)
    }
    pub fn export(&self, exporter_context: &[u8], out: &mut [u8]) {
        labeled_expand(&self.exporter_secret, b"sec", exporter_context, out);
    }
}

/// OM MessageRequest info (ADR-0135 §5).
pub fn message_request_info(
    profile: u64,
    ticket_id: &[u8; 16],
    request_key_digest: &[u8; 32],
) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(INFO_LABEL.into()),
        V::U(profile),
        V::B(ticket_id.to_vec()),
        digest(request_key_digest),
    ]))
}

/// Run every draft-ietf-hpke-pq-05 vector for (0x647a, HKDF-SHA256, ChaCha20Poly1305, base).
pub fn check_kats(json: &str) -> serde_json::Value {
    let hx =
        |s: &serde_json::Value| crate::crypto::unhex(s.as_str().unwrap_or("")).unwrap_or_default();
    let all: serde_json::Value = serde_json::from_str(json).expect("vector json");
    let mut n = 0;
    let mut checks = 0;
    let mut failed: Vec<String> = vec![];
    for t in all.as_array().unwrap() {
        if t["kem_id"] != KEM_ID as u64
            || t["kdf_id"] != KDF_ID as u64
            || t["aead_id"] != AEAD_ID as u64
            || t["mode"] != 0
        {
            continue;
        }
        let mut r: Vec<(String, bool)> = vec![];
        let (sk, pk) = derive_key_pair(&hx(&t["ikmR"]));
        r.push(("skRm".into(), sk.to_vec() == hx(&t["skRm"])));
        r.push(("pkRm".into(), pk == hx(&t["pkRm"])));
        let rnd: [u8; 64] = hx(&t["ikmE"]).try_into().expect("ikmE 64");
        let (ss, enc) = encaps_derand(&hx(&t["pkRm"]), &rnd).expect("encaps");
        r.push(("enc".into(), enc == hx(&t["enc"])));
        r.push((
            "shared_secret".into(),
            ss.to_vec() == hx(&t["shared_secret"]),
        ));
        let info = hx(&t["info"]);
        let mut cs = key_schedule(&ss, &info);
        r.push(("key".into(), cs.key.to_vec() == hx(&t["key"])));
        r.push((
            "base_nonce".into(),
            cs.base_nonce.to_vec() == hx(&t["base_nonce"]),
        ));
        r.push((
            "exporter_secret".into(),
            cs.exporter_secret.to_vec() == hx(&t["exporter_secret"]),
        ));
        let skr: [u8; 32] = hx(&t["skRm"]).try_into().expect("skRm");
        let ssr = decaps(&skr, &hx(&t["enc"])).expect("decaps");
        r.push(("decap".into(), ssr == ss));
        let mut cr = key_schedule(&ssr, &info);
        for (i, e) in t["encryptions"].as_array().unwrap().iter().enumerate() {
            let nonce_ok = cs.nonce().to_vec() == hx(&e["nonce"]);
            r.push((
                format!("seal[{i}]"),
                nonce_ok && cs.seal(&hx(&e["aad"]), &hx(&e["pt"])) == hx(&e["ct"]),
            ));
            r.push((
                format!("open[{i}]"),
                cr.open(&hx(&e["aad"]), &hx(&e["ct"])).ok() == Some(hx(&e["pt"])),
            ));
        }
        for (i, x) in t["exports"].as_array().unwrap().iter().enumerate() {
            let mut o = vec![0u8; x["L"].as_u64().unwrap() as usize];
            cs.export(&hx(&x["exporter_context"]), &mut o);
            r.push((format!("export[{i}]"), o == hx(&x["exported_value"])));
        }
        checks += r.len();
        failed.extend(
            r.into_iter()
                .filter(|(_, ok)| !ok)
                .map(|(k, _)| format!("vector{n}:{k}")),
        );
        n += 1;
    }
    serde_json::json!({ "vectors": n, "checks": checks, "failed": failed, "all_passed": n > 0 && failed.is_empty() })
}
