# ADR-0170 — W0 provider and reference-client review findings

- **Status:** Accepted
- **Date:** 2026-09-29
- **Review:** [`2026-09-29-claude-opus-5-5-w0-provider-client-review.md`](../verification/reviews/2026-09-29-claude-opus-5-5-w0-provider-client-review.md) (ADR-0167 review model)
- **Amends:** [`PERFORMANCE_MEASUREMENT_PLAN.md`](../docs/process/PERFORMANCE_MEASUREMENT_PLAN.md), public-release gate PR-013, gate V-018 phasing
- **Decided by (V-018 phasing):** operator direction of 2026-09-29 to adjust the design as needed to proceed with engineering

## F-18 — a single-vendor block still cuts most relationships

The V-020 vendor-block model computes the surviving fraction as its criterion requires. Under the Cloudflare-first T1
tier mix, a vendor-wide block leaves about 1.6% of established relationships alive without a second provider family, and
about 42–48% with the modeled host-console fallback. PR-013 already forbids resilience claims before a vendor-neutral
second Home exists, but it sets no survivability target. A release could meet PR-013 while one block still cuts more
than half of all relationships.

**Decision.** PR-013 gains a quantitative criterion: before any `public_production_resilience` or
`censorship_resilience` claim, the V-020 vendor-block simulation, run on the release's actual onboarding tier mix, shows
that at least 90% of established relationships and 90% of first contacts keep a working route when any single provider
family is blocked. The target can be changed only by ADR, and the measured value is published with the claim.

## F-19 — the measurement plan could not produce contestable numbers

[`PERFORMANCE_MEASUREMENT_PLAN.md`](../docs/process/PERFORMANCE_MEASUREMENT_PLAN.md) named devices, scenarios and
instruments, but it gave no sample sizes, device-state controls, cross-device clock bounds, numeric CI regression rule
or realistic-state rule. It also had no rule keeping instrumentation out of release builds.

**Decision.** The plan gains a "Measurement controls" section: minimum samples per percentile, battery/thermal/cool-down
rules, a 20 ms clock-offset bound, encrypted-at-rest search fixtures, a 10% median / 2% bundle CI regression rule, and
no performance telemetry in release builds.

## V-018 phasing — the comprehension study needs a running client

V-018 asks for moderated sessions with at least 12 non-technical participants on non-production builds of the real
flows, and a theme-compiler test on the real renderer. None of that exists before `reference_client_ui` engineering
starts, and no AI review can stand in for participants (ADR-0167 §5). Keeping V-018 in the pre-engineering phase
deadlocks the scope that must be built to run it.

**Decision.** V-018's pre-engineering phase is `waived_by_adr` (ADR-0170). All seven criteria move unchanged to its
pre-release phase, alongside the existing participant-study and facilitator independence. No public or beta release of
the reference client may happen until they pass. Engineering must build the flows V-018 names behind a non-production
flag so the study can run on them. The security-UX requirements themselves stay normative from the start.

## Consequences

- V-020 and V-022 pass pre-engineering, so `provider_reference` becomes eligible for its scoped unlock ADR.
- V-021 and V-023 pass pre-engineering. `reference_client_ui` remains blocked only by V-006 (WebDeviceVault browser
  matrix), which needs runs on real browsers and devices with real passkeys.
- Everything here passes under the ADR-0167 review model, not vendor-independent review.
