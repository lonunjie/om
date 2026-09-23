# ADR-0107 — Post-quantum status is per conversation type; groups migrate on objective triggers

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** `HARDENING_REVIEW_20260924.md`

## Decision

OM-Crypto-v1 1:1 conversations are hybrid-PQ. v1 MLS groups and group calls are classically confidential. No surface may claim PQ confidentiality for them (R-173). The v1 group wire must carry a tested, authenticated, downgrade-safe ReInit path to a hybrid-PQ MLS profile that preserves ConversationID and group authority (R-174, V-019). A hybrid-PQ group profile becomes default for new groups, and migrates existing ones automatically, once all of these hold: a published IETF suite/combiner definition with codepoints, ≥2 interoperable implementations, OM bandwidth/commit-size review and a new immutable CryptoProfile entry. This corrects the SECURITY_PRIVACY_PROFILES wording, which implied PQ-hybrid for all conversation types.
