# Notifications and Attention

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** normative R13A design. Call-specific notification UX is completed in R13B.

## 1. Principle

A notification is an **attention representation**, not the message itself. OM's attention budget applies outside the app as strongly as inside it.

> **Activity does not automatically deserve interruption.**

## 2. Attention classes

OM defines platform-neutral semantic classes. Platform adapters map them to system facilities without pretending every OS has identical levels.

### Ambient

No interruptive notification. Examples: routine sync, delivery/read receipt, background indexing.

### Passive

Available in notification history/badge/summary as appropriate; no sound by default. Examples: low-priority group activity configured by the user.

### Normal communication

A direct message or explicitly relevant conversation update under the user's notification policy.

### High-attention communication

User-defined priority contacts/conversations or mentions when configured to interrupt more strongly. This is a preference/policy, not inferred importance.

### Urgent security/account event

Only a genuinely time-sensitive security/account condition. Never use urgency merely to increase open rate.

Calls have their own live-communication class in R13B.

## 3. Foreground suppression

If the user is already viewing the relevant conversation, do not send a redundant system banner for the same incoming message. Insert it into the conversation and update unobtrusive unread/mention state elsewhere as necessary.

## 4. Bundling

Bursty group activity should collapse into conversation-level summaries rather than one interruption per message. Unread count and attention count are separate concepts.

## 5. Lock-screen privacy

OM's privacy-first default should minimize message-content exposure on locked devices. The exact platform behavior is constrained by OS notification settings, but OM should offer clear in-app choices conceptually equivalent to:

- hide content and sender;
- show sender/conversation, hide message content;
- show full preview.

A privacy-sensitive default should not expose full message text before the user has chosen that behavior.

System/user settings remain authoritative where the OS provides stronger restrictions.

## 6. Notification actions

Useful actions may include:

- Reply;
- Mark read;
- Mute conversation (contextual and reversible);
- other safe actions justified by platform conventions.

Avoid destructive/security-critical actions in compact notification surfaces unless the platform supplies sufficient context/authentication and OM's Action Registry explicitly permits them.

Direct reply still routes through canonical send semantics; the notification surface does not implement a second messaging system.

## 7. User control

Notification behavior must be configurable by IdentityProfile/conversation/category where feasible, with a searchable settings model. OM should respect OS-level Focus/Do Not Disturb/notification controls instead of trying to bypass them.

Core OM does not use communication notifications for marketing/growth pressure.

## 8. Badges

Badge counts represent a clearly defined user concept, preferably unread conversations/messages under user preference. They must not become engagement scores or unexplained counts.

## 9. Sensitive content

Do not include private message contents, attachment previews, invite secrets, recovery data, or security-sensitive details in a notification when the current privacy policy says they are hidden.

## 10. Platform mapping notes

- Apple distinguishes communication notifications and interruption levels for other notification classes; OM should use the semantic class that matches the real urgency.
- Android notification channels/importance let users control interruption and lock-screen visibility; OM should design its categories so those controls remain meaningful.
- Platform adapters may expose direct reply or other system actions, but canonical Action Registry policy still applies.

## R13B incoming calls

Incoming calls are high-attention communication events, but platform-native calling systems own much of their interruption behavior on iOS/Android.

OM should:

- use platform call surfaces where required/appropriate;
- expose Answer/Decline/Hang up as bindings to canonical call actions;
- respect platform Do Not Disturb/focus policy;
- avoid duplicating a system call surface with a second competing OM notification;
- disclose caller/group and receiving OM IdentityProfile when multiple accounts make ambiguity possible;
- keep lock-screen/privacy policy consistent with the user's communication privacy settings.

A joinable group call is not automatically equivalent to an incoming ringing call. OM only spends incoming-call-level attention when the architecture/user action actually requests ringing/alerting that participant.
