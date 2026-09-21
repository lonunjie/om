# ADR-0030 — AccountControlState and head gossip

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Security authority is a monotonic authenticated hash chain separate from chat events. Compact AccountControlHead observations by contacts/relays/witnesses detect stale/forked views without granting write authority.
