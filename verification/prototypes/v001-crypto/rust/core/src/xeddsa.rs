// SPDX-License-Identifier: Apache-2.0
//! XEdDSA over curve25519 per [XEdDSA] §2.3 (calculate_key_pair), §3.3 (sign), §3.4 (verify).
//! [PQXDH] §2.2 requires Sig(PK, M, Z) to be XEdDSA with 64 bytes of randomness Z, so the
//! identity key IK is a single X25519 key used for both DH and prekey signatures.
//! Built only from curve25519-dalek group operations; no new primitive.

use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::montgomery::MontgomeryPoint;
use curve25519_dalek::scalar::{clamp_integer, Scalar};
use sha2::{Digest, Sha512};

fn sha512_scalar(parts: &[&[u8]]) -> Scalar {
    let mut h = Sha512::new();
    for p in parts {
        h.update(p);
    }
    let mut w = [0u8; 64];
    w.copy_from_slice(&h.finalize());
    Scalar::from_bytes_mod_order_wide(&w)
}

/// [XEdDSA] §2.3 calculate_key_pair(k): E = kB; A.y = E.y, A.s = 0; a = -k if E.s == 1 else k.
fn calculate_key_pair(k: &[u8; 32]) -> ([u8; 32], Scalar) {
    let ks = Scalar::from_bytes_mod_order(clamp_integer(*k));
    let mut a_enc = EdwardsPoint::mul_base(&ks).compress().to_bytes();
    let sign = a_enc[31] >> 7;
    a_enc[31] &= 0x7f;
    let a = if sign == 1 { -ks } else { ks };
    (a_enc, a)
}

/// [XEdDSA] §3.3 xeddsa_sign(k, M, Z).
pub fn sign(k: &[u8; 32], msg: &[u8], z: &[u8; 64]) -> [u8; 64] {
    let (a_enc, a) = calculate_key_pair(k);
    // hash1(X) = SHA-512(2^256 - 1 - 1 || X), the prefix encoded as 32 little-endian bytes FE FF..FF.
    let mut prefix = [0xffu8; 32];
    prefix[0] = 0xfe;
    let r = sha512_scalar(&[&prefix, a.as_bytes(), msg, z]);
    let r_enc = EdwardsPoint::mul_base(&r).compress().to_bytes();
    let h = sha512_scalar(&[&r_enc, &a_enc, msg]);
    let s = r + h * a;
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_enc);
    sig[32..].copy_from_slice(s.as_bytes());
    sig
}

/// p = 2^255 - 19 little-endian.
const P_LE: [u8; 32] = {
    let mut p = [0xffu8; 32];
    p[0] = 0xed;
    p[31] = 0x7f;
    p
};

fn lt_le(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    false
}

/// [XEdDSA] §3.4 xeddsa_verify(u, M, (R || s)).
pub fn verify(u: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    // "if u >= p or R.y >= 2^|p| or s >= 2^|q|: return false" (|p| = 255, |q| = 253).
    // R.y is the low 255 bits of R (bit 255 is the x sign), so R.y < 2^255 always holds.
    if !lt_le(u, &P_LE) || sig[63] & 0xe0 != 0 {
        return false;
    }
    // convert_mont(u): Edwards point with y = (u - 1)/(u + 1), sign bit 0.
    let Some(a_pt) = MontgomeryPoint(*u).to_edwards(0) else { return false };
    let a_enc = a_pt.compress().to_bytes();
    let r_enc: [u8; 32] = sig[..32].try_into().unwrap();
    let s = Scalar::from_bytes_mod_order(sig[32..].try_into().unwrap());
    let h = sha512_scalar(&[&r_enc, &a_enc, msg]);
    // Rcheck = sB - hA
    let r_check = EdwardsPoint::vartime_double_scalar_mul_basepoint(&(-h), &a_pt, &s);
    r_check.compress().to_bytes() == r_enc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sign_verify_and_reject() {
        for i in 0u8..16 {
            let k = [i.wrapping_mul(37).wrapping_add(1); 32];
            let u = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(k)).to_bytes();
            let z = [i; 64];
            let sig = sign(&k, b"prekey", &z);
            assert!(verify(&u, b"prekey", &sig));
            assert!(!verify(&u, b"prekeY", &sig));
            let mut bad = sig;
            bad[5] ^= 1;
            assert!(!verify(&u, b"prekey", &bad));
        }
    }
}
