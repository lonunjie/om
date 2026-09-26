# OM Protocol Longevity and 0M Continuity

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 08:21 PHT
- **Resolves:** A-015
- **Horizon:** 20+ years, including founder/project/provider disappearance, crypto failure, protocol generations and dominant-implementation capture

## 1. Decision summary

OM's long-term survival is not defined as “the original organization remains healthy forever.”

It is defined as:

> **Users, independent implementations and successor stewards retain enough public specification, authority continuity, portable state and protocol freedom to continue interoperating or deliberately fork/migrate when any original dependency disappears or becomes hostile.**

This is the operational meaning of **0M — zero total control** over time.

The architecture therefore distinguishes:

```text
protocol identity / immutable published versions
project stewardship
client distribution trust roots
provider operation
registries and discovery
user/Room/Service authority
```

No one of these is allowed to silently become the others.

## 2. What may disappear without invalidating OM identity

Loss of any one of these must not inherently destroy `AccountID`, `ConversationID`, `SpaceID`, `ServiceID` or already-held history:

- SeedLife/lonunjie/founder;
- GitHub or the canonical forge;
- the project website or any DNS domain (for example a placeholder `project.example`; the steward's real domain is `lonunjie.org`);
- Cloudflare/AWS/one hosting provider;
- one directory/catalog;
- one app store;
- one reference client;
- one bot/service host;
- one moderation labeler;
- one relay/blob/push provider;
- one repository/package registry.

Availability may degrade and users may need migration/rediscovery, but identity continuity is not reassigned to the vanished party.

## 3. Published protocol versions are self-contained artifacts

A released protocol generation/profile must be usable without live access to the original project website or registry server.

Each normative release should have a **Protocol Continuity Bundle** containing or cryptographically committing to at least:

```text
protocol/profile identifier
normative schemas/specs
canonical encoding rules
golden/conformance vectors
extension/registry snapshot
CryptoProfile registry snapshot
security/deprecation status at publication
license texts
release metadata/signatures
reference implementation revision/digest (when available)
known migration guidance
```

Bundles are mirrorable/content-addressable and can be stored on multiple forges, archives, package mirrors or offline media.

A live registry is a convenience/update source, not a runtime oracle required to interpret already-published canonical bytes.

"Normative schemas/specs" is an explicit, versioned manifest of artifacts — not a basename allowlist that can silently
miss a semantic doc — kept at [`spec/continuity/om-crypto-v1-bundle-manifest.json`](../../spec/continuity/om-crypto-v1-bundle-manifest.json)
and enforced by [`tools/continuity_bundle.py`](../../tools/continuity_bundle.py) `build`/`verify`/`check-manifest`.

## 4. Registry continuity

Canonical registry IDs already assigned are immutable and never reused.

If the original registry steward disappears:

- existing IDs keep their published meaning;
- anyone can mirror the last signed/public snapshot;
- experimental/private extensions continue using collision-resistant namespaced IDs without central assignment;
- a successor standards group may publish a new registry lineage/namespace;
- clients may support multiple registry lineages when protocol generations diverge.

There is no requirement that one organization regain a monopoly on “the only registry that can exist.”

For a future extension to enter an **OM baseline interoperability profile**, its semantics must be public and independently implementable. A vendor-private extension can remain vendor/private but cannot retroactively redefine a published baseline ID.

## 5. Protocol generations coexist explicitly

OM must assume two or more protocol generations will coexist.

Peers exchange authenticated supported profile/generation sets. Negotiation obeys security floors and critical-extension semantics.

Possible states include:

```text
native compatible
compatible after negotiated extension set
migration/reinitialization required
read/export only
unsupported safely
```

An implementation that does not understand a critical authority/security semantic fails closed for that operation rather than guessing.

Old history remains readable/exportable where its cryptography is still processable; sending new traffic on a forbidden cryptographic profile is a separate decision.

## 6. No permanent “latest version” authority in the wire protocol

The network does not ask `project.example/latest` (or any project domain) whether a protocol object is valid.

Validity is determined from:

- the object's declared protocol/profile/extension semantics;
- canonical published rules;
- authenticated local/relationship/Room state;
- the implementation's configured allowed/forbidden security policy.

Project advisories can recommend migration or forbid a profile in a reference distribution, but independent distributions can verify the evidence and publish their own policy.

This prevents website/DNS/project compromise from becoming instant protocol-global authority.

## 7. Cryptographic primitive failure

Crypto agility already separates AccountID from algorithms. The longevity model adds explicit failure classes:

### Planned deprecation

A primitive becomes weaker/obsolete but is not catastrophically broken.

- new preferred CryptoProfile published;
- endpoints upgrade and authenticate cross-profile transition;
- security floor advances;
- old profile becomes deprecated then forbidden for new traffic;
- old keys/state erased when safe.

### Urgent break with still-trustworthy transition signatures

- emergency profile/update is distributed through normal authenticated release/profile mechanisms;
- clients prioritize migration/reinitialization;
- no hidden emergency bypass key is introduced.

### Authentication primitive catastrophically forgeable

If the old authorization signatures themselves can no longer prove continuity, cryptography cannot manufacture certainty after the fact.

OM must rely on evidence that existed **before** the break where available, such as:

- hybrid/multi-algorithm transition credentials already present;
- previously witnessed AccountControlHeads/transparency observations;
- relationship-held continuity evidence;
- recovery mechanisms under an independent primitive;
- user-assisted re-verification.

The UI must distinguish “cryptographically continuous” from “re-established by user/social verification.”

No central project key may simply declare ownership of every user's identity after a catastrophic break.

## 8. Release/update root failure

Client distribution trust belongs to each distribution, not the OM protocol.

The reference distribution uses TUF-style threshold/offline roots, provenance and transparency. If those roots are lost/compromised:

- that distribution may need a documented root-recovery/new-distribution transition;
- independently distributed OM clients remain independently valid;
- protocol peers do not reject another implementation merely because it lacks the original project's software-signing key;
- existing users can migrate to another conforming client using authenticated account/device migration, not a project-issued account transfer.

By S2 the project should test a creator/release-root disappearance drill.

## 9. Forge/domain/CDN disappearance

Protocol and source continuity use ordinary open tooling plus mirrors rather than a magical decentralized storage dependency.

At public-production maturity:

- Git repositories are mirrored on at least one independent forge/archive;
- signed source/release bundles are publishable outside the primary domain;
- conformance vectors and protocol continuity bundles have independent mirrors;
- build instructions do not require a private CI service;
- runtime clients do not load security-critical code from an unpinned third-party CDN.

DNS/domain loss affects discoverability, not cryptographic ownership of user identities.

## 10. Provider disappearance

Every provider role has an escape story proportional to its custody:

- relay: authenticated RelaySet rotation/overlap/repair;
- BlobStore: stable BlobID plus authenticated BlobLocationRecord/mirroring/restore;
- directory: alternate directories/direct invites/cached signed descriptors;
- IntroGate: replace endpoint/invite descriptors;
- KeyMaterialProvider: disjoint provider pools and republishing;
- push: optional wake mechanism, never message custody;
- service host: ServiceControlState authorizes endpoint migration;
- bridge host: BridgeControlState authorizes endpoint migration;
- client host: authenticated new-device/client-host migration;
- catalog/labeler: optional subscription/source replacement.

If every route is lost, manual rediscovery may be required. That is a liveness limitation, not an excuse for one mandatory global provider.

## 11. Dominant client capture / embrace-extend-extinguish

A company may gain 90% client share. OM cannot prevent market dominance by protocol magic.

It can prevent that market share from automatically becoming protocol authority.

Rules:

1. Published baseline specs/schemas/conformance remain implementation-neutral.
2. A vendor extension has no special rights because many users run it.
3. A proprietary/permission-gated extension cannot become mandatory for baseline OM conformance.
4. Critical shared semantics must have public canonical encoding and capability rules.
5. A Room may explicitly require an optional public extension, but this is visible Room policy and may exclude clients lacking it; it does not rewrite the global baseline.
6. Export/escape remains available so a vendor cannot trap canonical OM history solely in opaque local storage.
7. Independent conformance suites make divergence detectable.
8. Alternate clients/providers can continue interoperating on published profiles even if a dominant vendor forms a separate incompatible network.

If a dominant vendor deliberately changes semantics incompatibly, that is a **fork**, not a silent new definition of old OM bytes.

## 12. Dominant extension capture

An extension can become popular enough to feel mandatory.

For it to graduate into a future baseline:

- semantics/specification must be public;
- canonical encoding/criticality must be defined;
- at least one independent implementation/conformance path must be feasible;
- licensing/patent terms must permit implementation without vendor permission;
- security/privacy review is required;
- fallback/migration behavior is specified.

Popularity alone does not grant canonical status.

## 13. Project governance capture

Open licenses preserve the legal right to fork, but operational continuity also needs a technical path.

If the canonical steward is captured/hostile:

- users' protocol identities remain theirs;
- independent clients/providers continue published versions;
- maintainers may mirror/fork source/specs/tests;
- successor distributions use their own update roots;
- a successor protocol profile can be published under a new stewardship lineage;
- communities/users choose which lineage/profile to adopt through ordinary authenticated upgrades/policies.

The captured steward does not possess a protocol kill switch.

Trademark/domain ownership may affect branding, not cryptographic protocol validity.

## 14. Fork semantics

Forking is a last-resort continuity mechanism, not a routine governance tool.

OM distinguishes:

```text
implementation fork
  same protocol profiles, different code/governance

project/stewardship fork
  same existing protocol + new future release lineage

protocol-generation fork
  incompatible new semantics/profile

Room/Space governance fork
  successor social/control object chosen by participants
```

Existing immutable IDs/objects keep their historical meaning. A fork must not reuse an existing protocol/profile/registry ID for incompatible semantics.

Cross-fork bridges/migration can be added explicitly; incompatibility is not hidden.

## 15. Data and identity portability across generations

AccountID is algorithm/provider independent, but not every future generation can be assumed to support every old state machine forever.

A new generation should provide, where cryptographically possible:

- authenticated AccountControlState transition;
- new DeviceIdentity enrollment;
- relationship rekey/migration;
- Room reinitialization/migration or explicit successor Room link;
- archive/history read/export;
- Service/Bridge control-state transition;
- explicit unsupported/lossy report.

When continuity cannot be proven, create/link a successor identity rather than silently claiming cryptographic continuity.

## 16. Public archival preservation

The OM Project should publish continuity artifacts to multiple preservation channels before S2/public-production claims.

Candidates may include:

- independent Git forges;
- software heritage/public source archives;
- package/container mirrors;
- release tarballs/Git bundles;
- standards/document archives;
- community-operated mirrors.

No single archival service is mandatory. The purpose is redundancy and recoverability.

## 17. Dependency longevity

Reference implementations may depend on libraries/platforms, but protocol semantics may not depend on one abandoned binary/library.

Before freezing a required primitive/format:

- specification must be public;
- test vectors exist;
- at least a credible independent implementation path exists;
- licensing permits continued implementation;
- serialization is specified independently of one library;
- migration/deprecation is possible.

For cryptography, use reviewed standards/libraries rather than bespoke constructions, while keeping interfaces agile enough to replace implementations.

## 18. Mobile/app-store capture

Apple/Google may restrict distribution or APIs.

OM therefore keeps:

- protocol independent of store identity;
- PWA/web path where viable;
- independent Android/native distribution where platform permits;
- client export/migration;
- push optional to message correctness;
- provider roles usable by third-party clients.

Platform restrictions can reduce UX/availability on that platform; they must not redefine the protocol for everyone else.

## 19. Economic capture

A managed provider may subsidize service until alternatives disappear.

Protocol defenses are portability and competition:

- provider contracts are public/reimplementable;
- self-host/reference deployment remains possible where practical;
- provider migration does not change canonical identity;
- no proprietary provider capability becomes mandatory baseline;
- clients can configure multiple providers and expose cost/failure-domain policy.

OM does not promise that every provider is equally cheap or convenient.

## 20. Moderation/discovery capture

A dominant directory/catalog/safety labeler can shape visibility but cannot become protocol existence.

- direct invites/known descriptors bypass discovery;
- multiple directories/catalogs/labelers are supported;
- delisting does not revoke AccountID/ServiceID;
- safety advisories are subscribed policy inputs, not protocol truth;
- public content may be mirrored/hosted by independent providers where legal/desired.

## 21. Compatibility identity

A client should report conformance in terms of supported public profiles/capabilities, not brand.

Examples:

```text
OM Baseline Generation 1
CryptoProfile OM-Crypto-v1
Extension set {...}
Archive om-archive-v1
```

“Official client” is not a wire-level security role.

## 22. Continuity drill

Before public-production resilience is claimed, the project must perform a documented exercise in which reviewers assume:

```text
founder unavailable
GitHub unavailable
project domain unavailable
reference relay/provider unavailable
one release signing role compromised
one dominant client refuses a new open profile
```

The drill must demonstrate retrieval of specs/source/vectors from an independent mirror, building/verifying a compatible implementation, provider migration, and a credible successor release path without changing user AccountIDs merely because project infrastructure disappeared.

## 23. 0M invariants over time

1. No live project server is required to interpret an already-published protocol generation.
2. Existing assigned registry IDs never change meaning.
3. User/Room/Service/Bridge authority is not inherited by a successor project steward.
4. A dominant implementation/extension gains no special protocol authority from market share.
5. A proprietary mandatory extension cannot redefine baseline OM interoperability.
6. Provider/distribution disappearance has a documented migration/escape path or an explicit honest liveness limitation.
7. Cryptographic migration does not change AccountID when continuity can be authenticated.
8. Catastrophic crypto failure does not empower a project master key to seize user identities.
9. Forks use new lineage/profile identifiers rather than rewriting old semantics.
10. Source/spec/test rights and artifacts are sufficient for independent continuation.

## 24. Verification obligation

V-017 must run a protocol-longevity/continuity exercise covering at least:

- build/interpret a published protocol bundle with project website/forge offline;
- registry snapshot recovery and no ID-reuse ambiguity;
- two protocol generations negotiating/coexisting and failing safely on critical semantics;
- CryptoProfile deprecation/forbidden transitions and catastrophic-authentication-break tabletop cases;
- reference distribution root loss/rotation without protocol-global authority;
- dominant-client proprietary-extension attempt against conformance rules;
- provider disappearance/migration for relay/blob/directory/KeyMaterial/service host;
- independent source/spec/vector mirror restore;
- Room/Space/project fork identifiers and migration guidance;
- archive/history continuity across client implementations;
- creator-disappearance drill consistent with S0/S2 stewardship claims.

Passing V-017 does not prove OM will live for twenty years. It proves the architecture has no known single required continuity choke point hidden in the current design.
