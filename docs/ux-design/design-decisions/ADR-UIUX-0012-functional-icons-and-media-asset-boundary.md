# ADR-UIUX-0012 — Functional Icons and Media Asset Boundary

**Status:** Accepted for Run 11B.

## Context

Run 11A defined icon semantics before artwork. Run 11B needs actual functional SVGs and a media-pack architecture without conflating branding, executable plugins, or expression with core interaction semantics.

## Decision

1. Produce an OM-authored, monochrome, 24-unit functional icon candidate family keyed by the semantic inventory.
2. Keep accessible naming in components/contracts rather than relying on embedded SVG titles.
3. Integrate those icons into the PWA reference before brand/logo selection.
4. Treat invite QR, fallback avatars, illustrations, themes, stickers, effects, and sounds as media assets governed by declarative manifests.
5. Media packs are data-only: no script, network access, external references, or action remapping.
6. QR rendering prioritizes scan reliability over theme styling.
7. Generated fallback avatars are abstract and deterministic from safe opaque seeds; they never infer human traits.
8. Final OM identity/app-mark work remains deferred to Run 11C.

## Consequences

The design system now has real testable icons without prematurely freezing the brand. OwnMessenger/OhMessenger can grow an asset ecosystem while retaining the core privacy/security boundary.
