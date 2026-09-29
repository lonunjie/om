# OM Engineering Handoff — DESIGN-FROZEN, INACTIVE UNTIL NAMED-SCOPE AUTHORIZATION

> **Current design checkpoint: 2026-09-29 / ADR-0169.** Architecture design is closed. Product engineering is authorized for scopes `core_kernel` (ADR-0169) and `provider_reference` (ADR-0171) only; this document authorizes nothing by itself, and every other scope is not authorized. [`machine/repository_contract.json`](../../machine/repository_contract.json) is authoritative for scope unlock.

## 1. What “design-frozen” means

The protocol/security/product architecture is sufficiently specified to stop making new architecture choices by default. It does **not** mean the implementation verification gates have passed. Future implementation work should consume the design, measure it and challenge it with evidence; it should not silently substitute new authority/privacy/security semantics.

A contradiction discovered during implementation is valuable evidence. Preserve safe behavior, write an ADR and update the machine ledgers/conformance story before changing the contract.

## 2. Source-of-truth precedence

When two artifacts appear inconsistent:

1. constitution, [`AGENTS.md`](../../AGENTS.md), [`machine/repository_contract.json`](../../machine/repository_contract.json) and current machine truth;
2. accepted current ADRs + canonical architecture documents;
3. for a frozen W0 object, **CDDL + frozen registries + domain-separation/signature/hash rules define the accepted wire bytes**;
4. logical JSON schemas/models are documentation/tooling views and cannot add fields/critical-extension/encoding semantics to frozen W0 wire objects;
5. conformance vectors and bounded evidence;
6. implementation detail.

Do not resolve a true conflict by choosing the convenient interpretation. File an ADR.

## 3. Non-negotiable architecture boundaries

Engineering MUST preserve at least these properties:

- no mandatory OM/SeedLife-operated runtime service for established independent communication;
- AccountID is stable identity, never a routing address and never derived from replaceable provider/key coordinates;
- AccountControlState is the constitutional authority object; recovery/provider/directory/service/bridge/plugin/update roles do not become alternate account authority;
- accepted relationships use opaque rotatable routing/admission capabilities rather than stable canonical IDs;
- every inbound 1:1 decrypt/receive has mandatory relationship context and binds sender identity before secret/session/plaintext processing;
- AccountControl transitions follow the normative machine policy; stale reductions target immutable incarnations; control state never uses chat HLC/LWW/ordinary CRDT authority merge;
- backups/imported archives restore data/evidence, not stale authority or routing state;
- provider administration uses narrow opaque provider-resource capabilities; those capabilities never substitute for end-user authorization;
- RecoveryRecord storage is opaque/capability addressed; recovery/legacy powers remain narrower than normal controller authority;
- strongest approved CryptoProfile behavior is automatic; profile/privacy/notification settings never silently select weaker message crypto;
- **SecurityAdvisory expiry, supersession or override never lowers the relationship's stored security floor**;
- metadata claims are observer/surface specific; same-operator roles collude by construction; cryptographic unlinkability is not advertised as transport anonymity;
- extensions/services/bridges are explicit principals/recipients with scoped capabilities and visible provenance; plaintext read and network egress are distinct powers;
- user themes/expressive assets are inert data, not executable extensions;
- v1 MLS groups are classically protected/transferably authenticated as specified; do not claim PQ confidentiality or group deniability that the profile does not provide;
- published wire generations remain independently interpretable/mirrorable; incompatible semantic evolution uses explicit version/profile/fork mechanisms rather than rewriting old meaning;
- the Creator-Disappearance Gate remains satisfiable.

The compact index is [`SECURITY_INVARIANTS.md`](../architecture/safety/SECURITY_INVARIANTS.md); its machine trace is [`machine/security_invariant_traceability.json`](../../machine/security_invariant_traceability.json).

## 4. Wire and crypto implementation rules

- Never infer signed/hashed bytes from a permissive logical schema when a W0 CDDL/registry is frozen.
- Domain-separation labels are protocol data, not refactoring strings.
- Unknown/critical-extension behavior must follow the frozen registry rules.
- Do not invent new cryptographic constructions to make an implementation easier. Use only the approved/profiled constructions and reviewed dependency path.
- `OMA-ENC-1-candidate` is verification-only. It is **not** permission to ship archive encryption. V-014 must freeze the reviewed production archive profile first.
- Guardian recovery uses only the reviewed method/profile admitted by the current RecoveryPolicy. No bespoke threshold cryptography.

## 5. Implementation-readiness and unknown-outcome rule (ADR-0163)

Before implementing a scope, read [`IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md`](IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md) and its machine-readable [`machine/engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json) entry. Authorization dependencies remain solely in [`repository_contract.json`](../../machine/repository_contract.json); integration test prerequisites do not become hidden protocol dependencies.

For any durable mutation that can cross a crash/network uncertainty boundary, preserve its existing stable logical identity before external effect when required. A lost response is not proof of rejection: exact-retry/reconcile the same operation when safe, or reconcile authenticated state before creating a new operation. Core reports `PendingNetwork`/`PendingReconciliation` rather than inventing failure. Group membership/control changes additionally use the ADR-0163 crash-safe prepared-Commit journal; no application traffic may leave a tentative group branch.


## 5A. Protocol evolution and change-control rule (ADR-0165)

Before changing normative protocol/profile/extension material, read [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) and [`machine/protocol_evolution_policy.json`](../../machine/protocol_evolution_policy.json). Published meanings are immutable; profile IDs are not security ordering; extension criticality is immutable; and behavior-changing evolution requires a new immutable lineage/profile/extension or explicit fork. Every normative change must use the strongest applicable ADR-0165 change class and update its required ledgers/conformance/migration/continuity artifacts. A local harness never counts as independent/formal/interoperability evidence.

## 6. What engineers may choose without reopening architecture

Inside an authorized scope, normal engineering choices include:

- internal data structures, storage engines and caching strategies that preserve canonical semantics;
- languages/libraries/deployment packaging consistent with licensing, supply-chain and approved-crypto constraints;
- retry/batching/queue algorithms that preserve idempotency, limits, ordering and fail-closed authority behavior;
- UI component implementation/details that preserve mandated user-visible security semantics and accessibility;
- provider deployment/operations details within the role/capability/privacy contracts;
- performance optimizations that do not change wire/security/authority meaning;
- test/fuzz harness structure;
- non-wire internal schemas and migrations.

An implementation choice becomes an architecture change if it changes who can authorize, what an observer learns, which bytes authenticate, when a downgrade is accepted, how recovery/legacy becomes effective, which principal authored an action, or how independently operated implementations interoperate.

## 7. Scope map and principal evidence gates

The machine ledger is authoritative; this summary is orientation only. “Direct gates” means gates whose `unlock_scopes` explicitly names the scope. `core_kernel` additionally requires every `always_required_pre_engineering_gate` and cross-wave `interface_impact` feasibility criterion from the repository contract; those are not misrepresented below as direct unlock gates.

| Scope | Wave | Direct gates | Authorization dependencies |
|---|---:|---|---|
| `core_kernel` | W0 | V-001, V-002, V-003, V-005, V-015, V-016 | — |
| `reference_client_ui` | W0 | V-006, V-018, V-021, V-022, V-023 | `core_kernel` |
| `provider_reference` | W0 | V-015, V-020, V-022 | `core_kernel` |
| `groups` | W1 | V-004, V-009, V-019 | `core_kernel` |
| `portability` | W1 | V-014 | `core_kernel` |
| `expressive_media` | W1 | V-012 | `core_kernel` |
| `local_delivery` | W1 | V-003, V-015, V-024 | `core_kernel` |
| `calls` | W2 | V-007 | `core_kernel`, `groups` |
| `communities_publications` | W3 | V-008, V-016 | `groups` |
| `services_bots` | W4 | V-011 | `groups` |
| `plugins` | W4 | V-010 | `reference_client_ui` |
| `bridges` | W4 | V-013 | `groups` |

Do not copy this table into authorization logic; use [`machine/repository_contract.json`](../../machine/repository_contract.json) and current gate records. The validator also checks this handoff against [`machine/engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json).

## 8. Recommended implementation layering after authorization

This is dependency order, **not** permission to start:

1. Layer 0: `core_kernel`. Internally prefer types/codec -> identity/control -> crypto/session -> event/materialization -> delivery/storage/discovery -> private sync/Core bridge.
2. Layer 1 after Core: `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, and `local_delivery` may proceed independently when each has its own authorization. Reference UI/provider implementations can use interface harnesses; neither becomes a hidden dependency of Core.
3. Layer 2: `calls`, `communities_publications`, `services_bots`, `plugins`, and `bridges` follow their declared `groups` or `reference_client_ui` authorization dependency.

End-to-end integration may of course combine several authorized scopes. That does not retroactively add an authorization dependency or protocol authority edge.

## 9. Product/UX implementation rules

- Render local state quickly, but never fake irreversible security success.
- Optimism is allowed only where semantic rollback is safe.
- Provider/crypto jargon stays out of default UX unless the user is making a real security choice.
- Warnings must match the actual guarantee: endpoint/platform leakage and metadata exposure are not cured by E2EE marketing.
- Profile/account isolation is built into local namespaces even if the first UI exposes one profile.
- Telegram-like rich text uses OM's safe semantic representation, never arbitrary HTML/Markdown execution on the wire.

## 10. Update/distribution and anti-capture rule

A ClientHost, app store, package mirror, provider, directory, transparency log, safety-advisory publisher or catalog may be important operational infrastructure, but it does not acquire constitutional protocol authority merely by being popular or bundled. Verified releases and transparency/monitoring are supply-chain defenses; they are not a kill switch.

## 11. Architecture-change rule

When reality contradicts the design:

1. reproduce/measure the contradiction;
2. fail closed where a security decision is involved;
3. write an ADR explaining the evidence and alternatives;
4. update requirements/threats/security invariants/migrations/conformance/gates as applicable;
5. obtain any required independent review;
6. then implement the accepted change.

Never ship a quiet implementation-specific exception and document it afterward.

## 12. Current lock

At this checkpoint:

- architecture design queue: closed;
- verification gates: still ledger-owned/open unless explicitly passed/waived there;
- engineering scopes: unauthorized;
- global `engineering_authorized`: false.

The next phase is evidence and named-scope authorization, not additional speculative architecture hardening.


## 2026-09-29 Run 6 — algorithm-level handoff

The Run-6 algorithm supplement is now part of the normative implementation handoff. It resolves ordering/failure interpretation for ACS validation, 1:1/sealed delivery, group Commit recovery, mutable route/location convergence, GroupHistoryBundle isolation, extension dispatch, LAN admission and unknown outcomes. It introduces no new wire fields and does not authorize engineering.


## 11. Run 6 algorithm-level contract

[`docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md`](../architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md) and [`machine/dangerous_algorithm_contracts.json`](../../machine/dangerous_algorithm_contracts.json) are now part of the normative implementation handoff. They specify ordering/failure semantics for the eight highest-risk algorithm families without introducing new wire fields or authorization. The bounded sanity harness passes 49/49 cases; V-series evidence remains independently authoritative.
