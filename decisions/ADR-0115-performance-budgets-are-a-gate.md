# ADR-0115 — Performance budgets are a verification gate (V-022)

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** operator (agreed with review suggestion)

## Decision

"Fast" is in OM's mission, so it gets numbers. The measurable budgets in [`MOTION_AND_PERFORMANCE.md`](../docs/architecture/ux/MOTION_AND_PERFORMANCE.md) cover cold and
warm start, input latency, scroll, send-to-delivered, push wake, crypto cost, search, idle battery, memory and bundle
size. They become gate V-022 on `reference_client_ui` and `provider_reference`. Pre-engineering: the budgets, the
reference device classes and the measurement plan are recorded, and V-001/V-005 prototypes show that crypto and codec
costs fit the budgets on native and wasm32. Pre-release: every budget is measured on real reference devices, with CI
regression checks. Missing a budget needs an ADR waiver.
