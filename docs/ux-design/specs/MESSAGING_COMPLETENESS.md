# OM Messaging Completeness — R13A

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** normative messaging-domain design checkpoint. Calls, Spaces/groups, and deep security/recovery UX remain outside this run.

## 1. Goal

OM messaging must feel local, direct, and trustworthy even when the network is slow, absent, partially available, or recovering. The user manipulates **messages and media objects**, not transport machinery.

The messaging domain therefore follows these rules:

1. **Compose locally first.** Text entry, attachment staging, voice recording, draft persistence, and local search do not wait for a network round trip.
2. **Object identity survives transport.** A message or attachment remains visibly the same object while queued, uploading, retrying, downloading, or failed.
3. **Pending is normal, not alarming.** Queueing and temporary unavailability use calm neutral state. Warning/danger appears only when the user needs to notice or act.
4. **Failure is local and actionable.** Put recovery on the affected object; do not convert ordinary transport trouble into global modals.
5. **Retry is idempotent at the UX contract.** Retrying a failed object requests delivery of that same logical message/attachment rather than visually creating a duplicate.
6. **Drafts are user property.** Navigation, Omnibar use, resize, ordinary reload/restart, and temporary offline state must not casually destroy composed work.
7. **Send semantics stay explicit.** A draft, scheduled message, queued message, locally accepted message, delivered message, and read message are different states.
8. **Media permissions are just-in-time.** Camera/microphone access is requested only when the user invokes a capability that needs it.
9. **Notifications reflect attention, not activity volume.** Multiple events may collapse into one attention event; foreground UI does not notify itself redundantly.
10. **Privacy defaults beat convenience.** Lock-screen disclosure, remote media enrichment, transcription, and behavioral learning remain bounded by explicit privacy policy/preferences.

## 2. Canonical messaging objects

The UI treats these as distinct semantic objects:

- **Draft** — unsent local composition associated with IdentityProfile + conversation.
- **Staged attachment** — local media/file selected for a draft, not yet committed as a sent message attachment.
- **Message** — logical sent/queued communication object with stable local identity.
- **Attachment** — child object of a draft/message with its own transfer/availability state.
- **Voice note** — audio attachment with recording/review/playback semantics.
- **Scheduled message** — composition intentionally deferred to a future condition/time; not equivalent to sent/queued.
- **Notification event** — an attention representation of message/call/security state, not the source of truth for that state.

## 3. Messaging state hierarchy

```text
Draft
  ├─ composing
  ├─ attachment-review
  ├─ voice-recording/review
  └─ scheduled (after explicit scheduling)

Message
  ├─ queued
  ├─ accepted-local-transport
  ├─ delivered (only if protocol proves it)
  ├─ read (only if policy/protocol exposes it)
  └─ failed -> retrying -> queued

Attachment
  ├─ staged
  ├─ preparing
  ├─ queued
  ├─ transferring
  ├─ available
  ├─ failed -> retrying
  └─ unavailable/expired where architecture exposes those states
```

The UI must not collapse these into a generic spinner or a single boolean `sent`.

## 4. Fast-path principle

A user action that can be safely accepted locally should receive immediate local feedback. Network/domain confirmation updates the object afterward. The interface never blocks typing because a previous message is still moving through transport.

## 5. Scope boundaries

R13A defines user-facing semantics, not protocol choices. It does **not** decide:

- whether attachments are uploaded before/after a message envelope;
- which device/provider owns scheduled-message execution;
- protocol receipt semantics;
- media encryption/container/codec format;
- remote deletion/disappearing-message policy;
- detailed device recovery/security flows.

Those architecture decisions must map into the states defined here without inventing misleading UI states.
