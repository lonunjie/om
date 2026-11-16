#!/usr/bin/env bash
# ADR-0194 direct-transfer receipt vectors (the HMAC receipt construction and the lan `object_receipt` frame).
# Feasibility evidence, not product code. Writes build/ and checks verification/vectors/direct-transfer-receipt-vectors.json.gz.
#
#   1. generate the vectors (pure Python, independent of the Rust code) and compare them with the committed file
#   2. node:crypto and a second CBOR encoder recompute every construction value (check_node.mjs)
#   3. the same script runs a second strict decoder over every LanFrameV1 frame vector (the V-005 prototype codecs take
#      object types only; LanFrameV1 is a registered struct, so the product codec replays the frames in Rust:
#      crates/om-codec/tests/lan_receipt_vectors.rs)
#   4. two CDDL tools (Rust cddl 0.10.7, Ruby cddl 0.12.14) agree with spec/cddl/lan.cddl
# The Rust replay of the constructions is crates/om-core/tests/direct_receipt_vectors.rs.
#
#   ./run.sh                    everything
#   ./run.sh --no-cddl          skip step 4 (slow)
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
committed="$repo/verification/vectors/direct-transfer-receipt-vectors.json.gz"

echo "== vectors" >&2
python3 gen_direct_receipt_vectors.py "$B/vectors.json" >&2
gzip -9 -n -c "$B/vectors.json" > "$B/vectors.json.gz"
if [ "$update" = 1 ]; then
  cp "$B/vectors.json.gz" "$committed"
elif ! gzip -dc "$committed" | cmp -s - "$B/vectors.json"; then
  echo "regenerated vectors differ from $committed (rerun with --update-vectors if intended)" >&2
  rc=1
fi

echo "== node recompute" >&2
node check_node.mjs "$B/vectors.json" || rc=1

rm -f "$B/cddl.json"
if [ "$cddl" = 1 ]; then
  echo "== cddl (rust + ruby tools)" >&2
  python3 cddl_check_lan_receipt.py "$B/vectors.json" "$B" - "$reg" > "$B/cddl.json" || rc=1
fi

echo "direct-receipt: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )" >&2
exit $rc
