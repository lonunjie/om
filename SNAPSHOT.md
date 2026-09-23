# OM Architecture Snapshot

Generated: **2026-09-24 02:29 PHT (UTC+08:00)**

## State

OM is in **architecture verification / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`, and every entry in `engineering_scopes` is `authorized: false`.

- design queue A-001..A-022: all design-resolved (`design_open=0`); A-016..A-022 came from the 2026-09-24 hardening review;
- verification gates V-001..V-021: all open (`verification_open=21`, `pre_engineering_open=21`);
- public release gates PR-001..PR-014: all open.

## What changed in this checkpoint

An independent hardening review (`HARDENING_REVIEW_20260924.md`, findings F-01..F-17) found that the protocol core held up, and that the weaknesses sat around it:

1. **The engineering lock was circular.** Several gates need real implementations to pass. Gates are now two-phase, with independence rules, and unlock is scope-granular across waves W0..W4 (`RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md`, `verification/GATE_PASS_CRITERIA.md`).
2. **Onboarding required a personal Cloudflare account.** New Home tiers give zero-signup sponsored starter mailboxes from invites, hosted and self-hosted Homes, and a required second non-cloud reference provider. Vendor monoculture is a declared 0M risk (`HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md`).
3. **Endpoint adversaries were missing.** Device linking is never silent, lock controls are defined, OM refuses deniability claims, and a platform leakage matrix is required (`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`).
4. **Two honesty fixes.** v1 groups are classical, not PQ, and there is now an objective migration path. Recovery is informed-off with a ContinuityAssessment.
5. **Lifecycles.** Vulnerability response without a kill switch; account retirement, deletion and legacy; a public-surface illegal-content process.
6. **Machine truth.** Every requirement is traced (`machine/requirement_traceability.json`), threats are in a register (`machine/threat_register.jsonl`), and the validator enforces both, along with gate schema, scopes and the no-PQ-group-overclaim rule.

## Evidence

`python3 verification/run_phase1.py`: bounded cases now include 1,620 ContinuityAssessment cases (total recorded in `verification/results/phase1-summary.json`). No gate is promoted; evidence is not passage.

## Next verification work (in unlock order)

W0 (`core_kernel`, `reference_client_ui`, `provider_reference`) needs these pre-engineering criteria met:

1. V-002: install a JDK + pinned TLC, run `AccountControlState.tla`, extend it with retirement/legacy/device-link notice, get an independent review;
2. V-005: freeze CDDL/field/domain-separation registries for W0 objects, with two-encoder vectors;
3. V-001 and V-003: Rust native+wasm state-machine prototype; 1:1 session-manager model/fuzzer;
4. V-015 and V-016: MetadataExposureDescriptors (including the sponsored mailbox); IntroGate/Sybil resource model;
5. V-006, V-018, V-021: browser DeviceVault prototype; comprehension study on `design/prototype`; platform leakage matrix;
6. V-020: provider conformance spec, sponsorship model, migration model, vendor-block tabletop;
7. V-017: continuity bundle script and written tabletop drills (always required).

No product engineering begins from this checkpoint.
