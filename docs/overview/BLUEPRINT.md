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

Use an asynchronous reviewed handshake and continuously evolving ratchet with PQ/hybrid properties comparable to current PQXDH + Triple Ratchet class designs. Exact library/primitives are selected by profile ADR and interoperability tests.

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

Typing/presence/call negotiation is ephemeral. Local mute/pin/drafts/search remain local/private unless a separate account sync protocol covers them.

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

Default OM service can hide all of this from end users. Advanced users/operators may bring their own provider.

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

OM aims to have no single censorship switch, not to claim impossible uncensorability. Encrypted OM semantics sit above pluggable network transports and provider sets. Directory, introduction, relay, media, blob and network-transport providers can vary independently. LAN/direct operation, alternate relays/providers, and future proxy/pluggable-transport adapters can preserve reachability in some blocking environments without changing identity or conversation semantics.

## 19. Governance

Public protocol, conformance tests, test vectors, reproducible/release-transparent builds where practical, independent clients, and replaceable registries reduce the chance that OM's original maintainers become the universal controller.

## 20. Initial implementation boundary

First useful OM should prove:

- AccountID/device creation/link/revoke;
- QR/`om:` invite -> ContactDescriptor -> revocable IntroGate/MessageRequest;
- 1:1 E2EE OM-native delivery through replaceable multi-relay RelaySets/failover;
- canonical event convergence;
- encrypted attachments through replaceable BlobStore;
- basic multi-device sync;
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
