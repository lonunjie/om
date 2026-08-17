# OM System Architecture — Phase 0 working model

- **Status:** Proposed baseline
- **Date:** 2026-08-18
- **Brand/project shorthand:** **OM** = OpenMessenger / OwnMessenger / OurMessenger

## System intent

OM is a user-sovereign messaging system with a fast mainstream messenger UX. The infrastructure is a set of replaceable Lego blocks connected by open contracts. The user should be able to replace a relay, storage provider, directory, backup target, or transport without replacing their cryptographic identity or losing the canonical conversation model.

## Logical layers

```text
Android · iOS · Web
        │
        ▼
Unified Conversation / Event Model
        │
 ┌──────┼──────────┬───────────┬────────────┐
 │      │          │           │            │
Identity Crypto   Router    Multi-device  Storage policy
 │      │          │           │            │
 │      │      Transport       │        StoragePool
 │      │       adapters       │            │
 │      │          │           │      Blob/Backup/Recovery
 │      │          │           │            │
Directory      RelayProvider   Push      Provider adapters
Provider            │        Provider         │
 │                  │                         ├─ S3-compatible
 ├─ OM/default      ├─ community              ├─ WebDAV
 ├─ domain/self     ├─ Cloudflare             ├─ Drive-class
 └─ other           └─ self-hosted            ├─ NAS/local
                                              └─ other
```

Cross-cutting systems: threat model, abuse/DDoS controls, bot permissions, plugin sandboxing, migrations, observability, versioning, conformance, governance.

## Identity model

### Root identity

The root is a cryptographic account identity, not a username, phone number, email address, cloud account, or relay endpoint.

### Aliases and discovery

Human-friendly identifiers are replaceable resolvers:

- `@username`
- QR code / invite link
- optional phone-number discovery
- optional domain/namespace alias
- future third-party directory providers

An alias resolves to or introduces a signed account identity. Existing contacts continue to know the cryptographic identity if an alias changes or disappears.

### Devices

Each linked Android, iOS, web, or future desktop device has its own device identity/key material. Device addition and revocation are explicit account events.

### Person records, alias churn, and recycled phone numbers

The human-facing contact concept is a local `PersonRecord` that may accumulate multiple historical `AccountIdentity` and `AliasClaim` records. Phone numbers are explicitly modeled as recyclable aliases. Reuse of the same phone number by a different cryptographic identity MUST NOT transfer trust or silently replace the prior person. Historical aliases should normally be collapsed from the mainstream UI rather than creating permanent address-book cruft.

Identity replacement supports several proof strengths: direct old-account continuity signatures; optional pre-authorized threshold recovery guardians; direct contact attestations as non-authoritative evidence; and manual local merging when no proof exists. OM does not depend on a global transitive Web of Trust. See `IDENTITY_CONTINUITY.md`.

## Contact establishment and anti-spam boundary

A public username is not a permanent public mailbox address.

```text
@username / QR / invite
        │
        ▼
identity introduction / contact request
        │
 user accepts or policy permits
        ▼
pairwise private, rotatable delivery capability
        │
        ▼
normal encrypted messaging
```

Unknown contacts should be unable to immediately send unlimited messages, large attachments, calls, group floods, or arbitrary bot payloads.

## Relay model

`RelayProvider` is a capability, not a vendor.

A conforming relay should support the minimal store-and-forward role required by the protocol while learning as little as practical. Clients may use official/community infrastructure, a user-owned provider such as a Cloudflare implementation, or a self-hosted conforming relay.

Relay migration must preserve account identity and established relationships. Relay descriptors are signed by the account identity and can be rotated.

## Storage model

### Storage classes

1. **Delivery queue** — encrypted envelopes awaiting delivery; short-lived.
2. **Hot sync** — recent encrypted data needed for active device convergence.
3. **Hot media** — frequently accessed photos/video/files under user quota policy.
4. **Cold archive** — long-term encrypted history/backup snapshots.
5. **Recovery** — small long-lived encrypted/recovery metadata with stricter survivability rules.

Hot storage and cold storage are independent policies and may use different providers.

### StoragePool

A `StoragePool` composes one or more storage providers. The first redundancy mechanisms should be simple mirroring; erasure coding is reserved by the architecture but deferred until restore, repair, and manifest semantics are proven.

Examples:

```text
Photos hot:       R2-compatible provider, 2 GB target
Photos backup:    NAS + Drive-class mirror
Videos hot:       local only, 1 GB cache
Videos backup:    NAS
Files hot:        S3-compatible, 1 GB target
Text archive:     encrypted compressed segments, keep all
Recovery:         two independent destinations + offline recovery material
```

### Encrypted manifests

Provider locations and redundancy state are recorded in encrypted, authenticated, versioned snapshot manifests. Restore must detect missing objects, stale manifests, rollback, and partial-provider failure.

## Transport model

The conversation model is transport-agnostic. Adapters may include:

- OM-native secure transport
- SMS/MMS where supported
- RCS where authorized interfaces exist
- optional Matrix/XMPP/other bridges
- opportunistic P2P later

Transport capability and trust are explicit. A bridge or legacy transport must never silently inherit the security label of OM-native E2EE.

## Bot and extension model

Bots are explicit identities with machine-readable type and scoped permissions. They cannot rely on client goodwill for abuse control. Group access is permissioned and visible to participants.

Third-party infrastructure/provider plugins receive only the minimum secrets/capabilities necessary for their role. The plugin model must not create a universal extension process that can read all message plaintext.

## Push model

Push is a wake-up mechanism, not a message transport. Push payloads should contain opaque information sufficient to wake/schedule the client but not message content. Push provider dependence must not become identity or archive dependence.

## Reliability model

The protocol must define behavior for:

- duplicates
- replay
- reordering
- retries
- partial provider failure
- expiring queues
- stale devices
- revoked devices
- interrupted provider migration
- storage quota exhaustion
- provider suspension/unavailability
- offline restore

The desired property is graceful degradation: failure of one replaceable component should not unnecessarily corrupt unrelated identity/conversation state.

## Open-system promise

OM should be openly implementable, not merely source-visible. A third party should be able to build a compatible client, relay, directory, or storage provider from published contracts and conformance tests.

The project should avoid marketing claims that other systems are universally closed. OM's differentiator is that **openness and portability are architectural defaults across the whole stack rather than one feature or one server deployment mode.**


## Identity discovery and public verification

OM separates **recognition**, **discovery**, and **authentication**. Names, photos, mutual contacts, usernames, phone numbers, and email addresses may help a user find someone but do not independently prove real-world identity.

The identity layer supports composable evidence: cryptographic account continuity, domain-backed aliases, organization-delegated roles, direct/manual verification, bounded contact attestations, and optional verifier attestations. Verification records its evidence source rather than collapsing to one global trust score.

Domain-scoped aliases such as `alice@example.com` are first-class for organizations and public identities. A domain can authorize employee, role, service, and bot identities without owning the user's underlying AccountIdentity.

Public aliases resolve to constrained introduction endpoints rather than unlimited public write access to a user's private relationship queues.

See `IDENTITY_DISCOVERY_VERIFICATION.md`.


## Private group architecture

Human group membership is a set of `AccountID`s; cryptographic membership is a set of `DeviceIdentity` clients.

```text
Alice                 Alice / phone
Bob       ->          Alice / web
Carol                 Bob / phone
                      Carol / phone
```

Selected direction:

- MLS for group key architecture;
- `GroupState` contains application roles/policy separately from raw MLS state;
- membership changes bind application state to MLS epoch changes;
- default history is since-join;
- old history is shared through encrypted HistoryBundles;
- delivery can fan out across different user relay providers;
- relay-side group broadcast is optional optimization only;
- no mandatory shared group archive;
- bots and bridges are explicit trust boundaries.

See `GROUP_CHAT_ARCHITECTURE.md`.


## Client and extension boundary

OM separates protocol/core from product presentation.

```text
Official UI / third-party UI
            ↓
      stable client API
            ↓
          OM Core
            ↓
Crypto / Relay / Storage / Transport providers
```

The first reference implementation may be web/PWA for protocol velocity, but web is modeled as a normal independently revocable DeviceIdentity. Android/iOS remain platform adapters with capabilities the browser does not universally provide.

Third-party clients are supported by public protocol specifications, SDK/core libraries, and conformance tests. Arbitrary executable plugins inside the official app are deferred in favor of independent clients and narrow safe presentation extensions.

## Message-request and service-account boundary

Unknown senders enter through a constrained `IntroductionEndpoint` and `MessageRequest` flow. Accepted contacts receive private `RelationshipBinding` delivery capabilities.

Organizations delegate stable `ServiceIdentity` roles to scoped bot/human `AgentAuthorization`s. Bots never need the organization's root identity key merely to process messages.


## Canonical event spine

```text
OM Native / SMS / RCS / bridges
            │
      transport adapters
            │
            ▼
   canonical semantic events
            │
      local materialized views
            │
   official or third-party UI
```

The baseline durable event vocabulary is intentionally small:

- `message.create`
- `message.edit`
- `message.delete`
- `reaction.set`
- `receipt.update`

Identity/group/device authority stays in separate control-state protocols. Typing/presence is ephemeral. Mute/pin/drafts/search are local state.

See `EVENT_MODEL.md`.
