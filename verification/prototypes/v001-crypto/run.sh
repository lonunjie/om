#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Feasibility prototype for V-001. Not product code. Not for production.
#
# Builds the Rust prototype (native, wasm32-wasip1, wasm32-unknown-unknown), runs every check,
# and writes deterministic results to verification/results/v001-crypto-latest.json.
# Timing numbers (not deterministic) go to rust/target/v001-perf.json only.
#
#   run.sh          all checks (~2-4 min after first build; first build downloads crates and KAT files)
set -euo pipefail

LONG=0

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
RUST="$HERE/rust"
OUT="$RUST/target/v001-run"
RESULTS="$ROOT/verification/results/v001-crypto-latest.json"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
rm -rf "$OUT" && mkdir -p "$OUT"

log() { printf '[v001] %s\n' "$*" >&2; }
fail() { printf '[v001] FAIL: %s\n' "$*" >&2; exit 1; }
for t in cargo rustup wasmtime node uv python3 git curl shasum; do command -v "$t" >/dev/null || fail "missing tool: $t"; done

# ---- 1. Official NIST ACVP ML-KEM vectors, pinned commit + SHA-256 --------------------------
ACVP_COMMIT=975de31eb83d87039ec88934fdc47d8c312b892d
ACVP_CACHE="$RUST/target/acvp-$ACVP_COMMIT"
mkdir -p "$ACVP_CACHE"
fetch_acvp() { # name sha256
  local f="$ACVP_CACHE/$1.json"
  if [[ ! -f "$f" ]]; then
    log "fetching ACVP $1 @ $ACVP_COMMIT"
    curl -sfL -o "$f.tmp" "https://raw.githubusercontent.com/usnistgov/ACVP-Server/$ACVP_COMMIT/gen-val/json-files/$1/internalProjection.json" || fail "download $1"
    mv "$f.tmp" "$f"
  fi
  echo "$2  $f" | shasum -a 256 -c --status || fail "ACVP $1 hash mismatch"
}
fetch_acvp ML-KEM-keyGen-FIPS203 d7a62a2c3476957f56dd8d24f9004ea6776ccfe995ffe71a65bb9506dc9c7b1b
fetch_acvp ML-KEM-encapDecap-FIPS203 a556952ce869bb89c3a3196a701dad89647c193a34c86eafb61a9d710d5b810f
KG="$ACVP_CACHE/ML-KEM-keyGen-FIPS203.json"
ED="$ACVP_CACHE/ML-KEM-encapDecap-FIPS203.json"

# draft-ietf-hpke-pq-05 published test vectors (github.com/hpkewg/hpke-pq, tag draft-ietf-hpke-pq-05)
HPKE_COMMIT=6433c8fce0b8b749dfc86c1095081a88698ccfab
HPKE_VEC="$RUST/target/hpke-pq-05-$HPKE_COMMIT.json"
if [[ ! -f "$HPKE_VEC" ]]; then
  log "fetching draft-ietf-hpke-pq-05 test vectors @ $HPKE_COMMIT"
  curl -sfL --retry 3 -o "$HPKE_VEC.tmp" "https://raw.githubusercontent.com/hpkewg/hpke-pq/$HPKE_COMMIT/test-vectors.json" || fail "download hpke vectors"
  mv "$HPKE_VEC.tmp" "$HPKE_VEC"
fi
echo "35c59f4a0132e5631e50ac039d8ca3a72e99f5e92dfd94d45338d6ae243f613c  $HPKE_VEC" | shasum -a 256 -c --status || fail "hpke vector hash mismatch"

# ---- 2. Build ------------------------------------------------------------------------------
cd "$RUST"
log "building native, wasm32-wasip1, wasm32-unknown-unknown"
cargo build -q --locked --release -p om-v001-proto --features insecure-test-rng
cargo build -q --locked --release -p om-v001-proto --features insecure-test-rng --target wasm32-wasip1
cargo build -q --locked --release -p om-v001-node-wasm --target wasm32-unknown-unknown
NATIVE="$RUST/target/release/v001"
WASI="$RUST/target/wasm32-wasip1/release/v001.wasm"
NODEWASM="$RUST/target/wasm32-unknown-unknown/release/om_v001_node_wasm.wasm"
wasi() { wasmtime run "$@"; }

log "unit tests"
cargo test -q --locked --release -p om-v001-proto --features insecure-test-rng > "$OUT/unit.txt" 2>&1 || fail "cargo test"

# ---- 3. R-072 sketch: production feature cannot coexist with the insecure test RNG ----------
log "R-072 build-exclusion checks"
if cargo check -q --locked -p om-v001-proto --features production,insecure-test-rng >"$OUT/r072.txt" 2>&1; then
  echo '{"production_plus_test_rng_rejected": false}' > "$OUT/r072.json"
else
  grep -q "R-072" "$OUT/r072.txt" && P1=true || P1=false
  cargo check -q --locked -p om-v001-proto --features production >/dev/null 2>&1 && P2=true || P2=false
  # Artifact-level check: the production rlib must not contain the test RNG; the test build must (positive control).
  cargo build -q --locked --release -p om-v001-proto --features production --target-dir "$RUST/target/prod" >/dev/null 2>&1
  grep -aq InsecureTestRng "$RUST/target/prod/release/libom_v001_proto.rlib" && P3=false || P3=true
  grep -aq InsecureTestRng "$RUST/target/release/libom_v001_proto.rlib" && P4=true || P4=false
  echo "{\"production_plus_test_rng_rejected\": $P1, \"production_alone_builds\": $P2, \"production_rlib_lacks_InsecureTestRng\": $P3, \"test_rlib_contains_InsecureTestRng_control\": $P4}" > "$OUT/r072.json"
fi

# ---- 4. Selftests (one-time keys, replay, downgrade) on all three runtimes -------------------
log "selftests"
"$NATIVE" selftest > "$OUT/selftest-native.json" || fail "native selftest"
wasi "$WASI" selftest > "$OUT/selftest-wasi.json" || fail "wasi selftest"
node "$HERE/node/run_wasm.mjs" "$NODEWASM" selftest > "$OUT/selftest-node.json" || fail "node selftest"
node "$HERE/node/run_wasm.mjs" "$NODEWASM" rng > "$OUT/node-rng.json"

# ---- 5. Vectors: identical across runtimes and equal to the committed file ------------------
log "vectors"
"$NATIVE" vectors > "$OUT/vectors-native.json"
wasi "$WASI" vectors > "$OUT/vectors-wasi.json"
node "$HERE/node/run_wasm.mjs" "$NODEWASM" vectors > "$OUT/vectors-node.json"
COMMITTED="$HERE/vectors/om-v001-proto-vectors.json"
[[ -f "$COMMITTED" ]] || cp "$OUT/vectors-native.json" "$COMMITTED"

# ---- 6. Independent Python implementation + ACVP ------------------------------------------
log "independent Python check (uv)"
(cd "$HERE/python" && uv run --locked -q python check_vectors.py "$COMMITTED" "$KG" "$ED" "$HPKE_VEC") > "$OUT/python.json" || fail "python independent check"
# Mutation tests: the checker must reject (a) one flipped ciphertext nibble and (b) one flipped byte in
# the ML-DSA-65 half of the positive bundle signature.
python3 - "$COMMITTED" "$OUT" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
ev = next(e for e in v["cases"][0]["events"] if e["type"] == "send")
c = ev["ciphertext"]; ev["ciphertext"] = ("0" if c[0] != "0" else "1") + c[1:]
json.dump(v, open(sys.argv[2] + "/vectors-mutated-ct.json", "w"))
v = json.load(open(sys.argv[1]))
f = v["binding"]["positive_fetch"][0]
i = len(f) - 200
v["binding"]["positive_fetch"][0] = f[:i] + ("0" if f[i] != "0" else "1") + f[i + 1:]
json.dump(v, open(sys.argv[2] + "/vectors-mutated-sig.json", "w"))
PY
M1=true; M2=true
(cd "$HERE/python" && uv run --locked -q python check_vectors.py "$OUT/vectors-mutated-ct.json") > /dev/null 2>&1 && M1=false
(cd "$HERE/python" && uv run --locked -q python check_vectors.py "$OUT/vectors-mutated-sig.json") > /dev/null 2>&1 && M2=false
echo "{\"mutated_ciphertext_rejected\": $M1, \"mutated_mldsa_signature_rejected\": $M2}" > "$OUT/mutation.json"
log "draft-ietf-hpke-pq-05 KATs on the Rust HPKE (native, wasi, node)"
"$NATIVE" hpke-kat "$HPKE_VEC" > "$OUT/hpke-native.json" || fail "native hpke kat"
wasi --dir "$RUST/target::/t" "$WASI" hpke-kat "/t/$(basename "$HPKE_VEC")" > "$OUT/hpke-wasi.json" || fail "wasi hpke kat"
node "$HERE/node/run_wasm.mjs" "$NODEWASM" hpke-kat "$HPKE_VEC" > "$OUT/hpke-node.json" || fail "node hpke kat"
log "ACVP on the Rust ML-KEM layer (native + wasi)"
"$NATIVE" acvp "$KG" "$ED" > "$OUT/acvp-native.json" || fail "native ACVP"
wasi --dir "$ACVP_CACHE::/acvp" "$WASI" acvp /acvp/ML-KEM-keyGen-FIPS203.json /acvp/ML-KEM-encapDecap-FIPS203.json > "$OUT/acvp-wasi.json" || fail "wasi ACVP"

# ---- 7. Persistence: every step in a fresh process, four runtime schedules -----------------
log "persistence chains (native, wasi, alternating native/wasi, node fresh-instance)"
N=$("$NATIVE" persist-nsteps)
for mode in native wasi mixed; do
  d="$OUT/persist-$mode"; mkdir -p "$d"; : > "$OUT/persist-$mode.log"
  for k in $(seq 0 $((N - 1))); do
    if [[ $mode == native || ( $mode == mixed && $((k % 2)) == 0 ) ]]; then
      "$NATIVE" persist-step "$d" "$k" >> "$OUT/persist-$mode.log" || fail "persist $mode step $k"
    else
      wasi --dir "$d::/w" "$WASI" persist-step /w "$k" >> "$OUT/persist-$mode.log" || fail "persist $mode step $k"
    fi
  done
done
node "$HERE/node/run_wasm.mjs" "$NODEWASM" persist "$OUT/persist-node" > "$OUT/persist-node.log" || fail "persist node"

# ---- 8. Performance (V-022) -----------------------------------------------------------------
log "benchmarks"
"$NATIVE" bench 100 600 > "$OUT/bench-native.json"
wasi "$WASI" bench 100 600 > "$OUT/bench-wasi.json"
node "$HERE/node/run_wasm.mjs" "$NODEWASM" bench 100 600 > "$OUT/bench-node.json"

# ---- 9. Dependency licenses + tool versions ------------------------------------------------
cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-darwin > "$OUT/metadata.json"
{
  echo "rustc=$(rustc --version)"
  echo "cargo=$(cargo --version)"
  echo "wasmtime=$(wasmtime --version)"
  echo "node=$(node --version)"
  echo "uv=$(uv --version)"
  echo "python=$(cd "$HERE/python" && uv run --locked -q python --version)"
} > "$OUT/tools.txt"

# ---- 10. Assemble deterministic results -------------------------------------------------------
python3 "$HERE/assemble_results.py" "$OUT" "$HERE" "$RESULTS" "$LONG"
STATUS=$?
log "results: ${RESULTS#$ROOT/}"
exit $STATUS
