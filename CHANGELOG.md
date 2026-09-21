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
