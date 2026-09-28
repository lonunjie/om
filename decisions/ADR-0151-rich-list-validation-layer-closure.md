# ADR-0151 — Rich-list validation-layer closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 8B evidence-driven clarification of ADR-0150 `om.rich-list.v1`
- **Builds on:** ADR-0149 registered-extension strictness; ADR-0150 rich-list promotion
- **Does not change:** W0 RichText/Event body grammar, valid `om.rich-list.v1` wire encodings, extension criticality, item/depth/ordinal V1 limits, or any protocol authority

## Context

Run 8B adversarial vectors exposed an ambiguity between two already accepted rules. ADR-0149 says a **known registered extension** binds its owning object, criticality and payload schema, so malformed registered payloads reject the containing object. ADR-0150 correctly intends a non-critical list presentation failure to fall back to ordinary authenticated W0 RichText, but used the phrase "any ... bound ... wrong" broadly enough to sound as if a payload that violates the registered schema should also be ignored.

Those rules need one validation boundary, not two implementation-dependent interpretations.

## Decision

`om.rich-list.v1` validation has two ordered layers.

### 1. Registered extension/schema layer — malformed object

The normal ADR-0149 registered-extension rules apply first. The containing `ConversationEvent` is rejected when the extension has a wrong owning object or criticality, non-canonical/malformed CBOR, unknown payload/item field, missing required field, wrong primitive/container type, or violates a registry-declared numeric/cardinality/enum bound. For V1 this includes:

- item array outside `1..256`;
- `block_index` outside `0..4095`;
- `depth` outside `0..3`;
- style outside the registered `unordered|ordered` enum;
- a present `ordinal` outside `1..9999`.

This is not a new rich-list-specific failure mode; it is the existing rule for a malformed **known** extension payload.

### 2. Schema-valid presentation-semantic layer — ignore enhancement

Only after the registered payload schema is valid does rich-list semantic interpretation run. If any of these checks fail, the client ignores the **entire** `om.rich-list.v1` enhancement and renders the authenticated W0 RichText unchanged:

- event kind is not `message.create` or `message.edit`;
- block references are duplicate, non-increasing, missing or do not name a paragraph;
- first/run-reset depth is nonzero or a contiguous nesting increase exceeds `+1`;
- ordered items omit `ordinal` or unordered items carry it;
- the first inline is marked or does not begin with the exact canonical fallback prefix.

No partially interpreted list survives a semantic failure. The fallback text is already part of the signed W0 event; ignoring the enhancement introduces no second text source.

## Consequences

1. Supporting implementations cannot disagree over whether a malformed known payload is an ignorable extension or a malformed containing object.
2. Benign schema-valid presentation disagreements fail soft to the same W0 text older clients render.
3. Parser/resource ceilings remain enforceable before semantic rendering; the non-critical extension label does not turn malformed known CBOR/schema into accepted input.
4. ADR-0150's sentence saying "any ... bound ... wrong" ignores the extension is superseded **only** to the extent a referenced bound is already a registered schema bound. Its reference/nesting/fallback fail-soft intent remains unchanged.
5. Run 8B V-005 evidence must cover both classes with an independent Python/Node implementation pair.
