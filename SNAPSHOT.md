# OM Master Planning & Design Snapshot

Generated: **2026-09-21 08:01 PHT (UTC+08:00)**

This snapshot supersedes the 2026-08-18 root architecture while preserving it under `history/20260818/`.

## Phase

Architecture consolidation is mature enough to begin staged implementation, but security-sensitive gates must still be completed with test vectors, review, and conformance work before production claims.

## Major changes since 2026-08-18

1. Added explicit **scoped / replaceable / non-composable authority** model.
2. Made AccountID algorithm-independent and non-routable at the network layer.
3. Added versioned **CryptoProfile** architecture with invisible background migration and downgrade floors.
4. Made PQ/hybrid security the preferred direction while keeping draft-dependent pieces replaceable.
5. Added capability-routed relay architecture and relay metadata-minimization rules.
6. Added key-transparency/witness direction without making one directory the root identity authority.
7. Added full voice/video design: encrypted OM signaling, SFrame E2EE, adaptive LAN/P2P/TURN/SFU routing, group-call SFU strategy, and cost/privacy policies.
8. Added local-first/LAN resilience direction.
9. Clarified Cloudflare as an optional frictionless reference deployment only.
10. Clarified R2 as one possible encrypted blob store; relay/offline queues remain separate.
11. Deferred SMS/MMS/RCS from first product while preserving transport adapter sockets.
12. Reordered roadmap to OM-native messaging → daily-driver groups/sync/recovery → calls → infra freedom → carrier transports.
13. Added explicit cost/budget guardrails for provider adapters.
14. Added stronger abuse/reporting model that does not require global plaintext access.

## Current architecture gates

A. Identity and continuity — selected; encoding/recovery details remain.  
B. Canonical events — selected; encoding/test vectors remain.  
C. CryptoProfile v1 — select reviewed 1:1, identity authentication, and MLS implementation details.  
D. Relay protocol — capability queues, rotation, acknowledgements, expiry, abuse limits.  
E. Multi-device sync/history transfer — finalize private device sync and migration.  
F. StoragePool — manifest, encrypted blob lifecycle, restore, provider migration.  
G. Abuse/safety — message requests, relay enforcement, reporting evidence, rate limits.  
H. Calls/media — signaling, SFrame keying, route policy, TURN/SFU adapters.  
I. Provider contracts — relay/storage/media/directory/push cost/capability semantics.  
J. External transports — SMS/MMS/RCS/bridges after OM-native stability.  
K. Versioning/conformance/governance — downgrade policy, profile registry, test suites, release transparency.
