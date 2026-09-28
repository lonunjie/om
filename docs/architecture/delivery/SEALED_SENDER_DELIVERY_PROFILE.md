# Sealed Delivery Profile for Established 1:1 Messaging

- **Status:** Canonical W0 / OM-Crypto-v1 architecture
- **Checkpoint:** 2026-09-27 Run 3
- **Decision:** ADR-0142, amended by ADR-0143 and ADR-0144 (supersedes ADR-0138's later-profile disposition for A-054)
- **Design queue:** A-054 resolved and promoted into v1 before engineering

## 1. Goal and boundary

For an accepted `ACTIVE` 1:1 relationship, a mailbox-class Home must not receive protocol-visible sender AccountID, sender DeviceID, sender identity key, Double Ratchet header or ConversationID merely to store/route an asynchronous message.

OM therefore wraps the **entire unchanged ordinary 1:1 envelope** in `SealedDeliveryEnvelope` before a normal relay/mailbox deposit. The inner PQXDH/Double-Ratchet envelope remains the authentication authority. Sealed delivery is metadata protection against the routing provider, not anonymous messaging and not a replacement handshake.

First contact is still `MessageRequest`/IntroGate. Calls, live location, post-v1 ratchet upgrades, group-security/deniability upgrades, bots, plugins and bridges are outside this profile.

## 2. Why delivery keys are separate from RelaySet

The selected OM-Crypto-v1 hybrid HPKE public key is 1216 bytes. Placing one such key for every recipient device and relay directly into the 32 KiB `RelaySet` would create an accidental multi-device limit.

Instead:

- `RelaySet` remains the small per-relationship route/capability object;
- each recipient device publishes one signed `SealedDeliveryKeyRecord` per active RelayEntry to the accepted relationship;
- records travel only inside relationship E2EE state and can be delivered/updated independently; and
- the sender stores them under the already-trusted `RelationshipContext`.

This preserves per-relationship × per-recipient-device × per-provider-route isolation without inflating the routing object.

## 3. Route binding

For an exact private `RelayEntry`:

```text
route_binding = SHA-256(
  OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability]
)
```

The digest is not a public locator. It is relationship-private authenticated state. A delivery key is usable only with the RelayEntry that produces the same digest.

## 4. Delivery key material and signed record

`DeliveryKeyMaterial` contains:

```text
crypto_profile
opaque delivery_key_id      // 16 random bytes
HPKE public_key
route_binding               // Digest/SHA-256
expires_at
```

`SealedDeliveryKeyRecord` (object type 22) adds `recipient_device_id` and is signed.

Normative acceptance rules:

1. exactly one signature is present, role `device`;
2. signer id equals `recipient_device_id`;
3. that device is active and non-quarantined in the current relationship-pinned recipient ACS head;
4. the record is received inside the same accepted relationship and retained only in that local relationship state;
5. `route_binding` matches one current/overlap private RelayEntry for that relationship;
6. the profile's exact public-key length/suite is enforced; and
7. an expired record is not used for a new seal.

A record is not a globally publishable device credential. Replaying it into another relationship or route does not satisfy the route/context checks.

## 5. Outer envelope

`SealedDeliveryEnvelope` (object type 23) is:

```text
{
  crypto_profile,
  delivery_key_id,
  delivery_id,
  hpke_enc,
  ciphertext,
  expires_at
}
```

For OM-Crypto-v1 the HPKE suite is the same reviewed suite already pinned for MessageRequest: draft-ietf-hpke-pq-05 base-mode MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305. `hpke_enc` is exactly 1120 bytes and `DeliveryKeyMaterial.public_key` is exactly 1216 bytes.

The HPKE context is:

```text
info = OM-CBOR [
  "OM-SEALED-DELIVERY-HPKE-INFO-v1",
  crypto_profile,
  delivery_key_id,
  route_binding
]

aad = OM-CBOR [
  "OM-SEALED-DELIVERY-HPKE-AAD-v1",
  23, 1,
  crypto_profile,
  delivery_key_id,
  delivery_id,
  expires_at,
  plaintext_bucket_bytes
]
```

The wrapper itself contains no AccountID, DeviceID, ConversationID, identity key or Double Ratchet header.

## 6. Padding

HPKE plaintext is exactly:

```text
uint32_be(inner_envelope_length) || inner_envelope || CSPRNG padding
```

OM-Crypto-v1 uses the smallest fitting plaintext bucket from:

```text
1024, 4096, 16384, 65536, 131072
```

With the 16-byte AEAD tag, valid profile-1 ciphertext lengths are exactly:

```text
1040, 4112, 16400, 65552, 131088
```

After open, `inner_envelope_length` must be at most `bucket-4`, exactly delimit one complete ordinary envelope and leave only padding bytes after it. Padding is intentionally simple: it hides exact size only within a bucket.

## 7. Replay, expiry and retry

`delivery_id` is fresh random 128-bit data for each newly sealed wrapper. The recipient keeps durable replay state for `(delivery_key_id, delivery_id)` through acceptance expiry. For a **new** replay key, valid delivery-key/route/expiry checks and outer HPKE open/structure checks precede creation of a `RESERVED` record containing a collision-resistant fingerprint of the complete canonical wrapper before inner plaintext/event release or acknowledgement. A different wrapper under the same key/id is rejected. An exact same-wrapper retry may resume `RESERVED` after a crash; `COMPLETE` is an idempotent transport replay that never re-runs inner effects; `REJECTED` is terminal. Persistence failure is fail-closed.

For an **existing** replay key, bounded canonical outer parsing, local route-context checks and exact-wrapper fingerprint comparison occur before requiring live delivery-key material. `COMPLETE` plus the exact recorded fingerprint may therefore be acknowledged idempotently without delivery-key lookup, HPKE reopen or inner reprocessing; `REJECTED` remains terminal and a different fingerprint remains a collision. `RESERVED` exact retries still resume normal processing and therefore retain/access the private key needed to repeat HPKE open. This ordering is normative under ADR-0145 and permits completed bootstrap keys to be destroyed without breaking lost-ack retry liveness.

The authenticated inner session/dedupe acceptance and transition to `COMPLETE` must be one crash-consistent commit (transaction or equivalent write-ahead protocol) before application release/ack. If that commit fails, state remains `RESERVED` and an exact retry can resume. If a crash follows `COMPLETE` but precedes presentation/ack, normal local materialization recovers the already-durable event and the transport retry is acknowledged idempotently. See ADR-0143/0145.

`expires_at` is HPKE-authenticated and uses the registry's frozen default skew. Endpoint acceptance also requires `expires_at <= delivery_key.expires_at` and, when the bound RelayEntry has expiry, `expires_at <= route.expires_at`. For MessageRequest bootstrap, ADR-0147 defines the request acceptance deadline as `min(reply_route.expires_at, reply_delivery_key.expires_at)`; v1 has no independent local request-expiry input. A violating new wrapper is rejected before `RESERVED` allocation. Homes may impose shorter retention but cannot extend endpoint acceptance. Replay/tombstone state survives only through the authenticated acceptance window plus the skew needed to decide expiry and never extends message authority.

- retry to the same route may resend the exact wrapper;
- retry/fanout through another route must reseal using that route's key/context and a fresh `delivery_id`;
- the authenticated inner EventID remains canonical and handles logical-message deduplication.

## 8. Recipient processing and sender authentication

The receiving path is:

1. identify the local route/queue context, bounded-parse the canonical outer wrapper and compute its exact fingerprint;
2. consult durable replay state by opaque `(delivery_key_id, delivery_id)` before requiring live key material: exact `COMPLETE` on the same route/context is an idempotent acknowledgement/materialization path with no HPKE or inner rerun; exact `REJECTED` is terminal; a different fingerprint is a collision; exact `RESERVED` continues below;
3. for a new record or resumable `RESERVED`, find the relationship/request-local delivery-key record by opaque `delivery_key_id`;
4. require route-binding, device/request eligibility and expiry validity;
5. HPKE-open with the frozen info/AAD and validate the padded plaintext;
6. for a new record, durably create ADR-0143 `RESERVED` with the exact-wrapper fingerprint; for an existing `RESERVED`, require that exact fingerprint;
7. recover the unchanged ordinary inner envelope;
8. process it only through the normal inbound 1:1/bootstrap API requiring the appropriate trusted identity context; if an established-session API cannot yet decide authentication solely because the trusted peer ACS head may be stale or its authenticated monotonic refresh is temporarily unavailable, keep the exact wrapper `RESERVED` with no release/successful ack until refresh resolves the decision; and
9. once inner authentication is decisively valid, crash-consistently commit inner session/dedupe acceptance plus replay `COMPLETE` before application release/ack. A definitive inner failure may become `REJECTED`; uncertainty caused by required stale-head refresh is not a terminal failure.

Outer HPKE success is **not sender authentication**. The inner ACS/DeviceCredential/PQXDH/ratchet checks remain mandatory before plaintext/event acceptance. ADR-0144 composes K7/K12 with ADR-0143 so a temporarily stale trusted head cannot burn an otherwise valid exact retry.

## 9. First-contact bootstrap

Unknown senders do not have an established relationship delivery key. `MessageRequest` remains the only first-contact path.

`MessageRequestPayload` includes both:

- `reply_route`: the existing private request-scoped reply RelayEntry, interpreted as one **logical** bootstrap response with bounded transport use rather than one physical deposit; and
- `reply_delivery_key`: fresh request-scoped `DeliveryKeyMaterial` whose `route_binding` matches that exact route.

ADR-0144 makes the local scope explicit. For each MessageRequest the requester generates a fresh key pair and `delivery_key_id`, binds `(ticket_id, request_key_digest, crypto_profile, reply_route, reply_delivery_key)` in pending-request state, requires the key profile to equal the MessageRequest profile, and requires `reply_delivery_key.expires_at <= reply_route.expires_at`. The same private key/id is not reused by another request, relationship or ordinary delivery record.

The accepting recipient uses that key only to seal one logical post-acceptance control reply containing its `IntroIdentityBinding` plus current relationship RelaySet/delivery-key records. A transport retry resends the exact already-sealed candidate/winning wrapper bytes; it does not reseal an equivalent response merely to retry transport. While the request is `OPEN`, exact retries do not consume another candidate slot and at most **8 distinct candidate wrappers over the request lifetime** may be admitted across racing recipient devices; the ninth is rejected before durable `RESERVED` allocation. Rejection, terminalization, tombstone cleanup or compaction never replenishes a consumed slot while the request remains `OPEN`; sufficient durable tuple/accounting state remains for the lifetime bound. If multiple recipient devices race, candidates may be processed, but the requester's crash-consistent bootstrap commit atomically selects exactly one valid winning wrapper. A second different wrapper cannot become another bootstrap branch. After verifying the winning recipient binding, the requester can answer using the recipient's now-validated established delivery key and include its reciprocal binding and established key records. Existing rules still keep the relationship `ACCEPTED_UNBOUND` until both stable-identity bindings verify.

The winning bootstrap commit atomically binds the request to the winning `(delivery_key_id, delivery_id, wrapper fingerprint)`, installs the authenticated recipient binding/established route and delivery-key state needed for continuation, commits inner/session dedupe plus replay `COMPLETE`, marks every non-winning `RESERVED` candidate `REJECTED`, and changes the request to `COMPLETE_REPLAY_ONLY`. Only after that same commit may the bootstrap private key be destroyed. Cancellation, decline, expiry and terminal invalidation race on the same `OPEN` request transition and likewise terminalize all outstanding `RESERVED` candidates before key destruction. The non-secret replay/request tombstone remains through the bounded acceptance window. The winning wrapper is acknowledged idempotently only through its own expiry/skew window; the request remains `COMPLETE_REPLAY_ONLY` through the derived bootstrap acceptance deadline plus skew and then becomes `EXPIRED`, after which request-scoped tombstones may be collected. A wrapper still `RESERVED` therefore always has the private key needed to resume. Historical backup/restore never reactivates request-scoped bootstrap keys or live request/replay state. The bootstrap key is never promoted into a `SealedDeliveryKeyRecord` (ADR-0144/0145/0146/0147).

## 10. Authorization and abuse

The provider sees/validates only the opaque relationship/route deposit capability over the protected authorization channel plus the sealed payload metadata it needs to bound storage and expiry. It can enforce capability validity, rate limits, object-size limits, retention, quota and revocation without clear sender identity.

Unknown senders remain behind IntroGate/MessageRequest. An accepted contact can abuse only capabilities it possesses; block/relationship termination revokes those capabilities and associated delivery-key state.

## 11. Multi-device, offline and migration behavior

A sender creates a separate ordinary inner envelope per eligible target DeviceID, then seals each with a current record for that target device and selected relay route. Missing/stale records cause queueing/refresh/failure, never an unsealed relay fallback.

New devices publish records only after ACS authorization. Quarantined/revoked devices immediately become ineligible for new sends. Old private delivery keys may be retained only through bounded queued-message overlap.

A RelaySet migration creates fresh route capabilities and fresh delivery keys. Normal add/overlap/ack/drain/revoke semantics remain unchanged. A new higher RelaySet does not change AccountID, relationship identity or EventID.

## 12. Downgrade and unsupported peers

For an `ACTIVE` OM-Crypto-v1 relationship on a mailbox/relay path, sealed delivery is mandatory. A missing key, unsupported implementation, failed outer open, stripped wrapper or expired record causes an explicit delivery/session failure; it never causes an ordinary clear-sender envelope to be deposited as fallback.

Local error classes are:

- `sealed_delivery_required`;
- `delivery_key_unknown_or_stale`;
- `delivery_route_mismatch`;
- `delivery_expired`;
- `delivery_replay`;
- `delivery_open_failed`; and
- `sealed_delivery_unsupported`.

The superseded pre-release clear-sender draft may exist only as explicitly named non-production compatibility/test behavior. It is not OM-Crypto-v1 and cannot be silently negotiated.

## 13. Provider view and residual leakage

A conforming mailbox Home may observe:

- its opaque queue/deposit capability/resource handle;
- source network address unless a privacy-enhanced transport is used;
- timing/retry behavior;
- ciphertext/padding bucket;
- outer object/profile/version;
- opaque `delivery_key_id`; and
- authenticated expiry needed for retention handling.

By protocol design it does **not** receive sender AccountID, sender DeviceID, `EncodeEC(IK)`, Double Ratchet header or ConversationID from established 1:1 payloads.

Sealed delivery is not anonymity. Timing, IP, volume, route reuse, provider colocation and endpoint compromise remain separate threats.

## 14. Verification target

Run-3 verification must catch at least:

- sender AccountID/DeviceID/identity key/DR header/ConversationID appearing in the outer object;
- a delivery-key record used on the wrong route or relationship;
- profile-1 HPKE key/enc length or padding-bucket drift;
- outer replay/expiry bypass;
- treating outer HPKE open as sender authentication;
- accepted-relationship clear-envelope fallback or wrapper stripping;
- a MessageRequest missing the bootstrap reply key; and
- metadata descriptors that still claim mailbox Homes see the stable sender key/ratchet header on established 1:1 delivery.
