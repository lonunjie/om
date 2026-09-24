# OM Architecture Snapshot

Generated: **2026-09-24 23:30 PHT (UTC+08:00)**, after the independent architecture review R1–R4 (ADR-0117)

## State

OM is in **architecture verification / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`, and every entry in `engineering_scopes` is `authorized: false`.

- design queue: 13 open items (`design_open=13`), 10 of them blocking: A-024..A-028, A-032 and A-047 (`core_kernel`), A-035 (`calls`), A-045 (`provider_reference`), A-046 and A-047 (`reference_client_ui`); A-030, A-031 and A-036 are open but non-blocking;
- verification gates V-001..V-023: all open (`verification_open=23`, `pre_engineering_open=23`); V-023 (ClientHost update lifecycle) is new;
- public release gates PR-001..PR-014: all open; each now declares `applies_to`, `criteria` and `independence`;
- ledgers: requirements R-001..R-277 (268 records), threats T-001..T-089 (79 records), decisions through D-152 (ADR-0130). The per-scope gate and design-item table is generated in [`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md) §3.

## What changed: architecture review R1–R4 (2026-09-24)

Four independent reviews (records in `verification/reviews/2026-09-24-claude-opus-architecture-r1..r4.md`) were
applied as ADR-0126..ADR-0130: the W0 spec addendum (first-contact MessageRequest objects, fork bridge, mandatory PQ
signed prekey), account-authority transition rules, group reduction liveness and membership consent with a room
capability registry, extension/service/client trust boundaries, and distribution, provisioning and provider trust
roots (T0 client origin, verified-release provisioning, advisory-floor governance, a ClientHost lifecycle gate,
machine-applicable release gates, conformance-suite registry). V-005 now has 619 agreeing W0 vectors across two
codecs and two CDDL tools.

## Earlier: 2026-09-24 02:29 hardening checkpoint ([`CHECKPOINT_20260924_0229.md`](docs/checkpoints/CHECKPOINT_20260924_0229.md))

An independent hardening review ([`HARDENING_REVIEW_20260924.md`](docs/reviews/HARDENING_REVIEW_20260924.md), findings F-01..F-17) found that the protocol core held up, and that the weaknesses sat around it:

1. **The engineering lock was circular.** Several gates need real implementations to pass. Gates are now two-phase, with independence rules, and unlock is scope-granular across waves W0..W4 ([`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`](docs/process/RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md), [`verification/GATE_PASS_CRITERIA.md`](verification/GATE_PASS_CRITERIA.md)).
2. **Onboarding required a personal Cloudflare account.** New Home tiers give zero-signup sponsored starter mailboxes from invites, hosted and self-hosted Homes, and a required second non-cloud reference provider. Vendor monoculture is a declared 0M risk ([`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`](docs/architecture/providers/HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md)).
3. **Endpoint adversaries were missing.** Device linking is never silent, lock controls are defined, OM refuses deniability claims, and a platform leakage matrix is required ([`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md)).
4. **Two honesty fixes.** v1 groups are classical, not PQ, and there is now an objective migration path. Recovery is informed-off with a ContinuityAssessment.
5. **Lifecycles.** Vulnerability response without a kill switch; account retirement, deletion and legacy; a public-surface illegal-content process.
6. **Machine truth.** Every requirement is traced ([`machine/requirement_traceability.json`](machine/requirement_traceability.json)), threats are in a register ([`machine/threat_register.jsonl`](machine/threat_register.jsonl)), and the validator enforces both, along with gate schema, scopes and the no-PQ-group-overclaim rule.

## Evidence

`python3 verification/run_phase1.py`: bounded cases now include 1,620 ContinuityAssessment cases (total recorded in [`verification/results/phase1-summary.json`](verification/results/phase1-summary.json)). No gate is promoted; evidence is not passage.

## Next verification work (in unlock order)

W0 (`core_kernel`, `reference_client_ui`, `provider_reference`) needs, besides the pre-engineering criteria listed per gate in [`verification/GATE_PASS_CRITERIA.md`](verification/GATE_PASS_CRITERIA.md):

1. design items: A-024..A-028 and A-032 (crypto and referral-token choices), A-045 (Home API), A-046 (release-transparency log), A-047 (W0 conformance case lists);
2. independent (non-author) review of the evidence already produced for V-002, V-005, V-015, V-017, V-020 and V-021 (ADR-0117);
3. V-001 and V-003: the v1 path without the spqr crate; the 1:1 session-manager model;
4. V-006, V-018, V-021, V-022, V-023: browser DeviceVault prototype, comprehension study, ClientHost update lifecycle;
5. V-017 (always required): review of the continuity bundle script and drills.

No product engineering begins from this checkpoint.
