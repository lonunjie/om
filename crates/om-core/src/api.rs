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
pub struct Draft {
    pub text: String,
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
    RetryDelivery {
        message_id: MessageId,
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
    AcceptInvitation {
        link: String,
    },
    OfferDeviceLink,
    RequestDeviceLink {
        link: String,
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
}

impl IntentBody {
    pub fn kind(&self) -> &'static str {
        match self {
            IntentBody::SendMessage { .. } => "SendMessage",
            IntentBody::RetryDelivery { .. } => "RetryDelivery",
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
            IntentBody::PinNavigationTarget { .. } => "PinNavigationTarget",
            IntentBody::RestoreHistory { .. } => "RestoreHistory",
            IntentBody::CreateIdentityProfile { .. } => "CreateIdentityProfile",
            IntentBody::ConfirmRecoveryKeyStored { .. } => "ConfirmRecoveryKeyStored",
            IntentBody::CreateInvite => "CreateInvite",
            IntentBody::AcceptInvitation { .. } => "AcceptInvitation",
            IntentBody::OfferDeviceLink => "OfferDeviceLink",
            IntentBody::RequestDeviceLink { .. } => "RequestDeviceLink",
            IntentBody::ApproveDeviceLink { .. } => "ApproveDeviceLink",
            IntentBody::RejectDeviceLink { .. } => "RejectDeviceLink",
            IntentBody::RevokeDevice { .. } => "RevokeDevice",
            IntentBody::StartRecovery { .. } => "StartRecovery",
            IntentBody::VetoRecovery { .. } => "VetoRecovery",
        }
    }

    /// Intents that need an explicit confirmation token (DESTRUCTIVE_INTENT_KINDS in the platform API).
    pub fn confirmation(&self) -> Option<&ConfirmationToken> {
        match self {
            IntentBody::BlockPerson { confirmation, .. }
            | IntentBody::SuspendDevice { confirmation, .. }
            | IntentBody::ChangePrivacyPolicy { confirmation, .. }
            | IntentBody::ApproveDeviceLink { confirmation, .. }
            | IntentBody::RevokeDevice { confirmation, .. }
            | IntentBody::StartRecovery { confirmation, .. }
            | IntentBody::VetoRecovery { confirmation, .. } => Some(confirmation),
            _ => None,
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
    GetRecoveryStatus {
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
            | Query::GetRecoveryStatus { ctx } => Some(ctx),
        }
    }
}

// ---------------------------------------------------------------------------------------------- views

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeliveryState {
    PendingNetwork,
    PendingReconciliation,
    Sent,
    Delivered,
    Rejected { reason: RejectReason },
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
    pub last_message_at: Option<u64>,
    pub notices: Vec<SecurityNotice>,
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
    },
    Incoming {
        message_id: MessageId,
        text: String,
        sent_at: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationView {
    pub summary: ConversationSummary,
    pub timeline: Vec<TimelineItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroductionRequest {
    pub id: String,
    pub from: PersonId,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub id: ProfileId,
    pub label: String,
}

/// Display-once secret; returned only until ConfirmRecoveryKeyStored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyView {
    pub key_operation_id: OperationId,
    pub groups: Vec<String>,
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
    Suspend,
    Revoke,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountControlStateCode {
    Normal,
    ControlConflict,
    SafetyFreeze,
    ForkDetected,
}

/// `linkRequests` and `notices` are always empty in pass 1 (device linking is not implemented), so they are untyped here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesView {
    pub control_state: AccountControlStateCode,
    pub devices: Vec<DeviceSummary>,
    pub link_requests: Vec<serde_json::Value>,
    pub notices: Vec<serde_json::Value>,
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
    pub requests: Vec<serde_json::Value>,
}
