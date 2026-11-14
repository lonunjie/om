# Groups UI: open questions (2026-11-14)

Written while building the groups UI ([status](2026-11-14-groups-ui-status.md)). Each item is something the design is silent on. The UI
does the conservative thing described and nothing more; none of these were decided by inventing product.

## Entry points and navigation

- **Q-UI1 Entry points.** Where does "New group" live? Built: a "New group" button beside "New conversation" in the Chats header and an
  Omnibar action. Both exist only when the host reports groups available. Groups appear in the same Chats list as 1:1 chats, with a small
  people marker and the accessible name "Group, N people", and open at `/chats#<id>` (no new place in `place.ts`). Confirm or redirect.
- **Q-UI-search.** Group messages are not in local search yet (the search index reads 1:1 conversations). Omnibar offers group names only.
  Should group message search ship in V1, and under which ProfileExecutionContext rules beyond UI-10?
- **Q-UI-icon.** The "users" icon (two people) is new in the icon set. No avatar or photo for groups: the first letter of the name is used,
  like a chat. Confirm.

## What the views do not carry

- **Q-UI2 Names and "this is you".** `GroupDetailView.people` names people only by account (hex). The UI shows a name only when an
  invitation ties that account to a chat, and "You" only after the person has sent a message; everyone else is "Someone 2", "Someone 3".
  Core should return display names (from the relationship) and a "this is you" flag.
- **Q-UI3 Capability flags.** The views carry no "may add", "may remove" or "is admin" flag beyond `roles`. The UI offers Remove and Add
  to everyone and Core refuses (`not_authorized`) in plain words otherwise. Core should say what the person
  may do so the control is absent rather than refused.
- **Q-UI-feed.** `GroupApi` has no change feed. The UI re-reads after every step, on tab visibility and every 2.5 s while a group screen
  is shown. A subscribe would remove the poll (deviation D31 in the status report).
- **Q-UI-events.** No persistent system events ("Bob left", "Ana was added"). The roster shows the current state only; the chat log has no
  join or leave lines. Does the design want them in the log, and what is their wording?
- **Q-UI-name.** The name is optional in the form, but Core requires one. When it is left empty the UI names the group after the chosen people ("Ana and Ben"). Core still sees a name.
  Confirm that deriving a name is acceptable.
- **Q-UI-cap.** The 32-member cap (G7): the Action Registry refuses more than 31 chosen people (plus you), and the form then shows a generic "did not work" line. Confirm the number and whether the form should say so before the limit is reached.

## Confirmation of leave and remove

- **Q-UI-confirm.** Leave and remove ask "Leave X?" / "Remove Y from X?" in an in-place panel (safe action first, destructive action in the
  danger style, like the message-request pattern). Core's `IssueConfirmation` nonce covers only `CoreIntent`s, and the group intents run on
  `client.groups.do`. So these are UI confirmations, not Core-issued tokens (UI-11). Reading the design (commit review): this is not a departure. Accessibility "Dialogs/sheets" and
  the reconciled model ask for a subject-naming confirmation, and UI-11's nonce list does not include group leave or remove. Decide only whether
  they should be added to that list, which needs a Core change.
- **Q-UI-map.** [`ACTION_TO_CORE_INTENT_MAP.md`](../architecture/ux/ACTION_TO_CORE_INTENT_MAP.md) has no rows for the group actions (`group-create`, `group-invite`, `group-accept`,
  `group-decline`, `group-leave`, `group-remove-member`, `group-send-text`) or for leave. They were added to the Action Registry as a new
  result kind `group` (direct origin only: a suggestion is refused with `suggestion_cannot_confirm`). The map needs the rows (recorded as deviation D30 in the status report).

## Still open upstream

- **Q1, Q2, G9** from the groups plan remain open, and there is no host driver that carries group wires. The UI therefore runs only
  against the loopback harness (`createGroupSim`). The shipped `om_wasm.wasm` has no engine, so `groups.available()` is false and no
  group control is shown in the real app.
