# Sealed Delivery Profile for Established 1:1 Messaging

- **Status:** Canonical W0 / OM-Crypto-v1 architecture
- **Checkpoint:** 2026-09-27 Run 3
- **Decision:** ADR-0142 (supersedes ADR-0138's later-profile disposition for A-054)
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

`delivery_id` is fresh random 128-bit data for each newly sealed wrapper. The recipient keeps durable replay state for `(delivery_key_id, delivery_id)` through acceptance expiry. After valid outer open/structure checks, it creates a `RESERVED` record containing a collision-resistant fingerprint of the complete canonical wrapper before inner plaintext/event release or acknowledgement. A different wrapper under the same key/id is rejected. An exact same-wrapper retry may resume `RESERVED` after a crash; `COMPLETE` is an idempotent transport replay that never re-runs inner effects; `REJECTED` is terminal. Persistence failure is fail-closed.

The authenticated inner session/dedupe acceptance and transition to `COMPLETE` must be one crash-consistent commit (transaction or equivalent write-ahead protocol) before application release/ack. If that commit fails, state remains `RESERVED` and an exact retry can resume. If a crash follows `COMPLETE` but precedes presentation/ack, normal local materialization recovers the already-durable event and the transport retry is acknowledged idempotently. See ADR-0143.

`expires_at` is HPKE-authenticated and uses the registry's frozen default skew. Homes may impose shorter retention but cannot extend endpoint acceptance.

- retry to the same route may resend the exact wrapper;
- retry/fanout through another route must reseal using that route's key/context and a fresh `delivery_id`;
- the authenticated inner EventID remains canonical and handles logical-message deduplication.

## 8. Recipient processing and sender authentication

The receiving path is:

1. identify the local route/queue context;
2. find the relationship-local delivery-key record by opaque `delivery_key_id`;
3. require route-binding, device eligibility and expiry validity;
4. HPKE-open with the frozen info/AAD and validate the padded plaintext;
5. durably create/validate the ADR-0143 replay record (`RESERVED`, exact-wrapper fingerprint);
6. recover the unchanged ordinary inner envelope;
7. process it only through the normal inbound 1:1 API requiring trusted `RelationshipContext`; and
8. crash-consistently commit inner session/dedupe acceptance plus replay `COMPLETE` before application release/ack.

Outer HPKE success is **not sender authentication**. The inner ACS/DeviceCredential/PQXDH/ratchet checks remain mandatory before plaintext/event acceptance.

## 9. First-contact bootstrap

Unknown senders do not have an established relationship delivery key. `MessageRequest` remains the only first-contact path.

`MessageRequestPayload` includes both:

- `reply_route`: the existing one-use/private reply RelayEntry; and
- `reply_delivery_key`: a request-scoped `DeliveryKeyMaterial` whose `route_binding` matches that route.

The accepting recipient uses that key to seal the first post-acceptance control reply containing its `IntroIdentityBinding` plus current relationship RelaySet/delivery-key records. After verifying the recipient binding, the requester can answer using the recipient's now-validated established delivery key and include its reciprocal binding and established key records. Existing rules still keep the relationship `ACCEPTED_UNBOUND` until both stable-identity bindings verify.

The request-scoped key grants no stable identity, key-material-fetch authority or long-lived relationship privilege by itself.

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
