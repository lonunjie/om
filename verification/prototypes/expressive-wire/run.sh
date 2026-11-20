#!/usr/bin/env bash
# ADR-0252 expressive-media wire vectors (ExpressiveAssetRef and its three carriers, ContentPackRelease 45,
# ContentPackControlState 46). Feasibility evidence, not product code. Writes build/ and checks
# verification/vectors/expressive-wire-vectors.json.gz.
#
#   1. registry slot overlay (ReactionSet key 3, inline type 4) and additivity of the registry changes against the pre-freeze commit
#   2. generate the vectors and compare them with the committed file
#   3. two independently written strict codecs (Rust v005, Node harness) agree on every object vector under the overlay
#   4. the same two codecs, WITHOUT the overlay, reject every slot vector (a W0 receiver fed the W1 body) and accept the rest
#   5. the frozen W0 corpus still passes unchanged on both codecs with the edited registries
#   6. two semantic checkers (Python checks.py, the Rust test crates/om-codec/tests/expressive_vectors.rs) agree on every
#      cross-field, claims, budget, lineage, pack_ref and emission case (the Rust test also runs the product codec)
#   7. two CDDL tools (Rust cddl 0.10.7, Ruby cddl 0.12.14) agree with spec/cddl/expressive.cddl
#
#   ./run.sh                    everything
#   ./run.sh --no-cddl          skip step 7 (slow)
#   ./run.sh --update-vectors   overwrite the committed vectors instead of checking them
# Needs what verification/prototypes/v005-encoding/run.sh needs (python3, node, rustup, ruby with the cddl gem in .tools).
set -euo pipefail
cd "$(dirname "$0")"
repo="$(cd ../../.. && pwd)"
v5="$repo/verification/prototypes/v005-encoding"
cddl=1 update=0
for a in "$@"; do
  case "$a" in
    --no-cddl) cddl=0 ;;
    --update-vectors) update=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
if command -v rustup >/dev/null; then
  PATH="$(cd "$v5/rust" && dirname "$(rustup which cargo)"):$PATH"; export PATH
fi
B=build
mkdir -p "$B"
overlay="$PWD/$B/registries"
rc=0
committed="$repo/verification/vectors/expressive-wire-vectors.json.gz"

echo "== overlay + additivity" >&2
python3 overlay.py "$overlay" >/dev/null
python3 additive.py > "$B/additive.json" || rc=1

echo "== vectors" >&2
python3 gen_expressive_vectors.py "$B/vectors.json" >&2
gzip -9 -n -c "$B/vectors.json" > "$B/vectors.json.gz"
if [ "$update" = 1 ]; then
  cp "$B/vectors.json.gz" "$committed"
elif ! gzip -dc "$committed" | cmp -s - "$B/vectors.json"; then
  echo "regenerated vectors differ from $committed (rerun with --update-vectors if intended)" >&2
  rc=1
fi

echo "== build codec" >&2
(cd "$v5/rust" && DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib" cargo build --release -q)
v005="$v5/rust/target/release/v005"

echo "== codecs (W1 slot overlay)" >&2
OM_REG="$overlay" "$v005" vectors "$B/vectors.json" > "$B/rust.json"
OM_REG="$overlay" node "$repo/verification/cbor/node_strict_harness.mjs" "$B/vectors.json" > "$B/node.json"
python3 "$v5/gen/compare.py" "$B/vectors.json" rust "$B/rust.json" node "$B/node.json" > "$B/compare.json" || rc=1

echo "== W0 receivers (no overlay): the slots are refused" >&2
python3 w0_view.py "$B/vectors.json" "$B/w0-view.json" >&2
OM_REG="$repo/spec/registries" "$v005" vectors "$B/w0-view.json" > "$B/rust-w0.json"
OM_REG="$repo/spec/registries" node "$repo/verification/cbor/node_strict_harness.mjs" "$B/w0-view.json" > "$B/node-w0.json"
python3 "$v5/gen/compare.py" "$B/w0-view.json" rust "$B/rust-w0.json" node "$B/node-w0.json" > "$B/compare-w0.json" || rc=1

echo "== frozen W0 corpus unchanged under the edited registries" >&2
gzip -dc "$repo/verification/vectors/v005-w0-object-vectors.json.gz" > "$B/w0-object-vectors.json"
python3 "$v5/gen/gen_vectors.py" "$B/w0-regenerated.json" >&2
cmp -s "$B/w0-object-vectors.json" "$B/w0-regenerated.json" || { echo "W0 vectors regenerate differently" >&2; rc=1; }
"$v005" vectors "$B/w0-object-vectors.json" > "$B/w0-rust.json"
node "$repo/verification/cbor/node_strict_harness.mjs" "$B/w0-object-vectors.json" > "$B/w0-node.json"
python3 "$v5/gen/compare.py" "$B/w0-object-vectors.json" rust "$B/w0-rust.json" node "$B/w0-node.json" > "$B/w0-compare.json" || rc=1

echo "== semantic checker (python)" >&2
python3 check_vectors.py "$B/vectors.json" > "$B/semantic-py.json" || rc=1

rm -f "$B/cddl.json"
if [ "$cddl" = 1 ]; then
  echo "== cddl (rust + ruby tools)" >&2
  python3 cddl_check_expressive.py "$B/vectors.json" "$B" "$v005" "$overlay" > "$B/cddl.json" || rc=1
fi

echo "expressive-wire: $( [[ $rc == 0 ]] && echo PASS || echo FAIL ) (the Rust semantic and product-codec test: cargo test -p om-codec --test expressive_vectors)" >&2
exit $rc
