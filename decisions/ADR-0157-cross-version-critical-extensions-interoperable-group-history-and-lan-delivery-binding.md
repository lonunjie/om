# ADR-0157 — Cross-version critical extensions, interoperable group history and concrete LAN delivery binding

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** independent follow-up review of ADR-0155/0156; design/specification only, no product engineering authorization
- **Amends:** ADR-0155 §1, ADR-0156 §1 and §4, ADR-0023, ADR-0150 LAN binding status
- **Closes:** A-055 (`local_delivery` design item)
- **Does not close:** V-024 or any independent-review / runtime / interop evidence gate

## Context

Three semantic issues remained after the 2026-09-28 independent review.

1. **Future critical extensions still had a registry-version convergence hole.** ADR-0155 correctly made malformed non-critical registrations registry-version independent, but explicitly left a residual for a future critical registration sent with `critical=false`: an old client would see an unknown non-critical extension and accept/ignore it, while a newer client that knows the registration would reject a criticality mismatch. That is avoidable before W0 ships.
2. **ADR-0156 conflated two different kinds of history.** Account-private device sync may legitimately be implementation-owned and namespaced. A Room `HistoryBundle`, however, is sent by one group member to a newcomer who may run a different conforming client. Making that bundle implementation-private contradicts the group interoperability model and the existing `conformance:group_history` obligation.
3. **A-055 was concrete enough to resolve without inventing new authority or cryptography.** The LAN profile can reuse the already-frozen `RelayEntry`, route-binding and sealed-delivery constructions, standard TLS 1.3 channel binding, existing MessageRequest/IntroGate objects, existing AsyncKeyBundle validation and canonical BlobID/ciphertext semantics. The remaining work is verification, not architecture choice.

## Decisions

### 1. Future critical extensions have a wire-visible critical namespace

For key-23 extension IDs, OM-CBOR-v1 reserves a syntactic critical class:

- every **new** extension registration with `critical: true` after the W0 registry freeze MUST use an extension ID whose first character is `!`;
- any decoder conforming to OM-CBOR-v1 treats an **unknown** extension ID beginning with `!` as critical and rejects the containing object, regardless of the `ExtEntry.critical` boolean;
- a registered `!` extension with `critical=false` on the wire is rejected by the existing registered-criticality mismatch rule (`value_range` at the strict OM-CBOR schema layer; presentation-extension evidence may label the semantic condition `criticality_mismatch`);
- an ID beginning with `!` can never be registered non-critical;
- `om.acs.fork_ack` and `om.acs.legacy_policy` are the only grandfathered critical IDs outside this namespace. They are already in the W0 registry snapshot, so every W0 decoder knows their criticality.

The existing `critical` boolean remains in `ExtEntry` for explicitness and current encodings. The `!` class is a second, registry-independent fail-closed signal for future critical semantics. A sender cannot make a future critical extension look non-critical to an old client merely by clearing one boolean.

This rule does **not** make an unknown `!` extension interpretable; it only makes the safe verdict stable across registry versions. A critical feature still needs its normal capability/profile/version rules before use.

### 2. Separate account-private history sync from Room HistoryBundle interoperability

ADR-0156 §4 is narrowed to **account-private sync records only**.

- Drafts, private preferences and same-account history snapshots MAY use an implementation/codec identifier inside authenticated E2EE framing. Unknown records are ignored whole. Cross-implementation synchronization of those private records is not a V1 interoperability promise.
- The implementation-private same-account history object is called **`AccountHistorySnapshot`**. The name `HistoryBundle` is reserved for the Room/group feature described by [`GROUP_CHAT_ARCHITECTURE.md`](../docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md).
- A Room **`HistoryBundle` is an interoperable group protocol object**. It MUST NOT depend on an implementation identifier for interpretation. Before the `groups` scope can be authorized, V-004's ADR-0156 wire-freeze criterion MUST include HistoryBundle: frozen CDDL, registered fields/labels as applicable, positive/negative vectors, and two independent codecs.
- Existing provenance rules remain: the newcomer treats the bundle author as the source of the shared history and never upgrades claimed historical senders into authenticated authorship; expired/deleted/redacted content is excluded; `history_policy` remains cooperative policy.

This preserves implementation freedom for private local/device state without fragmenting an interoperable Room feature.

### 3. A-055 LAN rendezvous profile

The concrete V1 `LanDeliveryProfile` is W1 `local_delivery` and reuses existing OM authority/security objects. A LAN route is transport, never identity or relationship authority.

#### 3.1 Rendezvous secrets and epochs

Two secret sources are permitted:

1. **Established relationship:** when the user enables same-LAN discovery for a contact, each advertising recipient device creates a random 32-byte `LAN-Rendezvous-Secret` scoped to that accepted relationship and that recipient DeviceID. It is sent only through already-authenticated relationship E2EE state, may be rotated at any time, and is destroyed/retired on device revocation, block or relationship end. It is routing metadata, not account/device authority.
2. **QR/invite first contact:** an offline-capable Invite with scope `lan_rendezvous` uses `Invite.lan_rendezvous` as exactly 32 random bytes. It is short-lived with the Invite and gives no relationship or device eligibility by itself.

The rendezvous epoch is `floor(unix_time / 900)` (15 minutes). Wall time is only a discovery/liveness input, never authorization or message ordering authority. A matcher may compute the current and adjacent epoch (`e-1`, `e`, `e+1`) for ordinary clock skew, but a route established from an adjacent epoch still receives the route's own bounded expiry below.

For every advertisement cycle the device creates a fresh 16-byte `link_nonce`; it MUST change at every epoch and whenever the link-layer address changes. The 16-byte tag is:

```text
Trunc128(HMAC-SHA-256(
  LAN-Rendezvous-Secret,
  OM-CBOR ["OM-LAN-RENDEZVOUS-TOKEN-v1", epoch, link_nonce]
))
```

The label and construction are registered in [`om-domain-separation.json`](../spec/registries/om-domain-separation.json). Raw Double Ratchet/PQXDH message keys are never used as rendezvous keys.

#### 3.2 Fixed-cardinality advertisement

`LanPrivacyPolicy-v1` fixes **K = 8 advertisement slots per epoch per advertising endpoint**. Enabled real tags occupy slots and random 16-byte decoys fill the rest. If more than eight grants are enabled, a fair local scheduler rotates which grants receive slots in successive epochs; changing the number of contacts never changes the advertised slot count. A query/response discovery substrate may avoid unsolicited tag sets entirely, but if it emits a tag set it uses the same K=8 privacy profile.

A discovery record may expose only the current `link_nonce`, one tag slot, an ephemeral transport endpoint and transport capability token needed to open the local channel. It carries no AccountID, DeviceID, username, device name, stable hostname, ConversationID or stable certificate/public-key fingerprint.

#### 3.3 Channel confidentiality and secret binding

The native/desktop reference profile uses **TLS 1.3** with an ephemeral certificate/key that is regenerated at least once per rendezvous epoch and on link-layer address change. Ordinary public-PKI identity is not required; OM authenticates the local channel by binding possession of the rendezvous secret to the TLS channel.

After the TLS 1.3 handshake, each endpoint obtains 32 bytes from the RFC 8446 TLS exporter using exporter label `OM-LAN-TLS-EXPORTER-v1`. Before any identity-bearing OM object is exchanged, each side sends a role-separated proof:

```text
HMAC-SHA-256(
  LAN-Rendezvous-Secret,
  OM-CBOR ["OM-LAN-CHANNEL-PROOF-v1", role, epoch, link_nonce, tls_exporter]
)
```

Both proofs MUST verify. Because the proof includes the TLS exporter, an active relay that terminates two different TLS connections cannot simply forward proofs and read the protected route/key material. Failure closes the channel before route offers, DeviceIDs, delivery-key records, tickets or bundles are sent.

A PWA/browser advertises `local_delivery=unavailable` unless its PlatformAdapter can provide an equivalently channel-bound direct-local transport without changing OM semantics. V1 does not invent weaker browser-only authentication to claim parity.

#### 3.4 Local route and sealed 1:1 delivery

After channel proof, an established recipient device creates a fresh ephemeral local route using the **existing frozen `RelayEntry` shape**:

- `relay_uri = om-lan-v1://<base64url(16 random route_token)>`; the URI is a logical per-route handle, not an IP address or identity;
- `deposit_capability` is fresh random 32-byte capability material;
- transport list contains `lan-tls13-v1`;
- `expires_at` is bounded to the current rendezvous epoch plus at most 120 seconds of drain/skew;
- the route token/capability are replaced on every new epoch and every link-layer-address change.

The recipient creates a **fresh ordinary `SealedDeliveryKeyRecord`** for that exact RelayEntry. Its existing `route_binding = SHA-256(OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability])` rule is unchanged. The delivery key expires no later than the route and is not reused for another LAN route. The route offer (RelayEntry + signed SealedDeliveryKeyRecord) is sent only after the secret-bound TLS channel is established, so the record's recipient DeviceID and delivery-key identifier are not visible to a passive LAN observer.

A sender then transmits the exact canonical `SealedDeliveryEnvelope` already defined by ADR-0142. The bare inner 1:1 envelope is forbidden on LAN. Existing replay reservation, expiry, RelationshipContext, ACS/DeviceCredential, PQXDH/ratchet and EventID rules are unchanged. Moving between WAN and LAN requires a fresh route-specific outer seal but materializes the same logical EventID once.

Group LAN delivery carries MLS `PrivateMessage` bytes inside the same secret-bound local channel. It does not use 1:1 sealed-delivery keys and gains no new membership authority from LAN reachability.

#### 3.5 Framing

The LAN transport is a bounded record stream. Each record is:

```text
uint32_be(frame_length) || canonical_OM_CBOR(frame)
```

`frame_length` MUST be non-zero and at most 163840 bytes. The W1 frame schema is frozen by V-024 before `local_delivery` authorization. Its closed V1 frame-type set is:

1. route offer (RelayEntry + signed SealedDeliveryKeyRecord);
2. SealedDeliveryEnvelope;
3. MessageRequest;
4. IntroductionTicket / signed discovery material needed by an offline invite;
5. signed AsyncKeyBundle;
6. encrypted-blob range request;
7. encrypted-blob range response;
8. bounded acknowledgement/error;
9. MLS PrivateMessage.

Unknown frame types are rejected for `LanDeliveryProfile-v1`; future optional semantics use a new profile/version rather than ambiguous partial parsing. Identity-bearing payloads remain inside the secret-bound TLS channel.

#### 3.6 Offline first contact and async key material

A purely offline first contact is allowed only from an authenticated QR/NFC/contact-card package that gives the requester the recipient's intro identity / signed ContactDescriptor plus a short-lived Invite containing the 32-byte LAN rendezvous secret. LAN presence alone is never enough.

After the TLS channel proof:

1. the recipient's local endpoint serves a current signed IntroductionTicket (and, if needed, refreshed signed ContactDescriptor under the same authenticated intro identity) containing the normal RequestKey;
2. the requester creates the ordinary W0 MessageRequest. Its `reply_route` is an ephemeral `om-lan-v1` RelayEntry for the requester and its `reply_delivery_key` is the existing fresh request-scoped DeliveryKeyMaterial bound to that exact route;
3. the recipient runs the ordinary IntroGate/request admission, payload signature, descriptor and acceptance rules. LAN reachability creates no relationship or device eligibility;
4. after acceptance and stable-identity binding, devices may serve normal signed AsyncKeyBundles through the protected LAN channel. A device uses a distinct LAN `provider_pool_id`; one-time prekeys are never reused across WAN/provider pools. Bundle signature, generation rollback/equivocation and DeviceCredential/ACS validation are identical to any other source.

The first-contact response uses the existing sealed bootstrap path and the requester's LAN reply route. Exactly-one logical bootstrap response, candidate budget, expiry and crash/replay rules from ADR-0144–0147 remain unchanged.

#### 3.7 Encrypted file transfer and backpressure

LAN file transfer moves the same encrypted blob bytes identified by the canonical BlobID and `ciphertext_digest`; it never creates a LAN-specific plaintext/file identity.

The V1 transfer is **pull/range based**:

- the receiver requests `(BlobID, offset, length)` with `1 <= length <= 65536`;
- the sender returns only that ciphertext range plus the matching offset;
- at most four range requests may be outstanding per blob per channel;
- unsolicited blob payload is rejected;
- the receiver verifies total bounds and the canonical ciphertext digest before plaintext release;
- local receipt remains distinct from configured StoragePool durability, and later provider backfill copies the same ciphertext/BlobID rather than rewriting it.

This makes receiver demand the backpressure mechanism and bounds memory/work without a new streaming authority protocol.

### 4. Verification boundary

A-055 is resolved by the profile above. **V-024 remains open** and must still provide:

- frozen CDDL/numeric-field definitions and positive/negative two-codec vectors for the LAN frame/route-offer/blob-range structures;
- a passive-observer model proving no clear sender or stable LAN-visible OM identifier, including epoch and link-address rotation mutations;
- offline-first-contact traces proving LAN reachability never grants relationship/device authority and AsyncKeyBundle rules are unchanged;
- a native/desktop feasibility run of the TLS-exporter channel binding;
- an explicit PWA capability result (`available` only if an equivalent channel-bound direct-local transport exists; otherwise `unavailable`);
- pre-release real-LAN interop between independent implementations.

Closing A-055 therefore changes `design_open` from 1 to 0 but does not authorize `local_delivery` or any other engineering scope.

## Consequences

- Future critical-extension validity is registry-version independent even when a hostile sender clears the boolean.
- Group history remains a real interoperable Room feature, while account-private state keeps implementation freedom.
- LAN delivery now has one concrete architecture with no new identity/authority source and no custom cryptographic primitive: existing sealed delivery, standard TLS 1.3 exporter binding, MessageRequest/IntroGate, AsyncKeyBundle validation and canonical encrypted blobs are reused.
- PWA limitations are stated honestly instead of weakening the protocol for browser parity.
- `design_open=0`; verification and independent review remain the only blockers.
