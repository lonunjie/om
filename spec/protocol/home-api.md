# OM Home API v1

- **Status:** Normative W0 interface freeze (ADR-0133)
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
| `device_auth` | publish own descriptor/bundles/recovery records | signature by an active non-quarantined DeviceCredential plus referenced ACS head |
| `recovery_auth` | recovery-request store/fan-out | valid RecoveryRequest/RecoveryPolicy proof; never provider discretion |
| `sponsor_cap` | sponsored mailbox redemption | single-use, no-chain sponsorship capability |
| `blob_cap` | encrypted blob put/get/repair | object-scoped random capability; provider never receives blob key |
| `public_read` | explicitly public descriptor/capability docs | no secret auth; rate limits allowed |

## 3. Operations

Reference HTTP paths are bindings, not semantic authority.

| Operation | Reference binding | Auth | Request/response body | Required failures |
|---|---|---|---|---|
| relay deposit | `POST /v1/relay/deposit` | `deposit_cap` | `RelayDepositRequest -> RelayDepositReceipt` | quota, expired, revoked, payload_too_large, retryable |
| relay drain | `POST /v1/relay/drain` | `receive_cap` | `RelayDrainRequest -> RelayDrainResponse` | moved, revoked, retryable |
| relay ack | `POST /v1/relay/ack` | `receive_cap` | `RelayAckRequest -> Empty` | not_found, stale_generation |
| relay rotate | `POST /v1/relay/rotate` | `receive_cap` | `RelayRotateRequest -> RelayRotateResponse` | revoked, retryable |
| mint deposit capability | `POST /v1/relay/deposit-capabilities` | `device_auth` | `MintDepositCapabilityRequest -> DepositCapabilityRecord` | unauthorized, quota |
| revoke deposit capability | `DELETE /v1/relay/deposit-capabilities/{id}` | `device_auth` | idempotent | not_found is success-equivalent |
| publish descriptor | `PUT /v1/discovery/descriptors/{id}` | `device_auth` | signed ContactDescriptor | stale_generation, invalid_signature |
| fetch descriptor | `GET /v1/discovery/descriptors/{id}` | public/opaque locator policy | signed ContactDescriptor | not_found, moved |
| IntroGate ticket | `POST /v1/intro/tickets` | recipient-controlled capability | `IntroTicketRequest -> IntroductionTicket` | quota, policy_denied |
| submit MessageRequest | `POST /v1/intro/requests` | ticket/admission proof | `MessageRequest` | admission_denied, spent_token, payload_too_large, expired |
| drain MessageRequests | `POST /v1/intro/drain` | recipient capability | opaque accepted queue | retryable |
| key bundle publish | `PUT /v1/keys/bundles/{device}` | `device_auth` | signed AsyncKeyBundle | stale_generation, invalid_signature |
| key bundle fetch | `POST /v1/keys/fetch` | `relationship_cap` | requested DeviceID set -> bundles | unauthorized, revoked, not_found |
| sponsorship redeem | `POST /v1/sponsorship/redeem` | `sponsor_cap` | `SponsoredMailboxGrant` | spent_token, expired, quota |
| RecoveryRecord put | `PUT /v1/recovery/records/{account_ref}` | `device_auth` | encrypted RecoveryRecord | stale_generation, quota |
| RecoveryRequest fan-out | `POST /v1/recovery/requests` | `recovery_auth` | signed RecoveryRequest | invalid_signature, stale_generation, rate_limited |
| blob put | `POST /v1/blobs` | `blob_cap` | encrypted bytes + policy -> locator | quota, payload_too_large |
| blob get | `GET /v1/blobs/{opaque}` | `blob_cap` | encrypted bytes | not_found, expired |
| blob location repair | `POST /v1/blobs/repair` | `device_auth`/relationship policy | signed BlobLocationRecord set | invalid_signature, stale_generation |
| moved/forwarding lookup | normal fetch response | as original op | typed `moved` error with signed successor locator/proof | moved |
| capabilities | `GET /.well-known/om-provider-capabilities` | public | ProviderCapabilities JSON/CBOR | retryable |

Providers may expose additional endpoints only as advertised optional capabilities. A core client never branches on a
vendor name.

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
