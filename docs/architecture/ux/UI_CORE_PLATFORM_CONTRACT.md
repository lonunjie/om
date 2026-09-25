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
Clipboard            # write(text, sensitive: bool): sensitive marks the item for exclusion from clipboard history/sync and early expiry where supported
SecureSurface        # screenshot/recording blocking and app-switcher snapshot suppression
BackupExclusion      # exclude a storage path from OS/cloud device backup; file-protection class
InputPrivacy         # disable IME learning, personalized keyboard, spellcheck and autofill on a field
NotificationRemoval  # remove delivered OS notifications (on read, delete, disappearing expiry, quick lock)
LocalDiscovery
CallSystemIntegration
NetworkHints
WindowSystem
Haptics
ExternalTransport     # SMS/RCS later
```

The four leakage families and the sensitive clipboard flag are the controls [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §4 requires the adapter to expose. Each reports `available | partial | unavailable`, as [`PLATFORM_CAPABILITY_MATRIX.md`](../client/PLATFORM_CAPABILITY_MATRIX.md) does, so Core decides policy and the UI states limits instead of each shell adding them ad hoc (ADR-0129).

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

## CONFORMANCE-UI-INTENTS

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| UI-01 | UI requests send | typed intent reaches Core; UI never writes protocol/key stores directly |
| UI-02 | unavailable PlatformAdapter capability | feature degrades explicitly without semantic/security fallback |
| UI-03 | malicious/invalid view input | no authority-changing Core command is synthesized |
| UI-04 | security notice intent | reserved semantic survives platform/theme rendering |
| UI-05 | same intent on two platforms | canonical Core transition is equivalent |
