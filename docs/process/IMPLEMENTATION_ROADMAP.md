# OM Implementation Roadmap

> **CURRENT STATUS — ROADMAP NOT AUTHORIZED:** This is the intended implementation order after architecture freeze. The 2026-09-22 23:02 checkpoint remains pre-engineering. Architecture work may use bounded feasibility experiments, but must not begin this roadmap as product implementation.

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
- operatorless PWA/reference client first; native Android/iOS shells follow over the same Core/UI after the vertical slice;
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
6. **Privacy presets** — Recommended/Hardened/Maximum Privacy as policy bundles; same crypto floor.
7. **Push** — opaque wakeups, local decrypt/render; Android alternate path; honest iOS no-APNs limitation.
8. **Retention** — disappearing/view-once backup/convergence rules.
9. **Extensibility** — critical-field semantics, canonical encoding and registries before interoperability freeze.

Endpoint-attestation classes and nation-state anonymity transports remain deliberately later, with interfaces reserved now.

## 2026-09-22 roadmap refinement

Add an operatorless client track:

1. define portable OM Core API and canonical test vectors;
2. ship operatorless PWA against user-owned Worker/DO deployment;
3. implement per-user Web Push and ordinary daily-driver messaging/calls;
4. harden reproducible/provenance-aware reference distribution;
5. build Android native shell over the same core;
6. build iOS native shell over the same core;
7. add Android SMS/MMS/RCS adapters only after OM-native behavior is stable;
8. allow optional convenience/enterprise providers without protocol privilege.

Add Creator-Disappearance Gate to release acceptance.

## Shared UI implementation track — 2026-09-22 13:42

Run in parallel with core protocol prototypes once typed view-state/command boundaries are available:

1. design tokens + accessible primitives;
2. adaptive AppShell (Compact/Medium/Expanded/Wide);
3. Chats list/detail and composer with large-history performance harness;
4. People/discovery/request/contact-sharing flows;
5. call UI and platform call-system bridge contract;
6. Settings, Recovery, Devices, Protection and OM Home progressive-disclosure flows;
7. MotionSystem/performance budgets and low-end-device profiling;
8. declarative theme/font bundle parser + sandbox validation;
9. PWA shell integration;
10. native shell proof that the same `om-ui` can be hosted without semantic fork.

Before visual polish freezes, usability-test onboarding, message requests, Share Contact, recovery consequences and stolen-device suspension with non-technical users.

## Engineering execution checkpoint — 2026-09-22 14:34

The authoritative execution order is now `ENGINEERING_HANDOFF.md`. Build multi-profile namespacing now but expose multiple-account switching later. RichTextV1, DeviceVault/ClientHost, LocalDataClass, diagnostics redaction and formal AccountControlState are no longer optional design research; they are implementation requirements.

The first product proof uses one visible profile and a dev-only crypto provider where necessary. Production cryptography is integrated only after the explicit crypto gate. This allows UI/state/relay engineering to proceed without pretending an unaudited protocol composition is secure.


## Pre-engineering extension/ecosystem freeze — 2026-09-23 07:18 PHT

Before any eventual implementation of expressive media, packs, services/LLMs or plugins, engineering must consume `EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`, `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md` and `PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`. These decisions do not authorize engineering; V-010..V-012 remain explicit feasibility/adversarial gates.
