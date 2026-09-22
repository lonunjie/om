# OM Architecture Snapshot

Generated: **2026-09-23 07:56 PHT (UTC+08:00)**

## State

OM remains in **architecture-hardening / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`.

Three major pre-engineering clusters are now design-resolved:

- 06:15 — messenger baseline + Spaces/Rooms/Communities/permissions/Topics/Publications;
- 07:18 — expressive media + portable packs + entitlement boundary + typed ServicePrincipals/bots/LLMs + capability-sandboxed plugins;
- 07:56 — 0M/zero-total-control philosophy + permissionless service creation/delegated agency + external bridge provenance/E2EE boundaries + portable import/export/escape.

## New 07:56 architecture

- `0M` is a seventh canonical meaning of OM: **zero parties have total control**. It requires decentralization of ultimate authority/survivability, not the fiction that every local service must itself be peer-to-peer.
- ServiceID creation is local and permissionless. Discovery/catalogs/hosting may be centralized within their own scope but are optional and replaceable.
- OM does not infer a human-vs-AI truth bit. User-confirmed Account actions may remain Account-authored; autonomous actions are ServicePrincipal-authored; personal autonomous agents may use revocable `RepresentationGrant`s while remaining visibly service-authored.
- A-011 external interop is resolved around typed BridgePrincipals, bridge-attested foreign provenance, explicit Room plaintext export/E2EE termination, fidelity mapping and loop lineage.
- A-012 escape is resolved around public `om-archive-v1`: portable history/evidence is independent of a client/provider, but archive import never grants cryptographic authority and foreign exports never become fabricated native OM history.

## Remaining blocking design queue

- A-013 metadata threat model
- A-014 decentralized abuse/moderation
- A-015 20-year protocol longevity / anti-capture / 0M stress test

Resolved design does not imply verified implementation feasibility. Architecture verification gates remain blocking, including V-013 bridge provenance/fidelity/loop behavior and V-014 archive interoperability/import safety.

## Next architecture cluster

The next pass should attack **A-013 + A-014 together** where useful: metadata exposure and abuse controls interact strongly around public discovery, cheap service identity creation, service/bridge traffic, Room membership, reporting and provider anti-spam. Then A-015 should perform the final 20-year whole-system anti-capture/0M pass before any engineering authorization decision.
