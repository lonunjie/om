# ADR-0068 — Every theme supports light/dark auto; profile themes are optional identity-local UX

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Built-in themes have authored light/dark variants and Auto follows system preference. New IdentityProfiles may be offered distinct curated themes, but theme choices never enter public identity or security state.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
