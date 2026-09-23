// SPDX-License-Identifier: Apache-2.0
//! Feasibility prototype for V-001 (`production_crypto_path`). Not product code. Not for production.
//!
//! OM-Crypto-v1 1:1 path as selected by `CRYPTOGRAPHY_AND_AGILITY.md` (2026-09-23 resolution):
//! PQXDH (X25519 + ML-KEM-768) followed by the Triple Ratchet (EC Double Ratchet + Sparse
//! Post-Quantum Ratchet). Implemented from the public specifications:
//!
//! * [PQXDH] Signal, "The PQXDH Key Agreement Protocol", revision 3 (2023-05-24, updated 2024-01-23).
//! * [DR]    Signal, "The Double Ratchet Algorithm", revision 4 (2025-11-04), incl. §5 SPQR, §6 Triple Ratchet.
//! * [XEdDSA] Signal, "The XEdDSA and VXEdDSA Signature Schemes", revision 1 (2016-10-20).
//! * [FIPS203] NIST FIPS 203 ML-KEM (via `libcrux-ml-kem`).
//! * [RFC8452] AES-GCM-SIV, [RFC5869] HKDF, [RFC8949] deterministic CBOR.
//!
//! The SCKA / SPQR half of the Triple Ratchet is NOT implemented here: it is Signal's own
//! `spqr` crate (AGPL-3.0-only), used through its public `initial_state`/`send`/`recv` API.
//!
//! Written by a Claude agent. Every domain-separation string below is a prototype value
//! (`OM-Crypto-v1-proto`), not a frozen OM registry entry.

pub mod acvp;
pub mod bench;
pub mod cbor;
pub mod error;
pub mod kdf;
pub mod otk;
pub mod pqxdh;
pub mod ratchet;
pub mod rng;
pub mod store;
pub mod triple;
pub mod xeddsa;

#[cfg(feature = "insecure-test-rng")]
pub mod persist;
#[cfg(feature = "insecure-test-rng")]
pub mod vectors;

pub use error::Error;

/// Prototype profile identifier carried in every ratchet header (not a frozen CryptoProfile ID).
pub const PROFILE_PROTO: u64 = 1;

/// Hex helper used by vectors/results.
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
    if s.len() % 2 != 0 {
        return Err(Error::Decode("odd hex length"));
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
    let mut o = [0u8; 32];
    o.copy_from_slice(&h.finalize());
    o
}
