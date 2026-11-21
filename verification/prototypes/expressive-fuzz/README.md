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

## Second fuzzer: AFL++ (V-012)

ADR-0251 asks for a second, independently implemented fuzzer over the same targets. AFL++ (through `cargo-afl`) is that tool:
a different engine, mutator, scheduler and instrumentation from libFuzzer, driving the very same `fuzz_entry` functions.

- [`afl/`](afl): standalone crate (own lockfile, outside the engineering workspace) with one persistent-mode binary, `om-afl`.
  `OM_AFL_TARGET` picks the entry point (`webp`, `facts`, `asset_ref`, `event`, `release`, `control_state`, `pack_file`); inputs are cut
  at the libFuzzer `max_len` (64 KiB, 256 KiB for `pack_file`). Overflow checks and debug assertions stay on, so an unchecked
  operation or a failed invariant is a crash. No sanitizer: AFL++ is the unsanitized, coverage-guided counterpart to the
  AddressSanitizer libFuzzer runs.
- Same seed corpus ([`seeds.json.gz`](seeds.json.gz)), unpacked to `work/seeds/<target>`.
- Runner: `run_fuzz.py --engine afl`. One `-M main` plus `--jobs - 1` `-S` instances share one output directory per target, `-t 1000`
  (the same 1 second per-input bound as libFuzzer), `-m 2048`. Every slice resumes the previous queue (`-i -`).
  Crashes and hangs are copied to `work/artifacts/<target>/` as `crash-*` and `timeout-*`; either one is a finding (exit 3).
- Budget accounting is separate from libFuzzer: `--afl-budget-hours` (default 30) is split evenly over the five target groups
  (the four ADR targets and the pack-file codec), then over each group's binaries; the ledger is `work/ledger-afl.json` and the
  summary is [`verification/results/expressive-fuzz-afl-latest.json`](../../results/expressive-fuzz-afl-latest.json). AFL hours never
  count toward the libFuzzer budget and the reverse. Hours are wall seconds times instances, an upper bound.

One-time host setup (user-local, nothing system-wide): `cargo +nightly install cargo-afl --locked`, then
`cargo afl config --build` (the runner does the second step itself). `AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1` is set by the runner
because the host's `core_pattern` pipes to a crash handler; AFL++ detects crashes from the exit signal, not from core files.

```sh
# smoke (builds, then runs one binary for 20 seconds with 4 instances)
cd ~/dig/lonunjie && bin/lonunjie-heavy -- bash -c 'cd verification/prototypes/expressive-fuzz && \
  python3 run_fuzz.py --engine afl --afl-budget-hours 0.2 --slice-seconds 20 --jobs 4 --target facts'

# the V-012 run: copy the tree out of the synced path (rsync --delete would wipe work/) and detach
cd ~/dig/lonunjie && bin/lonunjie-heavy -- bash -c 'rm -rf ~/build/afl-v012 && mkdir -p ~/build/afl-v012 && \
  cp -r verification crates Cargo.toml Cargo.lock rust-toolchain.toml ~/build/afl-v012/ 2>/dev/null; \
  cd ~/build/afl-v012/verification/prototypes/expressive-fuzz && nohup systemd-run --user --scope -q -p MemoryMax=10G \
  nice -n 10 python3 run_fuzz.py --engine afl --jobs 12 --slice-minutes 15 --write-summary > ~/build/afl-v012.log 2>&1 &'
```

Resume by running the same last command without the `rm -rf`/`cp`; the ledger and queues live in `~/build/afl-v012/.../work`.

## Browser path (V-012)

Fuzz of the TypeScript and wasm side, deterministic and offline. Seeded generator (`OM_BROWSER_FUZZ_SEED`, default 2830),
`OM_BROWSER_FUZZ_ITERATIONS` scales it (default smoke 300 to 400), `OM_BROWSER_FUZZ_REPORT=1` prints a one-line tally.

- [`packages/om-web-platform/src/expressive/fuzz.test.ts`](../../../packages/om-web-platform/src/expressive/fuzz.test.ts): mutated WebP
  headers (the 57 smallest `webp` seeds, [`fuzz-corpus.ts`](../../../packages/om-web-platform/src/expressive/fuzz-corpus.ts), regenerate
  with [`gen_browser_corpus.py`](gen_browser_corpus.py)) and hostile header facts (NaN, infinities, negatives, 2^53, off-by-one at every ceiling) go through
  `checkBudget`, `decodeExpressiveAsset` and the native decoder. It checks: nothing throws or rejects unhandled; every refusal is one
  of the typed fallback reasons; an asset is accepted only when an independent restatement of the ADR-0251 ceilings (pixels,
  frames, canvas, size, resident bytes, duration) holds; a worker starts only for an accepted asset; a silent or hostile worker
  (no reply, error, message error, refused post, mismatch) always ends in a typed fallback inside the 5 second decode budget and is
  terminated; a native decoder never returns a frame whose size differs from the validated facts and closes every bitmap.
- [`packages/om-core-wasm/src/expressive-fuzz.test.ts`](../../../packages/om-core-wasm/src/expressive-fuzz.test.ts): the real
  `om_wasm_expressive.wasm` through `createCoreRuntime`. Mutated pack files (from the `pack_file` seeds) through
  `reviewPackFile`, and mutated release and control-state bodies, hostile asset references, ciphertexts and decoder observations
  through the expressive JSON commands. It checks: no wasm trap (a panic in Core), refusals typed (a `refused` value or a
  `CoreCallError` code, never `trapped`), 2 second per-input bound, the instance still answers, and resident memory growth under
  768 MiB. Skipped, loudly, when the artifact is not built (`pnpm --filter @om/core-wasm build:wasm:expressive`).

Run both as part of `vitest run`. Boundary: WebP bytes reach the real walker inside Core only through the pack and asset paths
the Rust fuzz already covers; the browser tests do not decode pixels (Node has no WebP codec) and do not drive a browser.

## Not done here

- The libFuzzer targets were not built in the authoring environment: the `arbitrary` crate is not in the offline
  cargo cache. They are one-line wrappers over the entry points the replay test exercises. (They were built and run on the build host.)
- Continuous fuzzing of shipped parsers (V-012 pre-release criterion).
