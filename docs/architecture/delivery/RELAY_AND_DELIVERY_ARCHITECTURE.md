# Relay and Delivery Architecture

## Design target

Reliable asynchronous delivery without turning the relay into an identity provider, conversation owner, or plaintext endpoint.

## AccountID is not a network address

A stable AccountID is valuable for continuity and authorization. It is dangerous as a universal routing handle. Relays therefore route using random opaque capabilities.

After Alice and Bob accept a relationship:

```text
Alice AccountID <---- private relationship ----> Bob AccountID

Alice -> Bob:
  random capability q1...
  one or more receiving relays

Bob -> Alice:
  unrelated random capability q2...
  one or more receiving relays
```

A relay should not require knowledge that q1 belongs to Bob, that q2 belongs to Alice, or that q1/q2 form a pair.

## Capability properties

Delivery capabilities should be:

- high-entropy and unguessable;
- scoped to a relationship/device/queue purpose;
- independently rotatable;
- revocable;
- optionally short-lived or epoch-derived where practical;
- unlinkable across providers where possible;
- distinct from AccountID, username, phone number, device display name, or group ID.

## Queue model

A relay provides approximately:

```text
create_queue(capability policy) -> opaque queue descriptor
put(queue_cap, ciphertext envelope)
fetch/stream(read_cap)
ack/delete(message token)
rotate(queue)
revoke(queue)
```

Exact wire methods may differ, but semantics are intentionally small.

## Envelope rules

- Relay sees ciphertext plus only metadata necessary to route/expire/rate-limit.
- Envelope size has a strict bound; large data uses encrypted BlobStore objects.
- Event identity is not assigned by relay.
- Relay order is not canonical conversation order.
- Delivery is idempotent; duplicate envelopes are safe.
- Acknowledgement/deletion is explicit and retry-safe.
- Offline retention is bounded.

## Group fanout correlation

When one MLS application ciphertext is delivered to multiple recipient devices/providers, wrap it in a fresh randomized transport envelope per destination. Do not place byte-identical outer ciphertext on multiple relays if avoidable; this reduces trivial cross-relay correlation.

## Multi-relay

A user may publish multiple private receiving capabilities on different providers. Senders can use redundancy according to policy:

- primary + fallback;
- mirrored delivery for reliability;
- rotating providers;
- different providers for different relationships.

Provider diversity should prefer different failure/coercion domains where practical, not merely different hostnames on the same underlying cloud.

## Introduction endpoint

Unknown senders do not receive normal private delivery capabilities. They use a constrained `IntroductionEndpoint` with cheap server-side validation, tight quotas, minimal payload sizes, and no expensive unsolicited features.

Acceptance creates a private `RelationshipBinding` and new delivery capabilities.

## Mandatory sealed delivery for established 1:1 traffic

ADR-0142 promotes sealed delivery into W0 / OM-Crypto-v1 for accepted `ACTIVE` 1:1 relationships. A normal
mailbox/relay deposit carries an unsigned `SealedDeliveryEnvelope` around the complete ordinary inner 1:1 envelope.
The outer wrapper exposes only the profile, opaque delivery-key id, fresh delivery id, HPKE encapsulation, padded
ciphertext bucket and expiry; it does **not** expose sender AccountID/DeviceID/identity key, ConversationID or the
Double Ratchet header. Opening that outer HPKE layer is not sender authentication: the recipient must still validate
the unchanged inner envelope under its required `RelationshipContext`.

Delivery keys are signed per accepted relationship × recipient device × exact RelayEntry route and are exchanged
inside relationship E2EE, rather than inflating the 32 KiB `RelaySet`. First contact remains
`MessageRequest`/IntroGate and carries one request-scoped reply delivery key so the first post-acceptance response can
already be sealed. Its reply RelayEntry is one-logical-response, not one-packet: providers preserve exact admitted-wrapper retries and may admit at most the protocol cap of 8 distinct bootstrap candidates while the request is open; endpoints enforce the same cap regardless of provider behavior. There is no clear-sender fallback for OM-Crypto-v1; missing/stale/unsupported sealed-delivery state
queues, refreshes or fails explicitly. See [`SEALED_SENDER_DELIVERY_PROFILE.md`](SEALED_SENDER_DELIVERY_PROFILE.md).

## Abuse enforcement

Because modified open-source clients can remove UI rate limits, a relay/recipient boundary enforces:

- capability validity;
- envelope size;
- per-capability rates;
- introduction quotas;
- storage/expiry limits;
- for MessageRequest reply capabilities, bounded distinct-candidate allocation plus exact-wrapper retry semantics rather than destructive first-deposit consumption;
- optional proof-of-work/token/billing mechanisms if later required;
- local blocking/denial lists without a mandatory global identity score.

## Cloudflare reference implementation

Workers + SQLite-backed Durable Objects can implement a convenient relay backend:

- Worker/API ingress;
- Durable Object per queue/shard/recipient-capability strategy;
- WebSocket hibernation for connected clients;
- SQLite for bounded offline envelopes;
- push trigger after durable enqueue where needed.

This is an implementation profile, not protocol truth. A VPS relay, community server, home node, or other serverless platform implements the same relay semantics.

## Operational budget behavior

Provider adapters expose quotas/cost state. Free-tier Durable Object limits fail further operations after limits are exceeded, so an OM reference deployment must track usage and fail over before hard failure. Paid-capable providers may instead bill overages; budget policy must be explicit.

## Required conformance tests

- relay A and relay B interoperate with same client;
- relationship survives relay migration;
- queue rotation does not change AccountID/contact;
- duplicate put/fetch/ack behavior;
- offline expiration;
- revoked capability rejection;
- hostile sender cannot bypass IntroductionEndpoint limits;
- relay cannot force crypto downgrade;
- relay loss does not destroy canonical conversation history;
- randomized wrappers differ for identical inner group ciphertext.

## Multi-relay RelaySet checkpoint

Accepted relationships MAY maintain multiple simultaneous relays. Each relay gets a distinct opaque capability. A private authenticated `RelaySet`, one per relationship and signed by a device active and not quarantined in the recipient's current AccountControlState ([`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) §2.1), defines priority/expiry/transport hints. Normal sending chooses one healthy relay; failover is automatic. Selected high-reliability messages MAY use independently wrapped redundant delivery and are deduplicated by authenticated EventID.

Providers should be diversified across failure domains where practical. See [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) and ADR-0022.
## 2026-09-22 key-material separation

Relays deliver opaque encrypted envelopes. They are not implicitly responsible for publishing recipient asynchronous handshake material. `KeyMaterialProvider` is a separate logical capability defined by [`ASYNC_KEY_MATERIAL.md`](../crypto/ASYNC_KEY_MATERIAL.md), even when one deployment co-hosts the roles.

## 2026-09-23 relay resolution

Deposit and drain capabilities are separate high-entropy scoped bearer capabilities. RelaySet is signed/versioned/hash-chained; migration uses add/overlap/ack/drain/revoke. Total known-route loss may use the optional relationship-secret opaque rendezvous in [`REACHABILITY_AND_OBJECT_REPAIR.md`](REACHABILITY_AND_OBJECT_REPAIR.md). Failure-domain metadata is evidence-scored rather than treated as provider truth.
