# ADR-0073 — One-to-one multi-device session management is a distinct protocol state machine

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM specifies a session manager above the handshake/ratchet for per-device sessions, fanout, crossed initiation, device changes, stale/orphaned sessions, reset, persistence, and logical-message deduplication. Device eligibility is validated from AccountControlState.

## Rationale

A secure ratchet alone does not define asynchronous multi-device behavior. This layer must be explicit to avoid provider device lists becoming authority.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
