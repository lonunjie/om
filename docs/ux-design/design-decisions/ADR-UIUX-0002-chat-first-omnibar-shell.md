# ADR-UIUX-0002: Chat-First, Omnibar-First Shell

**Status:** Accepted design direction  
**Date:** 2026-09-28

## Context

Different messengers expose different fixed bottom navigation sets. OM's OwnMessenger philosophy argues against assuming that Calls, People, Settings, or future capabilities deserve permanent navigation space for every user.

At the same time, OM needs one fast path that scales to people, messages, settings, commands, QR/invites, calls, and future actions without turning navigation into a growing tab collection.

## Decision

1. Chats/conversation list is the default root.
2. OM provides a fast global Omnibar as the universal find/do surface.
3. On compact mobile, the Omnibar occupies a lower reachable region by default.
4. Top-level views remain tabs, but users may choose which allowed tabs are shown and their order.
5. Visible tab count adapts to available width, accessibility scaling, localization, and user preference; excess tabs may use a semantically animated scrollable strip.
6. Account access is not fixed to top-left; shell placement is ergonomics- and preference-aware.
7. Conversation recency is newest-first by default; OwnMessenger may reverse/customize it.
8. Omnibar suggestions may learn locally, with learning off by user choice and no usage/preference data leaving the user's trust boundary by default.
9. The zero-contact first-run state is invite-first and does not upload contacts by default.
10. Direct and group conversations share a common outer grammar; richer nested group/space organization may progressively reveal only where needed.

## Consequences

- Users who only message can maintain a nearly single-surface messenger.
- Hidden tabs do not hide capabilities because Omnibar and Action Registry remain universal access paths.
- New features need not claim permanent navigation space.
- Personalization improves speed without requiring cloud profiling.
- The compact conversation screen still requires prototype work because the composer and global Omnibar both naturally want the bottom reach zone.
