# ADR-0070 — Use a layered anti-capture licensing and contribution model

- **Status:** Accepted
- **Checkpoint:** 2026-09-22 23:02 PHT

## Decision

OM uses MPL-2.0 for shared core/client libraries, AGPL-3.0-or-later for OM reference network-service implementations, Apache-2.0 for normative protocol/conformance artifacts, and CC-BY-SA-4.0 for architecture/documentation. Normal contributions use DCO/inbound-equals-outbound and do not require broad copyright assignment.

## Rationale

No one license simultaneously optimizes independent protocol implementation, reusable client integration, and network-service anti-enclosure. The layered model protects the commons while allowing commercial interoperability.

## Consequences

- machine decisions/requirements and canonical documents MUST encode this decision;
- changes require a new ADR plus security/interoperability/migration analysis where applicable;
- convenience layers cannot silently bypass the invariant.
