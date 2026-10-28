// SPDX-License-Identifier: MPL-2.0
//! Per-profile Core state and its persistence. A profile persists as two opaque records in that profile's
//! encrypted store: `device` (deterministic CBOR: keys, sessions, ledger, relationship pins) and `app`
//! (versioned JSON: conversations, invites, operation log, network outbox). Nothing here is ever shared
//! between profiles, and there is no identifier linking two profiles (ADR-0148).

use crate::api::{DeliveryState, OperationId, OutcomeClass};
use crate::crypto::cbor::{self, V};
use crate::crypto::store::Device;
use crate::host::{CoreError, NetCall, Record, Write};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A profile persists as ONE record, so a single key/value put makes a whole transition atomic: the ratchet,
/// ledger and conversation state can never be stored out of step (the host store has no multi-key transaction).
pub const REC_STATE: &str = "state";
pub const APP_VERSION: u32 = 3;

pub type Hex = String;

/// Hex text of a private seed. Stored exactly like any other hex string (`#[serde(transparent)]`), but never printed
/// by `Debug` and wiped when dropped, so clones, staged copies and discarded states do not leave seeds behind in
/// freed memory (audit M3). The encoded profile record that is handed to the host still contains it: removing that
/// needs a vault-held record key inside Core and is recorded as remaining work.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretHex(String);

impl From<String> for SecretHex {
    fn from(s: String) -> Self {
        SecretHex(s)
    }
}

impl core::ops::Deref for SecretHex {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Debug for SecretHex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretHex(<redacted>)")
    }
}

impl zeroize::Zeroize for SecretHex {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for SecretHex {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(self);
    }
}

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
    pub request_seed: SecretHex,
    pub request_digest: Hex,
    /// Public key of this invite's intro identity (the signer is re-derived from the profile's `intro_root`).
    #[serde(default)]
    pub intro_pk: Hex,
    pub revoked: bool,
    pub used: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroRec {
    pub request_id: String,
    pub invite_operation_id: OperationId,
    pub person_id: String,
    /// The requester's self-chosen label (descriptor `subject_hint`): unverified, never an identity.
    pub display_name: String,
    /// The requester's intro public key. There is deliberately no account material here (ADR-0181).
    pub peer_intro_pk: Hex,
    pub peer_mailbox: Hex,
    /// The requester's request-scoped bootstrap delivery key (MessageRequestPayload `reply_delivery_key`).
    pub reply_key: PubKey,
}

/// Public half of a sealed-delivery key (`DeliveryKeyMaterial`), as hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubKey {
    pub key_id: Hex,
    pub public_key: Hex,
    pub route_binding: Hex,
    pub expires_at: u64,
}

/// A key we receive on: the private seed lives only in this profile's encrypted state.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryKeyRec {
    pub public: PubKey,
    pub seed: SecretHex,
}

impl core::fmt::Debug for DeliveryKeyRec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "DeliveryKeyRec({}, <redacted>)", self.public.key_id)
    }
}

/// The key we seal to for this peer. `bootstrap` keys are request-scoped (usable for the one Hello only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerDelivery {
    pub public: PubKey,
    pub bootstrap: bool,
}

/// Request-scoped bootstrap key we generated for a MessageRequest (ADR-0144/0146/0147).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapRec {
    pub key: DeliveryKeyRec,
    /// Fingerprints of distinct wrappers ever admitted for this request (at most `MAX_BOOTSTRAP_CANDIDATES`).
    pub candidates: Vec<Hex>,
}

impl core::fmt::Debug for BootstrapRec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BootstrapRec({:?})", self.key)
    }
}

/// ADR-0143 replay states. `RESERVED` is only ever visible inside one staged transaction: acceptance of the inner
/// envelope and `COMPLETE` are committed together, so a durable record is always `Complete` or `Rejected`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayState {
    Reserved,
    Complete,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayRec {
    pub fingerprint: Hex,
    pub state: ReplayState,
    /// Wrapper `expires_at` (seconds); the record is collected after this plus the skew.
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelState {
    /// We are the inviter and are setting up the session (fetching the peer's key material, sending Hello).
    Establishing,
    /// We are the invitee: the request is sent, the inviter has not yet accepted (no session exists).
    AwaitingAcceptance,
    /// The request was accepted but the two `IntroIdentityBinding`s have not both verified: no account is known
    /// (inviter side) or the inviter's Hello has not arrived (invitee side). Nothing but binding material moves.
    AcceptedUnbound,
    Active,
}

/// Intro-identity context of a first contact that is not yet bound (ADR-0181). Dropped once `Active`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingBind {
    /// Our intro public key for this invite.
    pub my_intro_pk: Hex,
    /// The counterpart's intro public key (committed by the link, or signed into the request).
    pub peer_intro_pk: Hex,
    /// Digest of the request key the request was sealed to; both bindings name it.
    pub request_digest: Hex,
    /// After this (wall ms) an unfinished first contact is dropped.
    pub deadline_ms: u64,
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
    /// The exact encrypted inner envelope, kept until the relay accepts it so a retry never re-runs the ratchet.
    #[serde(default)]
    pub envelope: Option<Hex>,
    /// SHA-256 of the canonical `ConversationEvent` that created this message (EventID equivocation check).
    #[serde(default)]
    pub digest: Hex,
    /// EventClock of the create or the latest accepted edit (epoch-free 1:1 last-writer-wins register).
    #[serde(default)]
    pub clock_ms: u64,
    #[serde(default)]
    pub clock_logical: u32,
    /// Tombstoned by a `message.delete` from the author; never shown, kept so the EventID stays deduplicated.
    #[serde(default)]
    pub deleted: bool,
    /// Canonical event bytes of an outgoing message still waiting for the peer's established delivery key.
    #[serde(default)]
    pub queued_event: Option<Hex>,
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
    /// Keys we receive on for this relationship (current first; older ones kept for bounded queued overlap).
    #[serde(default)]
    pub my_delivery: Vec<DeliveryKeyRec>,
    /// The established delivery key of the peer (or its bootstrap key until the peer's signed record arrives).
    #[serde(default)]
    pub peer_delivery: Option<PeerDelivery>,
    #[serde(default)]
    pub bootstrap: Option<BootstrapRec>,
    /// `"<delivery_key_id>:<delivery_id>"` -> replay record.
    #[serde(default)]
    pub replay: BTreeMap<String, ReplayRec>,
    /// Random 256-bit ConversationID chosen by the inviter and carried in its authenticated Hello.
    #[serde(default)]
    pub canon_id: Option<Hex>,
    /// Last EventClock issued locally (HLC).
    #[serde(default)]
    pub hlc_ms: u64,
    #[serde(default)]
    pub hlc_logical: u32,
    /// EventIDs seen with two different payloads: every version is rejected (EVENT_MODEL.md equivocation rule).
    #[serde(default)]
    pub equivocated: BTreeSet<Hex>,
    /// Present while the relationship is `AwaitingAcceptance` or `AcceptedUnbound` (and `Establishing` on the
    /// inviter's side until the Hello is out).
    #[serde(default)]
    pub pending_bind: Option<PendingBind>,
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
        /// SHA-256 digest of the intro public key the link committed to.
        intro_digest: Hex,
    },
    DepositRequest {
        operation_id: OperationId,
        conversation_id: String,
    },
    /// The inviter's Accept, sealed to the requester's request-scoped bootstrap key.
    DepositAccept {
        operation_id: OperationId,
        conversation_id: String,
    },
    /// The requester's Bind (its IntroIdentityBinding), sealed to the inviter's established key.
    DepositBind {
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
    /// Our signed delivery-key record, sent after the peer's Hello (no user operation owns it).
    DepositKeys {
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
    /// The signed AccountControlState chain `[genesis, ..., head]` (each a W0 SignedAccountControlState, hex).
    #[serde(default)]
    pub acs_chain: Vec<Hex>,
    /// Every DeviceCredential body this account has committed to, current and historical (hex).
    #[serde(default)]
    pub acs_credentials: Vec<Hex>,
    /// Local display data for devices (never authority).
    #[serde(default)]
    pub device_meta: BTreeMap<Hex, DeviceMeta>,
    /// Root of the per-invite intro identities (ADR-0181). Created on first use; never leaves the profile record.
    #[serde(default)]
    pub intro_root: Option<SecretHex>,
    /// Persisted first-contact limiter and its stranger-pressure counters.
    #[serde(default)]
    pub first_contact: FirstContactLimiter,
    /// Core-issued destructive-intent confirmation nonces (M2): single use, expiring.
    #[serde(default)]
    pub confirmations: Vec<ConfirmationRec>,
}

/// Admission accounting for requests arriving at invite mailboxes (ADR-0181 section 5). Wall-clock milliseconds,
/// guarded by `high_water_ms`, which never moves backwards, so a clock set back cannot reopen a window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstContactLimiter {
    /// Admission times of the last 24 hours (at most 100).
    pub admitted: Vec<u64>,
    pub high_water_ms: u64,
    /// Requests silently dropped since the notice window began.
    pub dropped: u32,
    pub last_drop_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationRec {
    pub operation_id: OperationId,
    pub nonce: Hex,
    pub intent_kind: String,
    pub target: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMeta {
    pub label: String,
    pub enrolled_at: u64,
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
        // Deterministic CBOR {0: device state or null, 1: app state JSON}.
        let device = self
            .device
            .as_ref()
            .map(|d| V::B(d.to_bytes()))
            .unwrap_or(V::Null);
        let app =
            serde_json::to_vec(&self.app).map_err(|_| CoreError::CorruptState("encode app"))?;
        let bytes = cbor::encode(&V::M(vec![(0, device), (1, V::B(app))]));
        Ok(BTreeMap::from([(REC_STATE.to_string(), bytes)]))
    }

    pub fn decode(generation: String, records: &[Record]) -> Result<Self, CoreError> {
        let mut st = ProfileState::empty(generation);
        let mut seen = BTreeSet::new();
        for r in records {
            if !seen.insert(r.key.as_str()) {
                return Err(CoreError::CorruptState("duplicate record"));
            }
            if r.key != REC_STATE {
                continue; // unknown records belong to a newer client; preserved by the host, ignored here
            }
            let corrupt = |_| CoreError::CorruptState("state record");
            let v = cbor::decode(&r.value).map_err(corrupt)?;
            let device = v.get(0).map_err(corrupt)?;
            if !device.is_null() {
                st.device =
                    Some(Device::from_bytes(device.b().map_err(corrupt)?).map_err(corrupt)?);
            }
            let mut app: serde_json::Value =
                serde_json::from_slice(v.get(1).and_then(|x| x.b()).map_err(corrupt)?)
                    .map_err(|_| CoreError::CorruptState("app state"))?;
            match app.get("version").and_then(|x| x.as_u64()) {
                Some(v) if v == u64::from(APP_VERSION) => {}
                Some(2) => migrate_v2(&mut app),
                _ => return Err(CoreError::CorruptState("unsupported app state version")),
            }
            st.app =
                serde_json::from_value(app).map_err(|_| CoreError::CorruptState("app state"))?;
        }
        Ok(st)
    }
}

/// Version 2 -> 3 (ADR-0181). First contact changed shape (intro identity, hidden requester account, a new link
/// format), so anything that cannot complete under the new flow is dropped: pending introductions, unused invites
/// (their old-format links are no longer valid), relationships that are not yet `active` and the outbox entries
/// that belonged to them. Operations whose network work is dropped end `Rejected(stale_state)`. `active`
/// relationships, messages, keys and the account are untouched.
fn migrate_v2(app: &mut serde_json::Value) {
    use serde_json::{Value, json};
    let Some(o) = app.as_object_mut() else { return };
    o.insert("version".into(), json!(APP_VERSION));
    o.insert("intros".into(), json!({}));
    if let Some(Value::Object(invites)) = o.get_mut("invites") {
        for inv in invites.values_mut() {
            if inv.get("used") != Some(&json!(true)) {
                inv["revoked"] = json!(true);
            }
        }
    }
    let mut dropped_convs: Vec<String> = vec![];
    if let Some(Value::Object(rels)) = o.get_mut("relationships") {
        rels.retain(|id, r| {
            let keep = r.get("state") == Some(&json!("active"));
            if !keep {
                dropped_convs.push(id.clone());
            }
            keep
        });
    }
    let mut stale_ops: Vec<String> = vec![];
    if let Some(Value::Array(outbox)) = o.get_mut("outbox") {
        outbox.retain(|e| {
            let p = &e["purpose"];
            let kind = p.get("purpose").and_then(|x| x.as_str()).unwrap_or("");
            let conv = p
                .get("conversationId")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let drop = matches!(
                kind,
                "resolve_invite"
                    | "put_invite"
                    | "deposit_request"
                    | "fetch_peer_keys"
                    | "deposit_hello"
            ) || (kind == "deposit_keys" && dropped_convs.iter().any(|c| c == conv));
            if drop && let Some(op) = p.get("operationId").and_then(|x| x.as_str()) {
                stale_ops.push(op.to_string());
            }
            !drop
        });
    }
    if let Some(Value::Object(ops)) = o.get_mut("ops") {
        for op in stale_ops {
            if let Some(r) = ops.get_mut(&op) {
                r["outcome"] = json!({"kind": "Rejected", "reason": "stale_state"});
            }
        }
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
