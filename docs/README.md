# OM — Master Planning & Design Repository

**Snapshot:** 2026-08-18 14:03 PHT  
**Phase:** Phase 0 — architecture and threat model  
**Working umbrella name:** **OM — OpenMessenger / OwnMessenger / OurMessenger**

This is the consolidated human- and machine-readable source of truth for the OM planning/design discussion to date.

## Product thesis

A fast, uncluttered messenger with Textra/Telegram-like ease of use, strong E2EE, Android/iOS/web clients, and a completely open architecture in which identity, relay, storage, backup, directory, transport, and extensions are replaceable components.

**Normal users should not need to understand the infrastructure.**

## Architecture mantra

> **No infrastructure provider is part of the protocol. Providers are plugins.**

And:

> **Spend complexity only where it buys interoperability, security, reliability, or user freedom.**

## Start here

1. `BLUEPRINT.md` — full product/architecture narrative including the original working blueprint.
2. `SYSTEM_ARCHITECTURE.md` — whole-system component map.
3. `ARCHITECTURE_PHASE_PLAN.md` — ordered architecture gates and exit criteria.
4. `LEAN_ARCHITECTURE_RULES.md` — complexity/resource budget.
5. `IDENTITY_PROTOCOL.md` — Gate-A selected identity model.
6. `EVENT_MODEL.md` — Gate-B selected canonical event model.
7. `PWA_REFERENCE_CLIENT.md` — first execution plan.
8. `GROUP_CHAT_ARCHITECTURE.md` — private groups/MLS direction.
9. `CLIENT_AND_EXTENSION_ARCHITECTURE.md` — official vs third-party clients and extension boundary.
10. `INTRODUCTION_AND_SERVICE_ACCOUNTS.md` — message requests, businesses, bots/human agents.

## Identity companion documents

- `IDENTITY_CONTINUITY.md`
- `IDENTITY_DISCOVERY_VERIFICATION.md`

## Structured records

- `decisions/` — ADR summaries.
- `protocol/` — draft machine-readable logical schemas.
- `machine/decisions.jsonl`
- `machine/requirements.jsonl`
- `machine/open_questions.jsonl`
- `research/RESEARCH_NOTES.md`
- `threat-model/README.md`

## Canonical format

Markdown is canonical for reasoning/rationale. JSON/JSONL is used where structure helps machines and agents. No DOCX is required for ongoing design.

## Execution direction

**PWA first, not web only.**

The first usable reference client should validate OM-native identity, message requests, events, groups, relay/storage portability, and recovery with minimal deployment friction. Android follows early enough to validate native lifecycle/security assumptions and later the default-SMS role.

## What is currently frozen conceptually

- provider independence;
- device-first AccountID;
- aliases are not identity;
- local PersonRecord;
- bounded direct/social continuity evidence; no global trust score;
- message-request first-contact gate;
- organization/service/bot delegation;
- account-level group membership + device-level cryptographic membership;
- MLS direction for private groups;
- hot/cold/recovery storage separation and StoragePool;
- reference app vs open third-party clients;
- PWA-first execution;
- small immutable canonical event model.

## Next architecture work

1. Finish Gate-B encoding/test vectors.
2. Gate C: choose reviewed 1:1 crypto + concrete MLS implementation/cipher suites.
3. Gate D: relay protocol.
4. Gate E: multi-device sync/history transfer.
5. Gate F: StoragePool manifest/backup/restore.
6. Gate G: abuse/spam/DDoS model.
