// SPDX-License-Identifier: MPL-2.0
//! The JSON command layer over [`om_core::Core`]. Pure safe Rust and host-independent, so it is unit-tested natively.

#[cfg(feature = "scope-engines")]
use crate::communities;
use om_core::Core;
use om_core::api::{HomeSummary, Intent, ProfileExecutionContext, Query};
use om_core::attach::Meta;
use om_core::backup::BackupPlan;
use om_core::crypto::rng::{self, CryptoRng, RngCore};
use om_core::direct::{DirectFrame, ErrorCode, FetchState, TRANSFER_ID_LEN};
#[cfg(feature = "expressive-media")]
use om_core::expressive as media;
#[cfg(feature = "scope-engines")]
mod scope;
use om_core::groups::carriage::Sealed;
use om_core::groups::flow::{
    DeviceRelease, GroupEvent, GroupIntent, GroupQuery, GroupReply, GroupView, GroupWire, Outbound,
    Sender, Target,
};
use om_core::groups::room::Setting;
use om_core::host::{BlobLimits, Clock, CoreError, Inbound, NetResult, Record};
use om_core::route::{DepositRevocation, QueueRetirement, RoomRouteOffer, RouteOffer};
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

#[cfg(feature = "expressive-media")]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExpressiveSource {
    Attachment { index: usize },
    Inline { index: usize },
    Reaction { key: String, mine: bool },
}

#[cfg(feature = "expressive-media")]
impl ExpressiveSource {
    fn source(&self) -> media::Source<'_> {
        match self {
            Self::Attachment { index } => media::Source::Attachment(*index),
            Self::Inline { index } => media::Source::Inline(*index),
            Self::Reaction { key, mine } => media::Source::Reaction { key, mine: *mine },
        }
    }
}

#[cfg(feature = "expressive-media")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackBlob {
    blob_id: String,
    ciphertext: String,
}

#[cfg(feature = "expressive-media")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackInput {
    release: String,
    control: Option<String>,
}

#[cfg(feature = "expressive-media")]
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ExpressiveIntent {
    InstallPack {
        input: PackInput,
        release_id: String,
        control_digest: Option<String>,
        policy: media::CachePolicy,
        favourites: Vec<String>,
        cached: Vec<PackBlob>,
    },
    RemovePack {
        pack_id: String,
    },
    UpdatePicker {
        pack_id: String,
        release_id: String,
        item_id: String,
        change: media::PickerChange,
    },
}

#[cfg(feature = "expressive-media")]
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ExpressiveQuery {
    PreviewPack {
        input: PackInput,
    },
    Packs,
    Picker {
        pack_id: String,
    },
    Fetch {
        message_id: String,
        source: ExpressiveSource,
    },
    Open {
        message_id: String,
        source: ExpressiveSource,
        ciphertext: String,
    },
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
        #[cfg(feature = "expressive-media")]
        #[serde(default)]
        expressive_media: bool,
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
    /// Stateless typed queries for the services and bridges scopes. Built only into the engineering artifact
    /// (feature `scope-engines`); off in every release until each scope's gate evidence is in.
    #[cfg(feature = "scope-engines")]
    BridgeFidelityPlan {
        semantics: Vec<u8>,
        max_attachment_bytes: Option<u64>,
        op: scope::FidelityOp,
    },
    #[cfg(feature = "scope-engines")]
    ServiceBrokerRequest {
        granted: Vec<String>,
        tainted: bool,
        policy: scope::EgressPolicyIn,
        name: String,
    },
    /// Review a single exact disclosure against the engineering broker boundary.
    #[cfg(feature = "scope-engines")]
    ServiceInvocationReview {
        ctx: ProfileExecutionContext,
        conversation_id: String,
        disclosure: scope::ServiceDisclosure,
    },
    /// Room topics engine for this bridge instance (one Room per profile bridge in the engineering artifact).
    #[cfg(feature = "scope-engines")]
    TopicsDo {
        ctx: Option<ProfileExecutionContext>,
        caps: Vec<String>,
        actor: String,
        op: scope::TopicOp,
    },
    #[cfg(feature = "scope-engines")]
    ModerationDo {
        ctx: Option<ProfileExecutionContext>,
        caps: Vec<String>,
        actor: String,
        op: scope::ModOp,
    },
    #[cfg(feature = "scope-engines")]
    SpaceDelegatedIntentCheck {
        body: String,
        binding: scope::BindingIn,
        held_head: String,
        held_version: u64,
        held_head_age: u64,
        via_subdelegation: bool,
    },
    #[cfg(feature = "scope-engines")]
    LanFirstContact {
        ctx: Option<ProfileExecutionContext>,
        step: scope::FirstContactStep,
    },
    #[cfg(feature = "scope-engines")]
    RoomServicesDo {
        room: String,
        caps: Vec<String>,
        ctx: Option<ProfileExecutionContext>,
        op: scope::RoomOp,
    },
    #[cfg(feature = "scope-engines")]
    PluginsDo {
        ctx: Option<ProfileExecutionContext>,
        request: serde_json::Value,
    },
    #[cfg(feature = "scope-engines")]
    Portability {
        request: serde_json::Value,
    },
    #[cfg(feature = "scope-engines")]
    BridgeRoomDo {
        ctx: Option<ProfileExecutionContext>,
        room: String,
        caps: Vec<String>,
        op: scope::BridgeRoomOp,
    },
    #[cfg(feature = "scope-engines")]
    RoomSnapshot {
        ctx: ProfileExecutionContext,
        room: String,
        kind: String,
        snapshot: Option<String>,
    },
    #[cfg(feature = "scope-engines")]
    LanTransferDo {
        ctx: Option<ProfileExecutionContext>,
        side: String,
        handle: String,
        op: scope::TransferOp,
    },
    /// `local_delivery`: seal a WebRTC offer or answer for the other person of `conversationId` under the pair's LAN seed
    /// (which stays inside Core). `kind` is `offer` or `answer`, `sessionId` 16 hex bytes. Answers `{envelope}` (hex) or
    /// `{refused}`.
    #[cfg(feature = "scope-engines")]
    LanSignalSeal {
        ctx: ProfileExecutionContext,
        conversation_id: String,
        kind: String,
        session_id: String,
        sdp: String,
    },
    /// Open a sealed signal from the other person. Answers `{kind, sessionId, sdp}` or `{refused}`.
    #[cfg(feature = "scope-engines")]
    LanSignalOpen {
        ctx: ProfileExecutionContext,
        conversation_id: String,
        bytes: String,
    },
    /// `local_delivery`: the person's "find each other on the same Wi-Fi" choice for one conversation. Answers
    /// `{enabled, available}`.
    #[cfg(feature = "scope-engines")]
    LanNearbySet {
        ctx: ProfileExecutionContext,
        conversation_id: String,
        on: bool,
    },
    #[cfg(feature = "scope-engines")]
    LanNearbyState {
        ctx: ProfileExecutionContext,
        conversation_id: String,
    },
    /// A Room observes a signed service or bridge control state (ADR-0269): `signed` is the hex of a SignedEnvelope,
    /// `kind` is `service` or `bridge`, `approvers` is what the host reports at install (recorded as reported).
    #[cfg(feature = "scope-engines")]
    RoomControlObserve {
        ctx: ProfileExecutionContext,
        room: String,
        kind: String,
        signed: String,
        #[serde(default)]
        approvers: Vec<scope::ApproverIn>,
    },
    #[cfg(feature = "scope-engines")]
    RoomControlView {
        ctx: ProfileExecutionContext,
        room: String,
    },
    /// The tags to advertise now (hex list; empty when nobody is turned on).
    #[cfg(feature = "scope-engines")]
    LanAdvertise {
        ctx: ProfileExecutionContext,
    },
    /// The conversation an observed tag belongs to, or `{conversationId: null}`.
    #[cfg(feature = "scope-engines")]
    LanObserve {
        ctx: ProfileExecutionContext,
        tag: String,
    },
    #[cfg(feature = "scope-engines")]
    LanFrameEncode {
        frame: scope::FrameIn,
    },
    #[cfg(feature = "scope-engines")]
    LanFrameDecode {
        bytes: String,
    },
    #[cfg(feature = "scope-engines")]
    PublicationDo {
        ctx: ProfileExecutionContext,
        handle: String,
        op: communities::PublicationOp,
    },
    #[cfg(feature = "scope-engines")]
    SpaceDo {
        ctx: ProfileExecutionContext,
        handle: String,
        op: communities::SpaceOp,
    },
    #[cfg(feature = "scope-engines")]
    DirectoryDo {
        ctx: ProfileExecutionContext,
        op: communities::DirectoryOp,
    },
    #[cfg(feature = "scope-engines")]
    SpaceBindingDo {
        ctx: Option<ProfileExecutionContext>,
        handle: String,
        op: scope::LinkOp,
    },
    #[cfg(feature = "scope-engines")]
    ModerationView {
        ctx: Option<ProfileExecutionContext>,
        who: String,
    },
    #[cfg(feature = "scope-engines")]
    TopicsView {
        ctx: Option<ProfileExecutionContext>,
        topic: Option<String>,
        message: Option<String>,
    },
    ExpressiveAvailable,
    #[cfg(feature = "expressive-media")]
    ExpressiveBlobExposure {
        policy: media::BlobPolicy,
    },
    #[cfg(feature = "expressive-media")]
    ExpressiveDo {
        ctx: ProfileExecutionContext,
        intent: ExpressiveIntent,
    },
    #[cfg(feature = "expressive-media")]
    ExpressiveQuery {
        ctx: ProfileExecutionContext,
        query: ExpressiveQuery,
    },
    #[cfg(feature = "expressive-media")]
    PrepareExpressive {
        ctx: ProfileExecutionContext,
        pack_id: String,
        release_id: String,
        item_id: String,
        ciphertext: String,
        #[serde(default)]
        policy: media::BlobPolicy,
    },
    #[cfg(feature = "expressive-media")]
    ExpressiveDecode {
        ctx: ProfileExecutionContext,
        message_id: String,
        source: ExpressiveSource,
        ciphertext: String,
        observed: Option<media::DecodeFacts>,
    },
    #[cfg(feature = "expressive-media")]
    PackFileAppend {
        ctx: ProfileExecutionContext,
        offset: usize,
        bytes: String,
    },
    #[cfg(feature = "expressive-media")]
    PackFileReview {
        ctx: ProfileExecutionContext,
    },
    #[cfg(feature = "expressive-media")]
    PackInstallBegin {
        ctx: ProfileExecutionContext,
        ticket: u64,
        release_id: String,
        control_digest: Option<String>,
    },
    #[cfg(feature = "expressive-media")]
    PackInstallStage {
        ctx: ProfileExecutionContext,
        ticket: u64,
    },
    #[cfg(feature = "expressive-media")]
    PackInstallCommit {
        ctx: ProfileExecutionContext,
        ticket: u64,
    },
    #[cfg(feature = "expressive-media")]
    PackInstallAbort {
        ctx: ProfileExecutionContext,
        ticket: Option<u64>,
    },
    #[cfg(feature = "expressive-media")]
    PackCacheDigests {
        ctx: ProfileExecutionContext,
    },
    #[cfg(feature = "expressive-media")]
    PackCacheOpen {
        ctx: ProfileExecutionContext,
        pack_id: String,
        release_id: String,
        item_id: String,
        ciphertext: String,
        observed: Option<media::DecodeFacts>,
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
    RoomGrantClaimed {
        ctx: ProfileExecutionContext,
        grant: DepositRevocation,
    },
    RetireRoomGrant {
        ctx: ProfileExecutionContext,
        grant: DepositRevocation,
        expires_at: u64,
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
        #[serde(default)]
        channel: u64,
        frame: Value,
    },
    /// Frames the sender may send now, given how many bytes the channel still has buffered.
    DirectPump {
        ctx: ProfileExecutionContext,
        #[serde(default)]
        channel: u64,
        buffered: usize,
    },
    DirectDisconnected {
        ctx: ProfileExecutionContext,
        channel: u64,
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
    /// Dedicated queues due for retirement at their Home (ADR-0229 section 3): `{retire: [{relayUri, mailbox, requestId}], commit}`.
    GroupQueueRetires {
        ctx: ProfileExecutionContext,
    },
    GroupQueueRetired {
        ctx: ProfileExecutionContext,
        retired: QueueRetirement,
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
    if let Some((id, attachment)) = &m.attachment_view {
        v["attachmentMessageId"] = json!(id);
        v["attachments"] = json!([attachment]);
    }
    if m.expressive_fallback {
        v["expressiveFallback"] = json!(true);
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
    #[cfg(feature = "scope-engines")]
    topics: std::collections::BTreeMap<String, om_core::room_topics::RoomTopics>,
    #[cfg(feature = "scope-engines")]
    moderation: std::collections::BTreeMap<String, om_core::room_moderation::RoomModeration>,
    #[cfg(feature = "scope-engines")]
    first_contact: std::collections::BTreeMap<String, om_core::lan_first_contact::FirstContact>,
    #[cfg(feature = "scope-engines")]
    handles: std::collections::BTreeMap<String, scope::Handles>,
    #[cfg(all(test, feature = "scope-engines"))]
    test_only_handles: Option<scope::Handles>,
    #[cfg(feature = "scope-engines")]
    runtime_clock: Option<Box<dyn Fn() -> Option<u64>>>,
    #[cfg(feature = "scope-engines")]
    last_runtime_ms: Option<u64>,
    #[cfg(feature = "scope-engines")]
    community_handles: communities::Handles,
    #[cfg(feature = "scope-engines")]
    plugin_handles: std::collections::BTreeMap<String, crate::plugins::Handles>,
}

impl<R: RngCore + CryptoRng> Bridge<R> {
    pub fn new(make_rng: impl FnOnce() -> R + 'static, clock: impl Fn() -> u64 + 'static) -> Self {
        Bridge {
            make_rng: Some(Box::new(make_rng)),
            clock: Some(Box::new(clock)),
            core: None,
            #[cfg(feature = "scope-engines")]
            topics: Default::default(),
            #[cfg(feature = "scope-engines")]
            moderation: Default::default(),
            #[cfg(feature = "scope-engines")]
            first_contact: Default::default(),
            #[cfg(feature = "scope-engines")]
            handles: Default::default(),
            #[cfg(all(test, feature = "scope-engines"))]
            test_only_handles: None,
            #[cfg(feature = "scope-engines")]
            runtime_clock: None,
            #[cfg(feature = "scope-engines")]
            last_runtime_ms: None,
            #[cfg(feature = "scope-engines")]
            community_handles: Default::default(),
            #[cfg(feature = "scope-engines")]
            plugin_handles: Default::default(),
        }
    }

    #[cfg(feature = "scope-engines")]
    pub(crate) fn with_runtime_clock(mut self, clock: impl Fn() -> Option<u64> + 'static) -> Self {
        self.runtime_clock = Some(Box::new(clock));
        self
    }

    #[cfg(all(test, feature = "scope-engines"))]
    fn with_test_only_scope_reducers(mut self) -> Self {
        self.test_only_handles = Some(scope::Handles::default());
        self
    }

    #[cfg(feature = "scope-engines")]
    fn runtime_now(&mut self) -> Result<u64, CoreError> {
        let now = self
            .runtime_clock
            .as_ref()
            .and_then(|clock| clock())
            .filter(|&now| self.last_runtime_ms.is_none_or(|last| now >= last))
            .ok_or(CoreError::Invalid("monotonic clock unavailable"))?;
        self.last_runtime_ms = Some(now);
        Ok(now)
    }

    #[cfg(feature = "scope-engines")]
    fn clear_scope_profile(&mut self, profile: &str) {
        self.community_handles.close_profile(profile);
        self.handles.remove(profile);
        self.plugin_handles.remove(profile);
        self.topics.remove(profile);
        self.moderation.remove(profile);
        self.first_contact.remove(profile);
    }

    #[cfg(feature = "scope-engines")]
    fn scope_profile(&mut self, ctx: Option<ProfileExecutionContext>) -> Result<String, CoreError> {
        #[cfg(test)]
        if ctx.is_none() && self.test_only_handles.is_some() {
            return Ok(String::new());
        }
        let ctx = ctx.ok_or(CoreError::ProfileAccess("missing profile context"))?;
        self.core()?.community_reader(&ctx)?;
        Ok(ctx.profile_id)
    }

    #[cfg(feature = "scope-engines")]
    fn scope_room(
        &mut self,
        ctx: &ProfileExecutionContext,
        room: &str,
    ) -> Result<Option<String>, CoreError> {
        let Ok((profile, conversation)) = serde_json::from_str::<(String, String)>(room) else {
            return Ok(None);
        };
        Ok(
            (profile == ctx.profile_id && self.core()?.room_control_reader(ctx, &conversation)?)
                .then_some(conversation),
        )
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
            Command::Init {
                home,
                calls,
                #[cfg(feature = "expressive-media")]
                expressive_media,
            } => {
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
                #[cfg(feature = "expressive-media")]
                if expressive_media {
                    core.enable_expressive_media();
                }
                self.core = Some(core);
                Ok(Value::Null)
            }
            Command::OpenProfile {
                profile_id,
                records,
            } => {
                let ctx = self.core()?.open_profile(&profile_id, &records)?;
                #[cfg(feature = "scope-engines")]
                self.clear_scope_profile(&profile_id);
                Ok(enc(&ctx))
            }
            Command::CloseProfile { profile_id } => {
                self.core()?.close_profile(&profile_id);
                #[cfg(feature = "scope-engines")]
                self.clear_scope_profile(&profile_id);
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
            #[cfg(feature = "scope-engines")]
            Command::BridgeFidelityPlan {
                semantics,
                max_attachment_bytes,
                op,
            } => Ok(scope::fidelity_plan(&semantics, max_attachment_bytes, op)),
            #[cfg(feature = "scope-engines")]
            Command::ServiceBrokerRequest {
                granted,
                tainted,
                policy,
                name,
            } => Ok(scope::broker_request(&granted, tainted, policy, &name)),
            #[cfg(feature = "scope-engines")]
            Command::ServiceInvocationReview {
                ctx,
                conversation_id,
                disclosure,
            } => {
                let core = self.core()?;
                if !core.room_control_reader(&ctx, &conversation_id)? {
                    return Ok(json!({ "refused": "not_authorized" }));
                }
                Ok(scope::service_invocation_review(
                    disclosure,
                    core.wall_now_ms(),
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::TopicsDo {
                ctx,
                caps,
                actor,
                op,
            } => {
                let profile = self.scope_profile(ctx)?;
                Ok(scope::topics_do(
                    self.topics.entry(profile).or_default(),
                    &caps,
                    &actor,
                    op,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::ModerationDo {
                ctx,
                caps,
                actor,
                op,
            } => {
                let profile = self.scope_profile(ctx)?;
                Ok(scope::moderation_do(
                    self.moderation.entry(profile).or_default(),
                    &caps,
                    &actor,
                    op,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::SpaceDelegatedIntentCheck {
                body,
                binding,
                held_head,
                held_version,
                held_head_age,
                via_subdelegation,
            } => Ok(scope::space_intent_check(
                &body,
                binding,
                &held_head,
                held_version,
                held_head_age,
                via_subdelegation,
            )),
            #[cfg(feature = "scope-engines")]
            Command::LanFirstContact { ctx, step } => {
                let profile = self.scope_profile(ctx)?;
                Ok(scope::first_contact(
                    self.first_contact.entry(profile).or_default(),
                    step,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::RoomServicesDo {
                room,
                caps,
                ctx,
                op,
            } => {
                #[cfg(test)]
                if ctx.is_none()
                    && let Some(handles) = self.test_only_handles.as_mut()
                {
                    return Ok(scope::room_services_do(handles, &room, &caps, op, &[]));
                }
                let _ = caps; // Engineering handles have no authenticated capability source.
                let Some(ctx) = ctx else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                let Some(conversation) = self.scope_room(&ctx, &room)? else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                if op.needs_verified_authority() {
                    return Ok(json!({ "refused": "not_authorized" }));
                }
                let now = self.runtime_now()?;
                let controls = self.core()?.room_control_view(&ctx, &conversation, now)?;
                Ok(scope::room_services_do(
                    self.handles.entry(ctx.profile_id).or_default(),
                    &conversation,
                    &[],
                    op,
                    &controls,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::PluginsDo { ctx, request } => {
                if request.get("engine").and_then(Value::as_str) == Some("registry") {
                    return Ok(crate::plugins::plugins_do(&mut Default::default(), request));
                }
                let Some(ctx) = ctx else {
                    return Ok(crate::plugins::profile_refused());
                };
                self.core()?.community_reader(&ctx)?;
                Ok(crate::plugins::plugins_do(
                    self.plugin_handles.entry(ctx.profile_id).or_default(),
                    request,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::Portability { request } => Ok(crate::portability::portability(request)),
            #[cfg(feature = "scope-engines")]
            Command::BridgeRoomDo {
                room,
                caps,
                ctx,
                op,
            } => {
                #[cfg(test)]
                if ctx.is_none()
                    && let Some(handles) = self.test_only_handles.as_mut()
                {
                    return Ok(scope::bridge_room_do(handles, &room, &caps, op, &[], None));
                }
                let _ = caps;
                let Some(ctx) = ctx else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                let Some(conversation) = self.scope_room(&ctx, &room)? else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                let now = self.runtime_now()?;
                let controls = self.core()?.room_control_view(&ctx, &conversation, now)?;
                if op.needs_verified_authority() {
                    return Ok(json!({ "refused": "not_authorized" }));
                }
                Ok(scope::bridge_room_do(
                    self.handles.entry(ctx.profile_id).or_default(),
                    &conversation,
                    &[],
                    op,
                    &controls,
                    Some(now),
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::RoomSnapshot {
                ctx,
                room,
                kind,
                snapshot,
            } => {
                use om_core::room_snapshot::MAX_SNAPSHOT_BYTES;
                let Some(kind) = scope::control_kind(&kind) else {
                    return Ok(json!({ "refused": "malformed" }));
                };
                let Some(conversation) = self.scope_room(&ctx, &room)? else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                if let Some(snapshot) = snapshot {
                    let max_hex = (MAX_SNAPSHOT_BYTES + 32) * 2;
                    let Some(bytes) = (64..=max_hex)
                        .contains(&snapshot.len())
                        .then(|| scope::lan_bytes(&snapshot))
                        .flatten()
                    else {
                        return Ok(json!({ "refused": "invalid_snapshot" }));
                    };
                    let (tag, body) = bytes.split_at(32);
                    if self
                        .core()?
                        .room_snapshot_auth(&ctx, &conversation, kind, body, Some(tag))?
                        .is_none()
                    {
                        return Ok(json!({ "refused": "invalid_snapshot" }));
                    }
                    let now = self.runtime_now()?;
                    let controls = self.core()?.room_control_view(&ctx, &conversation, now)?;
                    Ok(self
                        .handles
                        .entry(ctx.profile_id)
                        .or_default()
                        .import_room_snapshot(&conversation, kind, body, &controls, now))
                } else {
                    let Some(snapshot) = self
                        .handles
                        .get(&ctx.profile_id)
                        .and_then(|handles| handles.export_room_snapshot(&conversation, kind))
                    else {
                        return Ok(json!({ "refused": "unknown_handle" }));
                    };
                    let body = match snapshot {
                        Ok(body) => body,
                        Err(error) => return Ok(scope::snapshot_refusal(error)),
                    };
                    let Some(tag) =
                        self.core()?
                            .room_snapshot_auth(&ctx, &conversation, kind, &body, None)?
                    else {
                        return Ok(json!({ "refused": "not_authorized" }));
                    };
                    Ok(json!({ "snapshot": scope::lan_hex(&[tag.as_slice(), &body].concat()) }))
                }
            }
            #[cfg(feature = "scope-engines")]
            Command::LanTransferDo {
                ctx,
                side,
                handle,
                op,
            } => {
                #[cfg(test)]
                if ctx.is_none()
                    && let Some(handles) = self.test_only_handles.as_mut()
                {
                    return Ok(scope::lan_transfer_do(handles, &side, &handle, op));
                }
                let Some(ctx) = ctx else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                self.core()?.community_reader(&ctx)?;
                Ok(scope::lan_transfer_do(
                    self.handles.entry(ctx.profile_id).or_default(),
                    &side,
                    &handle,
                    op,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanSignalSeal {
                ctx,
                conversation_id,
                kind,
                session_id,
                sdp,
            } => {
                let Some((kind, sid)) = scope::lan_signal_args(&kind, &session_id) else {
                    return Ok(json!({ "refused": "malformed" }));
                };
                let r = self
                    .core()?
                    .lan_signal_seal(&ctx, &conversation_id, kind, sid, &sdp)?;
                Ok(scope::lan_signal_sealed(r))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanSignalOpen {
                ctx,
                conversation_id,
                bytes,
            } => {
                let Some(b) = scope::lan_bytes(&bytes) else {
                    return Ok(json!({ "refused": "malformed" }));
                };
                let r = self.core()?.lan_signal_open(&ctx, &conversation_id, &b)?;
                Ok(scope::lan_signal_opened(r))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanNearbySet {
                ctx,
                conversation_id,
                on,
            } => {
                let s = self.core()?.lan_nearby_set(&ctx, &conversation_id, on)?;
                Ok(json!({ "enabled": s.enabled, "available": s.available }))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanNearbyState {
                ctx,
                conversation_id,
            } => {
                let s = self.core()?.lan_nearby_state(&ctx, &conversation_id)?;
                Ok(json!({ "enabled": s.enabled, "available": s.available }))
            }
            #[cfg(feature = "scope-engines")]
            Command::RoomControlObserve {
                ctx,
                room,
                kind,
                signed,
                approvers,
            } => {
                let (Some(kind), Some(bytes), Some(approvers)) = (
                    scope::control_kind(&kind),
                    scope::lan_bytes(&signed),
                    scope::approvers(&approvers),
                ) else {
                    return Ok(json!({ "refused": "malformed" }));
                };
                let now = self.runtime_now()?;
                let r = self
                    .core()?
                    .room_control_observe(&ctx, &room, kind, &bytes, approvers, now)?;
                if let Ok((_, Some(ref accepted))) = r {
                    let view = self.core()?.room_control_view(&ctx, &room, now)?;
                    let canonical = view.iter().any(|entry| {
                        entry.kind == kind
                            && entry.id == accepted.service_id
                            && !entry.frozen
                            && entry.head_hash == Some(accepted.hash)
                    });
                    if let Some(handles) = self.handles.get_mut(&ctx.profile_id) {
                        handles.observe_room_control(&room, kind, accepted, canonical, now);
                    }
                }
                Ok(scope::control_observed(r.map(|(outcome, _)| outcome)))
            }
            #[cfg(feature = "scope-engines")]
            Command::RoomControlView { ctx, room } => {
                let now = self.runtime_now()?;
                let v = self.core()?.room_control_view(&ctx, &room, now)?;
                Ok(scope::control_view(&v))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanAdvertise { ctx } => {
                let tags = self.core()?.lan_advertise(&ctx)?;
                let tags: Vec<String> = tags.iter().map(|t| scope::lan_hex(t)).collect();
                Ok(json!({ "tags": tags }))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanObserve { ctx, tag } => {
                let Some(t) = scope::lan_bytes(&tag).and_then(|b| <[u8; 16]>::try_from(b).ok())
                else {
                    return Ok(json!({ "refused": "malformed" }));
                };
                let c = self.core()?.lan_observe(&ctx, &t)?;
                Ok(json!({ "conversationId": c }))
            }
            #[cfg(feature = "scope-engines")]
            Command::LanFrameEncode { frame } => Ok(scope::lan_frame_encode(frame)),
            #[cfg(feature = "scope-engines")]
            Command::LanFrameDecode { bytes } => Ok(scope::lan_frame_decode(&bytes)),
            #[cfg(feature = "scope-engines")]
            Command::PublicationDo { ctx, handle, op } => {
                let core = self.core()?;
                core.community_reader(&ctx)?;
                let accounts = if matches!(op, communities::PublicationOp::AcceptControl { .. }) {
                    core.room_control_accounts(&ctx)?
                } else {
                    Vec::new()
                };
                Ok(self
                    .community_handles
                    .publication_do(&ctx, &handle, op, &accounts))
            }
            #[cfg(feature = "scope-engines")]
            Command::SpaceDo { ctx, handle, op } => {
                let core = self.core()?;
                let reader = core.community_reader(&ctx)?;
                let accounts = if matches!(op, communities::SpaceOp::AcceptState { .. }) {
                    core.room_control_accounts(&ctx)?
                } else {
                    Vec::new()
                };
                Ok(self
                    .community_handles
                    .space_do(&ctx, &handle, op, &accounts, reader))
            }
            #[cfg(feature = "scope-engines")]
            Command::DirectoryDo { ctx, op } => {
                self.core()?.community_reader(&ctx)?;
                Ok(self.community_handles.directory_do(&ctx, op))
            }
            #[cfg(feature = "scope-engines")]
            Command::SpaceBindingDo { ctx, handle, op } => {
                #[cfg(test)]
                if ctx.is_none()
                    && let Some(handles) = self.test_only_handles.as_mut()
                {
                    return Ok(scope::space_binding_do(handles, &handle, op));
                }
                let Some(ctx) = ctx else {
                    return Ok(json!({ "refused": "not_authorized" }));
                };
                self.core()?.community_reader(&ctx)?;
                if matches!(
                    op,
                    scope::LinkOp::ChildAccepts | scope::LinkOp::CreateBinding { .. }
                ) {
                    return Ok(json!({ "refused": "unverified_child_state" }));
                }
                Ok(scope::space_binding_do(
                    self.handles.entry(ctx.profile_id).or_default(),
                    &handle,
                    op,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::ModerationView { ctx, who } => {
                let profile = self.scope_profile(ctx)?;
                Ok(scope::moderation_view(
                    self.moderation.entry(profile).or_default(),
                    &who,
                ))
            }
            #[cfg(feature = "scope-engines")]
            Command::TopicsView {
                ctx,
                topic,
                message,
            } => {
                let profile = self.scope_profile(ctx)?;
                Ok(scope::topics_view(
                    self.topics.entry(profile).or_default(),
                    topic.as_deref(),
                    message.as_deref(),
                ))
            }
            Command::ExpressiveAvailable => Ok(json!(self.core()?.expressive_media_available())),
            #[cfg(feature = "expressive-media")]
            Command::ExpressiveBlobExposure { policy } => {
                let exposure = self
                    .core()?
                    .expressive_media_available()
                    .then(|| policy.metadata_exposure())
                    .flatten();
                Ok(json!(exposure.map(
                    |line| json!({ "kind": "shared_blob_linkage", "line": line })
                )))
            }
            #[cfg(feature = "expressive-media")]
            Command::ExpressiveDo { ctx, intent } => expressive_do(self.core()?, &ctx, intent),
            #[cfg(feature = "expressive-media")]
            Command::ExpressiveQuery { ctx, query } => expressive_query(self.core()?, &ctx, query),
            #[cfg(feature = "expressive-media")]
            Command::PrepareExpressive {
                ctx,
                pack_id,
                release_id,
                item_id,
                ciphertext,
                policy,
            } => {
                let core = self.core()?;
                let pack = id32(&pack_id)?;
                if !core.expressive_media_available() {
                    return Ok(json!({ "refused": om_core::attach::AttachError::Unsupported }));
                }
                if !core
                    .content_pack(&ctx, pack)?
                    .is_some_and(|p| om_core::crypto::hex(&p.release_id) == release_id)
                {
                    return Ok(json!({ "refused": om_core::attach::AttachError::NotFound }));
                }
                let (out, step) = core.prepare_expressive(
                    &ctx,
                    pack,
                    &item_id,
                    &expressive_ciphertext(&ciphertext)?,
                    policy,
                )?;
                Ok(match out {
                    Ok(prepared) => json!({ "prepared": prepared, "commit": step.commit }),
                    Err(e) => json!({ "refused": e, "commit": step.commit }),
                })
            }
            #[cfg(feature = "expressive-media")]
            Command::ExpressiveDecode {
                ctx,
                message_id,
                source,
                ciphertext,
                observed,
            } => Ok(json!(self.core()?.check_expressive_decode(
                &ctx,
                &message_id,
                source.source(),
                &expressive_ciphertext(&ciphertext)?,
                observed.as_ref()
            ))),
            #[cfg(feature = "expressive-media")]
            Command::PackFileAppend { ctx, offset, bytes } => {
                let bytes = bounded_hex(&bytes, 4 << 20)?;
                Ok(match self.core()?.append_pack_file(&ctx, offset, &bytes)? {
                    Ok(()) => json!({ "accepted": true }),
                    Err(reason) => json!({ "refused": reason }),
                })
            }
            #[cfg(feature = "expressive-media")]
            Command::PackFileReview { ctx } => {
                Ok(match self.core()?.review_uploaded_pack_file(&ctx)? {
                    Ok((ticket, preview)) => json!({ "ticket": ticket, "preview": preview }),
                    Err(reason) => json!({ "refused": reason }),
                })
            }
            #[cfg(feature = "expressive-media")]
            Command::PackInstallBegin {
                ctx,
                ticket,
                release_id,
                control_digest,
            } => Ok(
                match self.core()?.begin_pack_install(
                    &ctx,
                    ticket,
                    &release_id,
                    control_digest.as_deref(),
                )? {
                    Ok(count) => json!({ "count": count }),
                    Err(reason) => json!({ "refused": reason }),
                },
            ),
            #[cfg(feature = "expressive-media")]
            Command::PackInstallStage { ctx, ticket } => {
                Ok(match self.core()?.stage_pack_blob(&ctx, ticket)? {
                    Ok((digest, ciphertext)) => json!({
                        "digest": om_core::crypto::hex(&digest), "ciphertext": om_core::crypto::hex(&ciphertext)
                    }),
                    Err(reason) => json!({ "refused": reason }),
                })
            }
            #[cfg(feature = "expressive-media")]
            Command::PackInstallCommit { ctx, ticket } => {
                let (out, commit) = self.core()?.commit_pack_install(&ctx, ticket)?;
                Ok(match out {
                    Ok(()) => json!({ "accepted": true, "commit": commit }),
                    Err(reason) => json!({ "refused": reason }),
                })
            }
            #[cfg(feature = "expressive-media")]
            Command::PackInstallAbort { ctx, ticket } => {
                self.core()?.abort_pack_install(&ctx, ticket)?;
                Ok(Value::Null)
            }
            #[cfg(feature = "expressive-media")]
            Command::PackCacheDigests { ctx } => {
                let core = self.core()?;
                Ok(json!({
                    "digests": core.pack_cache_digests(&ctx)?,
                    "inventory": core.pack_inventory_digest(&ctx)?,
                }))
            }
            #[cfg(feature = "expressive-media")]
            Command::PackCacheOpen {
                ctx,
                pack_id,
                release_id,
                item_id,
                ciphertext,
                observed,
            } => {
                let opened = self.core()?.open_cached_pack_asset(
                    &ctx,
                    id32(&pack_id)?,
                    id32(&release_id)?,
                    &item_id,
                    &expressive_ciphertext(&ciphertext)?,
                );
                Ok(match opened {
                    Some((p, media::Opened::Renderable { plaintext, facts })) => {
                        if let Some(observed) = observed {
                            json!(facts.matches_decode(&observed))
                        } else {
                            json!({ "state": "renderable", "plaintext": om_core::crypto::hex(&plaintext),
                                "facts": facts, "kind": p.kind.as_str(), "mediaProfileId": p.media_profile_id,
                                "altText": p.alt_text })
                        }
                    }
                    Some((p, _)) => json!({ "state": "placeholder", "altText": p.alt_text }),
                    None => Value::Null,
                })
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
            Command::RoomGrantClaimed { ctx, grant } => {
                Ok(json!(self.core()?.room_grant_claimed(&ctx, &grant)?))
            }
            Command::RetireRoomGrant {
                ctx,
                grant,
                expires_at,
            } => {
                let step = self.core()?.retire_room_grant(&ctx, grant, expires_at)?;
                Ok(json!({ "commit": step.commit }))
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
            Command::DirectServe {
                ctx,
                channel,
                frame,
            } => {
                let frame = frame_from_json(&frame)?;
                let (frames, step) = self.core()?.direct_serve_on(&ctx, channel, &frame)?;
                Ok(json!({
                    "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>(),
                    "commit": enc(&step.commit),
                }))
            }
            Command::DirectPump {
                ctx,
                channel,
                buffered,
            } => {
                let frames = self.core()?.direct_pump_on(&ctx, channel, buffered)?;
                Ok(json!(frames.iter().map(frame_to_json).collect::<Vec<_>>()))
            }
            Command::DirectDisconnected { ctx, channel } => {
                self.core()?.direct_disconnected(&ctx, channel)?;
                Ok(Value::Null)
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
            Command::GroupQueueRetires { ctx } => {
                let (retire, step) = self.core()?.group_queue_retires(&ctx)?;
                Ok(json!({
                    "retire": serde_json::to_value(&retire).unwrap_or(Value::Null),
                    "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null),
                }))
            }
            Command::GroupQueueRetired { ctx, retired } => {
                let step = self.core()?.group_queue_retired(&ctx, &retired)?;
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

#[cfg(feature = "expressive-media")]
fn bounded_hex(s: &str, max: usize) -> Result<Vec<u8>, CoreError> {
    if s.len() > max * 2 {
        return Err(CoreError::Invalid("expressive size"));
    }
    unhex(s)
}

#[cfg(feature = "expressive-media")]
fn id32(s: &str) -> Result<[u8; 32], CoreError> {
    bounded_hex(s, 32)?
        .try_into()
        .map_err(|_| CoreError::Invalid("pack id"))
}

#[cfg(feature = "expressive-media")]
fn expressive_ciphertext(s: &str) -> Result<Vec<u8>, CoreError> {
    bounded_hex(s, (6 << 20) + om_core::attach::TAG_LEN as usize)
}

#[cfg(feature = "expressive-media")]
impl PackInput {
    fn bytes(&self) -> Result<(Vec<u8>, Option<Vec<u8>>), CoreError> {
        Ok((
            bounded_hex(&self.release, 2 << 20)?,
            self.control
                .as_deref()
                .map(|s| bounded_hex(s, 128 << 10))
                .transpose()?,
        ))
    }
}

#[cfg(feature = "expressive-media")]
fn expressive_do<R: RngCore + CryptoRng>(
    core: &mut Core<R>,
    ctx: &ProfileExecutionContext,
    intent: ExpressiveIntent,
) -> Result<Value, CoreError> {
    if !core.expressive_media_available() {
        return Ok(json!({ "refused": "expressive_disabled" }));
    }
    let (out, commit) = match intent {
        ExpressiveIntent::InstallPack {
            input,
            release_id,
            control_digest,
            policy,
            favourites,
            cached,
        } => {
            if cached.len() > 512 {
                return Ok(json!({ "refused": "value_range" }));
            }
            let (release, control) = input.bytes()?;
            let preview = match core.preview_content_pack(ctx, &release, control.as_deref())? {
                Ok(preview) => preview,
                Err(reason) => return Ok(json!({ "refused": reason })),
            };
            if preview.release_id != release_id || preview.control_digest != control_digest {
                return Ok(json!({ "refused": "confirmation_required" }));
            }
            let ids = cached
                .iter()
                .map(|blob| id32(&blob.blob_id))
                .collect::<Result<Vec<_>, _>>()?;
            let bodies = cached
                .iter()
                .map(|blob| expressive_ciphertext(&blob.ciphertext))
                .collect::<Result<Vec<_>, _>>()?;
            let cached: Vec<_> = ids
                .into_iter()
                .zip(&bodies)
                .map(|(blob_id, ciphertext)| media::CachedBlob {
                    blob_id,
                    ciphertext,
                })
                .collect();
            let confirmed_release = id32(&release_id)?;
            core.install_content_pack(
                ctx,
                media::Install {
                    release: &release,
                    expected_release: confirmed_release,
                    confirmed_release,
                    control: control.as_deref(),
                    policy,
                    favourites: &favourites,
                    cached: &cached,
                },
            )?
        }
        ExpressiveIntent::RemovePack { pack_id } => {
            core.remove_content_pack(ctx, id32(&pack_id)?)?
        }
        ExpressiveIntent::UpdatePicker {
            pack_id,
            release_id,
            item_id,
            change,
        } => core.update_expressive_picker(
            ctx,
            id32(&pack_id)?,
            id32(&release_id)?,
            &item_id,
            change,
        )?,
    };
    Ok(match out {
        Ok(()) => json!({ "accepted": true, "commit": commit }),
        Err(e) => json!({ "refused": e, "commit": commit }),
    })
}

#[cfg(feature = "expressive-media")]
fn expressive_query<R: RngCore + CryptoRng>(
    core: &Core<R>,
    ctx: &ProfileExecutionContext,
    query: ExpressiveQuery,
) -> Result<Value, CoreError> {
    use media::Opened;
    Ok(match query {
        ExpressiveQuery::PreviewPack { input } => {
            let (release, control) = input.bytes()?;
            match core.preview_content_pack(ctx, &release, control.as_deref())? {
                Ok(preview) => json!({ "preview": preview }),
                Err(reason) => json!({ "refused": reason }),
            }
        }
        ExpressiveQuery::Packs => json!(core.content_packs(ctx)?),
        ExpressiveQuery::Picker { pack_id } => json!(core.expressive_picker(ctx, id32(&pack_id)?)?),
        ExpressiveQuery::Fetch { message_id, source } => {
            let Some(p) = core.expressive_presentation(ctx, &message_id, source.source()) else {
                return Ok(Value::Null);
            };
            match core.expressive_fetch(ctx, &message_id, source.source()) {
                Ok(call) => json!({ "call": call, "altText": p.alt_text }),
                Err(_) => json!({ "altText": p.alt_text }),
            }
        }
        ExpressiveQuery::Open {
            message_id,
            source,
            ciphertext,
        } => {
            let Some(p) = core.expressive_presentation(ctx, &message_id, source.source()) else {
                return Ok(Value::Null);
            };
            match core.open_expressive(
                ctx,
                &message_id,
                source.source(),
                &expressive_ciphertext(&ciphertext)?,
            ) {
                Ok(Opened::Renderable { plaintext, facts }) => json!({
                    "state": "renderable", "plaintext": om_core::crypto::hex(&plaintext), "facts": facts,
                    "kind": p.kind.as_str(), "mediaProfileId": p.media_profile_id, "altText": p.alt_text,
                }),
                Ok(Opened::Placeholder { alt_text }) => {
                    json!({ "state": "placeholder", "altText": alt_text })
                }
                Err(_) => json!({ "state": "placeholder", "altText": p.alt_text }),
            }
        }
    })
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
