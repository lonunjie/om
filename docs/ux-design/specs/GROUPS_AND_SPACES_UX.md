# Groups and Spaces UX

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative R13B design specification

## 1. Keep ordinary groups ordinary

An OM group is first and foremost a conversation with multiple participants.

It should feel like a direct chat with the minimum additional structure needed for:

- participant awareness;
- membership changes;
- group-specific notification policy;
- shared media/files;
- group calls;
- membership/management actions authorized by the underlying capability model.

A user who only needs family/friends/work group chats should never have to learn a Discord-like hierarchy.

## 2. Spaces are progressively disclosed organization

A **Space** is a V1 organizational object for users who need multiple related Rooms/Spaces. It is optional in the default shell and progressively disclosed, not optional in the architecture.

Examples:

- a class with `General`, `Assignments`, `Study`;
- a project with `General`, `Design`, `Operations`;
- a community with several discussion channels.

Spaces do not replace direct chats or ordinary groups.

OwnMessenger may expose or hide a Spaces destination. Even when the tab is hidden, the Omnibar can open a Space/channel directly.

## 3. V1 structural model

The default presentation should remain simple, but it must render the canonical architecture: Spaces can contain/nest Spaces, link independent Rooms, and a Room may be linked from more than one Space. Navigation is cycle-safe. Visual nesting never creates transitive authority. The UI reveals only hierarchy that actually exists instead of forcing every group into a Discord-like tree.

## 4. Responsive presentation

### Compact/mobile

Entering a Space shows the current channel as the main conversation. Channel switching uses a sheet/drawer or dedicated Space surface that preserves the current message context.

The user should not permanently lose horizontal message space to a Discord-style rail on a phone.

### Expanded/tablet/desktop

A Space may reveal a persistent channel list alongside the conversation when space allows.

The mental model remains the same as mobile; larger screens reveal parallel context instead of creating a different information architecture.

## 5. Groups, channels, and direct chats can coexist in the chat list

The chat-first root may contain recent direct chats, groups, and optionally recent Space channels in one recency-based conversation list.

Visual metadata identifies the context without forcing separate navigation.

A user may choose stricter organization in OwnMessenger, for example:

- show Space channels only inside Spaces;
- mix recent Space channels into Chats;
- pin selected channels;
- expose a Spaces tab.

Explicit user organization outranks inferred organization.

## 6. Membership and authority

The UI must reflect the **actual architecture capability model**, not invent social authority that the protocol does not provide.

Where possible, describe actionable capability:

- `Can invite members`
- `Can remove members`
- `Can manage channels`
- `Can manage call entry`

rather than relying only on vague labels such as `Owner` or `Admin`.

Friendly role names may exist, but security-relevant consequences must be derived from authoritative capabilities.

## 7. Invites

Group/Space invitations use the same privacy-first invite architecture as one-to-one contact invites, with added scope shown clearly:

- destination name and type (`Family group`, `Design Space`);
- reusable vs one-time/limited invite when supported;
- expiry when supported;
- approval requirement when supported;
- permissions granted by accepting;
- option to revoke/reset an invite where architecture permits.

QR/link presentation must never hide whether the invitation joins a group, joins a Space, or only starts a one-to-one conversation.

## 8. Notification hierarchy

Notification policy can be inherited:

```text
Global OM policy
  ↓
Space/group policy
  ↓
Channel policy
  ↓
Conversation-specific override
```

The resolved policy must be understandable and reversible.

A busy Space must not force every channel into the user's attention budget. Useful defaults include:

- all activity;
- mentions/replies;
- muted/passive;
- explicit per-channel overrides.

OM should summarize high-volume activity rather than emitting one interruption per message.

## 9. Unread state

Unread indicators distinguish at least:

- unread activity;
- direct mention/reply when architecture supports it;
- active call/joinable call where relevant.

Color may accelerate recognition but is never the sole distinction.

Space-level unread summaries are aggregates, not replacements for channel/message truth.

## 10. Duplicate names and identity ambiguity

Groups create a higher risk of ambiguous display names.

If multiple participants use the same/similar display name, OM should provide contextual disambiguation through:

- avatar/pattern;
- user-local nickname;
- shared-group/context information;
- an AccountID/security-details path;
- a duplicate-name notice when ambiguity becomes consequential.

A display name is never treated as cryptographic identity.

## 11. Threads and replies

Message replies remain the universal lightweight structure. Canonical V1 also includes lightweight Room-local threads and persistent Topics. Threads preserve navigation to source context; Topics are shown only where the Room uses them and do not create separate membership/MLS state.

## 12. Group/Space calls

A live call belongs to a specific Room/conversation context. A **call timeline item/notice** may be visible so users understand where it came from and can return after leaving the live surface. Call signaling itself is not claimed to be a durable ConversationEvent.

Joining a call is distinct from calling/ringing every member.

## 13. Leaving/removal

Actions such as `Leave group`, `Leave Space`, `Remove member`, or `Delete channel` are consequential/destructive and must disclose scope.

OM must not imply that leaving deletes remote copies of prior messages or that removing a participant erases data already received by their devices unless architecture explicitly guarantees that behavior.

## 14. Search and Omnibar

The Omnibar can resolve:

```text
open Design Space
open #operations
mute Family group
find roadmap in Design
call Study Group
```

Search results preserve context labels so identical channel names in different Spaces are not confused.

## 15. No engagement mechanics

Spaces must not become a social-growth layer by default.

OM should not add:

- public follower counts;
- engagement streaks;
- algorithmic discovery feeds;
- popularity rankings;
- mandatory public profiles;

merely because hierarchical communities exist.

The Space model is organization for communication, not a centralized social network.

