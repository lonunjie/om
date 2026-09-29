# OM Product and UX Principles

## Prime rule

> **Bulletproof underneath; boringly easy on top.**

Assume a meaningful portion of users do not know what Wi‑Fi, encryption, NAT, a relay, an object store, or a cryptographic key is. That is not a user deficiency; it is a product constraint.

## What ordinary users should never be required to do

- select a cipher or manually approve a crypto migration;
- understand key rotation, MLS epochs, KEMs, signatures, or ratchets;
- choose STUN/TURN/SFU;
- manually configure Cloudflare/AWS resources, API tokens, Worker bindings, databases, or storage products;
- configure R2 buckets, S3 credentials, WebDAV, or relay URLs;
- understand P2P versus relayed calling;
- manually rebalance infrastructure quotas;
- understand device-key topology to add a phone/tablet/computer;
- know whether their contact changed relay or storage provider.

## Invisible automatic operations

The client should automatically:

1. choose the strongest mutually approved CryptoProfile;
2. rotate/session-rekey in the background;
3. migrate sessions/groups when profiles are superseded;
4. preserve a downgrade floor so old/attacker-injected profiles are rejected;
5. rotate private delivery capabilities;
6. choose relay/storage/media providers according to policy and health;
7. avoid or fail over providers near configured cost/quota boundaries;
8. choose LAN/P2P/TURN/SFU routing automatically;
9. retry/resume uploads, message delivery, and calls after network changes;
10. repair encrypted storage mirrors when possible;
11. present recovery and device-linking in human terms.

## Default UX

The default app should mainly expose:

- people and groups;
- messages/media;
- voice/video call buttons;
- search;
- privacy/safety controls in understandable language;
- device management;
- account recovery.

Infrastructure belongs behind `Settings → Advanced` and should usually be represented as policies:

- **Automatic**
- **Prefer free / no surprise charges**
- **Protect my network address**
- **Use my own providers**

rather than vendor jargon.

## Security state

Security warnings should be exceptional and actionable. The UI should not create “encryption fatigue.” Background upgrades are silent when they preserve or improve guarantees. User attention is reserved for meaningful identity discontinuity, suspicious device changes, an explicit transport downgrade (e.g. future SMS), or an action that exposes new data.

## Product invariants

- A provider migration should not look like creating a new account.
- A crypto migration should not look like creating a new account.
- A relay failure should not destroy conversation history.
- A storage failure should not destroy identity.
- A media provider failure should not change who is in the call.
- A network-path change should not change whether call content is E2EE.
- Advanced users may override defaults, but advanced choices may not weaken other users silently.

## Discovery invisibility rule

Resolver namespaces, ContactDescriptor validation, invite revocation epochs, IntroductionTickets, RelaySets, anti-abuse challenges, provider failover and censorship transports are infrastructure details. Ordinary users should normally see only `Search`, `Scan QR`, `Connect`, `Block`, `Report`, and `Revoke link`.

## Recovery and protection UX

Recovery is on by default and needs no setup: it is saved with the user's passkeys, or with a recovery key when passkeys are unavailable. Taking over an account through recovery requires waiting out a delay that any of the user's devices can stop (ADR-0111). Users can add guardians or a spare device, or explicitly choose a disposable identity with no recovery.

The product explains consequences, not primitives:

- **No recovery** — lose all devices, lose this identity.
- **Recovery code** — protect this like your digital life depends on it.
- **Trusted people/places** — require several shares/approvals.

## Protection presets

Recommended settings are the default; users are not forced through a threat-model wizard. Advanced users can choose Recommended, Hardened, Maximum Privacy, or customize component policies. These presets never select weaker E2EE ciphers.

## ~~Cloudflare onboarding exception~~ Onboarding without an infrastructure account

> **2026-09-24 (ADR-0102, ADR-0130):** there is no Cloudflare onboarding exception any more. A new user joins through
> an invite on a T0 sponsored starter mailbox or picks a T1 hosted Home, and needs no account anywhere. Owning a
> cloud account (T2) is an optional sovereignty choice with automated, verified-release provisioning; understanding
> Workers/DO/R2 remains unnecessary. See [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](../architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md).

## 2026-09-22 operatorless UX principle

The user should experience a normal messenger even when the infrastructure is individually owned.

Self-owned infrastructure must not mean “become a sysadmin.” The reference onboarding may use provider-native deployment templates and automatic health checks, while all infrastructure detail remains hidden unless the user opens Advanced settings.

PWA-first is a distribution choice, not a visible compromise in ordinary messaging UX. Native clients later improve OS integration without changing the user's identity or network.

## 2026-09-22 13:42 shared UI/OwnMessenger principles

The reference client family uses one shared `om-ui` above `om-core`. The intended experience is nearly identical across PWA, iOS, Android and desktop; OS-owned system surfaces remain native.

Default design priorities, in order:

1. ergonomics and functionality;
2. immediate legibility and low interaction cost;
3. perceived speed and continuity;
4. accessibility;
5. calm, durable aesthetics;
6. personalization.

The UI should feel fluid through immediate local feedback, semantic/interruptible motion, stable layouts and optimistic staging where safe. Movement explains state and spatial continuity; it is never decoration that blocks work.

OM/OwnMessenger supports deep user themes and fonts, but themes are untrusted declarative presentation data. They cannot execute code, load arbitrary remote tracking assets, alter cryptographic/provider policy, hide security-critical state, spoof system UI or weaken accessibility overrides.

Security UI remains exceptional and actionable. Routine E2EE/PQ state is not advertised with persistent padlocks or cyber-security decoration.

## OwnMessenger identity personalization

Multiple identities are a legitimate V1 product need. The reference client lets users switch configured IdentityProfiles instantly and optionally distinguish them with curated themes, while preserving the strict isolation/linkage rules in [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md). Do not infer or police the user's motive for creating identities; enforce abuse at relationship/IntroGate/provider boundaries rather than through hidden cross-profile fingerprinting (ADR-0148).

All themes offer Auto light/dark behavior. The default remains ergonomics/functionality first: immediate clarity, calm hierarchy, fast local response, then aesthetics/personalization.

## 2026-09-23 traction baseline

OM is not successful merely because the protocol is elegant. The first usable product must feel like a current messenger: expected reply/reaction/media/voice/call/group/search/mute/block and expressive-media ergonomics need an explicit baseline before engineering. That baseline is now frozen in [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md); `A-010` is resolved. Stickers and GIF-like looping animation remain product-baseline expectations; A-001/A-002 are now resolved by the bounded WebP expressive profile and portable inert ContentPack architecture in [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md).

## 2026-09-29 R15 reconciled interaction model

ADR-0159 integrates the standalone UI/UX design track into the canonical architecture without changing Core/protocol authority.

The default compact shell is now **chat-first**: Chats is the only exposed top-level destination by default, and a one-item tab strip is omitted. Calls, People, Spaces, Publications, Files and future views are user-exposable presentation destinations, not capability boundaries. The **Omnibar** is the universal local-first find-or-do surface and remains able to reach hidden destinations/actions.

The architecture may be rich while the presentation remains simple: scheduled send, Spaces/Rooms/nested Spaces, threads/Topics, Publication, polls, stickers, rich text, disappearing/view-once content, ContactCapsules/MessageRequests, reporting/evidence and privacy-aware previews remain V1 capabilities but appear contextually rather than occupying permanent navigation.

OwnMessenger presentation customization is separate from security/privacy Protection profiles. Focus/Balanced/Playful changes experience intensity; Recommended/Hardened/Maximum Privacy/Custom changes policy. Neither is allowed to silently weaken the other.

See [`RECONCILED_INTERACTION_MODEL.md`](../architecture/ux/RECONCILED_INTERACTION_MODEL.md) and [`SCREEN_MAP_AND_JOURNEYS.md`](../ux-design/SCREEN_MAP_AND_JOURNEYS.md).
