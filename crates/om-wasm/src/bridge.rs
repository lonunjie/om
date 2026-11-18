// SPDX-License-Identifier: MPL-2.0
//! The JSON command layer over [`om_core::Core`]. Pure safe Rust and host-independent, so it is unit-tested natively.

use om_core::Core;
use om_core::api::{HomeSummary, Intent, ProfileExecutionContext, Query};
use om_core::attach::Meta;
use om_core::backup::BackupPlan;
use om_core::crypto::rng::{self, CryptoRng, RngCore};
use om_core::direct::{DirectFrame, ErrorCode, FetchState, TRANSFER_ID_LEN};
use om_core::groups::carriage::Sealed;
use om_core::groups::flow::{
    DeviceRelease, GroupEvent, GroupIntent, GroupQuery, GroupReply, GroupView, GroupWire, Outbound,
    Sender, Target,
};
use om_core::groups::room::Setting;
use om_core::host::{BlobLimits, Clock, CoreError, Inbound, NetResult, Record};
use om_core::route::{DepositRevocation, RoomRouteOffer, RouteOffer};
use serde::Deserialize;
use serde_json::{Value, json};

/// The recovery place a host supplies (`SetRecoveryPlace`): host name, optional port, scope id (hex, 32 bytes).
#[derive(Deserialize)]
struct RecoveryPlaceArg {
    host: String,
    #[serde(default)]
    port: Option<u16>,
    scope: String,
}

#[derive(Deserialize)]
#[serde(
    tag = "cmd",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Command {
    Init {
        home: HomeSummary,
        #[serde(default)]
        calls: bool,
    },
    OpenProfile {
        profile_id: String,
        records: Vec<Record>,
    },
    CloseProfile {
        profile_id: String,
    },
    ContextFor {
        profile_id: String,
    },
    Dispatch {
        intent: Intent,
    },
    FinishCommit {
        commit_id: u64,
        ok: bool,
    },
    NetOps {
        ctx: ProfileExecutionContext,
    },
    NetResult {
        ctx: ProfileExecutionContext,
        net_id: u64,
        result: NetResult,
    },
    PollTargets {
        ctx: ProfileExecutionContext,
    },
    Inbound {
        ctx: ProfileExecutionContext,
        mailbox: String,
        items: Vec<Inbound>,
    },
    Query {
        query: Query,
    },
    PendingOperations {
        ctx: ProfileExecutionContext,
    },
    /// The host's durable monotonic reading, credited to every pending recovery (the owner's and, per contact, the
    /// ones this device tracks for a contact's account). Downtime and backwards readings credit nothing.
    RecoveryTick {
        ctx: ProfileExecutionContext,
        mono: om_core::host::MonotonicSample,
    },
    /// Send the device-enrollment notices that are due (ADR-0193); a no-op when none is owed.
    NoticeTick {
        ctx: ProfileExecutionContext,
    },
    CallTick {
        ctx: ProfileExecutionContext,
    },
    CallDescription {
        ctx: ProfileExecutionContext,
        call_session_id: String,
        #[serde(default)]
        ice: bool,
    },
    CallMediaState {
        ctx: ProfileExecutionContext,
        call_session_id: String,
        state: om_core::calls::MediaState,
    },
    CallRouteFacts {
        ctx: ProfileExecutionContext,
        call_session_id: String,
        route: om_core::calls::CallRoute,
    },
    Outcome {
        ctx: ProfileExecutionContext,
        operation_id: String,
    },
    /// Settings and a fresh salt for the next backup file (ADR-0184).
    BackupPlan,
    /// Seal the profile's history into a backup file; `key` is the host's Argon2id output for `plan` (hex).
    BackupExport {
        ctx: ProfileExecutionContext,
        plan: BackupPlan,
        key: String,
    },
    /// Read a backup file's public header (what the host needs to derive the key); needs no profile.
    BackupInspect {
        file: String,
    },
    /// Restore a backup file into the profile as read-only history: all of it or none of it.
    BackupImport {
        ctx: ProfileExecutionContext,
        file: String,
        key: String,
    },
    /// Routes the provider adapter allocated for mailboxes Core has not created yet. Answers how many Core holds.
    SupplyRoutes {
        routes: Vec<RouteOffer>,
    },
    SupplyRoomRoutes {
        ctx: ProfileExecutionContext,
        routes: Vec<RoomRouteOffer>,
    },
    /// The Home (relay_uri) of each route offer Core still holds.
    OfferHomes,
    /// What the BlobStore advertises (`om.blob.spool.v1` limits); `null` when it offers none. Without it attachments are
    /// refused (`unsupported`), never sent inline.
    SetBlobLimits {
        limits: Option<BlobLimits>,
    },
    /// Where this account keeps its recovery record (the Home's address and its `recovery_scope_id`), as the host read
    /// it from the Home's capabilities document; `null` when the Home offers no RecoveryStore. Must be set before the
    /// account is created for its key sheet to carry the place (ADR-0200); without one no record is written.
    SetRecoveryPlace {
        place: Option<RecoveryPlaceArg>,
    },
    /// Seal an attachment on this device (`plaintext` hex). Answers `{prepared, commit}`, or `{refused}` with the typed
    /// reason (`too_large` with the largest size, `storage_full`, `no_blob_store`, ...) before anything is sent. The host
    /// stages `prepared.ciphertext` under `prepared.blobId` and uses it only after `commit` is durable.
    PrepareAttachment {
        ctx: ProfileExecutionContext,
        plaintext: String,
        meta: Meta,
    },
    /// The ciphertext of a prepared or outgoing attachment again from its plaintext (a host that lost its staged bytes).
    RestageAttachment {
        ctx: ProfileExecutionContext,
        blob_id: String,
        plaintext: String,
    },
    /// The fetch for one attachment of a message: `{call: get_blob ...}` for the host to perform and hash-check.
    AttachmentFetch {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Verify fetched bytes against the reference's digest and length, then decrypt: `{opened}` or `{refused}`.
    OpenAttachment {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
        ciphertext: String,
    },
    /// Seal a file too big for the delivery service so it can be offered directly (`plaintext` hex). Answers
    /// `{prepared, commit}` or `{refused}`. Nothing is stored; the sealed object is held by Core for serving.
    PrepareDirectAttachment {
        ctx: ProfileExecutionContext,
        plaintext: String,
        meta: Meta,
    },
    /// After a restart: hand the original file back so a persisted offer can be served again (`{armed: true}` or `{refused}`).
    ArmDirectOffer {
        ctx: ProfileExecutionContext,
        blob_id: String,
        plaintext: String,
    },
    /// One frame from the other person's device (the sender side). Answers `{frames, commit}`.
    DirectServe {
        ctx: ProfileExecutionContext,
        frame: Value,
    },
    /// Frames the sender may send now, given how many bytes the channel still has buffered.
    DirectPump {
        ctx: ProfileExecutionContext,
        buffered: usize,
    },
    /// The first frame of a fetch of a direct attachment (the receiver side): `{frame}` or `{refused}`.
    DirectFetchStart {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// One frame from the sender; answers the frames to send back.
    DirectFetchHandle {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
        frame: Value,
    },
    /// `{state: idle|requested|receiving|complete|committed|failed, held}` or `null` when no fetch is open.
    DirectFetchProgress {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// The verified ciphertext of a completed fetch (hex) for the host's local blob cache: `{ciphertext}` or `null`.
    DirectFetchCiphertext {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// The host's durable write of that ciphertext succeeded (ADR-0194): answers the receipt `{frames}` to send, or
    /// `{refused}`. Call it before `DirectFetchOpen`; no commit, no receipt.
    DirectFetchCommitted {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Decrypt a completed direct fetch: `{opened}` or `{refused}`.
    DirectFetchOpen {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Whether this build links a group engine (`true` only with `groups-openmls` here; test builds use a fake).
    GroupsAvailable,
    /// Ordinary groups (ADR-0177), one typed step of the person (create, invite, accept, decline, leave, remove,
    /// text, commit published or accepted). Answers `{reply: {events, out}, commit}` or `{refused: code}`. `groups_unavailable`
    /// when this build does not link a group engine. The wire in `out` is Core-internal JSON (groups plan Q2), not a
    /// frozen format; the host carries each item as the `to` says and hands the answer to `group_receive`.
    GroupDo {
        ctx: ProfileExecutionContext,
        intent: GroupIntentJson,
    },
    /// Something that arrived for groups: `from` is what the transport authenticated (`{relationship}` or `{device}`).
    GroupReceive {
        ctx: ProfileExecutionContext,
        from: GroupFromJson,
        wire: GroupWire,
    },
    /// Typed groups queries: `groups`, `group`, `invites`, `messages`, `notices`.
    GroupQuery {
        ctx: ProfileExecutionContext,
        query: GroupQueryJson,
    },
    /// ADR-0189 section 2 (G10): Core accepts every own Commit whose three conditions hold. The host calls this after
    /// every durable group step and at start-up. Answers `{reply, commit}` (one merged reply) or `{refused}`.
    GroupSettle {
        ctx: ProfileExecutionContext,
    },
    /// The frames Core owns until they are deposited, oldest first: `{items: [{id, intentId, to, wire}]}`.
    GroupOutbox {
        ctx: ProfileExecutionContext,
    },
    /// The frames with these ids were deposited (transport progress only, never acceptance). Answers `{commit}`.
    GroupOutboxDone {
        ctx: ProfileExecutionContext,
        ids: Vec<u64>,
    },
    /// Ask, through the existing head request, for the account heads that held frames wait for. Answers `{commit}`.
    GroupPendingRefresh {
        ctx: ProfileExecutionContext,
    },
    /// Retry held frames (after a poll that may have brought a head). Answers `{reply, commit}` or `{refused}`.
    GroupPendingRetry {
        ctx: ProfileExecutionContext,
    },
    /// After a removal: a fresh route for the Room; the `route_update` frames are queued in Core's outbox. Answers `{queued, commit}`.
    GroupRouteUpdate {
        ctx: ProfileExecutionContext,
        conversation_id: String,
    },
    /// Send an Invite, Accept or Decline (relationship lane) as an event on the 1:1. Answers `{commit}` or `{refused}`.
    GroupLaneSend {
        ctx: ProfileExecutionContext,
        relationship_id: String,
        wire: GroupWire,
    },
    /// Take the oldest kept Invite, Accept or Decline into the 1:1 queue. Answers `{moved, commit}` (call again while
    /// `moved`) or `{refused}` when every kept item must wait (no route offer, relationship not usable yet).
    GroupLaneFlush {
        ctx: ProfileExecutionContext,
    },
    /// Hand every verified lane event that arrived to the group flow. Answers `{reply, pumped, commit}`.
    GroupLanePump {
        ctx: ProfileExecutionContext,
    },
    /// Seal one outbox item (or a Welcome or App `out`) to its destination devices' per-Room routes. Answers
    /// `{sealed: [{device, wrapper, entry}], commit}` or `{refused}`; the host deposits each wrapper at its entry.
    GroupFrameSend {
        ctx: ProfileExecutionContext,
        to: GroupToJson,
        wire: GroupWire,
    },
    /// A sealed wrapper that arrived at one of this device's per-Room routes. Answers `{reply, commit}` or `{refused}`
    /// (a refusal is a drop that changed nothing).
    GroupFrameReceive {
        ctx: ProfileExecutionContext,
        wrapper: String,
    },
    /// Retired routes whose overlap ended are dropped: `{revoke: [relay entries], commit}`.
    GroupRoutesExpire {
        ctx: ProfileExecutionContext,
    },
    GroupRouteRevoked {
        ctx: ProfileExecutionContext,
        revoke: DepositRevocation,
    },
    /// `{mailboxes: [{mailbox, relayUri}], routeOwed: [room], outbox, pending}`: what the host watches for the device lane.
    GroupCarriageStatus {
        ctx: ProfileExecutionContext,
    },
    /// `{invitations: [{relationshipId, inviteId, groupName, receivedS}], overflow: {invites, replies}}`: the group invitations
    /// held without the group engine (base module too) and what could not be kept. Read-only; never accepts anything.
    GroupHeldInvitations {
        ctx: ProfileExecutionContext,
    },
    /// The person read the overflow notice: clears its counters. Answers `{commit}`.
    GroupLaneOverflowAck {
        ctx: ProfileExecutionContext,
    },
    /// The person stopped using one of their Homes: relationships with another route publish a RelaySet without it.
    /// Answers how many relationships had no other route and so stay on that Home.
    RetireHome {
        ctx: ProfileExecutionContext,
        relay_uri: String,
        #[serde(default)]
        emergency: bool,
    },
}

/// The typed group intent as JSON: `kind` plus camelCase fields, byte strings as hex.
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum GroupIntentJson {
    Create {
        name: String,
        relationship_ids: Vec<String>,
    },
    CreateWithProtection {
        name: String,
        relationship_ids: Vec<String>,
        protection: u64,
    },
    RoomSetting {
        conversation_id: String,
        setting: Setting,
        #[serde(default)]
        confirmed: bool,
    },
    Invite {
        conversation_id: String,
        relationship_id: String,
    },
    AskToRejoin {
        relationship_id: String,
        room_hint: String,
    },
    ResolveRejoin {
        request_id: String,
        agree: bool,
    },
    Accept {
        invite_id: String,
        #[serde(default)]
        siblings: Vec<DeviceRelease>,
    },
    Decline {
        invite_id: String,
    },
    Leave {
        conversation_id: String,
    },
    Remove {
        conversation_id: String,
        account: String,
    },
    SendText {
        conversation_id: String,
        text: String,
    },
    SendReply {
        conversation_id: String,
        text: String,
        reply_to: String,
    },
    SendMentions {
        conversation_id: String,
        text: String,
        #[serde(default)]
        reply_to: Option<String>,
        mentions: Vec<WireMention>,
    },
    CreatePoll {
        conversation_id: String,
        question: String,
        options: Vec<String>,
        multiple: bool,
        #[serde(default)]
        max_selections: Option<u8>,
        visibility: String,
    },
    VotePoll {
        conversation_id: String,
        poll_id: String,
        option_ids: Vec<u8>,
    },
    ClosePoll {
        conversation_id: String,
        poll_id: String,
    },
    SetReaction {
        conversation_id: String,
        message_id: String,
        key: String,
        present: bool,
    },
    EditMessage {
        conversation_id: String,
        message_id: String,
        text: String,
    },
    DeleteMessage {
        conversation_id: String,
        message_id: String,
    },
    CommitPublished {
        intent_id: String,
    },
}

impl GroupIntentJson {
    fn into_intent(self) -> Result<GroupIntent, CoreError> {
        Ok(match self {
            Self::Create {
                name,
                relationship_ids,
            } => GroupIntent::Create {
                name,
                relationship_ids,
            },
            Self::CreateWithProtection {
                name,
                relationship_ids,
                protection,
            } => GroupIntent::CreateWithProtection {
                name,
                relationship_ids,
                protection,
            },
            Self::RoomSetting {
                conversation_id,
                setting,
                confirmed,
            } => GroupIntent::RoomSetting {
                conversation_id: unhex(&conversation_id)?,
                setting,
                confirmed,
            },
            Self::Invite {
                conversation_id,
                relationship_id,
            } => GroupIntent::Invite {
                conversation_id: unhex(&conversation_id)?,
                relationship_id,
            },
            Self::AskToRejoin {
                relationship_id,
                room_hint,
            } => GroupIntent::AskToRejoin {
                relationship_id,
                room_hint,
            },
            Self::ResolveRejoin { request_id, agree } => {
                GroupIntent::ResolveRejoin { request_id, agree }
            }
            Self::Accept {
                invite_id,
                siblings,
            } => GroupIntent::Accept {
                invite_id,
                siblings,
            },
            Self::Decline { invite_id } => GroupIntent::Decline { invite_id },
            Self::Leave { conversation_id } => GroupIntent::Leave {
                conversation_id: unhex(&conversation_id)?,
            },
            Self::Remove {
                conversation_id,
                account,
            } => GroupIntent::Remove {
                conversation_id: unhex(&conversation_id)?,
                account: unhex(&account)?,
            },
            Self::SendText {
                conversation_id,
                text,
            } => GroupIntent::SendText {
                conversation_id: unhex(&conversation_id)?,
                text,
            },
            Self::SendReply {
                conversation_id,
                text,
                reply_to,
            } => GroupIntent::SendReply {
                conversation_id: unhex(&conversation_id)?,
                text,
                reply_to,
            },
            Self::SendMentions {
                conversation_id,
                text,
                reply_to,
                mentions,
            } => GroupIntent::SendMentions {
                conversation_id: unhex(&conversation_id)?,
                text,
                reply_to,
                mentions: mentions
                    .into_iter()
                    .map(|m| om_core::groups::flow::GroupMention {
                        start: m.start,
                        end: m.end,
                        account: m.account,
                    })
                    .collect(),
            },
            Self::CreatePoll {
                conversation_id,
                question,
                options,
                multiple,
                max_selections,
                visibility,
            } => GroupIntent::CreatePoll {
                conversation_id: unhex(&conversation_id)?,
                spec: om_core::events::PollSpec {
                    question,
                    options: options
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| (i as u8, l))
                        .collect(),
                    multiple,
                    max_selections,
                    visibility: match visibility.as_str() {
                        "immediate" => 1,
                        "after_vote" => 2,
                        "after_close" => 3,
                        _ => return Err(CoreError::Invalid("poll_visibility")),
                    },
                },
            },
            Self::VotePoll {
                conversation_id,
                poll_id,
                option_ids,
            } => GroupIntent::VotePoll {
                conversation_id: unhex(&conversation_id)?,
                poll_id,
                selected: option_ids,
            },
            Self::ClosePoll {
                conversation_id,
                poll_id,
            } => GroupIntent::ClosePoll {
                conversation_id: unhex(&conversation_id)?,
                poll_id,
            },
            Self::SetReaction {
                conversation_id,
                message_id,
                key,
                present,
            } => GroupIntent::SetReaction {
                conversation_id: unhex(&conversation_id)?,
                message_id,
                key,
                present,
            },
            Self::EditMessage {
                conversation_id,
                message_id,
                text,
            } => GroupIntent::EditMessage {
                conversation_id: unhex(&conversation_id)?,
                message_id,
                text,
            },
            Self::DeleteMessage {
                conversation_id,
                message_id,
            } => GroupIntent::DeleteMessage {
                conversation_id: unhex(&conversation_id)?,
                message_id,
            },
            Self::CommitPublished { intent_id } => GroupIntent::CommitPublished { intent_id },
        })
    }
}

/// Where a frame goes, as the `to` of a group `out` item: a relationship or one member device.
#[derive(Deserialize)]
#[serde(untagged)]
enum GroupToJson {
    Relationship { relationship: String },
    Device { device: String, account: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum GroupFromJson {
    Relationship(String),
    Device(String),
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum GroupQueryJson {
    Groups,
    Rejoin,
    Group { conversation_id: String },
    Invites,
    Messages { conversation_id: String },
    Notices { conversation_id: String },
}

fn group_event_json(e: &GroupEvent) -> Value {
    use om_core::crypto::hex;
    match e {
        GroupEvent::RejoinChanged => json!({ "kind": "rejoin_changed" }),
        GroupEvent::DeviceRemoved { conversation_id } => {
            json!({ "kind": "device_removed", "conversationId": hex(conversation_id) })
        }
        GroupEvent::Created { conversation_id } => {
            json!({ "kind": "created", "conversationId": hex(conversation_id) })
        }
        GroupEvent::InviteSent { invite_id } => {
            json!({ "kind": "invite_sent", "inviteId": invite_id })
        }
        GroupEvent::InviteReceived { invite_id } => {
            json!({ "kind": "invite_received", "inviteId": invite_id })
        }
        GroupEvent::InviteAccepted { invite_id } => {
            json!({ "kind": "invite_accepted", "inviteId": invite_id })
        }
        GroupEvent::InviteDeclined { invite_id } => {
            json!({ "kind": "invite_declined", "inviteId": invite_id })
        }
        GroupEvent::CommitPrepared { intent_id } => {
            json!({ "kind": "commit_prepared", "intentId": intent_id })
        }
        GroupEvent::AdmitBlocked { invite_id, reason } => {
            json!({ "kind": "admit_blocked", "inviteId": invite_id, "reason": reason })
        }
        GroupEvent::MemberAdded {
            conversation_id,
            account,
        } => {
            json!({ "kind": "member_added", "conversationId": hex(conversation_id), "account": hex(account) })
        }
        GroupEvent::MemberRemoved {
            conversation_id,
            account,
        } => {
            json!({ "kind": "member_removed", "conversationId": hex(conversation_id), "account": hex(account) })
        }
        GroupEvent::Joined { conversation_id } => {
            json!({ "kind": "joined", "conversationId": hex(conversation_id) })
        }
        GroupEvent::NoLongerMember { conversation_id } => {
            json!({ "kind": "no_longer_member", "conversationId": hex(conversation_id) })
        }
        GroupEvent::MessageSent {
            conversation_id,
            id,
        } => json!({ "kind": "message_sent", "conversationId": hex(conversation_id), "id": id }),
        GroupEvent::MessageReceived {
            conversation_id,
            id,
            sender_account,
        } => {
            json!({ "kind": "message_received", "conversationId": hex(conversation_id), "id": id,
                "senderAccount": hex(sender_account) })
        }
        GroupEvent::MessageChanged {
            conversation_id,
            id,
        } => json!({ "kind": "message_changed", "conversationId": hex(conversation_id), "id": id }),
        GroupEvent::Dropped(reason) => json!({ "kind": "dropped", "reason": reason }),
        GroupEvent::Held(reason) => json!({ "kind": "held", "reason": reason }),
        GroupEvent::PolicyChanged {
            conversation_id,
            by_account,
            changes,
        } => json!({
            "kind": "policy_changed",
            "conversationId": hex(conversation_id),
            "byAccount": hex(by_account),
            "changes": changes,
        }),
        GroupEvent::RoomSettingBlocked {
            conversation_id,
            reason,
        } => json!({
            "kind": "room_setting_blocked",
            "conversationId": hex(conversation_id),
            "reason": reason,
        }),
        GroupEvent::CommitSuperseded { intent_id } => {
            json!({ "kind": "commit_superseded", "intentId": intent_id })
        }
    }
}

fn group_reply_json(r: &GroupReply) -> Value {
    use om_core::crypto::hex;
    json!({
        "events": r.events.iter().map(group_event_json).collect::<Vec<_>>(),
        "out": r.out.iter().map(|o| json!({
            "to": match &o.to {
                Target::Relationship(id) => json!({ "relationship": id }),
                Target::Device { account, device } =>
                    json!({ "device": hex(device), "account": hex(account) }),
            },
            "wire": serde_json::to_value(&o.wire).unwrap_or(Value::Null),
        })).collect::<Vec<_>>(),
    })
}

fn sealed_json(v: &[Sealed]) -> Value {
    use om_core::crypto::hex;
    Value::Array(
        v.iter()
            .map(|s| {
                json!({
                    "device": hex(&s.device),
                    "wrapper": hex(&s.wrapper),
                    "entry": serde_json::to_value(&s.entry).unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

/// One merged reply out of several (settle and pending retry finish one Commit or frame each).
fn merge_replies(v: Vec<GroupReply>) -> GroupReply {
    v.into_iter().fold(GroupReply::default(), |mut a, b| {
        a.events.extend(b.events);
        a.out.extend(b.out);
        a
    })
}

/// A Room message as the UI reads it: the text, who and when, what it answers, whether it was edited, and its reactions as
/// (key, count, mine). The registers behind them (who set what, and their order) stay in Core.
/// A mention as the UI writes it: a character range of the text and the member's account (hex).
#[derive(Clone, Debug, serde::Deserialize)]
struct WireMention {
    start: usize,
    end: usize,
    account: String,
}

fn group_message_json(m: &om_core::groups::flow::GroupMsg) -> Value {
    let mut v = json!({
        "id": m.id, "senderAccount": m.sender_account, "senderDevice": m.sender_device, "epoch": m.epoch,
        "text": m.text, "atMs": m.at_ms, "outgoing": m.outgoing,
    });
    if let Some(r) = &m.reply_to {
        v["replyTo"] = json!(r);
    }
    if m.edited {
        v["edited"] = json!(true);
    }
    if !m.mentions.is_empty() {
        v["mentions"] = json!(m.mentions);
    }
    if let Some(p) = &m.poll_view {
        let mut poll = json!({
            "options": p.options.iter().map(|(id, label, count)| match count {
                Some(c) => json!({ "id": id, "label": label, "count": c }),
                None => json!({ "id": id, "label": label }),
            }).collect::<Vec<_>>(),
            "multiple": p.multiple,
            "visibility": match p.visibility { 1 => "immediate", 2 => "after_vote", _ => "after_close" },
            "closed": p.closed, "mine": p.mine, "canClose": p.can_close,
        });
        if let Some(n) = p.max_selections {
            poll["maxSelections"] = json!(n);
        }
        if let Some(n) = p.voters {
            poll["voters"] = json!(n);
        }
        v["poll"] = poll;
    }
    let counts = m.reaction_counts();
    if !counts.is_empty() {
        v["reactions"] = counts
            .into_iter()
            .map(|(key, count, mine)| json!({ "key": key, "count": count, "mine": mine }))
            .collect();
    }
    v
}

fn group_view_json(v: &GroupView) -> Value {
    match v {
        GroupView::Rejoin(r) => json!({ "kind": "rejoin", "rejoin": r }),
        GroupView::Groups(g) => json!({ "kind": "groups", "groups": g }),
        GroupView::Group(d) => json!({ "kind": "group", "group": d }),
        GroupView::Invites(i) => json!({ "kind": "invites", "invites": i }),
        GroupView::Messages(m) => {
            json!({ "kind": "messages", "messages": m.iter().map(group_message_json).collect::<Vec<_>>() })
        }
        GroupView::Notices(n) => json!({ "kind": "notices", "notices": n }),
    }
}

pub struct HostClock<F: Fn() -> u64>(pub F);
impl<F: Fn() -> u64> Clock for HostClock<F> {
    fn now_ms(&self) -> u64 {
        (self.0)()
    }
}

/// One Core instance behind the JSON API. `Bridge::new` takes the RNG and clock so the native tests can inject
/// deterministic ones; the wasm entry point passes the system RNG.
pub struct Bridge<R: RngCore + CryptoRng> {
    make_rng: Option<Box<dyn FnOnce() -> R>>,
    clock: Option<Box<dyn Fn() -> u64>>,
    core: Option<Core<R>>,
}

impl<R: RngCore + CryptoRng> Bridge<R> {
    pub fn new(make_rng: impl FnOnce() -> R + 'static, clock: impl Fn() -> u64 + 'static) -> Self {
        Bridge {
            make_rng: Some(Box::new(make_rng)),
            clock: Some(Box::new(clock)),
            core: None,
        }
    }

    /// Handle one request; always returns a JSON response, never panics on malformed input.
    pub fn handle(&mut self, request: &[u8]) -> Vec<u8> {
        let out = match serde_json::from_slice::<Command>(request) {
            Err(e) => err("invalid", &format!("malformed command: {e}")),
            Ok(cmd) => match self.run(cmd) {
                Ok(v) => json!({ "ok": v }),
                Err(e) => core_err(&e),
            },
        };
        serde_json::to_vec(&out)
            .unwrap_or_else(|_| br#"{"err":{"code":"internal","message":"encode"}}"#.to_vec())
    }

    fn core(&mut self) -> Result<&mut Core<R>, CoreError> {
        self.core
            .as_mut()
            .ok_or(CoreError::Invalid("not initialised"))
    }

    /// Binary-only key handoff. The ABI copies these zeroizing bytes to its shared input buffer
    /// for the driver to take and wipe; no secret passes through the JSON response encoder.
    pub(crate) fn take_call_keys(
        &mut self,
        request: &[u8],
    ) -> Option<om_core::calls::InstallMediaKeys> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Request {
            ctx: ProfileExecutionContext,
            call_session_id: String,
        }
        let request: Request = serde_json::from_slice(request).ok()?;
        self.core()
            .ok()?
            .take_call_keys(&request.ctx, &request.call_session_id)
            .ok()
            .flatten()
    }

    fn group_result(
        &mut self,
        f: impl FnOnce(
            &mut Core<R>,
        ) -> Result<
            (
                Result<GroupReply, om_core::groups::GroupError>,
                om_core::host::Step,
            ),
            CoreError,
        >,
    ) -> Result<Value, CoreError> {
        let (out, step) = f(self.core()?)?;
        Ok(match out {
            Ok(r) => {
                json!({ "reply": group_reply_json(&r), "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
            }
            Err(e) => json!({ "refused": e.0 }),
        })
    }

    fn run(&mut self, cmd: Command) -> Result<Value, CoreError> {
        let enc = |v: &dyn erased::Ser| v.to_value();
        match cmd {
            Command::Init { home, calls } => {
                if self.core.is_some() {
                    return Err(CoreError::Invalid("already initialised"));
                }
                let (Some(make_rng), Some(clock)) = (self.make_rng.take(), self.clock.take())
                else {
                    return Err(CoreError::Invalid("not initialisable"));
                };
                let mut core = Core::new(make_rng(), HostClock(clock), home);
                if calls {
                    core.enable_calls();
                }
                self.core = Some(core);
                Ok(Value::Null)
            }
            Command::OpenProfile {
                profile_id,
                records,
            } => Ok(enc(&self.core()?.open_profile(&profile_id, &records)?)),
            Command::CloseProfile { profile_id } => {
                self.core()?.close_profile(&profile_id);
                Ok(Value::Null)
            }
            Command::ContextFor { profile_id } => {
                Ok(enc(&self.core()?.context_for(&profile_id)?))
            }
            Command::Dispatch { intent } => Ok(enc(&self.core()?.dispatch(&intent)?)),
            Command::FinishCommit { commit_id, ok } => {
                Ok(enc(&self.core()?.finish_commit(commit_id, ok)?))
            }
            Command::NetOps { ctx } => Ok(enc(&self.core()?.net_ops(&ctx)?)),
            Command::NetResult {
                ctx,
                net_id,
                result,
            } => Ok(enc(&self.core()?.net_result(&ctx, net_id, result)?)),
            Command::PollTargets { ctx } => Ok(enc(&self.core()?.poll_targets(&ctx)?)),
            Command::Inbound {
                ctx,
                mailbox,
                items,
            } => {
                let mailbox =
                    om_core::crypto::unhex(&mailbox).map_err(|_| CoreError::Invalid("mailbox"))?;
                Ok(enc(&self.core()?.inbound(&ctx, &mailbox, &items)?))
            }
            Command::Query { query } => self.core()?.query(&query),
            Command::RecoveryTick { ctx, mono } => {
                Ok(enc(&self.core()?.recovery_tick(&ctx, &mono)?))
            }
            Command::CallTick { ctx } => Ok(enc(&self.core()?.call_tick(&ctx)?)),
            Command::CallDescription {
                ctx,
                call_session_id,
                ice,
            } => Ok(enc(&self.core()?.call_description(
                &ctx,
                &call_session_id,
                ice,
            )?)),
            Command::CallMediaState {
                ctx,
                call_session_id,
                state,
            } => Ok(enc(&self.core()?.call_media_state(
                &ctx,
                &call_session_id,
                state,
            )?)),
            Command::CallRouteFacts {
                ctx,
                call_session_id,
                route,
            } => Ok(enc(&self.core()?.call_route_facts(
                &ctx,
                &call_session_id,
                route,
            )?)),
            Command::NoticeTick { ctx } => Ok(enc(&self.core()?.notice_tick(&ctx)?)),
            Command::PendingOperations { ctx } => {
                Ok(enc(&self.core()?.pending_operations(&ctx)?))
            }
            Command::Outcome { ctx, operation_id } => {
                Ok(enc(&self.core()?.outcome(&ctx, &operation_id)?))
            }
            Command::BackupPlan => Ok(enc(&self.core()?.backup_plan())),
            Command::BackupExport { ctx, plan, key } => {
                let key = unhex(&key)?;
                Ok(json!(om_core::crypto::hex(
                    &self.core()?.export_history(&ctx, &plan, &key)?
                )))
            }
            Command::BackupInspect { file } => Ok(enc(&Core::<R>::inspect_backup(&unhex(&file)?)?)),
            Command::BackupImport { ctx, file, key } => {
                let (file, key) = (unhex(&file)?, unhex(&key)?);
                let (summary, commit) = self.core()?.import_history(&ctx, &file, &key)?;
                Ok(json!({ "summary": enc(&summary), "commit": enc(&commit) }))
            }
            Command::OfferHomes => Ok(json!(self.core()?.offer_homes())),
            Command::SupplyRoutes { routes } => Ok(json!(self.core()?.supply_routes(routes))),
            Command::SupplyRoomRoutes { ctx, routes } => {
                Ok(json!(self.core()?.supply_room_routes(&ctx, routes)?))
            }
            Command::SetBlobLimits { limits } => {
                self.core()?.set_blob_limits(limits);
                Ok(Value::Null)
            }
            Command::SetRecoveryPlace { place } => {
                let place = match place {
                    None => None,
                    Some(p) => Some(om_core::recovery_record::RecoveryPlace {
                        host: p.host,
                        port: p.port,
                        scope: unhex(&p.scope)?
                            .try_into()
                            .map_err(|_| CoreError::Invalid("scope"))?,
                    }),
                };
                self.core()?.set_recovery_place(place);
                Ok(Value::Null)
            }
            Command::PrepareAttachment {
                ctx,
                plaintext,
                meta,
            } => {
                let (out, step) =
                    self.core()?
                        .prepare_attachment(&ctx, &unhex(&plaintext)?, meta)?;
                Ok(match out {
                    Ok(prepared) => {
                        json!({ "prepared": enc(&prepared), "commit": enc(&step.commit) })
                    }
                    // A refusal can still carry a commit (housekeeping the same transition did); the host finishes it.
                    Err(e) => json!({ "refused": enc(&e), "commit": enc(&step.commit) }),
                })
            }
            Command::RestageAttachment {
                ctx,
                blob_id,
                plaintext,
            } => Ok(
                match self
                    .core()?
                    .restage_attachment(&ctx, &blob_id, &unhex(&plaintext)?)
                {
                    Ok(ct) => json!({ "ciphertext": om_core::crypto::hex(&ct) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::AttachmentFetch {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.attachment_fetch(&ctx, &message_id, index) {
                    Ok(call) => json!({ "call": enc(&call) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::OpenAttachment {
                ctx,
                message_id,
                index,
                ciphertext,
            } => Ok(
                match self
                    .core()?
                    .open_attachment(&ctx, &message_id, index, &unhex(&ciphertext)?)
                {
                    Ok(o) => json!({ "opened": enc(&o) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::PrepareDirectAttachment {
                ctx,
                plaintext,
                meta,
            } => {
                let (out, step) =
                    self.core()?
                        .prepare_direct_attachment(&ctx, &unhex(&plaintext)?, meta)?;
                Ok(match out {
                    Ok(prepared) => {
                        json!({ "prepared": enc(&prepared), "commit": enc(&step.commit) })
                    }
                    Err(e) => json!({ "refused": enc(&e), "commit": enc(&step.commit) }),
                })
            }
            Command::ArmDirectOffer {
                ctx,
                blob_id,
                plaintext,
            } => Ok(
                match self
                    .core()?
                    .arm_direct_offer(&ctx, &blob_id, &unhex(&plaintext)?)
                {
                    Ok(()) => json!({ "armed": true }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::DirectServe { ctx, frame } => {
                let frame = frame_from_json(&frame)?;
                let (frames, step) = self.core()?.direct_serve(&ctx, &frame)?;
                Ok(json!({
                    "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>(),
                    "commit": enc(&step.commit),
                }))
            }
            Command::DirectPump { ctx, buffered } => {
                let frames = self.core()?.direct_pump(&ctx, buffered)?;
                Ok(json!(frames.iter().map(frame_to_json).collect::<Vec<_>>()))
            }
            Command::DirectFetchStart {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_start(&ctx, &message_id, index) {
                    Ok(f) => json!({ "frame": frame_to_json(&f) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::DirectFetchHandle {
                ctx,
                message_id,
                index,
                frame,
            } => {
                let frame = frame_from_json(&frame)?;
                Ok(
                    match self
                        .core()?
                        .direct_fetch_handle(&ctx, &message_id, index, &frame)
                    {
                        Ok(frames) => {
                            json!({ "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>() })
                        }
                        Err(e) => json!({ "refused": enc(&e) }),
                    },
                )
            }
            Command::DirectFetchProgress {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_progress(&ctx, &message_id, index) {
                    None => Value::Null,
                    Some((state, held)) => {
                        let (name, error) = match state {
                            FetchState::Idle => ("idle", None),
                            FetchState::Requested => ("requested", None),
                            FetchState::Receiving => ("receiving", None),
                            FetchState::Complete => ("complete", None),
                            FetchState::Committed => ("committed", None),
                            FetchState::Failed(e) => ("failed", Some(enc(&e))),
                        };
                        json!({ "state": name, "held": held, "error": error })
                    }
                },
            ),
            Command::DirectFetchCiphertext {
                ctx,
                message_id,
                index,
            } => Ok(
                match self
                    .core()?
                    .direct_fetch_ciphertext(&ctx, &message_id, index)
                {
                    Some(ct) => json!({ "ciphertext": om_core::crypto::hex(&ct) }),
                    None => Value::Null,
                },
            ),
            Command::DirectFetchCommitted {
                ctx,
                message_id,
                index,
            } => Ok(
                match self
                    .core()?
                    .direct_fetch_committed(&ctx, &message_id, index)
                {
                    Ok(frames) => {
                        json!({ "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>() })
                    }
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::DirectFetchOpen {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_open(&ctx, &message_id, index) {
                    Ok(o) => json!({ "opened": enc(&o) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::GroupsAvailable => Ok(json!(Core::<R>::groups_available())),
            Command::GroupDo { ctx, intent } => {
                let intent = intent.into_intent()?;
                self.group_result(|c| c.group_do(&ctx, intent))
            }
            Command::GroupSettle { ctx } => self.group_result(|c| {
                let (r, step) = c.group_settle(&ctx)?;
                Ok((r.map(merge_replies), step))
            }),
            Command::GroupOutbox { ctx } => {
                let items = self.core()?.group_outbox(&ctx)?;
                Ok(json!({ "items": items.iter().map(|i| json!({
                    "id": i.id,
                    "intentId": i.intent_id,
                    "to": match &i.to {
                        om_core::groups::carriage::OutTarget::Relationship(r) => json!({ "relationship": r }),
                        om_core::groups::carriage::OutTarget::Device { account, device } =>
                            json!({ "device": device, "account": account }),
                    },
                    "wire": serde_json::to_value(&i.wire).unwrap_or(Value::Null),
                })).collect::<Vec<_>>() }))
            }
            Command::GroupOutboxDone { ctx, ids } => {
                let step = self.core()?.group_outbox_done(&ctx, &ids)?;
                Ok(json!({ "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) }))
            }
            Command::GroupPendingRefresh { ctx } => {
                let step = self.core()?.group_pending_refresh(&ctx)?;
                Ok(json!({ "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) }))
            }
            Command::GroupPendingRetry { ctx } => self.group_result(|c| {
                let (r, step) = c.group_pending_retry(&ctx)?;
                Ok((r.map(merge_replies), step))
            }),
            Command::GroupRouteUpdate {
                ctx,
                conversation_id,
            } => {
                let conversation_id = unhex(&conversation_id)?;
                let (r, step) = self.core()?.group_route_update(&ctx, &conversation_id)?;
                Ok(match r {
                    Ok(queued) => {
                        json!({ "queued": queued, "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
                    }
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::GroupLaneSend {
                ctx,
                relationship_id,
                wire,
            } => {
                let (r, step) = self
                    .core()?
                    .group_lane_send(&ctx, &relationship_id, &wire)?;
                Ok(match r {
                    Ok(()) => {
                        json!({ "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
                    }
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::GroupLaneFlush { ctx } => {
                let (r, step) = self.core()?.group_lane_flush(&ctx)?;
                Ok(match r {
                    Ok(moved) => {
                        json!({ "moved": moved, "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
                    }
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::GroupLanePump { ctx } => {
                let (pumped, step) = self.core()?.group_lane_pump(&ctx)?;
                let mut merged = GroupReply::default();
                let mut list = vec![];
                for (rel, r) in pumped {
                    match r {
                        Ok(r) => {
                            list.push(json!({ "relationshipId": rel }));
                            merged = merge_replies(vec![merged, r]);
                        }
                        Err(e) => list.push(json!({ "relationshipId": rel, "refused": e.0 })),
                    }
                }
                Ok(json!({
                    "reply": group_reply_json(&merged),
                    "pumped": list,
                    "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null),
                }))
            }
            Command::GroupFrameSend { ctx, to, wire } => {
                let to = match to {
                    GroupToJson::Relationship { relationship } => {
                        Target::Relationship(relationship)
                    }
                    GroupToJson::Device { device, account } => Target::Device {
                        account: unhex(&account)?,
                        device: unhex(&device)?,
                    },
                };
                let (r, step) = self
                    .core()?
                    .group_frame_send(&ctx, &Outbound { to, wire })?;
                Ok(match r {
                    Ok(sealed) => {
                        json!({ "sealed": sealed_json(&sealed), "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
                    }
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::GroupFrameReceive { ctx, wrapper } => {
                let wrapper = unhex(&wrapper)?;
                self.group_result(|c| c.group_frame_receive(&ctx, &wrapper))
            }
            Command::GroupRoutesExpire { ctx } => {
                let (revoke, step) = self.core()?.group_routes_expire(&ctx)?;
                Ok(json!({
                    "revoke": serde_json::to_value(&revoke).unwrap_or(Value::Null),
                    "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null),
                }))
            }
            Command::GroupRouteRevoked { ctx, revoke } => {
                let step = self.core()?.group_route_revoked(&ctx, &revoke)?;
                Ok(json!({ "commit": step.commit }))
            }
            Command::GroupCarriageStatus { ctx } => {
                let st = self.core()?.group_carriage_status(&ctx)?;
                Ok(json!({
                    "mailboxes": st.mailboxes.iter().map(|m| json!({
                        "mailbox": om_core::crypto::hex(&m.mailbox),
                        "relayUri": m.relay_uri,
                    })).collect::<Vec<_>>(),
                    "routeOwed": st.route_owed,
                    "outbox": st.outbox,
                    "pending": st.pending,
                    "laneOut": st.lane_out,
                    "laneLost": st.lane_lost,
                }))
            }
            Command::GroupHeldInvitations { ctx } => {
                let h = self.core()?.group_held_invitations(&ctx)?;
                Ok(json!({
                    "invitations": serde_json::to_value(&h.invitations).unwrap_or(Value::Null),
                    "overflow": serde_json::to_value(h.overflow).unwrap_or(Value::Null),
                }))
            }
            Command::GroupLaneOverflowAck { ctx } => {
                let step = self.core()?.group_lane_overflow_ack(&ctx)?;
                Ok(json!({ "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) }))
            }
            Command::GroupReceive { ctx, from, wire } => {
                let from = match from {
                    GroupFromJson::Relationship(id) => Sender::Relationship(id),
                    GroupFromJson::Device(d) => Sender::Device(unhex(&d)?),
                };
                self.group_result(|c| c.group_receive(&ctx, from, wire))
            }
            Command::GroupQuery { ctx, query } => {
                let q = match query {
                    GroupQueryJson::Groups => GroupQuery::Groups,
                    GroupQueryJson::Rejoin => GroupQuery::Rejoin,
                    GroupQueryJson::Invites => GroupQuery::Invites,
                    GroupQueryJson::Group { conversation_id } => GroupQuery::Group {
                        conversation_id: unhex(&conversation_id)?,
                    },
                    GroupQueryJson::Messages { conversation_id } => GroupQuery::Messages {
                        conversation_id: unhex(&conversation_id)?,
                    },
                    GroupQueryJson::Notices { conversation_id } => GroupQuery::Notices {
                        conversation_id: unhex(&conversation_id)?,
                    },
                };
                Ok(match self.core()?.group_query(&ctx, q)? {
                    Ok(v) => group_view_json(&v),
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::RetireHome {
                ctx,
                relay_uri,
                emergency,
            } => {
                let (stuck, step) = self.core()?.retire_home(&ctx, &relay_uri, emergency)?;
                Ok(json!({ "stuck": stuck, "commit": enc(&step.commit) }))
            }
        }
    }
}

/// The production constructor: system CSPRNG (host entropy on wasm) and a host clock.
pub fn system_bridge(clock: impl Fn() -> u64 + 'static) -> Bridge<rng::SystemRng> {
    Bridge::new(rng::system_rng, clock)
}

fn err(code: &str, message: &str) -> Value {
    json!({ "err": { "code": code, "message": message } })
}

fn unhex(s: &str) -> Result<Vec<u8>, CoreError> {
    om_core::crypto::unhex(s).map_err(|_| CoreError::Invalid("hex"))
}

fn hex_array<const N: usize>(v: &Value, key: &str) -> Result<[u8; N], CoreError> {
    let bytes = unhex(
        v.get(key)
            .and_then(Value::as_str)
            .ok_or(CoreError::Invalid("frame"))?,
    )?;
    bytes.try_into().map_err(|_| CoreError::Invalid("frame"))
}

/// JSON shape of a direct-transfer frame: `{kind, ...}` with bytes as hex and camelCase keys (no wire codec: the host
/// maps its channel's records to this).
fn frame_to_json(f: &DirectFrame) -> Value {
    use om_core::crypto::hex;
    match f {
        DirectFrame::FetchBlob {
            request_id,
            read_cap,
            blob_id,
            digest,
        } => json!({ "kind": "fetch_blob", "requestId": hex(request_id), "readCap": hex(read_cap),
            "blobId": hex(blob_id), "digest": hex(digest) }),
        DirectFrame::Begin {
            transfer_id,
            total_bytes,
            sha256,
        } => json!({ "kind": "begin", "transferId": hex(transfer_id), "totalBytes": total_bytes,
            "sha256": hex(sha256) }),
        DirectFrame::Chunk {
            transfer_id,
            offset,
            bytes,
        } => {
            json!({ "kind": "chunk", "transferId": hex(transfer_id), "offset": offset, "bytes": hex(bytes) })
        }
        DirectFrame::End { transfer_id } => {
            json!({ "kind": "end", "transferId": hex(transfer_id) })
        }
        DirectFrame::Ack {
            transfer_id,
            offset,
        } => json!({ "kind": "ack", "transferId": hex(transfer_id), "offset": offset }),
        DirectFrame::Error { id, code } => {
            json!({ "kind": "error", "id": hex(id), "code": *code as u16 })
        }
        DirectFrame::Receipt { transfer_id, tag } => {
            json!({ "kind": "receipt", "transferId": hex(transfer_id), "tag": hex(tag) })
        }
    }
}

fn frame_from_json(v: &Value) -> Result<DirectFrame, CoreError> {
    let bad = CoreError::Invalid("frame");
    let u64_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_u64)
            .ok_or(CoreError::Invalid("frame"))
    };
    Ok(match v.get("kind").and_then(Value::as_str).ok_or(bad)? {
        "fetch_blob" => DirectFrame::FetchBlob {
            request_id: hex_array::<TRANSFER_ID_LEN>(v, "requestId")?,
            read_cap: unhex(
                v.get("readCap")
                    .and_then(Value::as_str)
                    .ok_or(CoreError::Invalid("frame"))?,
            )?,
            blob_id: hex_array(v, "blobId")?,
            digest: hex_array(v, "digest")?,
        },
        "begin" => DirectFrame::Begin {
            transfer_id: hex_array(v, "transferId")?,
            total_bytes: u64_of("totalBytes")?,
            sha256: hex_array(v, "sha256")?,
        },
        "chunk" => DirectFrame::Chunk {
            transfer_id: hex_array(v, "transferId")?,
            offset: u64_of("offset")?,
            bytes: unhex(
                v.get("bytes")
                    .and_then(Value::as_str)
                    .ok_or(CoreError::Invalid("frame"))?,
            )?,
        },
        "end" => DirectFrame::End {
            transfer_id: hex_array(v, "transferId")?,
        },
        "ack" => DirectFrame::Ack {
            transfer_id: hex_array(v, "transferId")?,
            offset: u64_of("offset")?,
        },
        "receipt" => DirectFrame::Receipt {
            transfer_id: hex_array(v, "transferId")?,
            tag: hex_array(v, "tag")?,
        },
        "error" => DirectFrame::Error {
            id: hex_array(v, "id")?,
            code: match u64_of("code")? {
                1 => ErrorCode::Unavailable,
                3 => ErrorCode::Integrity,
                _ => ErrorCode::Protocol,
            },
        },
        _ => return Err(CoreError::Invalid("frame")),
    })
}

fn core_err(e: &CoreError) -> Value {
    if let CoreError::Backup(b) = e {
        // `backup_wrong_password`, `backup_tampered`, ...: one code per cause, so the UI can say the right thing.
        return err(&format!("backup_{}", b.code()), &e.to_string());
    }
    let code = match e {
        CoreError::ProfileAccess(_) => "profile_access",
        CoreError::OperationIdConflict => "operation_id_conflict",
        CoreError::Busy => "busy",
        CoreError::CorruptState(_) => "corrupt_state",
        CoreError::NotFound(_) => "not_found",
        CoreError::Invalid(_) => "invalid",
        CoreError::Backup(_) => "invalid",
    };
    err(code, &e.to_string())
}

mod erased {
    pub trait Ser {
        fn to_value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }
}
