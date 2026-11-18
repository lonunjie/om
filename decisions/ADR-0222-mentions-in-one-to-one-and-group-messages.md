# ADR-0222 — Mentions in one-to-one and group messages: typed inline mention nodes plus the mention set

- **Status:** Accepted
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (everyday-chat baseline parity), to fill silence. This ADR fills silence only and amends no audited text.
- **Class:** purely additive and fail-closed. No frozen CDDL, registry, MLS extension or object type changes: `MessageCreate` key 3 and `inline-mention` already exist in [`events.cddl`](../spec/cddl/events.cddl).
- **Amends:** nothing. **Does not amend:** [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md), [`EVENT_MODEL.md`](../docs/architecture/events/EVENT_MODEL.md), ADR-0128 rule 20, ADR-0150, ADR-0198.
- **Scope:** `groups` (ADR-0177) and `reference_client_ui` (ADR-0172); the one-to-one chat engine.

## Context

The baseline lists mentions as core interoperable ("semantic RichText mention nodes; notification indexes may be denormalized but must agree with content"). Core read mention nodes for their label text but never wrote one, a Room refused any create with key 3, and the composer had no way to name a person.

## Decision

1. **Wire form.** A message with mentions is a `message.create` whose RichText holds one `inline-mention` node (type 3) per mention, its inner inlines being the label text exactly as typed (including the `@`), and whose key 3 is the set of mentioned accounts (`principal-ref`, kind account, each once, at most 64, byte order). The plain-text reading of the content is therefore exactly the text the author saw, so an older client shows the same words (ADR-0150).
2. **Agreement.** A receiver reads the mentions from the inline nodes and requires the key 3 set to equal the set of those accounts exactly. A disagreement, an empty or nested mention, a mention inside a link, an unsupported principal kind or more than 64 mentions voids the mentions of that message: the text is shown as written and nothing else changes. A disagreement never refuses the message.
3. **Who can be named.** In a one-to-one chat only the other person (and, on receipt, us). In a Room only a current member of the Room's directory, checked at send (refused, nothing sent) and at receipt (a non-member voids the mentions, text kept). A mention carries the AccountID, as ADR-0128 rule 20 states for forwarding; the forward path already turns mentions into plain text.
4. **Edit.** An edit carries plain replacement text and clears the message's mentions in this build (the baseline allows editing the mention set; building it is later work, and clearing is the safe reading: a stale range over replaced text must never highlight the wrong words).
5. **Attachments.** In a one-to-one chat a message with attachments goes out without mentions in this build (its caption is plain). A Room message with attachments is not part of ADR-0198.
6. **Notification safety.** Core exposes, per message, the ranges and whether one names us (`me`). A notification for a mention shows no more than any message notification would under the person's own notification preview setting; the mention flag only lets the client choose to raise the priority of a notification it would show anyway. No server or relay sees a mention: it travels inside the sealed event like any text.
7. **Presentation.** The composer offers a picker of people who can be named, with the same same-name and lookalike cues the people lists use (ADR-0172); the timeline shows each mention as a distinct, non-link span with an accessible name, and a stronger cue when it names us. Text, not colour alone, carries the meaning.
8. **Bounds.** Text bounds are those of the message (one-to-one: `MAX_MESSAGE_CHARS`; Room: 8192 bytes); at most 64 mentions.

## Where did authority go?

Nowhere new. A mention grants nobody anything: it names a member, it does not add, invite or notify anyone outside the conversation.

## Evidence and what is not covered

Core unit tests (round trip, set disagreement, bounds, W0 registry check), a one-to-one integration test (delivery, `me` flag, refusal of non-peers and bad ranges, disagreeing set shown plain), a three-account Room test (all devices see the mention, non-members and bad ranges refused). Not built: mention edit, mentions on attachment captions, notification priority wiring (the platform notification layer is not part of this build).
