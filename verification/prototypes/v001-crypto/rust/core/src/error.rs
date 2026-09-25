// SPDX-License-Identifier: Apache-2.0
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Malformed or non-canonical encoding.
    Decode(&'static str),
    /// A composite Ed25519 + ML-DSA-65 signature did not verify (PQXDH §3.3: "Alice aborts").
    BadSignature,
    /// The device binding chain (ACS -> DeviceCredential -> signed bundle) is inconsistent.
    Binding(&'static str),
    /// Bundle lacks a signed PQ prekey. OM never falls back to a classical-only handshake.
    MissingPqPrekey,
    /// PQ public key failed ML-KEM encapsulation-key validation (FIPS 203 §7.2).
    InvalidPqKey,
    /// Initial message carries a missing/short ML-KEM ciphertext (classical-only attempt).
    InvalidPqCiphertext,
    UnknownSignedPrekey,
    /// One-time PQ prekey id is unknown or was already consumed (single-use enforcement).
    UnknownOrConsumedPqPrekey,
    /// One-time EC prekey id is unknown or was already consumed.
    UnknownOrConsumedEcPrekey,
    /// Replay of a no-one-time-key handshake (PQXDH §4.2; ADR-0131 rule 2 exact ledger).
    ReplayedLastResortHandshake,
    /// The signed-prekey epoch reached last_resort_accept_limit (4096); fails closed, no eviction.
    LastResortExhausted,
    /// The durable vault did not confirm the commit; nothing was released.
    VaultCommitFailed,
    /// AEAD authentication failed.
    DecryptFailed,
    /// DR §3.5 SkipMessageKeys bound exceeded.
    TooManySkipped,
    /// DR §3.1 KDF_CK(None).
    NoChain,
    NoSession,
    /// A different handshake for an existing session; multi-session policy belongs to V-003.
    SessionConflict,
    NoPrekeysPublished,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Stable short name for results JSON.
    pub fn name(&self) -> String {
        match self {
            Error::Binding(s) => format!("Binding({s})"),
            Error::Decode(s) => format!("Decode({s})"),
            e => format!("{e:?}"),
        }
    }
}
