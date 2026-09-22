# ADR-0052 — Adaptive Window Layout, Not Device-Specific Product Forks

- **Status:** Accepted direction
- **Date:** 2026-09-22

## Decision

OM UI adapts to available application-window size using Compact/Medium/Expanded/Wide layouts. Device-name checks do not drive core information architecture.

Initial seed breakpoints are 600/840/1200 CSS px and must be validated by prototypes, localization and text scaling.

## Consequences

The same UI naturally supports phones, foldables, tablets, split-screen windows and desktop clients while preserving user state across resizing.
