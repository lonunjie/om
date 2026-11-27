# ADR-0273 — Communities and publications: the engineering host's read model and local follow (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** `communities_publications` scoped engines and the engineering-host communities adapter.
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It follows ADR-0261, ADR-0268, ADR-0269, ADR-0270 and ADR-0271, adds no wire object and lowers no floor.

## Decisions

1. **Core state is observed, verified and bounded.** The scoped module gains handle-based engines over the Core modules that already exist: a publication feed (control states and events accepted through `Feed`, with rollback, fork and conflict handling as implemented) and a Space chain (`SpaceChain`). The host supplies signed bytes; Core verifies and keeps only what its modules accept. A host's word is never authority (ADR-0269 item 2). Handle counts are bounded like the other scoped engines.
2. **A community directory is a host-observed list, labelled.** Display names and descriptions of communities, rooms and publications come from host-supplied observations bounded and cleaned as in ADR-0270; the adapter never invents a name. Link state (official, space side, child side, bound) comes from the existing binding engine.
3. **Admission offers mirror ADR-0270 but join is not sent in v1.** An admission offer is a host-supplied observation bound to a digest review target. No wire object exists for sending a join request to a community, so `join_community`, `request_community_join`, `join_room` and `request_room_join` stay `Rejected(unsupported)` in the engineering host and the screens show the offer as read-only. Building a request needs its own wire ADR and vectors.
4. **Follow is a local preference.** `follow` and `unfollow` change only a profile-private local preference held by an injected store, keyed by the publication id, with durable operation identity and no network effect. Core has no state for it; the adapter reads and writes the store and reports `Committed` only after the write is durable.
5. **Provenance shown honestly.** A publication's provenance is `confirmed` only when the feed's control chain is accepted and not forked, `conflict` when Core reports a conflict or fork, and `checking` otherwise.
6. **Everything else follows ADR-0271:** engineering-only, one adapter per captured profile, deny-all authority by default.
7. **Not in v1:** sending join requests, community discovery by search, and any non-engineering host.
