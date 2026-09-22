# OM Interoperability Bridges and Foreign Provenance

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:56 PHT
- **Resolves:** A-011
- **Scope:** external network bridges, BridgePrincipal identity, foreign actors/events, room mapping, loop prevention, semantic loss, E2EE boundary and identity linking

## 1. Decision summary

Independent conforming OM clients are not bridges; they are native participants in one public protocol.

A bridge exists only when OM crosses into a different messaging/network authority domain such as Matrix, Telegram, email, SMS/RCS, Discord-like systems, IRC or future networks.

The governing rule is:

> **A bridge may translate semantics; it may never translate trust invisibly.**

If a bridge receives OM plaintext, OM-native E2EE ends at that bridge. The external leg has its own security properties and is never silently labeled equivalent to OM-native E2EE.

## 2. BridgePrincipal

A bridge is a typed `BridgePrincipal`:

```text
PrincipalRef
  account(AccountID)
  service(ServiceID)
  bridge(BridgeID)
```

`BridgeID` is random stable 256-bit provider-independent data.

A bridge has a domain-separated `BridgeControlState` using the same controller-set principles as ServiceControlState:

```text
BridgeControlState
  bridge_id
  state_version
  previous_state_hash
  controllers[]
  approval_threshold
  endpoint_credentials[]
  adapter_definitions[]
  authorization_evidence
```

The bridge's hosting provider or external network does not own `BridgeID` merely because it runs an endpoint.

A bridge endpoint is never an Account device and never gains AccountControlState votes.

## 3. Why BridgePrincipal is distinct from ServicePrincipal

Execution may reuse much of the service hosting/capability machinery, but the principal type remains distinct because every bridge-authored event has an unavoidable semantic property:

> some meaning or actor attribution came from outside OM's native authentication domain.

That distinction drives mandatory UI, provenance, loop handling and security-state behavior.

A generic assistant does not need those semantics. A bridge always does.

## 4. External actors are not silently minted as OM users

A foreign network may expose millions of users. OM does not fabricate AccountIDs for them and does not permit a bridge to sign as if those people were native OM principals.

An imported bridged event is canonically authored by the `BridgePrincipal` and carries bridge-attested foreign provenance:

```text
ForeignProvenance
  foreign_network_namespace
  foreign_conversation_ref
  foreign_event_ref
  foreign_actor_ref?
  foreign_actor_display_hint?
  observed_source_time?
  bridge_observed_at
  fidelity_flags[]
  origin_fingerprint?
```

`foreign_*_ref` values are adapter-defined opaque identifiers. Clients treat display names/avatars from the foreign network as bridge-attested metadata, not OM identity proof.

## 5. Foreign identity linking is evidence, not identity merging

A person may control both an OM AccountID and a foreign account. OM can record an optional `ForeignIdentityLink`:

```text
ForeignIdentityLink
  account_id
  foreign_network_namespace
  foreign_actor_ref
  proof_method
  bridge_or_network_attestation
  account_authorization
  expiry/revocation
```

The proof method may vary by network: signed challenge, OAuth-style proof, posted nonce, domain claim, bridge attestation or another explicit method.

Even a valid link does not let the bridge author events as the AccountPrincipal. It only lets clients present useful recognition such as:

```text
Alice on Telegram — linked to Alice's OM identity
```

Native Account authorship still requires an authorized OM endpoint signature.

## 6. Bridge admission to a private Room

Continuous two-way bridging of a private OM Room is a high-risk plaintext disclosure.

The bridge must be explicitly admitted/bound under Room policy as a visible plaintext recipient/export boundary. Depending on implementation it may be a full MLS member or another cryptographically explicit authorized disclosure recipient defined by the bridge protocol, but it cannot be hidden inside a server-side integration.

Room state identifies:

```text
bridge_id
external_network(s)
direction                # OM->foreign, foreign->OM, both
mapped_foreign_scope
allowed_event_classes
fidelity/capability summary
approval evidence
revocation state
```

Removing the bridge stops future authorized disclosure and performs the relevant key/removal transition. It cannot claw back plaintext already transmitted to the external network.

## 7. Manual share is not continuous bridging

A user can always manually copy, screenshot or forward information they possess. OM does not pretend cryptography can prevent this.

However, automatic or continuous Room export is materially different. A service that continuously sends Room plaintext outside OM requires explicit bridge/export authorization; it cannot hide behind an ordinary local plugin's network permission.

A one-shot user-selected share may use the normal forwarding/export path and need not make a permanent bridge member, but the UI must still show the destination/trust boundary.

## 8. Security-state UI

A bridged Room must visibly communicate at least:

- that bridging is active;
- which external network/scope is connected;
- which BridgePrincipal is responsible;
- whether traffic is inbound, outbound or bidirectional;
- what event classes are exported;
- that OM E2EE terminates at the bridge for exported content;
- any known external-leg security claim separately from OM's claim.

Preferred language is factual:

```text
OM encryption protects this message to Bridge X.
Bridge X forwards it to Telegram; Telegram-side security is separate.
```

Do not show a single lock icon implying one cryptographic E2EE session spans both systems unless a future reviewed end-to-end protocol actually proves that property.

## 9. Room and conversation mapping

A bridge maintains explicit mapping state:

```text
BridgeMapping
  mapping_id
  bridge_id
  om_scope_ref
  foreign_network_namespace
  foreign_scope_ref
  direction
  capability_map
  policy
  state_version
```

One OM Room may map to zero/one/multiple foreign scopes only when Room policy permits it.

One foreign scope may map to zero/one/multiple OM Rooms, but duplicate/loop safeguards must make fan-out explicit.

A bridge mapping never changes the provider-independent `ConversationID` of the OM Room.

## 10. Capability and fidelity mapping

Every external adapter advertises semantic support such as:

```text
InteropCapabilities
  text
  rich_text_subset
  attachments
  replies
  edits
  deletes
  reactions
  polls
  threads/topics
  mentions
  read_receipts
  membership_events
  max_sizes
  external_e2ee_claim
  identity_proof_methods[]
```

Bridge translation produces explicit `fidelity_flags`, for example:

```text
exact
lossy_formatting
reply_target_unavailable
edit_represented_as_new_message
delete_not_propagated
reaction_not_supported
attachment_transcoded
foreign_actor_unverified
```

The protocol never assumes every network has OM-equivalent semantics.

## 11. Edits and deletes

A foreign edit may map to an OM edit only when the bridge can prove/track a stable foreign source event and the mapping policy allows it.

If not, the bridge emits a new bridged event such as “edited version” rather than pretending canonical OM edit ancestry exists.

Foreign deletion/redaction can update bridge materialization state, but the UI must not claim cryptographic erasure from OM recipients or from the external network.

Likewise, an OM delete/redaction forwarded outward is only a request/translation according to the external network's semantics.

## 12. Reactions, replies, Topics and polls

Targets are translated through the bridge mapping table. If a target no longer has a valid cross-network mapping, the semantic loss is explicit rather than attaching the action to an unrelated message.

Topics/threads are preserved only where the foreign network has a compatible stable target. Otherwise a bridge may flatten them with visible context markers.

Polls may be mirrored only when voter/privacy semantics are compatible enough for the configured policy; otherwise the bridge forwards a read-only summary/link rather than silently changing the voting model.

## 13. Loop prevention and duplicate suppression

Bridges carry an interop lineage envelope:

```text
InteropTrace
  origin_network_namespace
  origin_fingerprint
  traversed_bridge_ids[]
  hop_count
  hop_limit
  transformation_ids[]
```

`origin_fingerprint` is a domain-separated hash/opaque adapter fingerprint of a stable source event identifier where the source provides one. Raw foreign identifiers need not be exposed when an opaque fingerprint suffices.

A bridge must refuse/restrict an event when:

- its BridgeID already appears in the traversed path;
- hop limit is exhausted;
- the origin fingerprint has already been materialized for the same mapping/direction;
- policy identifies a reflection path.

The metadata-leak consequences of origin fingerprints and bridge paths remain subject to A-013.

## 14. Multi-bridge conflict behavior

If two bridges import the same foreign event, clients may deduplicate only when provenance is strong enough to establish the same origin fingerprint/network scope.

They must not merge merely because text, sender display name and timestamp look similar.

If provenance conflicts, show distinct events or a diagnostic conflict state; do not invent certainty.

## 15. External E2EE does not compose automatically

A foreign network may itself support E2EE. A bridge that decrypts both sides still creates two separate secure relationships:

```text
OM participant <== OM E2EE ==> Bridge <== external security ==> foreign participant
```

This is not one end-to-end encrypted channel between the humans.

A future bridge that can transform ciphertext without obtaining plaintext would be a different architecture and would require its own reviewed protocol/ADR.

## 16. Bridges and Services

A bridge may internally invoke ServicePrincipals (translation, moderation, summarization) only under separate capabilities. Adding a bridge does not automatically authorize AI processing of bridged plaintext.

Likewise, installing a ServicePrincipal does not authorize it to become a bridge/exporter.

## 17. Bridge discovery and hosting

Bridge identity follows the same anti-centralization principles as services:

- local/self-hosted creation is possible;
- hosting endpoint is replaceable;
- direct signed descriptors can be shared;
- directories/catalogs are optional;
- domain aliases may advertise bridges;
- no SeedLife bridge registry is mandatory.

External platforms can still impose their own centralized API/app-registration requirements on **their** side. OM labels that as an external dependency rather than importing it into OM identity.

## 18. Security invariants

1. A bridge cannot author as a foreign person or native AccountPrincipal without native authorization.
2. Foreign identity claims retain provenance and never silently become OM identity.
3. Continuous private-Room export is visible authorized state.
4. OM E2EE termination is explicit wherever plaintext crosses a bridge.
5. Unsupported semantics degrade visibly, not silently.
6. Loop/duplicate prevention is based on provenance, not text heuristics.
7. Bridge hosting/discovery is replaceable; no OM global gateway registry is required.
8. Removing a bridge prevents future authorized export but cannot revoke foreign plaintext already delivered.

## 19. Verification obligation

V-013 must adversarially exercise:

- loops/reflections across 2+ bridges;
- duplicate imports through independent bridges;
- malicious foreign display-name impersonation;
- forged identity-link claims;
- edit/delete/reaction target confusion;
- stale bridge mapping and revocation races;
- capability mismatch/fidelity signaling;
- Room removal/key transition;
- attempts to hide a bridge as an ordinary Account/Service author;
- metadata leakage from origin fingerprints and bridge traces in coordination with A-013.
