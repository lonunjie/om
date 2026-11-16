#!/usr/bin/env bash
# ADR-0190 recovery-notice wire vectors (RecoveryNotice 34, plus the RecoveryDossierEntry struct).
# Feasibility evidence, not product code. Writes build/ and checks verification/vectors/recovery-notice-vectors.json.gz.
#
#   1. generate the vectors and compare them with the committed file
#   2. two independently written strict codecs (Rust v005, Node harness) agree on every vector (the type is in the
#      registry itself, so no overlay is needed: OM_REG points at spec/registries)
#   3. two CDDL tools (Rust cddl 0.10.7, Ruby cddl 0.12.14) agree with spec/cddl/recovery-notice.cddl
#
#   ./run.sh                    everything
#   ./run.sh --no-cddl          skip step 3 (slow)
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
reg="$repo/spec/registries"
rc=0
committed="$repo/verification/vectors/recovery-notice-vectors.json.gz"

echo "== vectors" >&2
python3 gen_recovery_notice_vectors.py "$B/vectors.json" >&2
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

echo "== codecs" >&2
OM_REG="$reg" "$v005" vectors "$B/vectors.json" > "$B/rust.json"
OM_REG="$reg" node "$repo/verification/cbor/node_strict_harness.mjs" "$B/vectors.json" > "$B/node.json"
python3 "$v5/gen/compare.py" "$B/vectors.json" rust "$B/rust.json" node "$B/node.json" > "$B/compare.json" || rc=1

rm -f "$B/cddl.json"
if [ "$cddl" = 1 ]; then
  echo "== cddl (rust + ruby tools)" >&2
  python3 cddl_check_recovery_notice.py "$B/vectors.json" "$B" "$v005" "$reg" > "$B/cddl.json" || rc=1
fi

echo "recovery-notice-wire: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )" >&2
exit $rc
