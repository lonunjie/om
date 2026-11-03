# Home wake channel (`om.relay.wake.v1`)

- **Status:** Optional Home API feature; implemented in the reference Home and adapter (ADR-0185)
- **Scope:** `provider_reference`, `reference_client_ui`
- **Why it exists:** a client that asks its Home for new mail on a timer costs the Home a request per ask, whether or not anything arrived. The channel lets the Home tell a connected client that something newer exists, so the client asks once, when it matters. See [`2026-11-03-delivery-realignment-and-wake-channel`](../../reports/2026-11-03-delivery-realignment-and-wake-channel.md) for the measurements.

## 1. What it is, and what it is not

**Audited basis.** The Cloudflare reference profile lists "WebSocket hibernation for connected clients" ([`RELAY_AND_DELIVERY_ARCHITECTURE.md`](../delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md), "Cloudflare reference implementation"), and the Home API allows "additional endpoints only as advertised optional capabilities" ([`home-api.md`](../../../spec/protocol/home-api.md) section 3). This document specifies that optional capability; it adds no protocol authority.

An **optional** feature, advertised in `ProviderCapabilities.features` as `om.relay.wake.v1`. A Home that does not advertise it is polled with the ordinary `drain` operation, exactly as before. A client that does not know it never notices it. It is not a protocol dependency of any kind: no conversation, key, route or cryptographic state depends on it, and a lost or never-sent wake costs time only.

It carries **no message, no content, no item id, no sender or recipient identity and no capability in its URL**. A wake frame is a sequence number. The client then drains with the receive capability as always; delivery, acknowledgement, replay handling and expiry are untouched.

Where did authority go? Nowhere new. The channel confers no authority: it is opened and kept alive by whoever already holds the queue's receive capability, and the Home already knows when each deposit arrives.

## 2. Binding (reference: WebSocket)

Any transport with ordered, framed messages fits; the reference binding is a WebSocket.

| Step | Detail |
|---|---|
| Open | `GET /v1/relay/wake` with `Upgrade: websocket` and the subprotocols `om.relay.wake.v1` and `om.q.<32 hex>`. The URL is the bare path: no query, no fragment, no id. The Home answers with the subprotocol `om.relay.wake.v1` only. |
| Routing label | `om.q.<hex>` is the 16-byte routing prefix every capability for that queue begins with (home-api queue capabilities carry it so a Home can find the queue). It names the queue; it is **not** a capability and grants nothing. It travels in a header, not in the URL. |
| Authenticate | The client's first message is binary `WakeAuth {0: api_major, 1: receive_cap}` (CBOR, strict). Anything else first, or after, ends the session (`4400`). A forged capability, the deposit or admin capability, a revoked queue and a queue that does not exist all end it the same way (`4401`): no existence oracle. |
| Ready | The Home answers `WakeEvent {1: ready, 2: head_seq}`: the newest sequence stored (0 when empty). |
| Wake | Whenever a new item is stored (an exact retry of a stored deposit is not new), every authenticated socket of that queue receives `WakeEvent {1: wake, 2: seq}`. |
| Keepalive | A text frame `ping` is answered `pong` by the runtime without waking the queue. The client pings while the app is on screen and treats a missing pong as a dead connection. |
| End | Close codes (private range): `4400` bad frame, `4401` unauthorized, `4408` auth too late (`10` s), `4409` superseded (the receive capability was rotated; reconnect), `4429` too many sockets. |

Bounds: a queue holds at most 8 sockets (advertised as `limits.wake_sockets_per_queue`). When full, the oldest socket that has not authenticated is dropped to make room for the owner; if every socket is authenticated the newcomer is refused with `429`. Frames in either direction are at most 256 bytes.

A deployment can turn the feature off with `OM_WAKE=0`: it is then neither advertised nor served (`404`), and clients poll.

## 3. Client behaviour (reference adapter)

1. **Only while the app is on screen.** The host calls `setWakeActive(true)` when the page is visible and `false` when it is hidden; hiding closes every socket. Hidden, the slow poll (60 s) applies as before.
2. **Only when advertised.** The adapter reads the Home's capabilities once and opens sockets only if the feature is listed. The check is repeated at most every ten minutes after a failure.
3. **One socket per mailbox, at most 16.** Mailboxes beyond the cap keep polling.
4. **Quiet polls.** While a socket for a mailbox is live and nothing newer than what the client holds has been announced, the runtime's timer still ticks but `poll` answers locally with nothing, and no request is made. A wake, or a safety interval (5 minutes) with no real drain, lets the next poll go to the Home. A wake also asks the Core runtime to collect at once (`Provider.onWake`), so delivery does not wait for the timer.
5. **Failure is polling.** A socket that cannot open, times out before `ready` (10 s), drops, misses a pong, or is closed by the Home leaves the mailbox on ordinary polling; reconnects back off (2 s to 60 s) and, after repeated failure, rest for ten minutes. A refused capability is not retried.
6. **No new state.** Sockets are not persisted; the receive capability is derived from the install secret exactly as for `drain`.

## 4. Privacy review

What the channel adds to what a Home operator already sees (threat model [`2026-10-28-relay-threat-model`](../../reports/2026-10-28-relay-threat-model.md) section 1):

| Observation | With polling | With the wake channel | Net |
|---|---|---|---|
| That a network address watches a queue, and when | every poll (3 s, 15 s or 60 s cadence) | connection open to close, pings about every 45 s | the same class of information, coarser |
| Whether the app is on screen | inferred from the poll pace | inferred from the connection existing | the same |
| When a deposit arrives | known | known, and the connected recipient is told at once | no new fact; the Home chooses nothing new |
| Which queues belong to one client | queues drained from one address in one session | sockets from one address at one time | the same |
| Message content, sender, AccountID | never | never | none |

Residual risks, recorded: (1) a hostile Home could send spurious wakes to make a client drain (a bounded cost the client already pays on its timer); (2) it could withhold wakes, which the safety poll and the hidden-state poll bound; (3) a stranger who knows a queue's routing label can open up to 8 unauthenticated sockets to crowd a queue, which the eviction rule (the owner displaces an unauthenticated socket) turns into a nuisance, not a block; (4) the open socket is a stable per-session connection a network observer can see (it already sees every poll), so a privacy transport, where used, applies to it as to `drain`. `wss:` is already permitted by the web client's content security policy; nothing new is allowed.

## 5. Web Push for a closed installed app: evaluated, not built

Web Push would wake a service worker when the app is closed. It is **not** shipped, for these reasons, and it should not be without a privacy review and an ADR (PR-006 is waived for the beta only because the product makes no background-delivery claim, ADR-0183).

- **A third party sits on the path.** The browser's push service (Google, Mozilla or Apple, by browser) receives every push for a subscription endpoint. It learns the endpoint's wake timing, and the Home learns the endpoint, a stable per-install identifier it does not have today. [`NOTIFICATIONS_AND_PUSH.md`](../client/NOTIFICATIONS_AND_PUSH.md) already requires a distinct subscription per identity profile (distinct service-worker scope), so two identities on one device are not linked by a shared endpoint; the web client does not do that yet.
- **The Home would need a push identity.** Web Push requires VAPID keys. Whoever operates the Home holds the signing key the push service sees; with a SeedLife-run default Home that is a SeedLife identity on every subscription. A per-user Home (T2) is the design's intended owner.
- **Content must stay out.** Payloads would be empty (a timing signal only, TTL-limited), never sender, text, conversation or AccountID (rule 21:32, NOTIFICATIONS 2). Browsers (iOS in particular) require a user-visible notification for each push, so the notice would be a generic "New OM message", shown even when the vault is locked, which is itself a metadata disclosure to anyone who sees the screen (NotificationPolicy must default to generic).
- **Locked vault.** The service worker cannot open Core while the vault is locked, so it could show only the generic notice and could not fetch or decrypt anything; the stored receive capability derivation (the install secret) is not vault-protected today, so a push-time fetch would widen what a locked device exposes. Do not add one.
- **Platform reality.** iOS delivers Web Push only to an installed home-screen app; background delivery is best effort. The matrix in PR-006 is the gate, and it is waived, not met.

If built later, it is a separate optional feature (`om.relay.push.v1`: register an opaque subscription for a queue, per-profile, revocable, expiring), a user-visible opt-in with a plain disclosure, and a declared linkage point in the Home's MetadataExposureDescriptor. It would not replace this channel: while the app is open the socket is cheaper and has no third party.

## 6. Cost model on the reference Home

Facts below are from memory of Cloudflare's published pricing and must be checked on the day (AGENTS.md "External facts"). A WebSocket upgrade is one Worker request and one Durable Object request. An incoming WebSocket message is billed at 20 to 1 as a request, so a ping costs a twentieth of a request; messages the Home sends are not requests. A Durable Object using the hibernation API is not billed for duration while idle, so an idle queue with open sockets costs only its pings. A deposit costs what it always did (one Worker plus one Durable Object request) and now also wakes any listener for free.

## 7. Conformance

PC-W cases, run against the real Worker bundle in workerd (`infra/cloudflare-home/test/wake.test.ts`, `wake-adapter.test.ts`, `wake-runtime.test.ts`) and for the codecs in `packages/om-home-wire`:

| ID | Case |
|---|---|
| PC-W01 | advertised with its limits when on; absent and `404` when off |
| PC-W02 | a request without the upgrade, the feature id, or exactly one routing label is a typed `invalid_request`; the socket address is the bare path |
| PC-W03 | the receive capability authenticates (`ready` with the head sequence); forged, deposit, admin, truncated and non-existent queue all close `4401` identically; a bad first frame or a second frame closes `4400` |
| PC-W04 | a stored item wakes every authenticated socket of that queue with its sequence only; not other queues, not unauthenticated sockets; not an exact retry, a forged capability or a malformed deposit; sequences increase; rotation closes `4409` |
| PC-W05 | `ping` is answered `pong`; a queue's socket count is bounded and the owner displaces an unauthenticated socket; with every socket authenticated a newcomer is refused |
| PC-W06 | the channel sends one small frame per item and never an item, id or capability |
| adapter | polls make no request while a live socket vouches the queue is quiet; a wake collects the item; hidden or unsupported, refused, unreachable or webSocket-less operation is plain polling; a safety drain still happens |
| runtime | with a one-minute poll timer, invite, introduction and a message complete in under a second of wake-driven work; the same flow against a Home without the feature does not in the same time |
