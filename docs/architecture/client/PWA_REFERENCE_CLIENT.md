# OM PWA Reference Client — First Execution Plan

- **Status:** Selected execution direction
- **Timestamp:** 2026-08-18 14:03 PHT
- **Principle:** Web-first for protocol velocity; never web-only by architectural assumption.

## What the PWA is for

The first official implementation is a **reference client and interoperability laboratory**.

Its job is to prove:

- identity lifecycle;
- message requests;
- 1:1 OM-native messaging;
- canonical event convergence;
- groups;
- relay swapping;
- encrypted attachments;
- basic StoragePool behavior;
- device linking/revocation;
- backup/restore experiments;
- conformance vectors.

It is not meant to prove Android SMS replacement, final native key protection, or all mobile background behavior.

## Reference architecture

```text
Browser UI
  thin TypeScript presentation layer
        │
        ▼
OM Core
  Rust compiled to WASM where practical
        │
        ├── Identity
        ├── Event model
        ├── protocol state
        ├── group state
        ├── provider interfaces
        └── serialization
        │
        ├── CryptoProvider
        ├── LocalStore
        ├── RelayProvider
        ├── BlobStore
        └── Push/Wake adapter
```

The exact UI framework is not a protocol decision. Prefer a small, conventional framework and avoid framework-specific assumptions in core state.

## Why keep the core outside the UI

The browser UI is disposable.

The protocol logic should later run under:

- Android/Kotlin;
- iOS/Swift;
- other web clients;
- desktop;
- third-party applications;
- test harnesses.

A shared Rust core remains the preferred architectural direction because it can target native platforms and WebAssembly, but individual cryptographic library choices must be verified for those targets before final adoption.

## PWA local storage

Use a browser local database abstraction backed initially by IndexedDB.

Store:

- encrypted protocol/account state;
- canonical events;
- materialized views;
- attachment cache metadata;
- provider configuration.

Do not make `localStorage` the message database.

Secret handling remains behind `CryptoProvider`. The browser threat model is documented separately from native hardware-backed key storage.

## Service worker

Use the service worker narrowly:

- offline application shell;
- notification/wake handling where supported;
- safe retry of explicitly designed network work.

Correctness must not depend on the service worker being alive indefinitely.

## Network behavior

While active:

- persistent WebSocket or equivalent efficient relay connection where supported;
- batch acknowledgements;
- no polling loops when a push/wake path exists.

When inactive:

- opaque notification/wake signal where platform allows;
- client fetches encrypted envelopes from its selected relay when active again.

No message plaintext in push payloads.

## First PWA UI

Keep it deliberately boring:

```text
Conversation list
New message
Message requests
Contact/profile
Group chat
Settings
  Devices
  Identity
  Relay
  Storage
  Backup
```

No stories.
No feeds.
No channels.
No marketplace.
No theme ecosystem in milestone 1.
No bot store.

Polish speed, clarity, and conversation flow rather than breadth.

## PWA milestone sequence

### P0 — local protocol harness

- create AccountID;
- persist/reload local encrypted state;
- event encode/decode;
- test vectors;
- simulated two-client harness.

### P1 — two-browser 1:1 messaging

- minimal relay;
- IntroductionEndpoint;
- MessageRequest;
- RelationshipBinding;
- text events;
- dedupe/reorder/retry.

Exit: two browsers on different machines can reliably message after restarts.

### P2 — devices and recovery skeleton

- link second browser as DeviceIdentity;
- revoke it;
- basic account-state propagation;
- prototype encrypted recovery capsule;
- no "forgot password" server authority.

### P3 — attachments + storage abstraction

- encrypted photo/file;
- BlobStore interface;
- at least one simple object-store adapter plus local/dev adapter;
- hot-retention quota;
- recipient re-home.

### P4 — groups

- MLS implementation selected/reviewed;
- create private group;
- add/remove account;
- add/revoke device;
- since-join history;
- HistoryBundle experiment.

### P5 — provider portability

- switch relay without changing AccountID;
- switch blob store;
- mirror storage;
- restore from selected backup;
- export/import provider config safely.

### P6 — abuse and scale lab

- message-request flooding;
- malformed event fuzzing;
- attachment abuse;
- relay quotas;
- group fanout;
- offline queues;
- browser memory/storage pressure.

### P7 — Android starts before web assumptions harden

Build the native Android reference client against the same vectors/core contracts.

Use Android work to validate:

- secure key provider;
- lifecycle;
- notifications;
- contacts/share integration;
- local DB;
- background behavior.

Only after OM-native core stabilizes should Android default-SMS/MMS integration become a product milestone.

### P8 — iOS joins the V1 reference-family cycle

Once Core and push/device-lifecycle contracts are stable enough to exercise them, build the iOS reference shell against the same vectors/Core/UI contracts. Use it to validate Keychain/Secure Enclave integration where applicable, APNs/background limitations, share/media surfaces and system-call integration without creating iOS-only protocol semantics. Android/iOS are V1 reference targets, but their public releases need not be simultaneous with the first PWA vertical slice (ADR-0150).

## Deployment simplicity

Early reference deployment should be boring:

```text
static PWA hosting
one small relay service
one basic object store
one push/wake integration where needed
```

No Kubernetes.
No service mesh.
No Kafka.
No multi-region database before measured need.

A developer should be able to run the reference stack locally with a small number of processes/containers.

## Security rules for the web origin

- strict Content Security Policy;
- minimal third-party scripts;
- no ad/analytics JavaScript in the crypto origin;
- dependency pinning;
- versioned releases;
- aggressive XSS prevention;
- no remote arbitrary-code plugin loading;
- separate untrusted rich-content rendering if later needed.

## Success criterion

The PWA succeeds when it proves that OM's protocol contracts are coherent and replaceable, not when it has every feature of a mature messenger.


## 2026-09-21 scope update

The PWA remains useful as a protocol/conformance reference client, but the first consumer product milestone is OM-native secure messaging rather than proving carrier SMS replacement. Calls may be implemented before SMS/RCS once core messaging/groups/sync are stable.

## 2026-09-22 canonical addendum — operatorless first distribution

OM is now explicitly **PWA-first, not PWA-only**.

The reference PWA is the first operatorless distribution because it can be installed from the web, can use standards-based Web Push, and can operate against the user's own relay/discovery deployment without an app-store account or universal OM push service.

Web Push may be sent by the recipient's own Worker/PersonalRelay to the browser push subscription. This preserves opaque-wakeup rules; message plaintext remains locally decrypted/rendered.

The PWA is not expected to permanently replace native capabilities such as PushKit/CallKit-class incoming-call integration, platform hardware-key APIs, Android default-SMS integration, or all background execution semantics.

The project converges security/state logic into one portable OM Core compiled to WASM for the PWA and native libraries for the V1 Android/iOS/Desktop reference shells. Do not independently reinvent cryptographic/control-state behavior in each UI language.

## 2026-09-22 shared UI direction

The PWA is the first runtime shell for `om-ui`. Build UI primitives as production-quality shared components rather than web-only prototypes.

Prototype order for the visible client:

1. adaptive AppShell/navigation;
2. ConversationList + ConversationView + Composer;
3. People + MessageRequest + Share Contact;
4. Calls and shared in-call UI;
5. Settings/OM Home/Protection/Recovery/Devices;
6. shared design tokens, accessibility primitives and MotionSystem;
7. declarative theme/font import and preview.

A successful PWA phase therefore validates both protocol behavior and the shared product experience that the V1 Android/iOS/Desktop reference shells will host.

## ClientHost/origin engineering boundary — 2026-09-22

The installed PWA origin is a `ClientHost`, not the user's identity. Browser origin-scoped storage, service worker, Web Push and WebAuthn behavior make this an endpoint trust/continuity boundary.

The reference deployment SHOULD separate the application origin from public discovery/IntroGate/relay endpoints. Changing PWA origin/provider is an authenticated new-device migration with fresh local vault/push state; it is never defined as raw IndexedDB copying.

A malicious origin can eventually deliver malicious executable code. OM mitigates this with a user-owned origin where the user owns one (most T0/T1 users instead use a shared ClientHost, [`CLIENTHOST_UPDATE_LIFECYCLE.md`](CLIENTHOST_UPDATE_LIFECYCLE.md) §9), strict CSP/no remote runtime code, verifiable releases and the V1 native reference higher-assurance paths, but must not claim that PWA cryptography defeats a fully compromised execution origin.
