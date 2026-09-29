# ADR-UIUX-0013 — Identity exploration and platform adaptation

**Status:** Accepted  
**Date:** 2026-09-29

## Context

OM now has a stable-enough interaction language, visual token system, and functional icon family to begin identity work. Freezing a logo earlier would have risked designing around an unstable shell or confusing the brand mark with functional controls.

Current platform icon systems also require one identity to survive materially different containers: Apple appearance variants/layers, Android adaptive masks/themed monochrome icons, PWA maskable icons, favicons, and notification/status treatment.

## Decision

1. Define one **core OM mark** that is independent from the platform container.
2. Explore five provisional directions in Revision 11C using identical comparison colors.
3. Preserve the same core features across default/dark/tinted/monochrome/mask variants.
4. Treat final selection as an evidence-gated process, not a designer vote.
5. Advance One Thread, Open Loop, and Peer Link to the next blinded-human evaluation stage as three distinct hypotheses; keep Dialogue Fold and OM Fold as comparison controls.
6. Do **not** designate a final logo in Revision 11C.
7. Require legal/trademark/visual-similarity clearance before public freeze.
8. Keep wordmark typography provisional.
9. Keep the identity mark separate from functional icon semantics unless an explicit Action Registry role defines its use as a control.
10. Avoid use or imitation of the sacred `ॐ`/Om symbol; OM's name here is a product acronym/family, not a religious claim.

## Consequences

- Engineering receives platform-ready exploratory assets without being told that a brand decision is final.
- The design can be tested in real launchers/home screens before branding hardens.
- Android/PWA/Apple can each use native mask/material systems without changing identity.
- A candidate can be rejected for unintended associations, similarity risk, or small-size failure without reopening UI semantics.

## Revision 12 follow-up

ADR-UIUX-0014 preserves the three-candidate shortlist, adds blinded validation/freeze gates, and records preliminary public-web naming/collision signals. It does not select a winner.
