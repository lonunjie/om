# ADR-0059 — ClientHost and DeviceVault are separate security boundaries

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

The PWA origin/ClientHost is distinct from IdentityHome and local secrets live behind a versioned DeviceVault. Provider/origin migration is a new-device transfer, never raw browser-database migration.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
