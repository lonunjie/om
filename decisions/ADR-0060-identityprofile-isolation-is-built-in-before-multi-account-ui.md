# ADR-0060 — IdentityProfile isolation is built in before multi-account UI

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT
- **Amended by:** ADR-0148 (visible multi-profile switching promoted into the V1 reference client)

## Decision

Core/storage are profile-namespaced from v1. ADR-0148 now requires the V1 reference client to expose explicit profile switching when multiple IdentityProfiles are configured, without schema/key redesign; profiles do not receive a hidden global person/device identifier.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
