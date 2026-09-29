# R16A — Verification-gate triage and repo-local static closure

- **Base:** `om-design-20260929-0807-PHT-r15-uiux-integrated.zip`
- **Base SHA-256:** `cb13162f7f36c7af890ebb1ee27e0691a533f9a516b0bd64b038aa909aa729da`
- **Purpose:** classify every open architecture verification gate, close only unambiguous repo-local/static gaps, and leave environment/human/independent-review obligations honest.
- **Authorization:** this checkpoint does **not** authorize engineering.

## R16A closures

1. **V-005 / ADR-0157 critical-extension convergence.** Added an independent Python + Node vector model for the `om.critical.*` fail-closed prefix rule, updated the strict Node and Rust reference-codec sources, and added the new negative vector to the V-005 generator. The malicious `critical=false` downgrade now rejects under both baseline and future registries. All 13/13 repo-local V-005 criteria are now met; independent review/phase evidence remains open.
2. **V-013 bridge disclosure semantics.** The candidate bridge adapter contract now explicitly says an admitted private-Room bridge is a full Room/MLS member/plaintext recipient and that `allowed_event_classes` limits export, not reading. V-013 is now 2/4 repo-local criteria.
3. **V-020 verified-release provisioning.** Existing V-023 verifier-first lifecycle evidence and provider conformance cases PC-I01..PC-I03 were credited to V-020. V-020 is now 7/9 repo-local criteria.

## Gate inventory

| Gate | Category | Pre-eng | Local criteria | What actually remains |
|---|---|---:|---:|---|
| V-001 `production_crypto_path` | pre_engineering_passed | passed | 13/13 | pre-engineering already passed; only pre-release crypto review/build-exclusion remain |
| V-002 `account_control_formal_model` | independent_review_only | open | 10/11 | all 10 repo-local criteria met; model review by non-author remains; phase independence open |
| V-003 `one_to_one_session_model` | independent_review_only | open | 13/14 | all 13 repo-local criteria met; model review by non-author remains; phase independence open |
| V-004 `group_commit_control_model` | repo_local_substantial | open | 0/7 | formal group-control model; candidate MLS runtime ordering feasibility; reduction/liveness and partition convergence evidence; ADR-0156 wire freeze/two-codec vectors |
| V-005 `canonical_encoding_vectors` | independent_review_only | open | 13/13 | all 13 repo-local criteria now met; ADR-0157 normative amendment still requires different-vendor independent review/phase evidence |
| V-006 `web_device_vault_feasibility` | browser_platform_feasibility | open | 0/5 | WebAuthn PRF/WebCrypto/passphrase browser matrix; device-bound vs synced passkey behavior; origin-loss recovery on real target platforms |
| V-007 `sframe_interop` | runtime_interop_and_wire | open | 0/4 | two browser engines + native SFrame interop; call authorization/rekey runtime evidence; ADR-0156 wire freeze |
| V-008 `space_room_permission_authority_model` | repo_local_substantial | open | 0/6 | formal Room/Space authority model and revocation races; role/grant ceilings and delegated-intent cases; ADR-0156 wire freeze |
| V-009 `baseline_event_materialization_convergence` | independent_implementation | open | 0/5 | second materializer with independent authorization logic; reaction/vote/expiry/forwarding convergence |
| V-010 `plugin_sandbox_capability_model` | sandbox_runtime_feasibility | open | 0/7 | native Wasm + PWA sandbox prototypes; adversarial capability/egress/site-isolation cases; ADR-0156 wire freeze |
| V-011 `service_principal_authority_and_automation_model` | repo_local_substantial | open | 0/7 | service authority/automation model; tool-capability adversarial prototype; removal/incarnation rules; ADR-0156 wire freeze |
| V-012 `expressive_media_and_pack_parser_safety` | fuzz_interop_and_wire | open | 0/3 | agreed CPU-hour parser fuzzing; second ContentPack manifest implementation; ADR-0156 wire freeze |
| V-013 `bridge_provenance_fidelity_and_loop_model` | review_plus_wire | open | 2/4 | 2/4 repo-local criteria met after R16A; independent review of bridge adapter contract; ADR-0156 Bridge wire freeze/two-codec vectors |
| V-014 `portable_archive_interop_and_import_safety` | spec_freeze_and_review | open | 2/3 | archive spec remains frozen-candidate, not frozen; needs ADR/crypto-profile review/V-005 object coverage before criterion can close |
| V-015 `metadata_exposure_and_privacy_path_model` | independent_review_only | open | 16/16 | all 16/16 repo-local criteria met; phase independence/independent review remains |
| V-016 `decentralized_abuse_moderation_authority_model` | independent_review_only | open | 9/9 | all 9/9 repo-local criteria met; phase independence/independent review remains |
| V-017 `protocol_longevity_and_0m_continuity_drill` | independent_review_only | open | 3/3 | all 3/3 repo-local continuity criteria met; always-required pre-engineering gate; independent review remains |
| V-018 `security_ux_comprehension` | human_study | open | 0/7 | security UX participant study is intentionally postponed; theme compiler test is repo-local but does not substitute for participant evidence |
| V-019 `pq_group_reinit_migration` | crypto_runtime_prototype | open | 0/1 | authenticated MLS ReInit to hybrid-PQ suite prototype/vectors |
| V-020 `home_onboarding_and_provider_portability` | repo_local_model_remaining | open | 7/9 | 7/9 criteria met after R16A; blocked-vendor/suffix simulation still missing; sponsor-revocation overlap + non-sponsor second-route model still missing; independent review after repo-local closure |
| V-021 `endpoint_coercion_and_platform_leakage` | independent_review_only | open | 10/10 | all 10/10 repo-local criteria met; phase independence/independent review remains |
| V-022 `performance_budgets` | reviewed_plan | open | 2/3 | 2/3 pre-engineering criteria met; measurement harness plan exists but independent review is not recorded; real device/client measurements are pre-release/implementation evidence |
| V-023 `clienthost_update_lifecycle` | independent_review_only | open | 9/9 | all 9/9 repo-local criteria met; phase independence/independent review remains |
| V-024 `lan_delivery_binding` | mixed_wire_model_browser | open | 0/4 | LAN profile CDDL/registry/two-codec vectors; passive-observer privacy trace model; offline first-contact model; real browser DataChannel feasibility or explicit unavailable capability |

## Important scope conclusion

For **`core_kernel`**, the repo-local evidence is effectively finished after R16A: V-001 pre-engineering already passed; V-002, V-003, V-005, V-015 and V-016 have no remaining repo-local criteria; and always-required V-017 also has all repo-local criteria met. What remains before `core_kernel` authorization is the required independent-review/phase-independence evidence, not more local model construction.

This is useful because it prevents R16B from wasting time rebuilding core-kernel evidence that is already complete. R16B should instead target the **remaining executable/spec/vector work** for later scopes—especially V-004, V-008, V-011, V-013, V-014, V-020 and the repo-local portion of V-024—while treating V-006/V-007/V-010/V-012/V-019/V-024 browser/runtime portions and V-018 human study honestly as environment-dependent.

## What R16A intentionally did not close

- No independent-review-only gate was self-approved.
- No browser/device/runtime criterion was inferred from documentation.
- No participant-study criterion was approximated with model testing.
- V-014 remains a frozen **candidate**, not a frozen archive profile.
- V-022 remains open until the measurement plan has the required independent review; later real-device numbers remain separate.
- Overall gate statuses remain open where pre-release obligations remain.

## Next bounded run

**R16B:** execute the highest-value repo-local remaining model/spec/vector criteria in bounded groups, starting with the scopes closest to engineering usefulness. Do not spend R16B on independent-review-only gates; batch those for R16C/reviewer handoff.
