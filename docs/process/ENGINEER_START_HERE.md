# OM Engineering — Start Here

**Checkpoint:** 2026-09-22 14:34 PHT (UTC+08:00)  
**Phase:** Architecture candidate -> engineering prototype + adversarial validation

This repository is the canonical handoff to an engineering/coding agent. The design phase is intentionally being closed. Do not reopen settled architectural questions merely because a different implementation is easier.

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

1. `AGENTS.md` — absolute constraints.
2. `README.md` and `SNAPSHOT.md` — current source of truth.
3. `ENGINEERING_HANDOFF.md` — build sequence and acceptance gates.
4. `ENGINEERING_REPOSITORY_LAYOUT.md` — code ownership/boundaries.
5. `research/SECURITY_CRYPTO_REAUDIT_20260922.md` — current security/crypto baseline.
6. `PRODUCTION_CRYPTO_INTEGRATION_GATE.md` — what must be true before calling any build secure/production.
7. `DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`.
8. `MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`.
9. `RICH_TEXT_AND_CONTENT_SECURITY.md`.
10. `LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`.
11. `FORMAL_VERIFICATION_AND_FUZZING.md`.
12. Existing protocol/architecture/UI documents as the relevant work package reaches them.

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

Production cryptography is blocked by `PRODUCTION_CRYPTO_INTEGRATION_GATE.md`.

## First engineering milestone

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
