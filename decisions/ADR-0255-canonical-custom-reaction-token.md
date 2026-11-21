# ADR-0255 — Canonical custom-reaction token

- **Status:** Accepted by delegation (2026-11-21).
- **Date:** 2026-11-21
- **Scope:** Sender-side construction for `expressive_media` under ADR-0250. The frozen W0 and W1 receive rules remain in force.

## Design sources read

The [constitution digest](../docs/constitution/CONSTITUTION_DIGEST.md), ADR-0250 through ADR-0254, the [expressive-media architecture](../docs/architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), the [event model](../docs/architecture/events/EVENT_MODEL.md#reactionset), and the [frozen expressive CDDL](../spec/cddl/expressive.cddl) supply the decision below. The constitution originals were covered through the digest.

## Decision

1. For a new custom reaction, the canonical token is the asset's explicit `fallback_text` (key 14), copied byte for byte into `ReactionSet` key 1. It must fit the existing reaction-key rules: 1–64 UTF-8 bytes, at least one non-whitespace scalar value, and no Unicode control character (general category Cc). Whitespace here is Unicode `White_Space`. There is no trimming, normalization, case folding, or use of `alt_text` as a token. For example, `👍` maps to `👍`, and `:cheer:` maps to `:cheer:`.
2. Core offers the token for eligible picker items and checks it again against the prepared asset when admitting the send. The eligible kinds are `reaction_asset` and `custom_emoji`. A missing, invalid, or mismatched token refuses the custom send before queueing or consuming the prepared asset. The picker offers only items for which Core returned a token.
3. The existing authenticated capability check selects either the W1 add with key 3 or a plain W0 reaction with the same key 1. Both address the existing register keyed by actor, target, and token. An asset-free removal names that exact key. Assets and ordinary reactions that use identical token bytes share a register for the same actor and target. The token names the textual reaction, while the digest-bound blob supplies its custom rendering.
4. Receivers continue to interpret wire-valid legacy tokens as sent, including tokens that differ from the asset's fallback. Already admitted operations keep their saved token, so retries and subsequent removals use the same key. No CDDL, registry, capability, cryptographic floor, or default changes.

## Constitutional basis

C-04 and C-28 keep the token derivable without a pack service or registry lookup. C-20, C-23, C-43, and C-50 preserve the negotiated capability and frozen wire interpretation. C-26 and C-46 record the formerly open sender construction explicitly. Authority stays with the authenticated actor for that actor's reaction register and with the digest-bound blob for rendering.
