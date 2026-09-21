# ADR-0034 — Zero-payment Cloudflare core excludes mandatory R2

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

The personal reference core uses Worker + SQLite Durable Object. R2/D1/Pages are optional capabilities; R2 is not required because activation uses a subscription/checkout path.
