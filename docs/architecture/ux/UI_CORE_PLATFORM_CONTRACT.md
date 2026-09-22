# OM UI ↔ Core ↔ Platform Contract

- **Checkpoint:** 2026-09-22 13:42 PHT
- **Status:** implementation direction, interfaces remain to be prototyped

## Rule

`om-ui` never manipulates cryptographic, relay, recovery, account-control or canonical event state directly.

```text
User interaction
    ↓
om-ui intent/command
    ↓
om-core validates + transitions canonical state
    ↓
view state / event stream
    ↓
om-ui renders
```

## Core-facing UI model

Prefer typed intents rather than generic RPC strings.

Conceptual examples:

```text
SendMessage(conversation, draft)
AcceptIntroduction(request_id)
BlockPerson(person_id)
SuspendDevice(device_id)
RevokeInvite(invite_id)
ChangePrivacyPolicy(policy)
StartCall(target, media_kind)
ShareContact(person_id)
RestoreHistory(source)
```

Core returns explicit state such as:

```text
Committed
PendingAuthorization
PendingNetwork
Rejected(reason)
NeedsUserAction(action)
```

The UI must not infer that a security-sensitive operation succeeded because a local button was pressed.

## View models

Core should expose presentation-neutral domain/view state:

- ConversationSummary;
- ConversationTimeline items;
- PersonSummary;
- IntroductionRequest;
- DeviceSummary;
- AccountControlStatus;
- RecoveryStatus;
- OMHomeHealth;
- CallState;
- DeliveryState;
- SecurityNotice.

`om-ui` formats these into localized strings and layouts. Core supplies machine-semantic reason codes, not English warning copy.

## PlatformAdapter

Capability-oriented interface; no business/protocol logic.

Suggested families:

```text
SecureStorage
Biometrics
Push
Notifications
BackgroundExecution
Share
OpenExternal
Files
Camera
Microphone
ContactsLocal
Clipboard
LocalDiscovery
CallSystemIntegration
NetworkHints
WindowSystem
Haptics
ExternalTransport     # SMS/RCS later
```

### Capability discovery

UI asks what is available instead of checking `isIOS()`/`isAndroid()` for product behavior.

Example:

```text
capabilities.callSystemIntegration = true
capabilities.localContacts = read_optional
capabilities.backgroundPush = webpush | apns | fcm | unifiedpush | none
```

## Native shell policy

Native shells are thin and should not reimplement:

- crypto/session semantics;
- conversation event rules;
- discovery/contact semantics;
- recovery/control authorization;
- privacy-profile meaning;
- message lifecycle meaning.

Platform code may maintain transient OS state necessary to integrate with system APIs but canonical state remains in Core.

## Theme boundary

Theme engine belongs in UI. Core may store/sync the user's chosen theme identifier or presentation preferences as local/private settings, but themes never cross into protocol-security logic.

## Error translation

Provider/network failures arrive as stable semantic error categories from Core/adapter boundaries. UI maps them to plain language.

Bad:

> Durable Object 1102 CPU time exceeded.

Normal UI:

> Your OM home is temporarily unavailable. Messages will retry automatically.

Advanced diagnostics may show raw provider detail.

## Testing

Official clients should share:

- Core test vectors;
- UI component tests;
- navigation journey tests;
- PlatformAdapter conformance tests;
- screenshot/visual regression baselines for shared UI;
- accessibility checks;
- end-to-end security-sensitive flow tests.
