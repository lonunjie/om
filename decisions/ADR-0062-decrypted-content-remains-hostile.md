# ADR-0062 — Decrypted content remains hostile

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Attachments, links, themes and imported formatted content remain untrusted after E2EE decryption and pass bounded validation/sandboxed decoders before rendering.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
