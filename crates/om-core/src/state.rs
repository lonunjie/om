// SPDX-License-Identifier: MPL-2.0
//! Per-profile Core state and its persistence. A profile persists as two opaque records in that profile's
//! encrypted store: `device` (deterministic CBOR: keys, sessions, ledger, relationship pins) and `app`
//! (versioned JSON: conversations, invites, operation log, network outbox). Nothing here is ever shared
//! between profiles, and there is no identifier linking two profiles (ADR-0148).

use crate::api::{DeliveryState, OperationId, OutcomeClass};
use crate::crypto::store::Device;
use crate::host::{CoreError, NetCall, Record, Write};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const REC_DEVICE: &str = "device";
pub const REC_APP: &str = "app";
pub const APP_VERSION: u32 = 1;

pub type Hex = String;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyRec {
    pub key_operation_id: OperationId,
    pub groups: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRec {
    pub operation_id: OperationId,
    pub invite_id: Hex,
    pub link: String,
    pub expires_at: u64,
    pub mailbox: Hex,
    /// HPKE private key seed for the request key. Single-purpose; dies with the invite.
    pub request_seed: Hex,
    pub request_digest: Hex,
    pub revoked: bool,
    pub used: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroRec {
    pub request_id: String,
    pub invite_operation_id: OperationId,
    pub person_id: String,
    pub display_name: String,
    pub peer_acs_body: Hex,
    pub peer_credentials: Vec<Hex>,
    pub peer_mailbox: Hex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelState {
    /// We are the inviter and are setting up the session (fetching the peer's key material, sending Hello).
    Establishing,
    /// We are the invitee: the request is sent, the inviter has not yet accepted (no session exists).
    AwaitingAcceptance,
    Active,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Msg {
    pub id: String,
    pub outgoing: bool,
    pub operation_id: Option<OperationId>,
    pub text: String,
    pub sent_at: u64,
    pub delivery: DeliveryState,
    /// The exact encrypted envelope, kept until the relay accepts it so a retry never re-runs the ratchet.
    #[serde(default)]
    pub envelope: Option<Hex>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub person_id: String,
    pub display_name: String,
    pub state: RelState,
    pub peer_account_id: Hex,
    pub peer_device_id: Hex,
    pub peer_ik: Hex,
    pub peer_acs_body: Hex,
    pub peer_credentials: Vec<Hex>,
    pub my_mailbox: Hex,
    pub peer_mailbox: Option<Hex>,
    pub blocked: bool,
    pub integrity_failures: u32,
    pub created_at: u64,
    pub messages: Vec<Msg>,
    /// SHA-256 of the last processed envelopes (bounded). A relay may redeliver; a duplicate is dropped silently,
    /// anything else that fails to decrypt is counted as an integrity failure.
    #[serde(default)]
    pub seen_envelopes: Vec<Hex>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpRec {
    pub operation_id: OperationId,
    pub kind: String,
    pub outcome: OutcomeClass,
    pub message_id: Option<String>,
}

/// Why an outbox entry exists; decides what its result changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "purpose",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Purpose {
    PublishKeys,
    PutInvite {
        operation_id: OperationId,
    },
    RevokeInvite {
        operation_id: OperationId,
    },
    ResolveInvite {
        operation_id: OperationId,
        invite_id: Hex,
        acs_hash: Hex,
        request_digest: Hex,
    },
    DepositRequest {
        operation_id: OperationId,
        conversation_id: String,
    },
    FetchPeerKeys {
        operation_id: OperationId,
        conversation_id: String,
    },
    DepositHello {
        operation_id: OperationId,
        conversation_id: String,
    },
    DepositMessage {
        operation_id: OperationId,
        conversation_id: String,
        message_id: String,
    },
    Ack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub id: u64,
    pub call: NetCall,
    pub purpose: Purpose,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub version: u32,
    pub label: String,
    pub created_at: u64,
    pub recovery_key: Option<RecoveryKeyRec>,
    pub recovery_key_saved: bool,
    pub invites: BTreeMap<OperationId, InviteRec>,
    pub accepted_invites: BTreeSet<Hex>,
    pub intros: BTreeMap<String, IntroRec>,
    pub relationships: BTreeMap<String, Relationship>,
    pub ops: BTreeMap<OperationId, OpRec>,
    pub outbox: Vec<OutboxEntry>,
    pub next_net_id: u64,
    /// Mailbox (hex) -> highest relay sequence already processed and committed.
    pub cursors: BTreeMap<Hex, u64>,
    pub keys_published: bool,
}

#[derive(Clone, Debug)]
pub struct ProfileState {
    pub generation: String,
    pub device: Option<Device>,
    pub app: AppState,
}

impl ProfileState {
    pub fn empty(generation: String) -> Self {
        ProfileState {
            generation,
            device: None,
            app: AppState {
                version: APP_VERSION,
                ..AppState::default()
            },
        }
    }

    pub fn encode(&self) -> Result<BTreeMap<String, Vec<u8>>, CoreError> {
        let mut out = BTreeMap::new();
        if let Some(d) = &self.device {
            out.insert(REC_DEVICE.to_string(), d.to_bytes());
        }
        out.insert(
            REC_APP.to_string(),
            serde_json::to_vec(&self.app).map_err(|_| CoreError::CorruptState("encode app"))?,
        );
        Ok(out)
    }

    pub fn decode(generation: String, records: &[Record]) -> Result<Self, CoreError> {
        let mut st = ProfileState::empty(generation);
        let mut seen = BTreeSet::new();
        for r in records {
            if !seen.insert(r.key.as_str()) {
                return Err(CoreError::CorruptState("duplicate record"));
            }
            match r.key.as_str() {
                REC_DEVICE => {
                    st.device = Some(
                        Device::from_bytes(&r.value)
                            .map_err(|_| CoreError::CorruptState("device record"))?,
                    )
                }
                REC_APP => {
                    st.app = serde_json::from_slice(&r.value)
                        .map_err(|_| CoreError::CorruptState("app record"))?;
                    if st.app.version != APP_VERSION {
                        return Err(CoreError::CorruptState("unsupported app record version"));
                    }
                }
                _ => {} // unknown records belong to a newer client; preserved by the host, ignored here
            }
        }
        Ok(st)
    }
}

/// Writes needed to move a store from `old` to `new` (changed or new keys only).
pub fn diff(old: &BTreeMap<String, Vec<u8>>, new: &BTreeMap<String, Vec<u8>>) -> Vec<Write> {
    new.iter()
        .filter(|(k, v)| old.get(*k) != Some(*v))
        .map(|(k, v)| Write {
            key: k.clone(),
            value: Some(v.clone()),
        })
        .chain(old.keys().filter(|k| !new.contains_key(*k)).map(|k| Write {
            key: k.clone(),
            value: None,
        }))
        .collect()
}
