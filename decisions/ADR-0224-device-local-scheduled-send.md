# ADR-0224 — Device-local scheduled send in the reference client

- **Status:** Accepted
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (everyday-chat baseline parity), to fill silence. This ADR fills silence only and amends no audited text.
- **Class:** client-only (`reference_client_ui`, ADR-0172). No wire object, registry, CDDL, Core intent or Home API changes: a due item is released through the ordinary `SendMessage` intent.
- **Amends:** nothing. **Does not amend:** [`MESSENGER_BASELINE_V1.md`](../docs/architecture/events/MESSENGER_BASELINE_V1.md) "Scheduled messages", [`DRAFTS_SCHEDULED_AND_CONTINUITY.md`](../docs/ux-design/specs/DRAFTS_SCHEDULED_AND_CONTINUITY.md) sections 5 to 7, ADR-0156 item on scheduled send, ADR-0157 item on late binding.
- **Scope:** `reference_client_ui`.

## Context

The baseline classes scheduled send as local UX: an encrypted, device-local, profile-bound queue; the item becomes an ordinary new message at the actual send time; an overdue item goes out at the next opportunity marked "sent late" to the sender; an item that loses eligibility is discarded with a local notice and never migrates. Server-side scheduled delivery is a later service capability and is not built (it would disclose content or timing to a scheduler). The reference client had no way to schedule.

## Decision

1. **The queue is compose intent only** (ADR-0157): `{ id, conversation, text, due time, created time }`. Nothing is encrypted for a recipient, signed, addressed or deposited before the moment of sending; at that moment the client resolves the relationship, devices, epoch and expiry as for any message. Mentions and attachments are not part of a scheduled item in this build (the text goes out as plain text; the picker is for messages sent now).
2. **Storage.** The queue is one record in the identity's sealed private store under a key Core ignores (the store that holds drafts), written at once when it changes so closing the page does not lose it, and read back through a validator that drops malformed or oversized entries. It is per identity and per device, is not part of any sync, backup bundle or export, and is forgotten from memory on lock (it stays sealed on disk).
3. **Release.** Due items are released in due order through the ordinary send path: on a timer while OM is open, when the page becomes visible and when the network returns. A message goes out stamped with the time it is actually sent, never the scheduled time. One that goes out more than two minutes after its time is shown to its sender as sent late. An item whose send is refused for good (the chat is gone, blocked or cannot receive) is dropped with a notice that names the text and says the chat can no longer receive it. An item whose send needs the person (`NeedsUserAction`) stays queued and is tried again at the next opportunity.
4. **Truthful copy.** The scheduling form says that the message is sent from this device and goes out at that time only while OM is open on it; if OM is closed then it goes out the next time OM is opened and the owner is told it went out late; other people just see a normal message. No exact-time promise and no claim that another device or a server will send it.
5. **Discoverability and control.** There is no permanent Scheduled tab: the chat shows its own scheduled messages (and any late or dropped notice) above the composer only while there is something to show. Each item can be cancelled or edited (edit returns the text to the draft and removes the item); once released, ordinary edit and delete apply.
6. **Bounds.** At most 20 queued items, 8000 characters each, from 30 seconds to 366 days ahead, by the device's clock (the clock is only the person's own timer, never an authority for the receiver: the receiver sees the real send time).

## Where did authority go?

Nowhere new. The client sends nothing it was not asked to send, at a time the person chose, with the person's current authorization at that time; no relay, Home or other device learns of it before it is sent.

## Evidence and what is not covered

Unit tests for release order, on-time, late and dropped items, the form's copy and accessibility, listing, edit and cancel, the sealed-store round trip and the validator; a Chromium run that schedules a message and sees it arrive. Not built: scheduling with attachments or mentions, a "scheduled" marker on the conversation list, scheduling from the Omnibar, cross-device queues (excluded by the baseline).
