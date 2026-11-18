# ADR-0228 — Calls: per-device media secrets, ordered signalling, answer retry and watchdog, and exact TURN lease lookup

- **Status:** Accepted (by operator delegation)
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (constitution-driven rulings: fail closed, metadata minimisation, own-Home only, no operator; fill silences only, never amend audited text, never weaken a floor).
- **Class:** client and Core behavior plus one clarified Home behavior. No `CallSignal` field, CDDL, registry or domain label changes; the Home behavior is within the existing optional feature `om.media.turn.v1` (ADR-0221) and its retry rule.
- **Amends:** nothing. **Does not amend:** ADR-0218 (the frozen wire, the labels, the suite and the ring window stand), ADR-0219 (its lifecycle stands; the items below fill what it left to a later decision), ADR-0221 (its statement that an exact retry "receives the existing reservation or `expired`" is made precise), [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md).
- **Scope:** `calls` (ADR-0217).

## Design sources read, and not read

Read in full: ADR-0217, ADR-0218, ADR-0219, ADR-0221, [`call.cddl`](../spec/cddl/call.cddl), [`home-api.md`](../spec/protocol/home-api.md) section 5 (idempotency) and the constitution documents on authority, metadata and the operatorless core. Consulted by search: [`CALLS_AND_REALTIME_MEDIA.md`](../docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md), [`LIVE_CALLS_UX.md`](../docs/ux-design/specs/LIVE_CALLS_UX.md), ADR-0132. Not read in full: [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) beyond what ADR-0218 quotes, the TURN backend of any deployment.

## Decision

### 1. A call secret is drawn per callee device

ADR-0218 rule 3 draws one 256-bit `call_secret` for the call and sends it, in each offer, to each callee device through that pair's own session. The derivation of the directional base keys already names the sender and the recipient DeviceIDs, which are identifiers and not secrets. The required behavior is that **no device other than the selected one can derive the selected device's media keys**:

1. The caller draws a **fresh independent 256-bit `call_secret` for each callee device** it offers to, from the CSPRNG, and sends that device its own secret only, inside that device's own session. No two offers of one call share a secret.
2. The caller keeps the per-device secrets in live memory only, with the call, until an answer selects a device; on the first accepted answer it derives the directional keys from **that device's** secret (the answer arrives in that device's session, so the device is authenticated by the existing rule), wipes every other secret and sends `end` with `answered_elsewhere` to the others as ADR-0218 rule 9 already requires.
3. The derivation of ADR-0218 rule 3, the labels, the `key_generation` value 0, the suite and every wire byte are unchanged. A callee that receives an offer cannot tell per-device from shared secrets, so no negotiation and no downgrade exist; a peer built before this ADR interoperates.
4. **Evidence required.** A call with three callee devices: the three offers carry three distinct secrets; a non-selected device that stored its offer and knows the winner's DeviceID derives keys that fail to decrypt the winning pair's directional media (a negative key-knowledge vector next to the directional known-answer vector); an answer from a device whose secret was never offered is refused.

### 2. Call signals leave a leg one at a time

ADR-0218 rule 9 and the per-sender strict `seq` floor stay as written. A receiver applies a signal only if its `seq` is higher than the last accepted one, so a sender must never let a later signal overtake an earlier one it still needs applied.

1. **One in flight per leg.** For each (call, recipient device) Core releases the next signal for deposit only after the previous signal on that leg has a determined outcome: acknowledged by the Home, refused for good, or expired. An unknown outcome keeps the leg busy and is retried with the same sealed bytes and operation ID (ADR-0219 rule 4: an uncertain result preserves the call) until the signal's own deposit lifetime ends; the signal is then superseded and the leg is free.
2. **Coalescing.** While a signal is in flight on a leg, state-changing signals that arrive for the same leg (`media_update`) are coalesced to the latest value and released after the in-flight one; an `ice_update` and its answering `ice_update` are never coalesced away.
3. **Restart progress is owned by Core.** A call is `reconnecting` while an ICE restart exchange is open. Core allows the exchange **30 seconds**; on expiry it may start a new restart (a new `seq`, a new description) at most twice more, then ends the call with reason `failed`, wiping live keys. Receipt of a restart description never reopens an exchange already ended by a higher accepted `seq`.
4. **Evidence required.** A reordered `ice_update` and `media_update` on the Home: the restart is not discarded, or if it is, the sender's retry produces a new accepted exchange; the same call reconnects with its established keys; a sender never has two signals on one leg in flight.

### 3. The answer deposit is reconciled, and a watchdog ends a one-sided call

ADR-0219 rule 4 says an uncertain result preserves the call and waits for authenticated peer signalling or expiry. This section adds what the callee does while it waits.

1. **Exact retry.** An answer whose deposit result is unknown keeps its sealed bytes and operation ID in Core's bounded unsent set (ADR-0219 rule 4: up to 128 deposits) and is retried with identical bytes at 2, 4 and 8 seconds and then every 8 seconds, until the envelope's authenticated expiry. The Home's idempotency (Home API v1 section 5) makes a duplicate one outcome. The deposit of an answer is never re-sealed.
2. **Visibility without a verdict.** The callee's view reads "connecting, still trying to reach <name>" while the outcome is unknown; an unknown outcome is never shown as a refusal and never ends the call by itself. A refused deposit fails the leg as in ADR-0219.
3. **Core watchdog.** From a callee's accept, and from a caller's receipt of the first accepted answer, Core runs a **connecting deadline of 30 seconds** for the host's first media fact (the fact that sets `active`). On expiry Core ends the call with `failed`, sends `end` if it can, and wipes keys and descriptions. A host that takes keys and never reports media is therefore bounded by Core and not by host cleanup. The caller's 45-second ring deadline is unchanged and is cleared only by an accepted answer.
4. **Evidence required.** A lost answer response before and after durable deposit: the caller connects in the second case and rings out to `timeout` in the first while the callee ends with `failed` at the deadline, neither stuck; an adapter that never reports media ends at 30 seconds.

### 4. TURN lease lookup is exact and does not consult the meter

ADR-0221 states that an exact authenticated retry receives the existing reservation or `expired`, and an unknown result does not automatically retry with a new request ID. The required behavior, made precise:

1. **Client identity.** One `request_id` per **lease attempt**: Core (or the host) draws it once for the attempt, and every retry of that attempt at that Home reuses it. A new attempt for a later call or a later gathering draws a new one.
2. **Typed result.** The client reports `issued`, `refused` (quota, hard meter, disabled), `expired` or `unknown` (timeout, transport failure, an unreadable reply); `unknown` is never collapsed into `refused`.
3. **Lookup before meter.** The Home keeps a bounded table (at most 64 entries) of recent `request_id` values and their determined outcome (the backend's lease, or a typed refusal) for 600 seconds plus the existing envelope skew, durable across restart. A request whose `request_id` is in the table returns that outcome (or `expired` once the lease expired) **without consulting the usage meter**; only a `request_id` not in the table is gated by the hard meter. A reservation made before the meter turned hard is therefore retrievable by its exact retry and costs nothing further. The same `request_id` with a different body is `invalid_request` (Home API v1 section 5).
4. **Unknown across Homes.** On `unknown` the client retries the same `request_id` at the same Home at 1, 2 and 4 seconds, and only then moves to the next configured Home with a **new** `request_id`, accepting that the first Home may hold a reservation that expires within 600 seconds under its own hard budget (both Homes belong to the person). A relay-only profile with no usable lease refuses capture and ICE, as ADR-0221 says. A lease is never persisted, and nothing about a lease joins a call identifier to the request.
5. **Evidence required.** Loss of the first response: the exact retry returns the same lease; a retry after the meter turned hard returns the existing lease and a new request is refused with `quota`; an expired ID never creates a reservation; a stub that counts allocations sees one reservation per attempt; the data plane's byte and expiry enforcement after a restart is evidenced by the backend that claims it (ADR-0221).

## Where did authority go?

Nowhere new. Core still owns admission and call state; the session authenticates each signal; the person's Home still reserves only what its own budget allows and learns no call identifier.

## Review requirement

Section 1 changes who holds a media secret and is a cryptographic construction note that the V-007 specialist review must read; it adds no primitive, no wire byte and no weaker mode. The group-call key channel remains blocked on its own review (ADR-0218 rule 5).

## Traces

Related: ADR-0132, ADR-0185, ADR-0217, ADR-0218, ADR-0219, ADR-0220, ADR-0221.
