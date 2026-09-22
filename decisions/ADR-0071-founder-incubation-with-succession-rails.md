# ADR-0071 — Accept founder-only incubation while preparing succession rails

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

SeedLife/lonunjie is the initial steward and a single founder may hold project custody during S0 incubation. Multi-person authority is added only when trustworthy maintainers exist, but repository roles, asset inventory, release-key threshold migration, and handoff procedures are designed now.

## Rationale

Pretend redundancy with untrusted people is worse than explicit single-person risk. The protocol and licenses must already make founder disappearance/capture survivable by users and forks.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
