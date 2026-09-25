# Implementation Interface Sketches

These are conceptual interfaces, not frozen language APIs.

> **2026-09-24 (ADR-0130):** conceptual sketches are not enough for two independent Homes to interoperate. The
> normative **Home API** (operations, request authentication, CDDL request/response bodies, a typed error registry
> with at least `quota`, `moved` and `retryable`, and versioning) covering relay, IntroGate, descriptor publishing,
> sponsorship, recovery records and blob spool, a real ProviderCapabilities JSON Schema with `durability_class`, and
> the interfaces listed under "Missing contracts" below are design item **A-045**. It blocks `provider_reference`,
> and "Home API frozen" is a V-020 pre-engineering criterion. The provider conformance runner targets that API.

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

## Missing contracts (A-045)

AGENTS.md lists these as core interfaces; each needs a sketch here and, where a Home implements it, an operation in
the Home API: `KeyMaterialProvider` (bundle publish/fetch with per-relationship fetch capabilities, R-209),
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
  publish_bundle(device_auth, signed_async_key_bundle)
  fetch_bundles(relationship_cap, device_ids)
  rotate_fetch_capability(device_auth, relationship_id)

BackupStore
  put_encrypted_backup(object_cap, ciphertext, retention_policy)
  get_encrypted_backup(object_cap)
  delete_backup(object_cap)
  usage_state()

RecoveryStore
  put_recovery_record(device_auth, encrypted_record)
  fanout_recovery_request(recovery_auth, signed_request)
  drain_recovery_requests(controller_cap)

StoragePool
  put_ciphertext(object_cap, ciphertext, durability_class)
  get_ciphertext(object_cap)
  delete_ciphertext(object_cap)
  repair_locations(device_auth, signed_blob_location_records)

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

`CryptoProvider` is a local implementation boundary, never a network provider. Raw durable private-key bytes need not
be exportable; handles are implementation-private. Protocol outputs remain canonical and interoperable.

Per-contact deposit-capability mint/revoke, sponsorship redemption, `RecoveryRecord`, blob repair and signed `moved`
responses are Home API v1 operations. `NetworkTransport` remains the untrusted byte-carrier contract above.
