# OM Engineering Handoff — INACTIVE UNTIL ARCHITECTURE FREEZE

> **Current checkpoint 2026-09-22 23:02 PHT:** product engineering is not authorized. This document records the intended future handoff only. See `ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`.


## 1. Transition of phase

This document describes the planned engineering transition **after** architecture freeze. It is inactive in the 23:02 checkpoint. Architecture/specification evidence drives current work; implementation work is limited to explicitly bounded feasibility experiments used to answer architecture questions.

When a future ADR authorizes engineering, the objective will be to establish a trustworthy executable core and vertical slice without reopening settled principles.

## 2. Non-negotiable invariants

Engineering MUST preserve:

- no mandatory OM-operated runtime service for established independent communication;
- AccountID is identity, never a routing address;
- accepted relationships use opaque rotatable capabilities;
- one shared `om-core` owns protocol/security/state semantics;
- one shared `om-ui` owns official product flows;
- platform/provider layers expose capabilities, not alternate security logic;
- strongest approved CryptoProfile is automatic and not a user preference;
- privacy/recovery/notification profiles never select weaker message crypto;
- control-state authority never uses chat HLC, last-write-wins or ordinary CRDT merge;
- backups restore content, not stale authority;
- user themes are inert data, not extensions;
- user contact books are not uploaded/hashed for discovery in v1;
- normal operation does not centralize plaintext merely for hypothetical abuse reporting;
- Creator-Disappearance Gate remains satisfiable.

## 3. Engineering work packages

### WP-000 — Repository and trust-root bootstrap

Deliver:

- monorepo structure from `ENGINEERING_REPOSITORY_LAYOUT.md`;
- pinned Rust/JS toolchains and lockfiles;
- formatting/lint/test commands;
- dependency/license policy;
- SBOM generation;
- SLSA/in-toto-capable provenance skeleton;
- signed-tag/release documentation;
- CI with no production release capability yet.

Exit: clean checkout builds/tests without hidden credentials and produces deterministic protocol test artifacts.

### WP-001 — Canonical data model and deterministic encoding

Implement:

- IDs and bounded protocol primitives;
- OM deterministic-CBOR profile;
- parser limits and canonical rejection;
- domain-separated hashing/signing inputs;
- golden vectors;
- version/critical-extension envelope.

Exit: Rust and at least one independent decoder/harness agree on vectors and reject malformed/non-canonical equivalents.

### WP-002 — AccountControlState formal model

Before authoritative control-state code:

- write TLA+ state machine;
- encode controller promotion, quarantine, permanent revocation, recovery, concurrent transitions, stale/forked state;
- check invariants in `FORMAL_VERIFICATION_AND_FUZZING.md`;
- generate implementation transition-table tests from model scenarios where practical.

Exit: no known model trace lets a compromised constitutional minority manufacture permanent ownership.

### WP-003 — ClientHost, DeviceVault and IdentityProfile foundation

Implement:

- `DeviceVault` abstraction;
- per-profile root/vault keys and domain-separated subkeys;
- local encrypted database abstraction;
- `IdentityProfile` namespacing even though v1 UI exposes one profile;
- PWA origin migration as new-device transfer, never raw IndexedDB migration;
- explicit browser-storage-eviction handling.

Exit: two isolated local profiles can coexist in tests without key/database/capability crossover.

### WP-004 — Shared UI shell

Implement shared `om-ui`:

- Chats / Calls / People navigation;
- Compact/Medium/Expanded/Wide shell;
- accessible primitives;
- MotionSystem with reduced-motion path;
- default semantic light/dark themes;
- profile-aware theming architecture;
- no provider/crypto jargon in default UI.

Exit: responsive/accessibility tests and smooth local navigation with synthetic large histories.

### WP-005 — Personal OM Home + discovery vertical slice

Implement development/reference personal Home:

- public ContactDescriptor endpoint;
- revocable Invite;
- IntroGate/ticket boundary;
- bounded offline envelope queue;
- rate limits/cheap rejection order;
- `workers.dev` reference deployment plus local emulator;
- no R2 requirement.

Exit: two independently deployed Homes can discover via QR/link and exchange constrained message requests without any OM-operated server.

### WP-006 — Relationship and event vertical slice

Using dev crypto initially:

- accept/reject/block request;
- create RelationshipBinding;
- private relay capabilities;
- immutable events + materialized view;
- retries/idempotency;
- rich-text v1;
- delivery/read receipt policy;
- local drafts.

Exit: offline/retry/duplicate/reorder tests converge.

### WP-007 — Hostile content, attachments and local lifecycle

Implement:

- BlobStore interface and direct/local transfer path;
- decrypt-then-untrusted validation boundary;
- type/size/decompression limits;
- safe preview pipeline;
- view-once/disappearing local-data classes;
- link-preview policy/provider interface;
- diagnostic redaction.

Exit: fuzz corpus covers all public parsers and malicious attachment metadata; view-once data is absent from backup/index/cache assertions.

### WP-008 — Production crypto integration gate

Do not begin by writing a new secure-messaging protocol from primitives. Perform the dedicated library/protocol review in `PRODUCTION_CRYPTO_INTEGRATION_GATE.md`, then integrate only the approved profile.

Exit: independent security review plan, vectors, downgrade tests, compromise/recovery tests and library maintenance strategy exist.

### WP-009 — Push + PWA operational behavior

Implement:

- per-user Web Push sender from recipient Home;
- opaque wakeup payloads;
- local decrypt/render;
- subscription rotation/change handling;
- privacy presets;
- honest background-delivery behavior.

### WP-010 — Multi-device and recovery

After AccountControlState is real:

- device linking;
- encrypted history transfer;
- controller promotion policy;
- quarantine/revocation UX;
- RecoveryPolicy none/offline-root first;
- guardian recovery only after reviewed construction selection;
- backup anti-rollback.

### Later packages

Groups/MLS, calls/SFrame, StoragePool mirroring, native shells, SMS/MMS/RCS and anonymity transports remain later phases documented elsewhere.

## 4. UI engineering rule

The UI should feel faster than the network:

- render local state immediately;
- optimistic updates only when rollback is semantically safe;
- never fake irreversible security success;
- animations are interruptible and never gate an action;
- no layout jump when delivery state changes;
- cache/preload locally without violating LocalDataClass rules.

## 5. Multiple identities

Build namespacing/isolation now; expose switching later.

The future UI should be simple:

```text
avatar/profile chip
  -> Personal
     Work
     Temporary
     + Add identity
```

Creating a profile may optionally offer a distinct built-in theme so the user gets an immediate visual cue. This is convenience only and must not create a cross-profile identifier.

## 6. Rich text

Telegram-like formatting is a baseline, but OM must use its own safe semantic representation rather than HTML/Markdown-on-the-wire. See `RICH_TEXT_AND_CONTENT_SECURITY.md`.

## 7. Definition of “secure build”

Do not advertise a prototype as “bulletproof.” A production-security claim requires all MUST items in `PRODUCTION_CRYPTO_INTEGRATION_GATE.md`, formal/control-state gates, fuzzing, supply-chain floor and an independent review.
