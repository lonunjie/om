# ADR-0032 — workers.dev is the v1 personal discovery locator

- **Status:** Accepted
- **Checkpoint:** 2026-09-21 21:32 PHT

## Decision

Personal Cloudflare-backed deployments can publish signed OM discovery at their Worker FQDN without buying a domain. The FQDN is a locator/alias, never AccountID.
