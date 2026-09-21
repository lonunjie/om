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
