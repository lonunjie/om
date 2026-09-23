# OM — Master Planning & Design Repository

**Snapshot:** 2026-09-23 08:21 PHT (UTC+08:00)  
**Phase:** Architecture hardening / pre-engineering — engineering is not yet authorized  
**Project name:** **OM** — see `NAMING.md` for its seven canonical meanings, including **0M: zero total control**

This is the current canonical planning and architecture source of truth for OM. It incorporates the complete 2026-08-18 master snapshot plus the September 2026 design reviews covering decentralized authority, post-quantum cryptography, crypto agility, capability-routed/multi-relay delivery, the Discovery Fabric and revocable IntroGate, Cloudflare as an optional provider, invisible infrastructure UX, voice/video calling, privacy-preserving LAN operation, censorship/network resilience, and future SMS/RCS integration, baseline/community semantics, permissionless services/LLM agency, explicit external bridges, portable user escape, metadata-minimizing routing, decentralized/composable safety controls, protocol longevity, and the 0M zero-total-control principle.

Prior snapshots are preserved under `history/20260818/`, `history/20260921-0801/`, `history/20260921-1008/`, `history/20260921-2132/`, `history/20260922-0734/`, `history/20260922-1342/`, `history/20260922-1434/`, `history/20260922-2302/`, `history/20260923-0545/`, `history/20260923-0615/`, `history/20260923-0718/`, and `history/20260923-0756/` for traceability.

## Product thesis

OM is a fast, uncluttered, open-source messenger for Android, iOS, web, and later desktop/native variants, with strong end-to-end encryption and a protocol deliberately designed so no single company, relay, directory, storage service, media provider, or client vendor is indispensable.

**Normal users should not need to understand any of this architecture.**

A non-technical user should experience:

```text
Install OM → identify yourself → talk to people
```

not:

```text
choose relay → choose object store → manage ciphers → configure TURN → rotate keys
```

The sophistication belongs underneath the product.

## Architectural north star

> **Authority must be scoped, replaceable, and non-composable.**

- **Scoped:** a relay has relay authority, not identity authority.
- **Replaceable:** changing a provider does not change who the user is.
- **Non-composable:** using several services from one vendor must not silently recreate a universal controller.

The older mantra remains valid:

> **No infrastructure provider is part of the protocol. Providers are plugins.**

And the implementation discipline remains:

> **Spend complexity only where it buys interoperability, security, reliability, privacy, or user freedom.**

## Constitutional and settled directions

- no universal central authority is required for private communication;
- authority is distributed among protocol rules, users, groups, providers, and governance, each with narrow powers;
- AccountID is stable and provider-independent, but must not be used as a routable network address;
- identity continuity must survive complete cryptographic algorithm replacement;
- aliases are claims/locators, not root identity;
- accepted relationships use private, rotatable, opaque delivery capabilities;
- relays store/forward ciphertext and should not learn stable account identities;
- crypto upgrades are invisible to ordinary users;
- wire crypto uses reviewed, versioned **CryptoProfiles**, not arbitrary algorithm mix-and-match;
- OM is PQ-first and hybrid-secure by default where mature components exist;
- 1:1 secure messaging should track reviewed PQXDH/Triple-Ratchet-class designs without hard-coding one library as the protocol;
- private groups use MLS; PQ/hybrid MLS suites are profile-pluggable while standardization continues;
- SFrame is the preferred route-independent media E2EE layer for voice/video;
- call routing is adaptive: LAN/direct P2P first when policy permits, TURN when required, SFU for scalable groups;
- changing media route must never change content confidentiality;
- Cloudflare is an optional reference deployment target, never a protocol dependency;
- R2 is one possible encrypted blob/storage provider;
- Durable Objects/Workers can be one relay implementation; quotas/costs are operational provider metadata, not protocol constants;
- SMS/MMS and RCS are future TransportAdapters and do not block the initial OM-native product;
- local-first and LAN-capable behavior are architectural goals;
- abuse controls must remain enforceable against hostile modified clients without requiring universal content surveillance;
- official/reference clients are interoperable distributions, never protocol authorities; protocol conformance is public and testable.
- OM is PWA-first, not PWA-only; native shells later reuse the same portable OM Core.
- no OM-operated relay/directory/push/abuse infrastructure is required for the operatorless core.
- future convenience businesses/providers may exist, but users can migrate away without replacing AccountID or canonical history.
- metadata privacy is surface-specific; E2EE does not imply hidden timing/IP/size/membership metadata.
- shared safety advisories are optional signed policy inputs, never universal protocol authority.
- published protocol generations are independently preservable and interpretable without a live OM Project server.
- incompatible protocol evolution forks explicitly rather than reusing old identifiers with new meaning.

## Start here

**All agents:** start with `PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`, `ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`, and `AGENTS.md`. Product engineering is intentionally blocked in this checkpoint; `ENGINEER_START_HERE.md` is a future handoff document, not authorization to code.

1. `PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md` — what OM exists to preserve and how capture remains escapable.
2. `ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md` — machine-enforced architecture process and pre-engineering lock.
3. `BLUEPRINT.md` — consolidated product and technical design.
4. `SYSTEM_ARCHITECTURE.md` — complete layer/component map.
5. `LICENSING_AND_CONTRIBUTION_POLICY.md` — layered open licensing, DCO and protocol patent direction.
6. `PROJECT_STEWARDSHIP_AND_CONTINUITY.md` — SeedLife/lonunjie stewardship, founder-incubation risk and succession rails.
7. `ACCOUNT_CONTINUITY_AND_CONTROL.md` + `IDENTITY_PROTOCOL.md` — canonical identity/control authority.
8. `CRYPTOGRAPHY_AND_AGILITY.md` + `ASYNC_KEY_MATERIAL.md` + `ONE_TO_ONE_SESSION_MANAGEMENT.md` — 1:1 security architecture.
9. `GROUP_CHAT_ARCHITECTURE.md` + `GROUP_COMMIT_AND_STATE_CONVERGENCE.md` — MLS private Rooms plus application sequencing/binding.
10. `MESSENGER_BASELINE_V1.md` + `SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md` + `SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md` — ordinary messenger completeness, Communities/Spaces, Rooms/Publications, roles, moderation, threads and Topics.
11. `RELAY_AND_DELIVERY_ARCHITECTURE.md` + `MULTI_RELAY_AND_DELIVERY_RESILIENCE.md` + `REACHABILITY_AND_OBJECT_REPAIR.md` — private delivery and catastrophic repair.
12. `STORAGE_AND_BLOB_ARCHITECTURE.md` — encrypted storage/backup/location semantics.
13. `TIME_AND_EXPIRY_SEMANTICS.md` — one cross-protocol clock/expiry model.
14. `PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md` + `DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md` + `CLIENTHOST_UPDATE_LIFECYCLE.md` — endpoint/origin/deployment boundaries.
15. `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md` — release trust and reproducibility.
16. `DISCOVERY_FABRIC.md`, `INVITES_INTROGATE_AND_SPAM.md`, `CONTACT_SHARING.md` — discovery and first-contact trust.
17. `CALLS_AND_REALTIME_MEDIA.md` — SFrame/WebRTC media.
18. `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md` + `CENSORSHIP_AND_NETWORK_RESILIENCE.md` — local/network resilience.
19. `RICH_TEXT_AND_CONTENT_SECURITY.md` + `ABUSE_SAFETY_AND_REPORTING.md` — hostile content and safety boundaries.
20. `SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md` + `SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md` + `PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md` — permissionless services, delegated agency, and executable-extension boundaries.
21. `INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md` + `IMPORT_EXPORT_AND_USER_ESCAPE.md` — bridge trust boundaries, foreign provenance, and escape/portability.
22. `METADATA_THREAT_MODEL.md` + `DECENTRALIZED_ABUSE_MODERATION.md` + `PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md` — metadata exposure, scoped safety, and 20-year/0M continuity.
23. UI/accessibility/theme documents — product semantics above Core.
24. `ARCHITECTURE_PHASE_PLAN.md` — blocking verification gates and architecture-freeze criteria.
25. `ENGINEER_START_HERE.md` / `ENGINEERING_HANDOFF.md` — retained for the future transition only.

Existing detailed documents remain canonical where not superseded. Machine-readable authority and file classification live under `machine/`; run `python3 tools/validate_architecture.py` before accepting a checkpoint.

## Continuity / trust checkpoint documents

- `ACCOUNT_CONTINUITY_AND_CONTROL.md` — control-state chain, quarantine/revocation, controller promotion and anti-rollback.
- `RECOVERY_AND_BACKUP_POLICY.md` — no-recovery default, offline root/guardians and history-vs-identity recovery.
- `PROVISIONING_AND_BOOTSTRAP.md` — user-owned Cloudflare, workers.dev and the minimal zero-payment stack.
- `CONTACT_SHARING.md` — signed ContactCapsules and referral semantics.
- `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md` — update trust, provenance, reproducibility and forge independence.
- `SECURITY_PRIVACY_PROFILES.md` — Recommended/Hardened/Maximum Privacy policy bundles.
- `NOTIFICATIONS_AND_PUSH.md` — opaque push and platform tradeoffs.
- `DISAPPEARING_MESSAGES_AND_RETENTION.md` — lifecycle/backup convergence rules.
- `PROTOCOL_EXTENSIBILITY.md` — critical extensions, negotiation and registries.

## Structured records

- `decisions/` — Architecture Decision Records.
- `protocol/` — logical schemas/interfaces.
- `machine/decisions.jsonl`
- `machine/requirements.jsonl`
- `machine/resolved_questions.jsonl + typed gate ledgers`
- `research/CURRENT_REFERENCES_20260921.md`
- `research/CURRENT_UI_REFERENCES_20260922.md`
- `design/` — shared design seeds, component catalog, screen journeys and static shell prototype
- `threat-model/README.md`

## Snapshot rule

Future downloadable archives are complete snapshots named:

`om-master-planning-design-YYYYMMDD-HHMM-PHT.zip`

Do not nest old ZIP archives. Preserve prior source snapshots as ordinary files under `history/` only when useful for traceability.

## 2026-09-21 10:08 discovery/resilience checkpoint

New canonical material:

- `DISCOVERY_FABRIC.md`
- `INVITES_INTROGATE_AND_SPAM.md`
- `MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`
- `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`
- `CENSORSHIP_AND_NETWORK_RESILIENCE.md`
- `CHECKPOINT_20260921_1008.md`
- ADR-0019 through ADR-0025
- ContactDescriptor / Invite / IntroductionTicket / RelaySet / discovery-provider logical schemas

The preceding 2026-09-21 08:01 checkpoint is preserved under `history/20260921-0801/`.

## 2026-09-21 21:32 continuity/trust checkpoint

New canonical material:

- `ACCOUNT_CONTINUITY_AND_CONTROL.md`
- `RECOVERY_AND_BACKUP_POLICY.md`
- `PROVISIONING_AND_BOOTSTRAP.md`
- `CONTACT_SHARING.md`
- `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`
- `SECURITY_PRIVACY_PROFILES.md`
- `NOTIFICATIONS_AND_PUSH.md`
- `DISAPPEARING_MESSAGES_AND_RETENTION.md`
- `PROTOCOL_EXTENSIBILITY.md`
- `CHECKPOINT_20260921_2132.md`
- ADR-0026 through ADR-0042
- AccountControlState / AccountControlHead / RecoveryPolicy / ContactCapsule / SecurityProfile / NotificationPolicy / ProtocolExtension logical schemas

The preceding 10:08 discovery/resilience checkpoint is preserved under `history/20260921-1008/`.


## 2026-09-22 07:34 operatorless/ecosystem checkpoint

New canonical material:

- `OPERATORLESS_CORE_AND_ECOSYSTEM.md`
- `PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`
- `LEGAL_AND_OPERATIONAL_BOUNDARIES.md`
- `EVIDENCE_AND_EXTERNAL_REPORTING.md`
- `CHECKPOINT_20260922_0734.md`
- ADR-0043 through ADR-0050
- EvidencePackage / ProvisioningMethod / ClientDistribution logical schemas

Major change: OM is now explicitly modeled as an **operatorless protocol/core plus independent deployments and distributions**. The first reference client is PWA-first for independence, but native shells remain planned for stronger platform integration. Optional future businesses may sell convenience but receive no protocol privilege.

The preceding 2026-09-21 21:32 continuity/trust checkpoint is preserved under `history/20260921-2132/`.


## 2026-09-22 13:42 shared UI/OwnMessenger checkpoint

New canonical material:

- `UI_UX_SYSTEM.md`
- `MOTION_AND_PERFORMANCE.md`
- `THEMES_FONTS_AND_PERSONALIZATION.md`
- `ACCESSIBILITY_AND_INPUT.md`
- `UI_CORE_PLATFORM_CONTRACT.md`
- `design/` theme/token artifacts
- `research/CURRENT_UI_REFERENCES_20260922.md`
- `CHECKPOINT_20260922_1342.md`
- ADR-0051 through ADR-0058

Major change: the reference product now has one shared `om-ui` for PWA/iOS/Android/Desktop, a semantic MotionSystem, adaptive window layouts, release-gated accessibility and deeply customizable but data-only/sandboxed OwnMessenger themes.

The preceding 2026-09-22 07:34 operatorless/ecosystem checkpoint is preserved under `history/20260922-0734/`.

## 2026-09-22 14:34 PHT — Historical engineering-handoff checkpoint (superseded)

Architecture is now frozen enough to build and attack. New canonical engineering material includes:

- `ENGINEER_START_HERE.md` and `ENGINEERING_HANDOFF.md`;
- `ENGINEERING_REPOSITORY_LAYOUT.md`;
- `PRODUCTION_CRYPTO_INTEGRATION_GATE.md`;
- `DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`;
- `MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`;
- `RICH_TEXT_AND_CONTENT_SECURITY.md`;
- `LOCAL_DATA_LIFECYCLE_AND_DIAGNOSTICS.md`;
- `FORMAL_VERIFICATION_AND_FUZZING.md`;
- `research/SECURITY_CRYPTO_REAUDIT_20260922.md`;
- RichText/IdentityProfile/DeviceVault logical schemas;
- ADR-0059 through ADR-0068.

Multiple identities are structurally supported from v1 but the switcher can ship later. Every built-in theme supports light/dark + Auto. Production crypto remains a release gate rather than something coding agents may improvise.

## 2026-09-22 23:02 architecture-hardening checkpoint

The 14:34 engineering transition is superseded. OM remains in architecture hardening until the architecture-freeze contract deliberately authorizes engineering.

New blocking architecture material includes:

- project philosophy/anti-capture constitution;
- layered licensing/contribution policy and SeedLife stewardship/succession rails;
- asynchronous 1:1/MLS key-material publication;
- one-to-one multi-device session management;
- MLS Commit ordering and atomic group-policy/MLS binding;
- catastrophic RelaySet reachability and blob-location repair;
- unified time/expiry semantics;
- self-hosted ClientHost update lifecycle;
- machine-enforced repository contract/file registry/architecture map.

No product engineering phase begins from this checkpoint.
## 2026-09-23 05:45 PHT — architecture-resolution + OM identity checkpoint

The former 60-item open-question ledger and architecture queue A-001..A-015 are resolved into accepted decisions plus typed verification/release/deferred ledgers. `ARCHITECTURE_RESOLUTIONS_20260923.md` is the human-readable closure record. **No design blocker remains open**, but V-001..V-017 still prevent design confidence from being mistaken for implementation proof; engineering remains unauthorized.

Country-specific legal gating was removed from canonical architecture. `LEGAL_AND_OPERATIONAL_BOUNDARIES.md` now uses a role-based, jurisdiction-neutral model and keeps project operation/custody deliberately minimal.

