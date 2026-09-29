# OM — Consolidated Product & Technical Blueprint

**Status:** Canonical architecture direction as of 2026-09-21.

## 1. Executive summary

OM is an open secure messenger optimized for ordinary users and hostile assumptions underneath. It combines:

- cryptographic self-sovereign account continuity;
- provider-independent relays/storage/directories/media;
- private capability-addressed delivery rather than globally routable account identifiers;
- post-quantum/hybrid-ready CryptoProfiles with invisible migration;
- reviewed 1:1 ratchet and MLS group architecture;
- local-first encrypted state and portable storage;
- optional V1 same-LAN encrypted message/file delivery where privacy-preserving rendezvous and platform capability permit it;
- WebRTC/SFrame calls using LAN/P2P when practical and TURN/SFU when required;
- a transport adapter layer ready for future SMS/MMS/RCS without blocking v1;
- public protocol/conformance so OM-the-project is not itself a permanent central authority.

## 2. Product feel

OM should feel closer to a polished mainstream messenger than to a self-hosting tool. The ordinary user sees people, messages, files, and call buttons. Infrastructure is automatic. Advanced users can override providers, but no core capability requires that knowledge.

## 3. Security philosophy

Privacy is treated as a user right/power, not as a claim that private systems cannot be abused. Abuse controls exist at first-contact, recipient, group, provider, and reporting boundaries. OM does not create a universal decryption capability merely to make moderation easier.

The design therefore prefers **scoped authority** over either total anarchy or universal platform control.

## 4. Identity

`AccountID` is a stable random/provider-independent identifier for an account lineage. It is independent of phone number, username, email, relay, directory, storage provider, or cryptographic algorithm.

Devices have independent credentials and can be revoked independently. Account credential state moves through authenticated epochs so algorithms can be replaced.

Aliases are signed/verified claims with provenance. No global username registry is required by the core protocol.

## 5. Discovery and acceptance

OM uses a **Discovery Fabric**, not a mandatory global registry. A client-side Resolver can consume a directory alias, domain-scoped alias, QR/`om:` invite, contact card, NFC handoff, or privacy-preserving LAN rendezvous. Every mechanism yields a validated `ContactDescriptor`.

Useful DNS ideas are adopted—delegation, resolver/authority separation, caching with authenticated expiry, multiple endpoints, and local discovery—without requiring a single OM root namespace. Bare usernames are convenience aliases with retained namespace provenance.

Public directory records expose an `IntroductionServiceDescriptor`, not a reusable private mailbox capability. A sender obtains a short-lived/single-use `IntroductionTicket` from the recipient's IntroGate and submits a constrained MessageRequest. Accepted peers then exchange private rotatable relationship capabilities and a private multi-provider `RelaySet`; discovery infrastructure is no longer required for normal conversation.

Invites are revocable independently of accepted relationships. Optional oblivious lookup can split the client IP from the queried alias using independently operated discovery relay/resolver roles.

## 6. Crypto

OM uses a small approved registry of versioned CryptoProfiles. Profiles specify complete reviewed security combinations; peers do not independently mix arbitrary KEM/signature/AEAD choices.

Preferred posture is PQ-first/hybrid-secure while keeping standards-draft pieces replaceable. Migration is background and automatic; downgrade floors prevent an attacker from forcing old crypto after two peers have upgraded.

## 7. 1:1 messaging

Use an asynchronous reviewed handshake and a continuously evolving ratchet. **OM-Crypto-v1 is PQXDH + Double Ratchet (ADR-0118):** the handshake is hybrid post-quantum, so recorded first messages resist harvest-now-decrypt-later, but there is no post-quantum post-compromise healing in v1. A clean-room SPQR-style PQ ratchet is planned as OM-Crypto-v2 and is not a v1 property. Exact library/primitives are selected by profile ADR and interoperability tests.

Per-device sessions deliver canonical authenticated OM events.

## 8. Groups

Use MLS for private groups. Human membership is AccountID-level; MLS clients are DeviceIdentities. Application roles are distinct from crypto membership but atomically bound to membership-changing epochs.

PQ/hybrid MLS suites remain profile-pluggable while the IETF draft evolves.

## 9. Canonical events

Baseline durable semantic events stay deliberately small:

- message.create
- message.edit
- message.delete
- reaction.set
- receipt.update

Typing/presence/call negotiation is ephemeral. Local mute/personal bookmarks/drafts/search remain local/private unless a separate account sync protocol covers them. Shared Room pins are explicit authorized protocol state (`pin.set`), not local UX state.

Transport/provider identifiers do not define semantic event identity.

## 10. Relay

Relays route opaque queue/capability identifiers, never AccountID. Capabilities are random, scoped, rotatable, and revocable. Relays store bounded ciphertext for offline delivery and delete after acknowledged delivery/expiry.

Multiple simultaneous relays are first-class. Accepted relationships exchange a signed/private `RelaySet` whose entries preferably span independent providers/failure domains. Each relay gets a different capability. Normal delivery selects one healthy route, automatic failover uses another, and optional high-reliability delivery may send independently randomized wrappers through two relays and deduplicate by authenticated `EventID`.

Groups fan out fresh randomized transport wrappers to destination capabilities to reduce cross-relay correlation.

## 11. Storage

Local encrypted state is authoritative for the client. Attachments are encrypted before upload. `StoragePool` selects/mirrors/migrates BlobStore/BackupStore/RecoveryStore providers.

R2/S3/WebDAV/NAS/local storage are implementations.

## 12. Multi-device

No master phone. Android/iOS/web/desktop are independently authorized devices. Device linking and history transfer are encrypted account operations. Revoked devices lose future access.

## 13. Calls

Call signaling is E2EE OM control traffic. Media is protected with SFrame above WebRTC so direct, TURN, and SFU paths share the same content confidentiality.

Routing is adaptive:

1. safe same-LAN direct;
2. Internet direct P2P via ICE/STUN;
3. TURN fallback or privacy relay;
4. SFU for scalable group media.

Unknown callers/strict privacy policy can remain relay-only to hide IP addresses. Small groups may mesh P2P; larger/constrained groups use SFU. Mid-call migration must preserve E2EE.

## 14. Cloudflare

Cloudflare is an excellent reference deployment target but never part of the protocol:

- Workers + Durable Objects: possible relay;
- R2: possible encrypted BlobStore;
- Realtime STUN/TURN/SFU: possible MediaProvider.

The default client can hide all of this from end users. Advanced users/operators may bring their own provider.

## 15. SMS/MMS/RCS

Not required for first implementation. Keep `TransportAdapter` stable so Android SMS/MMS and authorized RCS integrations can be added later. No silent fallback from OM E2EE to carrier plaintext/weaker transport.

## 16. Clients

Native Android/iOS plus web/PWA are first-class goals around a shared secure core (Rust remains preferred). Web is a revocable cryptographic device, not a plaintext mirror. Third-party clients use public specs and conformance.

## 17. Safety

MessageRequests constrain unsolicited contact. Providers enforce resource limits. Users can block/report. Report bundles deliberately disclose selected evidence rather than giving the platform ambient content access. No global trust score is required.

## 18. Cost strategy

Infrastructure is used only where it adds value:

```text
same LAN -> local path
reachable peers -> direct P2P
hard NAT/privacy -> TURN
large group -> SFU
offline delivery -> relay
large attachment -> encrypted BlobStore
archive/backup -> StoragePool
```

Provider adapters expose quota/cost behavior. “Zero paid budget” is a real routing constraint, not just a notification.

## 18A. Network and censorship resilience

OM aims to have no single censorship switch, not to claim impossible uncensorability. This is a design target; no public claim of it is made for a release until PR-013 passes (R-269, ADR-0130). Encrypted OM semantics sit above pluggable network transports and provider sets. Directory, introduction, relay, media, blob and network-transport providers can vary independently. LAN/direct operation, alternate relays/providers, and future proxy/pluggable-transport adapters can preserve reachability in some blocking environments without changing identity or conversation semantics.

## 19. Governance

Public protocol, conformance tests, test vectors, reproducible/release-transparent builds where practical, independent clients, and replaceable registries reduce the chance that OM's original maintainers become the universal controller.

## 20. Initial implementation boundary

First useful OM should prove:

- AccountID/device creation/link/revoke;
- QR/`om:` invite -> ContactDescriptor -> revocable IntroGate/MessageRequest;
- 1:1 E2EE OM-native delivery through replaceable multi-relay RelaySets/failover;
- canonical event convergence;
- encrypted attachments through replaceable BlobStore;
- basic multi-device sync, including bounded account-private draft/preference sync;
- isolated multi-IdentityProfile switching in the reference client;
- ordinary audio/voice-note and one-shot static-location presentation with safe old-client fallback;
- identity verification/continuity;
- profile negotiation/downgrade safety;
- provider migration.

Then harden groups/recovery, then calls, then infrastructure self-service, then carrier transports.

## 21. Non-goals for early releases

- social feed;
- payments/marketplace;
- universal bot platform;
- global reputation score;
- mandatory blockchain;
- server-readable backups;
- arbitrary executable plugins inside official client;
- carrier/RCS approval as a prerequisite to launch;
- custom novel cryptographic primitive design.

## 21:32 checkpoint additions

### Account continuity

OM now has an explicit constitutional control plane. Chat ordering does not decide authority. Emergency quarantine can stop future secrets, but permanent revocation and control-device promotion require stronger authorization under the pre-change state or RecoveryPolicy.

### Recovery spectrum

~~Recovery is `none` by default.~~ **2026-09-24 (ADR-0111):** identity recovery is **on by default** (a synced passkey or recovery key as one vote with delay and veto) and can be turned off by an explicit, confirmed action. Users can add a machine-generated offline recovery root, guardians/shares, spare control devices, hardware-backed methods, and later threshold/MPC methods. Identity recovery and history decryption are separate powers.

### User-owned v1 Cloudflare bootstrap

~~Every early user may own their own Cloudflare account.~~ **2026-09-24 (ADR-0102, ADR-0112, ADR-0130):** most users start on a T0 sponsored starter mailbox or a T1 hosted Home and need no cloud account. Own-cloud (T2) users deploy a verified release through a template (preferred) or an optional OAuth/PKCE installer whose scopes are account-wide; the Worker FQDN under `workers.dev` is their initial personal discovery locator. Organizations/FQDN owners provide the first natural grouped directories. R2 is optional, not baseline.

### Trustworthy software

Open source is paired with secure update/supply-chain foundations from the first public release: signed/mirrored source, role-separated update keys, provenance, dependency pinning and reproducibility where feasible.

### Privacy without niche UX

All profiles retain the strongest approved E2EE. Recommended/Hardened/Maximum Privacy only tune metadata exposure, routing, notifications, recovery/storage/retention and future anonymity transports. Push remains instant and privacy-preserving by default through opaque wakeups and local notification rendering.

## 2026-09-22 checkpoint — operatorless ecosystem

OM now explicitly targets an operatorless public-good core. The first reference distribution is PWA-first for independence, but the long-term client family shares one portable OM Core across web/native shells. Optional businesses may provide managed convenience without owning identity, protocol or network authority.

The architectural survival test is: **if the original OM author/project/business disappears, do existing independent users continue communicating?** The required answer is yes.

## Shared UI/UX architecture — 2026-09-22 13:42

OM's reference product is one adaptive UI, not independent platform designs. `om-ui` sits above the portable Core and below thin platform shells. **ADR-0159 supersedes the earlier three-destination default:** the reference shell is conversation-first, with an Omnibar for people/search/settings/actions and optional user-pinned secondary surfaces such as Calls or People. Settings remains profile-scoped. Layout adapts to actual window size from one pane to list/detail/inspector arrangements.

The default experience is calm, ergonomic, fast and content-first. Telegram-class perceived fluidity is pursued through immediate local response, stable layout, semantic/interruptible motion and local-first rendering rather than ornamental animation.

OwnMessenger personalization is first-class: users can install declarative themes, fonts, wallpapers and validated sounds, while density and motion intensity remain explicit user ergonomic/accessibility preferences. Theme content is sandboxed data-only presentation and cannot execute code, fetch arbitrary remote tracking assets, replace semantic security/action icons, override security semantics or suppress accessibility constraints.

See [`UI_UX_SYSTEM.md`](../architecture/ux/UI_UX_SYSTEM.md), [`MOTION_AND_PERFORMANCE.md`](../architecture/ux/MOTION_AND_PERFORMANCE.md), [`THEMES_FONTS_AND_PERSONALIZATION.md`](../architecture/ux/THEMES_FONTS_AND_PERSONALIZATION.md), [`ACCESSIBILITY_AND_INPUT.md`](../architecture/ux/ACCESSIBILITY_AND_INPUT.md), and [`UI_CORE_PLATFORM_CONTRACT.md`](../architecture/ux/UI_CORE_PLATFORM_CONTRACT.md), [`NAVIGATION_AND_OMNIBAR.md`](../architecture/ux/NAVIGATION_AND_OMNIBAR.md), and [`VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md`](../architecture/ux/VISUAL_LANGUAGE_COLOR_ICONS_AND_ASSETS.md).

## 2026-09-22 23:02 architecture hardening

The earlier engineering transition is superseded. OM remains pre-engineering while the remaining expensive-to-retrofit state-machine boundaries are specified.

New first-class architecture components are [`ASYNC_KEY_MATERIAL.md`](../architecture/crypto/ASYNC_KEY_MATERIAL.md), [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md), [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md), [`REACHABILITY_AND_OBJECT_REPAIR.md`](../architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md), [`TIME_AND_EXPIRY_SEMANTICS.md`](../architecture/events/TIME_AND_EXPIRY_SEMANTICS.md), and [`CLIENTHOST_UPDATE_LIFECYCLE.md`](../architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md).

Project-level anti-capture/continuity is defined by [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`LICENSING_AND_CONTRIBUTION_POLICY.md`](../constitution/LICENSING_AND_CONTRIBUTION_POLICY.md), and [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](../constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md). The canonical project is intended to live under SeedLife/lonunjie while remaining technically and legally forkable if stewardship fails.

Architecture prose is backed by [`machine/repository_contract.json`](../../machine/repository_contract.json), [`machine/architecture_map.json`](../../machine/architecture_map.json), [`machine/file_registry.json`](../../machine/file_registry.json), and [`tools/validate_architecture.py`](../../tools/validate_architecture.py). Product engineering starts only after a future architecture-freeze ADR deliberately unlocks it.


## 2026-09-23 08:21 PHT — metadata, safety, and 0M longevity hardening

- Metadata protection is role/path-specific rather than a binary promise. Stable canonical identities remain above provider routing; delivery uses scoped opaque capabilities and independently randomized outer envelopes where applicable. See [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md).
- Abuse and moderation authority remains scoped to users, Rooms/Spaces, providers, catalogs, and voluntarily selected safety advisers. Signed safety advisories are portable advice, never universal protocol authority. See [`DECENTRALIZED_ABUSE_MODERATION.md`](../architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md).
- Published protocol generations must remain self-contained and mirrorable through Protocol Continuity Bundles. Registry meanings are immutable once assigned, forks/generations are explicit, and no dominant client, host, directory, extension, founder, or steward becomes protocol authority. See [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md).
- These decisions resolve A-013, A-014, and A-015 at the design level. Engineering remains unauthorized until the blocking verification gates pass.
