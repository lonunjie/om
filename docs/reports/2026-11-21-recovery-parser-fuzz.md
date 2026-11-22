# Owner-recovery and link-ceremony parser fuzzing, and a sanitized rebuild of the expressive targets (V-012)

This is verification evidence. It is not a release, it changes no product behavior, and it flips no gate ledger entry.

## Design sources read

- [AGENTS.md](../../AGENTS.md) (design gate) and the V-012 entry of [GATE_PASS_CRITERIA.md](../../verification/GATE_PASS_CRITERIA.md).
- The [expressive slice 7 report](2026-11-20-expressive-slice7.md) and the [expressive fuzz harness README](../../verification/prototypes/expressive-fuzz/README.md), as the method being extended.
- The product code under test, read as the interface being exercised: the recovery sheet, record and kit modules, the recovery approval codec, the recovery notice and inbox readers, the link-ceremony and continuation code, the controller-change link, and the invite link token.
- The frozen vector files used as seeds (sheet, record, notice, owner-recovery ceremony, device-link ceremony, controller change).

### Not read in this pass

The architecture documents and ADRs of the gate domains returned for the changed code paths (`crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`) were not read in full. The work adds test entry points and evidence only and changes no protocol rule, so no departure from them is claimed.

## What was built

| Item | Where |
| --- | --- |
| Byte-slice entry points for the sheet, record, kit and registry, approval, notice and inbox parsers, behind the `fuzz-entry` feature (refused by a `production` build, R-072) | `crates/om-core/src/flows/recovery_fuzz.rs`, exported as `om_core::recovery_fuzz_entry` |
| Entry points for the link text, invite token, controller-change link, sealed ceremony frames and the continuation chunker | `crates/om-core/src/flows/ceremony_fuzz.rs` |
| Two small gated shims beside private parsers | `controller_change.rs`, `recovery_inbox.rs`; one gated `mod` line in `ceremony.rs` and `flows.rs`, one gated re-export in `lib.rs` |
| Eight libFuzzer targets (standalone crate with its own lockfile) | `verification/prototypes/recovery-fuzz/` |
| Seed corpus (1,890 inputs) and its generator | `seeds.json.gz`, `gen_seed_corpus.py` |
| Budgeted, resumable runner for two suites, and a detached starter | `run_fuzz.py`, `run_detached.sh` |
| Offline replay of every seed and a deterministic mutation smoke | `crates/om-core/tests/recovery_fuzz.rs` |
| Results | [`verification/results/recovery-fuzz-latest.json`](../../verification/results/recovery-fuzz-latest.json), [`verification/results/expressive-fuzz-sanitized-latest.json`](../../verification/results/expressive-fuzz-sanitized-latest.json) |

All entry points compile only with the `fuzz-entry` feature. Default builds are unchanged; a build with `production` and `fuzz-entry` is refused by the existing R-072 compile error.

### Invariants the entry points enforce

A strict decoder re-encodes to exactly its input; a parsed sheet, kit, record, snapshot, policy, request or veto survives a re-encode and re-reads equal; every text form of an accepted sheet reparses to itself and passes the redirect host rules; the frame opener agrees with an independent restatement of the framing rule (length prefix, zero padding) and refuses the wrong role; a parsed link survives a re-encode; the continuation chunker keeps every state and credential in order within the 64 chunk bound; the chunk assembler returns the evidence exactly when no chunk was altered. A failed invariant or any panic is a finding.

### Scope notes

- The provisional certified kit of ADR-0246 (provisional type 45, renumbered 47) has no parser in the product and its type is unregistered, so the `kit` target can only show the frozen registry refusing it. Type 45 in the registry is `ContentPackRelease`, covered by the expressive targets.
- Signatures inside notices, evidence and records are not checked by these entry points (a record's cross-field verifier does run). Sealed frames are covered by sealing the fuzz input under fixed synthetic keys; the frame target is capped at the 16 KiB bucket.
- Sanitizers: AddressSanitizer with debug assertions and overflow checks. rustc offers no UBSan, so the overflow checks and assertions are its counterpart. MemorySanitizer was not attempted.

## Runs

Host: emeth4, nightly toolchain with `cargo-fuzz 0.13.2` (rustc 1.102.0-nightly, 2026-11-19), libFuzzer, 12 workers, 15 minute slices, 1 second per input, 768 MiB per worker. CPU-hours are wall seconds times workers, an upper bound on CPU used; build time is not counted.

### Recovery suite (24 CPU-hour budget, 3 per target)

| Target | CPU-hours | Executions | Result |
| --- | --- | --- | --- |
| sheet | 3.018 | 517,998,055 | no finding |
| record | 3.020 | 1,362,319,678 | no finding |
| kit | 3.017 | 3,142,593,999 | no finding |
| approval | 3.020 | 8,658,679,855 | no finding |
| notice | 3.024 | 6,144,385,307 | no finding |
| link | 3.016 | 485,496,547 | no finding (rerun, below) |
| frame | 3.020 | 718,310,778 | no finding |
| continuation | 3.021 | 52,672,289 | no finding |
| Total | 24.155 | 21,082,456,508 | budget met |

### Sanitized rebuild of the expressive targets (12 CPU-hour budget)

| Target | CPU-hours | Executions | Result |
| --- | --- | --- | --- |
| webp | 1.722 | 10,590,812,342 | no finding |
| facts | 1.722 | 10,280,506,277 | no finding |
| asset_ref | 1.727 | 2,057,978,588 | no finding |
| event | 1.728 | 921,909,054 | no finding |
| release | 1.729 | 1,377,659,216 | no finding |
| control_state | 1.730 | 778,879,532 | no finding |
| pack_file | 1.726 | 1,247,999,304 | no finding |
| Total | 12.085 | 27,255,744,313 | budget met |

This is a separate account from the 120.2 CPU-hours of the earlier release-mode libFuzzer run and the 30.2 CPU-hours of AFL++ recorded in the slice 7 report; none of the three is added to another.

## Findings

No crash, no sanitizer report, no failed invariant and no out-of-memory report in either suite.

One event is recorded for completeness. The first 3 CPU-hour run of the `link` target ended with eight timeout reports, all for the continuation case. Each of the eight inputs replays in 2 to 18 ms under the sanitized binary and in under 4 ms in a release build, so none reproduces. The most likely cause is the harness allocating large synthetic items for that case while twelve workers contended for the host. The harness was changed to allow the large item size for the first few items only, the eight inputs were kept locally outside the repository, and the target was rerun for a fresh 3 CPU-hours with no report. Nothing in product code was changed and no product defect is claimed or ruled out by this event beyond what the rerun covers.

## Checks

| Check | Result |
| --- | --- |
| `cargo test --offline -p om-core --test recovery_fuzz` | 2 passed (every seed meets its expectation, 1 second bound; mutation smoke) |
| `cargo test --offline -p om-core --lib` and the sheet, record, owner-recovery ceremony, device-link vector and expressive fuzz tests | passed |
| Clippy on the new files | no warnings from the new files |
| `gen_seed_corpus.py --check` | archive is current |

## Open

- The gate ledger entry for V-012 is unchanged. This work does not flip it.
- Continuous fuzzing of the shipped parsers (pre-release) remains open.
- No second engine (AFL++) over the recovery targets, and no structure-aware generator for the signed objects carried inside notices and evidence.
- The coverage reached is bounded by the seed corpus and the mutation depth; the hours are evidence of effort, not proof that the parsers are free of defects.
- Re-run the recovery suite when the recovery extension (ADR-0230 to ADR-0248) gains product parsers; none exist for it today.
