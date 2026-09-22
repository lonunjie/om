# OM Master Planning & Design Snapshot

Generated: **2026-09-23 06:15 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-23 05:45 PHT architecture-hardening checkpoint, preserved under `history/20260923-0545/`.

## Phase

**Architecture hardening / pre-engineering. Product engineering is not authorized.**

## Major decisions in this checkpoint

- froze the first-release ordinary-messenger semantic baseline in `MESSENGER_BASELINE_V1.md`;
- unified private group/Room identity on the existing provider-independent `ConversationID`, removing a redundant distinct `GroupID`;
- defined `Space`/Community as an organizational/control container over independent Rooms/Publications/nested Spaces, with no silent child membership inheritance;
- required bilateral official child links and explicit Room-localized authority delegation, with at most one authority-parent Space in the baseline;
- selected capability-scoped roles, explicit restrictions and delegation ceilings instead of numeric role hierarchy/override mazes or a protocol-required god-mode Administrator;
- kept governance free of a permanent magical owner/vendor takeover path;
- selected layered reply/thread/Topic organization without making Topics sub-Rooms or MLS groups;
- defined private announcements as restricted-post MLS Rooms and public broadcast as a separate signed `Publication` object with non-canonical follower lists;
- corrected checkpoint drift around shared pins and the already-selected `CommitCoordinator`;
- resolved architecture queue A-005, A-006, A-007, A-008 and A-010; A-001..A-004, A-009 and A-011 remain open, and this pass added A-012..A-015 so import/export escape, metadata privacy, decentralized abuse/moderation and protocol longevity cannot fall out of scope;
- added V-008 to adversarially/formally validate Space/Room permission delegation and revocation, and V-009 to validate convergence/authorization of the expanded baseline event vocabulary before engineering.

## Current engineering blockers

Engineering remains locked. The open architecture-design blockers are now:

```text
A-001 expressive media
A-002 content packs
A-003 creator commerce / entitlement boundary
A-004 bots, agents and LLMs
A-009 plugin ecosystem
A-011 external-network bridges / interoperability
A-012 import/export and user escape
A-013 metadata threat model
A-014 decentralized abuse/moderation hardening
A-015 20-year protocol longevity / anti-capture
```

The blocking verification gates remain V-001..V-009.

Run:

```sh
python3 tools/validate_architecture.py
```

A later accepted ADR must explicitly authorize engineering; this checkpoint does not.
