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
