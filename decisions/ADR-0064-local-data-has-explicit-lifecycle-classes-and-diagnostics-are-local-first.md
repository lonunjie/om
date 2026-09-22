# ADR-0064 — Local data has explicit lifecycle classes and diagnostics are local-first

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Secret/Persistent/Recoverable/Ephemeral/DerivedCache/ViewOnce/Diagnostic data have explicit persistence/deletion rules; default diagnostics stay local and redacted.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
