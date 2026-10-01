#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Feasibility prototype for V-004. Not product code. Not for production.
#
# Runs the OpenMLS 0.9.0 ordering probe natively and as wasm32-unknown-unknown under Node (V8), requires the
# two outputs to be byte-identical, type-checks the same crate for iOS and Android, and writes
# deterministic results to verification/results/v004-mls-ordering-latest.json.
#
#   run.sh        ~1 min after the first build (first run downloads crates and wasm-bindgen-cli)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
RUST="$HERE/rust"
OUT="$RUST/target/v004-run"
RESULTS="$ROOT/verification/results/v004-mls-ordering-latest.json"
TOOLS="${OM_TOOL_CACHE:-$HOME/.cache/om}/tools"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$TOOLS/bin:$PATH"
log() { printf '[v004] %s\n' "$*" >&2; }
fail() { printf '[v004] FAIL: %s\n' "$*" >&2; exit 1; }
for t in cargo rustup node python3; do command -v "$t" >/dev/null || fail "missing tool: $t"; done
rm -rf "$OUT" && mkdir -p "$OUT"
cd "$RUST"

log "native build + run"
cargo build --release --locked 2>&1 | tail -1 >&2
./target/release/om-v004-mls-ordering > "$OUT/native.json"

log "wasm32-unknown-unknown build"
cargo build --release --locked --lib --target wasm32-unknown-unknown 2>&1 | tail -1 >&2
WBV="$(awk '/name = "wasm-bindgen"$/{getline; gsub(/[^0-9.]/,"",$3); print $3; exit}' Cargo.lock)"
if ! command -v wasm-bindgen >/dev/null || [[ "$(wasm-bindgen --version | awk '{print $2}')" != "$WBV" ]]; then
  log "installing wasm-bindgen-cli $WBV into $TOOLS"
  cargo install wasm-bindgen-cli --version "$WBV" --locked --root "$TOOLS" 2>&1 | tail -1 >&2
fi
wasm-bindgen --target nodejs --out-dir "$OUT/wb" target/wasm32-unknown-unknown/release/om_v004_mls_ordering.wasm
cat > "$OUT/run.cjs" <<'JS'
const m = require('./wb/om_v004_mls_ordering.js');
process.stdout.write(m.run_all_json() + "\n");
JS
node "$OUT/run.cjs" > "$OUT/node.json"
cmp -s "$OUT/native.json" "$OUT/node.json" || fail "native and wasm32/Node outputs differ"

log "iOS and Android type-check"
for tgt in aarch64-apple-ios aarch64-linux-android; do
  rustup target add "$tgt" >/dev/null 2>&1 || true
  cargo check --release --locked --lib --target "$tgt" 2>&1 | tail -1 | grep -q Finished || fail "cargo check $tgt"
done

python3 - "$OUT/native.json" "$RESULTS" "$(node --version)" "$(rustc --version)" "$WBV" <<'PY'
import json, sys
native, results, node_v, rustc_v, wbv = sys.argv[1:6]
r = json.load(open(native))
def all_true(x):
    if isinstance(x, bool): return x
    if isinstance(x, dict): return all(all_true(v) for k, v in x.items() if k not in ("library", "ciphersuite", "window", "survivor_roster_size", "decrypts_by_lag_1_to_4"))
    return True
if not all_true(r): raise SystemExit("a probe check is false")
assert r["s5_past_epoch_window"]["decrypts_by_lag_1_to_4"] == [True, True, False, False]
out = {
  "gates": ["V-004"],
  "criteria": [
    {"gate": "V-004", "criterion": "candidate MLS library demonstrated to support the required external commit ordering on target runtimes",
     "status": "met",
     "evidence": "verification/prototypes/v004-mls-ordering/rust/src/lib.rs; verification/results/v004-mls-ordering-latest.json",
     "note": "OpenMLS 0.9.0 holds a prepared commit un-merged until the application orders it, discards the losing candidate (clear_pending_commit) and accepts the winner, on macOS arm64 native and wasm32-unknown-unknown under Node/V8 with byte-identical results; the same crate type-checks for aarch64-apple-ios and aarch64-linux-android (compiled, not run). A prepared commit survives a restart. External commits from a device outside the roster are staged and can be refused before merge."},
    {"gate": "V-004", "criterion": "two disconnected partitions independently create different direct reducing Commits from the same MLS epoch after sequencing_timeout; reconnection converges without silent key/state continuation on the losing branch",
     "status": "met",
     "evidence": "verification/prototypes/v004-mls-ordering/rust/src/lib.rs (s2_partition_fork_and_readd); verification/tla/GroupCommit.tla",
     "note": "Library level: after both partitions merge their own commit the branches cannot cross-process, the winning branch cannot read a message sent on the losing branch, the fork-resolution re-add (OpenMLS fork-resolution feature) re-joins the in-roster loser to the winning chain, a removed device cannot, and a rebased reduction converges. Protocol level: TLC checks the same in GroupCommit.tla under crash and equivocation."}
  ],
  "independence": "Written by a Claude agent from RFC 9420 and the OpenMLS 0.9.0 public API, and from GROUP_COMMIT_AND_STATE_CONVERGENCE.md and ADR-0163. Single-vendor review (ADR-0167); not an independent implementation.",
  "tool_versions": {"openmls": "0.9.0 (rust_crypto 0.6.0, basic_credential 0.6.0, memory storage 0.6.0, feature fork-resolution)", "rustc": rustc_v, "node": node_v, "wasm-bindgen": wbv},
  "summary": {"scenarios": ["s1_ordered_same_epoch_race", "s1b_mutual_removal", "s2_partition_fork_and_readd", "s3_external_commit_policy", "s4_crash_restart", "s5_past_epoch_window"],
              "runtimes_identical": ["native aarch64-apple-darwin", "wasm32-unknown-unknown (Node/V8)"],
              "type_checked_only": ["aarch64-apple-ios", "aarch64-linux-android"],
              "probe": r},
}
open(results, "w").write(json.dumps(out, indent=2, sort_keys=True) + "\n")
PY
log "PASS: wrote $RESULTS"
