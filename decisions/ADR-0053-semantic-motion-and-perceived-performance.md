# ADR-0053 — Semantic Motion and Perceived Performance

- **Status:** Accepted
- **Date:** 2026-09-22

## Decision

OM defines one semantic MotionSystem. Motion explains state/spatial continuity and never delays actions. Animations are interruptible; gestures track direct manipulation; optimistic local rendering is used where safe; reduced-motion has first-class mappings.

## Consequences

Smoothness is treated as an architectural/performance property rather than decorative animation. Network/provider work must not block local UI responsiveness where safe staging is possible.
