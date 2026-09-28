# ADR-0146 — Bootstrap candidate terminalization, bounded reply transport and expiry closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** final post-ADR-0145 hardening of MessageRequest bootstrap candidate lifecycle, reply-route transport semantics and sealed-wrapper acceptance bounds; no wire-object change and no engineering authorization
- **Amends:** ADR-0142 first-contact reply route, ADR-0145 bootstrap lifecycle/retry closure
- **Amended by:** ADR-0147 bootstrap lifetime/restore/durable-timer closure

## Context

A final adversarial pass over ADR-0145 found three remaining places where individually sensible rules could compose into an invalid state.

First, ADR-0145 allows more than one valid recipient-device candidate to hold a durable `RESERVED` replay record while a MessageRequest is `OPEN`, but only says losing candidates are terminally failed after the winner commits. If winner completion destroys the request-scoped private key before every losing `RESERVED` sibling becomes terminal, an exact retry of a losing candidate can find `RESERVED` state whose required key no longer exists. The same race exists between acceptance and request cancellation/decline/expiry.

Second, the frozen discovery CDDL called `reply_route` a “one-use deposit capability”. A physical one-deposit interpretation is incompatible with exact lost-ack transport retry and with the bounded multi-device candidate race permitted by ADR-0145. The intended scope is one logical bootstrap response, not exactly one network deposit.

Third, `SealedDeliveryEnvelope.expires_at` was authenticated but the receiver contract did not explicitly require it to fit inside the lifetime of the delivery key and bound route. An otherwise valid sender could therefore request replay/tombstone retention beyond the authority lifetime the wrapper depends on.

The repair is semantic and local. No numeric key, object type, cryptographic primitive or provider identity authority changes.

## Decisions

### 1. Every exit from `OPEN` atomically terminalizes all non-winning `RESERVED` candidates

The MessageRequest bootstrap lifecycle remains:

```text
OPEN -> COMPLETE_REPLAY_ONLY -> EXPIRED
  \-> CANCELLED / DECLINED / EXPIRED / TERMINAL_INVALID
```

Every state transition out of `OPEN` is one crash-consistent compare-and-set transaction (or equivalent write-ahead protocol) over the request and its request-scoped candidate replay set.

If a candidate wins bootstrap completion, that same commit MUST:

1. mark the winning exact wrapper `COMPLETE`;
2. install the authenticated bootstrap/imported established continuation state required by ADR-0145;
3. move the request to `COMPLETE_REPLAY_ONLY` bound to the winner; and
4. mark every other `RESERVED` candidate for that request `REJECTED` with a request-closed reason **before** the request-scoped private key may be destroyed.

If cancellation, decline, expiry or terminal invalidation wins instead, the same transition MUST terminalize every outstanding `RESERVED` candidate before destroying the active bootstrap key and closing the request.

Acceptance, cancellation, decline, expiry and terminal invalidation all race on the same `OPEN` request generation/state. Exactly one transition wins. No losing transition may reopen the request or leave a resumable `RESERVED` record that depends on destroyed key material.

### 2. `reply_route` is one-logical-response, bounded-transport-use capability

For W0 first-contact bootstrap, “one-use” means **one logical bootstrap response per MessageRequest**, not one physical relay deposit.

While the request is `OPEN`, the route/provider boundary MAY admit:

- byte-identical retries of an already admitted candidate wrapper without allocating another candidate slot; and
- at most **8 distinct candidate `(delivery_key_id, delivery_id, canonical-wrapper-fingerprint)` tuples over the lifetime of the request** across racing recipient devices.

The requester MUST enforce the same maximum even if a provider is more permissive. The ninth distinct candidate is rejected before allocating durable `RESERVED` replay state or performing unbounded expensive work. Providers MAY set a lower operational rate/storage quota, but a lower quota must still preserve exact-wrapper retry for already-admitted candidates through their acceptance window.

After the request leaves `OPEN`, no new distinct bootstrap candidate is admitted. While the request remains `OPEN`, a rejected or otherwise terminal candidate still consumes its lifetime slot: pruning or compaction MUST NOT replenish the eight-candidate budget (ADR-0147). `COMPLETE_REPLAY_ONLY` permits only the winning exact-wrapper replay behavior defined by ADR-0145; terminal non-winning records remain rejection tombstones only as long as needed by the bounded acceptance/replay policy.

This limit is an abuse/resource bound, not device authority: it does not make the first eight devices authoritative, and it does not require exposing a stable device identity to the relay. Recipient-device eligibility remains an endpoint authentication decision.

### 3. Sealed wrapper expiry cannot outlive its authority context

For every OM-Crypto-v1 `SealedDeliveryEnvelope`, endpoint acceptance requires:

```text
wrapper.expires_at <= delivery_key.expires_at
wrapper.expires_at <= bound_route.expires_at
```

when the bound route carries an expiry. For MessageRequest bootstrap, ADR-0147 defines the pending request/bootstrap acceptance deadline as `min(reply_route.expires_at, reply_delivery_key.expires_at)`; there is no separate local `request_expires` authority value in OM-Crypto-v1. `wrapper.expires_at` MUST be no later than that derived deadline.

A wrapper violating any bound is rejected before creating a new durable `RESERVED` record. A provider may retain for less time, but neither a provider nor sender may extend endpoint acceptance by choosing a later wrapper expiry.

Replay/tombstone retention is bounded by the authenticated acceptance windows plus the frozen wall-clock skew tolerance needed to decide expiry. ADR-0147 pins `COMPLETE_REPLAY_ONLY -> EXPIRED` to the derived bootstrap acceptance deadline plus skew, while idempotent acknowledgement of the winner remains limited by the winning wrapper's own expiry/skew window. Retention never extends bootstrap/delivery authority: after authority expiry, tombstones may only reject/recognize already-recorded tuples and cannot authorize a new logical message.

No universal short OM envelope lifetime is introduced here; offline-delivery policy remains possible as long as the wrapper, delivery key, route and request (when applicable) share a valid overlapping acceptance window.

### 4. V-003/V-015 must exercise the composition

Repo-local evidence MUST cover at least:

- two `RESERVED` candidates followed by one winning completion: winner `COMPLETE`, every sibling terminal, then key destruction;
- an exact retry of the losing candidate after winner completion returning terminal rejection rather than `key_missing_reserved`;
- cancellation/decline/expiry racing acceptance, with exactly one request-state transition winning and all candidates coherent after crash;
- exact retries not consuming new candidate slots;
- the eighth distinct candidate admitted and the ninth rejected before durable reservation;
- a wrapper whose expiry exceeds its delivery key, route or pending request rejected before `RESERVED`; and
- negative mutations that omit sibling terminalization, physically consume a one-use route on first deposit, remove the distinct-candidate bound, or ignore wrapper/key/route/request expiry ordering.

## Consequences

ADR-0145's single logical bootstrap response remains unchanged but is now closed under every terminal race. Destroying request-scoped key material can no longer strand a losing `RESERVED` candidate.

The historical “one-use” wording is clarified without changing `RelayEntry` bytes: the capability admits one logical bootstrap response with bounded transport retries/candidates, rather than exactly one packet/deposit. Durable resource use is finite even if an untrusted accepting peer controls many devices or repeatedly reseals candidate responses.

Authenticated wrapper expiry can no longer pin replay state beyond the authority context that made the wrapper acceptable. Provider retention may be shorter but never lengthens endpoint authority.

Engineering remains unauthorized. These semantics are part of the current independent-review surface.
