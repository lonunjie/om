# ADR-0270 — Bridge offers: how a Room holds a proposed bridge and names it for review (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** `bridges` Core Room state, the scoped-engine query, and the bridge screens of the reference web client.
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It amends no audited text, adds no wire object, lowers no floor and adds no authority path. It is Room-local, in-memory state like the rest of the bridge Room (ADR-0269).

## Decisions

1. **An offer is a host-supplied observation, never authority.** A host may place a *proposed* bridge in a Room: the bridge's verified control head, the mapping it asks for (network namespace, direction, event classes), the approvers the host reports and two display strings (a bridge name and an advertised claim about the external leg). Proposing grants nothing: the bridge is not a member, holds no key and exports nothing until the existing admission path (`room.add_bridge`, human quorum) succeeds.
2. **Display strings are untrusted text.** The bridge name and external claim are bounded (64 and 160 characters), stripped of control characters and bidirectional overrides, and shown as the bridge's own statement. The claim never changes OM's security indicator and is labelled as the bridge's claim (BRIDGES section 8).
3. **One opaque review target per exact offer.** The target is a digest over the bridge id, the mapping incarnation and the mapping's namespace, direction and class set. An admit or remove intent carries that target; Core re-derives it and refuses any intent whose target is not the current offer or member, so a change made after review (a different direction or class list) cannot be approved by the earlier review.
4. **Membership states.** *proposed* (an offer, not in the roster), *member* (admitted), *removed* (was admitted, now removed; kept as a tombstone with its last target so the Room can show history and a fresh offer can replace it). A new offer for the same bridge replaces the earlier one and invalidates its target.
5. **Admit and remove go through the existing checks.** Admitting an offer runs `BridgeRoom::admit` with the offer's head, mapping and reported approvers and the caller's capabilities; removal runs `BridgeRoom::remove`. Neither path is loosened. The approvers of an offer are a labelled install-time record (ADR-0269 item 3).
6. **Not in v1:** carrying offers between members, offers from a bridge directory, and any automatic admission. Each would need its own wire and review.

## Why

The bridge screens need a state the Room can honestly show before admission and after removal, and an intent that is bound to what was reviewed. Holding that as Room-local state with a digest target keeps authority where it already is, adds no wire, and fails closed on stale or altered reviews.
