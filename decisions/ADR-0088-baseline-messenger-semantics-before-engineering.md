# ADR-0088 — Ordinary messenger completeness has explicit semantic boundaries before engineering

## Status
Accepted — 2026-09-23

## Decision
The first-release product baseline is frozen in `MESSENGER_BASELINE_V1.md`. Shared pins, polls, moderation redaction and forwarding require explicit interoperable semantics; voice notes remain typed attachments; drafts/search/mute/archive/scheduled-send remain local/account-private; calls remain separate session state.

Product completeness may depend on optional capabilities, but visible convenience features do not receive hidden protocol authority and do not imply a SeedLife-operated service.
