//! OMA-ENC-1-candidate building blocks (OM_ARCHIVE_V1_SPEC.md §6). Only reviewed constructions are used:
//! HKDF-SHA256 (RFC 5869), Argon2id (RFC 9106), HMAC-SHA256 (RFC 2104), ChaCha20-Poly1305 (RFC 8439) and the
//! STREAM online-AE chunking of Hoang-Reyhanitabar-Rogaway-Vizar as profiled by the age file format (C2SP age,
//! "payload"). No new primitive is introduced. Feasibility prototype, not product code.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub const TAG: usize = 16;

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn hkdf(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm).expand(info, &mut out).expect("32 bytes is a valid HKDF length");
    out
}

pub fn hmac(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
    m.update(msg);
    m.finalize().into_bytes().into()
}

pub fn hmac_verify(key: &[u8; 32], msg: &[u8], tag: &[u8]) -> bool {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
    m.update(msg);
    m.verify_slice(tag).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argon2Params {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

/// Spec §6.2 accepted range for conforming (non-test) readers and writers.
pub fn argon2_params_acceptable(a: Argon2Params, insecure_test_kdf: bool) -> bool {
    let upper = a.m_kib <= 4 * 1024 * 1024 && a.t <= 10 && (1..=8).contains(&a.p);
    let lower = if insecure_test_kdf { a.m_kib >= 8 && a.t >= 1 } else { a.m_kib >= 64 * 1024 && a.t >= 3 };
    upper && lower
}

pub fn argon2id(pass: &[u8], salt: &[u8], a: Argon2Params) -> [u8; 32] {
    let params = argon2::Params::new(a.m_kib, a.t, a.p, Some(32)).expect("params validated by caller");
    let ctx = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    ctx.hash_password_into(pass, salt, &mut out).expect("argon2id");
    out
}

fn aead(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(key))
}

/// Key wrap: ChaCha20-Poly1305 under a single-use KEK with an all-zero nonce (safe: every KEK is derived from a
/// fresh per-archive salt / archive_id and wraps exactly one key).
pub fn wrap(kek: &[u8; 32], aad: &[u8], key: &[u8; 32]) -> Vec<u8> {
    aead(kek).encrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: key, aad }).unwrap()
}

pub fn unwrap(kek: &[u8; 32], aad: &[u8], wrapped: &[u8]) -> Option<[u8; 32]> {
    let pt = aead(kek).decrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: wrapped, aad }).ok()?;
    pt.try_into().ok()
}

fn stream_nonce(counter: u64, last: bool) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[3..11].copy_from_slice(&counter.to_be_bytes()); // 11-byte big-endian counter (top 3 bytes zero)
    n[11] = last as u8;
    n
}

/// STREAM encryption: chunks of exactly `cs` plaintext bytes, the final chunk 0..=cs bytes and flagged; the final
/// chunk is empty only when the whole plaintext is empty.
pub fn stream_encrypt(key: &[u8; 32], cs: usize, pt: &[u8]) -> Vec<u8> {
    let c = aead(key);
    let mut out = Vec::with_capacity(pt.len() + (pt.len() / cs + 1) * TAG);
    let n_chunks = if pt.is_empty() { 1 } else { pt.len().div_ceil(cs) };
    for i in 0..n_chunks {
        let chunk = &pt[i * cs..((i + 1) * cs).min(pt.len())];
        let last = i + 1 == n_chunks;
        out.extend(c.encrypt(Nonce::from_slice(&stream_nonce(i as u64, last)), chunk).unwrap());
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum StreamError {
    Auth,
    Length,
}

pub fn stream_decrypt(key: &[u8; 32], cs: usize, ct: &[u8]) -> Result<Vec<u8>, StreamError> {
    let c = aead(key);
    let full = cs + TAG;
    if ct.len() < TAG {
        return Err(StreamError::Length);
    }
    let n_chunks = ct.len().div_ceil(full);
    let mut out = Vec::with_capacity(ct.len());
    for i in 0..n_chunks {
        let chunk = &ct[i * full..((i + 1) * full).min(ct.len())];
        let last = i + 1 == n_chunks;
        if chunk.len() < TAG || (last && chunk.len() == TAG && n_chunks > 1) {
            return Err(StreamError::Length);
        }
        let pt = c.decrypt(Nonce::from_slice(&stream_nonce(i as u64, last)), chunk).map_err(|_| StreamError::Auth)?;
        out.extend(pt);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_roundtrip_and_truncation() {
        let k = [7u8; 32];
        for n in [0usize, 1, 63, 64, 65, 200] {
            let pt: Vec<u8> = (0..n).map(|i| i as u8).collect();
            let ct = stream_encrypt(&k, 64, &pt);
            assert_eq!(stream_decrypt(&k, 64, &ct).unwrap(), pt);
            if n > 64 {
                // dropping the final chunk leaves a non-final chunk in last position: must fail
                let cut = &ct[..64 + TAG];
                assert!(stream_decrypt(&k, 64, cut).is_err());
            }
        }
    }
}
