# OM Metadata Threat Model

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 08:21 PHT
- **Resolves:** A-013
- **Scope:** routing, relationship, group/Space, push, blob, discovery, service, bridge, migration and timing metadata

## 1. Decision summary

OM does not treat end-to-end content encryption as a claim that metadata is hidden.

The governing rule is:

> **Metadata privacy is surface-specific. A client may claim only the privacy property that the actual routing, provider and disclosure path provides.**

There is no single `metadata_private=true` bit.

The baseline architecture therefore does four things:

1. keeps stable canonical identity and conversation identifiers out of provider-routing paths where they are not required;
2. uses scoped opaque capabilities, fresh outer transport wrapping and provider separation to reduce easy correlation;
3. exposes privacy-relevant provider/path properties to policy and UI rather than hiding them behind one lock icon; and
4. reserves expensive batching, cover traffic and anonymity networks for Hardened/Maximum-Privacy policy rather than making ordinary messaging unusable.

This document does **not** promise anonymity against a global passive observer, compromised endpoints, provider collusion, traffic-analysis correlation or a participant who legitimately knows the Room membership.

## 2. Metadata classes

OM distinguishes at least these classes because each has a different legitimate audience and mitigation strategy:

```text
Content metadata
  message body structure, author identity inside conversation,
  reply target, Topic, reactions, attachment names/types

Routing metadata
  opaque queue/capability, destination relay, network address,
  transport session and retry/failover state

Relationship metadata
  who communicates with whom, accepted-contact edges,
  invitation/contact-request activity

Group/Space metadata
  membership, roles, ConversationID/SpaceID, epoch/change activity,
  child links and Room/Space affiliation

Traffic metadata
  timing, cadence, message counts, ciphertext lengths,
  online/offline patterns and retry timing

Blob metadata
  encrypted-object size class, upload/download timing,
  provider/object locator and access frequency

Push metadata
  device/platform token relationship, wake timing and frequency

Discovery metadata
  alias/handle query, client network identity, query timing,
  public descriptor lookup and directory choice

Service/bridge metadata
  invocation timing, selected context, installation scope,
  external-network mapping and interop lineage

Migration/archive metadata
  export/import timing, archive size, provider migration,
  object-set inventory and destination

Public-by-design metadata
  public Publication content, public service/catalog entry,
  intentionally public Room/Space descriptor or moderation advisory
```

A datum being encrypted in one layer does not make every observation of its existence harmless.

## 3. Threat actors

The metadata threat model considers separately:

- local network/ISP observer;
- relay operator;
- KeyMaterialProvider;
- directory/resolver and discovery relay;
- IntroGate/anti-abuse provider;
- BlobStore/media provider;
- push platform/provider;
- TURN/SFU/call infrastructure;
- Room/Space members and administrators;
- ServicePrincipal/LLM host;
- BridgePrincipal and foreign network;
- client distributor/update channel;
- archive/import/migration operator;
- colluding subsets of the above;
- global passive observer or nation-state network observer.

The last category is explicitly outside ordinary-profile anonymity guarantees. Maximum Privacy may reduce exposure through anonymizing/pluggable transports, but OM does not claim universal traffic-analysis resistance.

## 4. Stable canonical identifiers stay above routing

`AccountID`, `ConversationID`, `SpaceID`, `PublicationID`, `ServiceID` and `BridgeID` are canonical protocol identities. They are **not default network addresses**.

Normal private relay delivery uses scoped random deposit/drain capabilities. A relay does not need `AccountID` or `ConversationID` to enqueue or drain ciphertext.

For an accepted relationship:

```text
AccountID / ConversationID
        |
        | E2EE/private state
        v
per-relationship / per-device routing state
        |
        v
opaque provider-specific queue capabilities
```

Capabilities are distinct across providers where practical. Reusing one stable mailbox token across unrelated relays is forbidden because it becomes a trivial cross-provider correlator.

## 5. Hide inner protocol identifiers from delivery providers

MLS and other secure protocols may contain visible protocol headers such as a group identifier, epoch information or ciphertext length. OM does not hand such inner protocol messages directly to a relay when doing so would expose canonical Room identity unnecessarily.

The private-delivery profile therefore uses an independently randomized encrypted **OM transport envelope per destination/provider** around the inner session/MLS payload.

Consequences:

- the relay routes by opaque capability, not `ConversationID` or MLS group ID;
- two relays do not receive byte-identical outer ciphertext for the same logical event where avoidable;
- provider-visible retry/delivery metadata remains provider-local rather than becoming canonical event state;
- inner MLS metadata remains visible to legitimate Room recipients after decryption and remains subject to the underlying MLS protocol's own leakage properties.

This wrapping reduces easy provider correlation. It does not defeat correlation by timing/length against a strong observer.

## 6. Private Room/Space membership

Private Room and Space membership is sensitive relationship metadata.

Baseline rules:

- private Space membership/roles/private child organization are distributed only to authorized participants;
- private Room membership/control state is not published in a global directory;
- a relay/BlobStore does not receive the plaintext Room roster merely to deliver ciphertext;
- MLS ratchet-tree/group-control distribution must avoid unnecessary exposure to the delivery provider;
- proposals/commits/control objects that can be carried privately should not be exposed as provider-readable public protocol messages solely for implementation convenience;
- Room membership known legitimately by Room participants is **not** described as hidden from those participants;
- former members may retain metadata they learned while authorized.

Large-Room optimizations may expose more structure if deliberately selected. Such a provider profile must declare the additional exposure rather than inheriting a generic “E2EE” privacy claim.

## 7. Size leakage and padding

Ciphertext length leaks information. Attachments are especially distinctive.

The Recommended profile therefore permits/encourages low-cost **size bucketing/padding** for transport envelopes and small messages. Exact bucket boundaries are an implementation/conformance measurement, not constitutional authority.

For larger attachments:

- content is encrypted client-side before upload;
- the provider sees at least encrypted object size or size class unless a stronger profile pads/chunks it;
- filenames, captions, MIME semantics and canonical conversation identity are encrypted and withheld from the blob provider;
- optional padded chunking/mirroring may trade bandwidth/storage for weaker size fingerprints;
- no UI may imply that attachment size is hidden when it is not.

Padding must be bounded; attackers must not be able to request unbounded amplification from recipients/providers.

## 8. Timing and access-pattern leakage

Ordinary asynchronous messaging necessarily creates traffic timing.

Recommended OM does **not** continuously emit cover traffic simply to claim anonymity. That would impose substantial battery, bandwidth and provider cost and would be especially hostile to users in expensive/limited networks.

Policy layers may add:

```text
Recommended:
  ordinary delivery; cheap jitter/batching only where it does not harm UX

Hardened:
  stronger size buckets; optional short batching/jitter;
  provider diversity; private discovery; relayed calls where needed

Maximum Privacy:
  anonymity/pluggable transports where available;
  stronger padding/batching;
  optional cover-traffic mechanisms with explicit cost/battery disclosure;
  reduced presence/receipts; vendor push optionally disabled
```

Cover traffic, if implemented, is an explicit `MetadataPolicy` mechanism with measurable overhead and threat assumptions. It is not a baseline checkbox that silently burns user bandwidth.

## 9. Push notifications

Opaque push payloads protect content but **do not hide wake timing** from APNs/FCM or another push provider.

Push rules remain:

- no normal plaintext, sender name, conversation title or canonical ConversationID in vendor push;
- use opaque per-profile/per-device wake handles;
- minimize durable linkage between relay queue identity and platform token;
- local client renders notification content after fetching/decrypting;
- Maximum Privacy may disable vendor push with an explicit background-delivery tradeoff.

The UI must not say “Apple/Google learns nothing” merely because the payload is opaque.

## 10. Discovery and introduction metadata

Public discovery can leak both **what** is queried and **who** queried it.

OM supports separation patterned after oblivious HTTP/private resolver designs:

```text
client -> DiscoveryRelay -> DirectoryResolver
```

When independent/non-colluding:

- the relay can see client network identity but not plaintext alias query;
- the resolver can see the alias query but not the original client network address.

The client must understand that collusion, unique request content/configuration, timing or a global observer may still correlate the request.

IntroGate abuse controls use provider/surface-scoped signals, short-lived tickets and optional privacy-preserving admission proofs. They must not require turning `AccountID` into a global rate-limit cookie.

## 11. Anti-abuse metadata is deliberately scoped

Abuse defense creates pressure for stable identifiers. OM accepts **scoped linkability** when necessary while rejecting universal linkability.

Examples:

- a relay may rate-limit one queue/deposit capability;
- an IntroGate may rate-limit a source/network/session or consume unlinkable admission tokens;
- a Room can recognize a member AccountPrincipal because membership requires that identity;
- a user can block a known AccountPrincipal locally;
- a provider may keep short-lived operational abuse telemetry under its own policy.

None of these automatically justify a permanent cross-provider user fingerprint.

Privacy Pass-style admission may prove limited entitlement without exposing a stable per-client identifier, but its non-collusion/network-address assumptions must be documented.

## 12. ServicePrincipal and LLM metadata

A selected-context invocation should disclose only what the service needs for that invocation.

By default it need not receive:

- stable `ConversationID`;
- full Room roster;
- unrelated message history;
- private delivery capabilities;
- user's relay/provider topology;
- DeviceIdentity or AccountControlState details.

A Room-installed subscription/full member legitimately learns more because the Room explicitly granted that scope. That is a separate trust relationship and is visible shared state.

A remote LLM service may learn network and invocation timing in addition to selected plaintext unless an anonymizing intermediary is used. OM transport encryption does not erase that fact.

## 13. Bridge metadata

Bridge interoperability needs stable-enough lineage to suppress loops but must not create a universal cross-network tracking tag.

`InteropTrace.origin_fingerprint` is therefore **mapping/bridge-domain scoped** by default. Implementations must not expose a raw foreign event ID or one globally stable hash when a scoped keyed/domain-separated fingerprint can do the job.

Bridge path metadata is disclosed only to the extent required for loop prevention and diagnostics. It must not be copied into ordinary user-visible forwarding metadata across unrelated Rooms.

The foreign network/bridge still observes whatever its side inherently exposes. “OM E2EE ends at the bridge” includes metadata/trust boundaries as well as plaintext boundaries.

## 14. Blob/provider access patterns

A blob provider should not need AccountID/ConversationID to fetch an encrypted object.

Blob locators/capabilities are scoped and rotatable. Provider access still reveals:

- object locator it serves;
- encrypted size/size class;
- fetch/upload timing;
- client network identity unless proxied;
- repeated accesses to the same provider object.

Hardened/Maximum Privacy can use proxy/anonymity transport, mirroring or re-encryption/relocation strategies where justified. Replication itself can increase cross-provider correlation if byte-identical outer objects or synchronized access patterns are used, so policy must consider the tradeoff.

## 15. Calls

Direct WebRTC can expose peer IP information. `RoutePolicy` therefore separates call privacy from message cryptography.

- Recommended may permit direct peer paths when users accept that tradeoff;
- Hardened/Maximum Privacy may require TURN/relay/no-host-candidate paths;
- SFU/TURN providers necessarily observe call timing, approximate duration and traffic volume even when media is SFrame/E2EE;
- call metadata is not described as hidden merely because media content is encrypted.

## 16. Archives, migration and recovery

A migration/export provider can learn that a migration occurred, archive size and possibly object-set information depending on the path.

Ordinary portability must work client-to-client without a project migration service. If a cloud migration helper is used, it is a provider/trust surface with its own declared metadata exposure.

Archive encryption hides contents from storage but generally not archive size/timing unless additional padding/anonymity is used.

## 17. Metadata Exposure Descriptor

Providers/clients SHOULD expose a machine-readable `MetadataExposureDescriptor` (logical design artifact; not universal truth) covering at least:

```text
role
sees_client_network_address: always | sometimes | no_by_design
sees_stable_account_id: yes | no_by_design | only_if_explicit
sees_conversation_id: yes | no_by_design | only_if_explicit
sees_membership_or_roster: ...
sees_message_timing: ...
sees_ciphertext_size: ...
sees_blob_size: ...
sees_query_plaintext: ...
sees_push_timing: ...
retention_policy_reference?
non_collusion_assumptions[]
optional_mitigations[]
```

A provider self-description is an operational claim, not cryptographic proof. Clients may combine protocol-enforced facts with provider-asserted policy and independent evidence.

## 18. Privacy UI

The product should explain **effects**, not protocol jargon.

Examples:

- “This relay can see when this device connects, but not your OM account ID or message contents.”
- “Apple/Google can see notification wake timing; message content is fetched and decrypted by OM.”
- “This Room uses a bridge. The bridge can read exported messages and sees the connected external room.”
- “Direct calls may reveal your IP address to the other participant.”

Do not show a universal “private metadata” badge.

## 19. Non-goals / honest limits

V1 does not claim to defeat:

- a global passive network observer;
- collusion of every provider on a user's path;
- endpoint malware or compromised client runtime;
- traffic correlation based on highly distinctive timing/size despite padding;
- metadata known legitimately by conversation/Room participants;
- public metadata intentionally published in a Publication/directory/catalog;
- retrospective knowledge retained by former authorized participants.

## 20. Security invariants

1. Stable canonical IDs are not default provider-routing identifiers.
2. Private relay delivery does not require provider-readable `ConversationID`/MLS group identity.
3. Per-provider capabilities and outer ciphertext are not intentionally reused as cross-provider correlators.
4. Content E2EE is never presented as proof that timing, size, IP, membership or push metadata is hidden.
5. Privacy-strengthening paths state their non-collusion/anonymity assumptions.
6. Anti-abuse may use scoped linkability but does not create mandatory universal person tracking.
7. Service/bridge disclosure scope is explicit and minimized.
8. Expensive padding/cover traffic is policy-selectable and bounded, not silently mandatory.
9. Provider metadata claims are distinguishable from protocol-enforced properties.
10. Metadata privacy failures cannot silently downgrade message cryptographic authentication/E2EE.

## 21. Verification obligation

V-015 must adversarially test at least:

- no canonical AccountID/ConversationID leakage in normal relay routing traces;
- distinct provider capabilities and randomized outer wrappers;
- MLS/group-control delivery paths for accidental provider-readable group/membership data;
- push payload contents and stable-handle correlation;
- blob filename/MIME/conversation leakage and object-size behavior;
- discovery relay/resolver non-collusion assumptions and unique-request correlation;
- service selected-context identifier minimization;
- bridge fingerprint scoping and cross-Room/cross-bridge linkability;
- migration/archive metadata leakage;
- profile transitions Recommended/Hardened/Maximum Privacy;
- user-facing claims against what packet/provider traces actually reveal.

A passing V-015 is required before engineering authorization because provider API choices can otherwise bake accidental metadata disclosure into the first implementation.
