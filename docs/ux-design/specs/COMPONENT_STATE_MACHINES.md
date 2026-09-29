# Framework-Independent Component State Machines

**Status:** normative implementation boundary, Revision 9.

OM components are semantic state machines before they are React, SwiftUI, Compose, or future-platform widgets. Platform shells may render them differently, but they must preserve the same states, events, Action Registry actions, recovery behavior, and accessibility meaning.

## 1. Global rules

1. **State is explicit.** Do not encode important state only in CSS classes, animation phase, widget visibility, or platform-specific callbacks.
2. **Events are semantic.** Prefer `reply.requested` or `message.retry` over `swipeRight` or `button42Clicked`.
3. **Inputs are adapters.** Touch, pointer, keyboard, voice, accessibility actions, and future HID produce the same semantic events.
4. **Effects are declared.** Network/core work is an effect requested by the state machine; the UI does not infer success before the domain layer confirms it.
5. **Recoverability is explicit.** Undo/retry/cancel states are modeled rather than added ad hoc.
6. **Focus and selection are separate state dimensions.**
7. **Theme and expressive motion cannot change transitions.**

The machine-readable companion is `contracts/om-ui-components-v0.json`.

## 2. Chat row

State dimensions:

- `read_state`: read | unread | mentioned
- `draft_state`: none | draft
- `transport_state`: idle | queued | failed
- `selection_state`: unselected | selected
- `focus_state`: unfocused | focused
- `press_state`: rest | pressed

Semantic events:

- `chat.open`
- `chat.focus`
- `chat.blur`
- `chat.select`
- `chat.mark-read`
- `chat.retry-pending`
- `chat.context-actions`

The row must never require a swipe gesture to expose essential actions. Gesture bindings are accelerators.

## 3. Message object

Primary lifecycle:

```text
local-created
   -> queued
   -> accepted-local-transport
   -> [delivered]
   -> [read]

queued/accepted-local-transport
   -> failed
failed
   -> retrying
   -> queued
```

`delivered` and `read` exist only if OM's protocol/privacy state can prove them. The UI must not manufacture reassuring delivery states.

Orthogonal flags may include:

- edited;
- reply-linked;
- selected;
- focused;
- locally hidden;
- remote-delete pending/complete where supported.

Required semantic events include `message.reply`, `message.react`, `message.retry`, `message.copy`, `message.forward`, `message.edit`, `message.delete-request` and `message.context-actions`.

## 4. Composer

Modes:

```text
idle
  -> composing
  -> attachment-review
  -> reply-composing
  -> voice-note-recording

(any non-recording mode)
  -> omni-suspended
  -> previous composer mode
```

`omni-suspended` preserves draft text, cursor/selection when practical, attachments, and reply target. It does not reinterpret the draft as an Omnibar query.

Send transition:

```text
composing --send.requested--> send-validating
send-validating --valid--> message-created + composing/idle
send-validating --invalid--> composing + validation feedback
```

The UI requests `send-message`; core/domain authority decides whether the request is valid and accepted.

## 5. Omnibar

Modes:

```text
closed
  -> zero-query
  -> querying
  -> result-navigation
  -> resolving-ambiguity
  -> confirming-consequence
  -> executing
  -> closed
```

Key invariants:

- text query and selected candidate are separate values;
- late candidates cannot silently replace the actively selected candidate;
- bare entities retain stable default semantics;
- explicit verbs outrank learned preference;
- risky actions pass through Action Registry confirmation/authentication policy;
- prediction never executes an action by itself.

## 6. Contextual surface

For sheets, menus, popovers, dialogs, and similar transient surfaces:

```text
closed -> opening -> open -> closing -> closed
```

The machine tracks `invoker_ref` so dismissal can restore focus/context. A modal confirmation is a distinct semantic surface and must not be created merely because an operation is slow.

## 7. Tabs / top-level destinations

State:

- enabled destinations ordered by explicit user configuration;
- active destination;
- overflow state derived from available space;
- focused destination for keyboard/switch navigation.

If only one destination is enabled, the visible tab strip is absent. The destination still exists semantically and remains addressable by Omnibar/action paths.

Theme packages cannot add/remove/reorder destinations. OwnMessenger user configuration can.

## 8. Invite/zero-contact state

States:

```text
empty
  -> preparing-share-link
  -> displaying-my-qr
  -> scanning-qr
  -> resolving-invite
  -> accepted | rejected | expired | invalid
```

No state implicitly requests address-book upload. Contact import, if later offered, is an explicit separate action.

## 9. Event envelope

Implementation may use a shape conceptually similar to:

```json
{
  "event": "message.retry",
  "target": {"type": "message", "id": "..."},
  "source": "keyboard",
  "context": {"conversation_id": "..."}
}
```

`source` is useful for telemetry/debugging **only inside the user's privacy policy**. It cannot change the meaning or safety class of the event.

## 10. Conformance

A platform implementation conforms when:

- all normative states/events it supports map to the same Action Registry semantics;
- unsupported platform affordances have an accessible alternative;
- state is recoverable after shell resize/rotation where appropriate;
- accessibility focus/state corresponds to semantic state;
- transition animation can be removed without losing meaning.

## 11. R13A messaging-domain extensions

### 11.1 Attachment child object

States: `staged -> preparing -> queued -> transferring -> available`, with `failed -> retrying` recovery and `unavailable` where domain policy exposes it.

Events include `attachment.add`, `attachment.remove-staged`, `attachment.retry`, `attachment.cancel-transfer`, `attachment.download`, and `attachment.open`.

The attachment remains attached to the same draft/message identity while transport state changes.

### 11.2 Voice note

States: `idle -> requesting-permission -> recording -> review -> sending -> sent`, with `discarded` and `failed` paths.

`voice-note.start` and `omnibar.voice-start` are distinct semantic events. Recording state must have non-waveform text/icon semantics and an accessible Stop/Cancel path.

### 11.3 Draft

Draft identity is `(account, conversation)`. Routine navigation and shell transforms preserve draft contents. `draft.discard` prefers bounded Undo/recovery before modal confirmation when feasible.

### 11.4 Scheduled message

States: `scheduled -> editing/cancelled/release-due -> released-to-send`. `scheduled` is never rendered as `sent`. The domain layer must disclose material execution dependencies for the UI to present truthfully.

### 11.5 Notification representation

Notification state is derived from message/attention policy rather than becoming a second message state machine. Foreground presentation suppresses redundant system interruption when the relevant conversation is already visible.


## R13B — Call session

```text
incoming ──answer──> connecting ──media-usable──> active
   │                    │                              │
 decline                └─failure──> failed           ├─path-loss──> reconnecting
   ↓                                                   │                 │
 ended                                                end            recovered
                                                       ↓                 ↓
                                                     ending <────── active
                                                       ↓
                                                     ended
```

Normative properties:

- incoming video does not activate local camera by invitation alone;
- reconnecting preserves the logical call identity;
- route changes do not change trust semantics;
- `ringing` is exposed only when the domain can actually establish that state;
- end/failure reasons are mapped to user-actionable language rather than raw transport errors.

## R13B — Call participation

Microphone and camera are independent stateful controls:

```text
microphone: unknown → on ↔ muted | blocked
camera:     unknown → on ↔ off    | blocked
```

Permission denial changes capability state; it does not automatically destroy the call if an architecture-supported fallback remains possible.

## R13B — Space navigation

```text
Space list/open
    ↓
Space(channel=current)
    ├─ open-channel → Space(channel=target)
    ├─ open-member-details
    ├─ open-space-settings
    └─ leave-space → confirmation/consequence → outside Space
```

Compact clients may render channel selection as a sheet/drawer. Expanded clients may keep the channel list visible. The semantic state is the same.

## R13B — Security relationship

```text
not-manually-verified
    └─ compare + confirm ──> verified-by-user
                                │
                     cryptographic change
                                ↓
                     verification-changed
                                ↓
                         review-required
                         ├─ reverify ──> verified-by-user
                         └─ accept-new-state ──> not-manually-verified/architecture-defined
```

Automatic protocol checks may add informational substate, but do not skip the user's explicit verification action.

## R13B — Device authorization

```text
recently-added → active → revoke-confirmation → revoked
```

Revocation ends future authorization according to the core architecture. The UI does not transition to a fabricated `remote-data-erased` state unless domain evidence exists.

## R13B — Recovery operation

Recovery state is parameterized by explicit scope:

```text
scope ∈ {identity-authority, history-data, provider-discovery}

idle → preparing → awaiting-required-factors → applying → complete | failed | cancelled
```

The UI must not merge these scopes merely because they share a wizard shell.
