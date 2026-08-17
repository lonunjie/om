# AGENTS.md

## Mission

Build **OM (OpenMessenger / OwnMessenger / OurMessenger)**: a fast, simple, reliable, open-source messenger whose users can replace infrastructure providers without replacing identity, contacts, or canonical conversation state.

## Current phase

**Phase 0: architecture and threat model.** Do not prematurely implement product-scale features when a prerequisite architecture gate in `ARCHITECTURE_PHASE_PLAN.md` is unresolved.

## Non-negotiable rules

1. **Never hardcode a cloud/provider into the core protocol.** Provider behavior belongs behind capability interfaces/adapters.
2. **No novel cryptography.** Use established reviewed protocols/primitives and keep crypto behind a narrow auditable interface.
3. **Conversation model is transport-agnostic.** Secure relay, SMS/MMS, RCS, and optional bridges/P2P are routing choices, not separate canonical chat models.
4. **Identity is independent of phone number, username, and provider.** Human-readable aliases are discovery/introduction layers.
5. **Do not use `@username` as a permanent public write mailbox.** Contact introduction and accepted private delivery capabilities are separate concepts.
6. **Android + iOS + web are first-class surfaces.** Web is modeled as a revocable linked cryptographic device, not a plaintext/password mirror.
7. **Normal users should not need infrastructure vocabulary.** Prefer policies such as “Stay within free tier” over raw object-store configuration.
8. **Provider migration is a supported operation, not disaster recovery.** Identity and conversations survive relay/storage/directory changes.
9. **Keep infrastructure lean.** Prefer small, boring, independently testable components over speculative service sprawl.
10. **Delivery correctness beats feature count.** Specify retries, idempotency, acknowledgements, deduplication, ordering, offline queues, revocation, and partial failure.
11. **Open-source clients are adversarially modifiable.** Abuse/security controls requiring enforcement belong at recipient/relay/server trust boundaries, not only in UI/client code.
12. **No silent security downgrade.** Every transport/bridge/bot/plugin declares capabilities and trust boundaries.
13. **Portability must have tests.** A migration/restore promise without a conformance test is not an accepted architecture property.
14. **Do not claim security or current platform/provider support without verification.** Policies, quotas, APIs, and licensing are volatile.

## UX direction

- Textra-like speed, focus, and low friction.
- Telegram-like responsiveness, navigation, media handling, and ease of use.
- Conversation-first UI; infrastructure belongs in settings/onboarding only when needed.
- Security/transport state must be understandable without turning the interface into diagnostics.

## Core capability boundaries

At minimum:

- `DirectoryProvider`
- `RelayProvider`
- `PushProvider`
- `BlobStore`
- `BackupStore`
- `RecoveryStore`
- `StoragePool`
- `TransportAdapter`
- bot/extension permission capabilities

The core asks for capabilities; it does not branch on vendor names.

## Storage rules

Separate **delivery queue**, **hot sync**, **hot media**, **cold archive**, and **recovery** concerns. Users may set independent budgets by media class and independent storage/backup destinations.

Initial multi-provider redundancy should prefer full-copy mirroring because it is easy to verify and restore. Keep erasure coding possible at the abstraction layer but do not implement it until manifest/repair/restore behavior is proven.

Provider plugins expose quota/cost characteristics dynamically or via versioned provider profiles. Never hardcode volatile free-tier numbers into protocol behavior.

## Architecture change discipline

When changing architecture:

- Identify which gate in `ARCHITECTURE_PHASE_PLAN.md` owns the question.
- Add/update an ADR under `decisions/`.
- Update `machine/decisions.jsonl`, `machine/requirements.jsonl`, and when applicable `machine/open_questions.jsonl`.
- Update a schema only when the wire/config contract changes.
- Add threat-model consequences and negative test cases.
- Add or update a conformance/migration test expectation.
- Keep `BLUEPRINT.md` coherent for humans; agents must not reconstruct the product from JSONL alone.


## Frozen Phase-0 directions

Unless an ADR explicitly changes them:

- identity is passwordless-by-default and device-first;
- AccountID is independent of aliases/providers;
- PersonRecord is local, not globally authoritative;
- phone/email/usernames are recyclable/replaceable aliases;
- no global trust score or transitive Web of Trust;
- private group membership is AccountID-level in UX and DeviceIdentity-level cryptographically;
- design private group keying around MLS rather than inventing new group crypto;
- group roles/admin policy are application-layer;
- relays/storage/directories are providers, never identity roots;
- bridges/bots are explicit trust boundaries;
- default group history is since-join; history sharing is a separate encrypted mechanism.

- web-first does not mean web-only; no protocol decision may depend on browser-only behavior;
- the official client is a reference client, not the protocol;
- third-party clients should interoperate through public specs/conformance, not private APIs;
- unknown senders use MessageRequests/IntroductionEndpoints;
- bots/business agents use scoped delegated capabilities and never require organization root keys.


## Gate-B/event rules agents must preserve

- Do not turn every UI action into a network event.
- Durable baseline event kinds stay small: create/edit/delete/reaction/receipt.
- Identity/group/device authority comes from control protocols, not chat-message payloads.
- Typing/presence is ephemeral and disposable.
- Draft/mute/pin/archive/search index are local unless a private sync spec explicitly covers them.
- Attachments are out-of-band encrypted blobs.
- Baseline event envelope is capped at 64 KiB.
- Provider/transport details stay out of canonical semantic identity.
- PWA code is a reference client; browser behavior must not become wire-protocol truth.
- Prefer headless conformance tests over UI-coupled tests.
