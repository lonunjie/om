# Reachability and Object-Location Repair

- **Status:** Canonical resilience architecture direction
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Why repair is separate from normal routing

Normal OM communication uses private replaceable RelaySets and replaceable encrypted blob locations. Migration usually succeeds by overlapping old/new locations and distributing authenticated updates.

A harder case exists when **every location known to an offline peer disappears before that peer receives the update**. No protocol can retrieve an unknown new private location through a route that no longer exists.

OM therefore distinguishes:

```text
normal private routing
migration overlap
repair/rendezvous after total known-route loss
```

## 2. RelaySet migration

Canonical RelaySet updates require at least:

```text
RelaySet {
  version
  previous_set_hash
  valid_from
  entries[]
  overlap_until?
  emergency_revocation?
  optional_recovery_rendezvous?
  authorization
}
```

Exact schema may differ, but continuity/version/overlap semantics are mandatory. The frozen W0 RelaySet
([`delivery.cddl`](../../../spec/cddl/delivery.cddl)) carries them without new fields (ADR-0126):

| Semantic | Frozen W0 encoding |
|---|---|
| `version`, `previous_set_hash` | RelaySet keys 2 and 3 |
| `valid_from` | the moment a peer accepts the higher `version` |
| `overlap_until` | each old entry's own `expires_at` (entry key 4) |
| `emergency_revocation` | a new version that omits the entry, plus revoking its drain capability at the relay |
| `optional_recovery_rendezvous` | the separate ReachabilityRecoveryDescriptor ([`reachability-recovery.logical.schema.json`](../../../spec/protocol/reachability-recovery.logical.schema.json)) |
| `authorization` | a signature by a device active and not quarantined in the recipient's current AccountControlState ([`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) §2.1) |

### Hash-chain and concurrent-writer convergence (ADR-0161)

The previous-hash fields are not advisory. For both RelaySet and BlobLocationRecord lineages, genesis is version 1
with no previous hash; every successor is exactly parent version + 1 and names the canonical domain-separated hash of
a previously accepted parent in the same lineage. Concurrent children are valid branches. A record received before
its parent is pending rather than permanently rejected, and can be reconsidered when that parent arrives.

The effective record among structurally valid records with at least one authenticating signer **currently eligible** is selected
independently of arrival order: highest version, then lexicographically smallest canonical record hash. If that signer
later becomes ineligible, recompute from retained eligible candidates. An ineligible historical parent may remain as
continuity metadata; that never restores its authority. Implementations may compact payloads but retain enough
`(record_hash, version, lineage)` metadata while the relationship/BlobID is live to validate descendants and avoid
order-dependent branch loss. Resource limits may throttle/refuse excess updates but must not silently convert arrival
order into protocol state.

### Normal migration

1. publish/add new relay capabilities;
2. retain old routes for a bounded overlap period when safe;
3. distribute authenticated RelaySet update through established relationships/self-sync;
4. observe acknowledgements where privacy permits;
5. retire old routes according to policy.

### Catastrophic known-route loss

If all routes a peer knows are gone, automatic recovery requires an independently discoverable rendezvous mechanism that does not become normal routing authority. Possible sources include an authenticated recovery descriptor exposed through a previously known discovery identity, direct QR/contact exchange, or another user-selected recovery channel.

If the user configured no such rendezvous and no mutual path survives, OM states the limitation explicitly: **manual rediscovery/contact exchange may be required.**

OM will not solve this by publishing ordinary private relay capabilities globally.

## 3. Stable BlobID versus mutable locations

Messages refer to immutable encrypted-object identity independently from provider location:

```text
BlobID = random stable identifier for the encrypted object (integrity comes from ciphertext_digest, ADR-0119 ruling 6)
BlobLocationRecord = authenticated mutable knowledge of where that BlobID can currently be fetched
```

A location change MUST NOT change the canonical message/event identity.

## 4. Blob relocation

Normal relocation:

1. copy the same verified ciphertext (never re-encrypted) to new BlobStore(s);
2. publish/authenticate a newer BlobLocationRecord, signed (role `device`) by a device that is active and not
   quarantined in the current AccountControlState of the original sender or of a current conversation member.
   Peers reject any other signer. If none of its authenticating signers remains eligible, its record no longer wins the
   effective-record selection, though its accepted hash/version may remain as inert continuity ancestry;
3. keep old/new locations concurrently for policy-defined overlap where feasible;
4. verify recipient/self-device repair propagation;
5. retire old location.

## 5. Late/offline recipient repair

An authorized peer that possesses a message and BlobID but only stale/dead locators needs a bounded way to request/obtain an authenticated current location record from surviving relationship/self-sync/provider-independent repair paths.

BlobStore itself is not allowed to declare object ownership or rewrite the BlobID.

## 6. Privacy constraints

Repair mechanisms MUST avoid turning stable BlobIDs, AccountIDs or relationship graphs into globally queryable tracking handles. Where a repair lookup is offered by infrastructure, use pairwise/opaque authorization and minimize persistent observability.

## 7. Failure model

OM explicitly accepts that complete simultaneous loss of:

- all known routes;
- all configured recovery rendezvous;
- all peer-side contact knowledge;
- and all copies of an encrypted object

is unrecoverable. The architecture guarantees replaceability and repair paths, not magical recovery from deletion of every independent copy/reference.
