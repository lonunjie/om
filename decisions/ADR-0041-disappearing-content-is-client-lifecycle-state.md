# ADR-0041 — Disappearing content is client lifecycle state

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Relays need not learn human-facing disappearing-message semantics. View-once is not backed up; expired content is never restored; attachment-key destruction is the portable retention primitive.
