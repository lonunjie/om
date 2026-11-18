// SPDX-License-Identifier: MPL-2.0
//! The typed Core boundary: intents, queries, outcome classes and view models exactly as
//! `packages/om-platform-api/src/core` declares them (UI_CORE_PLATFORM_CONTRACT.md, ACTION_TO_CORE_INTENT_MAP.md).
//! The serde representation is the JSON the TypeScript binding exchanges; ids are opaque strings and Core
//! supplies machine codes only, never display copy.

use serde::{Deserialize, Serialize};

pub type OperationId = String;
pub type ProfileId = String;
pub type ConversationId = String;
pub type PersonId = String;
pub type MessageId = String;
pub type DeviceId = String;

/// Captured at dispatch (R-332); never re-derived from a process-global active profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExecutionContext {
    pub profile_id: ProfileId,
    pub generation: String,
}

// ---------------------------------------------------------------------------------------------- outcomes

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    Invalid,
    Unsupported,
    Unauthorized,
    Revoked,
    Expired,
    StaleState,
    Conflict,
    PolicyDenied,
    Quota,
    RateLimited,
    IntegrityFailure,
    Unavailable,
    NotFound,
    /// One object is bigger than the store will take (an advertised limit, or `payload_too_large`). Said before
    /// anything is sent; an exact retry cannot succeed, so it is not retryable.
    TooLarge,
    /// The contact is recovering their account (ADR-0190 section 5): nothing is sent to them until it finishes or ends,
    /// because their new device is not yet one this verifier may encrypt to. Nothing is queued; the message is not
    /// lost, it was never sent. Not retryable until the recovery ends.
    PeerRecovering,
}

impl RejectReason {
    /// Reasons for which an exact retry of the same logical operation may succeed.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            RejectReason::Unavailable | RejectReason::RateLimited | RejectReason::Quota
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserActionCode {
    AuthorizeWithDevice,
    ConfirmSecurityChange,
    Reauthenticate,
    UpdateClient,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OutcomeClass {
    Committed,
    PendingAuthorization,
    PendingNetwork,
    PendingReconciliation,
    NeedsUserAction { action: UserActionCode },
    Rejected { reason: RejectReason },
}

impl OutcomeClass {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OutcomeClass::Committed | OutcomeClass::Rejected { .. }
        )
    }
}

// ---------------------------------------------------------------------------------------------- intents

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub text: String,
    /// The message this one replies to (`reply_to`, EVENT_MODEL): a message of the same conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageId>,
}

/// A mention the composer wrote: the half-open character range `[start, end)` of `Draft::text` that is the label
/// (including its `@`), and the hex AccountID of the person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionInput {
    pub start: u32,
    pub end: u32,
    pub account_id: String,
}

/// A mention as shown: the range in the message text, the person, and whether it names us.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionView {
    pub start: u32,
    pub end: u32,
    pub account_id: String,
    pub me: bool,
}

/// One prepared attachment to send (`blob_id` from `Core::prepare_attachment`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInput {
    pub blob_id: String,
    /// Present on an audio attachment that is a voice note (`om.voice-note.v1`).
    #[serde(default)]
    pub voice_note: Option<VoiceNoteInput>,
}

/// An attachment sealed on this device. The host holds `ciphertext` (staged under `blob_id`) until Core's `PutBlob` for
/// it is performed; Core keeps only the key, the digest and the capabilities, never the ciphertext. Valid to use only
/// after the commit that came with it is durable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAttachment {
    pub blob_id: String,
    #[serde(with = "crate::host::hexs")]
    pub ciphertext: Vec<u8>,
    pub ciphertext_digest: String,
    pub ciphertext_len: u64,
}

/// A fetched attachment after its digest and authentication checked out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedAttachment {
    #[serde(with = "crate::host::hexs")]
    pub plaintext: Vec<u8>,
    pub media_type: String,
    pub filename: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceNoteInput {
    /// 16..256 normalized amplitude samples, lower-case hex. A presentation hint only.
    #[serde(default)]
    pub waveform: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockScope {
    Local,
    Provider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareScope {
    NameOnly,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPolicyLevel {
    Recommended,
    Hardened,
    Maximum,
    Custom,
}

impl PrivacyPolicyLevel {
    /// How much this level asks of the person (none is Recommended). A move to a lower rank is a loosening. Custom ranks with
    /// Hardened: it asks before a contact's recovery finishes, and it is never the way to go below either.
    pub fn rank(level: Option<Self>) -> u8 {
        match level {
            None | Some(Self::Recommended) => 0,
            Some(Self::Hardened | Self::Custom) => 1,
            Some(Self::Maximum) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryScope {
    Identity,
    History,
    Provider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryCustodianCode {
    SyncedPasskey,
    RecoveryKey,
    TrustedContacts,
    SpareDevice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallMedia {
    Audio,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySource {
    LinkedDevice,
    EncryptedBackup,
}

/// An explicit user confirmation minted by the UI's Action Registry. Core only requires it to be present for
/// destructive intents (UI-11); it carries no authority of its own.
pub type ConfirmationToken = serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum IntentBody {
    SendMessage {
        conversation_id: ConversationId,
        draft: Draft,
    },
    /// A text message with mentions inside `draft.text` (ADR-0199): each names a range of the text and an account of the
    /// conversation. Anything else about it is as for `SendMessage`.
    SendMessageMentioning {
        conversation_id: ConversationId,
        draft: Draft,
        mentions: Vec<MentionInput>,
    },
    RetryDelivery {
        message_id: MessageId,
    },
    /// Add or remove one of our reactions on a message of the conversation (`reaction.set`). The state is per person,
    /// message and reaction key, so repeating it is harmless and the latest wins on every device.
    /// Replace the text of one of our own messages (`message.edit`, RichText only: a photo or file is never replaced).
    EditMessage {
        conversation_id: ConversationId,
        message_id: MessageId,
        text: String,
    },
    /// Delete one of our own messages for everyone in the conversation (`message.delete`, a shared tombstone). It does
    /// not promise that a copy the other person already has is gone.
    DeleteMessage {
        conversation_id: ConversationId,
        message_id: MessageId,
    },
    SetReaction {
        conversation_id: ConversationId,
        message_id: MessageId,
        reaction: String,
        present: bool,
    },
    /// A message whose content is attachments the host prepared with `Core::prepare_attachment` (optionally with text).
    /// The objects are uploaded first and the message goes out only once every one is stored.
    SendAttachments {
        conversation_id: ConversationId,
        draft: Draft,
        attachments: Vec<AttachmentInput>,
    },
    /// A file too big for the delivery service, offered to be sent while the other person is online. The attachments
    /// were prepared with `Core::prepare_direct_attachment` (so Core already refused the spool path and said so). The
    /// message itself goes out at once through the ordinary sealed path; only the file waits (decision G-O1 of
    /// `2026-11-13-direct-transfer.md`).
    SendAttachmentsDirect {
        conversation_id: ConversationId,
        draft: Draft,
        attachments: Vec<AttachmentInput>,
    },
    /// Take a direct offer back: nothing is served any more. The message stays, and says the file was withdrawn.
    WithdrawDirectOffer {
        message_id: MessageId,
    },
    /// One-shot static location (`om.location-card.v1`): the point travels in the fallback text as a canonical `geo:`
    /// token, so every client shows it; a new client also draws the card.
    SendLocation {
        conversation_id: ConversationId,
        lat_e7: i32,
        lon_e7: i32,
        #[serde(default)]
        accuracy_m: Option<u32>,
        #[serde(default)]
        label: Option<String>,
        /// Text shown with the point (optional).
        #[serde(default)]
        note: Option<String>,
    },
    SwitchIdentityProfile {
        target_profile_id: ProfileId,
    },
    AcceptIntroduction {
        request_id: String,
    },
    BlockPerson {
        person_id: PersonId,
        scope: BlockScope,
        confirmation: ConfirmationToken,
    },
    ShareContact {
        person_id: PersonId,
        scope: ShareScope,
    },
    SuspendDevice {
        device_id: DeviceId,
        confirmation: ConfirmationToken,
    },
    ChangePrivacyPolicy {
        policy: PrivacyPolicyLevel,
        confirmation: ConfirmationToken,
    },
    ClearAdaptiveNavigation,
    OpenOmnibarResult {
        result_id: String,
    },
    ShareStaticLocation {
        conversation_id: ConversationId,
        location_card_id: String,
    },
    RevokeInvite {
        invite_id: String,
    },
    StartCall {
        target: PersonId,
        media: CallMedia,
        /// The driver's complete offer, after admission and candidate gathering.
        #[serde(default)]
        sdp: Option<Vec<u8>>,
    },
    AcceptCall {
        call_session_id: String,
        sdp: Vec<u8>,
        media_flags: u8,
    },
    DeclineCall {
        call_session_id: String,
    },
    EndCall {
        call_session_id: String,
    },
    UpdateCallIce {
        call_session_id: String,
        sdp: Vec<u8>,
    },
    SetCallMedia {
        call_session_id: String,
        media_flags: u8,
    },
    SetContactAllowCalls {
        conversation_id: ConversationId,
        allowed: bool,
    },
    PinNavigationTarget {
        target_id: String,
    },
    RestoreHistory {
        source: HistorySource,
    },
    CreateIdentityProfile {
        label: String,
    },
    ConfirmRecoveryKeyStored {
        key_operation_id: OperationId,
    },
    CreateInvite,
    ReconnectContact {
        account_id: String,
    },
    AcceptInvitation {
        link: String,
    },
    OfferDeviceLink,
    RequestDeviceLink {
        link: String,
    },
    /// On a profile with no device and no account: the person who lost every device enters the recovery key (as shown
    /// at creation, groups of four, any case, spaces or dashes) and the Home address. Core checks the key's checksum
    /// and keeps it, with the address, in the restricted recovery state (RECOVERY_AND_BACKUP_POLICY.md 10.3). It
    /// grants no authority and sends nothing; fetching the RecoveryRecord and requesting recovery need Home
    /// operations the Home API does not provide yet.
    BeginRecovery {
        /// The key sheet (ADR-0200: any text form or the QR payload) or the older key-only text.
        #[serde(alias = "sheet")]
        recovery_key: String,
        /// The Home address of the older entry; unused when the sheet names the place.
        #[serde(default)]
        home: String,
        /// With a key-only sheet: the place-only string (ADR-0200) that says where the account is saved.
        #[serde(default)]
        place: Option<String>,
    },
    /// On a recovering device whose record was found: ask for the account back (ADR-0202). Explicit user action only. It
    /// stages a tentative device, signs the request and the successor, delivers them to the Homes and the contacts the
    /// record can reach, and starts this device's own interval. It grants nothing until the interval, the checks and the
    /// owner's own pin all hold.
    RequestRecovery,
    /// A surviving controller opens a short, single-use recovery approval ceremony (ADR-0203). It shows a private link
    /// or QR for the restricted device; the person confirms the exact request with `ConfirmRecovery`.
    OfferRecoveryApproval,
    /// The restricted device, with its request waiting, sends that exact request to the controller whose
    /// `om:recover-approve/` link it was given. An enrollment link is refused.
    JoinRecoveryApproval {
        link: String,
    },
    /// Make a new recovery key for this account (ADR-0205): a controller device stages a new root and its policy and shows the new
    /// key once (`GetRecoveryKey` under this operation id). Nothing changes until `ConfirmRecoveryKeyStored` names this operation.
    ReplaceRecoveryKey,
    /// Close this device's own recovery attempt.
    CancelRecovery,
    /// Ask again now: re-read the record (before an attempt) or the Homes' boards and record (during one).
    CheckRecoveryAgain,
    /// Try again now to tell a device about the devices added to the account (ADR-0193 section 10).
    RetryDeviceNotice {
        device_id: DeviceId,
    },
    /// "Only add a device when all my other devices can be told right away" (ADR-0193 section 8). Device-local, off by
    /// default; it travels in no account record.
    SetStrictDeviceNotice {
        enabled: bool,
    },
    /// A name for a contact that only this device's person sees (ADR-0196): never sent to the contact, a group or a relay.
    /// Absent, empty or whitespace-only clears it. Not destructive.
    SetContactNickname {
        conversation_id: ConversationId,
        #[serde(default)]
        nickname: Option<String>,
    },
    OfferControllerChange {
        device_id: DeviceId,
        remove: bool,
    },
    JoinControllerChange {
        link: String,
    },
    ConfirmControllerChange {
        request_id: String,
        confirmation: ConfirmationToken,
    },
    CancelControllerChange {
        request_id: String,
    },
    ApproveDeviceLink {
        request_id: String,
        confirmation: ConfirmationToken,
    },
    RejectDeviceLink {
        request_id: String,
    },
    RevokeDevice {
        device_id: DeviceId,
        confirmation: ConfirmationToken,
    },
    StartRecovery {
        scope: RecoveryScope,
        custodian: RecoveryCustodianCode,
        confirmation: ConfirmationToken,
    },
    VetoRecovery {
        request_id: String,
        confirmation: ConfirmationToken,
    },
    /// "That was me" on an incoming recovery request: this controller co-signs the recovering device's successor, which
    /// completes the recovery at once (RECOVERY_AND_BACKUP_POLICY.md 10.3). `request_id` names an observed request.
    ConfirmRecovery {
        request_id: String,
        confirmation: ConfirmationToken,
    },
    /// "This isn't them" on a contact's recovery of their account (ADR-0190 section 4): ends that request for this
    /// contact only, keeps the pinned head, tells no one. `request_id` is the request digest.
    DeclinePeerRecovery {
        conversation_id: ConversationId,
        request_id: String,
        confirmation: ConfirmationToken,
    },
    /// "Yes, it's them" once the wait is over (Hardened and Maximum Privacy need it before the contact accepts).
    ConfirmPeerRecovery {
        conversation_id: ConversationId,
        request_id: String,
        confirmation: ConfirmationToken,
    },
    /// Ask Core for a single-use confirmation nonce bound to a destructive intent kind and its target (M2,
    /// ADR-0181 section 8). The nonce is read back with `Query::GetConfirmation`.
    IssueConfirmation {
        intent_kind: String,
        target: String,
    },
}

/// The destructive intent kinds that need a Core-issued confirmation (mirrors `DESTRUCTIVE_INTENT_KINDS`).
pub const DESTRUCTIVE_KINDS: [&str; 11] = [
    "BlockPerson",
    "SuspendDevice",
    "ChangePrivacyPolicy",
    "ApproveDeviceLink",
    "RevokeDevice",
    "StartRecovery",
    "VetoRecovery",
    "ConfirmRecovery",
    "DeclinePeerRecovery",
    "ConfirmPeerRecovery",
    "ConfirmControllerChange",
];

impl IntentBody {
    pub fn kind(&self) -> &'static str {
        match self {
            IntentBody::SendMessage { .. } => "SendMessage",
            IntentBody::SendMessageMentioning { .. } => "SendMessage",
            IntentBody::RetryDelivery { .. } => "RetryDelivery",
            IntentBody::SetReaction { .. } => "SetReaction",
            IntentBody::EditMessage { .. } => "EditMessage",
            IntentBody::DeleteMessage { .. } => "DeleteMessage",
            IntentBody::SendAttachments { .. } => "SendAttachments",
            IntentBody::SendAttachmentsDirect { .. } => "SendAttachmentsDirect",
            IntentBody::WithdrawDirectOffer { .. } => "WithdrawDirectOffer",
            IntentBody::SendLocation { .. } => "SendLocation",
            IntentBody::SwitchIdentityProfile { .. } => "SwitchIdentityProfile",
            IntentBody::AcceptIntroduction { .. } => "AcceptIntroduction",
            IntentBody::BlockPerson { .. } => "BlockPerson",
            IntentBody::ShareContact { .. } => "ShareContact",
            IntentBody::SuspendDevice { .. } => "SuspendDevice",
            IntentBody::ChangePrivacyPolicy { .. } => "ChangePrivacyPolicy",
            IntentBody::ClearAdaptiveNavigation => "ClearAdaptiveNavigation",
            IntentBody::OpenOmnibarResult { .. } => "OpenOmnibarResult",
            IntentBody::ShareStaticLocation { .. } => "ShareStaticLocation",
            IntentBody::RevokeInvite { .. } => "RevokeInvite",
            IntentBody::StartCall { .. } => "StartCall",
            IntentBody::AcceptCall { .. } => "AcceptCall",
            IntentBody::DeclineCall { .. } => "DeclineCall",
            IntentBody::EndCall { .. } => "EndCall",
            IntentBody::UpdateCallIce { .. } => "UpdateCallIce",
            IntentBody::SetCallMedia { .. } => "SetCallMedia",
            IntentBody::SetContactAllowCalls { .. } => "SetContactAllowCalls",
            IntentBody::PinNavigationTarget { .. } => "PinNavigationTarget",
            IntentBody::RestoreHistory { .. } => "RestoreHistory",
            IntentBody::CreateIdentityProfile { .. } => "CreateIdentityProfile",
            IntentBody::ConfirmRecoveryKeyStored { .. } => "ConfirmRecoveryKeyStored",
            IntentBody::CreateInvite => "CreateInvite",
            IntentBody::ReconnectContact { .. } => "ReconnectContact",
            IntentBody::AcceptInvitation { .. } => "AcceptInvitation",
            IntentBody::OfferDeviceLink => "OfferDeviceLink",
            IntentBody::RequestDeviceLink { .. } => "RequestDeviceLink",
            IntentBody::BeginRecovery { .. } => "BeginRecovery",
            IntentBody::RequestRecovery => "RequestRecovery",
            IntentBody::OfferRecoveryApproval => "OfferRecoveryApproval",
            IntentBody::JoinRecoveryApproval { .. } => "JoinRecoveryApproval",
            IntentBody::ReplaceRecoveryKey => "ReplaceRecoveryKey",
            IntentBody::CancelRecovery => "CancelRecovery",
            IntentBody::CheckRecoveryAgain => "CheckRecoveryAgain",
            IntentBody::OfferControllerChange { .. } => "OfferControllerChange",
            IntentBody::JoinControllerChange { .. } => "JoinControllerChange",
            IntentBody::ConfirmControllerChange { .. } => "ConfirmControllerChange",
            IntentBody::CancelControllerChange { .. } => "CancelControllerChange",
            IntentBody::ApproveDeviceLink { .. } => "ApproveDeviceLink",
            IntentBody::RejectDeviceLink { .. } => "RejectDeviceLink",
            IntentBody::RetryDeviceNotice { .. } => "RetryDeviceNotice",
            IntentBody::SetStrictDeviceNotice { .. } => "SetStrictDeviceNotice",
            IntentBody::SetContactNickname { .. } => "SetContactNickname",
            IntentBody::RevokeDevice { .. } => "RevokeDevice",
            IntentBody::StartRecovery { .. } => "StartRecovery",
            IntentBody::VetoRecovery { .. } => "VetoRecovery",
            IntentBody::ConfirmRecovery { .. } => "ConfirmRecovery",
            IntentBody::DeclinePeerRecovery { .. } => "DeclinePeerRecovery",
            IntentBody::ConfirmPeerRecovery { .. } => "ConfirmPeerRecovery",
            IntentBody::IssueConfirmation { .. } => "IssueConfirmation",
        }
    }

    /// The target a confirmation nonce is bound to (see `DESTRUCTIVE_KINDS`).
    pub fn confirmation_target(&self) -> Option<String> {
        let code = |v: &dyn erased_ser::Code| v.code();
        match self {
            IntentBody::BlockPerson { person_id, .. } => Some(person_id.clone()),
            IntentBody::SuspendDevice { device_id, .. }
            | IntentBody::RevokeDevice { device_id, .. } => Some(device_id.clone()),
            IntentBody::ConfirmControllerChange { request_id, .. }
            | IntentBody::ApproveDeviceLink { request_id, .. }
            | IntentBody::VetoRecovery { request_id, .. }
            | IntentBody::ConfirmRecovery { request_id, .. }
            | IntentBody::DeclinePeerRecovery { request_id, .. }
            | IntentBody::ConfirmPeerRecovery { request_id, .. } => Some(request_id.clone()),
            IntentBody::ChangePrivacyPolicy { policy, .. } => Some(code(policy)),
            IntentBody::StartRecovery {
                scope, custodian, ..
            } => Some(format!("{}:{}", code(scope), code(custodian))),
            _ => None,
        }
    }

    /// Intents that need an explicit confirmation token (DESTRUCTIVE_INTENT_KINDS in the platform API).
    pub fn confirmation(&self) -> Option<&ConfirmationToken> {
        match self {
            IntentBody::BlockPerson { confirmation, .. }
            | IntentBody::SuspendDevice { confirmation, .. }
            | IntentBody::ChangePrivacyPolicy { confirmation, .. }
            | IntentBody::ConfirmControllerChange { confirmation, .. }
            | IntentBody::ApproveDeviceLink { confirmation, .. }
            | IntentBody::RevokeDevice { confirmation, .. }
            | IntentBody::StartRecovery { confirmation, .. }
            | IntentBody::VetoRecovery { confirmation, .. }
            | IntentBody::ConfirmRecovery { confirmation, .. }
            | IntentBody::DeclinePeerRecovery { confirmation, .. }
            | IntentBody::ConfirmPeerRecovery { confirmation, .. } => Some(confirmation),
            _ => None,
        }
    }
}

mod erased_ser {
    /// The snake_case wire code of a serde enum value.
    pub trait Code {
        fn code(&self) -> String;
    }
    impl<T: serde::Serialize> Code for T {
        fn code(&self) -> String {
            serde_json::to_value(self)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub ctx: ProfileExecutionContext,
    /// Idempotency key: re-dispatching the same id never creates a second semantic operation.
    pub operation_id: OperationId,
    #[serde(flatten)]
    pub body: IntentBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationHandle {
    pub operation_id: OperationId,
    pub profile_id: ProfileId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOperationView {
    pub operation_id: OperationId,
    pub intent_kind: String,
    pub outcome: OutcomeClass,
}

// ---------------------------------------------------------------------------------------------- queries

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum Query {
    GetConversationView {
        ctx: ProfileExecutionContext,
        conversation_id: ConversationId,
    },
    GetConversationList {
        ctx: ProfileExecutionContext,
    },
    GetProfileList,
    GetIntroductions {
        ctx: ProfileExecutionContext,
    },
    GetRecoveryKey {
        ctx: ProfileExecutionContext,
        key_operation_id: OperationId,
    },
    GetInvite {
        ctx: ProfileExecutionContext,
        operation_id: OperationId,
    },
    GetHomeSummary {
        ctx: ProfileExecutionContext,
    },
    GetDevices {
        ctx: ProfileExecutionContext,
    },
    GetLinkRequest {
        ctx: ProfileExecutionContext,
        operation_id: OperationId,
    },
    GetLinkOffer {
        ctx: ProfileExecutionContext,
        operation_id: OperationId,
    },
    GetRelationshipRestore {
        ctx: ProfileExecutionContext,
    },
    GetRecoveryStatus {
        ctx: ProfileExecutionContext,
    },
    /// The controller's recovery approval offer: its link, state and the six digits to compare (`null` when unknown).
    GetRecoveryApprovalOffer {
        ctx: ProfileExecutionContext,
        operation_id: OperationId,
    },
    /// Progress of the same-owner history transfer of the latest device link (`null` when none was announced).
    GetHistoryTransfer {
        ctx: ProfileExecutionContext,
    },
    /// The nonce an `IssueConfirmation` operation produced (`null` once spent, expired or unknown).
    GetConfirmation {
        ctx: ProfileExecutionContext,
        operation_id: OperationId,
    },
    /// Whether first-contact requests from strangers are currently being dropped (ADR-0181 section 5).
    GetIntroductionLoad {
        ctx: ProfileExecutionContext,
    },
    /// How a file of `size` plaintext bytes may travel, before anything is sealed or sent (`AttachmentPlanView`).
    PlanAttachment {
        ctx: ProfileExecutionContext,
        size: u64,
    },
    /// The direct offers of this profile, newest last (`DirectOfferView[]`).
    GetDirectOffers {
        ctx: ProfileExecutionContext,
    },
    GetCallView {
        ctx: ProfileExecutionContext,
    },
    GetCallNotices {
        ctx: ProfileExecutionContext,
    },
    /// This person's own privacy level on this device (`PrivacyPolicyLevel`; Recommended when none was set). Local, never sent.
    GetPrivacyLevel {
        ctx: ProfileExecutionContext,
    },
}

impl Query {
    pub fn ctx(&self) -> Option<&ProfileExecutionContext> {
        match self {
            Query::GetProfileList => None,
            Query::GetConversationView { ctx, .. }
            | Query::GetConversationList { ctx }
            | Query::GetIntroductions { ctx }
            | Query::GetRecoveryKey { ctx, .. }
            | Query::GetInvite { ctx, .. }
            | Query::GetHomeSummary { ctx }
            | Query::GetDevices { ctx }
            | Query::GetLinkRequest { ctx, .. }
            | Query::GetLinkOffer { ctx, .. }
            | Query::GetRelationshipRestore { ctx }
            | Query::GetRecoveryStatus { ctx }
            | Query::GetRecoveryApprovalOffer { ctx, .. }
            | Query::GetHistoryTransfer { ctx }
            | Query::GetConfirmation { ctx, .. }
            | Query::GetIntroductionLoad { ctx }
            | Query::PlanAttachment { ctx, .. }
            | Query::GetDirectOffers { ctx }
            | Query::GetCallView { ctx }
            | Query::GetCallNotices { ctx }
            | Query::GetPrivacyLevel { ctx } => Some(ctx),
        }
    }
}

// ---------------------------------------------------------------------------------------------- views

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectContactView {
    pub account_id: String,
    pub label: String,
    pub invite_operation_id: Option<OperationId>,
    pub invite: Option<InviteView>,
    pub conversation_id: Option<ConversationId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeliveryState {
    PendingNetwork,
    PendingReconciliation,
    /// Written while the contact recovers their account: kept on this device, never encrypted to a device the pinned head does
    /// not list, and sent once their new device is confirmed (ADR-0190 section 5, D131).
    WaitingForPeerRecovery,
    Sent,
    Delivered,
    Rejected {
        reason: RejectReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityNoticeCode {
    SafetyNumberChanged,
    DeviceAdded,
    UnverifiedContact,
    SessionReset,
    IntegrityFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityNotice {
    pub code: SecurityNoticeCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<PersonId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub person_id: PersonId,
    pub display_name: String,
    /// The person's own name for this contact (ADR-0196); omitted when none. Shown instead of `display_name`, which stays the original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_calls: Option<bool>,
    pub last_message_at: Option<u64>,
    pub notices: Vec<SecurityNotice>,
    pub relationship: RelationshipStateCode,
    /// Hex of the contact's account once it is bound; omitted before that (and for restored history). Lets a screen tell
    /// that two entries are the same account, which a name cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_account: Option<String>,
    /// Local route to the temporary conversation replaced by this thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relinked_from: Option<ConversationId>,
    /// The recovery of this person's account this contact is tracking, if any (ADR-0190 section 4). Omitted when
    /// there is none, so existing readers are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_recovery: Option<PeerRecoveryView>,
}

/// What the conversation header needs to show one tracked recovery of the contact's account. No digest, epoch or
/// policy text, and no time other than whole days left on THIS device's own interval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRecoveryView {
    /// Hex of the request digest: what `DeclinePeerRecovery` / `ConfirmPeerRecovery` name (Details only).
    pub request_id: String,
    pub state: crate::state::PeerRecoveryState,
    /// Whole days left of this device's own interval (rounded up; 0 once it ran).
    pub days_left: u64,
    /// "This isn't them" is offered.
    pub can_decline: bool,
    /// The contact must confirm ("Yes, it's them") before this can finish: Ready on Hardened or Maximum.
    pub needs_confirm: bool,
    /// The account's policy asks that contacts see a completion notice (`notify_contacts_on_completion`).
    pub notify_on_completion: bool,
}

/// Where a relationship is in first contact (`ACCEPTED_UNBOUND` until both bindings verify; ADR-0181).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipStateCode {
    AwaitingAcceptance,
    AcceptedUnbound,
    Establishing,
    Active,
    /// History brought back from a backup file: read-only, no way to send (ADR-0184).
    Restored,
}

/// `unverified`: the requester's intro key is not bound to any account yet. `bound`: it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationCode {
    Unverified,
    Bound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "direction",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum TimelineItem {
    Outgoing {
        message_id: MessageId,
        operation_id: OperationId,
        text: String,
        sent_at: u64,
        delivery: DeliveryState,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentView>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<PresentationView>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<ReplyView>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        reactions: Vec<ReactionView>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        edited: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mentions: Vec<MentionView>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        read_only: bool,
    },
    Incoming {
        message_id: MessageId,
        text: String,
        sent_at: u64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentView>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<PresentationView>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<ReplyView>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        reactions: Vec<ReactionView>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        edited: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mentions: Vec<MentionView>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        read_only: bool,
    },
}

/// The message a reply answers, as this device can show it. The quoted text is read from the local copy of that message
/// when the timeline is built; it is never carried in the reply event (only the EventID is). When the original is not
/// here (never arrived, deleted, expired) `available` is false and nothing else is filled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyView {
    pub message_id: MessageId,
    pub available: bool,
    /// Whether the original is our own message (for "You" versus the person's name).
    #[serde(default)]
    pub outgoing: bool,
    /// The start of the original's text, bounded (`REPLY_PREVIEW_CHARS`). Empty for an attachment-only original.
    #[serde(default)]
    pub preview: String,
    /// The original carries at least one attachment.
    #[serde(default)]
    pub has_attachment: bool,
}

/// One reaction on a message: how many people have it set (at most two in a one-to-one chat) and whether we are one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionView {
    pub key: String,
    pub count: u32,
    pub mine: bool,
}

/// Characters of the original shown in a quoted block.
pub const REPLY_PREVIEW_CHARS: usize = 140;

/// Where an attachment's bytes are. "Stored for later" is a hand-over to a store with a lifetime, not delivery
/// (ADR-0150): a message can be delivered while its attachment is no longer there, and the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentState {
    /// Ours, not yet stored by the BlobStore.
    Uploading,
    /// Ours, stored by the BlobStore until `stored_until`; the other person can fetch it until then.
    StoredForLater,
    /// Theirs (or our other device's): fetched only when the host asks for it.
    Remote,
    /// Theirs, and offered directly: it can be fetched only while the sender is online (nothing is stored for later).
    RemoteDirect,
    /// Ours, too big for the delivery service: waiting to be sent while the other person is online.
    WaitingForRecipient,
    /// Ours: being sent to the other person right now.
    Transferring,
    /// Ours: their device says it has it, and nothing has checked that yet (ADR-0194).
    PeerSaysReceived,
    /// Ours: their device checked the file and has it (an authenticated receipt). That device only, not a copy kept
    /// anywhere else.
    DeliveredLocally,
    /// Ours: they said they had it, the offer lapsed, and it was never confirmed.
    NotConfirmed,
    /// Ours: no longer offered (taken back, or not fetched in time).
    NotOffered,
}

/// How a file may travel, decided before anything is sealed or sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AttachmentPlanView {
    /// Through the delivery service, as usual.
    Spool,
    /// Too big for the delivery service: not sent that way. It can be offered to go straight to the other person while
    /// they are online. `limit` is the largest file the service takes (0: none is set up).
    OfferDirect {
        limit: u64,
    },
    /// Cannot be sent at all; `limit` is the largest file OM can send (0 for an empty file).
    TooLarge {
        limit: u64,
    },
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectOfferView {
    pub conversation_id: String,
    pub message_id: String,
    pub blob_id: String,
    pub size: u64,
    pub state: AttachmentState,
    /// Wall ms after which the offer lapses.
    pub expires_at: u64,
    /// True after a restart until the host hands the file back (`Core::arm_direct_offer`): nothing is served meanwhile.
    pub needs_file: bool,
}

/// Everything here is sender-authored display data except `size` (which the fetched object is checked against).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentView {
    pub index: u32,
    pub blob_id: String,
    pub media_type: String,
    /// Plaintext bytes. The store sees `size` plus a fixed 16 bytes, so size is NOT hidden from it.
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub state: AttachmentState,
    /// Wall ms the BlobStore said it keeps our copy until (outgoing only; a display hint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored_until: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceNoteView {
    pub attachment_index: u32,
    /// Untrusted, bounded hint (16..256 samples); the client may regenerate it from the audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waveform: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationCardView {
    pub lat_e7: i32,
    pub lon_e7: i32,
    /// The canonical `geo:` token, the same text the message carries.
    pub geo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy_m: Option<u32>,
    /// Sender-authored annotation; it never replaces showing the point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// V1 presentation of a message, present only while its extensions are valid for the message as it currently reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_note: Option<VoiceNoteView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_card: Option<LocationCardView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationView {
    pub summary: ConversationSummary,
    pub timeline: Vec<TimelineItem>,
    /// This profile's AccountID for private, in-person comparison in an active chat.
    #[serde(rename = "ownAccount", skip_serializing_if = "Option::is_none")]
    pub own_account: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroductionRequest {
    pub id: String,
    pub from: PersonId,
    /// A label the requester chose for itself. Never verified.
    pub display_name: String,
    pub verification: VerificationCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationView {
    pub nonce: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroductionLoadView {
    pub throttled: bool,
    pub dropped: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub id: ProfileId,
    pub label: String,
}

/// Display-once secret; returned only until ConfirmRecoveryKeyStored.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyView {
    pub key_operation_id: OperationId,
    pub groups: Vec<String>,
    /// The key sheet text (ADR-0200) when this account has a recovery place; absent when the key is shown alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
}

impl core::fmt::Debug for RecoveryKeyView {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RecoveryKeyView")
            .field("key_operation_id", &self.key_operation_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub operation_id: OperationId,
    pub link: String,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeObserverCode {
    SponsorSeesMetadata,
    HostSeesMetadata,
    HostSeesCiphertext,
    NoThirdParty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientOriginChoiceCode {
    DefaultOrigin,
    IndependentOriginAvailable,
    IndependentOriginActive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSummary {
    pub tier: String,
    pub observers: Vec<HomeObserverCode>,
    pub migration_available: bool,
    pub client_origin_choice: Vec<ClientOriginChoiceCode>,
}

// Devices and recovery (ACCOUNT_CONTINUITY_AND_CONTROL.md, RECOVERY_AND_BACKUP_POLICY.md).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    Control,
    Messaging,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStateCode {
    Active,
    Quarantined,
    RevocationPending,
    Revoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastSeenClass {
    Now,
    Recent,
    ThisWeek,
    ThisMonth,
    LongAgo,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceActionCode {
    Promote,
    RevokeWithApproval,
    Suspend,
    Revoke,
    /// Try again now to tell this device about a newly linked device (ADR-0193).
    RetryNotice,
}

/// Whether a device has been told about the devices added to the account (ADR-0193 section 10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceNoticeState {
    /// Nothing is owed to it.
    #[default]
    Told,
    /// A notice is owed and not yet acknowledged ("Not told yet").
    Owed,
    /// It is paused (quarantined); it will be told when it is active again.
    Held,
    /// The approval that added it never reached it ("Not set up yet"): it holds no keys and cannot take part. Removing
    /// it is the way out; a new link starts again.
    NotSetUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCautionCode {
    SuspendWouldFreeze,
    LastVotingController,
    HistorySyncUnavailable,
    LongIdle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: DeviceId,
    pub label: String,
    pub role: DeviceRole,
    pub state: DeviceStateCode,
    pub is_current: bool,
    pub enrolled_at: u64,
    pub last_seen: LastSeenClass,
    pub actions: Vec<DeviceActionCode>,
    pub cautions: Vec<DeviceCautionCode>,
    #[serde(default)]
    pub notice_state: DeviceNoticeState,
    /// Wall ms of the last try to tell this device, when one was owed and tried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountControlStateCode {
    Normal,
    ControlConflict,
    SafetyFreeze,
    ForkDetected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkRequestState {
    AwaitingApproval,
    PendingAuthorization,
    Approved,
    Rejected,
    Expired,
}

/// What a device sees about a device-link request: the approving device (it holds the request) and the asking device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRequestView {
    pub request_id: String,
    pub operation_id: OperationId,
    pub device_label: String,
    /// Derived by Core from the ceremony transcript; both devices show the same code.
    pub verification_code: String,
    pub expires_at: u64,
    pub state: LinkRequestState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkOfferState {
    WaitingForScan,
    AwaitingApproval,
    Linked,
    Rejected,
    Expired,
}

/// What the device that offered a link shows while it waits for the other device (D24: the existing device offers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOfferView {
    pub operation_id: OperationId,
    pub link: String,
    pub expires_at: u64,
    pub verification_code: Option<String>,
    pub state: LinkOfferState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRole {
    /// This device produced the snapshot for a newly linked device.
    Sender,
    /// This device is the newly linked one and receives it.
    Receiver,
}

/// Progress of a `DeviceHistorySnapshot` transfer (MULTI_DEVICE_SYNC "History transfer"). Counts only: no content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTransferView {
    pub role: HistoryRole,
    pub chunks_total: u32,
    /// Receiver: chunks processed (including ignored ones). Sender: equal to `chunks_total` once deposited.
    pub chunks_done: u32,
    /// Receiver: chunks ignored whole (a codec this implementation does not know, or malformed).
    pub chunks_ignored: u32,
    pub messages_added: u32,
    /// Receiver: chunk indices not yet counted (`chunks_total` minus those counted).
    pub chunks_missing: u32,
    pub state: HistoryTransferState,
}

/// Where a history transfer stands. `Degraded` is the explicit end state of a transfer whose time ran out with chunk
/// indices missing: the device keeps what arrived, nothing more will come, and the UI says the history is partial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryTransferState {
    InProgress,
    Complete,
    Degraded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceNoticeCode {
    DeviceLinked,
}

/// A non-suppressible notice that a device joined this account (DeviceEnrollmentNotice, ceremony `link_ceremony`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceNoticeView {
    pub code: DeviceNoticeCode,
    pub device_id: String,
    pub label: String,
    pub at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesView {
    pub control_state: AccountControlStateCode,
    pub devices: Vec<DeviceSummary>,
    pub link_requests: Vec<LinkRequestView>,
    pub notices: Vec<DeviceNoticeView>,
    /// The strict reading of ADR-0193 section 8 is on for this device.
    #[serde(default)]
    pub strict_notice: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_change: Option<ControllerChangeView>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_join_controller_change: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovery_key_replacement_needed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerChangeView {
    pub request_id: String,
    pub device_id: DeviceId,
    pub remove: bool,
    pub host: bool,
    pub waiting: bool,
    pub verification_code: Option<String>,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPolicyState {
    On,
    Pending,
    OffExplicit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCustodianView {
    pub code: RecoveryCustodianCode,
    pub distribution_bound: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatusView {
    pub policy: RecoveryPolicyState,
    pub custodians: Vec<RecoveryCustodianView>,
    pub scopes: Vec<RecoveryScope>,
    pub activation_delay_ms: u64,
    pub recovery_key_saved: bool,
    pub requests: Vec<RecoveryRequestView>,
    /// Set only on a recovering device that has entered its key (`BeginRecovery`): the restricted recovery state.
    /// Absent otherwise, so existing readers are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovering: Option<RecoveringView>,
    /// How far this account's recovery record is: present only when the account keeps one (it has a recovery place).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<RecoveryRecordCode>,
}

/// The state of the recovery record kept at the Home (ADR-0199 section 6), in the words Settings uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryRecordCode {
    FinishingSetup,
    Ready,
    NeedsAttention,
    NeedsNewKey,
}

/// The restricted recovery state of a device that has entered a recovery key but has no account yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveringView {
    /// The Home address the person entered, or the one the sheet names.
    pub home: String,
    /// How the lookup of the account's record stands.
    pub stage: crate::state::JoinStage,
    /// The device cannot send as the identity (always true here); the record and the request are still to come.
    pub restricted: bool,
    /// What the record's evidence amounts to against the owner's own checkpoint (ADR-0202): only `pinned` can finish.
    #[serde(default)]
    pub evidence: crate::state::EvidenceStage,
    /// Where this device's own attempt stands, once one was asked for.
    #[serde(default)]
    pub attempt: Option<crate::state::AttemptState>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retry_from_verified_head: bool,
    /// Milliseconds of this device's own interval still to run (monotonic credit only), once an attempt is waiting.
    #[serde(default)]
    pub remaining_ms: Option<u64>,
    /// The approval exchange with a surviving controller, once this device joined one (ADR-0203).
    #[serde(default)]
    pub approval: Option<RecoveryApprovalView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalCode {
    WaitingForController,
    Receiving,
    Done,
    Failed,
    Expired,
}

/// The restricted device's side of the approval exchange: where it stands and the six digits to compare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryApprovalView {
    pub state: ApprovalCode,
    pub verification_code: Option<String>,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOfferCode {
    WaitingForDevice,
    AwaitingConfirmation,
    Approved,
    Declined,
    Failed,
    Expired,
}

/// What the surviving controller shows: the link while it waits, then the code and the request to confirm.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryApprovalOfferView {
    pub operation_id: OperationId,
    pub link: Option<String>,
    pub expires_at: u64,
    pub verification_code: Option<String>,
    /// The request digest `ConfirmRecovery` names, once a proposal arrived and verified.
    pub request_id: Option<String>,
    pub state: ApprovalOfferCode,
}

impl core::fmt::Debug for RecoveryApprovalOfferView {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RecoveryApprovalOfferView")
            .field("operation_id", &self.operation_id)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryRequestDirection {
    Incoming,
    Outgoing,
}

/// `stale` is an addition to the platform's `waiting | completed | vetoed | cancelled`: the account moved on and the
/// request was dropped (a device was enrolled or revoked meanwhile, or the parent head moved).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryRequestStateCode {
    Waiting,
    Completed,
    Vetoed,
    Cancelled,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRequestView {
    /// Hex of the request digest (`OM-RECOVERY-REQUEST-HASH-v1`); what `VetoRecovery` / `ConfirmRecovery` name.
    pub request_id: String,
    pub operation_id: Option<String>,
    pub direction: RecoveryRequestDirection,
    pub scope: RecoveryScope,
    pub custodian: RecoveryCustodianCode,
    /// Wall-clock display hint of this verifier's first observation.
    pub started_at: u64,
    /// This verifier's own earliest completion: now + (delay - credited monotonic time). Never a Home or request time.
    pub activates_at: Option<u64>,
    pub state: RecoveryRequestStateCode,
    pub can_stop: bool,
    /// The delay has run with no veto, and no recovery-signed successor is held yet to apply.
    pub ready: bool,
}
