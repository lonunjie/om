# RelaySet lineage, sender failover and Home migration (D22)

Date: 2026-11-04. Closes D22 in [`2026-10-28-relay-status.md`](2026-10-28-relay-status.md); D1's residual closes with it.

## Design sources read

Read in full: [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) (sections 1, 2, migration and failure drills), [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), `HOME_ONBOARDING_TIERS.md`, ADR-0142 (per-entry capability and delivery key), ADR-0161 (RelaySet continuity, effective-set rule, R-211), [`spec/cddl/delivery.cddl`](../../spec/cddl/delivery.cddl) (RelaySet, RelayEntry), the requirement rows R-187 and R-211, and the V-020 and PC-A05 conformance cases.
Not read: the full Discovery Fabric prose beyond the invite and descriptor parts already used by D19 to D21, and the catastrophic-loss rendezvous text (nothing here builds it).

## What was built

- **Core** (`crates/om-core/src/relayset.rs`, `flows.rs`): the signed RelaySet record (role `device`, label `OM-RELAYSET-SIG-v1`, hash `OM-RELAYSET-HASH-v1`), a per-relationship lineage for our own and the peer's sets, the deterministic effective set (highest version, then smallest hash, among records whose signer is eligible in the newest ACS), pending children, rejection of version jumps and malformed continuity, and bounded storage. The Accept and Bind messages carry the genesis set.
- **Several routes per relationship**: when the host offers routes at more than one Home, Core adds a route at each unused Home to every Active relationship (new capability, new delivery key, new set version naming both; the old route stays). Each route has its own `SealedDeliveryKeyRecord`; no capability is reused across Homes (PC-A05).
- **Sender failover**: after two consecutive no-progress results the same inner envelope is sealed afresh for the peer's next healthy route under that route's key, with a fresh wrapper and `delivery_id`. The OM event identity never changes; the recipient deduplicates. The route left is tried last for 10 minutes.
- **Migration** (`retire_home`): add, overlap, acknowledge, drain, revoke. Retiring a Home publishes a new version without it and keeps polling the dropped route; it closes when the peer has acknowledged a version without it, or one day after the 30-day overlap if not. `emergency` closes it at once. A Home that is a relationship's only route cannot be retired (reported as `stuck`).
- **Adapter** (`packages/om-provider-http/src/homes.ts`): `createHomesProvider(offered, draining)` routes deposit, ack and poll to the Home a route names, keeps key material, invites and introductions at the active Home, offers routes at the Home Core holds the fewest for (active Home on a tie), and never offers a draining Home. `Provider.allocateRoutes(count, held)` and the wasm command `offer_homes` carry the balance.
- **Home**: unchanged. The existing per-capability queues were enough.

## Evidence

- Rust: `crates/om-core/tests/relayset.rs` (15 tests): arrival-order independence over every permutation, same-version tie by hash, pending and reconsider, version jumps, ineligible signer, forged sets, concurrent-branch convergence, blocked-Home failover with exactly-once delivery, migration with overlap and acknowledgement, the unacknowledged close, emergency revocation, restart.
- End to end (`infra/cloudflare-home/test/two-homes.e2e.test.ts`): V-020 in full (Bob's active Home blocked, Alice's mail reaches him through his second Home, once, in order, nothing doubled when the Home returns) and migration with overlap (mail sent before the change still arrives, new mail follows the new set, a blocked old Home changes nothing).
- Adapter: `packages/om-provider-http/src/homes.test.ts`.
- Two fixes found by the end-to-end run, both would have hidden the feature in the browser: `PollTarget` serialised `relay_uri` while the host read `relayUri` (every mailbox was polled at the active Home), and the offer pool refilled only the active Home, so fallback offers ran out.

## Questions the design leaves open (decided here, to confirm)

1. **How a sender learns a retired route may close.** The design says "ack" and not how. Built: an in-session `KIND_RELAYSET_ACK` carrying the hash of the set the peer now treats as effective. Ack by use was rejected: a peer can use a new route without having seen the newest version.
2. **Constants.** Overlap 30 days (the R-187 floor), acknowledgement grace one day, failover after two consecutive no-progress results, an unhealthy route is tried last for 10 minutes. Entry `priority`: lower number is preferred.
3. **Key-material pool at one Home.** A peer fetches delivery keys for every route of a set from the keys sent inside the relationship, not from the second Home; the key-material pool (one-time prekeys) still lives only at the active Home. A blocked active Home therefore stops new first contacts, not existing relationships.

## Residuals (not built)

- No server-side revocation: emergency revocation stops Core polling and publishes a set without the route, but there is no NetCall to revoke the drain capability at the Home.
- Redundant fan-out for high-reliability sends, entry `weight` and `transports` are unused.
- No catastrophic-loss rendezvous.
- No Settings screen for a second Home (the web app stores one Home URL), so a person cannot yet choose fallback Homes in the product; the adapter and Core are ready. This is a product and UI decision.
- After `retire_home` the host must stop offering that Home (pass it as `draining`), or Core will add it back.
