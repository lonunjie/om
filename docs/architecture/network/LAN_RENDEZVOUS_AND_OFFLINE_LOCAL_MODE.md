# OM LAN Rendezvous and Offline Local Mode

- **Status:** Canonical V1 optional capability; platform capability verification remains
- **Checkpoint:** 2026-09-28 ADR-0150

## 1. Goal

Existing contacts—and peers who just exchanged a QR/invite—should be able to discover each other on the same local network without broadcasting stable AccountIDs, usernames, device names, or globally linkable service identifiers.

V1 optional capability: when WAN access is unavailable but the LAN remains functional, OM supports local messaging and encrypted file transfer for already-related or short-lived QR/invite-linked peers where the platform can establish a route; calls keep their existing LAN/direct RoutePolicy. Automatic discovery is capability-gated and is not promised on every platform (ADR-0150).

## 2. Existing-contact rendezvous

Peers already share relationship secret material. Derive a rotating local rendezvous token using a dedicated KDF domain and short time epoch:

```text
LAN-Rendezvous-Secret
      + epoch
      + local context
          ↓ KDF/HMAC
random-looking token
```

The precise derivation MUST use a purpose-specific exported secret and MUST NOT reuse raw message-ratchet keys.

Properties:

- rotates frequently;
- not reversible to AccountID;
- only known contacts can predict/match it;
- a passive observer sees changing random-looking values;
- replay across epochs is rejected.

### 2.1 Contacts can be trackers (ADR-0130 rule 8)

"Only known contacts can match" is also the risk: any contact, including an estranged partner or a former employer
who is still a contact, can passively detect the user's physical presence on a shared LAN (T-087). And a device that
advertises one tag per contact leaks its contact count, while a tag set that rotates together from one MAC/IP address
is linkable across epochs by a passive Wi-Fi observer (T-088). Rules (R-270, R-271):

1. **Off by default, per contact.** LAN rendezvous runs only for contacts the user marks ("Find each other on the
   same Wi-Fi"). It is off for everyone under Hardened and Maximum Privacy. A QR/invite LAN secret (§3) covers only
   that one pairing.
2. **Retired on block or relationship end.** Blocking or removing a contact, or ending the relationship, immediately
   stops advertising and answering that contact's tags on every device of the account.
3. **Fixed cardinality.** A device advertises exactly K tags per epoch regardless of how many contacts are enabled
   (real tags plus random decoys), so advertisement size carries no information; if more than K contacts are
   enabled, tags rotate through the slots across epochs. A querier-initiated design (the device only answers
   queries) is an acceptable alternative for platforms where it fits. K is a policy constant fixed with V-015, not a
   wire constant.
4. **Rotate with the link layer.** Where the platform exposes it, tag epochs change together with MAC address
   randomization, and a device never answers old-epoch tags from a new address.

V-015 includes a LAN trace criterion for these rules.

## 3. New-contact LAN rendezvous

A QR/invite may carry a temporary LAN rendezvous secret/capability. This lets two nearby devices find each other without Internet service.

Offline invites should be short-lived because immediate remote revocation cannot be enforced while disconnected.

## 4. Service advertisement

OM may use mDNS/DNS-SD or another local discovery substrate, but published fields must be privacy-preserving. Do not advertise a stable `Maj's phone`, username, AccountID, or permanent host identifier.

The privacy requirements in RFC 8882 are useful design guidance: avoid linkable identifiers and unnecessary disclosure of client/server identity or discovery interests.

## 5. Signed local service registration

Where a local registrar architecture is useful, borrow the useful idea from SRP (RFC 9665): registrations are defended by public-key signatures rather than trusting an unauthenticated name announcement.

OM does not need to expose DNS-shaped identities; SRP is an architectural reference for secure registration.

## 6. Offline local behavior

If both peers have enough existing crypto state:

```text
Internet unavailable
      X
local Wi-Fi / LAN alive
      │
Alice OM <------> Bob OM
 messages / files / calls
```

Canonical OM events remain the same. A LAN endpoint/token is an ephemeral transport candidate, never identity or authority. Local delivery uses ordinary OM cryptographic/session authorization and the same logical EventID that a WAN/provider route would carry; route-specific transport wrapping may differ, but route selection cannot change message meaning or security level. When WAN service returns, ordinary sync/deduplication reconciles retries so one logical EventID materializes once.

Encrypted file transfer is likewise transport-neutral: direct LAN transfer carries the same encrypted object named by the canonical BlobID and `ciphertext_digest`. A receiving endpoint authenticates it before plaintext release. Later BlobStore mirroring/backfill relocates the same object without rewriting EventID/BlobID. The client distinguishes local receipt from configured StoragePool durability until provider/storage replication is actually satisfied.

## 7. Security boundaries

- LAN presence does not imply trust.
- Discovery only identifies a candidate endpoint; OM cryptographic relationship verification remains mandatory.
- No silent fallback from encrypted OM semantics to plaintext LAN protocols.
- Public Wi-Fi adversaries must not learn stable identity from advertisements.
- Clients must defend against token flooding/spoofing and resource exhaustion before expensive cryptographic/blob work.
- LAN failure never falls back to plaintext or unauthenticated local messaging; it queues or chooses another allowed encrypted route.
- A transport acknowledgement is not a conversation event, relationship grant, device-eligibility proof or storage-durability proof.
## 2026-09-22 hardening — platform capability boundary

The LAN protocol remains client-agnostic, but discovery substrates are capabilities, not universal guarantees. Native clients may use OS mDNS/DNS-SD/local-network APIs where permitted. PWA clients may require explicit local-network permission and may need QR/manual/known-endpoint/WebRTC-style rendezvous when browser APIs do not expose native-equivalent discovery.

OM promises interoperable local-mode semantics where a route can be established; it does not promise identical automatic discovery on every platform.

## 2026-09-23 LAN resolution

Relationship-derived, short-lived opaque rendezvous tags replace stable identifier broadcast. Native mDNS/DNS-SD is optional capability; QR/manual/known-endpoint is universal fallback. Browsers/PWAs are never required to expose native-equivalent automatic LAN discovery. LAN continues to carry ordinary OM E2EE ciphertext.

## 2026-09-29 concrete LAN delivery profile (ADR-0157)

A-055 is resolved at the architecture level. The binding below is the canonical V1/W1 `local_delivery` design; V-024 still has to freeze its CDDL/field registry and prove the required models/interoperability before the scope can be authorized.

### Rendezvous secret and token

Established-contact local mode is **opt-in per contact**. Each advertising recipient device generates a random 32-byte `LAN-Rendezvous-Secret` scoped to that accepted relationship and DeviceID and sends it only through authenticated relationship E2EE state. It is routing metadata, never identity or authority, and is retired on block, relationship end or device revocation. For QR/invite first contact, `Invite.lan_rendezvous` is exactly 32 random bytes and dies with the short-lived invite.

The epoch is `floor(unix_time / 900)` (15 minutes). Time here is only a discovery/liveness input. A matcher may try the current and adjacent epoch for normal clock skew. Every advertisement cycle uses a fresh 16-byte `link_nonce`, replaced on every epoch and every link-layer-address change. The tag is:

```text
Trunc128(HMAC-SHA-256(
  LAN-Rendezvous-Secret,
  OM-CBOR ["OM-LAN-RENDEZVOUS-TOKEN-v1", epoch, link_nonce]
))
```

Raw message-ratchet/PQXDH keys are never reused for LAN rendezvous.

`LanPrivacyPolicy-v1` fixes **K = 8 advertisement slots** per epoch per endpoint. Real tags occupy slots and random 16-byte decoys fill the rest. More than eight enabled grants are fairly rotated across epochs; contact count never changes the slot count. A query/response discovery substrate may avoid unsolicited tag sets, but if it emits a tag set it uses the same K=8 profile.

### Local channel

The native/desktop reference binding uses TLS 1.3 with an ephemeral certificate/key regenerated at least once per rendezvous epoch and whenever the link-layer address changes. OM does not treat the certificate as identity. Instead, after the TLS handshake both peers bind possession of the rendezvous secret to the exact TLS channel.

Each obtains 32 bytes from the RFC 8446 exporter using label `OM-LAN-TLS-EXPORTER-v1`, then sends a role-separated proof:

```text
HMAC-SHA-256(
  LAN-Rendezvous-Secret,
  OM-CBOR ["OM-LAN-CHANNEL-PROOF-v1", role, epoch, link_nonce, tls_exporter]
)
```

No identity-bearing OM object is sent until both proofs verify. Binding the proof to the TLS exporter prevents an active relay that terminates two TLS connections from merely forwarding challenge/proof messages while reading route/key material.

A PWA/browser reports `local_delivery=unavailable` unless its PlatformAdapter can expose an equivalently channel-bound direct-local transport. OM does not introduce weaker browser-only semantics just to claim parity.

### Reuse of RelayEntry and sealed delivery

After channel proof, an established recipient creates an ephemeral **ordinary `RelayEntry`**:

- `relay_uri = om-lan-v1://<base64url(16 random route_token)>`; this is a logical route handle, not an IP address or identity;
- `deposit_capability` is fresh random 32-byte capability material;
- transports includes `lan-tls13-v1`;
- `expires_at` is no later than the end of the current rendezvous epoch plus 120 seconds of drain/skew;
- route token/capability are replaced every epoch and on link-layer-address change.

The recipient also creates a fresh ordinary `SealedDeliveryKeyRecord` bound to that exact RelayEntry. The existing `OM-SEALED-DELIVERY-ROUTE-v1` route-binding construction is unchanged, and the delivery key expires no later than the route. The route offer is carried only after the secret-bound TLS channel is established, so the signed record's DeviceID and opaque delivery-key identifier are not visible to a passive LAN observer.

Established 1:1 traffic then sends the **existing canonical `SealedDeliveryEnvelope`**. The bare inner envelope is forbidden. WAN↔LAN retries reseal for the selected route but preserve the same canonical inner EventID, so materialization/dedupe is route-independent. Group local delivery carries MLS `PrivateMessage` bytes inside the same protected local channel; LAN reachability grants no Room membership or authority.

### Framing

The local channel is a bounded record stream:

```text
uint32_be(frame_length) || canonical_OM_CBOR(frame)
```

`frame_length` is non-zero and at most 163840 bytes. V-024 freezes the exact CDDL. The V1 frame-type set is closed to:

1. route offer (`RelayEntry` + signed `SealedDeliveryKeyRecord`);
2. `SealedDeliveryEnvelope`;
3. `MessageRequest`;
4. `IntroductionTicket` / signed discovery material needed by an offline invite;
5. signed `AsyncKeyBundle`;
6. encrypted-blob range request;
7. encrypted-blob range response;
8. bounded acknowledgement/error;
9. MLS `PrivateMessage`.

Unknown frame types are rejected for `LanDeliveryProfile-v1`; later optional semantics require a new profile/version rather than ambiguous partial parsing.

### Offline first contact

Purely offline first contact requires an authenticated QR/NFC/contact-card package that gives the requester the recipient's intro identity/signed ContactDescriptor plus a short-lived Invite with `lan_rendezvous` scope and a 32-byte LAN secret. LAN presence by itself is never sufficient.

After channel proof the recipient serves a current signed `IntroductionTicket` (and, if needed, a refreshed signed ContactDescriptor under the same authenticated intro identity) containing the normal RequestKey. The requester creates the ordinary W0 `MessageRequest`. Its `reply_route` is an ephemeral `om-lan-v1` RelayEntry and its `reply_delivery_key` is the normal fresh request-scoped `DeliveryKeyMaterial` bound to that route. The recipient applies the normal IntroGate/admission/payload-signature rules. Acceptance still requires the normal reciprocal stable-identity binding.

After acceptance, devices may serve normal signed `AsyncKeyBundle`s through the protected local channel. LAN uses a distinct `provider_pool_id`; one-time prekeys are never reused across WAN/provider pools. Existing bundle signature, generation rollback/equivocation and DeviceCredential/ACS checks are unchanged.

### Encrypted files and backpressure

LAN transfer moves the same encrypted blob bytes named by the canonical BlobID and `ciphertext_digest`. It is pull/range based:

- a receiver requests `(BlobID, offset, length)` with `1 <= length <= 65536`;
- the sender returns only that ciphertext range and matching offset;
- at most four requests are outstanding per blob per channel;
- unsolicited blob bytes are rejected;
- the receiver validates total bounds and the canonical ciphertext digest before plaintext release.

Receiver demand therefore supplies backpressure without a new streaming authority protocol. Local receipt still is not configured StoragePool durability; later backfill copies the same ciphertext/BlobID.

### Verification boundary

The design item is closed, but `local_delivery` remains locked behind V-003, V-015 and V-024. V-024 must still provide the exact CDDL/numeric fields and two-codec vectors, passive-observer and first-contact mutation models, native/desktop TLS-exporter feasibility, an explicit PWA capability result, and pre-release real-LAN interop between independent implementations.

