# Release Waves and Scoped Engineering Unlock

- **Status:** Canonical architecture-process contract
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decisions:** ADR-0108, ADR-0109
- **Engineering:** not authorized. This document defines *how* a later ADR may unlock a scope. It unlocks nothing.

## 1. Problem

Before this checkpoint, engineering was one boolean that required **every** blocking gate to pass. Two defects followed:

1. **Circular lock.** Some gates can only pass with real implementations: V-013 needs real bridge adapters, V-014 two independent archive implementations, V-015 real packet/provider traces, V-017 an independent rebuild of a compatible implementation, and V-020 a second provider implementation. Engineering could therefore never be honestly authorized. The realistic failure mode was a silent waiver under schedule pressure, which is worse than a designed unlock.
2. **All-or-nothing coupling.** A plugin-sandbox proof (V-010) or a bridge proof (V-013) blocked the 1:1 messaging kernel, even though the kernel is needed to build either of them.

## 2. Split every gate into two phases

Every verification gate now carries:

| Field | Meaning |
|---|---|
| `pre_engineering_criteria` | Evidence obtainable with models, vectors, non-production feasibility prototypes and design review. Required before a scope that depends on the gate may be authorized. |
| `pre_release_criteria` | Evidence that needs real implementations, traces, independent rebuilds or users. Required before any public release or claim that depends on the gate. |
| `pre_engineering_status` | `open` / `passed` / `failed` / `waived_by_adr` |
| `status` | Overall gate status. `passed` only when both phases pass. |
| `independence` | Minimum independence of evidence (see [`verification/GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md)) |
| `unlock_scopes` | Engineering scopes this gate blocks |

[`verification/GATE_PASS_CRITERIA.md`](../../verification/GATE_PASS_CRITERIA.md) is the human-readable view. [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl) is authoritative.

## 3. Engineering scopes

[`machine/repository_contract.json`](../../machine/repository_contract.json) → `engineering_scopes`. Every scope starts `authorized: false`.

| Scope | Wave | Contents | Gates (pre-engineering criteria must pass) |
|---|---|---|---|
| `core_kernel` | W0 | canonical encoding, AccountControlState, 1:1 sessions/crypto, async key material, relay delivery, blobs, multi-device sync, IntroGate/block | V-001, V-002, V-003, V-005, V-015, V-016 |
| `reference_client_ui` | W0 | om-ui shell, onboarding, device vault, PWA ClientHost | V-006, V-018, V-021 |
| `provider_reference` | W0 | reference relay/home deployments, sponsored mailbox, provisioning | V-015, V-020 |
| `groups` | W1 | MLS rooms, GroupControlState, convergence, PQ-group ReInit path | V-004, V-009, V-019 |
| `portability` | W1 | om-archive-v1 export/import | V-014 |
| `expressive_media` | W1 | stickers, animation, content packs | V-012 |
| `calls` | W2 | 1:1 and group calls, SFrame | V-007 |
| `communities_publications` | W3 | Spaces, topics, Publications, scoped moderation | V-008, V-016 |
| `services_bots` | W4 | ServicePrincipals, LLM agents | V-011 |
| `plugins` | W4 | Wasm Component-Model plugin host | V-010 |
| `bridges` | W4 | external-network bridges | V-013 |

V-017 (0M continuity drill) blocks **every public production claim**, not any engineering scope. Its pre-engineering criteria (continuity bundle assembly, registry immutability checks, tabletop drills) still have to be done before `core_kernel` is authorized, because it is also listed in the contract's `always_required_pre_engineering_gates`.

## 4. Unlock rule

A scope may become `authorized: true` only when **all** of these hold:

1. an accepted ADR names the scope explicitly;
2. every gate that lists the scope in `unlock_scopes` has `pre_engineering_status` = `passed` or `waived_by_adr`, and so does every gate in `always_required_pre_engineering_gates`;
3. no open `architecture_design_queue` item lists the scope;
4. every scope of an earlier wave that this scope depends on is already authorized (`depends_on`);
5. `python3 tools/validate_architecture.py` passes.

The global `engineering_authorized` flag stays `false` until the operator authorizes at least one scope. After that it records "some engineering authorized"; per-scope flags are the real control. The validator rejects any authorized scope whose gates are unmet.

## 5. Waves

V1 remains a **semantic** contract: [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md) fixes meanings, event names and extension points. Delivery is sequenced:

| Wave | Delivers | Why this order |
|---|---|---|
| W0 | 1:1 messaging, attachments, multi-device, onboarding with Home tiers, block/IntroGate, local search | Smallest system that proves identity, delivery, continuity and usability. Everything else depends on it. |
| W1 | Private groups, pins, polls, reactions/edits at group scale, stickers/animation, archive export | Group ergonomics are an adoption requirement (R-113). |
| W2 | 1:1 and group calls | Calls depend on stable sessions, groups and push lifecycle, and platform risk is highest here. |
| W3 | Spaces, topics, Publications, delegated moderation | Community features need a stable group/authority core. |
| W4 | Services/bots/LLM agents, plugins, bridges | Ecosystem surfaces; highest authority-boundary risk. |

Rules:

- A later-wave feature must not add wire requirements to an earlier wave beyond the reserved extension/criticality points already frozen in V1 semantics.
- A later-wave gate must never block an earlier-wave scope.
- Reordering waves needs an ADR. Reordering must not remove a gate from a scope.

## 6. Reconciliation with LEAN rules

[`LEAN_ARCHITECTURE_RULES.md`](../overview/LEAN_ARCHITECTURE_RULES.md) "Anti-features for Phase 0/early execution" is now read as **delivery sequencing**, not semantic exclusion:

- public feeds/giant channels → W3 Publications (no massive private broadcast crypto in V1, unchanged);
- arbitrary bot platform/executable plugins → W4, behind V-010/V-011;
- voice/video until messaging is stable → W2.

Payments, ads, marketplaces, stories and exotic storage coding remain outside V1 entirely.
