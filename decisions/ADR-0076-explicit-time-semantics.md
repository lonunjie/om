# ADR-0076 — All canonical time fields declare clock and authority semantics

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM distinguishes HLC ordering, wall-clock expiry, monotonic local duration, provider-retention clocks, and cryptographic state sequencing. HLC/wall time never becomes account/group constitutional authority.

## Rationale

Scattered TTL/expiry behavior can create interop and rollback bugs. One cross-protocol time model makes bad-clock and skew behavior explicit.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
