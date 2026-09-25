#!/usr/bin/env bash
# Feasibility prototype for V-005. Not product code. Not for production.
#
# Reproduces all V-005 encoding evidence and writes
# verification/results/v005-encoding-latest.json.
#
#   ./run.sh            full run (vectors, both codecs, both CDDL tools, native + wasm32 bench)
#   ./run.sh --no-bench skip the V-022 codec-cost benchmarks
#   ./run.sh --update-vectors  overwrite the committed gzipped vectors instead of checking them
#
# Needs: python3, node, cargo/rustup (toolchain from rust/rust-toolchain.toml, target
# wasm32-wasip1), wasmtime, ruby with the `cddl` gem under .tools/gems, and the Rust `cddl`
# CLI at .tools/bin/cddl (see README.md for how to install both into .tools/).
set -euo pipefail
cd "$(dirname "$0")"
repo="$(cd ../../.. && pwd)"
bench=1 update=0
for a in "$@"; do
  case "$a" in
    --no-bench) bench=0 ;;
    --update-vectors) update=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

# use the rustup toolchain pinned in rust/rust-toolchain.toml (it carries the wasm32-wasip1
# target) even when a system cargo (e.g. Homebrew) comes first on PATH
if command -v rustup >/dev/null; then
  PATH="$(cd rust && dirname "$(rustup which cargo)"):$PATH"
  export PATH
fi

B=build
mkdir -p "$B"
committed="$repo/verification/vectors/v005-w0-object-vectors.json.gz"

echo "== vectors" >&2
python3 gen/gen_vectors.py "$B/w0-object-vectors.json" >&2
gzip -9 -n -c "$B/w0-object-vectors.json" > "$B/w0-object-vectors.json.gz"
if [ "$update" = 1 ]; then
  cp "$B/w0-object-vectors.json.gz" "$committed"
elif ! cmp -s "$B/w0-object-vectors.json.gz" "$committed"; then
  echo "regenerated vectors differ from $committed (rerun with --update-vectors if intended)" >&2
  exit 1
fi

echo "== build" >&2
(cd rust && cargo build --release -q)
v005=rust/target/release/v005

echo "== codecs" >&2
"$v005" fixtures ../../cbor/fixtures.json > "$B/fixtures-rust.json"
"$v005" vectors "$B/w0-object-vectors.json" > "$B/rust.json"
node ../../cbor/node_strict_harness.mjs "$B/w0-object-vectors.json" > "$B/node.json"
python3 gen/compare.py "$B/w0-object-vectors.json" rust "$B/rust.json" node "$B/node.json" > "$B/compare.json" || true

echo "== R-280 reciprocal IntroIdentityBinding sequences (python + node)" >&2
python3 gen/intro_binding.py "$B/w0-object-vectors.json" > "$B/intro-python.json" || true
node ../../cbor/node_intro_binding.mjs "$B/w0-object-vectors.json" > "$B/intro-node.json" || true

echo "== cddl" >&2
python3 gen/cddl_check.py "$B/w0-object-vectors.json" "$B" "$v005" > "$B/cddl.json" || true

rm -f "$B/bench-native.json" "$B/bench-wasm32.json"
if [ "$bench" = 1 ]; then
  echo "== bench (native, wasm32-wasip1)" >&2
  # rustup's macOS rust-lld 1.98.1 has rpath @loader_path/../lib, which misses the toolchain's
  # libLLVM.dylib in $sysroot/lib; point dyld at it for the wasm link
  (cd rust && DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib" cargo build --release -q --target wasm32-wasip1)
  "$v005" bench "$B/w0-object-vectors.json" > "$B/bench-native.json"
  wasmtime run --dir "$B::$B" rust/target/wasm32-wasip1/release/v005.wasm bench "$B/w0-object-vectors.json" \
    > "$B/bench-wasm32.json"
fi

python3 gen/assemble.py "$B" "$repo/verification/results/v005-encoding-latest.json"
