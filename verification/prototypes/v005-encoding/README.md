# V-005 encoding prototype

**Feasibility prototype. Not product code. Not for production.** It exists to
produce evidence for gate V-005 (canonical encoding of W0 signed/hashed
objects) and codec-cost numbers for V-022. Evidence is not gate passage:
the registries it tests are `proposed_for_freeze` until an operator ADR
freezes them.

## What it checks

| Input | Location |
|---|---|
| CDDL for the 18 W0 signed/hashed objects | `spec/cddl/*.cddl` |
| Registries (numeric fields, object types, domain separation, crypto profiles, time fields, CBOR profile) | `spec/registries/*.json` |
| 540 object vectors (36 positive, 504 negative) | `verification/vectors/v005-w0-object-vectors.json.gz` |

1. **Two codecs agree.** The Rust strict codec (`rust/`) and the Node strict
   harness ([`verification/cbor/node_strict_harness.mjs`](../../cbor/node_strict_harness.mjs)) decode, validate and
   re-encode every vector. For each one, `gen/compare.py` checks the verdict,
   the exact error class, byte-identical re-encoding and the derived
   signature/hash inputs. Rust also re-encodes the 14 existing
   [`verification/cbor/fixtures.json`](../../cbor/fixtures.json) fixtures.
2. **Two CDDL tools agree.** `gen/cddl_check.py` validates each vector with
   the Rust `cddl` crate 0.10.7 and the Ruby `cddl` gem 0.12.14. It also feeds
   the Ruby tool's generated examples to the Rust codec in lenient mode.
3. **Codec cost (V-022).** `v005 bench` times decode, decode+validate, encode
   and signature/hash-input derivation for one representative object per
   type, natively and as wasm32-wasip1 under wasmtime.

Positive vector bytes come from the existing, unmodified
[`verification/cbor/python_canonical.py`](../../cbor/python_canonical.py) encoder. The generator only
hand-emits bytes for deliberately non-canonical negatives.

## Run

```sh
./run.sh               # everything, writes verification/results/v005-encoding-latest.json
./run.sh --no-bench    # skip V-022 timings
./run.sh --update-vectors   # accept regenerated vectors (normally they must match the .gz)
```

Requirements: python3, node, rustup (the toolchain pinned in
`rust/rust-toolchain.toml`, with target `wasm32-wasip1`), wasmtime and ruby.
Install the two CDDL tools into the git-ignored `.tools/` directory:

```sh
cargo install cddl --version 0.10.7 --locked --root .tools
GEM_HOME=.tools/gems gem install cddl -v 0.12.14
```

## Known tool quirks (handled in the scripts)

- The Rust `cddl` 0.10.7 CLI exits 0 even when validation fails, so the
  verdict is read from its log.
- The same tool never matches a `KEY => VALUE` map member whose key is a
  named rule or has a control operator (`ext-id`, `tstr .size (1..128)`).
  Only the Rust tool validates against a copy with the `extensions` key
  inlined as `tstr`. The 1..128 bound is still checked by the Ruby tool and
  by both codecs.
- On macOS, rustup's `rust-lld` for 1.98.1 cannot find `libLLVM.dylib`.
  `run.sh` sets `DYLD_FALLBACK_LIBRARY_PATH` for the wasm link.

## Limits of this evidence

- One agent wrote both codecs, the registries and the generator in a single
  session. Their agreement shows internal consistency, not independent review
  (ADR-0117 is still proposed).
- KDF/AEAD/MAC labels, per-prekey signatures, DeviceCredential key fields and
  `OM-Crypto-v1` stay provisional until open items A-023..A-028 are settled.
  The vectors use the insecure test crypto profile (65535).
- Timings come from one machine (see the results JSON for host and
  toolchain). They show order of magnitude, not a budget.
