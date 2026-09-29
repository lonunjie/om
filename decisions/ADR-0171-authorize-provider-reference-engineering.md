# ADR-0171 — Authorize engineering scope `provider_reference` (W0)

- **Status:** Accepted
- **Decided by:** operator, 2026-09-29 (directed "do everything you need to do so we can proceed with engineering")
- **Date:** 2026-09-29
- **Authorizes:** engineering scope `provider_reference` only
- **Basis:** [`2026-09-29-claude-opus-5-5-w0-provider-client-review.md`](../verification/reviews/2026-09-29-claude-opus-5-5-w0-provider-client-review.md), ADR-0167, ADR-0170

## Context

`provider_reference` depends only on `core_kernel` (authorized by ADR-0169). Its gates V-015, V-020 and V-022, and the
always-required V-017, have passed pre-engineering phases. No gate lists it in `interface_impact`, no open design item
blocks it, and its `provider` conformance suite is specified.

## Decision

1. `provider_reference` is authorized: the reference Home implementations under `infra/` in
   [`ENGINEERING_REPOSITORY_LAYOUT.md`](../docs/process/ENGINEERING_REPOSITORY_LAYOUT.md), built against
   [`spec/protocol/home-api.md`](../spec/protocol/home-api.md), the `provider` conformance suite and the
   `provider_reference` entry of [`machine/engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json).
2. The reference provider stays replaceable. It never becomes Core runtime authority (ADR-0137 negative capabilities),
   and a Cloudflare reference Home never becomes a protocol dependency.
3. PR-013 now carries a 90% single-vendor-block survivability target (ADR-0170). No resilience claim is made before a
   vendor-neutral second reference Home passes conformance and the target is met.
4. Performance budgets are pre-release criteria measured per the updated measurement plan.

## Consequences

- `provider_reference.authorized=true` with `authorizing_adr=ADR-0171`. Authorized scopes: `core_kernel`,
  `provider_reference`.
- `reference_client_ui` remains unauthorized until V-006 passes.
