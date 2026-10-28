// SPDX-License-Identifier: MPL-2.0
//! OM-Crypto-v1 (CryptoProfile 1) as frozen by ADR-0118, ADR-0131, ADR-0135, ADR-0140 and ADR-0142:
//! PQXDH (X25519 + ML-KEM-768) plus a Double Ratchet with AES-256-GCM-SIV, a dual Ed25519 + ML-DSA-65
//! DeviceIdentity binding of every PQXDH public object, an exact last-resort replay ledger, and the
//! draft-ietf-hpke-pq-05 MLKEM768-X25519 HPKE suite. Only vetted crates are used (RustCrypto, dalek,
//! libcrux ML-KEM/ML-DSA); no primitive is implemented here.
//!
//! Adapted from the V-001 prototype (`verification/prototypes/v001-crypto`), whose committed vectors are replayed
//! by `tests/v001_vectors.rs`. Every domain-separation string is a label registered in
//! `spec/registries/om-domain-separation.json`.
//!
//! * [PQXDH] Signal, "The PQXDH Key Agreement Protocol", revision 3.
//! * [DR] Signal, "The Double Ratchet Algorithm", revision 4, section 3.
//! * FIPS 203 (ML-KEM), FIPS 204 (ML-DSA), RFC 8032, RFC 8452, RFC 5869, RFC 8949, RFC 8439.

// The frozen wire builders take every context field explicitly (that is the point: nothing is implied).
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod binding;
pub mod cbor;
pub mod delivery;
pub mod error;
pub mod hpke;
pub mod kdf;
pub mod ledger;
pub mod pqxdh;
pub mod ratchet;
pub mod rng;
pub mod session;
pub mod sig;
pub mod store;

#[cfg(any(test, feature = "insecure-test-rng"))]
pub mod selftest;
#[cfg(any(test, feature = "insecure-test-rng"))]
pub mod vectors;

pub use error::Error;

/// CryptoProfile id of OM-Crypto-v1 (spec/registries/om-crypto-profiles.json).
pub const PROFILE_V1: u64 = 1;

/// Lower-case hex.
pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

pub fn unhex(s: &str) -> Result<Vec<u8>, Error> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(Error::Decode("bad hex"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Decode("bad hex")))
        .collect()
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}
