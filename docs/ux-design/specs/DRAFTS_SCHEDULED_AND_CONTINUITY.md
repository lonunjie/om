# Drafts, Scheduled Messages, and Continuity

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** normative R13A design.

## 1. Drafts are durable local user work

A draft is associated with **IdentityProfile + conversation**, not merely the currently mounted composer widget.

Draft content may include:

- text and text selection/cursor metadata where practical;
- reply target;
- staged attachments;
- recorded-but-unsent voice note;
- other compose metadata explicitly supported by OM.

Ordinary navigation, temporary Omnibar use, layout changes, accidental back navigation, and routine app reload/restart should not destroy a draft where local persistence is available.

## 2. Autosave

Draft persistence should be incremental and local-first. The UI does not need to show `Saving…` for routine local autosave unless persistence is actually failing.

A draft-save failure is attached to the composer/draft state, not shown as a generic network error.

## 3. Draft synchronization

Local draft durability is mandatory. The canonical V1 architecture also permits account-private E2EE draft synchronization among eligible linked devices in the same IdentityProfile. Conflicting unsent drafts are preserved/resolved rather than silently discarded by a last-timestamp winner. Draft sync is convenience state, not conversation authority, and cross-implementation draft-sync interoperability is not a V1 promise. Infrastructure must not receive draft plaintext.

## 4. Discard

Prefer recoverability over repetitive confirmation. If a discarded draft can be recovered for a bounded local period, use `Draft discarded · Undo` rather than a modal.

If irreversible destruction would remove valuable recorded media or staged work and no recovery exists, explicit confirmation is appropriate.

## 5. Scheduled messages are a separate state

Scheduling is an explicit verb. A scheduled message is neither `sent` nor `queued`.

```text
draft -> schedule requested -> scheduled
scheduled -> edit -> scheduled
scheduled -> cancel -> draft/removed according to user choice
scheduled -> release due -> normal send state machine
```

The conversation should make scheduled content discoverable without requiring a permanent top-level Scheduled tab. A contextual indicator or Omnibar action such as `Scheduled messages` is sufficient.

## 6. Execution dependency must be truthful

The UX must expose any dependency that materially affects delivery. Examples:

- if the scheduling device must be online, say so;
- if another trusted device/provider can execute, represent that accurately;
- if the architecture cannot guarantee exact-time execution, do not promise exact-time delivery.

The UI never hides architectural limitations behind a calendar icon.

## 7. Editing/canceling scheduled content

Before release, scheduled content should normally be editable/cancelable. Once released into the send state machine, ordinary message-edit/delete semantics apply instead.

## 8. Multi-device continuity

When a conversation opens on another eligible linked device, OM may show architecture-synchronized account-private draft state and must preserve unresolved concurrent draft heads. If draft sync is unavailable/ineligible, it must not pretend a device-local draft exists there. Device-specific state can be indicated calmly only when relevant.
