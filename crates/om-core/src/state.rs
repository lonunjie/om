// SPDX-License-Identifier: MPL-2.0
//! Per-profile Core state and its persistence. A profile persists as two opaque records in that profile's
//! encrypted store: `device` (deterministic CBOR: keys, sessions, ledger, relationship pins) and `app`
//! (versioned JSON: conversations, invites, operation log, network outbox). Nothing here is ever shared
//! between profiles, and there is no identifier linking two profiles (ADR-0148).

pub use crate::flows::preferences::Preferences;

use crate::api::{DeliveryState, OperationId, OutcomeClass};
use crate::crypto::cbor::{self, V};
use crate::crypto::store::Device;
use crate::host::{CoreError, NetCall, Record, Write};
use crate::relayset::Lineage;
use crate::route::{RelayEntry, RelaySet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The ratchet, ledger and conversation state share one record so a single key/value write
/// keeps them atomic. Local pack activation independently writes its own record.
pub const REC_STATE: &str = "state";
pub(crate) const REC_EXPRESSIVE_PACKS: &str = "expressive-packs";
pub const APP_VERSION: u32 = 5;

pub type Hex = String;

/// Secret text persisted in the encrypted profile record. The wrapper preserves the string representation,
/// redacts debug output and zeroizes its owned allocation on drop.
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

/// A replacement recovery key that is shown but not yet confirmed saved (ADR-0205). Nothing of it is in force: the account's
/// recovery policy, the kit and the record change only when the person confirms the key is stored. Dropped on confirmation, on
/// a newer replacement and on any failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecovery {
    pub key_operation_id: OperationId,
    /// The verified head captured by the sheet.
    #[serde(default)]
    pub parent: Hex,
    #[serde(default)]
    pub parent_epoch: Option<u64>,
    #[serde(default)]
    pub controllers: Vec<Hex>,
    #[serde(default)]
    pub required_votes: usize,
    #[serde(default)]
    pub child_hash: Hex,
    /// The `RecoveryPolicy` body the new root's method key belongs to (epoch one above the current one).
    pub policy: Hex,
    /// The type-38 bytes of the kit for the new root, when the account has a recovery place.
    pub kit: Option<SecretHex>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyRec {
    pub key_operation_id: OperationId,
    pub groups: Vec<SecretHex>,
    /// The key sheet text (sheet form, ADR-0200) shown with the key when this Home gave a recovery place; absent
    /// otherwise. Shown once, dropped with the key when it is confirmed saved.
    #[serde(default)]
    pub sheet: Option<SecretHex>,
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
    /// The introduction service the link and the descriptor name (the inviter's own Home); absent on an invite made
    /// before introduction services existed, which is then polled as a plain mailbox.
    #[serde(default)]
    pub intro_service: Option<String>,
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
    /// Where the requester receives (MessageRequestPayload `reply_route`).
    pub peer_route: RelayEntry,
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

/// ADR-0143 replay states. A stale-head wrapper can remain durably `Reserved` until K7 refresh (ADR-0144).
/// Acceptance of the inner envelope and `Complete` are committed together.
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
    /// Delivery of this message to the other devices of the conversation (the peer's devices besides the first-contact
    /// one, and our own other devices). Empty for a one-device-per-side conversation.
    #[serde(default)]
    pub fan: Vec<Fan>,
    /// Attachments of a `message.create` (outgoing: with the secrets needed to upload and delete them).
    #[serde(default)]
    pub attachments: Vec<AttachRec>,
    /// Canonical bytes (hex) of the create's key-23 extensions map, kept so the presentation is derived from what was
    /// authenticated and re-checked against the message as it currently reads.
    #[serde(default)]
    pub ext: Option<Hex>,
    /// A group relationship-lane event (kinds 15 to 17, ADR-0189): sent and delivered like a message but never shown
    /// in the timeline, and never counted as the last message.
    #[serde(default)]
    pub lane: bool,
    /// Shared conversation context of the message (reply link; see [`Social`]).
    #[serde(default)]
    pub social: Social,
}

/// What other events say about a message: the message it replies to and the reactions on it. Never part of the canonical create bytes beyond `reply_to`; a view is derived from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Social {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expressive_send: Option<crate::expressive::Pending>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) inline_assets: Vec<SecretHex>,
    /// `reply_to` of the create: the EventID (hex) of a message in the same conversation (EVENT_MODEL `message.create`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Hex>,
    /// One register per (actor, reaction key): the latest `reaction.set` by `(EventClock)` wins, a removal is kept as a
    /// tombstone so a late older add cannot revive it (EVENT_MODEL `reaction.set`; 1:1 has one epoch, so clock order alone).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionRec>,
    /// An accepted `message.edit` replaced the text (EVENT_MODEL: the "edited" marker).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    /// Mentions of the create, as character ranges of the text (ADR-0199). Empty when the create carried none or its
    /// mention set disagreed with its text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<MentionRec>,
}

/// One mention inside a message text: the half-open character range `[start, end)` and the mentioned account (hex).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionRec {
    pub start: u32,
    pub end: u32,
    pub account: Hex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionRec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) asset: Option<SecretHex>,
    /// Whether the actor is us (any of our devices) rather than the other person.
    pub mine: bool,
    pub key: String,
    pub present: bool,
    pub clock_ms: u64,
    pub clock_logical: u32,
}

/// One attachment of a message. `reference` is the canonical `attachment-ref` and contains the content key, so it is
/// held like a secret; for an outgoing message it is completed (locations, read capability) when the BlobStore's receipt
/// arrives, and only then is the message event built from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachRec {
    pub blob_id: Hex,
    pub reference: SecretHex,
    /// Outgoing only: the owner capability the sender chose (it alone can delete) and the idempotency id of the put.
    #[serde(default)]
    pub owner_cap: Option<SecretHex>,
    #[serde(default)]
    pub request_id: Option<Hex>,
    /// Outgoing only: the BlobStore stored it and its receipt is applied.
    #[serde(default)]
    pub stored: bool,
    /// Outgoing only: where it is stored and until when (wall ms, the store's own word; a display hint).
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub expires_ms: u64,
    #[serde(default)]
    pub ciphertext_len: u64,
    /// Outgoing only: the object is offered directly (no BlobStore copy): it has no owner capability, is never put to a
    /// store, and its reference carries a read capability and no locations. Its offer is in `AppState.direct_offers`.
    #[serde(default)]
    pub direct: bool,
}

/// Where a direct offer stands. Local receipt is not durability (AT-06). `PeerClaim` is only the other device's own
/// say-so that it has the last byte (ADR-0194); `Confirmed` needs an authenticated receipt and still means that device only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectStatus {
    /// Offered; served whenever the recipient asks while this device is online.
    Waiting,
    /// The peer acknowledged every byte; unchecked. Also the meaning of a record stored as `delivered` before ADR-0194:
    /// it is migrated to the claim on load and is never promoted to `Confirmed`.
    #[serde(alias = "delivered")]
    PeerClaim,
    /// A valid receipt for the transfer arrived (ADR-0194).
    Confirmed,
    /// The person took it back; nothing is served any more.
    Withdrawn,
    /// Not fetched in time.
    Expired,
    /// The offer lapsed while it was only a claim: nothing proved the peer has the file.
    Unconfirmed,
}

impl DirectStatus {
    /// Still served (and rearmed after a restart): a claim is kept so the exchange can run again and be confirmed.
    pub fn is_live(self) -> bool {
        matches!(self, DirectStatus::Waiting | DirectStatus::PeerClaim)
    }
}

/// The persisted side of a direct offer, keyed by BlobID. It holds no secret and no bytes: the sealed object is rebuilt
/// from the person's original file (`Core::arm_direct_offer`) after a restart, and the reference with the key and read
/// capability lives in the message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectOfferRec {
    pub conversation_id: String,
    pub message_id: String,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub status: DirectStatus,
}

/// An attachment the host prepared but no message uses yet. Nothing external has happened to it; it is dropped when
/// unused for a day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedBlob {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expressive: Option<crate::expressive::Presentation>,
    pub rec: AttachRec,
    pub created_ms: u64,
}

/// One device's copy of a message (K5/K6: every eligible device of the recipient, plus our own other devices).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fan {
    pub device_id: Hex,
    /// A device of our own account (self-sync) rather than of the peer's.
    pub sibling: bool,
    pub state: FanState,
    /// Canonical event bytes, kept while the copy is waiting for a session or route, or until the device answered on
    /// the session it was sent on (so an initiation that loses a crossed race can be re-sent on the winner).
    #[serde(default)]
    pub event: Option<Hex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanState {
    /// No session or route for the device yet; sent the moment there is one (never dropped silently).
    Waiting,
    /// Sealed and in the outbox.
    Queued,
    /// The relay accepted the deposit.
    Sent,
}

/// A device of a conversation other than the first-contact peer device: its own routes, delivery keys and fetch
/// capability. The first-contact peer device stays in the `peer_*` fields of the relationship.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerDev {
    pub device_id: Hex,
    pub ik: Hex,
    pub relays: RelaySet,
    pub lineage: Lineage,
    pub delivery: Option<PeerDelivery>,
    pub keys: Vec<PeerDelivery>,
    pub route_health: BTreeMap<Hex, u64>,
    /// Where and with what capability we fetch this device's key bundle, when we hold one (then we initiate).
    #[serde(default)]
    pub fetch_service: Option<String>,
    #[serde(default)]
    pub fetch_cap: Hex,
    /// A bundle fetch for this device is in the outbox.
    #[serde(default)]
    pub fetching: bool,
}

/// Per-conversation multi-device state (additive to the first-contact fields of [`Relationship`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Multi {
    /// The peer account's devices besides the first-contact one.
    pub extra: BTreeMap<Hex, PeerDev>,
    /// Our own account's other devices (self-sync targets).
    pub siblings: BTreeMap<Hex, PeerDev>,
    /// The capability with which we fetch the first-contact peer device's bundle (held by the inviter), if any.
    pub primary_fetch_service: Option<String>,
    pub primary_fetch_cap: Hex,
    /// Some message has a copy waiting for a session or route.
    pub waiting: bool,
    /// In-memory only: the peer fields of the relationship currently hold another device's side (see `with_dev`).
    #[serde(skip)]
    pub in_view: bool,
}

/// A mailbox this install polls for one relationship, and the route peers deposit to for it (one per own RelayEntry).
/// A route the RelaySet dropped stays here, still polled, until its overlap ends (add/overlap/ack/drain/revoke).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyBox {
    pub mailbox: Hex,
    pub route: RelayEntry,
    /// When a published version dropped this route; `None` while it is part of the set we publish.
    #[serde(default)]
    pub retired_at_ms: Option<u64>,
    /// Polling stops at this wall time whatever else happens (the overlap bound).
    #[serde(default)]
    pub drain_until_ms: u64,
    /// The RelaySet version that dropped this route; the peer acknowledges by naming a version at or above it.
    #[serde(default)]
    pub retired_version: u64,
    /// The peer has acknowledged a RelaySet version that no longer names this route (a signed message in the
    /// relationship, so only the peer can say it).
    #[serde(default)]
    pub acked: bool,
    /// Overlap over: no longer polled and nothing arriving for its keys is accepted.
    #[serde(default)]
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub person_id: String,
    pub display_name: String,
    /// The owner's nickname (ADR-0196/0215): account-private display state, shown in place of `display_name`.
    #[serde(default)]
    pub nickname: Option<String>,
    pub state: RelState,
    pub peer_account_id: Hex,
    pub peer_device_id: Hex,
    pub peer_ik: Hex,
    pub peer_acs_body: Hex,
    pub peer_credentials: Vec<Hex>,
    /// The mailbox id this install first polled for this relationship (kept for reading older records; every mailbox we
    /// poll is in `my_boxes`).
    pub my_mailbox: Hex,
    /// The entries of our own effective RelaySet: where peers deposit. Every delivery key we receive on is bound to one.
    pub my_relays: RelaySet,
    /// Where the peer receives: the entries of its effective RelaySet; empty until its route is known.
    pub peer_relays: RelaySet,
    /// Every mailbox we poll for this relationship, with its route (the published ones and those draining).
    #[serde(default)]
    pub my_boxes: Vec<MyBox>,
    /// The signed lineage of the RelaySet we publish to this peer.
    #[serde(default)]
    pub my_lineage: Lineage,
    /// The signed lineage of the RelaySet this peer published to us. Empty for a peer that sent none (its one route
    /// from the Accept or the request stands as an implicit set until it publishes a signed one).
    #[serde(default)]
    pub peer_lineage: Lineage,
    /// Established delivery keys of the peer for routes other than the one in `peer_delivery`.
    #[serde(default)]
    pub peer_keys: Vec<PeerDelivery>,
    /// Route binding (hex) -> wall time until which a sender treats the route as unhealthy after a failover.
    #[serde(default)]
    pub route_health: BTreeMap<Hex, u64>,
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
    /// Sealed wrappers whose inner envelope came from a device the pinned peer head does not list yet (a newly linked
    /// device). ADR-0144: indeterminate, not rejected. They wait here, unprocessed, until an authenticated peer head
    /// refresh says what they are (K12, ONE_TO_ONE_SESSION_MANAGEMENT section 3).
    #[serde(default)]
    pub held: Vec<HeldWrapper>,
    /// Wall time of the last head request sent for this relationship (rate limit; never authority).
    #[serde(default)]
    pub head_asked_ms: u64,
    /// Our own head changed and the peer has not been sent it yet; retried until it is out.
    #[serde(default)]
    pub head_dirty: bool,
    /// When we last served the peer a head update (rate limit for its requests).
    #[serde(default)]
    pub head_sent_ms: u64,
    #[serde(default)]
    pub multi: Multi,
    /// Recoveries of the peer's account this contact is tracking (ADR-0190 section 4; at most two). Local, never sent.
    /// Inert history after a restore from backup: a restore carries no relationship, so none of it is revived.
    #[serde(default)]
    pub peer_recovery: Vec<PeerRecoveryRec>,
    /// Evidence for each recovery link of the peer's chain this verifier accepted, with THIS verifier's credited
    /// seconds. Local: the dossier a verifier forwards never contains it (ADR-0190 section 8).
    #[serde(default)]
    pub peer_links: Vec<RecoveryLinkRec>,
    /// A returning peer's original local thread, proven by the invitation binding.
    #[serde(default)]
    pub relink: Option<RelationshipRelink>,
    #[serde(default)]
    pub relinked_from: Option<String>,
    /// A contact selected for this recovery invitation. The binding must authenticate this account.
    #[serde(default)]
    pub reconnect_account: Option<Hex>,
    /// One sealed invitation response held while the existing contact verifies recovery.
    #[serde(default)]
    pub recovery_bind: Option<RecoveryBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipRelink {
    pub conversation_id: String,
    pub previous_head: Hex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBinding {
    pub account_id: Hex,
    pub bytes: Hex,
}

/// A sealed wrapper held while the peer's control head is refreshed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldWrapper {
    pub bytes: Hex,
    pub since_ms: u64,
}

/// A conversation brought back from a backup file (ADR-0184). It is history only: no relationship, key, route or outbox
/// entry exists for it, so it can be shown and re-exported and can never send, receive or act with authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredChat {
    pub id: String,
    pub display_name: String,
    /// See [`Relationship::nickname`]; restored from a version 2 backup body (ADR-0196).
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub peer_account_id: Hex,
    pub created_at: u64,
    #[serde(default)]
    pub canon_id: Option<Hex>,
    pub restored_at: u64,
    pub messages: Vec<RestoredMsg>,
}

/// `delivery`: 0 never left the device it was written on, 1 sent, 2 delivered, 3 refused (see `backup::ArchivedMsg`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredMsg {
    pub id: String,
    pub outgoing: bool,
    pub text: String,
    pub sent_at: u64,
    pub delivery: u8,
    #[serde(default)]
    pub clock_ms: u64,
    #[serde(default)]
    pub clock_logical: u32,
    #[serde(default)]
    pub digest: Hex,
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
    /// The RecoveryRecord write of this generation (outcome is the Home's `ETag`).
    RecoveryPut {
        generation: u64,
    },
    RecoveryDelete {
        locator: Hex,
    },
    /// The read that settles a refused compare-and-swap.
    RecoveryReconcile {
        generation: u64,
    },
    /// A recovering device reads the account's record (owner recovery, ADR-0199 section 4).
    RecoveryFind,
    /// The owner's request delivered to one Home (`RecoveryRequestPost`); nothing awaits the answer.
    OwnerRecoveryPost,
    /// A controller's inbox call: 0 register, 1 drain, 2 acknowledge.
    RecoveryInbox {
        kind: u8,
    },
    /// A read of the owner's attempt: a veto board (`kind` 1) or the record again before adoption (`kind` 0). `round`
    /// names the round so a late answer to an earlier one counts for nothing.
    OwnerRecoveryRead {
        kind: u8,
        round: u32,
    },
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
    /// The requester lets the inviter fetch its device bundle (relationship fetch capability).
    GrantKeyFetch {
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
    /// Our AccountControl head, or a request for the peer's, sent inside the relationship (no user operation owns it).
    DepositAcs {
        conversation_id: String,
    },
    DepositPreference {
        conversation_id: String,
        device_id: String,
    },
    /// Our next signed RelaySet version with the delivery keys of its routes, sent inside the relationship.
    /// A `RecoveryNotice` sealed to a contact's route (ADR-0190 3.1; no user operation owns it).
    DepositRecoveryNotice {
        conversation_id: String,
    },
    DepositRelaySet {
        conversation_id: String,
    },
    DepositMessage {
        operation_id: OperationId,
        conversation_id: String,
        message_id: String,
    },
    /// Upload one attachment's ciphertext to the BlobStore (the host holds the bytes). The message is built and sent
    /// only after every one of its blobs is stored.
    PutBlob {
        operation_id: OperationId,
        conversation_id: String,
        message_id: String,
        blob_id: Hex,
    },
    /// Remove a blob of ours the BlobStore holds for a message that will never be sent. Best effort; it expires anyway.
    DeleteBlob {
        blob_id: Hex,
    },
    /// A message copy for another device of the conversation (the peer's or our own).
    DepositFan {
        operation_id: OperationId,
        conversation_id: String,
        message_id: String,
        device_id: Hex,
    },
    /// Device-link ceremony (D24): the existing device opens its response queue and learns the receive capability.
    OpenCeremonyQueue {
        operation_id: OperationId,
    },
    /// Device-link ceremony: a sealed message on the ceremony's request or response queue.
    DepositCeremony {
        operation_id: OperationId,
        /// The approval message itself (not a roster entry or a history chunk): without it the new device holds nothing.
        #[serde(default)]
        approval: bool,
    },
    /// The peer's other device's bundle, fetched to start a session with it (we hold its fetch capability).
    FetchDeviceKeys {
        conversation_id: String,
        device_id: Hex,
    },
    /// A recovery veto (stub transport): nothing awaits its result.
    RecoveryFanout,
    /// Best-effort deletion of the previous signed-prekey generation at the Home, queued ahead of the rotation's publish.
    RevokeKeyGeneration,
    Ack,
}

impl Purpose {
    pub(crate) fn conversation_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::DepositRequest {
                conversation_id, ..
            }
            | Self::GrantKeyFetch { conversation_id }
            | Self::DepositAccept {
                conversation_id, ..
            }
            | Self::DepositBind { conversation_id }
            | Self::FetchPeerKeys {
                conversation_id, ..
            }
            | Self::DepositHello {
                conversation_id, ..
            }
            | Self::DepositKeys { conversation_id }
            | Self::DepositAcs { conversation_id }
            | Self::DepositPreference {
                conversation_id, ..
            }
            | Self::DepositRecoveryNotice { conversation_id }
            | Self::DepositRelaySet { conversation_id }
            | Self::DepositMessage {
                conversation_id, ..
            }
            | Self::PutBlob {
                conversation_id, ..
            }
            | Self::DepositFan {
                conversation_id, ..
            }
            | Self::FetchDeviceKeys {
                conversation_id, ..
            } => Some(conversation_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub id: u64,
    pub call: NetCall,
    pub purpose: Purpose,
    /// Consecutive attempts that made no progress (the Home was unreachable or its answer unknown).
    #[serde(default)]
    pub fails: u32,
    /// Present on a deposit of a session-sealed inner envelope: what is needed to re-seal the very same inner envelope
    /// for another of the peer's routes when this one makes no progress (sender failover; same EventID, fresh wrapper).
    #[serde(default)]
    pub failover: Option<Failover>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failover {
    pub conversation_id: String,
    pub inner: Hex,
    /// Route bindings already tried in this round.
    pub tried: Vec<Hex>,
    /// The conversation device this deposit is addressed to (`None`: the first-contact peer device).
    #[serde(default)]
    pub device: Option<Hex>,
}

/// The staged write of the RecoveryRecord: sealed once, and resent with exactly these bytes until the Home answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedPut {
    pub generation: u64,
    pub request_id: Hex,
    pub envelope: Hex,
    pub if_match: Option<String>,
    pub staged_at_ms: u64,
}

/// Writer bookkeeping for the RecoveryRecord (ADR-0199 section 6).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryWriter {
    /// Set when the person confirmed the recovery key is stored: the first write follows.
    pub enabled: bool,
    /// The current recovery key may still be held by an excluded controller.
    #[serde(default)]
    pub paused_for_exclusion: bool,
    /// The checkpoint put in the saved sheet. Legacy kits use the account's first state.
    #[serde(default)]
    pub sheet_anchor: Option<CheckpointRec>,
    #[serde(default)]
    pub recovered_with_sheet: bool,
    /// Hash (hex) of the ACS head the writer last saw.
    pub head_hash: Option<Hex>,
    pub head_seen_ms: u64,
    /// This device authored the head it has not yet written for (set where an ACS state is signed).
    pub authored: bool,
    /// A head this device did not author is awaiting the steward's catch-up delay.
    pub steward_pending: bool,
    /// Last refresh slot used for the current head (0 = the author's slot).
    pub refresh: u32,
    pub staged: Option<StagedPut>,
    /// Highest generation the Home acknowledged, and the ETag it gave (the next compare-and-swap).
    pub acked_generation: u64,
    pub acked_etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked_head_hash: Option<Hex>,
    #[serde(default)]
    pub record_conflict: bool,
    pub last_write_ms: u64,
    /// SHA-256 (hex) of the relationship snapshot last written.
    pub last_content: Option<Hex>,
}

/// A retired record's provider-scoped deletion, held until the replacement is acknowledged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetiredRecoveryRecord {
    pub place: JoinPlace,
    pub locator: Hex,
    pub write_cap: SecretHex,
    pub replacement_generation: u64,
    pub expires_ms: u64,
}

/// What Settings shows about the recovery record (ADR-0199 section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryRecordStatus {
    Off,
    FinishingSetup,
    Ready,
    NeedsAttention,
    NeedsNewKey,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(
        default,
        skip_serializing_if = "crate::events::DanglingMutations::is_empty"
    )]
    pub dangling_mutations: crate::events::DanglingMutations,
    /// Device-local call preferences and replay tombstones. Neither enters account sync or history export.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub calls_disabled: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_offers_seen: Vec<crate::calls::SeenOffer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_operations: Vec<OperationId>,
    /// Bounded device-local call timeline, excluded from account sync and history export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_notices: Vec<crate::calls::CallNotice>,
    pub version: u32,
    pub label: String,
    pub created_at: u64,
    pub recovery_key: Option<RecoveryKeyRec>,
    /// A replacement for the recovery key, shown and not yet confirmed (ADR-0205).
    #[serde(default)]
    pub pending_recovery: Option<PendingRecovery>,
    pub recovery_key_saved: bool,
    /// The device-private `RecoveryKit` (hex of its type-38 bytes). Never in a backup, never sent to a Home.
    #[serde(default)]
    pub recovery_kit: Option<SecretHex>,
    #[serde(default)]
    pub recovery_writer: RecoveryWriter,
    /// The public RecoveryPolicy body (hex) whose hash the genesis commits to (ACS key 10). Absent on a profile made
    /// before recovery policies existed, which stays `pending`.
    #[serde(default)]
    pub recovery_policy: Option<Hex>,
    pub invites: BTreeMap<OperationId, InviteRec>,
    pub accepted_invites: BTreeSet<Hex>,
    pub intros: BTreeMap<String, IntroRec>,
    pub relationships: BTreeMap<String, Relationship>,
    #[serde(default)]
    pub preferences: Preferences,
    /// History restored from a backup file: read-only, additive to version 4 (absent in older records).
    #[serde(default)]
    pub restored: BTreeMap<String, RestoredChat>,
    pub ops: BTreeMap<OperationId, OpRec>,
    pub outbox: Vec<OutboxEntry>,
    pub next_net_id: u64,
    /// Mailbox (hex) -> highest relay sequence already processed and committed.
    pub cursors: BTreeMap<Hex, u64>,
    pub keys_published: bool,
    /// The signed AccountControlState chain ending at the live head (hex).
    #[serde(default)]
    pub acs_chain: Vec<Hex>,
    /// Independent local anchor when owner recovery began with a non-genesis chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acs_checkpoint: Option<CheckpointRec>,
    /// Every DeviceCredential body this account has committed to, current and historical (hex).
    #[serde(default)]
    pub acs_credentials: Vec<Hex>,
    /// Verified competing sibling branches. The pinned branch remains in acs_chain.
    #[serde(default)]
    pub acs_forks: BTreeMap<Hex, (Vec<Hex>, Vec<Hex>)>,
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
    /// Device-link ceremonies this (existing) device offered, by operation id (D24).
    #[serde(default)]
    pub link_offers: BTreeMap<OperationId, LinkOffer>,
    /// The ceremony this (new) device is waiting on; while present, the profile is a device that has not joined yet.
    #[serde(default)]
    pub link_join: Option<LinkJoin>,
    /// Recovery approval ceremonies this (surviving controller) device offered (ADR-0203), by operation id.
    #[serde(default)]
    pub approval_offers: BTreeMap<OperationId, ApprovalOffer>,
    /// Ceremony evidence the sender repeats bit-identically (ADR-0226 section 5), by operation id.
    #[serde(default)]
    pub ceremony_repeats: BTreeMap<OperationId, EvidenceRepeat>,
    #[serde(default)]
    pub controller_change: Option<ControllerChange>,

    /// Enrollment notices received for devices that joined our account (newest last, bounded).
    #[serde(default)]
    pub notices: Vec<NoticeRec>,
    /// Enrollment notices owed to other devices of this account that have not acknowledged them (ADR-0193). A
    /// separate store from `notices`, so the display trim never evicts one. Empty in older records.
    #[serde(default)]
    pub owed_notices: Vec<OwedNotice>,
    /// Acknowledgements this device still owes to the devices that sent it a notice (their reply had no route yet).
    #[serde(default)]
    pub owed_acks: Vec<OwedAck>,
    /// Strict reading (ADR-0193 section 8), device-local, default off: approve a link only when every other active
    /// device has a deliverable path at that moment.
    #[serde(default)]
    pub link_strict_notice: bool,
    /// Attachments prepared but not yet sent, by BlobID (hex).
    #[serde(default)]
    pub prepared: BTreeMap<Hex, PreparedBlob>,
    /// Recovery requests this verifier has observed, by request digest (hex), bounded. Additive: absent in older
    /// records. The verifier-local interval lives here (ADR-0131/0147/0154).
    #[serde(default)]
    pub recovery_requests: BTreeMap<Hex, RecoveryObsRec>,
    /// Evidence for each completed recovery link in `acs_chain` (policy, request and the interval THIS verifier
    /// credited), so the chain can be replayed. Local, never sent to a peer. Additive.
    #[serde(default)]
    pub recovery_links: Vec<RecoveryLinkRec>,
    /// Set on a profile that has no device or account yet and is waiting to recover one with a recovery key: the
    /// restricted recovery state (RECOVERY_AND_BACKUP_POLICY.md 10.3). Additive.
    #[serde(default)]
    pub recovery_join: Option<RecoveryJoinRec>,
    /// The controller side of the recovery inbox (ADR-0201 section 8.2). Convenience only: nothing in it is trusted.
    #[serde(default)]
    pub recovery_inbox: RecoveryInboxRec,
    /// Drain-only capabilities of superseded epochs (ADR-0226 §1). Kept in the DeviceVault.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub earlier_recovery_inboxes: BTreeMap<u64, EarlierRecoveryInbox>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub earlier_recovery_code_used: bool,
    /// Record floors by scope and locator. A scope-only key is an unreconciled legacy floor (ADR-0225 §3).
    #[serde(default)]
    pub recovery_floors: BTreeMap<Hex, RecordFloorRec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retired_recovery_records: Vec<RetiredRecoveryRecord>,
    /// Advisory reconnect work staged with adoption, scoped to this encrypted profile.
    #[serde(default)]
    pub relationship_restore: Option<RelationshipRestore>,
    /// Earlier local messages of a re-linked thread. Display only, outside the live event reducer.
    #[serde(default)]
    pub relationship_history: BTreeMap<String, Vec<Msg>>,
    /// This person's own privacy level, when set. Hardened and Maximum make a contact confirm a recovery of someone
    /// else's account before accepting it; `None` is Recommended (ADR-0190 section 4). Additive.
    #[serde(default)]
    pub privacy_level: Option<crate::api::PrivacyPolicyLevel>,
    /// Ordinary groups (scope `groups`, ADR-0177): the head `GroupControlState` per ConversationID (hex). Additive:
    /// absent in older records.
    #[serde(default)]
    pub groups: BTreeMap<Hex, crate::groups::GroupRec>,
    /// Group crypto state (slice 2): this device's MLS engine snapshot, an OM-owned tagged record. Absent in older
    /// records.
    #[serde(default)]
    pub group_crypto: Option<crate::groups::store::GroupCryptoRec>,
    /// ConversationID (hex) -> the MLS `group_id` (hex), which is random and unrelated to it.
    #[serde(default)]
    pub group_mls_ids: BTreeMap<Hex, Hex>,
    /// The ADR-0163 commit journal, by intent id (hex). Terminal entries are pruned to a short tail.
    #[serde(default)]
    pub group_journal: BTreeMap<Hex, crate::groups::journal::JournalEntry>,
    /// ConversationID (hex) -> the last accepted Commit's `mls_commit_hash` (hex): the idempotency marker.
    #[serde(default)]
    pub group_accepted: BTreeMap<Hex, Hex>,
    /// Group invitations in both directions by invite id (hex) (slice 3). Additive.
    #[serde(default)]
    pub group_invites: BTreeMap<Hex, crate::groups::flow::GroupInviteRec>,
    /// ConversationID (hex) -> device id (hex) -> its account and device public key, as this device learned them. Additive.
    #[serde(default)]
    pub group_devices: BTreeMap<Hex, BTreeMap<Hex, crate::groups::flow::GroupDeviceRec>>,
    /// ConversationID (hex) -> the group's text messages as this device holds them. Additive.
    #[serde(default)]
    pub group_msgs: BTreeMap<Hex, Vec<crate::groups::flow::GroupMsg>>,
    /// Room attachment secrets are held separately from the message projection returned to the host.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_attachments: BTreeMap<Hex, BTreeMap<Hex, crate::groups::flow::RoomAttachment>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub room_uploads: BTreeMap<OperationId, crate::flows::RoomUpload>,
    /// ConversationID (hex) -> the last event clock (physical ms, logical) this device gave an event it sent in the Room. Additive.
    #[serde(default)]
    pub group_hlc: BTreeMap<Hex, (u64, u32)>,
    /// Group carriage (ADR-0189): this device's per-Room routes, other devices' verified routes and the lane inbox.
    #[serde(default)]
    pub group_carriage: crate::groups::carriage::GroupCarriageRec,
    /// ConversationID (hex) -> announced changes of the Room's settings, oldest first (bounded). Additive.
    #[serde(default)]
    pub group_notices: BTreeMap<Hex, Vec<crate::groups::room::Notice>>,
    /// ConversationID (hex) -> Room settings still to be carried out, one Commit at a time. Additive.
    #[serde(default)]
    pub group_room_queue: BTreeMap<Hex, Vec<crate::groups::room::Setting>>,
    /// Groups this person has asked to leave and whose removing Commit has not arrived: sending is refused. Additive.
    #[serde(default)]
    pub group_leaving: BTreeSet<Hex>,
    /// Direct-transfer offers by BlobID (hex): large files offered while the other person is online (group G).
    /// Additive: absent in older records.
    #[serde(default)]
    pub direct_offers: BTreeMap<Hex, DirectOfferRec>,
}

/// The restricted state of a recovering device before any account is known to it. It holds the RecoveryRoot (the
/// secret that signs the request later) and the Home address the person gave; it holds no authority and cannot send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryJoinRec {
    pub operation_id: OperationId,
    pub root: SecretHex,
    pub home: String,
    pub started_ms: u64,
    /// Where the RecoveryStore is (from the sheet, or a place string entered beside a key-only sheet); absent until known.
    #[serde(default)]
    pub place: Option<JoinPlace>,
    /// Redirects followed so far and the (host, scope) pairs visited (ADR-0200 section 6).
    #[serde(default)]
    pub visited: Vec<(String, Hex)>,
    #[serde(default)]
    pub stage: JoinStage,
    /// The padded plaintext of the verified RecoveryRecord (type 36), kept while the recovery is built from it.
    #[serde(default)]
    pub record: Option<SecretHex>,
    /// The owner checkpoint the sheet carried (version 2, ADR-0203 section 1): a minimum known head the owner kept with
    /// the recovery material. Its custody is its only provenance; nothing a Home or the record says replaces it.
    #[serde(default)]
    pub checkpoint: Option<CheckpointRec>,
    /// What the record's evidence amounts to against the checkpoint (ADR-0202 section 1). Advisory data alone is never
    /// enough to adopt anything.
    #[serde(default)]
    pub evidence: EvidenceStage,
    /// The attempt this device started (`RequestRecovery`): tentative device, exact signed request and successor, the
    /// verifier-local timer, vetoes and everything adoption re-checks. Persisted before any use.
    #[serde(default)]
    pub attempt: Option<OwnerAttemptRec>,
    #[serde(default)]
    pub terminal: Option<AttemptState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_floors: Option<CancelledRecoveryFloors>,
}

/// Floor keys survive cancellation without preserving the recovery secret (ADR-0225 §3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledRecoveryFloors {
    pub keys: BTreeSet<Hex>,
    pub last_sample: Option<crate::host::MonotonicSample>,
    pub credited_ms: u64,
}

/// A controller's recovery inbox bookkeeping. Items wait here for the next tick (which carries the monotonic reading the
/// observations need) and are acknowledged once handed to the verifier. Bounded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInboxRec {
    #[serde(default)]
    pub epoch: Option<u64>,
    #[serde(default)]
    pub registered_at_ms: Option<u64>,
    #[serde(default)]
    pub drained_at_ms: Option<u64>,
    #[serde(default)]
    pub registration_refused: bool,
    /// Core's clock at the last register or drain attempt: pacing only, never authority.
    pub last_ms: u64,
    pub in_flight: bool,
    pub pending: Vec<InboxItemRec>,
    pub to_ack: Vec<Hex>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarlierRecoveryInbox {
    pub host: String,
    pub port: Option<u16>,
    pub cap: SecretHex,
    pub verification_key: Hex,
    pub ended_ms: u64,
    pub last_drain_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemRec {
    pub id: Hex,
    pub kind: u8,
    pub bytes: Hex,
    #[serde(default)]
    pub home: Option<(String, Option<u16>)>,
}

/// An `OwnerRecoveryCheckpoint` as kept locally. All hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRec {
    pub account_id: Hex,
    pub generation: u64,
    pub acs_hash: Hex,
}

/// An opaque locator's AccountID binding, highest accepted generation and authenticated plaintext digest.
/// The persisted field `lineage` retains its original encoding for migration of scope-only entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordFloorRec {
    pub lineage: Hex,
    pub generation: u64,
    pub digest: Hex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerHeadObservation {
    pub head: CheckpointRec,
    pub scope: Hex,
    pub record_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipRestore {
    pub successor: Hex,
    pub record_generation: u64,
    pub candidates: BTreeMap<Hex, ReconnectCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectCandidate {
    pub conversation_id: Hex,
    pub label: String,
    pub invite_operation_id: Option<OperationId>,
}

impl AppState {
    fn restore_recovery_read_scopes(&mut self) {
        if !self.outbox.iter().any(
            |entry| matches!(&entry.call, NetCall::RecoveryGet { scope, .. } if scope.is_empty()),
        ) {
            return;
        }
        let kit = self.recovery_kit.as_deref().and_then(|hex| {
            let bytes = zeroize::Zeroizing::new(crate::crypto::unhex(hex).ok()?);
            crate::recovery_record::RecoveryKit::from_bytes(&bytes).ok()
        });
        let places = self.recovery_join.iter().flat_map(|join| {
            join.place
                .iter()
                .chain(join.attempt.iter().flat_map(|attempt| &attempt.homes))
        });
        let mut scopes: BTreeMap<_, _> = places
            .map(|p| ((p.host.clone(), p.port), p.scope.clone()))
            .collect();
        if let Some(kit) = kit {
            scopes.insert((kit.host.clone(), kit.port), crate::crypto::hex(&kit.scope));
        }
        for entry in &mut self.outbox {
            if let NetCall::RecoveryGet {
                host, port, scope, ..
            } = &mut entry.call
                && scope.is_empty()
                && let Some(expected) = scopes.get(&(host.clone(), *port))
            {
                scope.clone_from(expected);
            }
        }
    }

    pub(crate) fn close_owner_attempt(&mut self, state: AttemptState) {
        let Some(join) = self.recovery_join.as_mut() else {
            return;
        };
        let approval_op = join
            .attempt
            .take()
            .and_then(|a| a.approval)
            .map(|a| a.operation_id);
        join.root = SecretHex::from(String::new());
        join.record = None;
        join.place = None;
        join.visited.clear();
        join.stage = JoinStage::Found;
        join.terminal = Some(state);
        self.outbox.retain(|e| match &e.purpose {
            Purpose::RecoveryFind
            | Purpose::OwnerRecoveryPost
            | Purpose::OwnerRecoveryRead { .. } => false,
            Purpose::DepositCeremony { operation_id, .. } => {
                Some(operation_id) != approval_op.as_ref()
            }
            Purpose::Ack => !matches!(e.call, NetCall::Deposit { .. }),
            _ => true,
        });
    }

    pub(crate) fn reconnect_invite(
        &self,
        candidate: &ReconnectCandidate,
        now: u64,
    ) -> Option<&InviteRec> {
        let id = candidate.invite_operation_id.as_ref()?;
        self.invites.get(id).filter(|i| {
            !i.revoked
                && i.expires_at > now
                && self.ops.get(id).is_some_and(|op| {
                    !matches!(op.outcome, crate::api::OutcomeClass::Rejected { .. })
                })
        })
    }

    /// A matching backup is presentation only. A live, authenticated account and the snapshot's prior
    /// conversation are both required before attaching its history to the new chat.
    pub(crate) fn relationship_backup(&self, rel: &Relationship) -> Option<&RestoredChat> {
        if rel.state != RelState::Active || rel.blocked {
            return None;
        }
        let candidate = self
            .relationship_restore
            .as_ref()?
            .candidates
            .get(&rel.peer_account_id)?;
        self.restored.values().find(|r| {
            r.peer_account_id == rel.peer_account_id
                && r.canon_id.as_ref() == Some(&candidate.conversation_id)
        })
    }
}

/// Where the record's evidence stands against what the owner independently holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStage {
    /// No pin: a sheet without a checkpoint and no controller. The device stays restricted, even after the timer.
    #[default]
    Unpinned,
    /// A pin exists but the signed states between it and the record's head, or the credentials or a recovery link they
    /// need, are not here. The device stays restricted until a revalidated carrier supplies them.
    NeedsEvidence,
    /// The chain verifies forward from the independently pinned state.
    Pinned,
    /// The record contradicts the pin (another account, a lower head, a different state at the pinned generation).
    Conflict,
}

/// Where the owner's attempt stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    /// The request is out and this device's own interval is running.
    #[default]
    Waiting,
    /// A valid veto was observed (terminal, kept even if the Home prunes it).
    Vetoed,
    /// The parent, policy epoch or target is no longer current, or a newer or conflicting head was observed.
    Stale,
    /// The person ended it.
    Cancelled,
}

/// One owner recovery attempt. `obs` is the verifier-local record (digest, exact request, monotonic credit, vetoes and the
/// exact successor): the same shape every controller and contact uses for the same request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerAttemptRec {
    /// The tentative device (`Device::to_bytes`), outside `st.device` until the successor is adopted.
    pub device: SecretHex,
    pub obs: RecoveryObsRec,
    pub state: AttemptState,
    /// An authenticated approval reply supplied a newer head with the same recovery policy.
    #[serde(default)]
    pub retry_from_verified_head: bool,
    /// The parent head the request names, its generation, the policy epoch and the target credential hash.
    pub parent: Hex,
    pub parent_generation: u64,
    pub epoch: u64,
    pub target: Hex,
    /// The evidence adoption re-checks: signed states from the first the record held through the parent, the credential
    /// bodies and the policy body.
    pub chain: Vec<Hex>,
    pub creds: Vec<Hex>,
    pub policy: Hex,
    /// Every Home the request is delivered to (host, port), and the exact notice bytes sent to contacts.
    pub homes: Vec<JoinPlace>,
    pub notice: Option<Hex>,
    /// Resend marks already used (1/3 and 2/3 of the interval, ADR-0190 3.1).
    #[serde(default)]
    pub resends: u8,
    /// Wall-clock scheduling hint only. The verifier interval is measured separately.
    #[serde(default)]
    pub last_request_post_ms: Option<u64>,
    /// Credit at the last veto read, and the round of reads in flight (a round answers only itself).
    #[serde(default)]
    pub polled_ms: u64,
    #[serde(default)]
    pub round: u32,
    #[serde(default)]
    pub pending_reads: u32,
    /// The round that includes both kinds of read before adoption.
    #[serde(default)]
    pub reconcile_round: Option<u32>,
    #[serde(default)]
    pub round_conflict: bool,
    #[serde(default)]
    pub competing_heads: Vec<OwnerHeadObservation>,
    #[serde(default)]
    pub competing_heads_full: bool,
    /// A saturated board may have omitted vetoes. A later, pruned board cannot resolve that observation.
    #[serde(default)]
    pub veto_page_full: bool,
    /// The current board round did not receive a response from every known Home.
    #[serde(default)]
    pub round_veto_unanswered: bool,
    /// The record used to prepare this request was absent during reconciliation.
    #[serde(default)]
    pub round_record_missing: bool,
    /// A verified, co-signed successor arrived in the owner-mediated approval exchange.
    #[serde(default)]
    pub controller_approved: bool,
    /// A conflicted snapshot can be replaced only by a newer verified refresh.
    #[serde(default)]
    pub snapshot_conflict_generation: Option<u64>,
    /// Links of the pinned chain that need their recovery evidence (from a controller's evidence), in chain order.
    #[serde(default)]
    pub links: Vec<RecoveryLinkRec>,
    /// This device's side of the approval ceremony with a surviving controller (ADR-0203), once it joined one.
    #[serde(default)]
    pub approval: Option<ApprovalJoin>,
}

impl OwnerAttemptRec {
    pub(crate) fn has_competing_heads(&self) -> bool {
        self.competing_heads_full || !self.competing_heads.is_empty()
    }

    pub(crate) fn can_retry_from_verified_head(&self) -> bool {
        self.state == AttemptState::Stale
            && self.retry_from_verified_head
            && !self.has_competing_heads()
    }
}

/// ADR-0216. A single short-lived account-control exchange, separate from device enrollment and recovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerChange {
    pub operation_id: OperationId,
    pub expires_at: u64,
    pub secret: SecretHex,
    pub a_cap: SecretHex,
    pub b_recv: SecretHex,
    pub mailbox: Hex,
    pub relay_uri: String,
    pub origin: Hex,
    pub target: Hex,
    pub parent: Hex,
    pub child: Hex,
    pub remove: bool,
    /// The response queue deposit capability. Absent on the joining device.
    pub response_cap: Option<SecretHex>,
    pub confirmed: bool,
    pub request: Option<Hex>,
    pub recipient_seed: Option<SecretHex>,
}

/// The controller's side of a recovery approval ceremony (ADR-0203): two queues on its own Home, a fresh secret, and
/// the proposal it was shown. Never an enrollment: it has its own link prefix, keys and message types.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalOffer {
    pub operation_id: OperationId,
    pub secret: SecretHex,
    pub a_mailbox: Hex,
    pub a_route: RelayEntry,
    pub b_mailbox: Hex,
    pub b_route: RelayEntry,
    pub b_recv: Hex,
    pub link: Option<SecretHex>,
    pub created_ms: u64,
    pub expires_at: u64,
    pub state: LinkState,
    /// The request digest of the proposal shown to the person, and the six digits both devices display.
    #[serde(default)]
    pub request_digest: Option<Hex>,
    #[serde(default)]
    pub proposal: Option<Hex>,
    #[serde(default)]
    pub code: Option<String>,
    /// The Home refused the reply for good after the co-signature was made.
    #[serde(default)]
    pub reply_undelivered: bool,
}

impl core::fmt::Debug for ApprovalOffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ApprovalOffer")
            .field("operation_id", &self.operation_id)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// The restricted device's side: what it sent, what the controller announced and the evidence chunks received so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalJoin {
    pub operation_id: OperationId,
    pub secret: SecretHex,
    pub relay_uri: String,
    pub a_cap: Hex,
    pub b_mailbox: Hex,
    pub b_recv: Hex,
    pub expires_at: u64,
    pub state: LinkState,
    pub proposal: Hex,
    pub code: String,
    /// The authenticated descriptor (its exact bytes) once the reply verified, and the chunks by index.
    #[serde(default)]
    pub descriptor: Option<Hex>,
    #[serde(default)]
    pub chunks: Vec<Option<Hex>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPlace {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub scope: Hex,
}

/// Where the lookup of the account's record stands on a recovering device (ADR-0200 section 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinStage {
    /// The key says where the account is saved only through a place the person still has to give.
    NeedsPlace,
    /// Asking the Home for the record (also while it cannot be reached: the request is retried).
    #[default]
    Checking,
    /// The record was read and checked; the recovery can be asked for.
    Found,
    /// The same answer for a wrong key and an unknown record.
    NotFound,
    /// The record is there but cannot be used (not readable with this key, or it fails its checks).
    CantRead,
    /// The Home answered with an older record than this device already accepted (the record floor).
    OutOfDate,
    /// Another record at the same generation, or another account under the same place: never used.
    Conflict,
    /// An unrelated lineage cannot displace any of the saved floors.
    FloorTableFull,
}

/// Stored evidence of one completed recovery link (see `control::RecoveryLinkEvidence`). All hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryLinkRec {
    pub child_hash: Hex,
    pub policy: Hex,
    pub next_policy: Hex,
    pub request: Hex,
    pub elapsed_seconds: u64,
}

impl AppState {
    /// The stored recovery-link evidence in the form `control::verify_chain_with` takes (undecodable entries dropped,
    /// which can only make the chain fail to verify).
    pub fn link_evidence(&self) -> Vec<crate::control::RecoveryLinkEvidence> {
        let h = |s: &str| crate::crypto::unhex(s).ok();
        self.recovery_links
            .iter()
            .filter_map(|l| {
                Some(crate::control::RecoveryLinkEvidence {
                    child_hash: h(&l.child_hash)?.try_into().ok()?,
                    policy: h(&l.policy)?,
                    next_policy: h(&l.next_policy)?,
                    request: h(&l.request)?,
                    elapsed_seconds: l.elapsed_seconds,
                })
            })
            .collect()
    }
}

/// Where an observed recovery request stands for this verifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryObsState {
    Waiting,
    /// A valid veto by a live controller was observed: the request can never complete here.
    Vetoed,
    /// The account moved on (the parent head changed, the target was enrolled or revoked): dropped.
    Stale,
    /// The successor was verified and recorded.
    Completed,
}

/// The recovery-signed successor the recovering device produced, with what a verifier needs to check it
/// (`RECOVERY_AND_BACKUP_POLICY.md` 10.3): the signed child state, the rotated policy body and the new device's
/// credential body. All hex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySuccessorRec {
    pub child: Hex,
    pub next_policy: Hex,
    pub credential: Hex,
}

/// One observed recovery request. `credited_ms` is the ONLY clock: monotonic time durably checkpointed by this
/// verifier since it first saw the request. `first_seen_ms` is a wall-clock display hint, never authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryObsRec {
    pub digest: Hex,
    /// The raw `signed-RecoveryRequest`.
    pub request: Hex,
    pub first_seen_ms: u64,
    /// The host's boot session of the last credited reading, and that reading.
    pub boot_session: Hex,
    pub last_mono_ms: u64,
    pub credited_ms: u64,
    /// Raw `signed-RecoveryVeto`s that verified against the head (bounded).
    #[serde(default)]
    pub vetoes: Vec<Hex>,
    /// This device signed one of them.
    #[serde(default)]
    pub own_veto: bool,
    #[serde(default)]
    pub veto_homes: Vec<(String, Option<u16>)>,
    #[serde(default)]
    pub successor: Option<RecoverySuccessorRec>,
    pub state: RecoveryObsState,
}

/// Where a contact stands on one recovery of the peer's account (ADR-0190 section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRecoveryState {
    /// Valid evidence seen, this contact's own interval running, no veto.
    Waiting,
    /// The interval ran with no veto, but a step is missing: the successor, or this contact's own confirmation.
    Ready,
    /// Verified with this verifier's own interval: the pinned head is the successor.
    Accepted,
    /// A valid veto by a live controller was observed.
    Vetoed,
    /// The contact chose "This isn't them". Local only.
    Declined,
    /// The account moved on (the pinned head changed, the target was enrolled or revoked): dropped.
    Stale,
}

impl PeerRecoveryRec {
    /// Still running: `Waiting` or `Ready`.
    pub fn is_live(&self) -> bool {
        matches!(
            self.state,
            PeerRecoveryState::Waiting | PeerRecoveryState::Ready
        )
    }
}

/// The contact's own decision on a tracked recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerDecision {
    Declined,
    Confirmed,
}

/// One tracked recovery of a contact's account, per relationship. `obs` is the verifier-local record (digest, raw
/// request, first sighting, monotonic credit, vetoes, held successor); there is no time field anywhere else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRecoveryRec {
    pub obs: RecoveryObsRec,
    /// The `RecoveryPolicy` body the request's parent state commits to (hex).
    pub policy: Hex,
    pub state: PeerRecoveryState,
    #[serde(default)]
    pub decision: Option<PeerDecision>,
}

/// Where a device-link ceremony is, on either side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Existing device: the response queue is being opened, there is no link yet.
    Opening,
    WaitingForScan,
    AwaitingApproval,
    /// Authorized; the bounded roster/history handoff is still in progress.
    Transferring,
    Linked,
    Rejected,
    Expired,
    /// New device: the approval did not verify, or the ceremony was refused. Nothing was adopted.
    Failed,
}

impl LinkState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Linked | Self::Rejected | Self::Expired | Self::Failed
        )
    }
}

/// Local ceremony bookkeeping bounds; summaries expire 24 hours after the original ceremony deadline.
pub const MAX_TERMINAL_LINK_OFFERS: usize = 32;
pub const MAX_ACTIVE_LINK_OFFERS: usize = 4;
pub const LINK_SUMMARY_RETENTION_MS: u64 = 24 * 60 * 60 * 1000;

/// Apply wall-clock retention when the host supplies its current time; loading also enforces hard count bounds.
pub fn prune_link_offers(app: &mut AppState, now_ms: u64) {
    scrub_terminal_links(app);
    app.link_offers.retain(|_, o| {
        !o.state.is_terminal() || now_ms < o.expires_at.saturating_add(LINK_SUMMARY_RETENTION_MS)
    });
}

/// A terminal ceremony retains only its status and public summary (ADR-0192 section 6.4).
/// Shared by every terminal transition and record loading, including older records.
pub fn scrub_terminal_links(app: &mut AppState) {
    use zeroize::Zeroize;
    // Older records can exceed the active bound. Keep the newest bounded set and end the others safely.
    let mut active: Vec<_> = app
        .link_offers
        .iter()
        .filter(|(_, o)| !o.state.is_terminal())
        .map(|(id, o)| (o.created_ms, id.clone()))
        .collect();
    active.sort();
    for (_, id) in active
        .iter()
        .take(active.len().saturating_sub(MAX_ACTIVE_LINK_OFFERS))
    {
        if let Some(o) = app.link_offers.get_mut(id) {
            o.state = LinkState::Expired;
        }
    }
    for o in app
        .link_offers
        .values_mut()
        .filter(|o| o.state.is_terminal())
    {
        o.secret.zeroize();
        o.link.zeroize();
        o.link = None;
        o.a_route.capability.zeroize();
        o.b_route.capability.zeroize();
        o.b_recv.zeroize();
        if let Some(r) = o.request.as_mut() {
            r.code.zeroize();
            r.signing_pk.clear();
            r.ik.clear();
            r.nonce.clear();
            r.request_digest.clear();
        }
        o.introduced.clear();
    }
    let mut terminal: Vec<_> = app
        .link_offers
        .iter()
        .filter(|(_, o)| o.state.is_terminal())
        .map(|(id, o)| (o.created_ms, id.clone()))
        .collect();
    terminal.sort();
    for (_, id) in terminal
        .iter()
        .take(terminal.len().saturating_sub(MAX_TERMINAL_LINK_OFFERS))
    {
        app.link_offers.remove(id);
    }
    if let Some(j) = app.link_join.as_mut().filter(|j| j.state.is_terminal()) {
        j.secret.zeroize();
        j.a_cap.zeroize();
        j.b_recv.zeroize();
        j.code.zeroize();
        j.cont = None;
        if j.approved && j.history_done < j.history_expected {
            j.history_degraded = true;
        }
    }
}

/// What the new device asked for, as the existing device holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRequestRec {
    pub request_id: Hex,
    pub device_id: Hex,
    pub signing_pk: Hex,
    pub ik: Hex,
    pub label: String,
    pub nonce: Hex,
    pub code: String,
    /// SHA-256 of the exact request bytes (hex); the approval binding names it.
    #[serde(default)]
    pub request_digest: Hex,
    pub received_ms: u64,
}

/// The existing device's side of a ceremony: two queues on its own Home (request: it drains; response: it deposits).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOffer {
    pub operation_id: OperationId,
    /// The one-time ceremony key (16 bytes). Never leaves the two devices; absent from every log.
    pub secret: SecretHex,
    pub a_mailbox: Hex,
    pub a_route: RelayEntry,
    pub b_mailbox: Hex,
    pub b_route: RelayEntry,
    /// Receive capability of the response queue, handed to the new device in the link.
    pub b_recv: Hex,
    pub link: Option<String>,
    pub created_ms: u64,
    pub expires_at: u64,
    pub state: LinkState,
    pub request: Option<LinkRequestRec>,
    /// The new device's introductions (one per conversation) have been received for these conversations.
    #[serde(default)]
    pub introduced: Vec<String>,
    /// History chunks (`DeviceHistorySnapshot`, multi-device phase 5) deposited for the new device at approval.
    #[serde(default)]
    pub history_sent: u32,
    /// Roster entries (conversations handed over) at approval; the secret is kept until their introductions are in.
    #[serde(default)]
    pub roster_sent: u32,
    /// The Home refused the approval message for good after the approval committed: the new device never received it.
    #[serde(default)]
    pub approval_undelivered: bool,
}

/// The sender's repeat plan for a ceremony whose evidence is several pieces (a manifest and its chunks): the sealed pieces
/// exactly as first sent, in order (manifest first), the route to the receiver's queue, and how many of the repeats at one,
/// two, three and four fifths of the exchange's life have run (ADR-0226 section 5). The receiver announces nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRepeat {
    pub route: RelayEntry,
    pub sealed: Vec<String>,
    pub start_ms: u64,
    pub expires_at: u64,
    pub done: u8,
}

/// Repeats of ceremony evidence, at one fifth of the remaining life each (ADR-0226 section 5).
pub const EVIDENCE_REPEATS: u8 = 4;

/// The new device's side of a ceremony.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkJoin {
    pub operation_id: OperationId,
    pub secret: SecretHex,
    pub relay_uri: String,
    /// Deposit capability of the request queue (from the link).
    pub a_cap: Hex,
    pub b_mailbox: Hex,
    pub b_recv: Hex,
    pub created_ms: u64,
    pub expires_at: u64,
    pub label: String,
    pub nonce: Hex,
    pub request_id: Hex,
    pub code: String,
    /// SHA-256 of the request bytes sent (hex).
    #[serde(default)]
    pub request_digest: Hex,
    /// Digests of roster entries already counted (replays are no-ops).
    #[serde(default)]
    pub roster_seen: Vec<String>,
    pub state: LinkState,
    /// The approval verified; roster messages still expected (conversations handed over).
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub roster_expected: u32,
    #[serde(default)]
    pub roster_done: u32,
    /// The existing device's DeviceID, from the verified approval.
    #[serde(default)]
    pub approver: Hex,
    /// History chunks the approval announced, how many arrived, how many were ignored whole (unknown codec or
    /// invalid), and how many messages they added.
    #[serde(default)]
    pub history_expected: u32,
    #[serde(default)]
    pub history_done: u32,
    #[serde(default)]
    pub history_ignored: u32,
    #[serde(default)]
    pub history_messages: u32,
    /// Chunk indices already counted: each index counts once, so a replayed frame cannot stand
    /// in for a withheld one, and the missing set (`0..history_expected` minus this) stays known.
    #[serde(default)]
    pub history_seen: Vec<u32>,
    /// The ceremony ended (its time ran out) with chunk indices still missing: an explicit, final degraded end state.
    /// Nothing is retried; the device keeps what arrived and the person is told the history is partial.
    #[serde(default)]
    pub history_degraded: bool,
    /// A continuation approval (ADR-0195) waiting for the rest of the chain and credentials. Nothing is adopted while
    /// this is set; it is cleared at every end of the ceremony.
    #[serde(default)]
    pub cont: Option<LinkCont>,
}

/// The new device's buffer for a continuation approval: the approval as received, its manifest, and the chunks so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCont {
    /// The approval message plaintext (hex); it is verified in full only when every chunk is in.
    pub approval: Hex,
    pub digest: Hex,
    pub states: u32,
    pub creds: u32,
    pub chunks: u32,
    /// `(index, chunk plaintext hex)`, one per index.
    pub got: Vec<(u32, Hex)>,
}

/// Where a notice owed to another device stands. An acknowledged notice is deleted, and so is one whose device a
/// verified head revoked or removed, so neither is a state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwedState {
    /// A session send is in the outbox or accepted; unacknowledged it is sent again on a schedule.
    Queued,
    /// No path to the device yet.
    Waiting,
    /// The device is quarantined; delivery is owed from the moment it is active again.
    Held,
}

/// An acknowledgement of a notice, waiting for a route to the device that sent it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwedAck {
    pub sibling_device_id: Hex,
    pub notice_digest: Hex,
}

/// A durable, unacknowledged `DeviceEnrollmentNotice` owed to one other device (ADR-0193).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwedNotice {
    pub sibling_device_id: Hex,
    /// Hash of the AccountControlState the enrollment created.
    pub enrollment_state_hash: Hex,
    /// SHA-256 of the signed notice (what the acknowledgement names).
    pub notice_digest: Hex,
    /// The signed notice itself, so it is never lost to the display trim.
    pub notice: Hex,
    pub created_at: u64,
    pub attempts: u32,
    pub last_attempt_at: u64,
    pub state: OwedState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeRec {
    pub device_id: Hex,
    pub label: String,
    pub at: u64,
    /// The signed `DeviceEnrollmentNotice` (hex) this record was made from: a device that later approves a link hands
    /// it to a sibling that missed that enrollment (one notice per enrollment). Empty for a record made before this
    /// field existed.
    #[serde(default)]
    pub signed: String,
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
    /// For `ChangePrivacyPolicy` only: the rank of the person's level when the nonce was issued. The change runs only while
    /// the level still has that rank, so a nonce issued for a tightening can never apply as a loosening (D130).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_rank: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMeta {
    pub label: String,
    pub enrolled_at: u64,
}

#[derive(Clone, Debug)]
pub struct ProfileState {
    /// Pack inventory has its own DeviceVault record and stays outside account state.
    pub(crate) expressive_packs: crate::expressive::Packs,
    pub(crate) calls: crate::calls::Calls,
    pub generation: String,
    pub device: Option<Device>,
    pub app: AppState,
}

impl ProfileState {
    pub fn empty(generation: String) -> Self {
        ProfileState {
            expressive_packs: Default::default(),
            calls: crate::calls::Calls::default(),
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
        let mut records = BTreeMap::from([(REC_STATE.to_string(), bytes)]);
        self.encode_pack_record(&mut records)?;
        Ok(records)
    }

    pub(crate) fn encode_pack_record(
        &self,
        records: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), CoreError> {
        if self.expressive_packs.is_empty() {
            records.remove(REC_EXPRESSIVE_PACKS);
        } else {
            let packs = serde_json::to_vec(&self.expressive_packs)
                .map_err(|_| CoreError::CorruptState("encode expressive packs"))?;
            records.insert(REC_EXPRESSIVE_PACKS.into(), packs);
        }
        Ok(())
    }

    pub fn decode(generation: String, records: &[Record]) -> Result<Self, CoreError> {
        let mut st = ProfileState::empty(generation);
        let mut seen = BTreeSet::new();
        for r in records {
            if !seen.insert(r.key.as_str()) {
                return Err(CoreError::CorruptState("duplicate record"));
            }
            if r.key == REC_EXPRESSIVE_PACKS {
                st.expressive_packs = serde_json::from_slice(&r.value)
                    .map_err(|_| CoreError::CorruptState("expressive packs"))?;
                continue;
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
                Some(2) => {
                    migrate_v2(&mut app);
                    migrate_v3(&mut app);
                    migrate_v4(&mut app);
                }
                Some(3) => {
                    migrate_v3(&mut app);
                    migrate_v4(&mut app);
                }
                Some(4) => migrate_v4(&mut app),
                _ => return Err(CoreError::CorruptState("unsupported app state version")),
            }
            if let Some(attempt) = app.pointer_mut("/recoveryJoin/attempt")
                && attempt.get("roundConflict").and_then(|v| v.as_bool()) == Some(true)
                && attempt.get("competingHeads").is_none()
            {
                attempt["competingHeadsFull"] = serde_json::Value::Bool(true);
            }
            st.app =
                serde_json::from_value(app).map_err(|_| CoreError::CorruptState("app state"))?;
        }
        scrub_terminal_links(&mut st.app);
        if let Some(attempt) = st
            .app
            .recovery_join
            .as_ref()
            .and_then(|j| j.attempt.as_ref())
            && attempt.state != AttemptState::Waiting
            && !attempt.can_retry_from_verified_head()
        {
            st.app.close_owner_attempt(attempt.state);
        }
        st.app.restore_recovery_read_scopes();
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
    o.insert("version".into(), json!(3));
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

/// Version 3 -> 4 (audit M1, deviation D1). Delivery keys used to be bound to the fixed stand-in route
/// `om-core:mailbox/v0` plus the 16-byte mailbox; they are now bound to a `RelayEntry` the host's provider adapter
/// allocated. Nothing that already exists can be re-bound (a binding is a signed, peer-held commitment), so every
/// relationship, pending introduction and request keeps the stand-in as an explicit *legacy* entry: its keys keep
/// verifying, its sends keep going to the same capability, and only new relationships use real routes.
/// `peer_mailbox` (a hex capability) becomes `peer_relays`; `my_mailbox` gains `my_relays`.
fn migrate_v3(app: &mut serde_json::Value) {
    use serde_json::{Value, json};
    let legacy = |mailbox: &Value| -> Value {
        json!({"relayUri": crate::crypto::delivery::CORE_ROUTE_URI, "capability": mailbox, "priority": 0, "expiresAt": 0})
    };
    let Some(o) = app.as_object_mut() else { return };
    o.insert("version".into(), json!(APP_VERSION));
    if let Some(Value::Object(rels)) = o.get_mut("relationships") {
        for r in rels.values_mut() {
            let mine = r.get("myMailbox").cloned().unwrap_or(Value::Null);
            let peer = r.get("peerMailbox").cloned().unwrap_or(Value::Null);
            r["myRelays"] = json!({"entries": [legacy(&mine)]});
            r["peerRelays"] =
                json!({"entries": if peer.is_string() { vec![legacy(&peer)] } else { vec![] }});
            if let Some(m) = r.as_object_mut() {
                m.remove("peerMailbox");
            }
        }
    }
    if let Some(Value::Object(intros)) = o.get_mut("intros") {
        for i in intros.values_mut() {
            let peer = i.get("peerMailbox").cloned().unwrap_or(Value::Null);
            i["peerRoute"] = legacy(&peer);
            if let Some(m) = i.as_object_mut() {
                m.remove("peerMailbox");
            }
        }
    }
}

/// Version 4 -> 5 (D22). A relationship polled exactly one mailbox, `myMailbox`, whose route was the single entry of
/// `myRelays`; both now live in `myBoxes`. The signed lineages start empty: until the peer publishes a signed RelaySet,
/// its one route stands as an implicit set, and ours is signed the first time we change it.
fn migrate_v4(app: &mut serde_json::Value) {
    use serde_json::{Value, json};
    let Some(o) = app.as_object_mut() else { return };
    o.insert("version".into(), json!(APP_VERSION));
    if let Some(Value::Object(rels)) = o.get_mut("relationships") {
        for r in rels.values_mut() {
            let route = r["myRelays"]["entries"]
                .as_array()
                .and_then(|a| a.first())
                .cloned();
            let mb = r.get("myMailbox").cloned().unwrap_or(Value::Null);
            r["myBoxes"] = match route {
                Some(route) => json!([{"mailbox": mb, "route": route}]),
                None => json!([]),
            };
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

/// A nickname (ADR-0196): already trimmed, 1 to 64 characters, no control characters and no bidirectional controls, so
/// it cannot rewrite the text around it (IS-03). Anything else is kept exactly as typed.
pub fn valid_nickname(n: &str) -> bool {
    let len = n.chars().count();
    (1..=64).contains(&len)
        && n == n.trim()
        && !n.chars().any(|c| {
            c.is_control()
                || matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
}
