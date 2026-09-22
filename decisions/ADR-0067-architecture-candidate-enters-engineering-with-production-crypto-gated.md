# ADR-0067 — Architecture candidate enters engineering with production crypto gated

- **Status:** Superseded by ADR-0069
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Engineering begins with a dev-only CryptoProvider where needed; no prototype is called production secure until the dedicated production-crypto integration gate, vectors, fuzzing and review requirements pass.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
