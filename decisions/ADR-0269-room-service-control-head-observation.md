# ADR-0269 — How a client learns a Room's service and bridge control heads, and where approvers come from (v1)

- **Status:** Delegated design decision (additive; independent review remains required).
- **Date:** 2026-11-27
- **Scope:** `services_bots` and `bridges` Core state, the scoped-engine query, and the reference web host.
- **Authority:** delegated by the operator on 2026-11-27 ("decide on the constitution, usability, ease-security balance and security levels"). It amends no audited text, adds no wire object, lowers no floor and adds no authority path.

## Decisions

1. **No new carriage wire in v1.** The design says a service's controllers deliver every new control head to each Room where it is installed, and that members keep the highest validated head (ADR-0122 rule 6, ADR-0129 rule 6), but it freezes no object that carries a head into a Room beyond `state_head`. v1 invents none. Core accepts a head only through a host-supplied observation (an intent carrying the signed states, as already used for control heads elsewhere); how the host obtained the bytes (a Room message, a gossip exchange, a manual import in the engineering host) is outside Core and carries no trust.
2. **Core verifies; the host's word is never authority.** On observation Core checks every `SignedEnvelope` signature under the service or bridge control label, resolves each signer to a controller device of an account whose own control head Core holds, feeds the states to the existing `ServiceChain`, keeps only the highest validated head, rejects rollback, freezes on a fork, and accepts a reducing successor (ADR-0262, already implemented in `ServiceChain`). Controller kinds other than a single account (organization, Space, Room governance) have no verification path in v1 and are refused as `unverifiable_controller`: fail closed.
3. **Approver sets are a recorded governance fact, labelled as such.** The installation record holds only a digest of its approval evidence, and no group control field lists approvers. v1 records the approvers a host reports at install time next to the installation and shows them in the control view with the label "reported at install". Core never treats that set as proof of a quorum it did not verify; a human-quorum check that needs verified approvals stays refused until the Room carries them.
4. **Freshness uses the monotonic runtime clock.** The observation time of the Room's last accepted head is stamped from the runtime segment (ADR-0154), never wall-clock or event time; a head older than the bridge freshness bound is reported stale (ADR-0128 rule 11).
5. **Not in v1:** a Room carriage object for heads, head gossip between members, and kind 2 to 4 controllers. Release notes say so plainly. Each needs its own wire ADR, vectors and review.

## Why

A host that can hand Core a head and have it believed would make the host an authority. Verifying every signature in Core keeps authority where the design put it, costs the user nothing, and fails closed on anything unverifiable. Inventing a carriage object would be a wire freeze decision, which is not this ADR's to make.

## Consequences

Core gains per-Room service and bridge head state and a read-only control view (scope-engines only, per ADR-0268 item 1). The host adapters for bridges, portability and nearby read that view instead of supplying heads themselves.
