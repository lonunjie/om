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
    /// KeyMaterialProvider revokeGeneration (ASYNC_KEY_MATERIAL section 6 step 2): best-effort deletion of an older
    /// signed-prekey generation's bundle and one-time items, queued ahead of the rotation's publish. Idempotent; the
    /// provider keeping them is never evidence they are safe (PC-K06).
    RevokeKeyGeneration {
        #[serde(with = "hexs")]
        device_id: Vec<u8>,
        generation: u64,
    },
    /// KeyMaterialProvider fetch: `[signed bundle, popped one-time items...]`.
    /// `service` is the Home that holds the device's pool (from the peer's accepted relationship) and `capability`
    /// the fetch capability the peer issued for this relationship.
    FetchKeyMaterial {
        #[serde(with = "hexs")]
        device_id: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service: Option<String>,
        #[serde(default, with = "hexs", skip_serializing_if = "Vec::is_empty")]
        capability: Vec<u8>,
    },
    /// KeyMaterialProvider: the device owner lets the holder of `capability` fetch this device's bundle and one-time
    /// items (ASYNC_KEY_MATERIAL section 2.1, a per-relationship fetch capability; deviation D4). Sent to the owner's
    /// own Home; the capability itself travels to the peer inside the accepted relationship.
    GrantKeyFetch {
        #[serde(with = "hexs")]
        device_id: Vec<u8>,
        #[serde(with = "hexs")]
        capability: Vec<u8>,
    },
    /// Introduction service (the inviter's own Home): store the invite descriptor under its id and open the single-use
    /// ticket the descriptor's `IntroductionTicket` names (`expires_at` and `payload_limit` are that ticket's).
    PutInvite {
        #[serde(with = "hexs")]
        invite_id: Vec<u8>,
        #[serde(with = "hexs")]
        descriptor: Vec<u8>,
        expires_at: u64,
        #[serde(default)]
        payload_limit: u64,
    },
    /// Read an invite descriptor from the introduction service the link names (`service`), or from the host's own
    /// Home when the link names none.
    GetInvite {
        #[serde(with = "hexs")]
        invite_id: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service: Option<String>,
    },
    /// Submit a sealed `MessageRequest` to the introduction service named in the inviter's signed descriptor.
    SubmitIntro {
        service: String,
        #[serde(with = "hexs")]
        bytes: Vec<u8>,
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
        #[serde(default, skip_serializing_if = "MailboxKind::is_relay")]
        kind: MailboxKind,
        /// The Home that holds the mailbox (the `relay_uri` of the route Core bound to it); absent for a legacy mailbox
        /// and for an introduction mailbox, which live at the host's configured Home.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay_uri: Option<String>,
        /// The receive capability to present when it is not the host's own (a device-link ceremony queue another
        /// install opened); empty for every mailbox the host derives its capabilities for.
        #[serde(default, with = "hexs", skip_serializing_if = "Vec::is_empty")]
        capability: Vec<u8>,
    },
    /// BlobStore put of one attachment's ciphertext. The host holds the bytes (Core never keeps ciphertext in its state):
    /// it stored them when `prepare_attachment` returned them, and `restage_attachment` rebuilds them from the plaintext.
    /// `owner_cap` is the capability the sender chose for this object (read and delete); the answer is a
    /// [`BlobReceipt`] as `Bytes`. `request_id` makes an exact retry idempotent. A put that exceeds the store's limit
    /// answers `Rejected{too_large}`; a full store `Rejected{quota}`.
    PutBlob {
        #[serde(with = "hexs")]
        blob_id: Vec<u8>,
        #[serde(with = "hexs")]
        owner_cap: Vec<u8>,
        #[serde(with = "hexs")]
        request_id: Vec<u8>,
        /// `OM-BLOB-CIPHERTEXT-HASH-v1` over the ciphertext; the store must refuse bytes that do not hash to it.
        #[serde(with = "hexs")]
        digest: Vec<u8>,
        length: u64,
    },
    /// Fetch one attachment's ciphertext from the first of `locations` that answers (all are `https`, from the sender).
    /// Not an outbox item: the host performs it when the person (or its own auto-download policy) asks, hashes the
    /// bytes against `digest` and hands them to `Core::open_attachment`, which verifies again before it decrypts.
    GetBlob {
        locations: Vec<String>,
        #[serde(with = "hexs")]
        read_cap: Vec<u8>,
        #[serde(with = "hexs")]
        digest: Vec<u8>,
        length: u64,
    },
    /// Remove a blob of ours from the BlobStore that stored it (`location`). Absent is success.
    DeleteBlob {
        #[serde(with = "hexs")]
        blob_id: Vec<u8>,
        #[serde(with = "hexs")]
        owner_cap: Vec<u8>,
        location: String,
    },
    /// Device-link ceremony (D24): make sure the short-lived queue `mailbox` exists at the Home `relay_uri` and answer
    /// its receive capability as `Bytes`, so the other device can drain it. Uses the queue endpoints a Home already has.
    OpenQueue {
        #[serde(with = "hexs")]
        mailbox: Vec<u8>,
        relay_uri: String,
    },
    /// STUB (recovery, work group E): deliver a `signed-RecoveryRequest` to every route of the account's Home(s)
    /// (`RecoveryRequestFanout` in `home-api.cddl`). No Home implements it yet, so the stock adapters answer
    /// `Rejected{unsupported}`; Core never relies on it. The recovering device is Slice 4.
    RecoveryRequestFanout {
        #[serde(with = "hexs")]
        bytes: Vec<u8>,
    },
    /// STUB (recovery, work group E): deliver a `signed-RecoveryVeto` the same way. The Home API freezes no veto
    /// endpoint yet (see the recovery slice 3 report), so this is Core's intent only; adapters answer
    /// `Rejected{unsupported}` and the outbox drops it.
    RecoveryVetoFanout {
        #[serde(with = "hexs")]
        bytes: Vec<u8>,
    },
}

/// A durable monotonic reading the HOST supplies with every recovery observation (ADR-0131/0147/0154). `boot_session`
/// is an opaque 1..=32 byte id of the current boot of the host's monotonic clock, `elapsed_ms` the milliseconds that
/// clock has run in that boot. Core credits only growth inside one boot session; the gap between sessions (downtime)
/// is never credited, and a reading that goes backwards credits nothing. Wall clocks are not an input here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonotonicSample {
    #[serde(with = "hexs")]
    pub boot_session: Vec<u8>,
    pub elapsed_ms: u64,
}

/// What the adapter answers a `PutBlob` with (canonical CBOR as `NetResult::Bytes`):
/// `{0: read capability (bstr 16..64), 1: [locations: tstr], 2: ciphertext digest the store computed (bstr 32),
/// ? 3: expires_at (posix seconds)}`. Core compares the digest with its own and refuses anything else.
pub struct BlobReceipt {
    pub read_cap: Vec<u8>,
    pub locations: Vec<String>,
    pub digest: [u8; 32],
    pub expires_at: Option<u64>,
}

impl BlobReceipt {
    pub fn to_bytes(&self) -> Vec<u8> {
        use crate::crypto::cbor::{V, encode};
        let mut m = vec![
            (0, V::B(self.read_cap.clone())),
            (
                1,
                V::A(self.locations.iter().map(|l| V::T(l.clone())).collect()),
            ),
            (2, V::B(self.digest.to_vec())),
        ];
        if let Some(e) = self.expires_at {
            m.push((3, V::U(e)));
        }
        encode(&V::M(m))
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let v = crate::crypto::cbor::decode(b).ok()?;
        let read_cap = v.get(0).ok()?.b().ok()?.to_vec();
        if !(16..=64).contains(&read_cap.len()) {
            return None;
        }
        let locations: Vec<String> = v
            .get(1)
            .ok()?
            .a()
            .ok()?
            .iter()
            .map(|x| x.t().ok().map(str::to_string))
            .collect::<Option<_>>()?;
        Some(BlobReceipt {
            read_cap,
            locations,
            digest: v.get(2).ok()?.b32().ok()?,
            expires_at: match v.opt(3) {
                None => None,
                Some(e) => Some(e.u().ok()?),
            },
        })
    }
}

/// What the store advertises for blobs (`ProviderCapabilities.limits` of `om.blob.spool.v1`), supplied by the host with
/// `Core::set_blob_limits`. Until it is supplied Core has no BlobStore and refuses attachments (it never falls back to
/// sending bytes inline).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobLimits {
    /// Largest single object in bytes (ciphertext, so plaintext + 16).
    pub max_blob_bytes: u64,
    /// Whole spool, bytes. 0: not advertised (no pre-check; the store's `quota` still answers).
    #[serde(default)]
    pub max_spool_bytes: u64,
    /// Objects the spool holds. 0: not advertised.
    #[serde(default)]
    pub max_blobs: u32,
}

/// What a polled id names: a relay mailbox, or an invite's introduction service at the host's own Home.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxKind {
    #[default]
    Relay,
    Intro,
}

impl MailboxKind {
    pub fn is_relay(&self) -> bool {
        *self == MailboxKind::Relay
    }
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
#[serde(rename_all = "camelCase")]
pub struct PollTarget {
    #[serde(with = "hexs")]
    pub mailbox: Vec<u8>,
    pub after: u64,
    #[serde(default, skip_serializing_if = "MailboxKind::is_relay")]
    pub kind: MailboxKind,
    /// The Home that holds this mailbox (see `NetCall::Ack`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_uri: Option<String>,
    /// An explicit receive capability (see `NetCall::Ack`); empty for the host's own mailboxes.
    #[serde(default, with = "hexs", skip_serializing_if = "Vec::is_empty")]
    pub capability: Vec<u8>,
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
    /// A backup file could not be written or read (see `backup::BackupError`).
    Backup(crate::backup::BackupError),
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
