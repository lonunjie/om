# ADR-0103 — Endpoint coercion, proximate adversaries and platform leakage are first-class threats

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** [`HARDENING_REVIEW_20260924.md`](../docs/reviews/HARDENING_REVIEW_20260924.md)

## Decision

OM's threat model includes coercive adversaries (seizure, compelled unlock), proximate adversaries (intimate partners, family, brief physical access) and platform leakage (OS/cloud backups, snapshots, keyboards, cloud spellcheck, browser extensions, notification history). Device enrollment is never silent: authenticated notices appear on all other active devices (R-166). OM provides app/profile lock, quick lock, disappearing defaults, travel posture and remote suspend. OM does not promise hidden-volume/duress deniability without a research ADR (R-167). Each LocalDataClass maps to per-platform controls, verified by V-021 (R-168).

## Consequences

Some protections are platform-dependent and the PWA cannot guarantee them. The product states the difference instead of implying equivalence.
