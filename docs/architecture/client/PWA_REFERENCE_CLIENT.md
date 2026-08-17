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
