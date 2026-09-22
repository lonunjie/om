# OM Architecture Snapshot

Generated: **2026-09-23 07:18 PHT (UTC+08:00)**

## State

OM remains in **architecture-hardening / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`.

The canonical architecture now has two recently completed design clusters:

- 06:15 — messenger baseline + Spaces/Rooms/Communities/permissions/Topics/Publications;
- 07:18 — expressive media + portable packs + entitlement boundary + typed service principals/bots/LLMs + capability-sandboxed plugins.

The 07:18 pass also removes the human-only author assumption from `ConversationEvent`: canonical authorship is now a typed `PrincipalRef` with a separately authenticated endpoint. Account endpoints remain DeviceIdentities; service endpoints remain governed by ServiceControlState and never join AccountControlState majority.

## Remaining blocking design queue

- A-011 external interop bridges
- A-012 import/export and user escape
- A-013 metadata threat model
- A-014 decentralized abuse/moderation
- A-015 20-year protocol longevity / anti-capture

Resolved design does not imply verified implementation feasibility. Architecture verification gates remain blocking, including new plugin-sandbox, service-principal/automation and expressive-media/pack parser-safety gates.

## Next architecture cluster

The most coupled next pass is **A-011 + A-012**: external bridging and import/export/escape both cross trust boundaries and require explicit provenance, identity mapping, fidelity-loss and “OM E2EE ends here” semantics. After that, A-013/A-014 should adversarially attack metadata and abuse surfaces before A-015 performs the final 20-year continuity/anti-capture pass.
