# ADR-0278 — Community join in v1 is by invitation through the existing group flow

- **Status:** Delegated design decision (additive). Independent review remains required.
- **Date:** 2026-11-28
- **Scope:** Joining a community or Room that the host shows as an admission offer. Follows ADR-0273. No wire, registry or audited text changes.

1. A person joins a community's Rooms only by accepting an invitation that an existing member sends through the ordinary group flow (Invite, Accept, Decline over a relationship). That flow already has its wire, authority checks and vectors; nothing new is frozen.
2. There is no request from a stranger to a community in v1. Such a request needs a delivery path to people who do not know the sender and spam controls that interact with the introduction gate and relay limits. Choosing those here would invent audited design, so the safest path is to send nothing.
3. The engineering host keeps `join_community`, `request_community_join`, `join_room` and `request_room_join` as `Rejected(unsupported)`. The screens may say a member can invite the person; they must not suggest that a request was sent or is pending.
4. Reopening this needs a wire ADR with vectors, a stated delivery path, rate and size limits, and review. Until then release notes say plainly that joining is by invitation.
