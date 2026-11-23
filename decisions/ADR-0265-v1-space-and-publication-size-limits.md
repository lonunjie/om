# ADR-0265 — V1 limits for Spaces and Publications are the frozen wire bounds

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-24
- **Scope:** `communities_publications` only: the maximum size of a Space control state and of a Publication feed for V1.
- **Authority:** delegated, under the operator's standing delegation to resolve plan questions from the constitution by
  additive ADR. It introduces no number the frozen wire does not already carry, amends no audited text and lowers no floor.

## Context

The build plan for `communities_publications` (question Q3) asks for the maximum Space size and Publication mirror limits
in V1 and requires them recorded before the Space control engine is built. ADR-0160 froze the wire in
[`space-publication.cddl`](../spec/cddl/space-publication.cddl) with explicit bounds and the registries fix each object's
`max_bytes`.

## Decision

1. **The wire bounds are the V1 limits.** One `SpaceControlState` carries at most 1024 memberships, 128 role definitions,
   512 child links and 64 governance principals with a threshold of at most 64, within 131,072 bytes; a
   `SpaceDelegatedIntent` is at most 73,728 bytes; a `PublicationControlState` carries at most 64 publisher principals and 64
   signing-key states within 65,536 bytes; a `PublicationEvent` is at most 65,536 bytes.
2. **A Space at a limit fails closed.** An engine refuses a transition that would exceed a bound (a membership, role or link
   beyond its count, or an encoding beyond `max_bytes`) with a value-range failure. It never truncates, evicts or splits state.
   A Space larger than 1024 memberships is outside V1; supporting one needs a new wave and its own wire decision.
3. **Publication mirrors add no limit of their own.** A mirror holds what the feed's own objects bound: control states and
   events within their `max_bytes`, accepted in `seq` order per key. Per-account and per-provider quotas stay in the existing
   limits profile of the Home and relay, and an over-quota mirror may delay but never rewind or reorder a feed.
4. **No Room limit changes.** Room membership bounds stay those of the groups scope and its limits profile.

## Consequences

Slice 2 of the plan builds against these numbers and tests each bound at, and one past, its limit. Raising any limit later
is a wire decision, not an engineering tweak.

## Non-goals

This ADR does not set provider quotas, a hosted community directory size, or any limit for massive private broadcast, which
V1 does not have.
