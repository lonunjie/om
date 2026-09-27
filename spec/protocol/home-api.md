# OM Home API v1

- **Status:** Normative W0 interface freeze (ADR-0133), amended by ADR-0138 and ADR-0140
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
requests idempotent for the operation's advertised retry horizon. Idempotency never converts an invalid authorization
into a valid one. Deposit/message/blob payload ids are content- or sender-defined opaque ids and do not expose
AccountID.

## 6. Provider portability

All durable provider-held protocol objects are signed/encrypted so another conforming Home can store them without
reinterpretation. Migration changes locators/capabilities, not AccountID, ConversationID, authority or cryptographic
floors. A Home may return `moved` only with cryptographically verifiable replacement information appropriate to the
object type.

## 7. Conformance ownership

[`docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md`](../../docs/architecture/providers/PROVIDER_CONFORMANCE_SUITE.md) targets this API. V-020 must exercise request-auth
separation, typed error handling, migration, sponsor overlap, quota semantics and a non-cloud second implementation
before `provider_reference` is authorized.
