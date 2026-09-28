# ADR-0158 — LanDeliveryProfile v1: private rendezvous, sealed local routes, browser-capable framing

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** resolves A-055 for W1 scope `local_delivery`; amends ADR-0150/0156 LAN binding status
- **Verification gate:** V-024
- **Does not authorize engineering.**

## Context

ADR-0150 promised optional offline-local message/file delivery but ADR-0156 correctly blocked shipment until one concrete interoperable binding existed. The binding must not create a second messaging protocol, expose the clear-sender inner 1:1 envelope, turn LAN reachability into relationship authority, or require a native-only discovery API.

The lowest-risk design is to reuse frozen W0 objects wherever possible. In particular, an ephemeral local route can be represented by the existing `RelayEntry` shape, so `DeliveryKeyMaterial.route_binding` can continue using the already-frozen `OM-SEALED-DELIVERY-ROUTE-v1` construction. LAN transport adds discovery/signalling/framing, not a new authority primitive.

## 1. Profile constants

`LanDeliveryProfile-v1` fixes:

- rendezvous epoch: **900 seconds**;
- accepted discovery epochs: current plus adjacent `-1/+1` only for clock skew; a device never advertises an old epoch after a link-layer address change;
- advertisement cardinality **K = 8** tags per device per epoch, real tags plus cryptographically random decoys; if more than eight eligible peer-device tags exist, the local client rotates a deterministic local subset across epochs without changing wire semantics;
- rendezvous tag length: **16 bytes**;
- local route capability: **32 random bytes**;
- local route token/session/transfer identifiers: **16 random bytes**, fresh per rendezvous epoch or connection as specified below;
- application payload per `LanFrameV1` chunk: **at most 16,384 bytes**;
- sender backpressure high-water: **262,144 queued bytes**; resume at or below **65,536 bytes**. A runtime may use a lower high-water, never a higher one without negotiated profile change.

All wall time here is discovery/liveness metadata only, never message-ordering or authorization authority.

## 2. Rendezvous seeds and tags

### Existing accepted relationship

Each active device-pair derives a LAN seed from the hybrid PQXDH `SK` at the session establishment that created its current relationship-pinned Double Ratchet:

`LanSeed = HKDF-SHA256(SK, salt="", info=OM-CBOR ["OM-LAN-PAIR-SEED-v1", crypto_profile, initiator_device_id, responder_device_id], L=32)`.

The seed is stored as relationship-private session state, never advertised, never uploaded as provider-readable metadata, and destroyed when that device-pair relationship/session is retired. It is a purpose-separated export; raw ratchet message keys are never reused for LAN discovery.

### QR/invite first contact

When an `Invite` contains scope `lan_rendezvous` and key 9 decodes as `LanInviteRendezvousV1 {0: 1}`, the temporary seed is:

`LanSeed = HKDF-SHA256(invite_secret, salt="", info=OM-CBOR ["OM-LAN-INVITE-SEED-v1", invite_id], L=32)`.

It expires with the Invite and inherits the Invite's use/revocation limits where the receiving device has current revocation state. Offline operation cannot promise knowledge of a remote revocation, so short expiry remains mandatory UX guidance.

### Tag

For epoch `e = floor(posix_seconds / 900)`:

`tag = Trunc16(HMAC-SHA256(LanSeed, OM-CBOR ["OM-LAN-RENDEZVOUS-TAG-v1", e]))`.

A receiver accepts only a tag it can compute from an enabled current relationship/invite seed in `e-1`, `e`, or `e+1`, and rate-limits before any expensive crypto. The three labels are registered. Tag matching establishes only possession of rendezvous capability, never account/device authority.

## 3. Discovery and authenticated signalling

Native clients MAY advertise the fixed K-tag set with mDNS/DNS-SD or another local substrate. The advertisement contains no AccountID, DeviceID, username, device name, delivery-key id, RelaySet locator or stable service instance. Service-instance names and listening endpoints are fresh at least every rendezvous epoch and whenever the link-layer address changes.

All implementations MUST also understand the canonical `LanSignalEnvelopeV1` so discovery is not tied to mDNS. WebRTC SDP contains connection credentials and is therefore **not sent in clear** on a local signalling endpoint. Each signal has a fresh 16-byte `session_id`, rendezvous epoch and 12-byte random nonce. Derive:

`SignalKey = HKDF-SHA256(LanSeed, salt="", info=OM-CBOR ["OM-LAN-SIGNAL-KEY-v1", session_id, epoch], L=32)`.

Encrypt the UTF-8 SDP with the profile's existing AES-256-GCM-SIV primitive; associated data is `OM-CBOR ["OM-LAN-SIGNAL-AAD-v1", version, kind, session_id, epoch]`. The envelope carries only `(version, kind, session_id, epoch, nonce, ciphertext)`. Nonce reuse under one SignalKey is forbidden; a fresh session id/nonce is generated for every new offer/answer attempt.

The envelope may be exchanged through a local endpoint, copy/paste, or QR/multi-QR. A passive LAN listener without `LanSeed` learns neither the SDP/ICE credentials nor a stable identity and cannot forge a signal. For invite first contact, possession of the Invite is intentionally enough to decrypt/signal but is still not identity/relationship authority.

## 4. Byte channel

The interoperable V1 byte channel is an **ordered, reliable WebRTC `RTCDataChannel`** with application protocol label `om-lan-v1`. Same-LAN operation uses host/local ICE candidates and MUST NOT require a project-operated signalling, STUN or TURN service. DTLS protects the transport channel; OM payload cryptography remains authoritative and unchanged.

A platform that can establish another local byte channel MAY adapt it only if it presents exactly the same `LanFrameV1` stream and security properties. Native-only channel semantics are forbidden.

A PWA may use manual/QR `LanSignalEnvelopeV1` exchange when automatic local discovery is unavailable. If its browser cannot establish a host-local WebRTC DataChannel under the user's permissions/policy, the client reports `local_delivery = unavailable`; it must not claim a weaker alternate protocol.

## 5. Ephemeral local route reuses `RelayEntry`

After a channel is established, the receiver creates an ephemeral `RelayEntry` used only inside the current private relationship/bootstrap context:

- `relay_uri = "om+lan://v1/" + base64url(route_token)` where `route_token` is 16 random bytes without padding;
- `capability` is 32 random bytes;
- `priority` is local policy;
- `expires_at` is no later than the end of the current rendezvous epoch plus one 900-second skew epoch;
- `transports = ["webrtc-datachannel-v1"]`;
- `failure_domain`, if present, is only a local hint and carries no authority.

The URI is not globally resolvable and is never inserted into a public RelaySet. It names only a route in the current local-session table. The route token, capability and any `delivery_key_id` bound to it are destroyed/rotated at epoch or link-layer change.

A `SealedDeliveryKeyRecord` for this route uses the **existing** W0 route binding:

`SHA-256(OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, capability])`.

Therefore normal 1:1 LAN delivery carries an ordinary `SealedDeliveryEnvelope`; after outer open, the unchanged inner PQXDH/Double-Ratchet envelope follows all normal relationship, ACS, replay and EventID rules. Group delivery carries the same MLS `PrivateMessage` bytes it would carry over another route. LAN reachability never authenticates a sender.

## 6. `LanFrameV1` transport framing

The DataChannel carries canonical-CBOR `LanFrameV1` values from [`spec/cddl/lan.cddl`](../spec/cddl/lan.cddl). Frames are transport records, not conversation/account authority.

The frame kinds are:

1. `object_begin` — fresh transfer id, object kind, total byte count and SHA-256 of the exact transported bytes;
2. `object_chunk` — transfer id, absolute byte offset and at most 16 KiB;
3. `object_end` — transfer id;
4. `ack` — transfer id and highest contiguous byte offset durably accepted by this transport receiver;
5. `fetch_async_key` — request id, target DeviceID, CryptoProfile and relationship-scoped key-material fetch capability;
6. `fetch_blob` — request id, BlobID and expected ciphertext digest;
7. `error` — request/transfer id plus bounded profile error code.

Object kinds include sealed-delivery envelopes, MLS PrivateMessages, MessageRequests, signed IntroductionTickets/ContactDescriptors/AsyncKeyBundles/SealedDeliveryKeyRecords and encrypted blob ciphertext. `object_kind` is routing metadata only; the inner object is independently parsed and authenticated by its normal canonical rules. Unknown object kinds are rejected for this profile, not guessed.

Receivers stream chunks and MUST NOT allocate `total_bytes` eagerly. They enforce local policy limits before accepting a transfer, require offsets to be contiguous unless explicitly resuming from an acknowledged offset, and verify the declared SHA-256 before handing bytes to the inner parser. Blob plaintext is never released until the existing `ciphertext_digest`/AEAD checks succeed.

The sender stops enqueueing frames once the DataChannel `bufferedAmount` exceeds the high-water and resumes only after it is at/below the low-water (or an equivalent adapter signal). Acknowledgement is only transport progress; it is not message receipt, relationship acceptance or storage durability.

## 7. Offline first contact uses the existing IntroGate state machine

A QR/invite LAN session does **not** create a relationship.

1. The invite holder proves possession by successfully deriving the rendezvous tag and decrypting/authenticating the seed-bound signalling envelope.
2. The recipient device exposes a local IntroGate adapter over the channel. Using the Invite's existing capability/use policy, it provides a current intro-identity-signed `IntroductionTicket` and signed `ContactDescriptor`/request key. The local adapter applies the same payload/use/replay/rate limits as any IntroGate; it gains no new authorization rule.
3. The requester submits the ordinary HPKE `MessageRequest`. Its `reply_route` is the ephemeral `om+lan://v1/...` `RelayEntry`, and its `reply_delivery_key.route_binding` is the frozen W0 route digest over that entry.
4. Acceptance/decline and the reciprocal `IntroIdentityBinding` state machine are unchanged. LAN reachability or invite possession alone never creates `ACTIVE` relationship state or device eligibility.
5. Only after the relationship reaches the normal state that grants async-key fetch authority may `fetch_async_key` return a signed `AsyncKeyBundle`; the requester verifies all ordinary credential, freshness, provider-pool and replay rules before PQXDH work. No LAN-specific unsigned key bundle exists.

This is the same first-contact authority path with a local transport adapter, not a second offline identity protocol.

## 8. Files, retry and WAN reconciliation

A file request names the canonical BlobID and expected `ciphertext_digest`. The channel transfers the existing encrypted blob bytes in bounded frames. Interrupted transfer resumes from an acknowledged byte offset; resume state is transport-local and expires with the local route.

A message/file received locally keeps its ordinary EventID/BlobID. When WAN routes return, normal retry, dedupe and storage backfill operate on those same identities. A local transport ACK never claims provider durability; the UI continues to distinguish local receipt from configured StoragePool replication.

## 9. Privacy and lifecycle

- exactly K=8 advertised tags per active advertiser/epoch, including decoys;
- LAN rendezvous remains off by default per contact and disabled in Hardened/Maximum unless a future profile explicitly changes that policy;
- block/relationship end immediately deletes that contact's LAN seeds and route state locally;
- route/session identifiers never survive epoch or link-layer address change;
- channel timing, volume and IP/link-layer co-location remain observable residual metadata;
- a contact explicitly enabled for rendezvous can infer the user's presence on the shared LAN; the UI must describe that consequence.

## Consequences

A-055 is design-resolved. `design_open` returns to 0. The `local_delivery` scope remains **unauthorized** until V-024 produces the required two-codec vectors, passive-observer and first-contact mutation models, a real browser feasibility demonstration (or honest unavailable capability result), and independent review. Pre-release still requires two-implementation real-LAN interoperability.

This ADR is OpenAI-authored and cannot satisfy its own ADR-0117 independence requirement.
