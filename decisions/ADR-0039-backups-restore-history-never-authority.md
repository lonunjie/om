# ADR-0039 — Backups restore history, never authority

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Restored security/control records are historical until reconciled with the freshest authenticated AccountControlState/head. Old backups cannot resurrect revoked devices, profiles, relays, recovery policies, or live first-contact/bootstrap transport state. In particular, request-scoped MessageRequest reply private keys, `OPEN`/`RESERVED` bootstrap state, candidate budgets and replay-only tombstones from historical backup are inert after restore; a still-needed introduction is re-issued as a fresh MessageRequest (ADR-0147). Crash recovery of the same live endpoint remains governed by the crash-consistent local-state rules and is not equivalent to restoring a historical backup.
