# OM Home API v1

- **Status:** Normative W0 interface freeze (ADR-0133), amended by ADR-0138 and ADR-0140; additive RecoveryStore extension (section 8, ADR-0201)
- **API major:** 1
- **Encoding:** canonical deterministic CBOR for protocol bodies; opaque ciphertext remains bytes
- **Transport:** any authenticated byte transport satisfying `NetworkTransport`; HTTPS is the reference binding

## 1. Authority model

A Home is a replaceable bundle of provider capabilities, never account authority. Requests use the narrowest
operation-scoped capability available. AccountID is not a bearer credential. A provider may co-host roles, but each
role exposes only its declared namespace and metadata descriptor.

Every response carries `api_major=1`; additive behavior is advertised through `ProviderCapabilities.features`.
Unknown major versions fail with `unsupported_version`. Unknown optional feature ids are ignored; unknown required
feature ids fail closed.

## 2. Request authentication classes

| Class | Used for | Rule |
|---|---|---|
| `deposit_cap` | relay deposit | unguessable scoped capability; no account identity required |
| `receive_cap` | drain/ack/rotate | scoped receive capability bound to queue generation |
| `relationship_cap` | key bundle fetch / per-contact operations | minted to an accepted relationship, revocable independently |
| `relay_admin_cap` | mint/revoke deposit capabilities for one mailbox/queue family | provider-resource capability; not AccountID/DeviceCredential authority |
| `directory_publish_cap` | publish/replace one descriptor namespace entry | provider-resource capability; clients remain authoritative for descriptor/account validation |
| `key_publish_cap` | publish/rotate one device's provider-local key pool | provider-resource capability; does not authorize the device in AccountControlState |
| `recovery_record_read_cap` | fetch one encrypted RecoveryRecord | derived from RecoveryRoot for one RecoveryStore scope; ciphertext-read only |
| `recovery_record_write_cap` | put/replace one encrypted RecoveryRecord | separately derived from RecoveryRoot; ciphertext-write only |
| `recovery_auth` | recovery-request store/fan-out | valid signed RecoveryRequest/RecoveryPolicy proof; never provider discretion |
| `sponsor_cap` | sponsored mailbox redemption | single-use, no-chain sponsorship capability |
| `blob_cap` | encrypted blob put/get | object-scoped random capability; provider never receives blob key |
| `blob_repair_cap` | repair one explicitly scoped BlobLocationRecord set | provider-resource capability; not account/controller authority |
| `public_read` | explicitly public descriptor/capability docs | no secret auth; rate limits allowed |

## 3. Operations

Routine provider administration uses **resource-scoped opaque capabilities**, not AccountControlState or
DeviceCredential credentials (ADR-0138). The capability proves authority only over the provider resource named by its
class. Reusing a capability can let that provider correlate operations on the same resource; it never becomes a
global device/account identifier and never counts as protocol identity authority. Canonical signed objects are still
validated end-to-end by clients.

Reference HTTP paths are bindings, not semantic authority.

| Operation | Reference binding | Auth | Request/response body | Required failures |
|---|---|---|---|---|
| relay deposit | `POST /v1/relay/deposit` | `deposit_cap` | `RelayDepositRequest -> RelayDepositReceipt` | quota, expired, revoked, payload_too_large, retryable |
| relay drain | `POST /v1/relay/drain` | `receive_cap` | `RelayDrainRequest -> RelayDrainResponse` | moved, revoked, retryable |
| relay ack | `POST /v1/relay/ack` | `receive_cap` | `RelayAckRequest -> Empty` | not_found, stale_generation |
| relay rotate | `POST /v1/relay/rotate` | `receive_cap` | `RelayRotateRequest -> RelayRotateResponse` | revoked, retryable |
| mint deposit capability | `POST /v1/relay/deposit-capabilities` | `relay_admin_cap` | `MintDepositCapabilityRequest -> DepositCapabilityRecord` | unauthorized, quota |
| revoke deposit capability | `DELETE /v1/relay/deposit-capabilities/{id}` | `relay_admin_cap` | idempotent | not_found is success-equivalent |
| publish descriptor | `PUT /v1/discovery/descriptors/{id}` | `directory_publish_cap` | signed ContactDescriptor | stale_generation, invalid_signature |
| fetch descriptor | `GET /v1/discovery/descriptors/{id}` | public/opaque locator policy | signed ContactDescriptor | not_found, moved |
| IntroGate ticket | `POST /v1/intro/tickets` | recipient-controlled capability | `IntroTicketRequest -> IntroductionTicket` | quota, policy_denied |
| submit MessageRequest | `POST /v1/intro/requests` | ticket/admission proof | `MessageRequest` | admission_denied, spent_token, payload_too_large, expired |
| drain MessageRequests | `POST /v1/intro/drain` | recipient capability | opaque accepted queue | retryable |
| key bundle publish | `PUT /v1/keys/bundles/{device}` | `key_publish_cap` | signed AsyncKeyBundle | stale_generation, invalid_signature |
| key bundle fetch | `POST /v1/keys/fetch` | `relationship_cap` | requested DeviceID set -> bundles | unauthorized, revoked, not_found |
| key generation revoke | `DELETE /v1/keys/bundles/{device}/generations/{generation}` | `key_publish_cap` | idempotent best-effort deletion | not_found is success-equivalent |
| sponsorship redeem | `POST /v1/sponsorship/redeem` | `sponsor_cap` | `SponsoredMailboxGrant` | spent_token, expired, quota |
| RecoveryRecord put | `PUT /v1/recovery/records/{opaque_locator}` | `recovery_record_write_cap` | `RecoveryRecordPutRequest -> Empty` | unauthorized, quota |
| RecoveryRecord get | `GET /v1/recovery/records/{opaque_locator}` | `recovery_record_read_cap` | `RecoveryRecordGetRequest -> RecoveryRecordGetResponse` | unauthorized, not_found, retryable |
| RecoveryRequest fan-out | `POST /v1/recovery/requests` | `recovery_auth` | signed RecoveryRequest | invalid_signature, stale_generation, rate_limited |
| blob put | `POST /v1/blobs` | `blob_cap` | encrypted bytes + policy -> locator | quota, payload_too_large |
| blob get | `GET /v1/blobs/{opaque}` | `blob_cap` | encrypted bytes | not_found, expired |
| blob location repair | `POST /v1/blobs/repair` | `blob_repair_cap`/relationship policy | signed BlobLocationRecord set | invalid_signature, stale_generation |
| moved/forwarding lookup | normal fetch response | as original op | typed `moved` error with signed successor locator/proof | moved |
| capabilities | `GET /.well-known/om-provider-capabilities` | public | ProviderCapabilities JSON/CBOR | retryable |

Providers may expose additional endpoints only as advertised optional capabilities. A core client never branches on a
vendor name.

### 3.1 Bearer-capability transport hygiene (ADR-0141)

Every `*_cap`/relationship/recovery provider capability in this API is a bearer secret. It is carried only through an
authenticated encrypted transport. For HTTP, a client uses the `Authorization` header or the operation's frozen
encrypted/authenticated CBOR authorization slot; bearer capability bytes MUST NOT appear in a URL path, query string,
fragment or cookie. A browser-facing implementation uses `Referrer-Policy: no-referrer`, does not forward credentials
through cross-origin redirects, and marks authenticated/private responses `Cache-Control: no-store`. Equivalent
non-HTTP transports MUST provide the same no-cache/no-forward semantics.

Providers, gateways and clients redact bearer capability bytes from ordinary access/application/proxy logs, analytics,
telemetry, crash reports and diagnostics. A debugging identifier, if needed, is a non-reversible deployment-local
fingerprint that is not accepted as authorization. Opaque locators such as a hash of a read capability are not bearer
credentials, though they remain privacy-sensitive metadata.

### 3.2 Provider-resource capability rules

Provider-resource capabilities are at least 256 bits of CSPRNG/approved-KDF output, scoped to one provider and one
resource/operation family, independently rotatable, and never reused across unrelated providers or roles. A provider
may bind them to its local resource record, but must not require AccountID or DeviceCredential merely to service that
resource.

A provider that stores a signed object is not the authority that makes the object valid. Clients reject stale,
revoked, misbound or invalidly signed descriptors/bundles even if a provider accepted the write.

For RecoveryRecord storage, `opaque_locator = DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, recovery_record_read_cap)`.
The read and write capabilities are independently derived from RecoveryRoot and the provider's advertised
`recovery_scope_id` using the domain-separation labels in [`om-domain-separation.json`](../registries/om-domain-separation.json). Neither value contains or is
derived from AccountID.

The encrypted RecoveryRecord v1 carries authenticated `record_generation`, `recovery_epoch` and
`last_known_acs_head`. A provider may reject an obviously lower generation it has seen, but client-held monotonic
high-water state is the rollback boundary. The snapshot is advisory/potentially stale: after recovery, clients
revalidate provider routes/capabilities/descriptors and validate ACS state through the normal account-control chain.
The provider never upgrades a RecoveryRecord observation into account authority.

### 3.3 KeyMaterialProvider freshness and privacy (ADR-0140)

`AsyncKeyBundle` v1 carries no `acs_generation` or `acs_state_hash`; retired numeric keys 3 and 4 are rejected and never reused. The provider therefore cannot require those account-wide values to store or fetch a bundle. The fetcher authenticates the bundle's `DeviceCredential` against the **relationship-pinned trusted peer ACS**.

The signed-prekey `generation` is one monotonic namespace per `(DeviceID, CryptoProfile)` across **all** provider pools. A provider MAY reject a publish lower than the highest generation it has locally seen, but that check is only an availability/operational aid. The recipient's durable `(peer AccountID, peer DeviceID, CryptoProfile) -> (highest_generation, generation_core_hash)` state is the rollback boundary. Moving to another provider, changing `provider_pool_id`, or invoking the generation-delete operation MUST NOT reset or lower that floor.

The provider may hold different one-time-prekey inventories and expiry times for the same signed-prekey generation at different pools. One-time prekeys are device-signed over the ADR-0140 context including `provider_pool_id`; copying an item to another pool does not create a valid item there. The generation-delete operation is best-effort cleanup after rotation/compromise and is never evidence that an older generation is safe to accept.

## 4. Error behavior

Errors are structured `HomeError` values from [`spec/registries/om-home-errors.json`](../registries/om-home-errors.json). A client may retry only errors
whose registry entry says `retryable=true`. `moved` is never followed blindly: its signed forwarding/continuity proof
is verified before route replacement. `quota` includes whether the limit is hard-free-tier or potentially billable.
Unknown error codes are treated as non-retryable protocol failure.

## 5. Idempotency and replay

Mutating requests carry a random 128-bit `request_id`. Providers retain enough recent operation ids to make duplicate
requests idempotent for the operation's advertised retry horizon **across provider process restart**. A duplicate with
the same `request_id` and same authenticated operation fingerprint returns the already-determined semantic outcome or
resumes the same operation; the same `request_id` with different operation bytes/context is `invalid_request`. A client
that times out after transmitting a mutation treats the outcome as unknown and exact-retries/reconciles with the same
`request_id`; it does not mint a second semantic mutation merely because the response was lost.

Idempotency never converts an invalid authorization into a valid one. Deposit/message/blob payload ids are content- or
sender-defined opaque ids and do not expose AccountID. Operation-specific rules may require stronger replay identities
(for example sealed-delivery tuples); those stronger rules remain authoritative.

## 6. Provider portability

All durable provider-held protocol objects are signed/encrypted so another conforming Home can store them without
reinterpretation. Migration changes locators/capabilities, not AccountID, ConversationID, authority or cryptographic
floors. A Home may return `moved` only with cryptographically verifiable replacement information appropriate to the
object type.

## 7. Conformance ownership

[`docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md`](../../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) targets this API. V-020 must exercise request-auth
separation, typed error handling, migration, sponsor overlap, quota semantics and a non-cloud second implementation
before `provider_reference` is authorized.

## 8. RecoveryStore extension (ADR-0201, feature `om.recovery.store.v1`)

Additive: no row or text above changes, and a Home that does not advertise the feature is unaffected. The feature is advertised in
`ProviderCapabilities.features`, together with `recovery_scope_id` and the limits of section 8.4 in `limits`.

### 8.1 Binding of the three frozen RecoveryStore operations

| Operation | Binding detail added |
|---|---|
| RecoveryRecord put | `PUT /v1/recovery/records/{opaque_locator}`. `opaque_locator` is the 32-byte digest value of `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`, base64url without padding. The `recovery_record_write_cap` is `Authorization: Bearer <base64url>` (section 3.1). The body is the frozen `RecoveryRecordPutRequest`; its bytes are a `RecoveryRecordEnvelope` (type 35), at most 1,048,576 bytes, checked before the body is decoded. **Create** (no record at the locator) sends `If-None-Match: *`, needs the Home's owner admission as any other resource creation does, and binds `SHA-256(write_cap)` to the locator. **Replace** sends `If-Match: "<base64url SHA-256 of the stored envelope bytes>"` with the bound write capability; a mismatch is `stale_generation`. Failures: `unauthorized`, `quota`, `payload_too_large`, `stale_generation`, `rate_limited`, `admission_denied`. |
| RecoveryRecord get | `GET /v1/recovery/records/{opaque_locator}`, no HTTP body (its `request-id` is the optional `OM-Request-Id` header). The `recovery_record_read_cap` is `Authorization: Bearer`; the Home checks that its hash equals the locator. The response is the frozen `RecoveryRecordGetResponse` with an `ETag` of the same value `If-Match` takes. Failures: `unauthorized`, `not_found`, `rate_limited`, `retryable`. A wrong capability and an unknown locator are answered alike (`not_found`). |
| RecoveryRecord delete (new) | `DELETE /v1/recovery/records/{opaque_locator}`, `recovery_record_write_cap`, idempotent (`not_found` is success-equivalent). Used after custody rotation. |
| RecoveryRequest fan-out | `POST /v1/recovery/requests`, body the frozen `RecoveryRequestFanout`. See 8.2. |

### 8.2 Recovery inbox, fan-out and veto board

A **recovery inbox** is a bounded store of signed recovery objects for one account at one Home. Its index is
`inbox_id = DS-HASH(OM-RECOVERY-INBOX-ID-v1, account_id)` (digest value, base64url). It is read only with a `recovery_inbox_cap`: 32 random bytes minted by a
controller device, held by the account's controller devices (in the RecoveryKit), and **not** derived from RecoveryRoot, so a RecoveryRoot holder cannot read it.
The capability is a provider-resource capability (section 3.2): it proves nothing about the account.

| Operation | Reference binding | Auth | Body | Failures |
|---|---|---|---|---|
| inbox register | `PUT /v1/recovery/inbox/{inbox_id}` | `recovery_inbox_cap` in body key 2; creation also needs owner admission | `RecoveryInboxRegisterRequest -> Empty` | `unauthorized`, `quota`, `admission_denied` |
| fan-out | `POST /v1/recovery/requests` | `recovery_auth` (see below) | `RecoveryRequestFanout -> Empty` | `invalid_request`, `payload_too_large`, `rate_limited` |
| veto post | `POST /v1/recovery/vetoes` | `recovery_auth` (see below) | `RecoveryVetoPostRequest -> Empty` | `invalid_request`, `payload_too_large`, `rate_limited` |
| inbox drain | `POST /v1/recovery/inbox/{inbox_id}/drain` | `recovery_inbox_cap` | `RecoveryInboxDrainRequest -> RecoveryInboxDrainResponse` | `unauthorized`, `rate_limited` |
| inbox ack | `POST /v1/recovery/inbox/{inbox_id}/ack` | `recovery_inbox_cap` | `RecoveryInboxAckRequest -> Empty` | `unauthorized`, `not_found` is success-equivalent |
| veto read | `GET /v1/recovery/requests/{request_digest}/vetoes` | `public_read` | `RecoveryVetoListResponse` | `rate_limited` |

**`recovery_auth` here is a shape check, not trust.** The Home holds no recovery key and reads no account state (it stays a dumb store). It decodes
the signed object's outer envelope and its body only to check type, version, sizes and the `account_id` (fan-out and veto post) and, for a veto,
`request_digest`; it never counts a signature as valid and never acts on one. Every consumer verifies the signature against its own pinned
`AccountControlState` and `RecoveryPolicy`, so a forged object can fill space but never authorize anything.

- **Fan-out** appends the signed request to the inbox of `account_id` if one exists, de-duplicated by `SHA-256` of the request body inside the envelope; a duplicate is accepted and changes nothing.
  The Home answers identically whether or not an inbox exists (no account enumeration), and the answer carries no time.
- **Veto post** is accepted only when the inbox holds a request whose digest equals the veto's `request_digest`; otherwise it is answered like a success and dropped.
- **Drain** returns items in arrival order, each as `{item_id, kind, bytes}` with **no timestamp** (a Home is never a time source, ADR-0131). **Ack** removes by `item_id`.
- **Veto read** returns the stored vetoes for one request digest to anyone who knows the digest (it is a hash of the request, held by the recovering device and by every controller), at most 16.

Nothing the inbox holds is trusted, and nothing in it is needed for safety: a contact or controller that never sees a stored object still keeps its pinned head
and waits its own interval (ADR-0190). The inbox is a convenience path for live controllers, with the in-session and contact paths of ADR-0190 unchanged.

### 8.3 Metadata

The Home learns what it learns for an ordinary mailbox, plus: the locator and capability hashes of a record, the record size **bucket** (four values, section 8.4), the times of writes,
`inbox_id`, and, at fan-out only, the `account_id` and the fact of a recovery attempt (declared `recovery_attempt`, never `no_by_design`). It stores no network address and no
`DeviceID`, returns no timestamp, and cannot read a record, a capability's owner or a contact.

### 8.4 Limits (implementation parameters, advertised in `ProviderCapabilities.limits`; never protocol constants)

| Parameter | Default profile | Personal profile |
|---|---|---|
| `max_recovery_record_bytes` | 1,048,576 | 1,048,576 |
| `max_recovery_records` | 16 | 4 |
| `max_recovery_inboxes` | 16 | 4 |
| record put per write capability | burst 6, refill 1 per 600 s | same |
| record get per locator | burst 10, refill 1 per 30 s | same |
| failed record get (wrong capability or unknown locator), whole Home | burst 30, refill 1 per 10 s | same |
| distinct requests per inbox, rolling 24 h | 3 | 3 |
| pending items per inbox | 8 requests, 32 vetoes (at most 16 per request digest) | same |
| veto posts per inbox, rolling hour | 8 | 8 |
| fan-out and veto posts, whole Home | burst 10, refill 1 per 60 s | same |
| inbox drain per inbox | burst 20, refill 1 per 5 s | same |
| veto read per request digest | burst 10, refill 1 per 10 s | same |
| signed request or veto, bytes | 16,384 (the registry maximum of types 4 and 5) | same |
| inbox item retention | 14 days (`maxRetentionSeconds`) | same |
| record retention without a replace or refresh | 400 days | same |
| inbox retention without a register | 400 days | same |

A full inbox, or one that already took its distinct requests of the day or its vetoes of the hour, **keeps what it holds and drops the new item, answered like a success**, so that a poster learns nothing about whether an inbox exists; it never evicts an item it already holds. Only the Home-wide limits answer `rate_limited` (with `retry_after_seconds`), because they do not depend on an inbox.
