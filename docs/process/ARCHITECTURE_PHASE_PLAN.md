# OM Architecture Phase Plan — revised 2026-09-21

Each gate produces an ADR, threat-model consequences, machine-readable decisions/requirements, and conformance expectations.

## Gate A — Identity and continuity

Selected direction: algorithm-independent AccountID, independent DeviceIdentity, local PersonRecord, aliases as claims, provider independence.

Remaining:
- canonical AccountState/credential epoch encoding;
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
