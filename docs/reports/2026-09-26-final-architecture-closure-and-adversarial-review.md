# Final architecture closure and adversarial cross-boundary review

**Date:** 2026-09-26  
**Scope:** all eleven declared engineering scopes; architecture/specification only  
**Decision:** ADR-0139  
**Result:** no remaining blocking architecture-design question after the repairs recorded below; implementation and independent-review gates remain unchanged

## 1. Purpose and method

This is the final design-phase attack pass, not a claim that OM is implemented or proven secure. It asks whether the frozen architecture accidentally creates authority, privacy or downgrade behavior that contradicts OM's operatorless/anti-capture philosophy when subsystems are composed.

The pass attacked every engineering scope for the following classes:

1. **authority laundering** — can a narrow role acquire broader account/message/governance authority through another interface?
2. **optional security context** — can a caller omit peer identity, route, epoch, incarnation, policy, profile or authorization context and still succeed?
3. **stable correlators** — does a service learn a durable identifier not needed for its function?
4. **fail-open behavior** — does missing/stale/conflicting security state become permission rather than rejection?
5. **ABA/stale-object attacks** — can an old revoke/lift/grant/delete target a later incarnation?
6. **downgrade paths** — can migration, fallback, advisories or profile negotiation move an established floor backward?
7. **recovery escalation** — can recovery/storage/legacy authority become ordinary controller or messaging authority?
8. **provider colocation** — do colocated roles gain a new protocol power rather than merely the union of observations and explicitly granted capabilities?
9. **multi-device races** — can concurrent devices manufacture or resurrect authority?
10. **multi-account isolation** — can local state, push, vault, plugins or provider handles cross profile/account boundaries?
11. **migration/portability authority leakage** — can imported/exported history become current authority or routing identity?
12. **offline/partition behavior** — does lack of connectivity bypass revocation, policy or authorization checks?
13. **compromised-client blast radius** — does compromise of one device/provider/service inherit powers it should not possess?
14. **update/distribution capture** — can a project/provider/update role become mandatory global protocol authority?
15. **resource exhaustion as authority bypass** — do caps, queues, retries or pressure modes discard security state or relax authorization?

The review used the current canonical architecture, ADRs, machine ledgers and security invariants. Design-only Python checks may validate ledger consistency; no TLC, Rust, Wasm/native build or product runtime evidence is claimed.

## 2. Closure repairs found by this pass

### S2-1 — SecurityAdvisory lifecycle could be misread as a downgrade permission

**Risk:** advisory expiry/supersession/user override text allowed an implementation to interpret removal of an advisory recommendation as permission to lower the relationship's already-established CryptoProfile/protocol floor.

**Repair:** R-296 / T-102 / invariant D1 / ADR-0139. An advisory creates an additional floor. Removing that advisory floor never erases the stored relationship floor and never revives a forbidden profile.

### S2-2 — logical schema permissiveness could be mistaken for wire authority

**Risk:** some logical JSON schemas intentionally allow modeling flexibility while W0 CDDL/registries are frozen. Treating the JSON schema as wire authority could produce parser/signature/authorization differentials.

**Repair:** R-297 / T-103 / invariant E1 / ADR-0139. For frozen W0 objects, CDDL + registries + domain separation are authoritative bytes. Logical schemas cannot add fields or semantics.

### S2-3 — historical “open/candidate/TBD” wording outlived the decision that resolved it

**Risk:** an engineer could reopen identity/discovery/sync/transparency choices that are already resolved, or mistake a release/verification obligation for an architecture choice.

**Repair:** current canonical documents now disposition former questions explicitly. PR-011 and V-* obligations remain verification/release gates rather than design uncertainty. Frozen logical views no longer say the wire encoding is TBD where CDDL already answers it.

### S2-4 — archive encryption candidate could become a de-facto production choice

**Risk:** the `OMA-ENC-1-candidate` label might become implementation inertia even though its verification/review gate is incomplete.

**Repair:** it is explicitly verification-only. V-014 must freeze the reviewed production archive-encryption profile, vectors, limits and migration story before the `portability` scope is authorized.

No other cross-boundary issue in this pass required a new architecture rule. That statement means “no additional design contradiction was found,” not “the implementation is secure.”

## 3. Scope-by-scope adversarial review

### 3.1 `core_kernel`

**Boundaries attacked:** AccountControlState, 1:1 relationship binding, async key material, event authorization, multi-device synchronization, relay/blob/provider capabilities, recovery/legacy and canonical encoding.

**Disposition:**

- Account authority has one machine-readable transition policy; recovery is not controller voting authority, quarantined challenge signatures have only their explicitly narrow freeze/irreversible-action roles, and incarnation-bound reductions prevent ABA targeting.
- inbound 1:1 processing requires non-optional `RelationshipContext`; peer identity is bound before secret-key/session/plaintext processing.
- sync is account-private E2EE state replication, not an alternate AccountControlState merge path; CRDT/LWW semantics cannot manufacture constitutional authority.
- provider/resource capabilities are service powers only and do not authenticate AccountControlState or recover/sign as an account.
- RecoveryRoot/RecoveryPolicy/LegacyPolicy powers are disjoint from normal controller powers and use verifier-local waiting/contest semantics where irreversible action needs time.
- resource exhaustion may reject/drop bounded service work but must not erase revocation/replay/authorization state or turn a fail-closed check into permission.
- frozen W0 wire authority is CDDL/registries/domain separation, closing parser-differential ambiguity.

**Residual evidence:** V-001/002/003/005/008/009/011/013/014/015/016/019 and implementation/fuzz/runtime criteria remain ledger-owned.

### 3.2 `reference_client_ui`

**Boundaries attacked:** DeviceVault/profile isolation, onboarding, verification UX, security-warning/advisory UX, PWA ClientHost, push/notification surfaces and local data classes.

**Disposition:**

- local IdentityProfiles are namespace-isolated; UI convenience must not become a cross-profile identifier or shared vault key.
- user-visible acceptance/verification cannot silently downgrade cryptographic floors; advisory override only removes the advisory-added recommendation.
- irreversible security actions cannot be represented optimistically as successful before authoritative completion.
- ClientHost/update roots remain separate from AccountID/protocol authority; compromise of an update path is a client-distribution threat, not a protocol-wide ability to rewrite historical wire meaning.
- notification/push profiles cannot choose weaker message crypto and cross-profile push-token reuse is forbidden unless explicitly declared by the selected profile.
- platform leakage (keyboard, snapshots, backups, notification rendering, extensions) remains a declared endpoint threat rather than something E2EE claims to solve.

**Residual evidence:** V-006/V-018/V-021/V-022/V-023 and human/platform studies remain implementation/external work.

### 3.3 `provider_reference`

**Boundaries attacked:** Home/relay/directory/key/recovery/blob roles, provider colocation, onboarding, metadata, resource capabilities and failover.

**Disposition:**

- providers receive only the identifiers/capabilities required by each surface. Routine administration no longer needs DeviceCredential/ACS authority material.
- same-operator roles are treated as one colluding observer for metadata analysis; colocation unions observations/powers but creates no new protocol authority.
- provider capabilities cannot become AccountControlState, RecoveryPolicy, message-signing or decryption credentials. Clients remain authoritative for end-to-end canonical validity.
- RecoveryRecord get/put uses opaque RecoveryRoot-derived scope capabilities, while actual RecoveryRequest fan-out separately discloses the AccountID/attempt to the mailbox Home and is modeled as such.
- starter/onboarding providers are replaceable and cannot become mandatory runtime roots.

**Residual evidence:** V-015/V-020/V-022 plus conformance and deployment diversity gates remain open.

### 3.4 `groups`

**Boundaries attacked:** MLS membership, GroupControlState/governance reductions, stale epochs/incarnations, service/bridge membership, group-call relationship and future PQ transition.

**Disposition:**

- Room membership/authentication and governance authority are explicit and scoped; role reductions/grants target immutable incarnations.
- ordinary MLS group messages are transferably authenticated; OM v1 makes no deniability claim.
- service/bridge participants are explicit members/principals rather than invisible human impersonators.
- call-media keys are participant-bound and do not follow automatically from Room MLS epoch possession.
- group PQ upgrade is a later explicit ReInit/profile transition; v1 groups are not described as PQ confidential.

**Residual evidence:** V-004/V-009/V-019 and MLS/library/runtime conformance remain implementation work.

### 3.5 `portability`

**Boundaries attacked:** archive import/export, stale authority resurrection, foreign provenance, archive encryption, migration/fork semantics.

**Disposition:**

- archive history/evidence is distinct from current AccountControlState, routing and provider authority. Import cannot make stale credentials current or foreign history locally authoritative.
- foreign imports remain provenance-marked/non-routable until normal OM relationship/authority establishment occurs.
- `OMA-ENC-1-candidate` is verification-only and cannot ship by inertia; V-014 owns the reviewed production encryption freeze.
- export/import must preserve exact evidence needed to interpret history without transferring hidden execution/extension authority.

**Residual evidence:** V-014 remains the portability unlock gate.

### 3.6 `expressive_media`

**Boundaries attacked:** parsers, decompression/size pressure, remote execution, ContentPack provenance, expiry/export and cross-surface injection.

**Disposition:**

- expressive assets are inert bounded data; no remote scripts, HTML/CSS execution, SVG scripting or plugin hooks are smuggled into the content format.
- immutable/digest-bound assets and ContentPack provenance prevent an update source from silently rewriting already-rendered conversation meaning.
- parser/resource caps reject hostile content rather than changing authorization.
- shared blob/dedup modes that increase metadata leakage are explicit/off by default where required.

**Residual evidence:** V-012 parser/fuzz/runtime work remains open.

### 3.7 `calls`

**Boundaries attacked:** caller identity, unknown caller behavior, participant membership, media-key distribution and Room/call authority confusion.

**Disposition:**

- unknown callers do not obtain a privileged ringing path; admission remains relationship/permission controlled.
- SFrame/media keys are distributed to joined participant devices and rotate with participant-set generation; possession of Room state alone is not media decryption authority.
- call identity cannot bypass the underlying relationship/account binding.
- relay/media infrastructure carries ciphertext and transport state, not participant account authority.

**Residual evidence:** V-007 browser/native interoperability and real media/runtime behavior remain open.

### 3.8 `communities_publications`

**Boundaries attacked:** scoped moderation, governance, publication sequencing, appeal/evidence, community-to-global authority escalation and resource pressure.

**Disposition:**

- moderation decisions are scoped records; community roles do not become AccountControlState or global OM authority.
- governance/role changes use scoped capability semantics and immutable targets; stale reductions cannot retarget later occupants.
- publication sequencing/evidence can be verified without a mandatory OM-operated catalog.
- public-surface safety/legal processes do not create a universal abuse desk or plaintext custody requirement for private messaging.

**Residual evidence:** V-008/V-016 and relevant public-release gates remain open.

### 3.9 `services_bots`

**Boundaries attacked:** autonomous authorship, RepresentationGrant cascade, endpoint revocation, subscriptions/triggers, plaintext recipients and account impersonation.

**Disposition:**

- ServicePrincipal is distinct from AccountPrincipal; extension/service code cannot emit invisible Account-authored events.
- RepresentationGrant is scoped, visibly attributable and cascades from a valid control-device basis; loss/quarantine/revocation of that basis removes the delegated authority according to its rules.
- remote services receiving plaintext are explicit recipients/egress, not “local helpers” that bypass consent.
- subscription is a trigger/filter, not ambient permission to act or read.

**Residual evidence:** V-011 and service conformance/runtime abuse tests remain open.

### 3.10 `plugins`

**Boundaries attacked:** local plugin permissions, plaintext read, network egress, remote-origin mini-apps, bilateral relationship consent, update provenance and account isolation.

**Disposition:**

- plaintext access and network egress are separate capabilities; remote-origin plaintext crossing is itself egress.
- continuous 1:1 plaintext egress requires signed bilateral RelationshipPolicyConsent rather than one local user's silent decision for both participants.
- plugin/update authority cannot become account-control authority, and profile/account capability namespaces remain separate.
- inert content/themes/fonts cannot acquire extension execution privileges by format confusion.

**Residual evidence:** V-010 sandbox/runtime escape testing remains open.

### 3.11 `bridges`

**Boundaries attacked:** foreign provenance, human impersonation, Room membership, loop/replay lineage, plaintext termination, bridge/service role confusion and migration.

**Disposition:**

- BridgePrincipal is a distinct principal and cannot author as an OM Account or foreign human.
- a private-Room bridge is an explicit full member/termination point where applicable; E2EE termination is visible rather than hidden.
- foreign origin/provenance and loop lineage are preserved; imported/bridged content cannot silently become native current authority.
- bridge and service permissions remain separate even when the same operator runs both roles.

**Residual evidence:** V-013 bridge mapping/loop/provenance interoperability remains open.

### 3.12 `local_delivery` (addendum 2026-09-28, ADR-0156)

Added when the 2026-09-28 independent review split offline-local LAN delivery into its own scope.

**Boundaries attacked:** clear-sender exposure to LAN observers, stable LAN-visible correlators, LAN reachability as relationship or device authority, incompatible LAN bindings, unauthenticated resource exhaustion before crypto/blob work.

**Disposition:**

- 1:1 LAN traffic must carry `SealedDeliveryEnvelope` protection or a reviewed equivalent; a LAN route is never a clear-sender fallback, and group traffic stays MLS `PrivateMessage`.
- every LAN-visible identifier rotates at least per rendezvous epoch; no stable AccountID/DeviceID/device key is broadcast.
- first contact over LAN reuses MessageRequest/IntroGate; LAN reachability grants no relationship, device eligibility or security-floor change.
- route-independent EventID dedupe and blob backfill stay in `core_kernel`, so W0 is correct whatever route delivered an event.

**Residual evidence:** the concrete binding is design item A-055, and V-024 (frozen CDDL/vectors, passive-observer trace, offline first-contact model, browser feasibility) is open.

## 4. Cross-scope conclusions

### Authority

No provider, recovery, directory, service, bridge, plugin, archive, advisory or update role is intentionally able to become ordinary AccountControlState authority merely by exercising its normal function. Where a role can affect end-user behavior, the effect is scoped and named.

### Context and fail-closed behavior

Security-critical context that changes identity/authorization meaning is part of the relevant typed/canonical state rather than an optional convenience parameter. Missing or stale authority/profile/incarnation context is rejection or a bounded degraded service condition, not permission.

### Metadata

OM does not treat cryptographic unlinkability as transport/operational anonymity. Provider/local-resource correlators, IP/timing/size, push tokens, W0 mailbox sender material and recovery fan-out leakage are documented per observer. Future profiles may reduce exposure but cannot retroactively overclaim W0.

### Downgrade/evolution

Published/frozen meanings are monotonic: advisory lifecycle cannot lower a relationship floor; future stronger profiles use explicit profile/version transitions; incompatible evolution forks explicitly instead of silently changing old bytes.

### Operatorlessness and anti-capture

Starter providers, update/distribution infrastructure, catalogs, directories, verification providers and transparency services remain replaceable/non-constitutional. Established communication does not require a SeedLife/OM-operated runtime service. Colocation is modeled as collusion, not hidden independence.

## 5. What this pass intentionally does not prove

This pass does not prove:

- correctness of Rust/native/Wasm implementations;
- TLC state-space results for the current tree;
- browser/WebAuthn/SFrame/platform behavior;
- cryptographic library correctness or external review;
- parser/fuzzer coverage in production implementations;
- performance/resource budgets;
- human comprehension of warnings/recovery/verification UI;
- provider diversity in real deployments; or
- resistance to endpoint compromise beyond the designed blast-radius limits.

Those remain verification/engineering/public-release obligations. Design closure must not be used to bypass them.

## 6. Final disposition

The architecture-design queue is closed. The canonical design is frozen for engineering handoff, subject to the existing ADR process. A future agent should reopen architecture only when implementation evidence, independent review, a new requirement/standard or a discovered contradiction demonstrates a real defect. It should not reopen settled rules merely to choose a different implementation preference.

Engineering remains unauthorized until [`machine/repository_contract.json`](../../machine/repository_contract.json) authorizes a named scope after its required gates/feasibility/independence conditions are satisfied.
