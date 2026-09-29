# W0 provider and reference-client review — Claude Opus 5.5, 2026-09-29

- **Reviewer:** Claude Opus 5.5 (`claude-opus-5-5`), ADR-0167 operator review model (not vendor-independent).
- **Tree:** om `main` `58bea27` (after ADR-0169) plus ADR-0170.
- **Scope:** pre-engineering review obligations of V-020, V-021, V-022 and V-023, which unlock `provider_reference` and
  `reference_client_ui`. V-006 and V-018 are addressed separately (below).

## Evidence re-executed

`make verify` at `58bea27` passed: 51 phase-1 producers, 85 TLC runs as expected, validator, docs, file registry,
protocol evolution, continuity manifest and registry immutability. The producers relevant here are the home-tier model
(262,144 sponsorship and 73,728 migration cases), the R16B provider-resilience simulation, the ClientHost update models
(15/15 scenarios and 5/5 mutations; 13/13 and 8/8), the endpoint/profile/platform models (all scenarios pass, all
mutations caught), the ADR-0162 UI evidence (38/38) and the V-001/V-005 cost benchmarks.

## Findings

| ID | Severity | Summary | Resolution |
|---|---|---|---|
| F-18 | Major (non-blocking for engineering) | Under the Cloudflare-first tier mix a single-vendor block leaves about 1.6% of relationships alive without a second provider family and about 42–48% with the modeled fallback. PR-013 forbade resilience claims but set no survivability target | ADR-0170: PR-013 requires ≥90% of relationships and first contacts to survive any single-family block before a resilience claim |
| F-19 | Minor | The performance plan lacked sample sizes, device-state and clock controls, a CI regression rule, a realistic-state rule and a no-release-telemetry rule | ADR-0170: "Measurement controls" section added |

## Per-gate disposition

| Gate | Disposition | Basis |
|---|---|---|
| V-020 | **accept** | Home API frozen with typed errors and capabilities; sponsorship quotas/no-chain and T0→T2/T3 migration models pass. The migration model states its real limit (contacts offline past the forwarding window need re-introduction) and keeps the 30-day hand-off. Vendor-block fractions are computed as required; F-18 turns the result into a release target |
| V-021 | **accept** | Per-platform capability matrix present; device-link notice invariant holds in TLC (`NoSilentLink`); grant-without-notice failing trace present; ProfileExecutionContext, desktop/PersonRecord and Omnibar evidence pass with every mutation caught |
| V-022 | **accept** after F-19 | Budgets and device classes recorded; V-001 and V-005 costs are far inside budget (1:1 encrypt+decrypt median ≈13 µs native vs a 5 ms budget; session setup ≈0.65 ms vs 50 ms; codec decode+validate ≈1 µs); plan reviewed and completed with measurement controls |
| V-023 | **accept** | Owner-pinned root cannot be changed by release metadata; two transparency observations required; rollback, signature stripping, threshold omission, role/key substitution and root skipping are all caught; live browser/provider drills correctly stay pre-release |

## Not reviewable by AI, handled by decision

- **V-018** (security-UX comprehension) needs moderated sessions with at least 12 human participants on the real flows.
  ADR-0170 moves its criteria unchanged to pre-release, where they block any public or beta reference-client release.
- **V-006** (WebDeviceVault browser matrix) needs WebAuthn PRF ceremonies with real passkeys on each target browser and
  device. It stays a pre-engineering blocker for `reference_client_ui`. The feasibility prototype is
  [`verification/prototypes/v006-web-vault/`](../prototypes/v006-web-vault/README.md); its results must be recorded from
  real runs.

## Result

V-020, V-021, V-022 and V-023 pass pre-engineering. `provider_reference` is eligible for its scoped unlock ADR.
`reference_client_ui` remains blocked only by V-006.
