# Messenger Baseline and Ecosystem Architecture Queue

- **Status:** Canonical architecture queue/index; baseline, communities, extension/services and bridge/escape clusters resolved at design level
- **Checkpoint:** 2026-09-23 07:56 PHT

OM must be both a rigorous secure protocol and a messenger ordinary people actually want to use. Architecture-design items remain blocking until their protocol-impacting boundaries are decided; verification gates remain separate so a plausible design is never mislabeled proven.

## Resolved architecture clusters

### 06:15 — baseline + Spaces/Rooms/Communities

A-005, A-006, A-007, A-008 and A-010 are resolved by `MESSENGER_BASELINE_V1.md`, `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`, `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`, `GROUP_CHAT_ARCHITECTURE.md` and `EVENT_MODEL.md`.

### 07:18 — expressive media + packs + entitlements + services/LLMs + plugins

A-001, A-002, A-003, A-004 and A-009 are resolved by the expressive-media/content-pack, ServicePrincipal and plugin architecture documents.

### 07:56 — 0M + permissionless services + external bridges + user escape

| Queue / principle | Resolution |
|---|---|
| 0M / decentralization | `0M` is the seventh canonical meaning: zero parties have total control. Scoped central services may exist, but none may become an indispensable universal OM authority. |
| service creation / human-AI boundary | ServiceID creation is local and permissionless. Hosting/discovery/catalogs are replaceable. OM classifies authority rather than trying to detect human-vs-AI content; autonomous on-behalf-of agents use scoped `RepresentationGrant`s. |
| A-011 interop_bridges | External gateways are typed `BridgePrincipal`s. Foreign actors remain bridge-attested provenance; private-Room plaintext export/E2EE termination is visible; semantic loss and loop/dedup lineage are explicit. |
| A-012 import_export_escape | `om-archive-v1` is a public portability model. Archives restore history/evidence, not authority; exact native objects are preserved where possible and foreign imports remain non-routable provenance-bearing historical records. |

Canonical detail lives in:

- `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`
- `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`
- `INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`
- `IMPORT_EXPORT_AND_USER_ESCAPE.md`
- `TRANSPORT_ADAPTERS_AND_INTEROP.md`
- updated `EVENT_MODEL.md`, discovery/identity documents and logical schemas.

## Still blocking before engineering

| Queue item | Remaining architecture question |
|---|---|
| A-013 metadata_threat_model | Dedicated timing/graph/group/link/push/blob/service/bridge/import/migration metadata analysis and selective-disclosure boundaries. |
| A-014 decentralized_abuse_moderation | Blocking/reporting/spam/bans/appeals/admin abuse/operator exposure/service abuse without a universal moderator. |
| A-015 protocol_longevity | 20-year dependency/crypto/registry/client-dominance/protocol-generation/EEE survival and migration, now explicitly tested against the 0M principle. |

## Verification is still blocking

The resolved design clusters retain explicit proof obligations. V-011 now includes permissionless ServiceGenesis, host migration and RepresentationGrant impersonation boundaries. V-013 covers bridge provenance/fidelity/loop behavior. V-014 covers cross-implementation archive portability and hostile import safety.

## Guardrail

Resolved architecture may be extended by A-013..A-015, but those passes may not reintroduce:

- a mandatory OM homeserver, app store, service/bot issuer, bot store, marketplace or global moderator;
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
- any single party acquiring total protocol-wide control contrary to 0M.

Engineering remains unauthorized while blocking design and verification gates are open.
