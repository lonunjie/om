# OM architecture — overview and reading guide

**Snapshot:** 2026-09-24 02:29 PHT (UTC+08:00)  
**Phase:** Architecture verification / pre-engineering — engineering is not yet authorized  
**Project name:** **OM** — see [`NAMING.md`](constitution/NAMING.md) for its six canonical meanings plus **0M: zero total control**

This is the current canonical planning and architecture source of truth for OM. It incorporates the complete 2026-08-18 master snapshot plus the September 2026 design reviews covering decentralized authority, post-quantum cryptography, crypto agility, capability-routed/multi-relay delivery, the Discovery Fabric and revocable IntroGate, Cloudflare as an optional provider, invisible infrastructure UX, voice/video calling, privacy-preserving LAN operation, censorship/network resilience, and future SMS/RCS integration, baseline/community semantics, permissionless services/LLM agency, explicit external bridges, portable user escape, metadata-minimizing routing, decentralized/composable safety controls, protocol longevity, and the 0M zero-total-control principle.

Every earlier design checkpoint is a git commit tagged `checkpoint/YYYYMMDD-HHMM` (for example `git show checkpoint/20260921-0801`); see [checkpoints/README.md](checkpoints/README.md). The ADRs in [`decisions/`](../decisions/INDEX.md) record *why* each choice was made, and [reviews/](reviews/) holds the independent reviews that changed the design.

## How this documentation is organized

| Folder | What it holds |
|---|---|
| [`constitution/`](constitution/) | What OM exists to protect: philosophy, anti-capture, 0M, licensing, stewardship, legal boundaries. Changes here need the strongest justification. |
| [`overview/`](overview/) | The whole system on a page: blueprint, system architecture, design rationale, lean rules, product principles. |
| [`architecture/`](architecture/) | One folder per domain (identity, crypto, groups, discovery, delivery, storage, events, calls, network, client, safety, providers, ux, ecosystem). These are the normative designs. |
| [`threat-model/`](threat-model/) | Narrative threat model and metadata threat model; the structured register is [`machine/threat_register.jsonl`](../machine/threat_register.jsonl). |
| [`process/`](process/) | How the design becomes software: architecture contract, phase plan, release waves, verification strategy, future engineering handoff. |
| [`reviews/`](reviews/) | Dated independent reviews and resolution records. |
| [`checkpoints/`](checkpoints/) | Dated checkpoint records (what changed, why, what was verified). |
| [`research/`](research/) | Dated external references behind decisions. Research informs; it is not normative. |
| [`ux-design/`](ux-design/) | Reconciled UI/UX system: journeys, semantic contracts, tokens, icons/media assets, prototypes, validation and architecture reconciliation records. |
| [`reports/`](reports/) | Working reports from reviews and operator sessions. |

Normative protocol schemas live in [`spec/`](../spec/), executable evidence in [`verification/`](../verification/README.md), and machine-checked project state in [`machine/`](../machine/).


## Product thesis

OM is a fast, uncluttered, open-source messenger for Android, iOS, web, and native desktop variants, with strong end-to-end encryption and a protocol deliberately designed so no single company, relay, directory, storage service, media provider, or client vendor is indispensable.

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
- 1:1 secure messaging is PQXDH + Double Ratchet in v1 (ADR-0118) and tracks reviewed Triple-Ratchet-class designs for v2, without hard-coding one library as the protocol;
- private groups use MLS; PQ/hybrid MLS suites are profile-pluggable while standardization continues;
- SFrame is the preferred route-independent media E2EE layer for voice/video;
- call routing is adaptive: LAN/direct P2P first when policy permits, TURN when required, SFU for scalable groups;
- changing media route must never change content confidentiality;
- Cloudflare is an optional reference deployment target, never a protocol dependency;
- R2 is one possible encrypted blob/storage provider;
- Durable Objects/Workers can be one relay implementation; quotas/costs are operational provider metadata, not protocol constants;
- SMS/MMS and RCS are future TransportAdapters and do not block the initial OM-native product;
- local-first offline-LAN message/file delivery is an optional V1 capability where a privacy-preserving route can be established;
- abuse controls must remain enforceable against hostile modified clients without requiring universal content surveillance;
- official/reference clients are interoperable distributions, never protocol authorities; protocol conformance is public and testable.
- OM is PWA-first, not PWA-only; Android and iOS are explicit V1 reference-family targets over the same portable OM Core/shared UI, while public native releases need not be simultaneous with the first PWA slice (ADR-0150).
- no OM-operated relay/directory/push/abuse infrastructure is required for the operatorless core.
- future convenience businesses/providers may exist, but users can migrate away without replacing AccountID or canonical history.
- metadata privacy is surface-specific; E2EE does not imply hidden timing/IP/size/membership metadata.
- shared safety advisories are optional signed policy inputs, never universal protocol authority.
- published protocol generations are independently preservable and interpretable without a live OM Project server.
- incompatible protocol evolution forks explicitly rather than reusing old identifiers with new meaning.

## Start here

**All agents:** start with [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md), [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md), and [`AGENTS.md`](../AGENTS.md). Product engineering is intentionally blocked in this checkpoint; [`ENGINEER_START_HERE.md`](process/ENGINEER_START_HERE.md) is a future handoff document, not authorization to code.

1. [`PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`](constitution/PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md) — what OM exists to preserve and how capture remains escapable.
2. [`ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md`](process/ARCHITECTURE_CONTRACT_AND_DRIFT_CONTROL.md) — machine-enforced architecture process and pre-engineering lock.
3. [`BLUEPRINT.md`](overview/BLUEPRINT.md) — consolidated product and technical design.
4. [`SYSTEM_ARCHITECTURE.md`](overview/SYSTEM_ARCHITECTURE.md) — complete layer/component map.
5. [`LICENSING_AND_CONTRIBUTION_POLICY.md`](constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) — layered open licensing, DCO and protocol patent direction.
6. [`PROJECT_STEWARDSHIP_AND_CONTINUITY.md`](constitution/PROJECT_STEWARDSHIP_AND_CONTINUITY.md) — SeedLife/lonunjie stewardship, founder-incubation risk and succession rails.
7. [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) + [`IDENTITY_PROTOCOL.md`](architecture/identity/IDENTITY_PROTOCOL.md) — canonical identity/control authority.
8. [`CRYPTOGRAPHY_AND_AGILITY.md`](architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md) + [`ASYNC_KEY_MATERIAL.md`](architecture/crypto/ASYNC_KEY_MATERIAL.md) + [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) — 1:1 security architecture.
9. [`GROUP_CHAT_ARCHITECTURE.md`](architecture/groups/GROUP_CHAT_ARCHITECTURE.md) + [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) — MLS private Rooms plus application sequencing/binding.
10. [`MESSENGER_BASELINE_V1.md`](architecture/events/MESSENGER_BASELINE_V1.md) + [`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](architecture/groups/SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md) + [`SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md`](architecture/groups/SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md) — ordinary messenger completeness, Communities/Spaces, Rooms/Publications, roles, moderation, threads and Topics.
11. [`RELAY_AND_DELIVERY_ARCHITECTURE.md`](architecture/delivery/RELAY_AND_DELIVERY_ARCHITECTURE.md) + [`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](architecture/delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) + [`REACHABILITY_AND_OBJECT_REPAIR.md`](architecture/delivery/REACHABILITY_AND_OBJECT_REPAIR.md) — private delivery and catastrophic repair.
12. [`STORAGE_AND_BLOB_ARCHITECTURE.md`](architecture/storage/STORAGE_AND_BLOB_ARCHITECTURE.md) — encrypted storage/backup/location semantics.
13. [`TIME_AND_EXPIRY_SEMANTICS.md`](architecture/events/TIME_AND_EXPIRY_SEMANTICS.md) — one cross-protocol clock/expiry model.
14. [`PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md`](architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md) + [`DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md`](architecture/client/DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md) + [`CLIENTHOST_UPDATE_LIFECYCLE.md`](architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md) — endpoint/origin/deployment boundaries.
15. [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) — release trust and reproducibility.
16. [`DISCOVERY_FABRIC.md`](architecture/discovery/DISCOVERY_FABRIC.md), [`INVITES_INTROGATE_AND_SPAM.md`](architecture/discovery/INVITES_INTROGATE_AND_SPAM.md), [`CONTACT_SHARING.md`](architecture/discovery/CONTACT_SHARING.md) — discovery and first-contact trust.
17. [`CALLS_AND_REALTIME_MEDIA.md`](architecture/calls/CALLS_AND_REALTIME_MEDIA.md) — SFrame/WebRTC media.
18. [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md) + [`CENSORSHIP_AND_NETWORK_RESILIENCE.md`](architecture/network/CENSORSHIP_AND_NETWORK_RESILIENCE.md) — local/network resilience.
19. [`RICH_TEXT_AND_CONTENT_SECURITY.md`](architecture/safety/RICH_TEXT_AND_CONTENT_SECURITY.md) + [`ABUSE_SAFETY_AND_REPORTING.md`](architecture/safety/ABUSE_SAFETY_AND_REPORTING.md) — hostile content and safety boundaries.
20. [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](architecture/ecosystem/SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) + [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](architecture/ecosystem/SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) + [`PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md`](architecture/ecosystem/PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md) — permissionless services, delegated agency, and executable-extension boundaries.
21. [`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](architecture/ecosystem/INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) + [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) — bridge trust boundaries, foreign provenance, and escape/portability.
22. [`METADATA_THREAT_MODEL.md`](threat-model/METADATA_THREAT_MODEL.md) + [`DECENTRALIZED_ABUSE_MODERATION.md`](architecture/safety/DECENTRALIZED_ABUSE_MODERATION.md) + [`PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md`](constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md) — metadata exposure, scoped safety, and 20-year/0M continuity.
23. UI/accessibility/theme documents — product semantics above Core.
24. [`ARCHITECTURE_PHASE_PLAN.md`](process/ARCHITECTURE_PHASE_PLAN.md) + [`verification/README.md`](../verification/README.md) — blocking verification gates, current evidence, and the evidence-vs-passage boundary.
25. [`ENGINEER_START_HERE.md`](process/ENGINEER_START_HERE.md) / [`ENGINEERING_HANDOFF.md`](process/ENGINEERING_HANDOFF.md) — retained for the future transition only.
26. [`HARDENING_REVIEW_20260924.md`](reviews/HARDENING_REVIEW_20260924.md) — independent gap/blind-spot/weak-spot review (F-01..F-17) and where each fix lives.
27. [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) + [`verification/GATE_PASS_CRITERIA.md`](../verification/GATE_PASS_CRITERIA.md) — delivery waves, scope-granular unlock, and two-phase gate criteria.
28. [`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md) — zero-signup sponsored onboarding, Home tiers, and anti-monoculture.
29. [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) + [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) + [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md) — endpoint adversaries, account end-of-life, and security response without a kill switch.

Existing detailed documents remain canonical where not superseded. Machine-readable authority and file classification live under `machine/`; run `python3 tools/validate_architecture.py` before accepting a checkpoint.

## Continuity / trust checkpoint documents

- [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) — control-state chain, quarantine/revocation, controller promotion and anti-rollback.
- [`RECOVERY_AND_BACKUP_POLICY.md`](architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) — recovery on by default (passkey or recovery key custody, delayed and vetoable activation), guardians, and history-vs-identity recovery.
- [`PROVISIONING_AND_BOOTSTRAP.md`](architecture/providers/PROVISIONING_AND_BOOTSTRAP.md) — user-owned Cloudflare, workers.dev and the minimal zero-payment stack.
- [`CONTACT_SHARING.md`](architecture/discovery/CONTACT_SHARING.md) — signed ContactCapsules and referral semantics.
- [`SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md`](architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md) — update trust, provenance, reproducibility and forge independence.
- [`SECURITY_PRIVACY_PROFILES.md`](architecture/ux/SECURITY_PRIVACY_PROFILES.md) — Recommended/Hardened/Maximum Privacy policy bundles.
- [`NOTIFICATIONS_AND_PUSH.md`](architecture/client/NOTIFICATIONS_AND_PUSH.md) — opaque push and platform tradeoffs.
- [`DISAPPEARING_MESSAGES_AND_RETENTION.md`](architecture/storage/DISAPPEARING_MESSAGES_AND_RETENTION.md) — lifecycle/backup convergence rules.
- [`PROTOCOL_EXTENSIBILITY.md`](constitution/PROTOCOL_EXTENSIBILITY.md) — critical extensions, negotiation and registries.

## Structured records

- `decisions/` — Architecture Decision Records.
- `spec/protocol/` — logical schemas/interfaces.
- [`machine/decisions.jsonl`](../machine/decisions.jsonl)
- [`machine/requirements.jsonl`](../machine/requirements.jsonl)
- `machine/resolved_questions.jsonl + typed gate ledgers`
- [`docs/research/CURRENT_REFERENCES_20260921.md`](research/CURRENT_REFERENCES_20260921.md)
- [`docs/research/CURRENT_UI_REFERENCES_20260922.md`](research/CURRENT_UI_REFERENCES_20260922.md)
- `docs/ux-design/` — canonical detailed UI/UX contracts, component/journey specs, semantic tokens, icons/media, prototypes and validation artifacts under the architecture UX rules
- [`docs/threat-model/README.md`](threat-model/README.md) + [`machine/threat_register.jsonl`](../machine/threat_register.jsonl) — narrative and structured threat model
- [`machine/requirement_traceability.json`](../machine/requirement_traceability.json) — every requirement mapped to its proof obligation
