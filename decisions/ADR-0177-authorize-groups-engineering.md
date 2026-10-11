# ADR-0177 — Authorize engineering scope `groups` (W1)

- **Status:** **Accepted** (operator, 2026-10-11). The operator accepted the ADR-0167 single-vendor review basis as is; the cross-vendor re-check stays a pre-release item.
- **Date:** 2026-10-01
- **Proposed by:** design agent (Claude, sole design agent, ADR-0167)
- **Would authorize:** engineering scope `groups` only
- **Basis:** [`2026-10-01-claude-sonnet-5-5-groups-review.md`](../verification/reviews/2026-10-01-claude-sonnet-5-5-groups-review.md), ADR-0167, ADR-0173, ADR-0174, ADR-0175, ADR-0176

## Context

`groups` (W1) depends on `core_kernel` (ADR-0169). Its three blocking gates now have their pre-engineering evidence and a review under the ADR-0167
model: V-004 (formal model, MLS ordering, wire freeze), V-009 (second materializer, 266 cases) and V-019 (OM-level authenticated migration).
The `group_history` conformance suite is specified (ADR-0176), no design-queue item is open, and the scope contract's integration prerequisite
(frozen W1 CDDL, registries, two-codec vectors and a selected MLS runtime) is met: OpenMLS 0.9.0 passes the V-004 probe on native and
wasm32-unknown-unknown.

## Decision

1. `groups` is authorized: MLS rooms, GroupControlState, group commit ordering and convergence, GroupHistoryBundle, and the OM-level hybrid-PQ migration path.
   Engineers follow [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md), the `groups` entry of
   [`engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json), ADR-0173 (order, recovery), ADR-0174 (materialization), ADR-0175 (migration)
   and ADR-0176 (wire and admission rules), and conform to the frozen vectors.
2. Pre-release items stay open and block any public or beta release of groups, not the engineering: the V-004 multi-client partition harness on two
   implementations, the V-009 corpus run by every conforming implementation, and the V-019 interoperable migration once the ADR-0107 triggers are met.
3. Open design points the engineers must not decide on their own: the migration transition wire (ADR-0175 rule 5), a larger-envelope profile for rooms
   beyond about 500 members or histories beyond one bundle (ADR-0176 F-27), and the recomputation of `reducing` for the state-dependent operations (T-161).
4. No surface claims post-quantum confidentiality for v1 groups, and the PQ suite remains a draft code point (ADR-0175).

## Applied on acceptance

- [`machine/repository_contract.json`](../machine/repository_contract.json): `engineering_scopes.groups.authorized = true` and `authorizing_adr = "ADR-0177"`.
- V-004, V-009 and V-019 must show `pre_engineering_status = passed` (the validator refuses otherwise), and the status line of this ADR becomes Accepted.
- [`AGENTS.md`](../AGENTS.md), [`SNAPSHOT.md`](../SNAPSHOT.md), [`ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md) and the unlock table name the fourth authorized scope.

## Consequences

- Review basis: the ADR-0167 single-vendor model. The evidence executor and the reviewer were one agent session (disclosed in the review). The operator may prefer a fresh-session or cross-vendor re-check of the three gates before accepting.
