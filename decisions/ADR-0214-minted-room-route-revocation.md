# ADR-0214 — Mint and revoke per-Room deposit capabilities

- **Status:** Accepted by delegation
- **Date:** 2026-11-18
- **Decided by:** operator delegation on 2026-11-18 for D117.
- **Scope:** `groups` and `provider_reference` implementation of existing route semantics.
- **Amends:** Nothing. ADR-0189 section 1.3, the Home API, the sealed-delivery profile, and the metadata floor govern this decision.

## Design sources read

Read in full: [ADR-0189](ADR-0189-group-carriage-own-commit-acceptance-and-room-defaults.md), [ADR-0198](ADR-0198-group-application-messages-typed-events.md) and [Relay and Delivery Architecture](../docs/architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md). Read in part: D117 of the [relay status](../docs/reports/2026-10-28-relay-status.md), sections 2 and 3 of the [Home API](../spec/protocol/home-api.md), the mint bodies in [Home API CDDL](../spec/cddl/home-api.cddl), [ADR-0142](ADR-0142-sealed-delivery-v1-for-established-one-to-one.md) and the delivery and provider-resource sections of the [Metadata Threat Model](../docs/threat-model/METADATA_THREAT_MODEL.md). The implementation handoff records the code-path design gate and backend conformance evidence.

## Context

ADR-0189 requires a separate minted deposit capability for each device's route in each Room. The earlier HTTP adapter used a fresh mailbox's default deposit capability. Those legacy routes lack the mint record ID required for targeted Home revocation.

## Decision

1. Use the Home API's existing mint and revoke operations for new Room routes. The resource-scoped `relay_admin_cap` authorizes both operations at the Home that owns the mailbox. Mint with an opaque request ID and label, and a bounded lifetime. The signed route carries the opaque deposit capability and an expiry no later than the Home's expiry. Each capability remains specific to one Room, one receiving device and one Home. The route binding and fresh sealed-delivery keys continue to cover the exact route.
2. Persist a pending mint's exact request before sending it. Persist the Home, mailbox, returned ID, capability and expiry before offering the route to Core. Keep the ID and any local revocation metadata out of the signed record. Resume the same logical mint after an ambiguous response, subject to the Home's bounded replay window. An unresolved grant must never be published by guessing its ID or substituting the mailbox's default capability.
3. Preserve the existing sequence after a removal. Prepare a new route, commit it with the update debt for the remaining roster, then retire the previous capability after the existing bounded overlap. Persist revocation debt before dropping the local route entry. Retry `DELETE /v1/relay/deposit-capabilities/{id}` at the owning Home after restart, and complete the debt on success, success-equivalent not-found or expiry of the Home-issued capability under its lifetime contract. The path uses the opaque record ID. A timeout leaves revocation outstanding. Local receive eligibility ends at the overlap boundary independently of HTTP progress.
4. Old offers and routes have no mint ID. Retain their existing signed expiry and authentication rules, and replace them through the usual authenticated route update before relying on mint-based revocation. Expose only supported routes for new allocations. A Home without a working mint and revoke path cannot supply a new route that promises targeted revocation. Keep 1:1 allocation and the frozen peer-facing route format intact.

## Queue isolation

Each receiving device uses a dedicated queue for each Room. A replacement uses a fresh queue so its predecessor can drain during the bounded overlap. Queue prefixes remain distinct across Rooms. The existing queue and route limits apply. Relative polling times and other traffic metadata remain observable under the metadata threat model.

Both reference Homes share the existing mint and revoke handlers. The HTTP adapter keeps the pending mint and its result in the profile's encrypted store until handover. Core owns retirement and may stop using the old receive key independently of the Home response. The receive entry retains the grant ID for retry until DELETE succeeds or the grant expires.

## Where did authority go?

The receiving device chooses and authenticates its Room routes. The Home administers only its own opaque queue capability. Neither step grants Room membership, approves a Commit, changes an AccountID or makes a provider an identity authority.

## Verification

Add failing integration cases for a pre-removal capability that still admits deposits after overlap, the replacement remaining usable, wrong-Home and wrong-resource authorization, a lost mint response, and a crash or partial failure between retirement and DELETE. Exercise multiple Rooms and Homes to check the metadata boundary, legacy routes without a grant ID, and identical behavior across supported Home implementations. The removed device must learn no replacement capability. Verify the existing sealed-delivery and no-fallback cases and the WASM size budget.
