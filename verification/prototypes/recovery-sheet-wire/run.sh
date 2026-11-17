#!/usr/bin/env bash
# ADR-0200 recovery sheet string vectors (spec/protocol/recovery-sheet.md). Not CBOR, so no CDDL step.
# Feasibility evidence, not product code. The generator holds an independent encoder, parser, one-substitution correction and
# redirect rules; Core replays the file (crates/om-core/tests/recovery_sheet_vectors.rs, `cargo test -p om-core`).
#
#   ./run.sh                    regenerate and compare with verification/vectors/recovery-sheet-vectors.json.gz
#   ./run.sh --update-vectors   overwrite the committed vectors instead of checking them
set -euo pipefail
cd "$(dirname "$0")"
repo="$(cd ../../.. && pwd)"
update=0
for a in "$@"; do
  case "$a" in
    --update-vectors) update=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
mkdir -p build
committed="$repo/verification/vectors/recovery-sheet-vectors.json.gz"
python3 gen_recovery_sheet_vectors.py build/vectors.json >&2
gzip -9 -n -c build/vectors.json > build/vectors.json.gz
if [ "$update" = 1 ]; then
  cp build/vectors.json.gz "$committed"
elif ! gzip -dc "$committed" | cmp -s - build/vectors.json; then
  echo "regenerated vectors differ from $committed (rerun with --update-vectors if intended)" >&2
  echo "recovery-sheet-wire: FAIL" >&2
  exit 1
fi
echo "recovery-sheet-wire: PASS" >&2
