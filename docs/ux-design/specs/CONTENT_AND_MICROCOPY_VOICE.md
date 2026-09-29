# Content and Microcopy Voice

**Status:** Normative content-design contract

## OM voice

OM is:

- calm;
- direct;
- respectful;
- concrete;
- concise;
- technically honest;
- action-oriented when recovery is possible.

OhMessenger may add warmth and playfulness to low-risk celebratory moments. It does **not** make errors, security warnings, permissions, recovery, or destructive actions cute.

## Core writing pattern

When something needs explanation, prefer:

> **Object/state → what it means → safe next action**

Examples:

```text
Waiting to send
We'll try again when a route is available.
```

```text
This message wasn't sent
Retry
```

```text
Verification changed
Maria's security identity is different from the one you verified.
Review
```

Avoid:

```text
Error 503
Peer transport exception
Oops! Something went wrong 😅
```

## Rules

1. **Name the actual thing.** Say `message`, `device`, `history`, `provider`, or `verification` rather than generic `account` when the scope is narrower.
2. **State facts, not blame.** Avoid `you forgot`, `invalid user`, or language that assigns fault unnecessarily.
3. **Prefer verbs for actions.** `Retry`, `Review`, `Revoke device`, `Keep draft`, `Open settings`.
4. **Do not hide serious consequences behind euphemism.** Destructive/security actions are brief but explicit.
5. **Do not expose implementation jargon by default.** ICE, TURN, CBOR, ratchets, provider pools, queue IDs, and protocol diagnostics belong behind Details/diagnostics unless the user explicitly asks.
6. **Do not over-apologize.** A useful recovery action is more valuable than `Sorry`.
7. **Avoid unnecessary humor/colloquialisms.** They age badly and localize poorly.
8. **Use sentence case for ordinary UI copy.** Preserve proper names and platform conventions.
9. **Accessible names and visible labels describe the same action.** Do not call a visible button `Review` while its accessibility label says `Confirm identity` unless that distinction is intentional and accurate.

## Empty states

Empty states explain the immediate next possibility rather than marketing OM.

Preferred:

```text
No conversations yet
Invite someone to chat
Scan QR code
Show my QR
```

Avoid multi-slide feature tours or pressure to upload contacts.

## Errors

An error message should answer, when useful:

- what failed;
- what object is affected;
- whether the user's work is safe;
- whether OM will retry;
- what the user can do next.

If no action is needed, avoid an interruptive alert.

## Permissions

Ask at the moment of need and explain the immediate benefit:

```text
Use your microphone for this call?
```

not:

```text
OM needs microphone permission for the best experience.
```

The OS permission sheet remains authoritative.

## Security terminology

Reserved phrases:

- `Verified by you` — manual/user-driven verification only;
- `Encryption check passed` — automated protocol check;
- `Verification changed` — previously user-verified state differs;
- `Authorized device` — device authority, not person identity;
- `Recover identity authority`, `Restore history/data`, `Reconnect provider/discovery` — scope-specific recovery.

Do not use a generic `Secure` badge as a catch-all status.

## Canonical terminology

The design system maintains one canonical term for each concept. Aliases may be accepted by search/voice, but visible product wording stays consistent. Examples:

- Conversation, not alternating between Chat/Thread/Room for the same object.
- Group for ordinary multiperson conversation.
- Space for optional organized multi-channel context.
- Channel only inside a Space.
- Omnibar for the product concept; localized UI may use a clearer translated term if direct transliteration is poor.
