// SPDX-License-Identifier: Apache-2.0
//! HKDF/HMAC-SHA-256 helpers and every OM-Crypto-v1 domain-separation constant in one place.
//! Each string is a label registered in spec/registries/om-domain-separation.json; the exact input
//! constructions are pinned by ADR-0135 (the CryptoProfile ADR that closes V-001).

use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// [PQXDH] §2.1 info (registered verbatim as a reserved label).
pub const PQXDH_INFO: &[u8] = b"OM-Crypto-v1_PQXDH_X25519_SHA-256_ML-KEM-768";
/// [DR] §5.2 KDF_RK info.
pub const DR_RK_INFO: &[u8] = b"OM-1TO1-RATCHET-KDF-v1";
/// ADR-0131 rule 3.
pub const DR_AEAD_KEY_INFO: &[u8] = b"OM-DR-AEAD-KEY-v1";
pub const DR_AEAD_NONCE_INFO: &[u8] = b"OM-DR-AEAD-NONCE-v1";
/// DS-HASH label of the canonical ratchet header (ADR-0135 §4).
pub const DR_HEADER_HASH: &str = "OM-DR-HEADER-HASH-v1";
/// AEAD associated-data label (ADR-0135 §4).
pub const EVENT_AEAD_AD: &str = "OM-1TO1-EVENT-AEAD-AD-v1";
/// ADR-0131 rule 2.
pub const HANDSHAKE_ID: &str = "OM-PQXDH-HANDSHAKE-ID-v1";

pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out)
        .expect("HKDF output length within 255*32");
}

pub fn hkdf_expand(prk: &[u8; 32], info: &[u8], out: &mut [u8]) {
    Hkdf::<Sha256>::from_prk(prk).expect("32-byte PRK").expand(info, out).expect("HKDF length");
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC accepts any key length");
    m.update(data);
    let r = m.finalize().into_bytes();
    let mut o = [0u8; 32];
    o.copy_from_slice(&r);
    o
}

/// [PQXDH] §2.2 KDF(KM): HKDF(salt = 0^32, IKM = 0xFF^32 || KM, info = PQXDH_INFO), 32 bytes.
pub fn pqxdh_kdf(km: &[u8]) -> [u8; 32] {
    let mut ikm = vec![0xffu8; 32];
    ikm.extend_from_slice(km);
    let mut sk = [0u8; 32];
    hkdf_sha256(&[0u8; 32], &ikm, PQXDH_INFO, &mut sk);
    zeroize::Zeroize::zeroize(&mut ikm);
    sk
}

/// [DR] §5.2 KDF_RK(rk, dh_out) = HKDF(salt = rk, IKM = dh_out, info), 64 bytes -> (rk', ck).
pub fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut o = [0u8; 64];
    hkdf_sha256(rk, dh_out, DR_RK_INFO, &mut o);
    (o[..32].try_into().unwrap(), o[32..].try_into().unwrap())
}

/// [DR] §5.2 KDF_CK(ck): mk = HMAC(ck, 0x01), ck' = HMAC(ck, 0x02). Returns (ck', mk).
pub fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    (hmac_sha256(ck, &[0x02]), hmac_sha256(ck, &[0x01]))
}

/// ADR-0131 rule 3: K = HKDF-Expand(MK, "OM-DR-AEAD-KEY-v1", 32);
/// N = first 12 bytes of HKDF-Expand(MK, "OM-DR-AEAD-NONCE-v1" || header_hash, 32).
pub fn dr_aead_key_nonce(mk: &[u8; 32], header_hash: &[u8; 32]) -> ([u8; 32], [u8; 12]) {
    let mut k = [0u8; 32];
    hkdf_expand(mk, DR_AEAD_KEY_INFO, &mut k);
    let mut info = DR_AEAD_NONCE_INFO.to_vec();
    info.extend_from_slice(header_hash);
    let mut n = [0u8; 32];
    hkdf_expand(mk, &info, &mut n);
    (k, n[..12].try_into().unwrap())
}
