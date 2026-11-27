# ADR-0272 — Service offers in a Room, and what the engineering host offers for invocation (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** `services_bots` Room state and the engineering-host connected-services adapter.
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It mirrors ADR-0270, amends no audited text, adds no wire object and lowers no floor.

## Decisions

1. **Service offers mirror bridge offers (ADR-0270).** A Room can hold a proposed service as a host-supplied offer (verified control head, requested capabilities, trigger filter, approvers reported at proposal, a bounded display name). It grants nothing. A digest review target covers the service id, installation incarnation, filter and capability set; install and remove refuse any other target (`stale_target`). Membership states are proposed, installed and removed (tombstone); revoked is reported from the held control head. A new offer replaces an earlier one and voids its target.
2. **Install and remove use the existing checks.** Installing an offer runs the existing `room.install_service` capability and human-quorum path; removal runs the existing removal path. Neither is loosened. Display text is cleaned and bounded as in ADR-0270.
3. **The engineering host does not offer invocation review in v1.** An invocation review discloses selected context to a service. The design fixes its wording but the host has no trigger source or disclosure builder yet, so the adapter reports `invocation: null` and never an invoke action. Install, revoke and remove are offered. Invocation review needs its own design and review.
4. **Everything else follows ADR-0271** (engineering-only, one adapter per captured profile, deny-all authority by default, durable operation identity reconciled by re-reading Core).
