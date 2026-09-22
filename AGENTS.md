# AGENTS.md — OM implementation/research rules

## Mission

Build OM as a fast, simple, reliable secure messenger whose identity and canonical conversations survive provider and cryptographic replacement.

## Absolute rules

1. Provider names never become wire-protocol semantics.
2. AccountID is never used as a relay/media routing address.
3. No novel cryptographic primitive/protocol unless an explicit research ADR says otherwise; prefer reviewed standards/designs.
4. Crypto agility means approved **CryptoProfiles**, not arbitrary primitive negotiation.
5. Crypto upgrades are automatic and normally invisible; never add a user-facing cipher selection flow for ordinary users.
6. Never silently downgrade an established relationship/group/transport security level.
7. Conversation semantics are transport-agnostic; SMS/MMS/RCS are future adapters.
8. Media E2EE must remain independent of direct/TURN/SFU route.
9. Use infrastructure only when it adds value; prefer LAN/P2P for calls when privacy policy permits.
10. Unknown senders are constrained before expensive work and before direct calls.
11. Open-source clients are adversarially modifiable; enforce abuse/resource limits at recipient/provider boundaries.
12. No global trust score is required for protocol validity.
13. Storage classes remain separate; do not turn relay DB or R2 into universal state.
14. Provider migration and restore require conformance tests.
15. Public specs/test vectors define compatibility, not official-client private behavior.
16. Normal users must not need infrastructure/crypto vocabulary.
17. Advanced provider choices live behind policy interfaces; default is automatic.
18. Cloudflare is a reference implementation, never a dependency.
19. Store/carrier approval work must not block OM-native product delivery.
20. Every architecture addition must answer: **Where did authority go?**

## Core interfaces

At minimum:

- `CryptoProvider` / `CryptoProfileRegistry`
- `DirectoryProvider`
- `RelayProvider`
- `PushProvider`
- `BlobStore`
- `BackupStore`
- `RecoveryStore`
- `StoragePool`
- `MediaProvider`
- `RoutePolicy`
- `TransportAdapter`
- scoped bot/service capabilities

## User-facing design test

For any implementation proposal ask:

> Could a user who does not know how to configure Wi-Fi still use this correctly and securely?

If no, the complexity probably belongs in automatic policy/provider code or advanced settings.

## Change discipline

Architecture change requires:

- update relevant canonical doc;
- ADR under `decisions/`;
- machine decision/requirement/open-question records;
- threat-model consequences;
- negative/conformance tests;
- migration/backward-compatibility story when wire/storage state changes.

## External facts

Cloud/provider quotas, app-store policies, drafts, APIs, library support, and pricing are volatile. Verify current official sources before implementation or budgeting. Do not encode current free-tier numbers into protocol behavior.

## Discovery/resilience checkpoint

Treat `DISCOVERY_FABRIC.md`, `INVITES_INTROGATE_AND_SPAM.md`, `MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`, `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`, and `CENSORSHIP_AND_NETWORK_RESILIENCE.md` as canonical. Do not introduce a protocol-global username root, publish stable relationship mailboxes in directories, reuse a capability across multiple relays, or broadcast stable identity on LAN. New network/circumvention transports must remain below canonical OM semantics.

## 21:32 checkpoint implementation rules

- Do not implement account authority using chat timestamps, CRDT merge, or last-write-wins.
- Do not let one existing controller promote arbitrary new control devices without parent-state constitutional authorization.
- Do not make backup content authoritative for device/recovery/security state.
- Do not require R2/payment setup for the v1 zero-payment personal Cloudflare bootstrap.
- Do not upload/hash the OS address book for discovery.
- Do not place sender/message plaintext in APNs/FCM payloads.
- Do not interpret Recommended/Hardened/Maximum Privacy as weaker/stronger cipher selections.
- Do not ship a public binary whose trust reduces to one mutable CI workflow/one online update key if the documented trust-root gate is incomplete.

## 2026-09-22 operatorless constraints

Agents must preserve these invariants:

- do not add a mandatory OM-operated control plane merely for convenience;
- do not make a reference client/distribution technically privileged;
- treat provisioning, push, reporting and managed hosting as replaceable provider/distribution capabilities;
- preserve Creator-Disappearance Gate;
- do not infer “no legal obligations” from decentralization; flag jurisdiction-specific operator-role questions instead.

## 2026-09-22 shared UI constraints

- Official PWA/iOS/Android/desktop product screens belong in shared `om-ui`; do not create independent product/security semantics in platform shells.
- Platform adapters expose capabilities/system surfaces, not messaging/security business logic.
- UI components send typed intents to Core and render returned state; never mutate cryptographic/control/provider state directly.
- Motion must be semantic, interruptible where interactive, and reduced-motion compatible; never delay core actions for animation.
- Themes are declarative data-only; no arbitrary CSS/HTML/script/WASM/native code or remote tracking assets.
- Themes cannot suppress/spoof critical security UI, focus, accessibility or external-link destination semantics.
- Accessibility regressions in shared primitives are release-blocking.
- Default UI prioritizes ergonomics/functionality and low visual noise; advanced capabilities use progressive disclosure.
