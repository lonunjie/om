# ADR-0169 — Authorize engineering scope `core_kernel` (W0)

- **Status:** Accepted
- **Decided by:** operator, 2026-09-29 (directed "do everything you need to do so we can proceed with engineering")
- **Date:** 2026-09-29
- **Authorizes:** engineering scope `core_kernel` only
- **Basis:** [`2026-09-29-claude-opus-5-5-core-kernel-review.md`](../verification/reviews/2026-09-29-claude-opus-5-5-core-kernel-review.md), ADR-0167, ADR-0168

## Context

[`machine/repository_contract.json`](../machine/repository_contract.json) makes unlock scope-granular. For `core_kernel` every condition now holds:

- every gate listing the scope (V-001, V-002, V-003, V-005, V-015, V-016) and the always-required V-017 has
  `pre_engineering_status=passed`. V-001 passed on the independent-implementation/upstream-KAT basis; the others passed
  under the ADR-0167 review model;
- every gate whose `interface_impact` lists `core_kernel` (V-004, V-008, V-009, V-011, V-013, V-014, V-019) has
  cross-wave feasibility evidenced;
- no open design item blocks it; it has no scope dependencies;
- every conformance suite naming it is `specified`; and
- the validator and `make verify` pass.

## Decision

1. `core_kernel` is authorized. Product code for it may be written in this repository following
   [`ENGINEERING_REPOSITORY_LAYOUT.md`](../docs/process/ENGINEERING_REPOSITORY_LAYOUT.md) (Rust workspace crates
   `om-types`, `om-codec`, `om-protocol`, `om-core`, `om-control`, `om-crypto-api`, `om-storage-api`, `om-discovery`,
   `om-relay-client`, plus `conformance/` and `fuzz/`).
2. Implementation follows the `core_kernel` entry in
   [`machine/engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json) (normative sources, restart
   and retry contracts, forbidden shortcuts), [`IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md`](../docs/process/IMPLEMENTATION_READINESS_AND_TRANSACTION_CONTRACT.md),
   [`IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md`](../docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md)
   and [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md).
3. Every other scope stays unauthorized until its own ADR. In particular, `core_kernel` code must not implement
   reference-client UI, reference-provider infrastructure, group/MLS, local-delivery or later-wave features. It may define
   the traits and interfaces those scopes will implement.
4. The verification prototypes remain evidence, not product code. Product crates are written fresh against the specs and
   must replay the committed vectors (V-001, V-002, V-005 pre-release criteria) in CI.
5. Pre-release criteria and public-release gates are unchanged. Authorizing engineering is not a release decision.

## Consequences

- `engineering_authorized=true`, `phase=scoped_engineering_w0`, and `core_kernel.authorized=true` with
  `authorizing_adr=ADR-0169`.
- Documents that said "engineering is not authorized" now say which scope is authorized.
- A semantic change to a `core_kernel` normative source still needs an ADR and its own re-verification (ADR-0165).
