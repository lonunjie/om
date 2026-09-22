# ADR-0074 — MLS groups require application commit ordering and OM-state binding

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM will define a GroupCommitOrdering profile for same-epoch MLS Commit conflicts and bind membership/role-affecting GroupControlState transitions to the corresponding MLS Commit/resulting epoch.

## Rationale

RFC 9420 requires applications to resolve conflicting same-epoch Commits. OM cannot assume replaceable relays or a library silently provide canonical ordering, and application/cryptographic membership must not diverge.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
