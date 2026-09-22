# ADR-0060 — IdentityProfile isolation is built in before multi-account UI

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Core/storage are profile-namespaced from v1. Multiple-account switching can ship later without schema/key redesign; profiles do not receive a hidden global person/device identifier.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
