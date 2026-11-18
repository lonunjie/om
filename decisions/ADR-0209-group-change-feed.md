# ADR-0209 — A change feed for group screens

## Design sources read

[`UI_CORE_PLATFORM_CONTRACT.md`](../docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md) ("Rule": Core produces "view state / event stream", the UI renders it; all),
[`PWA_REFERENCE_CLIENT.md`](../docs/architecture/client/PWA_REFERENCE_CLIENT.md) (the line "no polling loops when a push/wake path exists"), [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../docs/architecture/client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md)
(all), ADR-0189 section 2 (settle) and the relay-status deviation D31. The other mapped documents were not read in full.

- **Status:** Accepted by delegation, 2026-11-18.
- **Scope:** fills a silence only. The contract says the UI renders view state and an event stream from Core but
  does not say how a screen learns that a view changed. Group screens used a 2.5 second re-read (D31) because the
  group path had no such signal.

## Decision

1. The group API gains an optional **change feed**: `onChange(listener)` calls the listener with a `ProfileId`
   whenever a state change of that profile became durable and was released by the host runtime's commit step.
   It carries no content, no group id and no event; the screen re-reads the views it shows.
2. The feed is the host runtime's own **commit stream**, so it covers every moment a group view, the invitation
   lists, the rejoin view or the held invitations can read differently: every group step, every received frame,
   every settle, and also changes that arrive through the one-to-one path (a held invitation, a contact's name).
   Reads never report. A profile's feed never reports another profile (UI-06).
3. Screens subscribe for as long as they are shown, re-read on each report and when the tab becomes visible, and
   do not run a timer. Only a host that offers no feed falls back to the previous calm interval.
4. **No new relay-visible signal.** The feed is local and in-process. It neither causes nor reveals any network
   request, and its timing is that of commits that already happen. Group views have no time-based content, so
   nothing that the interval refreshed is lost.
5. The one-to-one chat screens already used `client.subscribe`; they are unchanged. Group search (ADR-0208)
   uses the same feed.

## Consequences

D31 is retired. A change made on this device is shown in the same turn instead of within 2.5 seconds, and an
idle group screen performs no periodic work.
