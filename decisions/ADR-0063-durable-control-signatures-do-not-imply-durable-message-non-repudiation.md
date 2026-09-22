# ADR-0063 — Durable control signatures do not imply durable message non-repudiation

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Control/discovery objects may be durably signed; ordinary message authenticity comes from the secure session/group protocol without adding a separate AccountID signature to each message.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
