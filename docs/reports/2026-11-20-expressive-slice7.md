# Expressive media slice 7: hardening and first V-012 evidence

This is hardening work and a first evidence pass. It is not a release, it flips no gate ledger entry, and the coverage-guided fuzz budget, the AFL++ second fuzzer and the browser-path fuzz were then run (see V-012 criteria and Open, 2026-11-21).

## Design sources read

- [AGENTS.md](../../AGENTS.md) (design gate), the V-012 entry of [GATE_PASS_CRITERIA.md](../../verification/GATE_PASS_CRITERIA.md), [Formal verification, fuzzing and adversarial testing](../process/FORMAL_VERIFICATION_AND_FUZZING.md), the [metadata trace test plan](../process/METADATA_TRACE_TEST_PLAN.md) and the [W0 metadata exposure descriptors](../../spec/metadata/w0-metadata-exposure-descriptors.json) (blob store and mailbox roles, `shared_blob_linkage`).
- [Expressive-media Core evidence](../../verification/EXPRESSIVE_NOTES.md), the [slice 6 report](2026-11-20-expressive-slice6.md), [ADR-0251](../../decisions/ADR-0251-expressive-media-fuzz-and-decode-budgets.md) and [ADR-0254](../../decisions/ADR-0254-expressive-media-pack-distribution.md).
- The product code and tests under `crates/om-core/src/expressive/` and the existing attachment, expressive and pack-file tests, read as the interface being tested.

### Not read in this pass

The slice 4, 5a and 5b reports were not read. The mapped gate domains returned for these paths (`crypto_1to1`, `discovery_first_contact`, `identity_control_recovery`, `metadata_privacy`, `relay_reachability`, `time_ordering_events`) were not read document by document in this pass, and neither were the constitution digest, the metadata threat model, the storage and blob architecture or the expressive-media architecture. Their earlier full readings are recorded in the slice 4 to 6 reports. Nothing here depends on a reading of those documents beyond the passages quoted above. This slice adds no wire, protocol or product behaviour; the only product-code change is a feature-gated test surface.

## What was built

| Item | Where |
| --- | --- |
| Byte-slice entry points for every parser, behind the `fuzz-entry` feature (refused by a `production` build, R-072) | `crates/om-core/src/expressive/fuzz_entry.rs`, [`crates/om-core/Cargo.toml`](../../crates/om-core/Cargo.toml), `crates/om-core/src/lib.rs`, `crates/om-core/src/expressive/mod.rs` |
| libFuzzer targets (`cargo-fuzz`, standalone crate), one per entry point | `verification/prototypes/expressive-fuzz/` |
| Seed corpus (530 inputs) and its generator | `verification/prototypes/expressive-fuzz/seeds.json.gz`, [`gen_seed_corpus.py`](../../verification/prototypes/expressive-fuzz/gen_seed_corpus.py) |
| Budgeted, resumable runner recording CPU-hours | [`verification/prototypes/expressive-fuzz/run_fuzz.py`](../../verification/prototypes/expressive-fuzz/run_fuzz.py) |
| Offline replay of every seed, mutation smoke, resource-ceiling properties | `crates/om-core/tests/expressive_fuzz.rs` |
| Expressive-media metadata trace test | `crates/om-core/tests/expressive_metadata_trace.rs` |
| Smoke summary | [`verification/results/expressive-fuzz-latest.json`](../../verification/results/expressive-fuzz-latest.json) |

Targets map to ADR-0251 as follows. (a) the WebP walker and budget validator are `webp` and `facts`; (b) the asset reference and expressive attachment-ref decoders are `asset_ref` and `event` (inline type 4, reaction key 3); (c) the manifest and release parser is `release`; (d) the control-state parser and lineage verifier is `control_state`; the portable pack-file codec and its predecessor-chain walk is `pack_file`, a fifth target that ADR-0251 prices at a further 24 CPU-hours. Resource-ceiling checks are covered by `webp`, `facts` and `pack_file` (the 64 KiB overhead, 32 MiB and chain-count ceilings sit in the pack-file decoder) and by the proptest in `expressive_fuzz.rs` that feeds arbitrary and extreme 32-bit facts to the budget validator.

The invariants an entry point enforces by panicking are determinism, budget bounds on every accepted header, a refused pack file leaving the inventory unchanged, staging refused before confirmation, and the staged count matching the announced count.

### Runner

`run_fuzz.py --budget-hours H` (default 96) gives each of the four ADR targets H/4 and the pack-file target a further H/4. Hours are wall seconds times `--jobs`. It writes a ledger after every slice, so an interrupted run resumes from the recorded hours. A finding stops its target, keeps the input and makes the exit code 3. Only the `libfuzzer` engine counts toward the budget; the offline `mutate` engine is recorded and never counted.

## Metadata trace (expressive-media portion)

Capture is the loopback provider's complete view: every call Core issues, the relay deposit log and the BlobStore contents. Detectors search that material for every string and byte string inside the pack, the pack and release identifiers, the file name, the media type and the conversation identifiers, in raw, hex, base64 and base64url form. A control test plants a value in each encoding and alignment and requires the detector to find it.

| Property asserted | Result |
| --- | --- |
| Sending a pack file as an ordinary attachment makes the same provider calls, in the same numbers, as a same-sized ordinary attachment | Holds |
| The stored object and the deposits have the same lengths as the control | Holds |
| Receiving makes only the ordinary blob fetch | Holds |
| Review and install make no provider call and queue no provider work | Holds |
| No pack content, identifier or file metadata reaches any provider | Holds |
| Sticker, inline emoji and reaction sends add no call class beyond the ordinary baseline | Holds |
| Default sends of one asset upload unrelated objects; the shared-blob option reuses one object, and that is the only added signal | Holds |
| The disclosure line for the shared-blob option is present, the default has none, and the descriptors give `shared_blob_linkage` as `sometimes` for the blob store and `no_by_design` for every other role | Holds |

Not covered: browser, TLS, CDN, push and native captures, the Home implementations, other profiles, and WebAuthn. The plan's pass rule for real builds is still open.

## V-012 criteria

Status words: met, partly met, open. "Independent" requirements are those of the gate ledger and are untouched.

| Criterion | Status | Evidence and what is left |
| --- | --- | --- |
| Pre-engineering 1: candidate decoders and pack parsers fuzzed for the agreed CPU-hour budget with no unresolved crash | Partly met | Harness, targets, committed seed corpus, per-input 1 second bound and a resumable CPU-hour runner exist ([fuzz harness](../../verification/prototypes/expressive-fuzz/README.md)). Coverage-guided libFuzzer runs under a sanitizer spent 120.2 CPU-hours (12 per WebP/facts/asset-ref/event target, 24 each for release, control-state and pack-file) with no finding. The second independently implemented fuzzer, AFL++ (`cargo-afl`, AFL++ 4.40c) over the same entry points, spent 30.2 CPU-hours (3 each for WebP, facts, asset-ref and event; 6 each for release, control-state and pack-file; 12 instances per target; 1.46 billion executions) with no crash and no hang, on its own ledger ([summary](../../verification/results/expressive-fuzz-afl-latest.json)). A deterministic browser-path fuzz (TypeScript budget preflight, decode driver against hostile workers, native decoder size agreement, and the real expressive Core wasm through the runtime) ran 20,000 iterations per probe on three seeds with no failure ([summary](../../verification/results/expressive-fuzz-browser-latest.json)). Both are evidence, not a flip: the AFL++ account is smaller than the libFuzzer one, it ran without a sanitizer, and the browser run decodes no pixels (Node has no WebP codec). Continuous fuzzing and the ledger's independence requirements stay open. |
| Pre-engineering 2: canonical ContentPack manifest and signature vectors agree across two implementations | Partly met | Earlier evidence: the frozen vectors ([vectors](../../verification/vectors/expressive-wire-vectors.json.gz), [method](../../verification/prototypes/expressive-wire/run.sh)) are replayed by the product codec (`om-codec` test `expressive_vectors`, re-run here) and by an independently written Node harness; the pack-file vectors are produced by Node and re-decoded by Core. The full `run.sh` was not re-run in this pass. |
| Pre-engineering 3: ADR-0156 wire freeze for inline type 4, ReactionSet key 3 and `ExpressiveAssetRef` | Partly met | Frozen CDDL, registries and positive and negative vectors exist ([CDDL](../../spec/cddl/expressive.cddl), [ADR-0252](../../decisions/ADR-0252-expressive-media-wire-freeze-and-delegated-constructions.md)), and two codecs agree on them per the earlier slices. Not re-verified in this pass beyond the `expressive_vectors` run, and the ledger's independence requirement for this phase is untouched. |
| Pre-release 1: shipped parsers under continuous fuzzing | Open | No continuous fuzzing is configured. The runner and targets are the starting point. |
| Gate ledger independence (independent tool and implementation) | Open | Not satisfied. No ledger entry was changed. |

The ADR-0251 corpus requirement is met in content: the positive and negative wire vectors, every over-ceiling case one over and exactly at, and truncated, flipped and appended variants of the golden files, committed. The overlapping and reordered chunk shapes of the hostile header corpus are included for the WebP walker.

## Observations from the replay

- The wire grammar accepts an `alt_text` of 2048 bytes; Core also enforces the architecture's 512 scalar value limit, so that vector is a wire-level verdict. Seeds mark it with no product expectation.
- The frozen `ConversationEvent` accept vectors and the wire-accepted "semantic" release rows are grammar verdicts. Core refuses the event rows with `value_range` because it binds the clock tiebreak to the authoring device (ADR-0121), and refuses the semantic rows with the named semantic code. This is expected. It means the event seeds reach the grammar and semantic checks but not a fully accepted event; coverage-guided runs will explore from the refusals.
- The mutation smoke found no crash, hang or invariant failure. This is smoke, not evidence of safety.

## Verification

Results as run on the authoring host unless noted.

| Check | Result |
| --- | --- |
| `cargo test --offline -p om-core --test expressive_fuzz --test expressive_metadata_trace` | 5 passed and 1 ignored (the timed run), then 4 passed |
| `cargo test --offline -p om-codec --test expressive_vectors` | 10 passed |
| `python3 verification/prototypes/expressive-fuzz/gen_seed_corpus.py --check` | seed corpus current: 530 files |
| Smoke fuzz, `run_fuzz.py --engine mutate --budget-hours 0.04`, seven binaries | 25,352,824 executions in 181.9 CPU-seconds, no finding. Not counted toward the budget. |
| `cargo test --workspace --offline --no-fail-fast` on the build host (before a final lint-only edit; the new test files were re-run locally afterwards) | Exit 0; 74 result lines, every one ok, no failures |
| `make check` | Exit 0 |
| `pnpm exec vitest run packages/om-web-platform/src/expressive packages/om-core-wasm/src/expressive` (browser-path smoke, real expressive wasm built) | 43 passed |
| `run_fuzz.py --engine afl --jobs 12 --slice-minutes 15` on emeth4 | 30.172 CPU-hours, 1,461,322,760 executions, 0 crashes, 0 hangs |
| `run_browser_fuzz.py --iterations 20000 --seeds 2830 2831 2832` | all tests passed, 0 failures |
| `cargo check -p om-core --features production,fuzz-entry` | Refused by the R-072 compile error |
| Clippy on the new files | No warnings from the new files |

## Open

- Done 2026-11-21: the ADR-0251 budget (120.2 CPU-hours under libFuzzer with a sanitizer on the build host, no finding). Summary: [expressive-fuzz-latest.json](../../verification/results/expressive-fuzz-latest.json).
- Done 2026-11-21: the second fuzzer. AFL++ 4.40c through `cargo-afl 0.18.2` over the same seven entry points, 30.2 CPU-hours (wall seconds times 12 instances, an upper bound) on emeth4, 1,461,322,760 executions, no crash and no hang. Driver crate `verification/prototypes/expressive-fuzz/afl/`, runner `run_fuzz.py --engine afl`, summary [expressive-fuzz-afl-latest.json](../../verification/results/expressive-fuzz-afl-latest.json). The AFL++ budget (30 CPU-hours, set here) is its own account and is not added to the libFuzzer count; no sanitizer was used. Per-target hours, executions and queue sizes are in the summary.
- Done 2026-11-21: the browser-path fuzz, `packages/om-web-platform/src/expressive/fuzz.test.ts` and `packages/om-core-wasm/src/expressive-fuzz.test.ts` (small checked-in corpus, deterministic, a short smoke inside `vitest run`, scalable with `OM_BROWSER_FUZZ_ITERATIONS`). Three seeds at 20,000 iterations per probe: every refusal typed, no accepted asset outside a ceiling, a worker started only for an accepted asset, every hostile worker ended in a typed fallback within the 5 second budget, no Core trap, slowest input under 10 ms, instance alive afterwards. Summary [expressive-fuzz-browser-latest.json](../../verification/results/expressive-fuzz-browser-latest.json). Limits: Node, not a browser; no pixel decode; the real WebP walker is reached only through the pack and asset paths.
- Still open for V-012: continuous fuzzing of the shipped parsers (pre-release), a sanitized or longer second-fuzzer account if the operator wants parity with libFuzzer, and the gate ledger entry, which this work does not flip.
- The libFuzzer targets were not built in the authoring environment because the `arbitrary` crate is not in the offline cargo cache. They are one-line wrappers, but a first build is unverified.
- Seeds that reach a fully accepted `ConversationEvent` (a real signed event with a matching device tiebreak) would improve depth for the `event` target.
- Real-build metadata captures (browser, native, Homes, push) and the other privacy profiles.
- A decision on whether a feature-gated public test surface in `om-core` is acceptable as the stable fuzz boundary, or whether the fuzz crate should wrap the engine API instead.
