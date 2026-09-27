// SPDX-License-Identifier: Apache-2.0
//! Feasibility prototype for V-001 (`production_crypto_path`). Not product code. Not for production.
//!
//! OM-Crypto-v1 as frozen by ADR-0118 (no SPQR in v1), ADR-0131 and ADR-0135:
//! PQXDH (X25519 + ML-KEM-768) + Double Ratchet with AES-256-GCM-SIV, a dual Ed25519 + ML-DSA-65
//! DeviceIdentity binding of every PQXDH public object, an exact last-resort replay ledger, and the
//! draft-ietf-hpke-pq-05 MLKEM768-X25519 HPKE suite for MessageRequests. From the public specifications:
//!
//! * [PQXDH] Signal, "The PQXDH Key Agreement Protocol", revision 3 (2023-05-24, updated 2024-01-23).
//! * [DR]    Signal, "The Double Ratchet Algorithm", revision 4 (2025-11-04) §3.
//! * [FIPS203] ML-KEM (`libcrux-ml-kem`), [FIPS204] ML-DSA (`libcrux-ml-dsa`), [RFC8032] Ed25519.
//! * [RFC8452] AES-GCM-SIV, [RFC5869] HKDF, [RFC8949] deterministic CBOR, [RFC8439] ChaCha20Poly1305.
//! * draft-ietf-hpke-pq-05 and its normative references (hpke.rs).
//!
//! Written by a Claude agent. Every domain-separation string is a label registered in
//! spec/registries/om-domain-separation.json.

pub mod acvp;
pub mod bench;
pub mod binding;
pub mod cbor;
pub mod delivery;
pub mod error;
pub mod hpke;
pub mod kdf;
pub mod ledger;
pub mod otk;
pub mod pqxdh;
pub mod ratchet;
pub mod rng;
pub mod session;
pub mod sig;
pub mod store;

#[cfg(feature = "insecure-test-rng")]
pub mod persist;
#[cfg(feature = "insecure-test-rng")]
pub mod vectors;

pub use error::Error;

/// CryptoProfile id of OM-Crypto-v1 (spec/registries/om-crypto-profiles.json).
pub const PROFILE_V1: u64 = 1;

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
