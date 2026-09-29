# Architecture-Mandated V1 UX Surfaces — R15 Reconciliation

**Status:** normative design reference beneath canonical architecture / ADR-0159.

This closes the UI-design gaps identified by R14 without making them permanent top-level navigation.

## 1. Rich text and sanitized paste

The composer supports the bounded semantic RichText profile defined by architecture. Formatting may be exposed through contextual controls, keyboard shortcuts and accessibility actions. Pasted external markup is converted/sanitized into the supported semantic tree; unsupported/executable styling is discarded rather than smuggled into the message.

If conversion meaningfully changes content, the user gets calm preview/feedback before send. The UI never treats arbitrary HTML/Markdown as executable rendering authority.

## 2. Polls

Polls are contextual composer content.

States:

- compose options/rules;
- open;
- voted/not-voted presentation;
- results as current materialized state;
- closed.

Voting/close ordering and authorization are Core/event-model truth. Local wall clock never overrides canonical poll-close semantics. Unsupported clients retain safe fallback content as defined by the baseline event profile.

## 3. Threads and Topics

- **Reply:** universal lightweight reference to a source message.
- **Thread:** lightweight Room-local conversation context derived from supported thread semantics.
- **Topic:** persistent Room-local organizational object with TopicID.

The default chat remains linear. Thread/Topic affordances appear only where content or Room mode uses them. Entering a thread/topic preserves a visible path back to the parent Room/source message. Topics do not imply separate membership or encryption state.

## 4. Publication

A Publication is public signed content, not a private chat channel.

The feed shows:

- publication identity/source;
- signed-public provenance/continuity status when needed;
- follow/unfollow state;
- post/edit/retraction state;
- optional link to an independent discussion Room.

Do not display private-Room E2EE cues. Followers are not Publication members by implication. A retracted post is presented as retracted; OM cannot promise erasure from mirrors/copies.

## 5. Stickers and bounded looping media

Stickers/expressive media are first-class attachment/content selection, not a separate execution surface. Media packs are inert declarative content under the asset-pack contract. Every animated asset has a static/Reduced Motion fallback; recipient expression preferences control rendering intensity.

## 6. Typing, presence and read receipts

These are privacy-policy-controlled presentation signals, not proofs of identity/availability.

- Recommended may expose configured signals;
- Hardened reduces metadata according to canonical policy;
- Maximum Privacy minimizes/disables signals where architecture specifies;
- per-conversation user overrides are shown where allowed.

Absence of typing/presence/read receipt never means absence/offline/ignored with certainty.

## 7. Disappearing and view-once content

Conversation timers and per-message view-once state are visible before send and on received content. Search/export/backup views respect the lifecycle policy.

Copy/screenshot/endpoint capture limitations are explained honestly: client lifecycle semantics are not a magical remote-erasure guarantee.

## 8. ContactCapsule and MessageRequest provenance

A shared contact card names provenance (`Shared by Carol`) and never turns introducer provenance into identity authority. The receiver explicitly chooses to send a MessageRequest; acceptance creates the normal relationship path. Unknown requests remain separated from accepted conversations until accepted.

## 9. Link previews

Preview fetching follows canonical privacy/protection policy. Maximum Privacy may default preview fetch off. Failure leaves an ordinary clickable textual link; it does not make message send fail.

Where preview fetch can reveal network identity or content to a third party, policy/adapter architecture is authoritative. UI must not silently route around it.

## 10. Block, report and selective evidence

Block and Report are separate actions.

Report flow:

1. identify the actual report destination/provider/external handler where possible;
2. let the user inspect/select what evidence will be disclosed;
3. construct the architecture-defined EvidencePackage;
4. confirm submission/export scope.

The UI must not imply OM operates a universal abuse desk.

## 11. Home tiers and sponsored onboarding

The first-run surface remains invite-first and nontechnical, but the flow must represent T0–T4 honestly.

For T0/T1 or sponsored invite paths:

- explain the sponsor/host/co-resident observation facts required by architecture;
- explain migration/independence in user terms;
- present supported native plus independent PWA-origin client choices where required;
- do not use sponsor-controlled defaults to imply sponsor authority over identity.

Advanced infrastructure detail remains hidden unless the user asks for it.

## 12. Recovery defaults and Protection profiles

Recovery screen names current custody and status truthfully:

- synced passkey custodian when available;
- recovery-key custodian/fallback as policy requires;
- pending/unconfigured only when architecture truly cannot establish a custodian;
- explicit off state and irreversible-loss consequence.

Identity recovery, history restore and provider/discovery reconnection remain separate.

Protection profile picker is distinct from experience profile:

- Recommended;
- Hardened;
- Maximum Privacy;
- Custom.

These change privacy/routing/push/recovery policy but never lower the approved cryptographic floor.

## 13. Device suspend, revocation and Safety Freeze

Device card distinguishes:

- authorized/healthy;
- suspended/quarantined;
- pending stronger control transition;
- contested/Safety Freeze;
- permanently revoked/removed.

`Suspend this device` is the immediate emergency action when canonical state permits. Permanent revoke/remove is separate and may require stronger authorization. Contested control freezes privileged progression rather than letting the UI choose a last writer.

## 14. Service/bridge/plaintext boundaries

If a service/bridge/representing principal can newly read or author plaintext, a protected non-themeable notice names the service/boundary and gives Review/Revoke/Remove options where authorized. This is not treated as routine encryption decoration.
