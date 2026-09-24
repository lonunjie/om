// SPDX-License-Identifier: Apache-2.0
//! HKDF/HMAC-SHA-256 helpers and every prototype domain-separation constant in one place.

use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// [PQXDH] §2.2 KDF info: "<info>_<curve>_<hash>_<pqkem>".
pub const PQXDH_INFO: &[u8] = b"OM-Crypto-v1-proto_CURVE25519_SHA-256_ML-KEM-768";
/// [DR] §7.1: expand PQXDH SK into SKec || SKscka for the Triple Ratchet.
pub const TR_SPLIT_INFO: &[u8] = b"OM-Crypto-v1-proto TripleRatchet SK split";
/// [DR] §7.2 KDF_RK info.
pub const DR_RK_INFO: &[u8] = b"OM-Crypto-v1-proto DoubleRatchet KDF_RK";
/// [DR] §7.2 KDF_HYBRID info (TR_PROTOCOL_INFO).
pub const TR_PROTOCOL_INFO: &[u8] = b"OM-Crypto-v1-proto TripleRatchet KDF_HYBRID";
/// OM policy (PQXDH §4.2 "blacklist of observed messages") for last-resort handshakes.
pub const LR_REPLAY_TAG: &[u8] = b"OM-Crypto-v1-proto last-resort replay tag";

pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out)
        .expect("HKDF output length within 255*32");
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

/// [DR] §7.1: SK -> (SKec, SKscka).
pub fn tr_split(sk: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut o = [0u8; 64];
    hkdf_sha256(&[0u8; 32], sk, TR_SPLIT_INFO, &mut o);
    (o[..32].try_into().unwrap(), o[32..].try_into().unwrap())
}

/// [DR] §7.2 KDF_RK(rk, dh_out) = HKDF(salt = rk, IKM = dh_out, info), 64 bytes -> (rk', ck).
pub fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut o = [0u8; 64];
    hkdf_sha256(rk, dh_out, DR_RK_INFO, &mut o);
    (o[..32].try_into().unwrap(), o[32..].try_into().unwrap())
}

/// [DR] §7.2 KDF_CK(ck): mk = HMAC(ck, 0x01), ck' = HMAC(ck, 0x02).
pub fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    (hmac_sha256(ck, &[0x02]), hmac_sha256(ck, &[0x01]))
}

/// [DR] §6.3/§7.2 KDF_HYBRID(ec_mk, pq_mk) = HKDF(salt = pq_mk, IKM = ec_mk, info = TR_PROTOCOL_INFO).
/// Output: 32-byte AES-256-GCM-SIV key || 12-byte nonce ([DR] §3.1 permits a nonce derived
/// alongside the key because each message key is used once; GCM-SIV adds misuse resistance).
pub fn kdf_hybrid(ec_mk: &[u8; 32], pq_mk: &[u8]) -> ([u8; 32], [u8; 12]) {
    let mut o = [0u8; 44];
    hkdf_sha256(pq_mk, ec_mk, TR_PROTOCOL_INFO, &mut o);
    (o[..32].try_into().unwrap(), o[32..].try_into().unwrap())
}
