# ADR-0274 — Invocation review in the engineering host, driven by an injected selection (v1)

- **Status:** Delegated design decision (additive; independent review remains required). Supersedes only item 3 of ADR-0272, which was written before the broker command was exposed.
- **Date:** 2026-11-27
- **Scope:** the connected-services adapter in `apps/web/src/host/scope/` and its tests.
- **Authority:** delegated by the operator on 2026-11-26 and 2026-11-27. It follows SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS (selected-context invocation is the default, a bounded one-shot disclosure the user initiates) and ADR-0124, ADR-0129, ADR-0258 and ADR-0271. It adds no wire object and lowers no floor.

## Decisions

1. **The selection is the host's, the verdict is Core's.** The engineering host takes an injected `SelectionSource` that returns the items the user selected for one command (trigger, message, attachment, draft, thread or topic, text, participant, scope). The adapter never reads a conversation itself. It asks the existing broker command whether the invocation is allowed and what its information-flow result is (tainted, egress state), and renders only what Core returns.
2. **The review shows exactly what leaves.** The invocation view lists every disclosed item in order, including the trigger and any metadata actually disclosed, the destination, and an expiry. The review identity is a digest of the service, command, destination and item list; any change to the selection yields a new identity and voids the earlier review.
3. **Invoke is offered only when Core allows it.** The action is `ready` only if the broker allows the exact request and the service is installed with the capability; otherwise it is `blocked` with a Core-derived reason. Default authority stays deny-all (ADR-0271), so without an authority source the action is blocked.
4. **No send in the engineering host.** There is no 1:1 service session or transport in the engineering host, so a confirmed invocation is recorded as a durable pending operation and answered `PendingNetwork` and never `Committed`; the adapter never claims the service received anything. Delivery to a service endpoint needs its own design and is not in v1.
5. **Everything else follows ADR-0271 and ADR-0272.**
