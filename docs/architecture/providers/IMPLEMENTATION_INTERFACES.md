# Implementation Interface Sketches

These are conceptual interfaces, not frozen language APIs.

> **2026-09-24 (ADR-0130):** conceptual sketches are not enough for two independent Homes to interoperate. The
> normative **Home API** (operations, request authentication, CDDL request/response bodies, a typed error registry
> with at least `quota`, `moved` and `retryable`, and versioning) covering relay, IntroGate, descriptor publishing,
> sponsorship, recovery records and blob spool, a real ProviderCapabilities JSON Schema with `durability_class`, and
> ADR-0133 resolved **A-045**: the Home API, typed errors, ProviderCapabilities schema and formerly missing provider contracts are now frozen architecture inputs. V-020 still owns implementation/conformance evidence before `provider_reference` can authorize. The provider conformance runner targets that API.

```rust
trait CryptoProfileRegistry {
    fn allowed_profiles(&self) -> Vec<ProfileId>;
    fn preferred_profile(&self, peer_caps: &PeerCryptoCaps, floor: SecurityFloor) -> Result<ProfileId>;
    fn is_forbidden(&self, profile: ProfileId) -> bool;
}

trait RelayProvider {
    fn capabilities(&self) -> RelayCapabilities;
    fn create_receive_capability(&self, policy: QueuePolicy) -> ReceiveCapability;
    fn put(&self, target: DeliveryCapability, envelope: CiphertextEnvelope) -> DeliveryReceipt;
    fn subscribe(&self, receive: ReceiveCapability) -> Stream<CiphertextEnvelope>;
    fn ack(&self, token: DeliveryToken) -> Result<()>;
    fn rotate(&self, receive: ReceiveCapability) -> ReceiveCapability;
    fn usage_state(&self) -> ProviderUsageState;
}

trait BlobStore {
    fn put_ciphertext(&self, object: EncryptedBlob, policy: BlobPolicy) -> BlobLocator;
    fn get_ciphertext(&self, locator: BlobLocator) -> EncryptedBlob;
    fn delete(&self, locator: BlobLocator) -> Result<()>;
    fn usage_state(&self) -> ProviderUsageState;
}

trait MediaProvider {
    fn capabilities(&self) -> MediaCapabilities;
    fn turn_credentials(&self, scope: CallScope, ttl: Duration) -> ScopedTurnCredentials;
    fn create_sfu(&self, scope: CallScope) -> SfuSession;
    fn usage_state(&self) -> ProviderUsageState;
}

trait RoutePolicy {
    fn choose_call_route(&self, context: CallContext, candidates: Vec<RouteCandidate>) -> RouteDecision;
}

trait TransportAdapter {
    fn capabilities(&self, peer: &TransportPeer) -> TransportCapabilities;
    fn send(&self, semantic_event: CanonicalEvent) -> TransportResult;
}
```

## Negative-capability contracts (ADR-0137)

The positive method list is only half of a provider interface. The following powers are **not** ambient capabilities
of the role and must be absent from its ordinary credentials/key material/interface surface. This is a protocol
boundary, not a promise that an operator of open-source software cannot write unrelated software.

| Role | Must be structurally unable to do merely by holding the role |
|---|---|
| `RelayProvider` / mailbox Home | decrypt E2EE payloads; sign AccountControlState transitions; mint a recipient's referral issuer tokens; turn a queue/deposit capability into AccountID authority |
| `DirectoryProvider` | authorize controllers/devices; make an alias a permanent/global AccountID root; sign as the discovered account |
| `RecoveryStore` | independently authorize an AccountControlState transition; decrypt message-history backups; convert possession of a stored RecoveryRecord into controller authority |
| `IntroGate` | decrypt sealed MessageRequest plaintext; mint the recipient's issuer private-key output; derive a stable requester identifier from the W0 referral-token protocol |
| `BlobStore` / `StoragePool` | obtain blob content keys; derive canonical AccountID/ConversationID from an opaque blob locator by protocol design |
| `PushProvider` / gateway | obtain message plaintext/content keys; become account, relationship or conversation authority from an opaque wake handle |
| `CryptoProvider` | become a remote mandatory service or network trust root; W0 `CryptoProvider` is a local implementation boundary only |

Role colocation gives an operator the **union of observations and local service powers**, never additional cryptographic
protocol authority. Combining a directory, mailbox and recovery service does not satisfy an AccountControlState
threshold unless the operator separately possesses the required end-user controller/recovery credentials.

Conformance work should test these as negative cases by withholding forbidden keys/credentials and attempting the
prohibited operation. It must not phrase them as an impossible claim that a malicious host cannot run arbitrary
extra code.

### Provider-resource administration capabilities (ADR-0138)

Routine provider administration never requires the provider to validate an AccountControlState or receive a
DeviceCredential solely to decide whether the caller may edit provider-local state. The canonical auth inputs are
resource capabilities (`relay_admin_cap`, `directory_publish_cap`, `key_publish_cap`,
`recovery_record_read_cap`/`write_cap`, `blob_repair_cap`). They are independent per provider/resource and cannot be
used as AccountID, controller, recovery or E2EE authority.

ADR-0141 makes their **transport** a negative-capability boundary too: bearer capabilities never travel in URL
paths/query strings/fragments/cookies, ordinary logs/analytics/telemetry/crash reports, referrers or cross-origin
redirects. Authenticated/private responses are non-cacheable (`no-store` on HTTP). `RecoveryStore` additionally
stores only authenticated encrypted advisory snapshots; provider-side generation checks never replace the client's
durable RecoveryRecord high-water mark, and recovered routes/capabilities are revalidated before use.

The provider may reject malformed objects, but end-to-end clients decide whether signed OM objects are current and
authorized. This prevents a convenience Home from becoming an identity verifier/root while keeping compromised
provider capabilities bounded to service-level nuisance until rotation.

## ProviderUsageState

Must distinguish at least:

- healthy/unhealthy;
- soft quota approaching;
- hard free-tier failure threshold;
- billable overage threshold;
- configured budget remaining;
- reset time/window;
- unknown/unverifiable.

Routing policy must fail safely when cost state is unknown and the operator requires a hard zero-cost guarantee.

## Discovery / introduction / relay interfaces

```text
Resolver
  resolve(input, policy) -> ContactDescriptor[]
  verify(descriptor, policy)
  refresh(descriptor)

DirectoryProvider
  resolve(alias)
  search(query)
  publish(claim)
  revoke(claim)
  transparency_proof(...)

IntroGate
  issue_ticket(introduction_service, admission_context)
  submit_request(ticket, sealed_request)
  revoke_invite(invite_id)

RelaySelector
  select(relay_set, route_policy)
  failover(relay_set, failed_entry)
  redundant_targets(relay_set, reliability_policy)

LanRendezvous
  advertise(rotating_token)
  discover(expected_tokens)
```

The interfaces are logical contracts; transport/serialization details follow the 2026-09-23 accepted NetworkTransport/canonical-encoding decisions.

## NetworkTransport (Q-019, ADR-0130 rule 6)

```text
NetworkTransport
  capabilities() -> TransportCaps            // stream | datagram, max_payload, latency class, privacy class
  open(endpoint, policy) -> Channel          // endpoint is an opaque locator from a RelaySet/descriptor
  send(channel, bytes) / recv(channel) -> bytes
  close(channel)
  health() -> Reachable | Blocked | Degraded | Unknown
```

A transport is an **untrusted byte carrier** (R-268). It carries already-encrypted OM envelopes and never sees
plaintext, keys, AccountID or ConversationID. Canonical semantics are identical over every transport: failover from
HTTPS to QUIC, a proxy, a community bridge, a Tor-style adapter or LAN changes the path, never the meaning, the
authentication or the security level of what is carried. Transports are selected by RoutePolicy, which may escalate
along CENSORSHIP_AND_NETWORK_RESILIENCE §5 but never downgrades security to regain reachability.

Conformance cases (`conformance:network_transport`, registered in [`machine/conformance_suites.json`](../../../machine/conformance_suites.json)):

| ID | Case | Pass condition |
|---|---|---|
| NT-01 | same envelope over two transports | recipient materializes byte-identical canonical events |
| NT-02 | transport tampers, truncates, replays or reorders bytes | every fault is detected above the transport; no state change, no plaintext exposure |
| NT-03 | failover mid-conversation (endpoint blocked) | delivery continues on the next transport or route with no identity, key or history migration |
| NT-04 | transport sees only ciphertext and opaque locators | trace tap finds no AccountID, ConversationID, key or plaintext |
| NT-05 | all transports blocked | the client reports "can't reach" and queues; it never falls back to a weaker security level |
| NT-06 | same logical EventID sent over a local route then WAN/provider retry | authenticated dedupe materializes one canonical event; local endpoint/route metadata grants no identity, device or relationship authority (ADR-0150) |
| NT-07 | concurrent valid RelaySet branches arrive in different orders, including child-before-parent | after all valid records are known every peer selects the same effective set (highest version, then smallest canonical hash); version jumps/cross-lineage parents reject and signer revocation recomputes eligibility without reviving authority (ADR-0161) |

## Provider contracts resolved by A-045 / ADR-0133

AGENTS.md lists these as core interfaces. ADR-0133 resolved the architecture contract; the sketches below and the Home API define their design boundary, while V-020/conformance owns executable provider evidence: `KeyMaterialProvider` (bundle publish/fetch/revoke with per-relationship fetch capabilities and client-side ADR-0140 freshness, R-209/R-291/R-298..R-301),
`BackupStore`, `RecoveryStore` (RecoveryRecord store and RecoveryRequest fan-out, ADR-0111), `StoragePool`,
`CryptoProvider`, per-contact deposit-capability mint/revoke (block), sponsorship redemption (ADR-0102),
BlobLocationRecord repair (ADR-0075) and the forwarding "moved" state.

## Additional logical interfaces

```text
AccountControlStore
  get_head()
  fetch_state_chain(from_head)
  submit_transition(transition, authorization)
  quarantine(device, evidence/policy)
  resolve_control_fork(...)

RecoveryMethod
  describe_capabilities()
  enroll(policy_context)
  authorize_recovery(challenge)
  rotate_or_revoke(...)

ProvisioningProfile
  authorize_provider_account()
  plan_resources()
  apply_plan()
  self_test()
  export_migration_state()

PushProvider / PushGateway
  register_opaque_wake_handle()
  wake(handle, opaque_hint)
  revoke_or_rotate(handle)

ContactShare
  mint_contact_capsule(policy)
  verify_contact_capsule(capsule)
  attach_referral(capsule, recipient_context)
```

Vendor names remain implementations, not interface branches.

## 2026-09-22 operatorless interfaces

```text
ProvisioningMethod
  inspect_requirements()
  deploy_into_user_account()
  self_test()
  export_migration_state()

EvidenceExporter
  select_items()
  build_evidence_package()
  verify_package_locally()
  hand_off_to_user_chosen_target()

ClientDistribution
  distribution_id()
  update_authority()
  push_provider?()
  report_targets?()
  protocol_capabilities()
```

`ClientDistribution` metadata is not peer trust. A remote peer must never grant protocol privilege merely because the other side claims to use a reference/official build.

## Shared UI / PlatformAdapter contract

Add a presentation boundary above Core:

```text
om-ui intent -> om-core typed command -> canonical state transition -> view state/event -> om-ui render
```

`om-ui` never manipulates key bytes, relay capabilities, AccountControlState or protocol stores directly.

`PlatformAdapter` capability families should include secure storage, push/notifications, background execution, share/open-external, files, camera/microphone, local contacts, clipboard, local discovery, system calling, network/window hints, haptics and later external carrier transports.

Product behavior should query capabilities rather than branch broadly on platform names.

## Engineering client/security interfaces — 14:34 checkpoint

```text
DeviceVault
  create_profile_vault(profile_id)
  seal(profile_id, purpose, bytes)
  open(profile_id, purpose, sealed)
  rotate_wrapping(profile_id)
  capabilities()

IdentityProfileStore
  list_profiles()
  create_profile()
  open_profile(profile_id)
  delete_profile(profile_id)

ClientMigration
  create_link_offer()
  authenticate_new_device()
  transfer_profile_state(policy)
  finalize_new_device()

LinkPreviewProvider
  fetch_preview(url, privacy_policy)

DiagnosticsStore
  record_redacted(event)
  export_user_reviewed_bundle()
```

`IdentityProfileStore` is a local application namespace, not a global identity registry. `DeviceVault` never exposes provider-specific key handles as protocol fields.

## 2026-09-25 frozen W0 provider contracts (ADR-0133)

The former A-045 "Missing contracts" are now explicit logical interfaces. Wire bindings are in
[`spec/protocol/home-api.md`](../../../spec/protocol/home-api.md) and [`spec/cddl/home-api.cddl`](../../../spec/cddl/home-api.cddl).

```text
KeyMaterialProvider
  publish_bundle(key_publish_cap, signed_async_key_bundle)
  fetch_bundles(relationship_cap, device_ids)
  revoke_generation(key_publish_cap, device_id, generation)  # best-effort deletion only
  rotate_fetch_capability(key_publish_cap, relationship_id)

BackupStore
  put_encrypted_backup(object_cap, ciphertext, retention_policy)
  get_encrypted_backup(object_cap)
  delete_backup(object_cap)
  usage_state()

RecoveryStore
  put_recovery_record(recovery_record_write_cap, opaque_locator, encrypted_record)
  get_recovery_record(recovery_record_read_cap, opaque_locator)
  fanout_recovery_request(recovery_auth, signed_request)
  drain_recovery_requests(controller_cap)

StoragePool
  put_ciphertext(object_cap, ciphertext, durability_class)
  get_ciphertext(object_cap)
  delete_ciphertext(object_cap)
  repair_locations(blob_repair_cap, signed_blob_location_records)

CryptoProvider
  profile_capabilities()
  random_bytes(n)
  hkdf(profile, inputs)
  aead_seal(profile, key_handle, nonce, aad, plaintext)
  aead_open(profile, key_handle, nonce, aad, ciphertext)
  kem_encap(profile, public_key)
  kem_decap(profile, private_key_handle, encapsulation)
  sign(role, key_handle, canonical_bytes)
  verify(role, public_key, canonical_bytes, signature)
```

**ADR-0140 provider rule:** `KeyMaterialProvider` stores the current v1 bundle without provider-visible ACS generation/hash. `generation` is global per DeviceID + CryptoProfile across provider pools; `provider_pool_id` scopes consumable items but never resets freshness. `revoke_generation` is cleanup only. Clients/fetchers authenticate the DeviceCredential against relationship-pinned ACS and enforce the durable recipient high-water mark/equal-generation core hash.


`CryptoProvider` is a local implementation boundary, never a network provider. Raw durable private-key bytes need not
be exportable; handles are implementation-private. Protocol outputs remain canonical and interoperable.

Per-contact deposit-capability mint/revoke, sponsorship redemption, `RecoveryRecord`, blob repair and signed `moved`
responses are Home API v1 operations. `NetworkTransport` remains the untrusted byte-carrier contract above.
