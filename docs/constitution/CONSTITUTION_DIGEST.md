# OM Constitution Digest

> **Derived and non-normative.** This is a short index of the nine constitution documents, written so people and tools can
> load only the rules a task needs. If it disagrees with an original, **the original wins**. It replaces nothing in the
> design gate's reading duty ([`AGENTS.md`](../../AGENTS.md)), changes no audited text, and adds no rule: every line
> cites its source. Find rules with `python3 tools/constitution.py find <topic>`; show one with
> `constitution.py show C-12 --source`. The machine copy is [`constitution_index.json`](../../machine/constitution_index.json),
> generated from this file by `constitution.py gen`.
>
> Source codes: PHIL = PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE, LIC = LICENSING_AND_CONTRIBUTION_POLICY,
> STEW = PROJECT_STEWARDSHIP_AND_CONTINUITY, OPLESS = OPERATORLESS_CORE_AND_ECOSYSTEM, PRIV = PRIVACY_GOVERNANCE_AUTHORITY,
> LEGAL = LEGAL_AND_OPERATIONAL_BOUNDARIES, EXT = PROTOCOL_EXTENSIBILITY, LONG = PROTOCOL_LONGEVITY_AND_0M_CONTINUITY,
> NAME = NAMING. `§` names a heading or its number in that document.

## Operatorless core {tags: operatorless, survivability}

- **C-01 No required OM operator.** OM must exist as a protocol and open-source project without an OM company, account service, relay fleet, master directory, push gateway, recovery service or abuse desk. [OPLESS §1]
- **C-02 The project holds only public artifacts.** It publishes specs, core, vectors, reference client source, advisories, release metadata and docs, and needs no accounts, keys, plaintext, traffic logs, mailboxes, contact graphs, backup keys, universal blocklist or abuse-report database. [OPLESS §2]
- **C-03 Users operate their own endpoints.** Most users start on a sponsored or hosted Home run by someone else, and whoever operates a Home's roles is one observer for that user. [OPLESS §3; ADR-0102, ADR-0130]
- **C-04 Creator-Disappearance Gate.** Already-accepted communication must keep working if the author, website, forge, client distributor or any convenience provider disappears. [OPLESS §4; PHIL §P-02]
- **C-05 Convenience is allowed, privilege is not.** Businesses and a future foundation may sell or fund convenience but never become mandatory identity, relay, directory, recovery, network-wide update or policy authority, and users can migrate away keeping their AccountID. [OPLESS §5; OPLESS §6; PRIV §2026-09-22 governance refinement]
- **C-06 No indispensable institution.** No project, foundation or company may be indispensable to already-established private communication; a feature that cannot meet this needs an explicit, optional, replaceable dependency that cannot redefine identity or history. [PRIV §2026-09-22 governance refinement; PHIL §P-02]
- **C-07 Operatorless is not metadata-free.** Providers learn some metadata and communities may moderate; what is forbidden is any provider, safety or project role becoming universally mandatory. [OPLESS §10] #privacy

## Authority and identity {tags: authority, identity}

- **C-08 Scoped, replaceable, non-composable.** Every authority must be scoped to its function, replaceable, and unable to combine with others into universal control. [PRIV §Core authority rule; ADR-0009]
- **C-09 Authority stays local.** A relay, group admin, directory or media provider can act only within its own relay, group, namespace or allocation and cannot revoke identities globally, rewrite keys, decrypt media or add participants. [PRIV §Scoped]
- **C-10 Providers are swappable.** A user can switch relay, blob store, directory, media provider or push strategy without a new identity. [PRIV §Replaceable; PHIL §P-01]
- **C-11 Roles do not compose.** Roles use distinct identifiers and minimum metadata so one vendor running several roles cannot rebuild the social graph from identifiers and keys, but it can still correlate by timing and network address and OM declares that instead of assuming non-collusion. [PRIV §Non-composable; ADR-0130]
- **C-12 Users are principals.** Identity, history meaning, relationships and protocol authority never belong to relays, directories, storage, push, media, discovery, app stores or managed services. [PHIL §P-01]
- **C-13 Users keep their own choices.** The user controls their devices, contacts, recovery policy and provider choices, and never anyone else's identity. [PRIV §Authority map] #recovery
- **C-14 Ask where authority went.** Every new feature must name what each new actor can observe, deny, modify, impersonate or correlate, and whether replacing it is survivable. [PRIV §Design review question]
- **C-15 No global username registry.** Directories control only their own namespace and search, a local Resolver may use many directories, domains, invites or LAN rendezvous, and an accepted relationship needs no discovery infrastructure. [PRIV §Discovery does not recreate the central authority]
- **C-16 Abuse control without surveillance.** Safety is layered (constrain unknown senders, block and report, group rules, provider-local rules, deliberate evidence disclosure, endpoint-targeted lawful investigation) with no universal decryption mechanism or global ban. [PRIV §Governance without universal surveillance]
- **C-17 Identity outlives any one dependency.** Losing the founder, forge, website, domain, cloud, directory, app store, reference client, bot host, labeler, relay or registry must not destroy AccountID, ConversationID, SpaceID, ServiceID or held history. [LONG §2]
- **C-18 Stewards never inherit user authority.** A successor steward does not inherit user, Room, Service or Bridge authority or the right to redefine published registry IDs, and no steward asset becomes part of AccountID or end-user authority. [STEW §9; STEW §5; LONG §23]
- **C-19 Three compromises stay distinct.** Compromise of project stewardship, of a user account and of a provider are separate; rotating steward credentials never changes user identities. [STEW §7]

## Fail closed, never silently downgrade {tags: security, fail-closed}

- **C-20 Open does not mean weak.** Interoperability never justifies lowering a security or privacy floor, and changes touching authentication, confidentiality, identity, authority, downgrade or wire compatibility are explicit, versioned, testable and fail safely. [PHIL §P-06]
- **C-21 Unknown critical semantics are refused.** An implementation that does not understand a critical extension or authority/security semantic must refuse the affected operation instead of guessing. [EXT §2; LONG §5]
- **C-22 Security semantics default to critical.** Authentication, membership, crypto, recovery and authority semantics are critical unless explicitly classed otherwise. [EXT §2] #recovery
- **C-23 Negotiation cannot be stripped.** Capability negotiation is authenticated and bound into the session, group or control transcript, and clients keep minimum security and profile floors. [EXT §3]
- **C-24 Negotiation picks the highest safe profile.** Implementations intersect validated profiles, drop forbidden or below-floor ones, and choose the highest security generation; profile IDs and extension versions never denote strength. [LONG §15 Evolution; ADR-0165]
- **C-25 Convenience cannot change principles.** "Easier to implement" never justifies central authority, hidden privilege, lock-in or weaker security, and principle-level change needs an ADR plus threat-model and machine-contract updates. [PHIL §P-12]
- **C-26 Ambiguity is unfinished architecture.** Ambiguity at a security, authority, wire, recovery or migration boundary must be resolved in the design, never improvised in implementation, and implementation starts only when the freeze contract allows. [PHIL §P-10]
- **C-27 No hidden emergency bypass.** Urgent crypto-break handling uses normal authenticated release and profile channels and introduces no emergency key. [LONG §7]
- **C-28 Validity needs no live oracle.** An object's validity comes from its declared semantics, published rules, authenticated state and local policy, never from asking a project domain for "latest". [LONG §6]

## Privacy and metadata minimization {tags: privacy, metadata}

- **C-29 Privacy is structural.** OM minimizes what any one party can learn, preferring pairwise opaque capabilities, local-first state, separate roles, explicit disclosure and replaceable providers over observation plus policy promises. [PHIL §P-08]
- **C-30 Oblivious lookup splits metadata.** Optional oblivious lookup lets the discovery relay know the client address but not the query, and the resolver know the query but not the address. [PRIV §Discovery does not recreate the central authority]
- **C-31 Collect less.** The project collects no identity, contact or telemetry for analytics, and keeps diagnostics local, redacted and user-exported by default. [LEGAL §3]
- **C-32 Never overclaim privacy.** Do not describe OM as untraceable, government-proof, impossible to subpoena, law-free or perfectly anonymous. [LEGAL §9]

## Recovery and escape {tags: recovery, continuity}

- **C-33 No recovery authority.** There is no mandatory recovery service or authority, and recovery policy is the user's. [OPLESS §1; PRIV §Authority map; PRIV §2026-09-22 governance refinement]
- **C-34 Continuity after a catastrophic break uses prior evidence.** If old authorization signatures become forgeable, rely on evidence from before the break (hybrid transition credentials, witnessed control heads, relationship-held evidence, independent-primitive recovery, user re-verification) and no master key may seize identities. [LONG §7]
- **C-35 Show how continuity was established.** The UI distinguishes "cryptographically continuous" from "re-established by user or social verification". [LONG §7]
- **C-36 Migration keeps AccountID.** A new generation should offer authenticated account-control transition, device enrollment, relationship rekey, Room migration, history export, and an explicit unsupported or lossy report. [LONG §15 Data; LONG §23]
- **C-37 Every provider role has an escape story.** Escape is proportional to custody; if every route is lost, manual rediscovery is an honest liveness limit, not a reason for a mandatory global provider. [LONG §10]
- **C-38 Users can leave an operator.** Export and migration let users and organizations leave an operator, and no vendor may trap canonical history in opaque local storage. [LEGAL §3; LONG §11]
- **C-39 Provider disappearance is not identity loss.** DNS or domain loss affects discoverability, not cryptographic ownership; delisting never revokes an AccountID or ServiceID. [LONG §9; LONG §20]

## Extensibility, protocol evolution and longevity {tags: extensibility, protocol, longevity}

- **C-40 Every interpretive field has a criticality.** Non-critical fields may be ignored by older implementations (bytes preserved where required); critical ones must be understood or the operation is refused. [EXT §2]
- **C-41 Public versioned registries.** CryptoProfiles, event and control types, transport and provider capabilities, critical extension IDs and security-relevant formats live in public versioned registries with experimental ranges and public governance that makes no implementation mandatory. [EXT §4]
- **C-42 The critical prefix is permanent.** IDs under `om.critical.` are permanently critical, unknown ones are treated as critical, and non-critical registrations may not use the prefix. [EXT §4; ADR-0157, ADR-0155]
- **C-43 Assigned IDs never change meaning.** Registry IDs and `(extension_id, version)` registrations are immutable and never reused, and criticality is immutable per ID. [EXT §7; LONG §4; ADR-0165]
- **C-44 Security-affecting change needs a new ID.** Changes to authority, authentication, confidentiality, membership, downgrade behavior, owning object or fallback validity need a new extension ID or CryptoProfile, never a version bump. [EXT §7; ADR-0165]
- **C-45 Private extensions need no permission.** Experimental and private extensions use collision-resistant namespaced IDs, and market adoption grants no canonical status. [EXT §2026-09-23 registry/extension resolution; EXT §2026-09-23 08:21 longevity/registry resolution]
- **C-46 Shared meaning is never only in plugin code.** An extension that changes interoperable state must publish schema, criticality, canonical encoding and capability semantics independently of one executable plugin. [EXT §2026-09-23 registry/extension resolution]
- **C-47 Canonical encoding before freeze.** Every signed or hashed object has deterministic serialization and published golden vectors before wire freeze, and unknown-field preservation is testable. [EXT §5]
- **C-48 Published versions are self-contained.** Each release has a mirrorable Protocol Continuity Bundle (specs, encoding, vectors, registry and CryptoProfile snapshots, licenses, signatures), and a live registry is a convenience, not a runtime oracle. [LONG §3; LONG §4; EXT §2026-09-23 08:21 longevity/registry resolution]
- **C-49 Generations coexist.** Peers exchange authenticated supported-profile sets, fail closed on critical semantics, and keep old history readable or exportable where its cryptography is still processable. [LONG §5]
- **C-50 Published meaning is immutable.** Corrections that change behavior become a new profile, extension or lineage or an explicit fork, every normative change is classified, and agents must not downgrade the class for ease. [LONG §15 Evolution; ADR-0165]
- **C-51 Plan for crypto failure.** Planned deprecation moves through a new preferred profile, rising floor and forbidden old profile; an urgent break ships through normal authenticated channels. [LONG §7]
- **C-52 Distribution trust is per distribution.** Software-signing roots belong to each distribution, peers do not reject a conforming client for lacking the project's signing key, and "official client" is not a wire-level role. [LONG §8; LONG §21]
- **C-53 Market share is not authority.** A dominant client or extension gains no protocol rights, a proprietary extension cannot become mandatory for baseline conformance, and incompatible change by a dominant vendor is a fork, not a redefinition. [LONG §11; LONG §12]
- **C-54 Baseline graduation has criteria.** An extension enters a baseline only with public semantics, canonical encoding and criticality, a feasible independent implementation, permissive terms, security review and fallback; a Room may visibly require an optional public extension. [LONG §12; LONG §11]
- **C-55 Forks use new lineage identifiers.** A fork never reuses an ID for incompatible semantics, is a last resort, and is one of implementation, stewardship, protocol-generation or Room/Space forks. [LONG §14]
- **C-56 Freeze only what can outlive its library.** Before freezing a primitive or format: public spec, vectors, a credible independent implementation path, permitting licenses, library-independent serialization and a migration path. [LONG §17]
- **C-57 Mirror and avoid single chokepoints.** At public-production maturity mirror source, bundles and vectors on independent channels, need no private CI, and load no security-critical code from an unpinned third-party CDN. [LONG §9; LONG §16]
- **C-58 Platforms cannot redefine the protocol.** The protocol is independent of store identity, push is optional to correctness, and platform limits reduce only that platform's experience. [LONG §18]
- **C-59 Economic and discovery capture are countered by portability.** Provider contracts are public and reimplementable, no proprietary provider capability is baseline, direct invites bypass discovery, and safety advisories are subscribed policy inputs, not protocol truth. [LONG §19; LONG §20]
- **C-60 Continuity is drilled.** Before claiming public-production resilience the project runs the creator-disappearance, forge, domain, provider and release-root drill (V-017). [LONG §22; LONG §24; STEW §9]

## Anti-capture, licensing and stewardship {tags: anti-capture, licensing, stewardship}

- **C-61 Commercial participation yes, protocol privilege no.** Businesses may build clients, run providers and sell support, but SeedLife, the founder and any successor may steward the project without owning the right to make private communication depend on them. [PHIL §1]
- **C-62 Implementable without permission.** A conforming implementation must be possible from public normative artifacts, with no proprietary SDK, secret server, private registry, trademark license, contract or SeedLife authorization. [PHIL §P-03]
- **C-63 No hidden privilege.** Official clients and convenience providers get no secret extension, privileged endpoint, private key hierarchy, unpublished compatibility rule or mandatory service. [PHIL §P-04]
- **C-64 Freedom survives capture.** Public releases are under irrevocable open licenses, normative artifacts are mirrorable, and the community keeps the legal and technical ability to fork if the steward is captured. [PHIL §P-05; LONG §13]
- **C-65 No proprietary mandatory extension.** Canonical OM must not require a royalty-bearing patent license, closed SDK, undisclosed spec, private approval or vendor-only service; optional bridges may use external proprietary systems. [PHIL §P-07]
- **C-66 Ten layered anti-capture mechanisms.** Open normative artifacts, public conformance vectors, replaceable roles, core copyleft, network copyleft on reference providers, no broad copyright-assignment CLA, DCO inbound-equals-outbound, royalty-free patent policy, transparent governance and forkability. [PHIL §3]
- **C-67 No field-of-use limits.** OM does not forbid commercial, paid-hosting, proprietary-surrounding, research, government or competing use, because that would undermine its open-source claim. [PHIL §4]
- **C-68 Stewardship cannot revoke rights.** The canonical project picks its release line, supported versions, trademark use and maintainers, but cannot retroactively revoke granted rights, make AccountIDs depend on it, or stop third-party implementation. [PHIL §5]
- **C-69 Tie-breaker for design conflicts.** Prefer the design that, while usable and secure, gives users more durable agency, needs fewer irreplaceable authorities, exposes less metadata, survives failure, stays independently implementable, eases escape from capture, keeps UX simpler and makes fewer irreversible commitments. [PHIL §7]
- **C-70 Capability is not operation.** Supporting a content pack, bot, community, directory or commerce extension does not oblige the project to host, run, moderate, process payments or become a default provider. [PHIL §9]
- **C-71 Licensing is layered by role.** Core and shared client libraries are MPL-2.0, reference network-service code is AGPL-3.0-or-later, normative schemas, registries, vectors and harness interfaces are Apache-2.0, and docs are CC-BY-SA-4.0. [LIC §1]
- **C-72 No ordinary copyright assignment.** Contributions use DCO 1.1 sign-off with inbound-equals-outbound licensing; a broad relicensing CLA is not required, and any narrow agreement needs public review showing no capture path. [LIC §5]
- **C-73 Patent rule for the protocol.** Canonical OM must not knowingly adopt a royalty-bearing or permission-gated dependency, and suspected encumbrance blocks until a lawyer-reviewed royalty-free patent commitment exists. [LIC §6]
- **C-74 Trademarks are not protocol locks.** No trademark license is needed to implement or interoperate, truthful "OM-compatible" statements stay allowed, and rights remain usable under another name if the trademark fails. [LIC §7]
- **C-75 No closing released versions.** No steward may claim released versions can be closed or granted rights revoked; any future license change needs a public ADR, legal analysis and respect for contributors and anti-capture intent. [LIC §8]
- **C-76 Founder custody is stated honestly.** Early on one founder steward is accepted; release-root custody is always described as 2-of-3 shares held by one custodian, a threshold against loss and online compromise but 1-of-1 against coercion, never as multi-party. [STEW §2; STEW §4; ADR-0130]
- **C-77 Succession rails, not pretend succession.** Roles and key ceremonies are documented before they are populated so authority can become multi-person without protocol migration. [PHIL §6; STEW §3]
- **C-78 Maturity stages gate claims.** S0 founder incubation, S1 trusted maintainer circle, S2 public-production resilience (multiple owners, multi-party release root, independent credential recovery, drills) and S3 institutional longevity; S2 claims need S2 custody. [STEW §4; STEW §10]
- **C-79 Co-stewards come from the community.** Recruiting them is a stewardship milestone, not a W0 or first-release blocker; the first offer is a scoped reviewer or security role before any custody role. [STEW §10; ADR-0114]

## Legal and operational boundaries {tags: legal, operations}

- **C-80 Liability follows the operator.** Compliance attaches as closely as practical to the actor that operates, publishes, hosts, distributes, transacts or controls the service; the project is not a universal intermediary, and open licensing or cryptography does not erase mandatory law. [LEGAL §1; OPLESS §8]
- **C-81 Minimize project exposure.** Unless a later ADR says otherwise, the project runs no mandatory account service, relay, blob store, directory, bot cloud, community host or payment processor, holds no mandatory plaintext or decryption key, avoids a global marketplace, and publishes releases separately from optional hosted services that are separately identifiable operators. [LEGAL §3]
- **C-82 No custom restrictive EULA.** The project gives no default warranty, SLA, indemnity, legal advice or compliance certification; role-specific terms come from the actor offering the service. [LEGAL §4; LIC §11]
- **C-83 Private by default, public by choice.** Private messaging is the baseline, public discovery is a distinct optional provider, a catalog operator answers for what it lists, signatures prove provenance not copyright ownership, and the client offers report, block and export without a universal moderation desk. [LEGAL §5]
- **C-84 OM does not handle money.** OM Core processes no payments, custodies no value, issues no token and is not merchant of record; extensions may carry an opaque entitlement proof only. [LEGAL §7]
- **C-85 Review legal duties per role and place.** No country is a home jurisdiction; before a project-controlled distribution or service is broadly offered, review that role in the places it operates. [LEGAL §8]

## Naming and product promise {tags: naming, ux}

- **C-86 The meanings are product constraints.** OM keeps its name because it carries several compatible meanings, and O may be read as 0 (0M) without a rename. [NAME §Open-source Messenger; NAME §0M]
- **C-87 Open-source Messenger.** Protocol, normative artifacts and reference implementation stay independently usable, forkable and improvable under the open licensing architecture. [NAME §Open-source Messenger]
- **C-88 OpenMessenger.** Independent clients implement the public protocol without permission, and interoperation with services, bots, LLM agents and bridges is a goal within explicit security boundaries and user consent. [NAME §OpenMessenger]
- **C-89 OwnMessenger.** Identity, infrastructure, client, themes, media, automations and extensions can be replaced or customized without weakening the security floor or needing a central account. [NAME §OwnMessenger]
- **C-90 OurMessenger.** Shared spaces may carry their own roles, rules, rooms, services and governance without making the OM Project their operator. [NAME §OurMessenger]
- **C-91 OneMessenger.** Optional features live behind interoperable extensions and capability negotiation, "One" means breadth and interoperability, and OM may not centralize control to win adoption. [NAME §OneMessenger]
- **C-92 0M means zero total control.** Zero does not mean zero servers or authorities; it means no founder, company, government, cloud, app store, directory, provider, dominant client or extension is the one party whose permission OM needs to continue, so authorities stay scoped, replaceable and non-composable. [NAME §0M; PHIL §P-13]
- **C-93 Practical anti-capture, not theater.** The target is identity that outlives providers, services without a central issuer, competing directories, independent clients, self-governing communities, movable infrastructure, mirrorable artifacts, and no commercial success turning into protocol privilege. [NAME §0M]
- **C-94 Naming tie-break list.** For ambiguous product choices: keep the commons open, interoperate, allow safe customization, support shared spaces, prefer extensible capability over fragmentation, keep a simple delightful default, and keep total control at zero. [NAME §Interpretation]
- **C-95 Complexity belongs under the product.** Normal users get a messenger, not a distributed-systems control panel, with strong defaults, automatic safe migration and advanced controls that do not require understanding relays, ciphers, stores or governance. [PHIL §P-09] #ux
