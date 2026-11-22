# Owner-recovery and link-ceremony parser fuzzing (V-012)

Verification harness, not product code. It fuzzes the parsers OM itself ships for the owner-recovery codecs and the
device-link ceremony frames, with sanitizers on, and also holds the runner for a sanitized rebuild of the existing
expressive-media targets ([`../expressive-fuzz`](../expressive-fuzz/README.md)).

| Binary | What it reaches (`om_core::recovery_fuzz_entry`) |
| --- | --- |
| `sheet` | recovery sheet text forms (compact, sheet form with line checks, QR payload), the binary layout (raw, and with a valid whole-string check appended so mutations reach the field rules), the owner checkpoint, the typing helpers, the redirect rule |
| `record` | `RecoveryRecord` (type 36) with its verifier, `RecoveryRedirect` (37), the envelope (35) opened under fixed synthetic keys, the bucket rule, the relationship snapshot and its second layer |
| `kit` | `RecoveryKit` (type 38), and the frozen registry's pre-decode check over every registered object |
| `approval` | the approval offer link, proposal, descriptor, reply, chunk and evidence messages, and the chunk assembler |
| `notice` | `RecoveryNotice`, the dossier reader, the recovery policy, request and veto bodies, the drained inbox item |
| `link` | the device-link text (`om:link/`), the invite link token, the controller-change link, the approval offer, and a raw queue item opened under fixed keys |
| `frame` | sealed ceremony and approval frames: the input is sealed under fixed synthetic keys and opened again, and the opener is checked against an independent restatement of the framing rule (length prefix, zero padding, wrong-role refusal) before the field readers run over the body |
| `continuation` | the continuation chunker of the link approval (ADR-0195): chunk count bound, order and completeness |

The provisional certified kit of ADR-0246 has no parser in the product (its object type is not registered), so the
`kit` binary can only show the frozen registry refusing it. Object type 45 in the registry is `ContentPackRelease`, which
the expressive targets already cover.

The entry points compile only with om-core's `fuzz-entry` feature, which a `production` build refuses (R-072). Each returns
`Ok(())` when the input was accepted and a stable refusal code otherwise, and panics only when a stated invariant fails
(a strict decoder re-encodes to exactly its input, a parsed value survives a re-encode, the frame opener agrees with the
framing rule, the chunker keeps every item in order). That panic is the finding. Sources:
`crates/om-core/src/flows/recovery_fuzz.rs`, `crates/om-core/src/flows/ceremony_fuzz.rs`, and two small gated shims next to the
private parsers in `controller_change.rs` and `recovery_inbox.rs`.

## Files

- `seeds.json.gz`: the committed seed corpus, `{target: {file: hex}}`, built only from the frozen vector files (recovery
  sheet, record, notice, owner-recovery ceremony, device-link ceremony, controller change) plus deterministic truncation,
  flip and append variants. File names carry the expectation (`ok-`, `bad-`, `x-`). `python3 gen_seed_corpus.py` regenerates it;
  `--check` fails on drift.
- `fuzz_targets/` with [`Cargo.toml`](../../../Cargo.toml) and `Cargo.lock`: the libFuzzer targets, a standalone crate outside the engineering workspace.
- [`run_fuzz.py`](run_fuzz.py): budgeted, resumable runner for two suites (`--suite recovery`, `--suite expressive`).
  `--status`, `--dry-run`, `--budget-hours H`, `--jobs N`, `--write-summary`, and `--summary-only` (writes the summary from the ledger of every target and runs nothing; use it instead of `--write-summary` after a run limited with `--target`, which would otherwise write a summary of that one target).
- [`run_detached.sh`](run_detached.sh): starts both suites one after the other, detached, on the build host.
- `../../../crates/om-core/tests/recovery_fuzz.rs`: offline replay of every seed (verdicts, 1 second per input) and a short
  deterministic mutation smoke. This is what `cargo test` runs, and the regression gate for any input a long run keeps.
- Results: [`verification/results/recovery-fuzz-latest.json`](../../results/recovery-fuzz-latest.json) and
  [`verification/results/expressive-fuzz-sanitized-latest.json`](../../results/expressive-fuzz-sanitized-latest.json).

## Sanitizers

libFuzzer via `cargo-fuzz` on the nightly toolchain, with AddressSanitizer (`-s address`) and `--debug-assertions`, which also
turns on arithmetic overflow checks. rustc has no UBSan, so overflow checks and debug assertions are the counterpart: an
unchecked operation or a failed internal invariant is a crash, as in a debug build. The expressive suite reuses the existing
fuzz crate and entry points but builds them this way (the earlier release-mode runs had ASan only). Each input is bounded to
1 second (`-timeout=1`), 768 MiB per worker.

## Running

Smoke, offline, stable toolchain (what `cargo test` already runs):

```sh
cargo test --offline -p om-core --test recovery_fuzz
python3 verification/prototypes/recovery-fuzz/gen_seed_corpus.py --check
```

The budget, on the build host (heavy; one run at a time; nightly toolchain with `cargo-fuzz`):

```sh
cd ~/dig/lonunjie && bin/lonunjie-heavy -- bash verification/prototypes/recovery-fuzz/run_detached.sh
```

That copies the tree to `~/build/recovery-fuzz-v012` (the heavy-run rsync deletes untracked directories, so the work
directory must live elsewhere), runs 12 libFuzzer workers per target, 15 minute slices, logs to `~/build/recovery-fuzz.log`.
Run the same command again to resume (a rerun of one target takes `--target NAME`, then `--summary-only` per suite). Default budgets are 24 CPU-hours for `recovery` (3 per target) and 12 CPU-hours for
`expressive` (about 1.7 per target). Hours are CPU-hours, wall time times workers, an upper bound on CPU used.

The ledger (`work/ledger-<suite>.json`, untracked) is written after every slice. A finding (crash, timeout, out-of-memory,
sanitizer report) stops that target, keeps its input under `work/<suite>/artifacts/`, and makes the runner exit 3. Keep such
inputs local. After a fix, commit only a neutral regression test, clear the target's state in the ledger and resume.

## Not done here

- A second engine over these targets (AFL++) and structure-aware generation for the signed objects inside the notice and
  evidence frames (their signatures are not checked by these entry points).
- The long run is evidence of the stated hours only; it does not establish that the parsers are free of defects.
