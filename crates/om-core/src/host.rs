// SPDX-License-Identifier: MPL-2.0
//! The sans-IO host boundary. Core never touches storage or the network: it returns durable `Commit`s and
//! `NetOp`s for the host (a PlatformAdapter-backed driver) to perform, and is told what happened.
//!
//! * **Storage** is the host's encrypted per-profile key/value store (the DeviceVault `EncryptedKv`). Core
//!   hands it opaque records and receives them back at `open_profile`; it holds no keys for them.
//! * **Commit before release.** A state change is staged, returned as a `Commit`, and only becomes visible to
//!   queries (and its network effects only become visible to the host) after `finish_commit(id, true)`. If the
//!   host cannot persist it, `finish_commit(id, false)` discards it and nothing was released.
//! * **Network** operations are listed from a persisted outbox. They can be re-listed after a restart and retried
//!   with the same identity; a result is fed back with `net_result`.

use crate::api::{OperationHandle, OperationId, ProfileId, RejectReason};
use serde::{Deserialize, Serialize};

/// Serde helper: `Vec<u8>` as lower-case hex.
pub mod hexs {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::crypto::hex(b))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        crate::crypto::unhex(&s).map_err(|_| serde::de::Error::custom("bad hex"))
    }
}

/// Serde helper: `Vec<Vec<u8>>` as an array of hex strings.
pub mod hexs_vec {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(b: &[Vec<u8>], s: S) -> Result<S::Ok, S::Error> {
        b.iter()
            .map(|x| crate::crypto::hex(x))
            .collect::<Vec<_>>()
            .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|s| crate::crypto::unhex(s).map_err(|_| serde::de::Error::custom("bad hex")))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub key: String,
    #[serde(with = "hexs")]
    pub value: Vec<u8>,
}

/// One durable write in a profile's store. `value: None` deletes the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Write {
    pub key: String,
    #[serde(with = "option_hex")]
    pub value: Option<Vec<u8>>,
}

mod option_hex {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(b: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match b {
            Some(b) => s.serialize_str(&crate::crypto::hex(b)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| crate::crypto::unhex(&s).map_err(|_| serde::de::Error::custom("bad hex")))
            .transpose()
    }
}

/// An atomic batch the host must make durable (all or nothing) before calling `finish_commit(id, true)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub id: u64,
    pub profile_id: ProfileId,
    pub writes: Vec<Write>,
}

/// Result of one Core transition. `commit` is `None` when nothing durable changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub commit: Option<Commit>,
    pub handle: Option<OperationHandle>,
}

/// What `finish_commit` releases: which operations and profiles changed, for observers/subscribers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Released {
    pub profile_id: ProfileId,
    pub changed_operations: Vec<OperationId>,
}

// ---------------------------------------------------------------------------------------------- network

/// A capability call on the host's provider adapter. Provider names never appear here (absolute rule 1); routes
/// are random per-purpose ids, never AccountIDs (absolute rule 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "call",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum NetCall {
    /// KeyMaterialProvider publish: signed bundle replaces, one-time items append.
    PublishKeyMaterial {
        #[serde(with = "hexs")]
        device_id: Vec<u8>,
        #[serde(with = "hexs")]
        bundle: Vec<u8>,
        #[serde(with = "hexs_vec")]
        opks: Vec<Vec<u8>>,
        #[serde(with = "hexs_vec")]
        pqopks: Vec<Vec<u8>>,
    },
    /// KeyMaterialProvider fetch: `[signed bundle, popped one-time items...]`.
    FetchKeyMaterial {
        #[serde(with = "hexs")]
        device_id: Vec<u8>,
    },
    /// IntroGate store of an invite descriptor under its capability id.
    PutInvite {
        #[serde(with = "hexs")]
        invite_id: Vec<u8>,
        #[serde(with = "hexs")]
        descriptor: Vec<u8>,
        expires_at: u64,
    },
    GetInvite {
        #[serde(with = "hexs")]
        invite_id: Vec<u8>,
    },
    RevokeInvite {
        #[serde(with = "hexs")]
        invite_id: Vec<u8>,
    },
    /// Relay deposit. `mailbox` is the destination's deposit capability exactly as its `RelayEntry` carried it, and
    /// `relay_uri` is that entry's `relay_uri`, so the adapter can refuse a destination that is not at the Home it is
    /// configured for instead of depositing at the wrong place. `relay_uri` is absent only for a legacy (pass 2)
    /// destination, where `mailbox` is the bare 16-byte mailbox id and the adapter derives the capability itself.
    Deposit {
        #[serde(with = "hexs")]
        mailbox: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay_uri: Option<String>,
        #[serde(with = "hexs")]
        bytes: Vec<u8>,
    },
    /// Relay acknowledgement that everything up to `seq` is durably processed (frees relay storage).
    Ack {
        #[serde(with = "hexs")]
        mailbox: Vec<u8>,
        seq: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetOp {
    pub id: u64,
    pub call: NetCall,
}

/// Provider-neutral failure classes. `Timeout` means the bytes may have been delivered (never treated as rejection).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NetError {
    /// Definitely not performed; safe to retry the exact call.
    Unavailable,
    /// Outcome unknown; reconcile by exact retry.
    Timeout,
    NotFound,
    Rejected {
        reason: RejectReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum NetResult {
    Ok,
    Bytes {
        #[serde(with = "hexs")]
        bytes: Vec<u8>,
    },
    Many {
        #[serde(with = "hexs_vec")]
        items: Vec<Vec<u8>>,
    },
    Err {
        error: NetError,
    },
}

/// A mailbox the host should poll, and the highest sequence Core has already processed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollTarget {
    #[serde(with = "hexs")]
    pub mailbox: Vec<u8>,
    pub after: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inbound {
    pub seq: u64,
    #[serde(with = "hexs")]
    pub bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------------------------- errors

/// Thrown (not an outcome): the call itself was malformed or addressed state it does not own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    /// Captured context is unknown, stale or forged, or the object belongs to another profile. Fails closed.
    ProfileAccess(&'static str),
    /// An operation id was reused for a different intent.
    OperationIdConflict,
    /// A commit for this profile is still awaiting the host.
    Busy,
    /// Stored records could not be decoded; the profile stays closed.
    CorruptState(&'static str),
    NotFound(&'static str),
    Invalid(&'static str),
}

impl core::fmt::Display for CoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CoreError {}

/// Wall-clock milliseconds. Display and expiry hints only: never authority or security freshness
/// (engineering_scope_contracts.json forbidden shortcut).
pub trait Clock {
    fn now_ms(&self) -> u64;
}

impl<F: Fn() -> u64> Clock for F {
    fn now_ms(&self) -> u64 {
        self()
    }
}
