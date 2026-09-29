# ADR-UIUX-0009 — Semantic Component Contracts, Native Accessibility Mapping, and Evidence-Gated Ranking

**Status:** Accepted  
**Date:** 2026-09-28

## Context

The UI/UX design had reached a point where more mockup-level detail risked drifting away from implementation semantics. OM also needed a concrete way to test whether a more sophisticated personalized Omnibar ranker was actually worth its complexity.

## Decision

1. Define `om-ui` components as framework-independent semantic state machines before binding them to React, SwiftUI, Compose, or future frameworks.
2. Map those semantics into native accessibility mechanisms on each platform rather than inventing one cross-platform accessibility abstraction that erases platform strengths.
3. Treat gestures as accelerators and expose equivalent named accessibility actions.
4. Use native HTML semantics first on the web; use ARIA patterns only where native semantics do not express the widget.
5. Model Omnibar candidates as one executable candidate per selectable result so entity/verb semantics remain accessible and predictable.
6. Keep the transparent local recency/frequency/context ranker as the baseline.
7. Keep Bayesian smoothing experimental until replay, device performance, and human testing demonstrate material benefit.
8. Preserve prediction stability as an explicit metric alongside accuracy.

## Consequences

- UI implementations can differ visually while sharing testable semantic contracts.
- Accessibility regressions can be caught at the component/action boundary.
- OM avoids coupling its UX to one framework.
- Omnibar learning can evolve without becoming a hidden AI authority.
- The 1M-message stress case now has measured evidence that the naive full-text path requires optimization before the provisional latency target can be considered met.
