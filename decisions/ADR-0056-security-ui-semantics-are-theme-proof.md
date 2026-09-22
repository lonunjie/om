# ADR-0056 — Security UI Semantics Are Theme-Proof

- **Status:** Accepted
- **Date:** 2026-09-22

## Decision

Themes cannot hide, remove, spoof or semantically recolor critical identity/control/transport/destructive states beyond constrained accessible stylistic tokens. External link destinations and OS/security prompts cannot be disguised by theme content.

## Rationale

A theme is untrusted presentation data. Allowing it to control security meaning would turn visual customization into a phishing/social-engineering extension surface.
