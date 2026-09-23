# ADR-0108 — Release waves and scope-granular engineering unlock

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** `HARDENING_REVIEW_20260924.md`

## Context

The single engineering lock required every blocking gate to pass, but several gates require real implementations. The lock was circular. It also coupled the 1:1 kernel to plugin/bridge proofs, and LEAN rules contradicted MESSENGER_BASELINE_V1 on V1 scope.

## Decision

V1 is a semantic contract delivered in waves W0 (1:1 kernel, reference client, reference providers), W1 (groups, portability, expressive media), W2 (calls), W3 (communities/publications), W4 (services/bots, plugins, bridges). `engineering_scopes` in the repository contract name each scope, its wave, dependencies and gates. All start `authorized: false`. A scope may be authorized only by an ADR naming it after every gate listing it, plus every always-required gate, has passed its pre-engineering criteria. Later waves never block earlier scopes or add earlier-wave wire requirements beyond frozen extension points. LEAN anti-features are read as sequencing.

## Consequences

This ADR authorizes nothing. It turns an unsatisfiable lock into a satisfiable one without weakening any gate's final, pre-release bar.
