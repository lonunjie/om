# ADR-0039 — Backups restore history, never authority

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Restored security/control records are historical until reconciled with the freshest authenticated AccountControlState/head. Old backups cannot resurrect revoked devices, profiles, relays, or recovery policies.
