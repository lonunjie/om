# OM Multi-Relay and Delivery Resilience

- **Status:** Canonical architecture checkpoint
- **Checkpoint:** 2026-09-21 10:08 PHT

## 1. Multiple relays are first-class

An OM account/device may use several relays simultaneously. This is not merely a manual disaster-recovery setting.
Each accepted relationship gets its own RelaySet (§2), so a leaked capability exposes one relationship only.

```text
recipient
   │
 signed/private RelaySet
   │
 ┌─┼─────────────┐
 ▼ ▼             ▼
A  B             C
CF Hetzner       community/self-hosted
```

The set should prefer independent provider/ASN/failure domains where practical.

## 2. RelaySet

Logical fields:

```text
RelaySet
  version
  entries[]:
    relay_uri
    opaque_capability
    priority
    weight?
    expiry
    transport profiles
    privacy/cost hints
    failure_domain hints?
  signature/authentication
```

A public discovery descriptor may contain only safe introduction relay hints. Accepted relationships exchange the actual private `RelaySet` over E2EE.

### 2.1 Scope and signer (normative, R-211)

- **Per relationship.** A RelaySet is the receiving side's set for **one** accepted relationship. It is sent only
  inside that relationship's E2EE session, and its capabilities are never reused in another relationship's set.
- **Signer.** It is signed (role `device`) by a device that the recipient's **current** AccountControlState lists as
  active and not quarantined (control or messaging). Peers MUST validate the signer against the newest ACS they hold
  and reject any other signer: quarantined, removed, revoked or unknown devices cannot publish a RelaySet.
- **Version/branch convergence (ADR-0161).** Version 1 omits `previous_set_hash`; every later record is exactly
  `parent.version + 1` and names the canonical `OM-RELAYSET-HASH-v1` hash of a previously accepted parent in this
  relationship's RelaySet lineage. Concurrent valid devices may extend the same parent. Peers retain those branches
  as authenticated availability history and choose the effective set deterministically: highest version, then
  lexicographically smallest canonical record hash. A child received before its exact parent is pending and is
  reconsidered when the parent arrives; arrival order MUST NOT choose a permanent winner. Version jumps, cross-lineage
  previous hashes and malformed genesis/successor continuity are rejected.
- **Signer changes.** New records require a signer still valid in the current ACS. If no authenticating signer remains eligible after later
  quarantine/revocation/removal, the record stops being an effective candidate; its hash/version may remain as a
  historical continuity node so an otherwise-valid descendant signed by a currently eligible device stays
  structurally verifiable. Old signing authority is not thereby kept live.
- **Semantics on the frozen fields** (ADR-0126/0161): the effective set changes under the deterministic rule above;
  an old entry's overlap ends at its own `expires_at`; emergency revocation is a new version without the entry plus
  revoking its drain capability at the relay; the catastrophic-loss rendezvous is the separate
  ReachabilityRecoveryDescriptor ([`REACHABILITY_AND_OBJECT_REPAIR.md`](REACHABILITY_AND_OBJECT_REPAIR.md)).

### 2.2 Sealed-delivery keys (ADR-0142)

`RelaySet` deliberately stays small. For every active RelayEntry, each eligible recipient device issues a separate
signed `SealedDeliveryKeyRecord` to the accepted relationship. The record's `route_binding` commits to the exact
`relay_uri` and deposit capability, so a key learned for route A cannot be transplanted to route B. Records rotate or
expire independently of RelaySet publication and are removed when the relationship, device or route is revoked.

A byte-identical wrapper retry MAY use the same `(delivery_key_id, delivery_id)` on the same route. A new attempt on a
different route MUST select that route's delivery key, run a fresh HPKE seal and use a fresh `delivery_id`. If a usable
record is absent, the sender refreshes/queues or returns an explicit local failure; it never sends the ordinary inner
envelope in the clear as a compatibility fallback.

## 3. Different capability per relay

Never reuse one mailbox/capability identifier across providers.

Bad:

```text
Relay A: ABC123
Relay B: ABC123
```

Preferred:

```text
Relay A: q-8fc2...
Relay B: x-991a...
Relay C: m-37e4...
```

This reduces trivial cross-provider correlation.

## 4. Delivery modes

### Normal

Choose one healthy preferred relay according to policy.

### Failover

If delivery cannot make progress, retry another entry without changing the OM event identity.

### Redundant high-reliability

For selected high-priority deliveries, send independently wrapped copies through two relays. The recipient deduplicates using `EventID` after authentication.

Redundant fanout is not the default because it increases metadata exposure and traffic.

## 5. Independent outer wrapping

Even when two relays carry the same logical encrypted OM event, their transport envelopes should be freshly randomized so colluding relays cannot correlate byte-identical ciphertext as easily.

## 6. Relay rotation

Relay membership/capabilities can rotate independently of AccountID and conversation identity.

Existing contacts learn changes through authenticated relationship state. Directory providers do not own or rewrite accepted relationship RelaySets.

## 7. Health/cost policy

The sender/recipient can select routes using:

- reachability;
- latency;
- quota/budget;
- provider failure domain;
- privacy policy;
- recipient preference;
- censorship reachability.

The user normally sees none of this.

## 8. Cloudflare

Cloudflare Workers/Durable Objects can implement one relay. A different provider or self-hosted implementation can implement another. OM wire semantics must remain identical.
## 2026-09-22 hardening — migration overlap and catastrophic reachability

RelaySet rotation now explicitly distinguishes normal overlap from the case where every route a peer knows disappears before it receives the new set. [`REACHABILITY_AND_OBJECT_REPAIR.md`](REACHABILITY_AND_OBJECT_REPAIR.md) is canonical for this failure.

RelaySet version/continuity/overlap semantics are required. A separately configured authenticated recovery rendezvous may assist catastrophic repair, but OM does not solve the problem by globally publishing normal private delivery capabilities. Without any surviving route or recovery rendezvous, manual rediscovery is an explicit liveness limitation.

