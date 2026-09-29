# Offline, Failure, and Recovery UX

**Status:** normative R13A design.

## 1. Connectivity is not a binary UI mode

OM can communicate through multiple routes and providers, so `online/offline` is often too coarse. Prefer capability/object truth:

- this message is queued;
- this attachment is retrying;
- this call route is unavailable (R13B);
- provider discovery is unavailable;
- local history remains available.

A small global `Connection limited` state is appropriate only when many relevant operations are affected.

## 2. Queue first, alarm later

Safe outgoing messages accepted locally appear immediately in the conversation with calm queued/pending state.

Transient transport failures may retry automatically according to core policy without repeatedly escalating the UI. Escalate to an explicit failure when:

- automatic retry is exhausted/blocked;
- user action is required;
- the failure persists long enough to matter;
- policy/security requires attention.

## 3. Per-object recovery

Place Retry/Cancel/Details on the affected message or attachment. Avoid a generic modal that forces the user to remember which object failed.

Examples:

`Queued · Waiting for connection`

`Failed to send · Retry`

`Photo failed to upload · Retry`

## 4. Retry identity

`Retry` targets the same logical object. The UI must not visually create a second message merely because a delivery attempt changed. Core/domain implementations should support idempotent retry semantics or return enough state for the UI to avoid duplicates.

## 5. Cancel queued send

If domain state still permits cancellation, the user may cancel a queued send. The UI must distinguish:

- `Cancel sending` — prevent a not-yet-committed operation where possible;
- `Delete`/`Delete for me` — remove local presentation;
- remote delete/retract semantics — separate architecture-defined action.

Do not label all three `Delete`.

## 6. App restart / PWA lifecycle

The user experience cannot depend on experimental browser background APIs for correctness. Queued state must persist locally enough to resume/reconcile when OM next has an execution opportunity.

Background Sync/Background Fetch may improve delivery on supporting browsers, but the PWA must remain correct when those capabilities are unavailable.

## 7. Stale state reconciliation

After reconnect/restart, the domain layer reconciles local objects with authoritative state. UI rules:

- preserve stable message identity;
- avoid duplicate insertion;
- update state in place;
- surface conflicts only when the user must decide;
- never fabricate delivered/read receipts during reconciliation.

## 8. Foreground feedback

When OM is already showing the affected conversation, prefer inserting/updating the actual message state rather than issuing a redundant system notification.

## 9. Error language

Use concrete, recoverable language:

- `Waiting for connection`
- `Couldn't send yet · Retry`
- `File is no longer available on this device`

Avoid transport jargon unless the user opens technical details.
