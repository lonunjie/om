# ADR-UIUX-0015 — Messaging Completeness and Object-Preserving Transport UX

**Status:** Accepted for the UI/UX design track in R13A.

## Context

Earlier OM UI/UX passes established the shell, Omnibar, message pending/failed states, and media placeholders, but did not close the complete messaging interaction domain: attachment staging/transfer, voice-note recording/review, durable drafts, scheduled messages, offline/retry semantics, or notification attention policy.

A messenger that is visually coherent but loses drafts, hides transfer state, duplicates retries, or over-notifies would violate OM's natural/calm/trustworthy principles.

## Decision

1. Composing is local-first and independent from network availability.
2. Draft/message/attachment/voice-note/scheduled-message are distinct semantic objects.
3. Message/media identity remains stable across queued/transferring/retrying states.
4. Pending/queued is neutral; failure is explicit only when user attention/action is warranted.
5. Retry targets the same logical object; visual duplicate creation is forbidden.
6. Voice notes default to record -> review -> explicit send; hold-to-record may be an accelerator but not the only accessible path.
7. Drafts are durable local user work; cross-device draft sync is not implied.
8. Scheduled messages are not labeled sent and must reveal material execution dependencies.
9. Notification policy models attention classes and privacy separately from unread/activity counts.
10. PWA correctness cannot depend on limited-availability Background Sync/Background Fetch APIs.

## Consequences

- `om-ui` needs explicit draft, attachment, voice-note, scheduled, and notification policy states.
- Core/domain interfaces must report enough state to avoid fabricated delivery receipts or duplicate retry UI.
- Platform notification adapters map OM's semantic classes to OS-specific notification systems.
- R13B may extend the notification model for live calls but must preserve the same attention-budget principles.

## Deferred

- actual scheduled-message execution ownership;
- attachment transport protocol;
- media codecs/encryption;
- transcript engine/provider choices;
- disappearing/remote-delete security semantics;
- call notifications and live communication UX.
