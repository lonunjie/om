# ADR-0027 — Separate identity, history, and provider recovery

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

A recovery credential that authorizes AccountControlState recovery must not automatically decrypt message history or own provider credentials. Backup/history keys are separate.
