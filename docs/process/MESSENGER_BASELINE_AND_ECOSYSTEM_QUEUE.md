# Messenger Baseline and Ecosystem Architecture Queue

- **Status:** Canonical architecture queue/index; messenger/community and extension/ecosystem passes resolved at design level
- **Checkpoint:** 2026-09-23 07:18 PHT

OM must be both a rigorous secure protocol and a messenger ordinary people actually want to use. Architecture-design items remain blocking until their protocol-impacting boundaries are decided; verification gates remain separate so a plausible design is never mislabeled proven.

## Resolved architecture clusters

### 06:15 — baseline + Spaces/Rooms/Communities

A-005, A-006, A-007, A-008 and A-010 are resolved by `MESSENGER_BASELINE_V1.md`, `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`, `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`, `GROUP_CHAT_ARCHITECTURE.md` and `EVENT_MODEL.md`.

### 07:18 — expressive media + packs + entitlements + services/LLMs + plugins

| Queue item | Resolution |
|---|---|
| A-001 expressive_media | Baseline expressive payloads are bounded inert WebP assets with semantic sticker/custom-emoji/reaction/loop kinds; conversation rendering is BlobID-based and independent of a pack/catalog. |
| A-002 content_packs | `ContentPackID` + immutable canonical releases + signed update lineage; manifests are data-only, directly installable/hostable/forkable, with provenance/licensing metadata but no mandatory catalog. |
| A-003 creator_commerce | OM standardizes only an optional generic entitlement-presentation hook for acquisition/update. Payment, settlement, tax, refunds, pricing and marketplace operation remain external. Entitlements never gate rendering of content already delivered in a conversation. |
| A-004 bots_agents | Bots/agents/LLMs are typed `ServicePrincipal`s. Selected-context invocation is the default; scoped subscriptions/full-Room membership require explicit grants, and a full-Room service is a visible plaintext recipient. Local inference, network egress and tool authority are distinct. |
| A-009 plugin_ecosystem | Protocol extensions, inert packs, sandboxed local plugins, isolated web apps, remote services and provider adapters are distinct classes. Local executable plugins use capability-scoped WebAssembly Component Model/WIT; no ambient authority or arbitrary in-process official-client plugins. |

Canonical detail lives in:

- `EXPRESSIVE_MEDIA_CONTENT_PACKS_AND_ENTITLEMENTS.md`
- `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`
- `PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`
- `PROTOCOL_EXTENSIBILITY.md`
- updated `EVENT_MODEL.md`, `GROUP_CHAT_ARCHITECTURE.md`, `RICH_TEXT_AND_CONTENT_SECURITY.md` and logical schemas

## Still blocking before engineering

| Queue item | Remaining architecture question |
|---|---|
| A-011 interop_bridges | External-network mapping, loops, edits/deletes/reactions fidelity, explicit plaintext/E2EE-end UI and identity mapping. |
| A-012 import_export_escape | Portable export/migration/provenance and user escape from clients/providers without SeedLife dependence. |
| A-013 metadata_threat_model | Dedicated timing/graph/group/link/push/blob/migration metadata analysis and selective-disclosure boundaries. |
| A-014 decentralized_abuse_moderation | Blocking/reporting/spam/bans/appeals/admin abuse/operator exposure without a universal moderator. |
| A-015 protocol_longevity | 20-year dependency/crypto/registry/client-dominance/protocol-generation/EEE survival and migration. |

## Verification is still blocking

The resolved design cluster adds explicit proof obligations for plugin sandbox enforcement, service-principal authorization/disclosure/automation-loop behavior, and malformed expressive-media/pack safety. These are verification gates rather than reasons to keep the architecture questions artificially open.

## Guardrail

Resolved architecture may be extended by the remaining items, but they may not reintroduce:

- a mandatory OM homeserver, app store, pack catalog, bot store, marketplace or global moderator;
- provider-owned identity, Room identity, ServiceID, PackID or PluginID;
- silent plaintext readers or bots disguised as humans;
- hidden transitive authority from Space nesting;
- a permanent vendor/creator superuser;
- executable code hidden in media/content packs;
- payment/marketplace operation as a requirement for OM Core;
- canonical shared semantics available only through opaque proprietary plugin code.

Engineering remains unauthorized while blocking design and verification gates are open.
