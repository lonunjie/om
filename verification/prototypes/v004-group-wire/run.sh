#!/usr/bin/env bash
# V-004 group wire freeze (ADR-0176). Feasibility evidence, not product code.
#
# Reproduces the W1 `groups` wire evidence and writes verification/results/v004-group-wire-latest.json:
#   1. registry overlay (W1 event bodies) and additivity of the registry changes against the pre-freeze commit
#   2. object vectors (positive + negative) for GroupControlState/Intent/CommitBinding/HistoryBundle and typed pin/poll bodies
#   3. two independently written strict codecs (Rust, Node) agree on every vector, under OM_REG=overlay
#   4. the frozen W0 corpus still passes unchanged on both codecs with the default registries
#   5. two semantic checkers (Python, Node) agree on the cross-field and GroupHistoryBundle admission cases
#   6. two CDDL tools (Rust cddl 0.10.7, Ruby cddl 0.12.14) agree with the vectors
#
#   ./run.sh              everything
#   ./run.sh --no-cddl    skip step 6 (about 10 minutes)
#   ./run.sh --update-vectors   overwrite the committed gzipped vectors instead of checking them
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

echo "== overlay + additivity" >&2
python3 overlay.py "$overlay" >/dev/null
python3 additive.py > "$B/additive.json" || rc=1

echo "== vectors" >&2
python3 gen_group_vectors.py "$B/object-vectors.json" >&2
python3 build_semantic.py "$B/object-vectors.json" "$B/semantic.json" >&2
for f in object-vectors semantic; do
  committed="$repo/verification/vectors/v004-group-wire-$f.json.gz"
  gzip -9 -n -c "$B/$f.json" > "$B/$f.json.gz"
  if [ "$update" = 1 ]; then
    cp "$B/$f.json.gz" "$committed"
  elif ! gzip -dc "$committed" | cmp -s - "$B/$f.json"; then
    echo "regenerated $f differs from $committed (rerun with --update-vectors if intended)" >&2
    rc=1
  fi
done

echo "== build codec" >&2
(cd "$v5/rust" && DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib" cargo build --release -q)
v005="$v5/rust/target/release/v005"

echo "== codecs (W1 overlay)" >&2
OM_REG="$overlay" "$v005" vectors "$B/object-vectors.json" > "$B/rust.json"
OM_REG="$overlay" node "$repo/verification/cbor/node_strict_harness.mjs" "$B/object-vectors.json" > "$B/node.json"
python3 "$v5/gen/compare.py" "$B/object-vectors.json" rust "$B/rust.json" node "$B/node.json" > "$B/compare.json" || rc=1

echo "== W0 decoders (no overlay): typed W1 event negatives are opaque to W0" >&2
"$v005" vectors "$B/object-vectors.json" > "$B/rust-w0.json"
node "$repo/verification/cbor/node_strict_harness.mjs" "$B/object-vectors.json" > "$B/node-w0.json"

echo "== frozen W0 corpus unchanged under the edited registries" >&2
gzip -dc "$repo/verification/vectors/v005-w0-object-vectors.json.gz" > "$B/w0-object-vectors.json"
python3 "$v5/gen/gen_vectors.py" "$B/w0-regenerated.json" >&2
cmp -s "$B/w0-object-vectors.json" "$B/w0-regenerated.json" || { echo "W0 vectors regenerate differently" >&2; rc=1; }
"$v005" vectors "$B/w0-object-vectors.json" > "$B/w0-rust.json"
node "$repo/verification/cbor/node_strict_harness.mjs" "$B/w0-object-vectors.json" > "$B/w0-node.json"
python3 "$v5/gen/compare.py" "$B/w0-object-vectors.json" rust "$B/w0-rust.json" node "$B/w0-node.json" > "$B/w0-compare.json" || rc=1

echo "== semantic checkers (python, node)" >&2
python3 semantic.py "$B/semantic.json" > "$B/semantic-py.json"
node semantic.mjs "$B/semantic.json" > "$B/semantic-node.json"
python3 check_semantic.py "$B/semantic.json" python "$B/semantic-py.json" node "$B/semantic-node.json" > "$B/semantic-compare.json" || rc=1

rm -f "$B/cddl.json"
if [ "$cddl" = 1 ]; then
  echo "== cddl (rust + ruby tools)" >&2
  python3 cddl_check_group.py "$B/object-vectors.json" "$B" "$v005" "$overlay" > "$B/cddl.json" || rc=1
fi

python3 assemble.py "$B" "$repo/verification/results/v004-group-wire-latest.json" || rc=1
echo "v004-group-wire: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )" >&2
exit $rc
