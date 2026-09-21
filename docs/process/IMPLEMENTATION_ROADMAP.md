# OM Implementation Roadmap

## Guiding rule

Build the secure OM-native product first. Add integrations only after the abstraction they depend on is proven.

## Phase 0 — architecture/test harness

Deliverables:

- canonical encodings and golden vectors for identity/events/provider capabilities;
- CryptoProfile registry format and negotiation vectors;
- threat model;
- provider interfaces;
- reference local relay/blob implementations;
- headless conformance runner.

Exit: two independent test implementations can pass core vectors without sharing application code.

## Phase 1 — OM-native 1:1 messenger

- AccountID/device lifecycle;
- QR/invite discovery;
- MessageRequests;
- accepted RelationshipBinding;
- PQ/hybrid-capable profile v1;
- 1:1 encrypted delivery;
- canonical create/edit/delete/reaction/receipt events;
- local encrypted DB;
- attachments via BlobStore;
- push wakeups;
- Android + PWA/reference clients, iOS close behind;
- relay/provider migration.

Exit: ordinary users can install and reliably message without infrastructure knowledge.

## Phase 2 — daily-driver security/reliability

- multi-device sync/history transfer;
- device link/revoke hardening;
- recovery flow;
- MLS private groups;
- key transparency/witness prototype;
- StoragePool backup/restore/mirroring;
- blocking/report evidence;
- provider quota/budget policy;
- security review/fuzzing/reproducibility improvements.

Exit: groups, device loss/replacement, provider failure, and restore are survivable.

## Phase 3 — calls

### 3A 1:1 voice/video

- encrypted OM signaling;
- SFrame layer;
- WebRTC ICE/STUN;
- LAN/direct P2P;
- TURN fallback/privacy mode;
- route migration;
- Cloudflare MediaProvider reference adapter + alternative provider test adapter.

### 3B group calls

- small-group mesh policy;
- SFU provider interface;
- MLS/SFrame group keying integration;
- adaptive topology;
- participant join/leave rekey behavior.

Exit: media provider cannot decrypt content; call survives route/provider changes.

## Phase 4 — infrastructure freedom / frictionless deployment

- one-click reference deployments;
- Cloudflare relay/R2/media profile;
- ordinary VPS Docker Compose relay/blob/media alternatives;
- provider health/cost discovery;
- user/community provider onboarding;
- local/LAN rendezvous prototype;
- failure-domain diversity policies.

Exit: a community can replace the default infrastructure without forking OM.

## Phase 5 — Android SMS/MMS

- platform/store approval work;
- default-SMS integration where allowed;
- `SmsMmsTransport`;
- unified conversation UI with explicit security provenance;
- no silent downgrade.

## Phase 6 — RCS

- only where authorized/provider APIs permit;
- `RcsTransport` capability mapping;
- carrier/provider-specific adapters isolated from OM core.

## Phase 7 — optional ecosystem

- business ServiceIdentity tooling;
- safe automation/bot endpoints;
- bridge adapters;
- specialized clients/accessibility clients;
- optional public/community discovery with separate moderation model.

## Cross-cutting work every phase

- conformance tests;
- migration tests;
- fuzzing;
- dependency/supply-chain review;
- privacy threat-model update;
- operational cost tests;
- accessibility;
- low-end-device/network testing;
- backup/restore regression tests.

## Discovery/resilience implementation sequence

Before public username search becomes a product dependency, implement in this order:

1. QR/`om:` invite -> ContactDescriptor -> revocable IntroGate flow.
2. ContactDescriptor validation, expiry, caching and alias provenance.
3. Private accepted relationship capabilities and multi-relay RelaySet/failover.
4. Replaceable DirectoryProvider search and domain-scoped `/.well-known/om` delegation.
5. Directory transparency/equivocation defenses.
6. Optional oblivious/private lookup path.
7. Rotating LAN rendezvous and offline-local reconciliation.
8. Pluggable network transports/censorship-resilience adapters after the core relay protocol is stable.

## 21:32 priority refinements

Before wire-format freeze / public security claims:

1. **AccountControlState prototype first** — transition table, quarantine/revocation/promotion, fork/freshness tests.
2. **Recovery v1** — default none; offline 256-bit RecoveryRoot; optional guardian sharing after library/design review; history recovery separate.
3. **Personal Cloudflare bootstrap** — OAuth/PKCE provisioning for Worker + SQLite DO; `workers.dev` publisher; no R2 dependency.
4. **ContactCapsule/share-contact** — subject-signed forwarding + optional referral context.
5. **Supply-chain floor** — signed tags, dependency pinning, provenance, TUF-style update roles before first public binary.
6. **Privacy presets** — Standard/Hardened/Maximum Privacy as policy bundles; same crypto floor.
7. **Push** — opaque wakeups, local decrypt/render; Android alternate path; honest iOS no-APNs limitation.
8. **Retention** — disappearing/view-once backup/convergence rules.
9. **Extensibility** — critical-field semantics, canonical encoding and registries before interoperability freeze.

Endpoint-attestation classes and nation-state anonymity transports remain deliberately later, with interfaces reserved now.
