# Messenger Baseline and Ecosystem Architecture Queue

- **Status:** Canonical architecture queue/index; first messenger/community pass partially resolved
- **Checkpoint:** 2026-09-23 06:15 PHT

OM must be both a rigorous secure protocol and a messenger ordinary people actually want to use. Architecture-design items remain blocking until their protocol-impacting boundaries are decided; product wishes that do not affect those boundaries should not masquerade as architecture blockers.

## Resolved in the 06:15 hardening pass

The following queue items now have canonical architecture decisions:

| Queue item | Resolution |
|---|---|
| A-005 communities | `Space` is an organizational/control container over independent Rooms/Publications; membership does not silently inherit. |
| A-006 broadcast | Private announcement = permission-restricted MLS Room; public one-to-many = separate signed `Publication`; no new massive-private-broadcast crypto in V1. |
| A-007 permissions | Capability-scoped authority; roles are bundles; explicit restrictions win; delegation ceilings prevent self-escalation; no required magical owner/Admin bypass. |
| A-008 topics_threads | Replies + lightweight threads + persistent room-local Topics form a layered model; Topics are not sub-Rooms/MLS groups. |
| A-010 baseline_product | First-release user expectations and local/network semantic boundaries are frozen in `MESSENGER_BASELINE_V1.md`. |

Canonical detail lives in:

- `MESSENGER_BASELINE_V1.md`
- `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`
- `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`
- updated `GROUP_CHAT_ARCHITECTURE.md` and `EVENT_MODEL.md`

## Cross-product synthesis behind the decision

The design deliberately takes patterns rather than copying products:

- **Discord:** useful evidence for role/channel granularity and forum organization, but OM avoids an always-privileged creator plus an allow/deny override maze as protocol fundamentals.
- **Telegram:** useful separation of groups, broadcast channels and 2026 Communities that link distinct groups/channels/bots.
- **Matrix:** strong precedent for Spaces containing independent Rooms and for membership-conditioned joins; OM does not adopt a mandatory homeserver authority model.
- **Zulip:** strong precedent for persistent topics plus per-user follow/mute organization in busy conversation.
- **WhatsApp Communities:** useful simple mental model for umbrella community + announcements.

The resulting OM shape is:

```text
Space / Community                 organizational/control container
   |
   +-- Room (ConversationID)      interactive; private V1 uses MLS
   +-- Room (ConversationID)      independent membership/security
   +-- Publication (PublicationID) signed public one-to-many feed
   +-- nested Space
```

## Still blocking before engineering

The first pass intentionally does **not** pretend it resolved adjacent security surfaces:

| Queue item | Remaining architecture question |
|---|---|
| A-001 expressive_media | Safe canonical sticker/animation media types, formats/transcoding, custom reactions and offline behavior. |
| A-002 content_packs | Portable signed/integrity-addressed pack manifests, updates/forks, resource limits, provenance/licensing. |
| A-003 creator_commerce | Whether to standardize generic entitlement proof semantics while keeping settlement/marketplaces external. |
| A-004 bots_agents | Typed service principals, installation, commands, selected-context disclosure, local vs remote LLMs, streaming/automation/revocation. |
| A-009 plugin_ecosystem | Sandboxed client/protocol/provider plugin capabilities, signing, permissions, UI surfaces and anti-fragmentation rules. |
| A-011 interop_bridges | External-network mapping, loops, edits/deletes/reactions fidelity, explicit plaintext/E2EE-end UI and identity mapping. |
| A-012 import_export_escape | Portable export/migration/provenance and user escape from clients/providers without SeedLife dependence. |
| A-013 metadata_threat_model | Dedicated timing/graph/group/link/push/blob/migration metadata analysis and selective-disclosure boundaries. |
| A-014 decentralized_abuse_moderation | Blocking/reporting/spam/bans/appeals/admin abuse/operator exposure without a universal moderator. |
| A-015 protocol_longevity | 20-year dependency/crypto/registry/client-dominance/protocol-generation/EEE survival and migration. |

## Guardrail

Resolved community/baseline architecture may be extended by those remaining items, but they may not reintroduce:

- a mandatory OM homeserver or global moderator;
- provider-owned Room identity;
- silent plaintext readers;
- hidden transitive authority from Space nesting;
- a permanent vendor/creator superuser;
- cross-conversation private tracking identifiers;
- payment/marketplace operation as a requirement for OM Core.

Engineering remains unauthorized while the remaining blocking design items and verification gates are open.
