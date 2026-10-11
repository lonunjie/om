# ADR-UIUX-0008 — Validation gates and protected default experience

**Status:** Accepted

## Context

OM now has enough shell, Omnibar, motion, personalization, and visual-system definition that further unconstrained design expansion risks producing attractive but untested complexity. OwnMessenger also creates a risk that customization could weaken clarity or safety.

## Decision

1. Treat the UI/UX constitution as **testable contracts**, not aspirational prose.
2. Adopt [`VALIDATION_GATES.md`](../specs/VALIDATION_GATES.md) as the pre-freeze acceptance criteria for the default experience and built-in preset themes.
3. Harden core components through a common state contract ([`COMPONENT_BEHAVIOR_AND_STATES.md`](../specs/COMPONENT_BEHAVIOR_AND_STATES.md)).
4. Keep **OM Calm** as the reference default candidate while validation runs; do not churn the base palette without evidence.
5. Permit OwnMessenger themes to vary presentation only inside the protected Theme Contract.
6. Prefer ergonomic margins above minimum accessibility conformance for high-frequency messaging controls.
7. Do not claim real-user or real-device validation until it has actually been performed and recorded.

## Consequences

- Future UI decisions must identify which validation gates they affect.
- A visually attractive preset can be rejected if it fails semantic transfer, contrast, focus, targetability, reduced-motion, or comprehension tests.
- Accessibility is part of default product quality rather than a separate skin.
- The next engineering prototype has a finite test matrix instead of an open-ended visual brief.
