# ADR-0042 — Explicit critical protocol extensions

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Unknown non-critical extensions may be ignored/preserved as specified; unknown critical extensions cause the affected operation to fail. Negotiation is authenticated and public registries/version lifecycle are required before wire freeze.
