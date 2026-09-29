# ADR-UIUX-0006 — Semantic visual system and protected theme contract

**Status:** Accepted

## Context

OM now has a stable-enough shell, Omnibar model, action grammar, semantic motion model, and personalization philosophy to begin defining visual rules. Freezing a permanent brand palette or bespoke typography before real-device prototypes would be premature, while leaving visual behavior entirely unspecified would make platform implementations drift.

## Decision

OM will define a platform-neutral semantic visual system before freezing brand styling.

The system uses:

- semantic design tokens;
- a 4-unit spacing grid;
- semantic typography roles;
- protected accessibility/contrast/focus rules;
- semantic interaction states;
- bounded motion classes;
- declarative, versioned OwnMessenger theme tokens.

Default typography uses platform system UI fonts unless/until a dedicated OM family demonstrates a meaningful benefit.

OwnMessenger themes may alter approved presentation tokens but cannot alter canonical action meaning, safety policy, required focus/status visibility, or input/data permissions.

## Consequences

- iOS, Android, PWA, and future clients can share a recognizable design language without pixel-identical rendering.
- visual branding can evolve without rewriting component semantics;
- third-party/personal themes are safer and more portable;
- accessibility and security state remain protected from decorative overrides;
- initial prototypes can test interaction clarity before committing to permanent brand artwork.
