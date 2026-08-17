# OM Master Planning & Design Snapshot

Generated: **2026-08-18 14:03 PHT (UTC+08:00)**

This folder is the consolidated source of truth for the planning and architecture discussion to date.

## Archive rule

Future downloadable archives should be timestamped:

`om-master-planning-design-YYYYMMDD-HHMM-PHT.zip`

Each new archive is a **complete snapshot**, not a delta and not a zip containing older zips.

## Current phase

**Phase 0 — architecture and threat model**

Current architecture gates:

1. Identity — conceptually frozen; crypto/wire details remain.
2. Canonical event model — selected in this snapshot; schema/test-vector work next.
3. Cryptography — 1:1 protocol + MLS implementation details.
4. Relay protocol.
5. Multi-device synchronization.
6. StoragePool / hot-cold backup / restore.
7. Abuse, spam, DDoS.
8. Extension/provider capability contracts.
9. External transport/bridge security.
10. Versioning, conformance, governance.
