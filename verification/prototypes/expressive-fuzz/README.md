# Expressive-media parser fuzzing (V-012, ADR-0251)

Verification harness, not product code. It fuzzes the parsers OM itself ships for expressive media: the WebP header
walker and budget validator, the `ExpressiveAssetRef` and event decoders, the `ContentPackRelease` and
`ContentPackControlState` parsers with the lineage verifier, and the portable pack-file codec (ADR-0254).

| Binary | ADR-0251 target | Entry point (`om_core::expressive::fuzz_entry`) |
| --- | --- | --- |
| `webp`, `facts` | (a) WebP walker and budget validator | `webp`, `facts` |
| `asset_ref`, `event` | (b) `ExpressiveAssetRef` and inline and reaction decoders | `asset_ref`, `event` |
| `release` | (c) `ContentPack` manifest and `ContentPackRelease` | `release` |
| `control_state` | (d) `ContentPackControlState` and lineage walker | `control_state` |
| `pack_file` | extra: portable pack file and predecessor chain | `pack_file` |

The entry points compile only with om-core's `fuzz-entry` feature, which a `production` build refuses (R-072). Each
returns the stable refusal code and panics only when a stated invariant fails.

## Files

- `seeds.json.gz`: the committed seed corpus, `{target: {file: hex}}`. Sources: the frozen expressive wire vectors
  (positive and negative), the pack-file golden vectors, the container budget rows (at, one over and over every
  ceiling), the hostile WebP chunk shapes, and deterministic truncation, flip and append variants. File names carry the
  expectation (`ok-`, `bad-`, `x-`). Regenerate with `python3 gen_seed_corpus.py`; `--check` fails on drift.
- `fuzz_targets/` with the crate manifest and lockfile: the libFuzzer targets (`cargo-fuzz`), a standalone crate outside the
  engineering workspace.
- [`run_fuzz.py`](run_fuzz.py): budgeted, resumable runner. `--status`, `--dry-run`, `--budget-hours H` (default 96), `--jobs N`.
- `../../../crates/om-core/tests/expressive_fuzz.rs`: offline replay of every seed (verdicts, 1 second per input), a
  short deterministic mutation smoke, proptest properties for the ceilings, and the timed mutation driver the runner
  uses for its `mutate` engine.

## Running

Smoke, offline, stable toolchain (this is what `cargo test` already runs):

```sh
cargo test --offline -p om-core --test expressive_fuzz
python3 verification/prototypes/expressive-fuzz/run_fuzz.py --engine mutate --budget-hours 0.01 --slice-seconds 10
```

The ADR-0251 budget, on the build host (heavy; one run at a time; nightly toolchain with `cargo-fuzz` and the crates
the first build downloads):

```sh
cd ~/dig/lonunjie && bin/lonunjie-heavy -- python3 verification/prototypes/expressive-fuzz/run_fuzz.py \
  --jobs 8 --slice-minutes 20 --write-summary
```

Hours are CPU-hours (wall time times `--jobs`) and only the `libfuzzer` engine counts toward the budget. The ledger
(`work/ledger.json`, untracked) is written after every slice, so an interrupted run resumes where it stopped. A finding
stops that target, keeps its input under `work/artifacts/`, and makes the runner exit 3. Fix it, keep the input as a
regression seed (add it to [`gen_seed_corpus.py`](gen_seed_corpus.py)'s sources or a dedicated `x-` seed), clear the target's `finding` state
in the ledger and resume. `--write-summary` writes [`verification/results/expressive-fuzz-latest.json`](../../results/expressive-fuzz-latest.json).

## Not done here

- The second, independently implemented fuzzer (AFL++) over the same targets. The entry points are tool-neutral.
- The libFuzzer targets were not built in the authoring environment: the `arbitrary` crate is not in the offline
  cargo cache. They are one-line wrappers over the entry points the replay test exercises.
- The full budget: 24 CPU-hours per target, so at least 96, plus 24 for the pack-file target.
