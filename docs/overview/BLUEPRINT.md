---
title: Open Messaging Blueprint
status: exploration-pre-design
canonical_format: markdown
updated: 2026-08-18
audience: humans-and-agents
---

> **Canonical source:** this Markdown repository supersedes the DOCX working draft for ongoing design work. JSON/YAML files are machine interfaces and structured records, not replacements for the narrative blueprint.

## Current direction — August 18, 2026

- **Working name:** **OM** — OpenMessenger / OwnMessenger / OurMessenger.
- **UX:** Textra-like speed and focus + Telegram-like fluidity and ease of use. Conversation-first, fast startup, minimal visual clutter, no infrastructure jargon in normal flows.
- **Primary surfaces:** Android, iOS, and web. Web is treated as a linked cryptographic device with independent revocation; native desktop remains optional/later.
- **Architecture:** every infrastructure role is a replaceable Lego block connected by narrow versioned capability contracts.
- **Identity:** cryptographic account identity is the root. `@username`, QR/invite, phone-number discovery, directories, relays, and storage are replaceable references/services.
- **Anti-spam:** public discovery is not a public unlimited-write mailbox. Accepted relationships receive private rotatable delivery capabilities.
- **Provider independence:** no provider is part of the protocol. Cloudflare, S3-compatible stores, Drive-class providers, WebDAV, NAS/local storage, community relays, and self-hosted servers are implementations where their capabilities fit.
- **Storage:** delivery queue, hot sync, hot media, cold archive, and recovery are distinct storage classes. Hot and cold policies can use different providers; StoragePool can mirror encrypted data across providers.
- **Bots/plugins:** bots are explicit permission-scoped principals; provider plugins receive only role-specific secrets/capabilities.
- **Transport openness:** OM may bridge/connect through authorized adapters, but no bridge may silently claim OM-native E2EE properties.
- **Open source extensibility:** third parties should be able to add providers/transports or implement compatible components without modifying the conversation model, identity model, or cryptographic core.
- **Lean organization:** few services, boring interfaces, strong boundaries, aggressive testing, robust delivery. Avoid microservice sprawl.
- **Normal-user rule:** convenience by default; sovereignty without forcing users to understand federation, object stores, relay topology, or keys.
- **Product edge:** openness and portability as defaults across the whole system—not merely an open-source client or self-hosting option.

## Architecture phase status

The project is now explicitly in **Phase 0 — architecture and threat model**. Work should proceed through dependency gates rather than implementing all subsystems at once. The immediate next gate is **Identity, Devices, Aliases, and Recovery**. See `ARCHITECTURE_PHASE_PLAN.md` and `SYSTEM_ARCHITECTURE.md`.

## Product test

A proposed feature should improve at least one of **convenience, ownership, reliability, or freedom to leave** without forcing ordinary users to learn infrastructure.

# Original working blueprint (converted from DOCX)

OPEN MESSAGING
BLUEPRINT

A lean, open-source, secure, multi-platform messenger that combines the best ideas from Textra, Google Messages, and Signal - without depending on a single operator.

| PRODUCT<br>Lean messaging UX | SECURITY<br>Established E2EE primitives | NETWORK<br>Open relays + optional P2P |
| --- | --- | --- |

Purpose

This document consolidates the discussion into a working product and technical blueprint. It is intentionally opinionated: it identifies the architecture that appears most viable, the constraints imposed by Android/iOS/RCS, and a practical path from prototype to a trustworthy public release.

Status: Exploration / pre-design. Platform APIs, store policies, carrier/RCS access, and cryptographic library licensing must be re-verified before implementation.

## 1. Executive decision summary

| Decision | Working position |
| --- | --- |
| Buildable | A high-quality Android + iPhone encrypted messenger with independent desktop clients is absolutely feasible for a small team augmented by coding agents. |
| Android advantage | Android can support the fullest vision: secure messaging plus default SMS/MMS behavior, with future RCS transport integration where APIs/provider access permit. |
| iPhone constraint | The secure-messenger experience can be first-class on iPhone, but replacing Apple Messages for normal carrier messaging remains region/policy constrained. |
| RCS is an adapter, not the foundation | Do not architect the product around Google/Jibe RCS access. Treat RCS as a transport plugin that can be enabled when authorized access exists. |
| No new cryptography | Use mature, reviewed protocols/primitives. Keep cryptographic implementation behind a small interface so it can be audited or replaced. |
| Decentralize the dependency, not the UX | Prefer open, interchangeable store-and-forward relays plus optional direct P2P. The goal is that no single operator is indispensable, not that every message must be peer-to-peer. |
| Keep identity independent of phone numbers | Use a cryptographic account identity plus usernames/QR invites; phone numbers can be optional discovery metadata. This reduces cost, SIM-swap dependence, and telecom verification overhead. |
| Ship native clients | Use native Android (Kotlin/Compose) and iOS (Swift/SwiftUI) shells around a shared Rust protocol/security core. Add desktop as a first-class cryptographic device rather than a phone proxy. |

## 2. Product vision

> **Design note:** Working product thesis: Textra-like focus and polish + Signal-like security goals + Google-Messages-like interoperability/multi-device convenience, implemented as an open client, open protocol, and open self-hostable relay network.

### What the product should feel like

- Fast, uncluttered, conversation-first UI.
- Normal people never need to understand federation, relays, keys, or transport routing.
- Clear indication of how a message will be sent: Secure / RCS / SMS-MMS.
- Excellent scheduling, search, media handling, replies, reactions, groups, muting/blocking, and accessibility.
- Desktop behaves like a real device, not a fragile mirror of the phone.
### What the product should not become

- A social network, content feed, payments app, bot platform, marketplace, or ad product.
- A protocol experiment that invents novel cryptography.
- A service that requires one company-operated server to survive forever.
- A giant microservice/Kubernetes stack before scale requires it.
## 3. Platform feasibility: Android, iPhone, desktop

| Capability | Android | iPhone | Desktop |
| --- | --- | --- | --- |
| Native E2EE messaging | Yes | Yes | Yes |
| Groups / media / reactions | Yes | Yes | Yes |
| Independent linked-device identity | Yes | Yes | Yes |
| Default SMS/MMS client | Yes | Limited by Apple policy/region | N/A |
| Consumer RCS transport | Provider/API dependent | Provider/API + Apple policy dependent | Via account transport, not carrier stack |
| Read/replace iMessage history | No | No | No |
| Push notifications | FCM / system | APNs | Native background service |

> **Design note:** Key constraint: The product can be cross-platform from day one, but carrier-message replacement cannot be assumed to be feature-identical across Android and iPhone. The shared secure network should therefore be the primary product; SMS/RCS are compatibility transports where the platform permits them.

## 4. Recommended architecture

### 4.1 Client structure

Android: Kotlin + Jetpack Compose
iPhone:  Swift + SwiftUI
Desktop: native or high-quality cross-platform shell
                |
                v
         Shared Rust core
   crypto / sessions / identity
 groups / sync / serialization
 attachment metadata / key state

Why this split: platform UI, lifecycle, notifications, permissions, contacts, and SMS integration remain native, while security-sensitive protocol behavior is implemented once and exercised identically across clients.

### 4.2 Unified message model and transport adapters

Conversation UI
      |
Unified Message Model
      |
      +--> SecureTransport  --> open encrypted relay network
      +--> SmsMmsTransport  --> Android carrier network
      +--> RcsTransport     --> future/provider-specific adapter
      +--> DirectP2P        --> opportunistic device-to-device path

Design rule: the conversation layer must not care which transport carried a message. Transport capability discovery, fallback, security indicators, and delivery semantics belong in a routing layer.

## 5. Identity, accounts, and discovery

- Primary identity: a cryptographic account identifier, not a phone number.
- Human-friendly discovery: username plus QR code / invite link.
- Optional phone-number mapping: opt-in discoverability only; never the root identity.
- Each linked device receives its own keys and device identifier.
- Device addition/removal must be visible to users and reflected in session/key state.
- Account recovery should avoid a server-held master decryption key; recovery design needs a dedicated threat model.
> **Design note:** Cost/security benefit: avoiding mandatory SMS verification removes a major telecom dependency and eliminates a large cost center that Signal has publicly identified in its own infrastructure spending.

## 6. Security model

### 6.1 Cryptography

- Use established, peer-reviewed protocol ideas and maintained cryptographic primitives; do not design a new ratchet or handshake casually.
- Keep crypto behind a narrow internal interface so library/protocol implementations can be independently reviewed, fuzzed, or replaced.
- Consider Signal protocol specifications and libsignal as references, while evaluating licensing, API stability, and support constraints before adoption.
- Implement explicit key verification / safety-number or QR verification for high-assurance users.
- Use authenticated encryption for all message bodies and attachments; servers should see ciphertext and minimal routing metadata only.
### 6.2 Security engineering requirements

- Threat model before protocol freeze.
- Independent security review before marketing the product as suitable for sensitive communications.
- Fuzzing for parsers, serialization, attachment handling, and protocol state machines.
- Reproducible/signable builds where practical; strict dependency pinning and supply-chain review.
- Separate release signing, infrastructure credentials, and security-sensitive operational access.
- Minimal analytics; never collect message content, contact graph, or behavioral telemetry by default.
## 7. Multi-device model

Desktop should be a first-class encrypted endpoint. The design should avoid the failure mode where the desktop constantly asks whether the phone is online. Each linked device has its own key material and can independently receive encrypted envelopes.

Account identity
                  |
       +----------+----------+
       |          |          |
   Android      iPhone     Desktop
   key A        key B       key C
       \          |          /
        +--- encrypted relay ---+

- Offline delivery is queued per target device.
- Adding a device changes the recipient device set for future sends.
- Revoked devices must stop receiving new messages immediately after revocation propagates.
- History sync is a separate encrypted mechanism; avoid requiring the server to store long-term plaintext or decryptable archives.
- Browser support can be added, but a signed native desktop client is easier to secure and reason about than code re-downloaded on every web visit.
## 8. Infrastructure required

### 8.1 Minimum production services

| Service | Responsibility | Data sensitivity |
| --- | --- | --- |
| Relay / offline queue | Receive encrypted envelopes, hold briefly for offline devices, deliver, then delete. | Ciphertext + routing metadata |
| Identity/key directory | Publish account/device public keys and device state. | Public key material + minimal account metadata |
| Attachment blob store | Hold client-encrypted media/files; use random object identifiers and expiry. | Ciphertext blobs |
| Push gateway | Trigger APNs/FCM wakeups without embedding message content. | Opaque notification metadata |
| Abuse/rate limiting | Protect registration, relay, attachment, and invitation endpoints. | Operational metadata |
| Observability | Health, latency, errors, capacity; privacy-preserving metrics only. | Aggregated operational data |
| TURN/STUN (later) | NAT traversal and privacy relay for voice/video calls. | Real-time encrypted media traffic / IP metadata |

### 8.2 Lean starting stack

- 2-3 stateless relay instances behind a load balancer.
- PostgreSQL (or similarly boring relational store) for device/account metadata and durable operational state.
- A queue/cache only where measured need justifies it; avoid introducing Kafka-class infrastructure prematurely.
- S3-compatible object storage for encrypted attachments with short retention/expiry.
- APNs + FCM integration for mobile wakeups.
- Basic monitoring, alerting, backups, CI/CD, and secret management.
> **Design note:** Infrastructure principle: a relay should be boring enough that a community member can run one on an ordinary VPS. Self-hosting should target a Docker Compose-level experience, not a cloud platform team.

## 9. Decentralization strategy

Recommended goal: not "every message is fully peer-to-peer," but "no indispensable centralized party." Servers are allowed because reliable offline delivery needs them; any particular server operator should be replaceable.

| Model | Strength | Assessment |
| --- | --- | --- |
| Central encrypted relay | Simplest UX and operations; one operator remains a dependency. | Good bootstrap mode, insufficient as the final resilience story. |
| Matrix-style federation | Independent homeservers with server-to-server federation. | Powerful but stateful and operationally complex for a deliberately lean messenger. |
| SimpleX-inspired dumb relays | No account homeserver required; clients choose interchangeable store-and-forward relays. | Best fit for openness, self-hosting, and minimal server knowledge. |
| Pure P2P / Briar-like | Direct connectivity over local links/Tor; no central delivery service. | Excellent censorship resistance, but poor always-available delivery when both endpoints are not simultaneously reachable. |
| Hybrid relays + opportunistic P2P | Default relays for reliability, optional direct path when available. | Recommended architecture. |

> **Design note:** Continuity test: if the original organization disappears, can users point their clients at community relays and keep the network alive? The intended answer is yes.

## 10. Attachments and calls

### 10.1 Attachments

- Encrypt media locally before upload.
- Upload a random-looking ciphertext blob to object storage.
- Send the blob locator, decryption key/material, hash, size, and MIME metadata inside the E2EE message envelope.
- Use expiry policies to control storage cost and privacy exposure.
- Long-term backups, if offered, should be separately encrypted and optional.
### 10.2 Voice/video (later)

- Prefer direct WebRTC when both devices can connect.
- Use STUN for NAT discovery and TURN when direct connectivity fails or a user chooses an IP-hiding privacy mode.
- Treat calls as a separate scale/cost problem: text traffic is cheap; relayed real-time media can become one of the largest bandwidth expenses.
- Community-operated TURN nodes may be possible later, but abuse controls and reliability requirements are substantially higher than text relays.
## 11. Cost model and the Signal comparison

The claim that Signal spends $50 million per month is not supported by Signal's published discussion. The figure referenced in our discussion was approximately $50 million per year of expected operating need by 2025, with Signal separately disclosing substantial infrastructure and staffing costs. One notable cost category was phone-number/SMS verification.

> **Design note:** Implication for this project: a new messenger at 10,000 or even 100,000 mostly-text users does not need Signal-scale infrastructure. Cost is driven more by verification, attachment retention, abuse, and especially relayed calls than by encrypted text envelopes.

| Cost area | Relative pressure | Notes |
| --- | --- | --- |
| Encrypted text relays | Low | Small envelopes, short retention, easy horizontal scaling. |
| Attachment storage/bandwidth | Medium | Depends heavily on media size, expiry, and download patterns. |
| SMS verification | Potentially high | Avoidable if phone number is not mandatory identity. |
| Push notifications | Low to medium | APNs/FCM operational integration rather than bulk message storage. |
| TURN / relayed calls | High at scale | Real-time bandwidth grows rapidly and can dominate infra spend. |
| Abuse/spam defense | Medium and labor-heavy | Often becomes more difficult than raw serving capacity. |
| Security engineering/audits | Essential investment | Not optional for a product claiming high security. |

## 12. Development model with AI agents

AI coding agents materially improve the feasibility of a small-team implementation, especially for platform glue, tests, migrations, UI repetition, build systems, protocol conformance harnesses, fuzz scaffolding, documentation, and issue reproduction. They should not be treated as a substitute for cryptographic expertise or independent security review.

| Role | Primary scope |
| --- | --- |
| Android agent | Compose UI, SMS/MMS integration, permissions, notifications, instrumentation tests. |
| iOS agent | SwiftUI, APNs, Keychain, lifecycle, share/notification extensions, App Store-specific plumbing. |
| Core/protocol agent | Rust state machines, serialization, test vectors, cross-platform bindings. |
| Backend agent | Relay APIs, key directory, storage, push gateway, rate limiting, deployment. |
| Test/fuzz agent | Interop matrices, property tests, fuzz corpora, regression reproduction. |
| Security-review agent | Static review, dependency checks, threat-model traceability; advisory only, not final authority. |

## 13. Suggested phased roadmap

| Phase | Scope | Exit criterion |
| --- | --- | --- |
| Phase 0 - Architecture & threat model | Freeze product principles, threat model, identity model, transport interface, relay protocol, licensing approach, and cross-platform test vectors. | No production users. |
| Phase 1 - Secure messaging prototype | Android + iOS + one desktop client; 1:1 E2EE text, device linking, offline relay, push, basic attachments. | Prove protocol and multi-device design. |
| Phase 2 - Daily-driver UX | Groups, replies, reactions, search, scheduling, media, blocking/muting, backup/recovery design, accessibility, localization. | Make it pleasant enough to replace mainstream secure messengers for early adopters. |
| Phase 3 - Android carrier integration | Default SMS/MMS mode, unified inbox/routing, fallback behavior, contact integration. | Deliver the Textra-like compatibility layer. |
| Phase 4 - Open relay network | Document server protocol, Docker Compose deployment, relay selection/failover, community operators, portability. | Remove the original operator as a required dependency. |
| Phase 5 - RCS adapters where possible | Integrate authorized carrier/provider RCS interfaces without changing the conversation model. | Treat availability as capability discovery, not a universal assumption. |
| Phase 6 - Calls | WebRTC, STUN/TURN, privacy relay mode, call safety and abuse controls. | Add only once core messaging is stable. |

## 14. MVP feature boundary

### Must have

- 1:1 encrypted text messaging.
- Android and iOS clients.
- Independent desktop linked device.
- Username/QR identity and device management.
- Offline encrypted relay delivery.
- Photos/files with client-side encryption.
- Replies, reactions, read/delivery state, typing indicator.
- Basic groups.
- Search, mute, block, notifications.
- Android SMS/MMS after the secure core is stable.
### Explicitly defer

- Global RCS support promises.
- Voice/video calls.
- Stories/status feeds.
- Bots/channels/business accounts.
- Payments, ads, monetized identity, social discovery feeds.
- Complex federation/state replication.
- Novel cryptographic constructions.
## 15. Principal risks and blockers

| Risk | Severity | Mitigation / design response |
| --- | --- | --- |
| RCS ecosystem access | High | Android/iOS APIs alone do not guarantee access to Google/Jibe or carrier consumer RCS infrastructure. Keep it optional. |
| iOS carrier-messaging policy | High | Apple controls whether third-party default carrier messaging is available in a region. Secure messaging remains unaffected. |
| Security credibility | High | A messenger can work correctly and still contain dangerous protocol, storage, or lifecycle flaws. Budget for expert review. |
| Spam/abuse | High at scale | Open registration and relay networks attract abuse. Rate limits, invitation controls, reputation, and operator tools need careful privacy tradeoffs. |
| Multi-device state complexity | Medium-high | Device add/remove, missed messages, stale sessions, history sync, and group state create difficult edge cases. |
| Open-relay incentives | Medium | Community relays need operational documentation, abuse controls, and perhaps sustainable funding without becoming identity gatekeepers. |
| App-store policy changes | Medium | Store entitlements and background-execution rules can change; keep platform adapters isolated. |
| Scope creep | High | The product loses its advantage if it becomes a general social platform. Preserve a strict lean feature philosophy. |

## 16. Open design questions

1. Which cryptographic protocol/library stack should be adopted, and what licensing model best fits the open-source project?
1. How should account recovery work without weakening end-to-end security?
1. Should usernames be globally unique, namespace-scoped, or resolved through portable identity documents?
1. How should clients select, rank, and fail over among relays without leaking the social graph?
1. How much metadata should a relay be able to observe, and which metadata-hiding techniques are worth the complexity?
1. How should group membership/state be represented and synchronized across independent devices?
1. How are spam, unsolicited contact, and relay abuse handled without mandatory phone numbers?
1. What is the minimal Android SMS/MMS integration required for a credible Textra replacement?
1. Which RCS providers/carriers expose usable third-party interfaces, and under what commercial/technical terms?
1. Will the desktop client be native, Rust+Tauri-style, or another approach, and what is the security/update model?
1. What level of anonymous diagnostics is acceptable for reliability while preserving the privacy promise?
## 17. Source and verification notes

These references were discussed during the planning conversation. They are included as starting points, not as a substitute for an implementation-time legal/API review. Platform availability, entitlements, API level requirements, pricing, and policies can change.

- Signal - "Signal is expensive": https://signal.org/blog/signal-is-expensive/
- Signal protocol specifications: https://signal.org/docs/
- libsignal repository: https://github.com/signalapp/libsignal
- Android default handlers: https://developer.android.com/guide/topics/permissions/default-handlers
- Android AlternativeMessageTransportService: https://developer.android.com/reference/kotlin/android/service/messaging/AlternativeMessageTransportService
- Google RCS for Business developer documentation: https://developers.google.com/business-communications/rcs-business-messaging
- Apple support - default apps / messaging (availability should be rechecked): https://support.apple.com/en-us/121430
- Apple background push documentation: https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app
- Matrix specification: https://spec.matrix.org/
- SimpleX messaging architecture: https://simplex.chat/messaging/
- SimpleX server documentation: https://simplex.chat/docs/server.html
- Briar project: https://briarproject.org/
## 18. Working north star

> **Design note:** Build a messenger whose convenience does not depend on surrendering control of the network. Official infrastructure should make the app effortless for normal users, while the open protocol, portable cryptographic identity, independent devices, and self-hostable relays ensure that the community can keep the network alive without the original operator.

End of working blueprint
