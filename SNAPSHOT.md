# OM Architecture Snapshot

Generated: **2026-09-23 08:21 PHT (UTC+08:00)**

## State

OM remains in **architecture-hardening / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`.

The current architecture-design queue A-001..A-015 is now **design-resolved**. This does not authorize engineering: blocking verification gates V-001..V-017 remain the next phase.

## Completed hardening clusters

- 06:15 — messenger baseline + Spaces/Rooms/Communities/permissions/Topics/Publications;
- 07:18 — expressive media + portable packs + entitlement boundary + typed ServicePrincipals/bots/LLMs + capability-sandboxed plugins;
- 07:56 — 0M/zero-total-control + permissionless service creation/delegated agency + external bridge provenance/E2EE boundaries + portable import/export/escape;
- 08:21 — metadata threat model + decentralized abuse/moderation + 20-year protocol longevity/0M continuity.

## New 08:21 architecture

- Metadata privacy is surface-specific. Stable canonical IDs remain above provider routing; private relay delivery uses provider-scoped opaque capabilities and randomized encrypted outer envelopes. E2EE is never presented as a blanket claim about IP/timing/size/membership/push/access metadata.
- Recommended/Hardened/Maximum Privacy share the same crypto floor but may progressively add size padding, private discovery, provider separation, relayed calls, batching and anonymity transports. Expensive cover traffic is explicit and bounded.
- Abuse/moderation is layered and scoped to users, Rooms, Spaces and providers. Optional signed `SafetyAdvisory` sources enable collaborative defense, but subscribers choose enforcement; there is no universal OM moderator, master blocklist or global numeric reputation score.
- Reports remain selective `EvidencePackage` disclosures. Provider denial/delisting affects only provider custody/reach and cannot rewrite AccountID/Room/Service authority.
- Protocol longevity is now explicit: released generations have mirrorable self-contained `Protocol Continuity Bundle`s, assigned registry meanings never change, incompatible evolution forks under new lineage/profile IDs, and dominant clients/extensions gain no protocol authority from market share.
- Catastrophic crypto/project/provider failure never creates a universal OM master key. Use authenticated migration where continuity is provable; otherwise use explicit re-verification/successor/fork semantics.

## Design queue

**No open architecture-design blockers remain.** `design_open=0`.

## Verification is now the blocker

The architecture is selected but not yet proven. Blocking gates now include:

- V-001..V-014: crypto, authority state machines, canonical encoding, browser vault, SFrame, Space/Room permissions, event convergence, plugin sandbox, service/agent authority, expressive media parsers, bridges and archives;
- **V-015:** metadata exposure/privacy-path model;
- **V-016:** decentralized abuse/moderation authority model;
- **V-017:** protocol longevity/0M continuity drill.

## Next phase

The next work should be **architecture verification**, not feature engineering. Start with the highest-risk state/authority gates that can invalidate other work: V-002/V-004/V-008/V-011/V-016, while V-001/V-005/V-006 establish the production crypto/encoding/browser feasibility floor. Engineering remains locked until every blocking gate passes and a later explicit ADR authorizes it.
