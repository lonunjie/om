#!/usr/bin/env bash
# ADR-0192 device-link ceremony vectors. Feasibility evidence, not product code.
#   1. generate (pure Python, independent of the Rust code) and compare with verification/vectors/device-link-ceremony-vectors.json.gz
#   2. recompute every expected value with Node (node:crypto and a second CBOR encoder)
# The Rust replay is crates/om-core/tests/device_link_vectors.rs (cargo test -p om-core --test device_link_vectors).
#   ./run.sh                    check
#   ./run.sh --update-vectors   overwrite the committed vectors
set -euo pipefail
cd "$(dirname "$0")"
repo="$(cd ../../.. && pwd)"
committed="$repo/verification/vectors/device-link-ceremony-vectors.json.gz"
update=0
for a in "$@"; do
  case "$a" in
    --update-vectors) update=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
B=build
mkdir -p "$B"
rc=0
python3 gen_device_link_ceremony_vectors.py "$B/vectors.json"
gzip -9 -n -c "$B/vectors.json" > "$B/vectors.json.gz"
if [ "$update" = 1 ]; then
  cp "$B/vectors.json.gz" "$committed"
elif ! gzip -dc "$committed" | cmp -s - "$B/vectors.json"; then
  echo "regenerated vectors differ from $committed (rerun with --update-vectors if intended)" >&2
  rc=1
fi
node check_node.mjs "$B/vectors.json" || rc=1
echo "device-link-ceremony: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )" >&2
exit $rc
