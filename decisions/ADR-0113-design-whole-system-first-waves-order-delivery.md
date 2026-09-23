# ADR-0113 — Design the whole system first; waves only order delivery

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator ("it is better to design the whole thing now and research if it is feasible than to bolt on a constructed layer")
- **Amends:** ADR-0108

## Decision

W0..W4 stay the delivery order. The architecture is designed for all waves now. Any later-wave gate whose outcome
could change W0 wire formats, canonical encoding, authority objects or principal types declares
`interface_impact`. A W0 scope cannot be authorized until each such gate has `feasibility_status: evidenced`, meaning a
recorded bounded model or prototype shows the W0 interfaces carry the later feature unchanged
([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](../docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) §7). Pre-release criteria of later waves still never block W0.

## Consequences

Feasibility work for groups, permissions, services, bridges, archives and PQ group migration moves ahead of W0 unlock.
That costs time before the first line of product code and saves a wire-format break later. The validator enforces it.
