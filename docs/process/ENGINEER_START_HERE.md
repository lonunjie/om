# ENGINEER START HERE — design-frozen, engineering still locked

**Final architecture checkpoint:** 2026-09-26 (ADR-0139)  
**Architecture design:** closed for every scope except `local_delivery` (`design_open=1`: A-055, the LAN wire binding; ADR-0156)  
**Engineering authorization:** **NO** — use [`machine/repository_contract.json`](../../machine/repository_contract.json); a closed design is not an authorized scope. Engineering is not authorized.

This is the shortest correct entry point for a future repo/build agent. Do not reconstruct policy from chat history.

## Read in this order

1. [`AGENTS.md`](../../AGENTS.md) — repository constitution and phase rules.
2. [`SNAPSHOT.md`](../../SNAPSHOT.md) — current checkpoint/state.
3. [`ENGINEERING_HANDOFF.md`](ENGINEERING_HANDOFF.md) — source-of-truth precedence, non-negotiable boundaries and scope unlock rules.
4. [`SECURITY_INVARIANTS.md`](../architecture/safety/SECURITY_INVARIANTS.md) — compact cross-cutting security properties.
5. [`2026-09-26-final-architecture-closure-and-adversarial-review.md`](../reports/2026-09-26-final-architecture-closure-and-adversarial-review.md) — final design-phase attack pass and residual evidence boundaries.
6. [`machine/repository_contract.json`](../../machine/repository_contract.json), [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl), [`machine/public_release_gates.jsonl`](../../machine/public_release_gates.jsonl), [`machine/requirements.jsonl`](../../machine/requirements.jsonl), [`machine/threat_register.jsonl`](../../machine/threat_register.jsonl), [`machine/security_invariant_traceability.json`](../../machine/security_invariant_traceability.json).

For the subsystem you are working on, then follow the architecture map/ADRs/specs for that scope.

## Phase rule

Architecture is **design-frozen**, but product engineering is not currently authorized. Work that the verification lifecycle explicitly permits before scope authorization stays under `verification/` and must remain non-production evidence. Do not create production code because the design is closed for your scope. The one open design item (A-055) blocks only `local_delivery`. Later-wave scopes also require their own wire objects to be frozen as CDDL with two-codec vectors before authorization (ADR-0156).

A named engineering scope becomes legal only when [`machine/repository_contract.json`](../../machine/repository_contract.json) says its unlock conditions are satisfied and an accepted ADR explicitly authorizes that scope.

## Sources of truth

When documents seem to conflict, do not improvise. Apply this order and raise an ADR if conflict remains:

1. constitution / [`AGENTS.md`](../../AGENTS.md) / repository contract and current machine ledgers;
2. accepted current ADRs and canonical architecture documents;
3. for **frozen W0 wire objects**, canonical CDDL + frozen registries + domain-separation/signature/hash rules;
4. logical schemas/models as explanatory/tooling views only — they cannot add frozen W0 wire fields or semantics;
5. verification vectors/conformance material;
6. implementation convenience last.

## Non-negotiable implementation rule

Do not silently simplify a security, authority, privacy, downgrade, recovery, provider or extension boundary. If implementation reality contradicts the design:

1. preserve the safe behavior/fail closed;
2. collect concrete evidence;
3. write an ADR;
4. update requirements, threats, migrations and conformance/evidence obligations;
5. only then change the canonical architecture.

## Scope authorization

The eleven scopes are `core_kernel`, `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, `calls`, `communities_publications`, `services_bots`, `plugins`, and `bridges`. Their dependencies/gates are machine-owned. Never infer authorization from wave order or from a green test file.

## Two easy-to-miss closure rules

- SecurityAdvisory expiry/supersession/override may remove an advisory-added recommendation, **never** lower an already-established relationship floor or revive a forbidden profile.
- `OMA-ENC-1-candidate` is a **verification-only** archive-encryption placeholder. It cannot ship; V-014 must freeze the reviewed production profile before portability authorization.

## Completion signal

The architecture team is done unless a later review or implementation experiment exposes an actual contradiction. The next work is evidence and scoped engineering authorization, not another speculative redesign pass.
