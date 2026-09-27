# ADR-0142 — Sealed delivery is mandatory for established OM-Crypto-v1 1:1 relay delivery

- **Status:** Accepted
- **Date:** 2026-09-27
- **Scope:** Run 3 architecture remediation only; no engineering authorization
- **Amends:** ADR-0138 A-054 disposition, ADR-0131 relay-metadata exposure wording, OM-Crypto-v1 delivery profile
- **Amended by:** ADR-0143 crash-safe replay reservation/exact-retry completion

## Context

ADR-0138 fixed a later-profile design for hiding the sender identity key and Double Ratchet header from mailbox-class Homes, while W0 still exposed those fields. The Run-3 remediation asks whether that protection can safely ship in v1 before implementation.

It can, but the key-distribution shape matters. Embedding a hybrid HPKE public key for every `(relationship, recipient device, relay)` tuple directly in the 32 KiB `RelaySet` would create an accidental device-count/relay-count ceiling. OM already permits many messaging devices, and the selected MLKEM768-X25519 public key is 1216 bytes. The delivery-key distribution therefore remains relationship-private but is carried as individually signed records rather than bloating `RelaySet`.

The security target is provider metadata minimization, not anonymous messaging. A relay may still observe source network information, timing, a padding bucket, an opaque deposit capability, an opaque delivery-key id and retry behavior. It must not receive protocol-visible sender AccountID, sender DeviceID, sender identity key, Double Ratchet header or ConversationID for established 1:1 delivery.

## Decisions

### 1. Established OM-Crypto-v1 relay delivery is sealed

For an `ACTIVE` accepted 1:1 relationship using OM-Crypto-v1, every normal mailbox/relay deposit carries a `SealedDeliveryEnvelope` around the **complete unchanged ordinary 1:1 envelope**. The ordinary envelope remains the authority for PQXDH/Double-Ratchet sender authentication and continues to require the trusted `RelationshipContext` of ADR-0137.

A mailbox/relay route MUST NOT accept or reinterpret an unsealed ordinary 1:1 envelope as an OM-Crypto-v1 established-relationship delivery. A peer that cannot use the sealed path is not conformant to the amended pre-release OM-Crypto-v1 profile. There is no automatic clear-sender fallback under profile id 1.

### 2. Delivery keys are per relationship, recipient device and relay route, without bloating `RelaySet`

A recipient device generates a fresh HPKE key pair for each accepted relationship and each active relay route. The public half is carried in a signed `SealedDeliveryKeyRecord` (object type 22); the private half remains in that recipient device's local protected state.

A record binds:

- `recipient_device_id`;
- `crypto_profile`;
- an opaque random `delivery_key_id`;
- the profile-defined HPKE public key;
- a domain-separated `route_binding` digest over the private `relay_uri` + deposit capability; and
- `expires_at`.

The record is sent only inside that accepted relationship's existing E2EE state. It MUST contain exactly one device-role signature whose signer is `recipient_device_id`, and that DeviceID MUST be active and non-quarantined in the relationship-pinned recipient ACS head when accepted. The sender stores the record only under that local `RelationshipContext`; it is not a globally reusable device credential.

This split is deliberate: `RelaySet` stays bounded and provider-replaceable, while any number of eligible recipient devices can publish small independent delivery-key records over relationship state.

### 3. Route binding is cryptographic and capability-derived

For a private `RelayEntry`:

```text
route_binding = SHA-256(
  OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability]
)
```

The digest is encoded as the normal `Digest {hash_alg=1, bytes}` structure. The capability itself never appears in the sealed wrapper and remains subject to ADR-0141 bearer-capability hygiene.

A sender uses only a delivery-key record whose `route_binding` matches the exact RelayEntry selected for that send. A recipient opens a wrapper only under the local key record associated with the queue/route on which it was received. A record transplanted to a different relationship or relay route therefore cannot produce a valid HPKE context.

### 4. `SealedDeliveryEnvelope` wire format and HPKE binding

Object type 23 is an unsigned outer delivery object:

```text
SealedDeliveryEnvelope {
  crypto_profile
  delivery_key_id
  delivery_id
  hpke_enc
  ciphertext
  expires_at
}
```

For OM-Crypto-v1 it uses the already selected reviewed HPKE suite from MessageRequest:

- draft-ietf-hpke-pq-05;
- base mode;
- MLKEM768-X25519;
- HKDF-SHA256;
- ChaCha20Poly1305;
- `hpke_enc` exactly 1120 bytes;
- recipient HPKE public key exactly 1216 bytes.

No new cryptographic primitive is introduced.

HPKE context is frozen as:

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

The wrapper carries none of `route_binding`, AccountID, DeviceID, ConversationID, identity public key or Double Ratchet header. `route_binding` comes from authenticated relationship-local delivery-key state.

### 5. Simple padding buckets

The HPKE plaintext is exactly:

```text
uint32_be(inner_envelope_length) || inner_envelope || random_padding
```

`random_padding` comes from a CSPRNG. OM-Crypto-v1 plaintext buckets are exactly:

```text
1024, 4096, 16384, 65536, 131072 bytes
```

The sender chooses the smallest bucket that can hold the four-byte length plus the complete inner envelope. A message that cannot fit the largest bucket is not valid for this delivery object; large media remains BlobStore content. ChaCha20Poly1305 adds a 16-byte tag, so profile-1 `ciphertext` length MUST be exactly one of `1040, 4112, 16400, 65552, 131088` bytes.

Padding reduces exact-size leakage; it does not hide the bucket, timing or traffic volume.

### 6. Replay, expiry and retry semantics are fail-closed

`delivery_id` is a fresh random 128-bit value for every newly sealed wrapper.

The recipient maintains durable replay state keyed by `(delivery_key_id, delivery_id)` through the wrapper's acceptance lifetime. After successful outer authentication and structural parsing, the id is durably reserved **before** inner plaintext can be released or the delivery acknowledged. ADR-0143 refines this into crash-safe `RESERVED` / `COMPLETE` / `REJECTED` local states: an exact same-wrapper retry may resume an interrupted `RESERVED` attempt, while completed/rejected replays and same-key/id collisions never re-run inner application effects. The inner session/dedupe acceptance and `COMPLETE` transition commit crash-consistently before release/ack. If replay or completion state cannot be persisted safely, the recipient does not release the inner plaintext.

`expires_at` uses the frozen default wall-clock skew policy. Expiry is authenticated by HPKE AAD. A Home may delete earlier according to its queue contract, but cannot extend recipient acceptance by rewriting expiry.

Retry rules:

- retry to the **same route** MAY resend the exact same wrapper and `delivery_id`;
- delivery through a **different relay/provider route** MUST use that route's delivery key, fresh HPKE randomness and a fresh `delivery_id`;
- logical message/EventID semantics remain unchanged and deduplicate after inner authentication.

### 7. Outer decryption never authenticates the sender

HPKE open proves only that the wrapper was formed for the corresponding delivery key/context. It does not establish which relationship device sent the inner envelope; a peer that knows the public delivery key can create wrappers.

After open, the recipient processes the recovered ordinary envelope exactly as before. The mandatory ADR-0137 `RelationshipContext`, ACS/DeviceCredential binding, PQXDH checks, established-ratchet checks and replay rules remain authoritative. No plaintext or authenticated event is released merely because the outer HPKE layer opened.

### 8. First contact remains MessageRequest; acceptance gets an explicit bootstrap key

Unknown senders do not receive established-relationship delivery keys or normal deposit capability. First contact remains `MessageRequest` through IntroGate.

To avoid one clear-sender acceptance reply, `MessageRequestPayload` now includes a required `reply_delivery_key` (`DeliveryKeyMaterial`). Its route binding must match the existing required `reply_route`. The key is generated for that request and is valid only for the post-acceptance bootstrap/control exchange associated with that request. It does not itself grant relationship identity or key-material-fetch authority.

On acceptance:

1. the recipient sends its first `IntroIdentityBinding` and current private RelaySet/delivery-key records inside an inner E2EE/control payload sealed to the requester's `reply_delivery_key`;
2. after verifying the recipient binding, the requester replies with its reciprocal binding and its own current private RelaySet/delivery-key records, sealed to a validated recipient `SealedDeliveryKeyRecord`;
3. the relationship becomes `ACTIVE` only under the existing reciprocal-binding rules; normal delivery then uses only established per-device/per-route records.

The bootstrap key may be retried with the same wrapper/id but is not a normal long-lived relationship delivery key.

### 9. Multi-device, offline, rotation, block and migration

For every eligible target recipient DeviceID, the sender needs a current validated `SealedDeliveryKeyRecord` for the selected RelayEntry. Absence, expiry or invalidity causes local queueing/key refresh or a typed delivery failure; it NEVER causes a clear-sender relay deposit.

A newly authorized device publishes its own records after it has valid recipient ACS authority and route state. A quarantined/revoked device's records are immediately ineligible for new sends. Recipient devices retain old private delivery keys only for bounded overlap needed to open already queued wrappers, subject to revocation policy.

Relay migration uses the existing higher-version RelaySet rules. New routes get fresh capabilities and fresh delivery keys. Old-route key records expire with the old route overlap; emergency route revocation invalidates both the route capability and its associated delivery-key records. Provider migration never changes AccountID, relationship identity or EventID.

### 10. Abuse controls remain capability-scoped

The provider authorizes a deposit with the relationship/route opaque capability over the protected authorization channel defined by ADR-0141. It enforces capability validity, rate/size/retention/storage quotas and revocation without receiving sender identity fields from the payload.

A malicious accepted contact can still send traffic to a capability it legitimately possesses. Recipient block/relationship termination revokes that capability and delivery-key state. Unknown senders remain constrained by IntroGate/MessageRequest and cannot use sealed established delivery as an admission bypass.

### 11. Errors and downgrade handling

The client/session boundary distinguishes at least these local protocol failures:

- `sealed_delivery_required` — an established relay path attempted an unsealed ordinary envelope;
- `delivery_key_unknown_or_stale` — no current relationship/device/route record matches;
- `delivery_route_mismatch` — the key record does not bind the selected/receiving route;
- `delivery_expired` — authenticated wrapper expired beyond allowed skew;
- `delivery_replay` — `(delivery_key_id, delivery_id)` already reserved;
- `delivery_open_failed` — HPKE or padded-plaintext validation failed; and
- `sealed_delivery_unsupported` — peer claims a profile/path that cannot satisfy the v1 requirement.

These are endpoint/session errors, not new Home authority. Providers continue to use the frozen Home error registry for provider operations.

A stripped/corrupted wrapper cannot become a valid clear-sender fallback: an active relay path expects object type 23, and the inner ordinary envelope is inaccessible without HPKE open. A pre-release implementation of the superseded clear-sender draft may be retained only under an explicitly named non-production compatibility/test mode; it MUST NOT be negotiated automatically as OM-Crypto-v1.

### 12. Threat boundary and explicit exclusions

Sealed delivery hides protocol sender identity/header fields from a mailbox-class Home. It does **not** hide:

- sender network address unless a privacy-enhanced transport path is used;
- message timing, padding bucket or traffic volume;
- the recipient queue/deposit capability or provider-local resource handle;
- the opaque `delivery_key_id` during that key's lifetime;
- cross-role observations when one provider operates several roles; or
- endpoint compromise.

This ADR does not pull calls, live location, SPQR/Triple Ratchet, PQ MLS, group deniability, bots, plugins or bridges into v1. Group transport wrapping keeps its existing independent design.

## Consequences

The prior ADR-0138 “later profile” disposition for A-054 is superseded: sealed delivery is now part of the pre-release OM-Crypto-v1 W0 relay-delivery contract for established 1:1 relationships. That is an intentional frozen-wire change before engineering, so V-001/V-005/V-015 evidence must be refreshed and remains open where a tool-complete or independent run is still required.

No provider becomes an identity authority. Delivery-key records are authenticated by recipient device authority and relationship context, while sender authentication remains solely inside the existing end-to-end 1:1 protocol. Engineering remains unauthorized.
