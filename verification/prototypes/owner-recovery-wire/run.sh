#!/usr/bin/env bash
# Replay ADR-0203 wire fixtures with both independent V-005 codecs.
set -euo pipefail
cd "$(dirname "$0")"
repo="$(cd ../../.. && pwd)"
v005="$repo/verification/prototypes/v005-encoding"
fixture="$repo/verification/vectors/owner-recovery-vectors.json.gz"
scratch="$(mktemp -d)"
trap 'find "$scratch" -depth -delete' EXIT
python3 gen_owner_recovery_vectors.py "$scratch/regenerated.json.gz"
if [[ ${1-} == --update-vectors ]]; then
  cp "$scratch/regenerated.json.gz" "$fixture"
fi
gzip -dc "$fixture" > "$scratch/vectors.json"
gzip -dc "$scratch/regenerated.json.gz" | cmp - "$scratch/vectors.json"
(cd "$v005/rust" && DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib" cargo build --release --locked --offline --quiet)
export OM_REG="$repo/spec/registries"
"$v005/rust/target/release/v005" vectors "$scratch/vectors.json" > "$scratch/rust.json"
node "$repo/verification/cbor/node_strict_harness.mjs" "$scratch/vectors.json" > "$scratch/node.json"
python3 "$v005/gen/compare.py" "$scratch/vectors.json" rust "$scratch/rust.json" node "$scratch/node.json"
node check_transcript.mjs "$scratch/vectors.json"
RUBYOPT="${RUBYOPT:-} -rset" python3 check_cddl.py "$scratch/vectors.json" "$scratch"
