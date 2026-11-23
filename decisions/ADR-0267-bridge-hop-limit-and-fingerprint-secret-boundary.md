# ADR-0267 — Default bridge hop limit and the fingerprint-secret boundary

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-24
- **Scope:** `bridges` only: the default `hop_limit` an exporting bridge writes, and what the V1 engine may assume about the Room-scoped fingerprint secret.
- **Authority:** delegated, under the operator's standing delegation to resolve plan questions (bridges plan Q3) from the constitution by additive ADR. It amends no audited text and lowers no floor.

## Decision

1. **Default hop limit is 3.** A bridge that starts a trace (the origin leg of an OM-native or foreign event) writes `hop_limit = 3`. This is the value the V-013 model (`verification/prototypes/v013-bridges/bridgesim`) uses. The wire allows 1 to 16 ([`bridge.cddl`](../spec/cddl/bridge.cddl)); an engine accepts any value in that range on a received trace but **never raises** a received `hop_limit` and never exceeds it, and refuses an import whose `hop_count` has reached it.
2. **The fingerprint secret is an input, never derived by the V1 engine.** The Room-scoped secret that keys `OM-INTEROP-FP-MAC-v1` is supplied by the caller. The engine never derives it from a public Room identifier, never writes it into a trace, provenance or log, and treats a missing or wrong-length secret as a refusal. Its derivation, distribution across MLS epochs and admitted bridges, and rotation are **not decided here**: they stay release-blocking items for independent cryptographic review (finding BAC-R03 of the adapter-contract review). Until that review, a Room has no continuous cross-bridge dedup guarantee across a secret change and the engine reports that honestly (the event is marked `origin_unverified` rather than merged on weaker evidence).
3. **No retry-horizon number is introduced.** Outbound operation identity across restart follows the contract's supported horizon; the engine adds none.

## Consequences

Slice 2 builds the provenance and loop engine against these two numbers and tests the boundary of each (a trace at `hop_count = 2` of 3 is imported, at 3 is refused). Changing either default is an additive ADR, not an engineering change.

## Non-goals

This ADR sets no secret schedule, no per-network trust levels and no UI wording.
