# ADR-UIUX-0017 — System completeness and v1 scope candidate

**Status:** Accepted for the UI/UX design track  
**Date:** 2026-09-29

## Context

By R13B, OM had mature design contracts for shell/navigation, Omnibar, semantic components, visual system, messaging, calls, groups/Spaces, and trust/security. Remaining gaps were system-wide rather than feature-specific: settings architecture, localization/RTL, adaptive layout, content voice, sensory feedback, performance budgets, token-engineering boundaries, and release-scope control.

## Decision

1. Treat Settings as a stable, Omnibar-searchable information architecture rather than a collection of app-specific screens.
2. Internationalize before localization: externalized messages, plural/select formatting, logical directions, bidi isolation, locale formatters, pseudolocalization, and mixed-script search are engineering requirements.
3. Drive layout from available window/input capabilities using compact/split/expanded/multi-pane states. Numeric platform breakpoints are adapter references, not universal OM law.
4. Adopt a calm/direct/concrete content voice. Errors name the affected object and useful recovery; security wording names exact scope/consequence.
5. Define haptics/sounds semantically and keep them optional, short, causal, and subordinate to visual/accessibility meaning.
6. Adopt provisional measurable performance budgets. Preserve interaction/accessibility before decoration under resource pressure.
7. Harden design tokens into primitive → semantic → component alias → state layers, with protected roles and versioned migration.
8. Adopt [`V1_SCOPE_MAP.md`](../specs/V1_SCOPE_MAP.md) as a **candidate UI/UX scope for reconciliation**, not as authority over the protocol/core architecture.
9. Keep identity/logo human testing paused; do not use R13C to select a final public mark or long-form name.

## Consequences

- The UI/UX design track is now structurally complete enough for an architecture-reconciliation audit.
- Implementation can begin mapping semantic contracts to PWA/iOS/Android without waiting for a final logo.
- Optional/experimental features cannot become hidden correctness dependencies.
- Human/device testing remains required before final visual/brand/accessibility freeze.
