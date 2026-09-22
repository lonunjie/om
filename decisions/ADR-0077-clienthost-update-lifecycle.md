# ADR-0077 — Self-hosted ClientHosts require an explicit update/deployment lifecycle

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM defines signed-release discovery, provider-native deployment/staging, service-worker lifecycle, rollback/freeze protection, emergency minimum versions, credential scope, and compromised-origin migration. Same-origin code is not an independent trust root after origin compromise.

## Rationale

PWA origin independence is incomplete if thousands of personal hosts cannot be kept safely updated or recovered without identity loss.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
