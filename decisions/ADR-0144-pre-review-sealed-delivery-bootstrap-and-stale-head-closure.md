# ADR-0144 — Pre-review sealed-delivery bootstrap and stale-head semantic closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** final design-hardening before different-vendor `core_kernel` review; no engineering authorization
- **Amends:** ADR-0142 §8 first-contact bootstrap, ADR-0143 inner-outcome handling, V-003 one-to-one session gate
- **Amended by:** ADR-0145 single-response/completed-retry closure; ADR-0147 bootstrap lifetime/restore closure

## Context

The pre-review audit found two places where the accepted design was stronger in intent than in its machine-checkable contract.

First, ADR-0142 says the `MessageRequestPayload.reply_delivery_key` is generated for one request and bound to `reply_route`, but the canonical requirement did not spell out the complete local-state binding, freshness, expiry and disposal rules needed to prevent a bootstrap key from becoming a cross-request capability or accidental relationship credential.

Second, ADR-0143 permits an inner-authentication failure to move a sealed-delivery replay record from `RESERVED` to `REJECTED`. The one-to-one session rules separately require a possibly stale trusted peer AccountControlState head to be refreshed before a new initiator is accepted. Without an explicit composition rule, an implementation could mistake “authentication is not yet decidable because the trusted head may be stale” for a terminal authentication failure, mark the exact wrapper `REJECTED`, and permanently burn a valid delivery before the required ACS refresh completes.

Neither issue requires a new wire field, provider service or cryptographic primitive. They require tighter endpoint-local state and gate semantics.

## Decisions

### 1. The MessageRequest reply-delivery key is fresh and bound to one pending request

For every newly created `MessageRequestPayload`, the requester generates a fresh `DeliveryKeyMaterial` key pair and fresh `delivery_key_id`. A requester MUST NOT intentionally reuse either the private key or `delivery_key_id` in another MessageRequest, relationship or ordinary established-delivery record.

The requester's durable pending-request state binds the complete tuple:

```text
(ticket_id,
 request_key_digest,
 crypto_profile,
 reply_route,
 reply_delivery_key)
```

The `reply_delivery_key.route_binding` MUST equal the normal `OM-SEALED-DELIVERY-ROUTE-v1` digest of that exact `reply_route`'s `relay_uri` and deposit capability. The reply key's `crypto_profile` MUST equal the MessageRequest profile, and `reply_delivery_key.expires_at` MUST NOT exceed `reply_route.expires_at`. ADR-0147 defines the request's bootstrap acceptance deadline as the authenticated minimum of those two expiries; an implementation may cache that derived value but OM-Crypto-v1 has no independent hidden request-expiry input.

A recipient accepts the bootstrap key only as data authenticated by the signed `MessageRequestPayload` for that request. It cannot transplant the key into another request, route or relationship context and thereby gain any stable identity, AsyncKeyBundle-fetch, account-control, recovery or ordinary relationship-delivery authority.

### 2. Bootstrap use is narrowly lifecycle-scoped

The request-scoped reply key is valid only for the accepting recipient's first post-acceptance bootstrap/control response associated with that exact pending request. Same-route transport retry may reuse the exact same sealed wrapper/id under the normal crash-safe replay rules; a new logical bootstrap response requires a new wrapper/id while the request remains eligible.

The requester retires the private bootstrap key and the recipient retires any cached public bootstrap material when the earliest applicable terminal condition occurs:

- the request is declined, cancelled or expires;
- the accepted bootstrap exchange is durably complete and established relationship delivery-key state has taken over; or
- the relationship/request is otherwise terminally invalidated.

Retention solely to finish an already accepted exact-wrapper retry may continue only through the bounded acceptance/replay overlap. The bootstrap key is never promoted into a `SealedDeliveryKeyRecord` and never becomes stable relationship state.

### 3. Stale trusted ACS is a deferred authentication outcome, not terminal rejection

After a sealed wrapper has entered `RESERVED`, inner processing still follows the ordinary `RelationshipContext` and K12 rules. If inner authentication cannot yet be decided solely because the locally trusted peer ACS head may be stale, or because the authenticated monotonic refresh required by K7/K12 is temporarily unavailable, the endpoint:

1. keeps the exact wrapper replay record in `RESERVED`;
2. releases no plaintext/event and sends no successful transport acknowledgement;
3. performs or awaits authenticated monotonic peer-head refresh, never accepting an older head; and
4. resumes the byte-identical wrapper only after the relationship state is sufficient to decide authentication.

`REJECTED` is reserved for a definitive failure: malformed/cryptographically invalid inner content, or authentication/authorization failure established against sufficiently current validated relationship state. Temporary absence of the evidence needed to make that decision is not itself a definitive authentication failure.

If refresh later proves the claimed initiator/device ineligible, the same exact wrapper may transition `RESERVED -> REJECTED`. If refresh validates it, normal inner acceptance plus replay `COMPLETE` still commit crash-consistently before application release/ack as required by ADR-0143.

This does not weaken replay safety: another wrapper using the same `(delivery_key_id, delivery_id)` remains a collision and is rejected while the original is `RESERVED`.

### 4. Security-invariant and threat traceability must name the crash and bootstrap boundaries

The structured threat/invariant mapping must explicitly cover:

- crash between durable outer `RESERVED` and durable inner acceptance;
- failure to atomically bind inner dedupe/session acceptance with `COMPLETE`;
- same-key/id cross-wrapper collision while an attempt is pending;
- cross-request or cross-route bootstrap-key reuse/transplant; and
- stale-ACS deferral being incorrectly converted into terminal replay rejection.

These are endpoint-state threats. They do not create provider authority.

### 5. V-003 canonical criteria distinguish existing ADR-0143 evidence from new ADR-0144 evidence

The existing sealed-delivery V-003 criterion is tightened to name the full ADR-0143 `RESERVED/COMPLETE/REJECTED`, exact-wrapper fingerprint, crash-consistent inner-dedupe/`COMPLETE`, collision and idempotent retry rules.

A separate ADR-0144 criterion requires executable negative/cross-product coverage for:

- `reply_delivery_key.route_binding != reply_route`;
- profile mismatch, expiry beyond `reply_route`, and cross-request key/id reuse;
- bootstrap material being promoted into long-lived relationship authority;
- `RESERVED -> wait for authenticated ACS refresh -> COMPLETE`; and
- the mutation where stale/unknown relationship state is incorrectly converted to `REJECTED` before refresh.

That new criterion is intentionally **not** marked met by this design-only run. The following closure run must add/adjust executable evidence before the package is handed to the independent reviewer.

## Consequences

OM-Crypto-v1 wire objects remain unchanged. The additional state is local pending-request and receive-processing policy. Provider-visible metadata does not increase. A stale relationship head can delay delivery, but it cannot silently authenticate an unknown device and cannot destroy an otherwise valid exact retry merely because refresh was temporarily needed.

This ADR reopens no architecture-design queue item. It strengthens the frozen design's explicit invariants and temporarily makes the V-003 local-evidence queue non-empty until the bounded Run-2 criterion is evidenced. Engineering remains unauthorized.
