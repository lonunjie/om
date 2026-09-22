# ADR-0078 — Architecture prose is backed by a machine-enforced repository contract

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

Every current non-history file is classified; architecture domains map docs/schemas/ADRs/requirements/open questions; a validator checks canonical terminology/schema uniqueness, IDs, links, phase lock, registry coverage and manifest integrity. Engineering authorization is a machine-visible false value until explicitly changed by ADR.

## Rationale

OM wants values expressed in prose but operational drift made detectable. The repository itself must fail validation when current architecture quietly diverges.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
