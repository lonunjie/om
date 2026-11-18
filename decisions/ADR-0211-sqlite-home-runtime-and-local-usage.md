# ADR-0211 — SQLite Home runtime and local usage accounting

- **Status:** Accepted by operator delegation for Stage 1 implementation choices
- **Date:** 2026-11-18
- **Scope:** `provider_reference` (ADR-0171)

## Context

ADR-0186 calls for a person's own Home. The provider-diversity design recommends an embedded SQLite implementation and leaves its runtime, disk layout, and local accounting mechanism to engineering. The optional personal usage API already has deployed field names and client decoding.

## Decision

1. Use Node.js, embedded SQLite, and a bundled entry point. The second Home runs on a standard host or in a container. Build it from the same Home API handler, resource logic, limits, and wire package as the first Home. The Node build substitutes the local bindings for the reference runtime's storage and wake facilities. There is no cloud service at runtime.
2. Use one SQLite file per scoped resource, with one process holding the data-directory lock. Serialize operations per resource and store the mutation, retry outcome and scheduled expiry in one SQLite transaction. Return success and publish wakes after commit. Unknown queue and discovery lookups do not allocate persistent resource files.
3. Terminate HTTPS at the operator's local reverse proxy and default the Home listener to loopback. Require owner admission for resource creation on the standalone entry point. Use the existing personal profile by default. Leave optional push unbound so it is absent from the capabilities response.
4. Preserve the optional usage response, meter windows, thresholds and shedding rules. The existing metric names count logical ingress and routed storage work using the same accounting formula, including key fetch fan-out. There is no expenditure, paid resource provisioning, or external usage source. Both implementations run the same public API cases for the surfaces they share.

These choices fill implementation details. The Home API and the release criteria in PR-013 and V-020 continue to apply. Account, device, relationship and recovery authority stay with end-to-end verifiers.
