# ADR-0036 — Software update trust is designed from the first public release

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

OM uses signed/mirrored source identity, TUF-style compartmentalized update roles, pinned dependencies, build provenance, and reproducibility where feasible. Expensive audits can come later; trust-root shape cannot.

## 2026-09-28 ADR-0148 amendment

The first-public-release metadata/verifier boundary is also algorithm-agile: owner-pinned root policy can carry and later require multiple algorithm-tagged signatures without a metadata-format migration. The reference distribution is ML-DSA-65 co-signature-ready, but PQ release signatures become mandatory only through an authenticated root-policy transition; distribution signing keys never become OM protocol identity.
