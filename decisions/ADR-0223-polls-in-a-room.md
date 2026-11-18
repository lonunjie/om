# ADR-0223 — Polls in a Room: create, vote, change a vote, close, as typed W1 events with a convergent tally

- **Status:** Accepted
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (everyday-chat baseline parity), to fill silence. This ADR fills silence only and amends no audited text.
- **Class:** purely additive and fail-closed. No frozen CDDL, registry, MLS extension, object type or `GroupTransportFrame` field changes: kinds 8, 9 and 10 and their bodies (`PollCreate`, `PollVote`, `PollClose`) are already typed by [`group.cddl`](../spec/cddl/group.cddl) and the W1 overlay (ADR-0176); the capabilities (`room.post`, `room.react`, `room.redact_others`) are already in [`om-room-capabilities.json`](../spec/registries/om-room-capabilities.json).
- **Amends:** nothing. **Does not amend:** [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md) "Poll semantics", [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md) "Polls", ADR-0121 rules 3, 7 and 17, ADR-0176, ADR-0198, ADR-0222.
- **Scope:** `groups` (ADR-0177) and `reference_client_ui` (ADR-0172).

## Context

The baseline includes polls in the V1 product (create, vote, close; no claim of anonymity). The bodies, the ordering rule and the capability mapping are written; the Room carried only kinds 1 to 4 (ADR-0198), and no screen made or showed a poll.

## Decision

1. **Carriage.** A poll event is a canonical `ConversationEvent` of kind 8, 9 or 10 inside the Room's MLS application plaintext, as in ADR-0198 (same header builder, same sender authentication from MLS and the Room directory, same author-mismatch refusal). Its body is the typed `PollCreate`, `PollVote` or `PollClose`. A poll is identified by its `poll.create` EventID. A Room-only feature: a one-to-one chat has no poll in this build (the baseline lists polls under group-room behaviour; the 1:1 `ConversationEvent` lane stays kinds 1 to 5).
2. **Create.** Needs `room.post`. The question is plain RichText (one paragraph, 1 to 256 characters of text, no mention or link nodes in this build), the options are 2 to 12 (the UI bound; the wire allows 32) distinct non-empty labels of at most 80 characters, option ids are 0 to n-1 in order, the mode is single or multiple, a multiple poll has `max_selections` between 2 and the option count or none (none means any), and results visibility is `immediate`, `after_vote` or `after_close`. A body that does not parse, or breaks a bound, is dropped (`unsupported_event`), never half-applied.
3. **Vote.** Needs `room.react`. A vote is a register per (member account, poll) ordered by `(sending epoch, EventClock)`; the latest wins and an empty selection retracts. The selection must be known option ids, distinct, one id for a single poll, at most `max_selections` for a multiple one; otherwise dropped (`unsupported_event`). A vote for a poll that is not in the Room (not arrived, never seen) is accepted with no effect, as reactions are (ADR-0198 item 6; ADR-0121 rule 6 would buffer it: the same recorded departure D143).
4. **Close.** The poll's author, or a member holding `room.redact_others`, may close it; anyone else's close is dropped (`not_author`). Close is monotonic and ordered: a vote whose `(sending epoch, EventClock)` is after the close's is ignored, whatever its arrival time; a vote ordered before the close counts even if it arrives later. Backdating within one epoch is the documented residual risk of ADR-0128 rule 17; wall-clock time is never authority. When a vote and the close are held together the tally is recomputed from the registers, so replay, reorder and repetition converge.
5. **Tally and visibility.** Core derives the tally from the registers: per option the number of members whose current selection holds it, the number of voters, whether the poll is closed, and which option ids the person using the device chose. `results_visibility` is a display rule for the UI built on that: `immediate` shows counts at once; `after_vote` shows counts once this person has a vote on record; `after_close` shows counts only once closed. It is not a secrecy guarantee: every member's device can decrypt every vote (the baseline says V1 does not claim anonymous voting), and the screen says so in plain words when it shows who voted what. This build shows counts only and who has voted per option is not listed.
6. **Bounds.** At most 64 polls per Room are kept open for tallying beyond which a new create is refused locally (`too_many_polls`); at most 4096 vote registers per Room; a member has at most one register per poll. Counts are of members in the Room directory at the time the tally is read (a removed member's vote is no longer counted, as their reactions are not).
7. **Not built.** Poll edit, deletion by the author (a `message.delete` of the create tombstones the poll and clears its tally), pinning, expiry timers, polls in attachments or threads, listing voters by name.

## Where did authority go?

Nowhere new. Who may create, vote and close is decided by the Room's own capabilities and authorship (ADR-0121 rule 7); who a vote is from is what MLS and the directory say. No service, relay or Home learns anything it did not (one sealed frame, as before).

## Evidence and what is not covered

Core tests in a three-account room (create, single and multiple votes, change and retract, close by author and by a moderator, a stranger's close dropped, vote after close ignored by order, a vote that arrives late but is ordered before the close counts, replay and reorder converge, bounds, spoofed author). A UI test per path (create form, voting, closing, hidden results before a vote, accessible names), and a Chromium run of three people. No new vector: the bodies are frozen typed W1 bodies.
