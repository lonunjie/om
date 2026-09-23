# Messenger Baseline and Ecosystem Architecture Queue

- **Status:** Canonical architecture queue/index; A-001..A-015 resolved at design level
- **Checkpoint:** 2026-09-23 08:21 PHT

OM must be both a rigorous secure protocol and a messenger ordinary people actually want to use. The architecture-design queue is now closed; verification gates remain separate so a selected design is never mislabeled proven.

## Resolved architecture clusters

### 06:15 — baseline + Spaces/Rooms/Communities

A-005, A-006, A-007, A-008 and A-010 are resolved by [`MESSENGER_BASELINE_V1.md`](../architecture/events/MESSENGER_BASELINE_V1.md), [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](../architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md), [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](../architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md), [`GROUP_CHAT_ARCHITECTURE.md`](../architecture/groups/GROUP_CHAT_ARCHITECTURE.md) and [`EVENT_MODEL.md`](../architecture/events/EVENT_MODEL.md).

### 07:18 — expressive media + packs + entitlements + services/LLMs + plugins

A-001, A-002, A-003, A-004 and A-009 are resolved by [`EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`](../architecture/ecosystem/EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md), [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](../architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) and [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](../architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md).

### 07:56 — 0M + permissionless services + external bridges + user escape

A-011/A-012 and the 0M/service-creation refinements are resolved by:

- [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](../architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md);
- [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](../architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md);
- [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md);
- [`NAMING.md`](../constitution/NAMING.md) and [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](../constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md).

Key decisions: local permissionless ServiceID creation; authority-based rather than human/AI classification; typed BridgePrincipals with explicit foreign provenance/E2EE termination; `om-archive-v1` restoring data/evidence rather than authority; and **0M = zero total control**.

### 08:21 — metadata + decentralized safety + 20-year continuity

A-013, A-014 and A-015 are resolved by:

- [`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md);
- [`DECENTRALIZED_ABUSE_MODERATION.md`](../architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md);
- [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](../constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md).

Key decisions:

- metadata privacy is surface-specific; canonical IDs stay above provider routing and E2EE is never a blanket metadata claim;
- moderation is scoped to the user/Room/Space/provider authority that actually exists;
- signed `SafetyAdvisory` sources are optional policy inputs, never universal authority;
- published protocol generations are independently mirrorable/interpretable through Protocol Continuity Bundles;
- registry meanings never change and incompatible protocol evolution forks explicitly;
- dominant clients/extensions receive no special protocol rights from market share.

## Architecture design queue

**No open A-series design blocker remains.** The machine ledger is authoritative.

## Verification is still blocking

The selected design now has V-001..V-017 proof obligations. The 08:21 pass adds:

- V-015 metadata exposure/privacy-path model;
- V-016 decentralized abuse/moderation authority model;
- V-017 protocol longevity/0M continuity drill.

Product engineering remains unauthorized until all blocking verification gates pass and a later ADR explicitly authorizes engineering.

## Guardrail

Verification or new standards evidence may reopen a resolved design through an explicit ADR, but future work may not silently reintroduce:

- a mandatory OM homeserver, app store, service/bot issuer, bot store, marketplace, global moderator or live registry oracle;
- provider-owned AccountID, Room identity, ServiceID, BridgeID, PackID or PluginID;
- autonomous service output silently authored as a human AccountPrincipal;
- AI-detection as an authentication primitive;
- bridge-created synthetic AccountIDs for foreign people;
- silent plaintext/E2EE downgrade across bridges;
- imported foreign history presented as native cryptographically authenticated OM history;
- hidden transitive authority from Space nesting;
- a permanent vendor/creator superuser;
- executable code hidden in media/content packs;
- payment/marketplace operation as a requirement for OM Core;
- canonical shared semantics available only through opaque proprietary plugin code;
- E2EE used as an unsupported claim that all metadata is hidden;
- a universal reputation score/master blocklist whose decision becomes protocol truth;
- reuse of an existing registry/profile ID for incompatible future semantics;
- any single party acquiring total protocol-wide control contrary to 0M.
