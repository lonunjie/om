# V-005 Rust/CDDL verification — 2026-09-27

## Scope

This run verifies only V-005 on checkpoint `om-design-20260927-1308-PHT.zip`. The supplied archive SHA-256 was `efde9acf8b4da94af567fcc3a8af93ddbedc9d37939e23112093874303db39ac`, matching the expected value before extraction.

## Initial reproduction and portability correction

The unmodified runner regenerated 735 vectors, then stopped because it compared implementation-specific gzip container bytes. Decompressing both files showed identical canonical JSON: 1,204,364 bytes with SHA-256 `d5de1deaca350942b286a55d09de2748c234b37c26f77cad315d6c72bb46644a`. The generated gzip had SHA-256 `731c041699a20f1bb7e3a15f7dc12b9e7a295896604f908668193a8b8884ee9f`; the committed gzip had SHA-256 `a1a49a3046d7eef5c592a8af3acae9a50c62e9cdf90a59883b100bf6f919ad79`. Their compressed data differed only in the gzip header's OS byte. The runner now decompresses the committed gzip and compares canonical JSON to regenerated JSON. It does not rewrite vectors.

## Toolchain and commands

Pinned tools were installed from the checkpoint instructions:

- Rust `1.98.1 (48a229cea 2026-09-01)`; Cargo `1.98.1 (797e8a9bc 2026-08-05)`; target `wasm32-wasip1` was installed.
- Rust CDDL crate CLI `cddl 0.10.7` (`cargo install cddl --version 0.10.7 --locked --root .tools`).
- Ruby CDDL gem `cddl 0.12.14` (`GEM_HOME=.tools/gems gem install cddl -v 0.12.14`); `json_pure 2.6.3` was installed for the pinned system Ruby 2.6.
- Node `v24.14.0`, Ruby `2.6.10p210`, Python `3.14.7`, wasmtime `49.0.1`.

Executed the full default command from `verification/prototypes/v005-encoding/`:

```sh
./run.sh
```

Also executed the two CDDL dependency install commands above and `GEM_HOME=.tools/gems gem install json_pure -v 2.6.3`.

## Results

- Corpus: 735 vectors, 71 positive and 664 negative; 23 object types.
- Rust strict codec: 735/735 verdicts, 0 mismatches; existing fixtures 14/14, 0 mismatches.
- Node strict codec: 735/735 verdicts, 0 mismatches.
- R-280: Python and Node each checked 16 sequences, 0 mismatches; 5 sequences reached ACTIVE per implementation.
- CDDL: Rust and Ruby each checked 399 applicable vector cases; 0 disagreements.
- CDDL generation: Ruby generated 12 examples for each of 23 roots (276 total); Rust strict lenient mode reported 0 unexpected rejections.
- Native and wasm32-wasip1 benchmark stages completed.
- Machine result: [`verification/results/v005-encoding-latest.json`](../../verification/results/v005-encoding-latest.json), `all_checks_pass: true`.

The runner emitted a non-fatal `rust-objcopy` warning about loading `libLLVM.dylib` while stripping debug info. Rust codec, CDDL, and both benchmark stages completed successfully.

## Status

The two remaining V-005 pre-engineering criteria are now met, and the gate ledger records 8/8 criteria plus `pre_engineering_independence_status: satisfied`. Overall V-005 stays `open` because the separate pre-release criterion (all conforming release implementations pass the complete vector suite) has not been performed. No other gate changed, and this result does not authorize engineering.

## Applied

The verification evidence and harness correction are applied to the supplied checkpoint and packaged in the returned ZIP. The archive is a bounded verification checkpoint, not an import of the unrelated changes already present in the supplied drop.
