# ADR-0099 — Moderation is scoped; shared safety advisories are not authority

- **Status:** Accepted
- **Date:** 2026-09-23

## Decision

OM uses layered user/relationship/Room/Space/provider moderation with explicit capability scope. Reports are selective disclosures and provider denial affects only provider custody; neither creates universal identity revocation.

Optional signed `SafetyAdvisory` sources may share spam/fraud/malware/moderation signals. Subscribers choose local enforcement. OM defines no mandatory labeler, global reputation score, master blocklist or universal moderator. Non-public principals should use labeler-scoped pseudonymous moderation references where practical to avoid turning safety feeds into a cross-provider identity graph.
