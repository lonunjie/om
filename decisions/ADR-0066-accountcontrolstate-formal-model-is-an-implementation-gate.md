# ADR-0066 — AccountControlState formal model is an implementation gate

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

TLA+ model checking of control-device promotion, quarantine, revocation, recovery, rollback and concurrent transitions is required before production authority code is accepted.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
