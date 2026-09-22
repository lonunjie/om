# ADR-0061 — Rich text is a bounded semantic tree, never executable markup

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 14:34 PHT

## Decision

Conversation text uses RichTextV1 semantic blocks/inlines/marks. HTML/CSS/JavaScript/Markdown are not authoritative wire formats.

## Rationale

This closes a design-phase gap before implementation makes the choice expensive to reverse. The decision follows OM's principles of scoped authority, replaceability, progressive disclosure, strong defaults and no hidden centralization.

## Consequences

- implementation and conformance tests MUST encode this boundary;
- changes require a new ADR plus migration/threat-model analysis;
- convenience layers cannot silently bypass the invariant.
