# OM UI Action → Core Intent Map

- **Status:** canonical boundary guidance under ADR-0159
- **Purpose:** ensure the UI Action Registry never becomes protocol authority or a generic string-RPC escape hatch

## Rule

```text
input binding
   → semantic UI action
   → typed Core intent/query OR local presentation operation OR PlatformAdapter capability
   → Core/platform validates and returns explicit state
   → UI renders that state
```

The names below are conceptual typed intents. Implementation APIs may use different type names, but they must preserve the same authority boundary and parameter typing.

## Messaging and content

| UI action | Core/platform mapping | Notes |
|---|---|---|
| `open-conversation` | local navigation + `GetConversationView(conversation_id)` | no protocol mutation |
| `send-message` | `SendMessage(conversation_id, compose_intent)` | Core creates canonical event/session material at send time |
| `retry-message` | `RetryDelivery(message_or_event_id)` | retry same logical object; Core decides eligibility |
| `attach-media` | PlatformAdapter picker/camera → local stage → `PrepareAttachment` as needed | file picker/camera is platform-owned |
| `download-attachment` | `FetchAttachment(blob_id, policy)` | BlobID/ciphertext rules remain Core/storage authority |
| `edit-message` | `EditMessage(target_event_id, rich_text)` | authorization/expiry checked by Core |
| `delete-message` | typed local-delete or canonical delete/redaction intent according to requested scope | UI must label scope honestly |
| `react-message` | `SetReaction(target_event_id, reaction)` | Core validates capability/epoch |
| `reply-message` | local compose state references source; `SendMessage` carries reply metadata at send | swiping is only one binding |
| `forward-message` | `CreateForwardComposeIntent(source, destination)` | provenance/policy retained as architecture defines |
| `schedule-message` | `ScheduleComposeIntent(profile, conversation, compose_intent, due_time)` | device-local V1; no pre-encryption/fanout |
| `edit-scheduled-message` | `UpdateScheduledComposeIntent(id, changes)` | before release only |
| `cancel-scheduled-message` | `CancelScheduledComposeIntent(id)` | local/profile-scoped |
| `create-poll` | typed poll create intent | canonical event body per messenger baseline |
| `vote-poll` | typed poll vote intent | Core handles close/order/authorization |
| `close-poll` | typed poll close intent | no UI clock authority |
| `set-disappearing-policy` | `ChangeConversationRetentionPolicy(...)` | bounded by architecture policy |
| `view-once-open` | typed content-lifecycle intent/query | UI cannot promise remote erasure |
| `create-link-preview` | privacy-policy-gated preview request | failure-neutral; Maximum Privacy may disable |

## People, discovery and onboarding

| UI action | Mapping | Notes |
|---|---|---|
| `scan-invite-qr` | PlatformAdapter Camera → `ParseInvite` / intro flow | QR possession is not relationship authority |
| `accept-invite` | typed Invite/IntroGate/MessageRequest flow | Core enforces uses/expiry/revocation |
| `share-contact` | `CreateContactCapsule(person_id, scope)` | provenance is preserved |
| `send-message-request` | typed MessageRequest intent | request constraints stay canonical |
| `accept-message-request` | `AcceptIntroduction(request_id)` | relationship transition is Core-owned |
| `block-person` | `BlockPerson(person_id, scope)` | local/provider/capability effects explicit |
| `report-content` | `BuildEvidencePackage(selection, disclosure_scope)` + chosen external/provider report adapter | no mandatory global OM moderator implied |
| `switch-identity-profile` | `SwitchIdentityProfile(profile_id)` presentation namespace switch | no logout/rekey/cross-profile heuristic |
| `choose-home-tier` | provisioning/Home typed workflow | T0–T4 semantics are provider architecture-owned |
| `choose-client-origin` | distribution/platform choice | sponsor cannot silently force its own client origin |

## Rooms, Spaces, Topics and Publication

| UI action | Mapping | Notes |
|---|---|---|
| `open-space` / `open-room` / `open-topic` | local navigation + Core view query | organization is presentation; authority stays object-local |
| `create-topic` | typed topic create intent | persistent TopicID where supported |
| `move-message-to-topic` | typed `message.topic.set` intent | authorization/convergence checked by Core |
| `invite-to-room/space` | typed invite/capability intent | UI displays real scope |
| `remove-member` | group control intent | authority checked by current Room state |
| `manage-space-role` | Space control/delegated intent | no hidden transitive permission magic |
| `follow-publication` | account-private/local/provider subscription state | follower is not Publication member |
| `publish-post` | typed PublicationEvent create intent | public signed object, not private Room message |
| `retract-publication-post` | typed Publication tombstone/retraction | UI says retracted, never promises Internet erasure |

## Calls

| UI action | Mapping | Notes |
|---|---|---|
| `start-audio-call` | `StartCall(target, audio)` | route chosen automatically |
| `start-video-call` | `StartCall(target, video)` + just-in-time camera capability | no camera activation before intent/permission |
| `answer-call-*` | `AnswerCall(call_session_id, media_kind)` | native call surfaces bind to same intent |
| `decline-call` | `DeclineCall(call_session_id)` | |
| `end-call` | `EndCall(call_session_id)` | |
| `join-group-call` | `JoinCall(call_session_id)` | distinct from ringing every member |
| `set-call-muted` | local media/Core call-state intent | platform mic capability remains adapter-owned |
| `set-camera-enabled` | local media/Core call-state intent + PlatformAdapter permission | |

## Devices, recovery and trust

| UI action | Mapping | Notes |
|---|---|---|
| `view-security-details` | Core query only | no mutation |
| `mark-verified-by-user` | typed local/account-private verification record | does not claim real-world identity beyond user's check |
| `suspend-device` | `SuspendDevice(device_id)` | immediate quarantine semantics distinct from revocation |
| `finalize-device-revocation` | typed account-control transition | may require authorization; UI shows pending/frozen state |
| `add-device` | typed device-link/authorization workflow | security-critical |
| `start-recovery` | `StartRecovery(scope, custodian)` | identity/history/provider scopes remain distinct |
| `cancel/veto-recovery` | typed recovery veto intent | exact timing rules are Core authority |
| `change-protection-profile` | `ChangePrivacyPolicy(profile)` | Recommended/Hardened/Maximum/Custom; never crypto downgrade |
| `change-provider` | typed provider migration/config intent | AccountID continuity preserved |

## Settings and OwnMessenger

Many settings are local/profile-private presentation state and do not require a Core security intent. Examples: theme, density, tab exposure, experience profile, haptic intensity and permitted key bindings. When a setting affects privacy, notification routing, provider policy, retention, protection profile, recovery, device authority or conversation capabilities, it must bind to the corresponding typed Core/domain intent rather than writing UI preferences directly into canonical state.

## Omnibar and voice

Omnibar, command palette and voice produce **candidate semantic actions plus typed targets**. They never directly invoke protocol methods. Normal registry preconditions, confirmation, authentication and Core rejection states still apply.

No learned model or LLM can mark a target confirmed, lower a risk class, bypass required confirmation, or convert a bare entity query into a hidden side-effecting verb.
