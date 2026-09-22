# OM Architecture Phase Plan — revised 2026-09-21

Each gate produces an ADR, threat-model consequences, machine-readable decisions/requirements, and conformance expectations.

## Gate A — Identity and continuity

Selected direction: algorithm-independent AccountID, independent DeviceIdentity, local PersonRecord, aliases as claims, provider independence.

Remaining:
- canonical AccountControlState/credential epoch encoding;
- recovery cryptographic construction;
- credential-transition authorization across algorithm families.

## Gate B — Canonical event model

Selected direction: small immutable semantic event model + materialized local views.

Remaining:
- deterministic CBOR/CDDL or equivalent canonical encoding;
- golden vectors and conflict/convergence tests.

## Gate C — Cryptography / CryptoProfile v1

Requirements:
- reviewed asynchronous 1:1 PQ/hybrid-capable construction;
- continuous post-compromise/forward-security ratchet;
- identity/device authentication plan including PQ migration;
- MLS implementation selection;
- profile lifecycle/registry/downgrade rules;
- background migration design.

Exit: complete vectors, negative downgrade tests, library replacement boundary.

## Gate D — Relay and delivery

Specify:
- opaque capability creation/rotation/revocation;
- push/pull/WebSocket semantics;
- ack/delete/idempotency;
- bounded expiry;
- randomized per-destination transport wrapping;
- multi-relay failover;
- introduction endpoint/rate limits;
- reference DO/VPS implementation profiles.

## Gate E — Multi-device sync/history

Specify account-private device synchronization, new-device bootstrap, encrypted history transfer, revocation propagation, and conflict handling without a master phone.

## Gate F — StoragePool

Specify blob descriptor, encrypted manifest, mirroring, repair, migration, backup snapshot, recovery-store separation, restore into newer CryptoProfile.

## Gate G — Abuse/safety

Specify server-enforceable first-contact limits, blocking, provider-local denial, report evidence bundles, resource-exhaustion limits, and public/discovery moderation boundaries.

## Gate H — Calls/media

Specify:
- call signaling/control schema;
- CallSessionID;
- SFrame keying;
- 1:1 call key authentication;
- MLS group call keying;
- ICE/STUN/TURN route policy;
- LAN rendezvous privacy;
- SFU interface;
- mid-call route/provider migration;
- privacy and zero-budget policies.

## Gate I — Provider capability contracts

Finalize versioned interfaces for DirectoryProvider, RelayProvider, PushProvider, BlobStore, BackupStore, RecoveryStore, MediaProvider, StoragePool, TransportAdapter, cost/quota/health signals.

## Gate J — External transports and bridges

After OM-native stability, implement SMS/MMS/RCS/bridge security/provenance semantics without changing canonical conversation semantics.

## Gate K — Versioning, conformance, governance

Finalize:
- protocol version negotiation;
- CryptoProfile lifecycle registry;
- public conformance suite;
- release transparency/reproducibility strategy;
- governance/change process;
- compatibility expectations for forks/third-party clients.

## Added checkpoint gates

- **D1 Discovery fabric:** ContactDescriptor encoding, resolver policy, alias provenance, domain delegation, cache/expiry semantics.
- **D2 IntroGate:** revocable invite lifecycle, ticket format, replay protection, provider-side anti-abuse and public-figure scale testing.
- **D3 Multi-relay:** RelaySet encoding, capability rotation, independent outer wrapping, failover/redundancy convergence tests.
- **D4 LAN:** rotating rendezvous derivation and local reconciliation without stable identity broadcast.
- **I2 Network resilience:** stable NetworkTransport contract and alternate/pluggable transport testing after core relay stability.

## New Gate A2 — Account continuity and recovery

Exit criteria:

- authenticated AccountControlState transition specification;
- no single compromised ordinary controller can fabricate permanent ownership in a healthy multi-controller account;
- quarantine/revocation/fork tests;
- recovery-off-default semantics and independent history recovery;
- stale-backup anti-rollback tests.

## New Gate K2 — Software trust

Exit criteria before public production claims:

- release-root key ceremony/threshold;
- signed release tags and forge mirrors;
- pinned build dependencies/actions;
- build provenance;
- TUF-style rollback/freeze protections or equivalent;
- reproducibility target documented/tested for supported platforms where feasible.

## Architecture-freeze gate — 2026-09-22 23:02

The previous 14:34 decision to enter engineering is superseded. The project remains in **architecture hardening / pre-engineering**.

Before product engineering is authorized, the architecture must close or safely bound every issue capable of forcing a later incompatible security/wire/authority redesign. Blocking additions from the 23:02 audit are:

1. asynchronous per-device 1:1/MLS key-material publication, depletion and multi-provider consumption;
2. explicit multi-device 1:1 session-management state machine;
3. MLS same-epoch Commit ordering plus atomic OM GroupControlState↔MLS binding;
4. RelaySet catastrophic-known-route-loss semantics;
5. stable BlobID plus authenticated mutable BlobLocationRecord repair;
6. one cross-protocol time/expiry/skew model;
7. self-hosted ClientHost release/update/rollback/recovery lifecycle;
8. protocol patent/RF contribution policy and mixed-license packaging review before broad contributions/public release;
9. machine-enforced repository/architecture map with no unclassified current files.

### Engineering authorization criteria

A later checkpoint may authorize engineering only when:

- every blocking architecture open question is resolved or explicitly proven bounded/non-blocking;
- normative terminology/schemas expose one canonical current definition;
- security-critical provider/state-machine interfaces have adversarial failure semantics;
- external feasibility assumptions have current authoritative evidence;
- `python3 tools/validate_architecture.py` passes;
- an accepted ADR explicitly changes `engineering_authorized` to true.

Feasibility prototypes/formal models used only to answer architecture questions are permitted; they must not create accidental production commitments.
