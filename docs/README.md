# OM — Master Planning & Design Repository

**Snapshot:** 2026-09-21 08:01 PHT (UTC+08:00)  
**Phase:** Architecture consolidation → implementation-ready gates  
**Working umbrella name:** **OM — OpenMessenger / OwnMessenger / OurMessenger**

This is the current canonical planning and architecture source of truth for OM. It incorporates the complete 2026-08-18 master snapshot plus the September 2026 design review covering decentralized authority, post-quantum cryptography, crypto agility, capability-routed relays, Cloudflare as an optional provider, invisible infrastructure UX, voice/video calling, LAN/P2P routing, and future SMS/RCS integration.

The prior snapshot is preserved verbatim under `history/20260818/` for traceability.

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
- official clients are reference implementations; protocol conformance is public and testable.

## Start here

1. `BLUEPRINT.md` — consolidated product and technical design.
2. `DESIGN_RATIONALE_AND_DISCUSSION_SYNTHESIS.md` — why the major tradeoffs were chosen.
3. `SYSTEM_ARCHITECTURE.md` — complete layer/component map.
4. `PRODUCT_AND_UX_PRINCIPLES.md` — invisible-complexity product rules.
5. `PRIVACY_GOVERNANCE_AUTHORITY.md` — how OM avoids recreating a central controller.
6. `CRYPTOGRAPHY_AND_AGILITY.md` — PQ/hybrid crypto, CryptoProfiles, migration, downgrade prevention.
7. `RELAY_AND_DELIVERY_ARCHITECTURE.md` — capability-addressed relay model.
8. `STORAGE_AND_BLOB_ARCHITECTURE.md` — queues, sync, blobs, archive, backup, StoragePool.
9. `CALLS_AND_REALTIME_MEDIA.md` — signaling, SFrame, WebRTC, LAN/P2P/TURN/SFU, cost/privacy policy.
10. `TRANSPORT_ADAPTERS_AND_INTEROP.md` — OM-native now; SMS/MMS/RCS/bridges later.
11. `CLOUDFLARE_REFERENCE_DEPLOYMENT.md` — optional frictionless deployment profile.
12. `ABUSE_SAFETY_AND_REPORTING.md` — privacy-preserving abuse boundaries.
13. `IMPLEMENTATION_ROADMAP.md` — implementation order and acceptance gates.
14. `ARCHITECTURE_PHASE_PLAN.md` — architecture gates and exit criteria.
15. `AGENTS.md` — rules for coding/research agents.

Existing detailed documents remain canonical where not superseded: identity, event model, group architecture, client/extension architecture, and PWA reference client.

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
