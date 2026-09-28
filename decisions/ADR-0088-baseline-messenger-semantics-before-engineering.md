# ADR-0088 — Ordinary messenger completeness has explicit semantic boundaries before engineering

## Status
Accepted — 2026-09-23

## Decision
The first-release product baseline is frozen in [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md). Shared pins, polls, moderation redaction and forwarding require explicit interoperable semantics; voice notes remain typed attachments; drafts/search/mute/archive/scheduled-send remain local/account-private; calls remain separate session state.

Product completeness may depend on optional capabilities, but visible convenience features do not receive hidden protocol authority and do not imply a SeedLife-operated service.

## 2026-09-28 ADR-0148 amendment

The V1 baseline now includes visible reference-client multi-profile switching, bounded account-private draft/selected-preference sync, `om.voice-note.v1` presentation and one-shot `om.location-card.v1`. These additions reuse existing local/account-private or optional-extension boundaries and do not add shared authority. Live location and the other explicitly deferred social/service capabilities remain later work.
