# ADR-0072 — Asynchronous key material is a first-class replaceable provider capability

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM explicitly models per-device asynchronous handshake material and MLS KeyPackage publication/fetch/depletion/rotation. Consumable one-time material defaults to disjoint pools per independent provider rather than mirroring the same consumable item.

## Rationale

PQXDH-class asynchronous initiation and MLS joining require offline-published key material. Multi-provider OM must define consumption semantics instead of inheriting a centralized server assumption.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
