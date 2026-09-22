# ADR-0057 — Accessibility Is a Shared UI Release Gate

- **Status:** Accepted
- **Date:** 2026-09-22

## Decision

Shared UI primitives target WCAG 2.2 AA or stronger, full keyboard use, screen-reader semantics, visible focus, text scaling/reflow, reduced motion, RTL readiness and non-color state communication. Accessibility regressions in foundational primitives block release.

Primary touch controls target at least 44×44 CSS px where practical, exceeding the WCAG 2.2 AA minimum-target baseline.
