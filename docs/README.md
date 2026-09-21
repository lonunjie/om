# OM — Master Planning & Design Repository

**Snapshot:** 2026-09-22 07:34 PHT (UTC+08:00)  
**Phase:** Architecture consolidation → implementation-ready gates  
**Working umbrella name:** **OM — OpenMessenger / OwnMessenger / OurMessenger**

This is the current canonical planning and architecture source of truth for OM. It incorporates the complete 2026-08-18 master snapshot plus the September 2026 design reviews covering decentralized authority, post-quantum cryptography, crypto agility, capability-routed/multi-relay delivery, the Discovery Fabric and revocable IntroGate, Cloudflare as an optional provider, invisible infrastructure UX, voice/video calling, privacy-preserving LAN operation, censorship/network resilience, and future SMS/RCS integration.

Prior snapshots are preserved under `history/20260818/`, `history/20260921-0801/`, `history/20260921-1008/`, and `history/20260921-2132/` for traceability.

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

## What is now conceptually frozen

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

## Start here

1. `BLUEPRINT.md` — consolidated product and technical design.
2. `DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md` — why the major tradeoffs were chosen.
3. `SYSTEM_ARCHITECTURE.md` — complete layer/component map.
4. `PRODUCT_AND_UX_PRINCIPLES.md` — invisible-complexity product rules.
5. `PRIVACY_GOVERNANCE_AUTHORITY.md` — how OM avoids recreating a central controller.
6. `DISCOVERY_FABRIC.md` — resolver fabric, namespaced aliases, ContactDescriptor, private lookup.
7. `INVITES_INTROGATE_AND_SPAM.md` — revocable invites, IntroductionTickets, public-first-contact abuse controls.
8. `RELAY_AND_DELIVERY_ARCHITECTURE.md` + `MULTI_RELAY_AND_DELIVERY_RESILIENCE.md` — private capabilities, RelaySet, failover/redundancy.
9. `CRYPTOGRAPHY_AND_AGILITY.md` — PQ/hybrid crypto, CryptoProfiles, migration, downgrade prevention.
10. `STORAGE_AND_BLOB_ARCHITECTURE.md` — queues, sync, blobs, archive, backup, StoragePool.
11. `CALLS_AND_REALTIME_MEDIA.md` — signaling, SFrame, WebRTC, LAN/P2P/TURN/SFU, cost/privacy policy.
12. `LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md` — private local discovery and WAN-independent local operation direction.
13. `CENSORSHIP_AND_NETWORK_RESILIENCE.md` — no single network choke point, replaceable transports.
14. `TRANSPORT_ADAPTERS_AND_INTEROP.md` — OM-native now; SMS/MMS/RCS/bridges later.
15. `CLOUDFLARE_REFERENCE_DEPLOYMENT.md` — optional frictionless deployment profile.
16. `ABUSE_SAFETY_AND_REPORTING.md` — privacy-preserving abuse boundaries.
17. `IMPLEMENTATION_ROADMAP.md` — implementation order and acceptance gates.
18. `ARCHITECTURE_PHASE_PLAN.md` — architecture gates and exit criteria.
19. `OPERATORLESS_CORE_AND_ECOSYSTEM.md` — operatorless core, role separation and creator-disappearance gate.
20. `PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md` — PWA-first/native-later client strategy.
21. `LEGAL_AND_OPERATIONAL_BOUNDARIES.md` — scoped custody/responsibility and legal-role boundaries.
22. `EVIDENCE_AND_EXTERNAL_REPORTING.md` — local evidence export and optional distribution-specific reporting.
23. `AGENTS.md` — rules for coding/research agents.

Existing detailed documents remain canonical where not superseded: identity, event model, group architecture, client/extension architecture, and PWA reference client.

## Continuity / trust checkpoint documents

- `ACCOUNT_CONTINUITY_AND_CONTROL.md` — control-state chain, quarantine/revocation, controller promotion and anti-rollback.
- `RECOVERY_AND_BACKUP_POLICY.md` — no-recovery default, offline root/guardians and history-vs-identity recovery.
- `PROVISIONING_AND_BOOTSTRAP.md` — user-owned Cloudflare, workers.dev and the minimal zero-payment stack.
- `CONTACT_SHARING.md` — signed ContactCapsules and referral semantics.
- `SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md` — update trust, provenance, reproducibility and forge independence.
- `SECURITY_PRIVACY_PROFILES.md` — Standard/Hardened/Maximum Privacy policy bundles.
- `NOTIFICATIONS_AND_PUSH.md` — opaque push and platform tradeoffs.
- `DISAPPEARING_MESSAGES_AND_RETENTION.md` — lifecycle/backup convergence rules.
- `PROTOCOL_EXTENSIBILITY.md` — critical extensions, negotiation and registries.

## Structured records

- `decisions/` — Architecture Decision Records.
- `protocol/` — logical schemas/interfaces.
- `machine/decisions.jsonl`
- `machine/requirements.jsonl`
- `machine/open_questions.jsonl`
- `research/CURRENT_REFERENCES_20260921.md`
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
