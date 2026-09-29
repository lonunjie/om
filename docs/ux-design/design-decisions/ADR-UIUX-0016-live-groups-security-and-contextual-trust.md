# ADR-UIUX-0016 — Live communication, optional Spaces, and contextual trust

**Status:** Accepted  
**Date:** 2026-09-29

## Context

After R13A closed ordinary messaging semantics, three major interaction domains remained structurally incomplete: live calls, group/Space organization, and security/privacy operations.

These domains can easily violate OM's constitution if treated as separate products: calls can become a phone app bolted onto messaging; Spaces can turn simple groups into a Discord clone; security can become a permanent dashboard of cryptographic jargon.

## Decision

1. Calls are logical communication objects anchored to people/conversations/channels and reachable from chat, Omnibar, notifications/system call surfaces, or an optional Calls destination.
2. Temporary route/media failure preserves the logical call and uses a `reconnecting` state before terminal failure.
3. Direct/relay/provider route information is diagnostic, not a trust or encryption rating.
4. Incoming video invitations never activate the local camera before explicit user action.
5. Ordinary groups remain ordinary multi-person conversations.
6. Spaces are optional organizational containers with a deliberately shallow v1 `Space → Channel` model.
7. UI authority labels derive from actual capabilities rather than inventing centralized roles.
8. Cryptographic continuity, manual verification, device authorization, recovery authority, provider routing, and real-world identity are separate concepts.
9. Manual `Verified by you` state is a local user assertion after an approved comparison; automatic cryptographic checks must not claim real-world identity.
10. If a manually verified peer's identity/continuity changes, OM requires explicit review before new outbound communication to that peer by default, subject to final architecture merge semantics.
11. Device revocation and recovery operations are security-critical and disclose exact target/scope.
12. Identity, history, and provider/discovery recovery remain distinct in UX.
13. Constant decorative lock/shield scoring is rejected; security states surface contextually when meaningful.

## Consequences

- Native iOS/Android call adapters integrate with system call frameworks while preserving OM action semantics.
- PWA calling remains capability-truthful and may not promise background behavior the browser cannot provide.
- Group/Space UI remains optional and progressive.
- Security warnings become rarer but more meaningful.
- Several group-send and recovery edge cases remain architecture merge gates rather than being guessed in UI.

## Rejected directions

- Permanent Calls tab for everyone.
- Treating relay/TURN as weaker security than direct P2P.
- Automatically activating camera for incoming video invitations.
- Requiring Spaces/channels for all group chats.
- Unlimited nested Space hierarchy in v1.
- Universal `Admin/Owner` labels detached from actual capabilities.
- One global `secure/insecure` score or shield.
- Calling automatic key continuity checks `identity verified`.
- Generic `Recover account` UX that hides identity/history/provider scope.

