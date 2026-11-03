# ADR-0185 — Home wake channel (`om.relay.wake.v1`): event-driven delivery instead of a poll timer

- **Status:** Accepted by operator delegation 2026-11-03 (operator: reduce the exchange with the delivery service to what is needed instead of constant polling, to save quota; stage the Worker change, do not deploy it)
- **Date:** 2026-11-03
- **Amends (adds one optional Home API feature; no change to any required operation or object):** [`home-api.md`](../spec/protocol/home-api.md) section 3 (optional capabilities), [`NOTIFICATIONS_AND_PUSH.md`](../docs/architecture/client/NOTIFICATIONS_AND_PUSH.md) section 5 (the "persistent path": an authenticated encrypted relay/Home wake/fetch connection)
- **Does not amend:** any wire object, crypto, key or route rule; ADR-0183 (PR-006 push/VAPID stays waived; no background-delivery claim is made); which Home is the default.
- **Scope:** `provider_reference`, `reference_client_ui`, `core_kernel` (one optional `Provider` hook). Evidence: [`2026-11-03-delivery-realignment-and-wake-channel`](../docs/reports/2026-11-03-delivery-realignment-and-wake-channel.md); specification and privacy review: [`HOME_WAKE_CHANNEL.md`](../docs/architecture/providers/HOME_WAKE_CHANNEL.md).

## Context

Every open client asked its Home for new mail on a timer, one request per mailbox per tick, whether or not anything had arrived. On the free-plan reference Home that is the dominant cost and a hard stop for everyone when it runs out. The design already names the answer for a client that is open: "a persistent path is an authenticated encrypted relay/Home wake/fetch connection" (NOTIFICATIONS_AND_PUSH section 5), and "push is a wakeup optimization, never message storage or protocol correctness".

## Decision

1. **An optional feature, `om.relay.wake.v1`, advertised in `ProviderCapabilities.features`.** A Home that omits it is polled exactly as before. Cloudflare, WebSockets and Durable Objects are the reference implementation's means; the contract is "an ordered, framed, authenticated channel that says a newer item exists" and any transport may carry it (absolute rule 18).
2. **It carries a sequence number and nothing else.** No content, item id, identity or capability in the URL. The queue is named by a routing label in a header (the 16-byte prefix every queue capability already carries; not a capability); authority is the receive capability in the client's first message. Forged, wrong-role and unknown-queue capabilities are indistinguishable.
3. **The client opens it only while the app is on screen**, only if the Home advertises it, one socket per mailbox (16 at most), and falls back to ordinary polling on any failure. While a socket vouches that nothing newer exists, polls make no request; a five-minute safety drain bounds a lost wake. Hiding the app closes the sockets and the 60 s hidden poll applies.
4. **Core stays unchanged apart from an optional `Provider.onWake` hook** that lets the runtime collect at once; a provider without it behaves as before.
5. **Web Push is evaluated and not built** ([`HOME_WAKE_CHANNEL.md`](../docs/architecture/providers/HOME_WAKE_CHANNEL.md) section 5). It would add a third party and a stable per-install identifier and needs its own ADR, a per-profile subscription and a V-015 descriptor.
6. **Staged, not deployed.** The Worker change is in the repository with tests; deploying `om-home-lonunjie` is a separate operator approval. The client is safe to ship before or after the Worker: against a Home without the feature it polls as before (checked against the live Home).
7. **Kill switch.** `OM_WAKE=0` on a deployment turns the feature off (not advertised, `404`).

## Where did authority go?

Nowhere new. Holding a socket proves possession of the receive capability, which already authorises `drain`. The Home gains no fact it did not have (it already sees every deposit and every poll). The wake cannot add, remove, reorder or alter an item; it only brings a drain forward.

## Consequences

- Idle open clients stop costing requests; a delivery reaches an open client in about a tenth of a second instead of up to a poll interval. Measured numbers are in the report.
- Cost moves from "requests per poll" to "one upgrade per connection plus a ping about every 45 s per socket (billed at a twentieth of a request)". A hidden tab still polls at 60 s; keeping a socket for hidden tabs is a possible follow-up with its own battery and privacy trade-off.
- The reference Home's queue object gains `fetch`, `webSocketMessage`, `webSocketClose` and `webSocketError` handlers and an auto-response pair. No new Durable Object class and no storage migration.
- Wake state is not persisted; a Home restart drops sockets and clients reconnect.

## Verification

Wire codecs and routing label (`packages/om-home-wire`), PC-W01..W06 against the real Worker bundle in workerd, adapter behaviour against the real Home (quiet polls, hidden, unsupported, unreachable, safety drain, foreign mailbox), hub state machine with fake sockets and timers, and a real-Core test where a one-minute poll timer still completes invite, introduction and a message in well under a second of wake-driven work. Request counts before and after with the opt-in live end-to-end spec are in the report.
