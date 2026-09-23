# OM Engineering — Start Here

**Checkpoint:** 2026-09-23 08:21 PHT (UTC+08:00)  
**Phase:** FUTURE HANDOFF — engineering not authorized by this checkpoint

> **2026-09-24:** engineering unlock is now scope-granular ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md)). The first vertical slice below corresponds to the W0 scopes `core_kernel`, `reference_client_ui` and `provider_reference`. None is authorized. Also read [`HARDENING_REVIEW_20260924.md`](../reviews/HARDENING_REVIEW_20260924.md), [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) (the "personal OM Home" may be a T0 sponsored mailbox or T1 hosted Home, not only an own-cloud deployment), [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) and [`verification/GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md).

This file is retained so the future transition is explicit, but the current repository is **not yet an engineering handoff**. Architecture hardening remains active. Do not begin the product vertical slice until a later ADR sets `engineering_authorized=true` in [`machine/repository_contract.json`](../../machine/repository_contract.json) and the architecture validator passes.

## Mission

Build the smallest real OM vertical slice that proves the architecture:

```text
one portable om-core
+ one shared om-ui
+ one operatorless PWA
+ one user-owned personal OM Home
+ one real end-to-end 1:1 message path
```

without introducing a mandatory central operator or weakening the documented security boundaries.

## Read in this order

1. [`AGENTS.md`](../../AGENTS.md) — absolute constraints.
2. [`README.md`](../../README.md) and [`SNAPSHOT.md`](../../SNAPSHOT.md) — current source of truth.
3. [`ENGINEERING_HANDOFF.md`](ENGINEERING_HANDOFF.md) — build sequence and acceptance gates.
4. [`ENGINEERING_REPOSITORY_LAYOUT.md`](ENGINEERING_REPOSITORY_LAYOUT.md) — code ownership/boundaries.
5. [`docs/research/SECURITY_CRYPTO_REAUDIT_20260922.md`](../research/SECURITY_CRYPTO_REAUDIT_20260922.md) — current security/crypto baseline.
6. [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md) — what must be true before calling any build secure/production.
7. [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](../architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md).
8. [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md).
9. [`RICH_TEXT_AND_CONTENT_SECURITY.md`](../architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md).
10. [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md) + [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md) + [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md).
11. [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) + [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) + [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md).
12. [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md) + [`DECENTRALIZED_ABUSE_MODERATION.md`](../architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md) + [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md).
13. [`LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`](../architecture/storage/LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md).
14. [`FORMAL_VERIFICATION_AND_FUZZING.md`](FORMAL_VERIFICATION_AND_FUZZING.md).
15. Existing protocol/architecture/UI documents as the relevant verification/work package reaches them.

## Engineering defaults

These are implementation defaults, not protocol dependencies:

- **Core/security/state:** stable Rust, memory-safe by default; `unsafe` forbidden in protocol/state crates unless an ADR isolates and audits it.
- **Web/reference UI:** TypeScript + React + Vite is the default starting stack because it maximizes maintainability, tooling and coding-agent reliability. The product architecture remains framework-independent; replace only after a measured spike demonstrates a material problem.
- **PWA bridge:** Rust -> WASM via a narrow generated/typed boundary.
- **Future native bridge:** Rust -> Swift/Kotlin through a generated FFI layer such as UniFFI or equivalent, selected after the native bridge spike.
- **Cloudflare personal Home reference:** TypeScript Worker + SQLite Durable Object. No mandatory R2.
- **Signed/wire/control encoding:** OM deterministic CBOR profile derived from RFC 8949; no JSON signatures.
- **Formal model:** TLA+ first for AccountControlState; model checking is a CI gate for control-state changes.

Versions must be current stable at implementation time and pinned by lockfiles. Do not copy version numbers from this planning snapshot without checking upstream.

## Critical rule: prototype crypto is not production crypto

Engineering may initially use a clearly named **DEV-ONLY CryptoProvider** to unblock state/UI/network plumbing. It MUST:

- be impossible to mistake for a production profile;
- refuse production/release builds;
- never be called E2EE/PQ secure in UI or docs;
- have test-only keys/state clearly separated from real profiles.

Production cryptography is blocked by [`PRODUCTION_CRYPTO_INTEGRATION_GATE.md`](../architecture/crypto/PRODUCTION_CRYPTO_INTEGRATION_GATE.md).

## Future first engineering milestone (inactive until authorization)

The first milestone is not “build a beautiful messenger.” It is:

1. reproducible monorepo/bootstrap;
2. canonical types/encoding test harness;
3. AccountControlState TLA+ model;
4. DeviceVault/Profile storage abstraction;
5. shared responsive UI shell;
6. personal Cloudflare Home emulator/reference deployment;
7. QR/invite -> message request -> accept -> relationship vertical slice using dev crypto;
8. local encrypted-state plumbing and hostile-input limits;
9. fuzz/conformance CI.

Only after that vertical slice is stable should production crypto be integrated.

## Architecture-change rule

When implementation reality contradicts the design:

- collect evidence/measurements;
- write an ADR describing the conflict and proposed change;
- update threat model, machine records, migration story and conformance tests;
- do not silently “simplify” protocol/security semantics in code.

## Completion definition for this handoff

A coding agent can start work without reconstructing decisions from chat. Unresolved choices are explicitly gates, not missing context.

## Current checkpoint lock — 2026-09-23

This document is retained for the future transition only. Product engineering is **not authorized** in the current checkpoint. The architecture-design queue is resolved, but all blocking architecture-verification gates must pass first; [`machine/repository_contract.json`](../../machine/repository_contract.json) is authoritative.
