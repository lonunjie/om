// SPDX-License-Identifier: Apache-2.0
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Malformed or non-canonical encoding.
    Decode(&'static str),
    /// A prekey signature did not verify (PQXDH §3.3: "Alice aborts the protocol").
    BadSignature,
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
    /// Replay of a handshake that used the reusable last-resort PQ prekey (PQXDH §4.2 mitigation).
    ReplayedLastResortHandshake,
    /// AEAD authentication failed.
    DecryptFailed,
    /// DR §3.5 SkipMessageKeys bound exceeded.
    TooManySkipped,
    /// DR §3.1 KDF_CK(None).
    NoChain,
    NoSession,
    /// A different handshake for an existing session; multi-session policy belongs to V-003.
    SessionConflict,
    /// SPQR produced no message key: PQ ratchet disabled or downgraded. Never accepted.
    PqRatchetUnavailable,
    /// Error surfaced by Signal's `spqr` crate (e.g. MinimumVersion on a V0 downgrade).
    Spqr(String),
    NoPrekeysPublished,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for Error {}

impl From<spqr::Error> for Error {
    fn from(e: spqr::Error) -> Self {
        Error::Spqr(format!("{e:?}"))
    }
}

impl Error {
    /// Stable short name for results JSON.
    pub fn name(&self) -> String {
        match self {
            Error::Spqr(s) => format!("Spqr({})", s.split('(').next().unwrap_or(s)),
            Error::Decode(s) => format!("Decode({s})"),
            e => format!("{e:?}"),
        }
    }
}
