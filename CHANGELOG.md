## 2026-09-23 09:20 PHT — phase-1 architecture verification evidence

- preserved the complete 08:21 current tree under `history/20260923-0821/`;
- added a TLA+ `AccountControlState` model/configuration and transition regression vectors without claiming TLC execution;
- added bounded authority/state/convergence models covering V-002/V-004/V-008/V-009/V-011/V-016;
- added bounded bridge/archive/metadata models for V-013/V-014/V-015 and an offline continuity dry run for V-017;
- added two independently written Python/Node.js deterministic-CBOR foundation encoders with 14 agreeing fixtures for V-005 groundwork;
- executed 7,772 bounded/adversarial cases plus the CBOR and continuity checks with no modeled invariant violation;
- documented the V-005 gap: per-object CDDL/numeric-field/domain-separation registries still need to be frozen before that gate can pass;
- added standards/runtime feasibility reconnaissance for V-001/V-006/V-007/V-010/V-012;
- kept every V-001..V-017 gate open and `engineering_authorized=false`; evidence is not gate passage.

## 2026-09-23 07:56 PHT — 0M / permissionless services / bridges / escape hardening

- added **0M** as the seventh canonical OM meaning: zero parties have total control;
- made ServiceID creation local/permissionless with optional replaceable hosting, discovery and catalogs rather than a BotFather-style root issuer;
- replaced a human-vs-bot cognitive distinction with authority-based authorship and scoped `RepresentationGrant` for autonomous personal agents;
- resolved A-011 with typed BridgePrincipal, bridge-attested foreign provenance, explicit E2EE termination, fidelity mapping and loop lineage;
- resolved A-012 with public `om-archive-v1`, history/authority separation and provenance-safe foreign imports;
- added V-013/V-014 and expanded V-011;
- preserved the complete 07:18 tree under `history/20260923-0718/`;
- engineering remains unauthorized.

## 2026-09-23 07:18 PHT — Extension ecosystem / bots / expressive-media hardening

- resolved A-001/A-002/A-003/A-004/A-009 at architecture-design level;
- froze bounded inert WebP expressive-media profiles and self-contained conversation rendering;
- added portable immutable ContentPack releases, update lineage, provenance/fork semantics and optional entitlement proofs while keeping commerce external;
- added typed ServicePrincipal/ServiceID + ServiceInstallation architecture for bots, agents and local/remote LLMs;
- generalized ConversationEvent authorship to typed PrincipalRef + authenticated endpoint rather than forcing automation into human AccountIDs;
- froze capability-sandboxed WebAssembly Component Model/WIT local plugins plus isolated web apps and anti-fragmentation rules;
- added V-010..V-012 so plugin sandboxing, service authority/automation and media/pack parser safety remain verification-blocking;
- engineering remains unauthorized.

# 2026-09-22 23:02 PHT — architecture hardening / anti-capture checkpoint

- superseded the 14:34 engineering transition; returned phase to architecture hardening / pre-engineering;
- added canonical project philosophy/anti-capture constitution;
- selected layered licensing/contribution direction: MPL core, AGPL reference network services, Apache normative protocol/conformance artifacts, CC BY-SA documentation, DCO/inbound-equals-outbound and no ordinary broad copyright assignment;
- defined SeedLife/lonunjie as initial steward, with founder-only S0 custody accepted until trustworthy maintainers emerge and succession rails prepared now;
- made asynchronous handshake/MLS key-material publication a first-class provider/state-machine boundary;
- added one-to-one multi-device session management above the cryptographic ratchet;
- added MLS same-epoch Commit ordering and atomic OM group-policy/MLS binding requirements;
- added catastrophic RelaySet reachability recovery and stable BlobID/mutable BlobLocationRecord repair;
- added unified clock/time/expiry semantics;
- added self-hosted ClientHost update/deployment lifecycle;
- canonicalized AccountID as random 256-bit stable data, AccountControlState terminology, and one current ProviderCapabilities schema;
- added machine repository contract, architecture map/file registry/stewardship registry and executable drift validator;
- added ADR-0069 onward and new requirements/open questions/protocol logical schemas;
- preserved the complete 14:34 checkpoint under `history/20260922-1434/`.

# 2026-09-22 13:42 PHT — shared UI / OwnMessenger design checkpoint

- Added one shared `om-ui` architecture across PWA/iOS/Android/Desktop official clients.
- Added adaptive Compact/Medium/Expanded/Wide layout system and Chats/Calls/People IA.
- Added complete onboarding/discovery/share-contact/message-request/call/security/recovery/device/OM Home UX.
- Added semantic MotionSystem and perceived-performance targets.
- Added data-only theme/font architecture with CSP/Trusted Types hardening and theme-proof security semantics.
- Added shared accessibility/input release gates and PlatformAdapter capability contract.
- Added ADR-0051 through ADR-0058, design seed artifacts and current UI references.

# Changelog — 2026-09-21 master snapshot

## 2026-09-21 21:32 PHT — continuity / trust / personal-bootstrap checkpoint

- added AccountControlState, two-stage revocation, controller-promotion safeguards and head gossip;
- made recovery optional/off-by-default and separated identity/history/provider recovery;
- made user-owned Cloudflare + workers.dev the initial personal bootstrap model;
- reduced no-payment Cloudflare core to Worker + SQLite DO; R2 optional;
- added signed ContactCapsule Share Contact flow;
- added update/supply-chain trust, privacy profiles, notification policy, backup anti-rollback, retention and extensibility design;
- added ADR-0026..0042, logical schemas, requirements/open questions and current research references.

## 2026-09-21 10:08 PHT — Discovery Fabric / resilience checkpoint

- Replaced the implicit global-looking username model with an explicit Resolver + namespaced DirectoryProvider architecture.
- Added `ContactDescriptor` as the common output of directory, domain, QR/link, contact-card and LAN discovery.
- Added optional OHTTP/ODoH-inspired oblivious directory lookup.
- Added revocable/scoped Invite design and explicit offline-revocation limits.
- Added public `IntroductionServiceDescriptor`, short-lived `IntroductionTicket`, and IntroGate anti-spam/DDoS boundary.
- Added first-class multi-relay `RelaySet`, distinct capability per relay, automatic failover and optional redundant delivery.
- Added privacy-preserving rotating LAN rendezvous and offline-local-mode direction.
- Added modular censorship/network-resilience architecture with pluggable transports; explicitly no uncensorability guarantee.
- Added ADR-0019 through ADR-0025 and five new logical protocol schemas.
- Preserved prior 08:01 PHT checkpoint under `history/20260921-0801/`.


This is a design changelog, not a software release changelog.

## Philosophy and governance

The architecture now explicitly rejects the idea that safety should depend on a permanently benevolent universal platform operator. OM still permits authorities, but each authority is deliberately narrow. Users control their own relationships; group administrators control their own groups; relays control their own service; directories control their own namespace; media providers control media transport allocation; none of those powers should compose into universal content or identity control.

A recurring review question is now:

> **Where did authority go?**

Every new component must answer that question and show that it does not silently re-centralize OM.

## Cryptography

Crypto agility is upgraded from a library abstraction into a protocol property. OM negotiates approved, versioned CryptoProfiles and records downgrade floors. Account continuity is no longer allowed to depend on a particular signature/KEM family. PQ/hybrid support is preferred, but draft or experimental suites remain explicitly replaceable.

## Relay and metadata

AccountID is never a message-routing address. Accepted relationships receive random opaque delivery capabilities that can rotate. Per-destination transport wrappers should prevent easy cross-relay correlation of identical group ciphertext.

## Storage

Delivery queue, local database, hot sync, attachment blobs, cold archive, and recovery are separate storage classes. R2 is an example BlobStore, not “the OM database.”

## Calls

The old “WebRTC later” note is replaced by a complete media architecture: encrypted signaling through OM, SFrame above WebRTC/SRTP, adaptive LAN/P2P/TURN/SFU routing, strict no-silent-privacy-downgrade behavior, and provider-independent `MediaProvider`/`RoutePolicy` interfaces.

## Cloudflare

Cloudflare is documented as a strong zero/low-friction reference backend:

- Workers/Durable Objects: possible relay implementation;
- R2: possible encrypted blob store;
- Realtime STUN/TURN/SFU: possible call connectivity/media provider.

OM core does not branch on “Cloudflare.” Provider quotas/pricing are discovered/configured operationally and must not become wire semantics.

## Carrier messaging

SMS/MMS/RCS remain architectural adapters but are removed from first implementation scope. Store/carrier approvals must not block OM-native messaging or calls.

## 2026-09-22 07:34 PHT — Operatorless core / client ecosystem checkpoint

- Made the operatorless nature of OM explicit: no mandatory OM-operated runtime service.
- Added PWA-first/not-PWA-only client strategy and shared portable OM Core direction.
- Added operatorless provisioning and Deploy-to-Cloudflare/template path.
- Added local EvidencePackage export and removed any architectural requirement for a central AbuseDesk.
- Split protocol/project/deployment/provider/distribution legal/operational roles.
- Added optional convenience-provider/business model without protocol privilege.
- Added Creator-Disappearance Gate.
- Added ADR-0043 through ADR-0050 and three new logical schemas.

## 2026-09-22 14:34 PHT — Engineering handoff / final design-gap checkpoint

- moved phase from architecture consolidation to architecture candidate + engineering/adversarial validation;
- added engineering start/handoff/repository documents;
- added ClientHost/DeviceVault/PWA migration architecture;
- added multi-IdentityProfile isolation now, visible multi-account switching later;
- added semantic RichTextV1 and hostile-content/link-preview safety design;
- froze durable-control-signature vs message-deniability direction;
- added LocalDataClass and privacy-safe diagnostics;
- selected deterministic CBOR direction for canonical signed/wire objects;
- made AccountControlState TLA+ model a production implementation gate;
- added production crypto integration gate and current security/crypto re-audit;
- required light/dark/Auto for all themes and optional distinct profile theme suggestions;
- added ADR-0059..0068 and new logical schemas;
- preserved 13:42 checkpoint under `history/20260922-1342/`.

## 2026-09-23 05:45 PHT

- resolved former Q-001..Q-061 architectural choices and replaced generic open-question tracking with typed resolution/design/verification/release/deferred ledgers;
- removed country-specific canonical legal gating; adopted jurisdiction-neutral role-scoped operational/legal boundaries;
- strengthened standard-license warranty/liability and downstream-operator separation without field-of-use restrictions;
- made the six meanings of OM canonical in `NAMING.md` and project philosophy;
- added pre-engineering architecture queue for expressive media/stickers/GIF-like animation, portable external content packs, creator-commerce boundaries, bots/LLMs, communities/spaces/rooms, broadcast channels, roles/topics/plugins/bridges and first-release messenger baseline;
- added ADR-0079..0083 and new machine requirements/decisions;
- preserved the 2026-09-22 23:02 checkpoint under `history/20260922-2302/`.

## 2026-09-23 06:15 PHT — Messenger baseline + Spaces/Rooms/Communities hardening

- froze ordinary first-release messenger semantics and separated shared protocol state from local/account-private UX;
- unified interactive private groups/Rooms on ConversationID;
- defined Spaces/Communities as independent-object containers with bilateral links and explicit authority-parent delegation;
- selected capability grants + restrictions + delegation ceilings and no protocol-required magical owner/Admin bypass;
- selected layered replies/threads/Topics;
- separated private announcement Rooms from signed public Publications;
- resolved A-005/A-006/A-007/A-008/A-010 and added V-008 authority-model plus V-009 baseline-event convergence verification;
- added blocking A-012 import/export escape, A-013 metadata threat model, A-014 decentralized abuse/moderation hardening and A-015 protocol longevity so the broader hardening agenda remains machine-visible;
- preserved the complete 05:45 current tree under `history/20260923-0545/`;
- engineering remains unauthorized.

## 2026-09-23 08:21 PHT — metadata / decentralized safety / 0M longevity hardening

- resolved A-013 with a dedicated metadata threat model: canonical IDs remain above provider routing; private delivery uses scoped opaque capabilities/randomized outer envelopes; exposure is declared per role/path; stronger anonymity/padding remains explicit policy;
- resolved A-014 with layered user/Room/Space/provider enforcement, selective evidence, optional signed SafetyAdvisories, labeler-scoped pseudonymous subject references and no universal moderator/reputation/master blocklist;
- resolved A-015 with Protocol Continuity Bundles, immutable registry meanings, explicit generation/fork lineage, crypto/project/provider failure migration and dominant-client/extension anti-capture rules;
- added `METADATA_THREAT_MODEL.md`, `DECENTRALIZED_ABUSE_MODERATION.md`, `PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` and supporting logical schemas/research references;
- added ADR-0098..ADR-0101, decisions D-120..D-123, requirements R-146..R-160 and verification gates V-015..V-017;
- closed the current A-001..A-015 design queue while keeping engineering unauthorized pending all blocking verification gates;
- preserved the complete 07:56 tree under `history/20260923-0756/`.
