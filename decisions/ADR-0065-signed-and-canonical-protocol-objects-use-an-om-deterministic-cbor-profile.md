# ADR-0065 — Signed and canonical protocol objects use an OM deterministic-CBOR profile

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

OM derives a strict deterministic encoding profile from RFC 8949 with no floats/indefinite lengths/duplicate keys, bounded parsing and exact canonical-byte signatures.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
