# ADR-0075 — Separate normal routing from catastrophic reachability and blob-location repair

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

RelaySet migration includes authenticated version/continuity/overlap and explicit all-known-routes-dead behavior. Attachments use stable BlobID plus authenticated mutable BlobLocationRecord so provider relocation does not rewrite canonical message identity.

## Rationale

Provider replaceability needs a repair story for peers that were offline across migration. Some total-loss cases are inherently unrecoverable and must be stated rather than hidden by centralization.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
