# OM Phase 0 — Architecture Plan

- **Status:** Active planning
- **Date:** 2026-08-18
- **Goal:** Freeze the contracts and security properties that downstream client/backend work can safely depend on.

## Why Phase 0 exists

OM is becoming a system of independently replaceable parts rather than a single app plus a server. That increases architectural surface area, but the complexity can be contained if the core contracts are decided in dependency order.

The rule for Phase 0 is: **do not solve every future feature; freeze only the invariants and interfaces that would be expensive or dangerous to change later.**

## Architecture constitution

1. **Identity is independent of providers.** Relay, storage, backup, directory, push, transport, and project-operated infrastructure may all change without creating a new person.
2. **Human-readable names are locators, not identities.** `@username`, phone numbers, domains, QR codes, and invite links resolve or introduce a cryptographic identity; none is the root identity.
3. **No provider is part of the protocol.** Provider-specific behavior sits behind capability-driven adapters.
4. **Servers are untrusted by default.** Private content is encrypted on trusted endpoints before provider storage or relay whenever technically possible.
5. **No silent downgrade.** A transport, bridge, bot, web surface, or recovery path with weaker properties must advertise that boundary to the user and core.
6. **Portability is tested.** Migration and restore are protocol test cases, not marketing claims.
7. **Abuse resistance is part of the protocol.** Open source means hostile clients can remove local limits; relays and recipients must enforce cheap, server-side controls.
8. **Delivery correctness precedes features.** Idempotency, retries, deduplication, ordering, acknowledgements, offline delivery, revocation, and partial failure have explicit semantics.
9. **Crypto is adopted, not invented.** OM defines integration contracts and state handling around reviewed cryptographic protocols/libraries rather than creating new primitives.
10. **Normal users see a messenger, not infrastructure.** Provider choice and sovereignty are available without making routine messaging an administration task.

## Dependency-ordered architecture gates

### Gate A — Identity, devices, aliases, and recovery

**Questions to freeze**
- Canonical account identifier and signing/root-key model.
- Device identity, linking, trust, revocation, and replacement.
- `@username` semantics and `DirectoryProvider` contract.
- Phone numbers as recyclable, time-bounded aliases rather than identity; alias churn/reassignment semantics.
- Local `PersonRecord` behavior that collapses historical numbers/accounts without losing provenance.
- QR/invite representation and pairwise contact establishment.
- Recovery options without a provider-held plaintext master key.
- Direct account-continuity proofs, optional pre-authorized threshold recovery guardians, and non-authoritative contact attestations.
- Key rotation and continuity proof after provider/directory migration or account replacement.
- Separation of recognition hints (photo/name/mutual contacts) from authentication evidence.
- Domain-backed aliases (`alice@example.com`) and organization-delegated identities.
- Verification evidence model: domain proof, continuity, direct verification, organization credentials, and optional verifier attestations.
- Public-figure/business name protection without making a single global directory the root of identity.
- Alias/key transparency or another reviewed anti-equivocation mechanism for directory mappings.

**Output**
- Identity ADR.
- Identity document schema.
- Person/Account/Alias state model.
- Continuity and social-recovery proof schema.
- Device lifecycle state machine.
- Recovery threat-model section.
- Test vectors for device add/revoke/recover/migrate, phone-number reassignment, and account replacement.

**Exit criterion**
A user can change username directory, relay, or storage provider and still prove they are the same account to existing contacts.

### Gate B — Canonical event and conversation model

**Questions to freeze**
- Stable event envelope/versioning.
- Message IDs and idempotency rules.
- Replies, edits, deletion/tombstones, reactions, receipts, typing, membership events.
- Local ordering versus transport arrival ordering.
- Attachment references independent of storage provider.
- How SMS/RCS/bridges map into the model without pretending they have OM-native guarantees.

**Output**
- Event schema and compatibility rules.
- Conversation state-machine specification.
- Transport capability matrix.
- Golden cross-client serialization vectors.

**Exit criterion**
Android, iOS, web, and a reference test harness can independently ingest the same event stream and derive the same conversation state.

### Gate C — Cryptographic session and group-state architecture

**Selected direction**
- Design private group keying around MLS (RFC 9420 / RFC 9750); do not invent OM group cryptography.
- Human group membership is `AccountID`-level; cryptographic membership is `DeviceIdentity`-level.
- Roles/admin ACLs remain above MLS.
- Default pre-join history is `since_join`; optional history sharing uses encrypted HistoryBundles rather than old MLS epoch secrets.

**Questions to freeze**
- Reviewed 1:1 session protocol/library choice.
- MLS implementation/library, cipher suite, credential binding, KeyPackage lifecycle, and licensing.
- Multi-device fan-out/session management.
- GroupState-to-MLS epoch binding and concurrency/fragmentation repair.
- Safety-number/key verification behavior.
- History transfer between trusted devices.
- Handling stale devices, compromise, recovery, and forks.

**Output**
- Crypto integration ADR; no homegrown algorithms.
- Library/license review.
- Protocol test vectors and negative tests.
- Group-state threat model.

**Exit criterion**
Independent clients can pass 1:1 and group interoperability vectors across device-add/remove, offline, reorder, replay, and key-change cases.

### Gate D — Relay protocol and contact-capability model

**Questions to freeze**
- Store-and-forward envelope API.
- Pairwise/rotatable delivery capabilities.
- Queue creation/rotation/revocation.
- Expiry, acknowledgement, retry, deduplication, and maximum-object rules.
- Relay descriptor signing, selection, failover, and migration.
- What metadata a relay necessarily sees.

**Anti-abuse rule**
`@username` must not be a permanent public write endpoint. Discovery introduces identities; accepted relationships receive private delivery capabilities.

**Output**
- Relay protocol spec + OpenAPI/JSON schemas.
- Reference relay.
- Relay conformance suite.
- Abuse-budget model.

**Exit criterion**
Two clients can communicate through two independently implemented conforming relays and can migrate relay providers without changing identity or contacts.

### Gate E — Multi-device synchronization

**Questions to freeze**
- Per-device queues and acknowledgement cursors.
- History sync versus live message delivery.
- New-device bootstrap and existing-device authorization.
- Device revocation propagation.
- Conflict resolution when multiple devices mutate state.
- Web as an explicitly linked cryptographic device.

**Output**
- Device-sync state machine.
- Offline/reorder/replay test matrix.
- Web-linked-device security policy.

**Exit criterion**
Phone A can be offline, Phone B and Web can continue, and later all trusted devices converge without server plaintext or phone-proxy dependence.

### Gate F — StoragePool, hot data, cold data, and restore

**Questions to freeze**
- `BlobStore`, `BackupStore`, `RecoveryStore`, and `StoragePool` contracts.
- Hot-storage policy versus cold/archive retention.
- Storage classes: delivery queue, hot sync, hot media, cold archive, recovery.
- Content-addressing/deduplication strategy after encryption constraints are understood.
- Encrypted manifests, snapshot sequencing, integrity, and rollback protection.
- Provider-aware quotas/free-tier guardrails.
- Mirroring first; optional erasure coding later.
- Re-homing received media into recipient-owned storage.

**Output**
- StoragePool ADR and schema.
- Backup/restore format specification.
- `mirror-1`, `mirror-2`, and `mirror-N` policy tests.
- Provider capability profile contract.

**Exit criterion**
A test account can restore from supported storage providers without OM-operated infrastructure and can move hot/cold storage independently.

### Gate G — Abuse, spam, DDoS, and bot security

**Questions to freeze**
- Unknown-contact request limits.
- Cheap validation before expensive work.
- Per-capability/recipient/relay rate and size limits.
- Proof-of-work, reputation, invite quotas, or paid-cost mechanisms only if justified by threat testing.
- Blocking/reporting while minimizing centralized identity authority.
- Bot identity declaration and scoped permissions.
- Group bot access visibility.
- Federation/relay-level denial mechanisms for abusive operators.

**Output**
- Abuse threat model.
- Relay enforcement contract.
- Bot permission schema.
- Load/adversarial test plan.

**Exit criterion**
An unmodified relay can reject abusive hostile clients without trusting client-side limits, while legitimate unknown-contact introduction remains usable.

### Gate H — Plugin/capability model and sandbox boundaries

**Questions to freeze**
- What is a built-in adapter versus third-party plugin.
- Capability negotiation/versioning.
- Secrets each provider plugin may access.
- Process/sandbox boundary for untrusted extensions.
- Plugin signing/distribution/update policy.
- Provider health and failure reporting.

**Output**
- Provider capability schema v1.
- Plugin security ADR.
- Minimal SDK contract.

**Exit criterion**
A third party can implement a new storage or relay provider without modifying the cryptographic core or receiving unrelated secrets.

### Gate I — Transport and bridge security

**Questions to freeze**
- Transport adapter capability vocabulary: encryption, receipts, edits, media, groups, max sizes, identity strength.
- Security labels for OM-native, SMS/MMS, RCS, Matrix/XMPP/other bridges.
- Bridge trust boundary and explicit encryption termination.
- Fallback policy and anti-downgrade rules.

**Output**
- Transport adapter spec.
- Capability/security matrix.
- UI-facing security-state vocabulary.

**Exit criterion**
The conversation core can route through multiple transports without treating weaker transports as OM-native E2EE.

### Gate J — Versioning, conformance, governance, and survivability

**Questions to freeze**
- Wire-version negotiation and deprecation rules.
- Compatibility guarantees.
- Reference vectors/conformance badge requirements.
- How protocol changes are proposed and accepted in an open-source project.
- Continuity plan if the original organization disappears.

**Output**
- Protocol versioning policy.
- Conformance suite structure.
- Governance/compatibility ADR.
- Minimal continuity/recovery documentation.

**Exit criterion**
A third party can implement a compatible client/relay/provider from public specifications and tests without private coordination with the original team.

## What we should decide next

The **immediate next architecture sprint is Gate A: Identity, Devices, Aliases, and Recovery.** It sits underneath relay migration, storage migration, contacts, anti-spam introductions, multi-device, and restoration. Choosing storage or relay wire formats before identity continuity is frozen would risk designing around the wrong root object.

Recommended decision order inside Gate A:

1. Define canonical account ID and root signing authority.
2. Define device IDs/keys and device certificate chain.
3. Define contact identity record presented to another user.
4. Define `@username`/directory resolution as replaceable aliasing.
5. Define QR/invite/contact-request capability.
6. Define device add/revoke/replacement state transitions.
7. Define recovery threat cases and acceptable recovery modes.
8. Prove identity continuity through directory + relay + storage migration.

## Architecture artifacts expected before Phase 1

- `SYSTEM_ARCHITECTURE.md`
- `ARCHITECTURE_PHASE_PLAN.md`
- Threat model with resolved Phase-0 properties.
- ADRs for identity, cryptography, relay, multi-device, storage, abuse, plugins, bridges, versioning.
- Versioned machine schemas for identity/event/relay/storage/provider capabilities.
- Cross-client serialization + crypto test vectors.
- Relay and provider conformance harnesses.
- Restore/migration test suite.
- Small reference relay and reference provider adapters.

## Explicit Phase-0 non-goals

- Perfect UI implementation.
- Calls/video architecture beyond reserving interfaces.
- Massive-group/channels optimization.
- Production-scale federation economics.
- Erasure coding implementation.
- Full SMS/RCS integration.
- Monetization.
- Novel cryptography.
- Premature microservice topology.

## Complexity-control rule

Every new subsystem must answer four questions before it enters the architecture:

1. What stable contract does the core need from it?
2. Can it be replaced without changing user identity or canonical conversation data?
3. What secrets and metadata can it observe?
4. What is the smallest conformance test proving another implementation can replace it?

If those answers are unclear, the subsystem is not ready to become part of the protocol.


## Client/execution direction — selected

- OM Core and wire protocols are client-neutral.
- The official app is the reference implementation, not the definition of OM.
- Independent clients/UIs are explicitly supported through public specs, SDKs, and conformance tests.
- Arbitrary executable plugins inside the official client are deferred; independent clients are the safer extension mechanism.
- First execution may be a web/PWA OM-native reference client for rapid protocol validation.
- Android native must follow early enough to validate lifecycle, secure-key, push, and eventual default-SMS requirements.
- Web-first is not a commitment to web-only production.

See `CLIENT_AND_EXTENSION_ARCHITECTURE.md` and `INTRODUCTION_AND_SERVICE_ACCOUNTS.md`.



## Gate B selected direction — 2026-08-18 14:03 PHT

See `EVENT_MODEL.md`.

Selected:

- immutable durable ConversationEvents + materialized local views;
- control-state protocols remain separate;
- ephemeral typing/presence is non-durable;
- local UI preferences are not conversation protocol;
- 128-bit random client-generated EventID;
- Hybrid Logical Clock for deterministic merge/display ordering without a central sequencer;
- baseline durable kinds: create/edit/delete/reaction/receipt;
- 64 KiB baseline event limit;
- large media is encrypted out-of-band;
- small capability/extension mechanism with safe fallback.

### Immediate Gate-B work

1. deterministic CBOR/CDDL wire draft;
2. diagnostic JSON mapping;
3. HLC exact algorithm;
4. golden event vectors;
5. reorder/dedupe/convergence fixtures;
6. adapter provenance structure.

Once these vectors exist, begin Gate C crypto/library selection and the PWA P0/P1 harness in parallel.
