# Implementation Interface Sketches

These are conceptual interfaces, not frozen language APIs.

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

The interfaces are logical contracts; transport/serialization details are Gate D work.

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
