# OM Lean Architecture Rules

- **Status:** Normative Phase-0 design constraints
- **Timestamp:** 2026-08-18 14:03 PHT

## Core rule

> Spend complexity only where it buys interoperability, security, reliability, or user freedom.

Everything else must earn its place.

## Complexity budget

Before adding a subsystem, answer:

1. What stable contract does OM Core need?
2. Can it be replaced without changing AccountID or canonical conversation state?
3. What secrets/plaintext/metadata can it observe?
4. What is the smallest conformance test proving replacement works?
5. Can the feature remain outside the baseline protocol?
6. What happens if the provider/component disappears tomorrow?

If these cannot be answered simply, the subsystem is not ready.

## Resource rules

- Attachments are out-of-band encrypted blobs.
- Baseline durable event ≤ 64 KiB.
- Batch receipts, acknowledgements, and housekeeping.
- Relay stores ciphertext only as long as delivery/offline policy requires.
- No mandatory permanent server-side history.
- No global social graph as a core dependency.
- No mandatory global presence service.
- No expensive work before cheap request validation/rate limiting.
- No generic microservice split unless independent scaling/security boundaries justify it.
- No Kafka-class infrastructure before measured queue requirements justify it.
- No mandatory cross-provider storage replication; it is a user policy.
- No public group/channel architecture in private-chat v1.
- No arbitrary executable plugin system inside the official client in v1.
- No invented cryptography.

## Protocol rules

- Provider names never appear in core identity semantics.
- UI concepts never define wire protocol truth.
- Unknown optional capability must degrade safely.
- No silent security downgrade across bridges/transports.
- Local preferences stay local unless explicitly designed for private sync.
- Security-critical state has one authoritative protocol representation.
- Derive UI notices from authoritative control state instead of duplicating it.

## Operational rules

Reference infrastructure starts with:

- one simple relay binary/service;
- one boring durable metadata store only if required;
- object storage for encrypted blobs;
- push/wake gateway;
- basic metrics/logging with privacy discipline.

Scale by measurement, not imagination.

## Client rules

- Official UI is important but replaceable.
- Third-party clients use public contracts.
- Core logic has headless test harnesses.
- PWA is first execution path, not protocol authority.
- Native clients must be able to implement the same semantics without browser-specific behavior.

## Storage rules

Separate:

1. delivery queue;
2. hot sync;
3. hot media;
4. cold archive;
5. recovery.

Do not pay hot-storage cost for cold data.

StoragePool policies may select, mirror, or later erasure-code encrypted objects across providers, but v1 starts with single/mirror policies because they are easier to restore and reason about.

## Anti-features for Phase 0/early execution

Defer:

- public feeds;
- stories;
- giant broadcast channels;
- payments;
- ads;
- marketplace;
- arbitrary bot platform;
- full social graph;
- complex federation state resolution;
- exotic storage coding before mirroring works;
- voice/video until messaging is stable.

## 2026-09-24 reconciliation with MESSENGER_BASELINE_V1 (ADR-0108)

The anti-feature list above is now read as **delivery sequencing**, not semantic exclusion. [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md) fixes V1 meanings, and [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) sequences delivery: W0 1:1 kernel; W1 groups/media/export; W2 calls; W3 communities/publications; W4 services/plugins/bridges. Payments, ads, marketplaces, stories and exotic storage coding stay outside V1. A later wave may never add wire requirements to an earlier wave beyond frozen extension points.
