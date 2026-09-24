#!/usr/bin/env bash
# Feasibility prototype for V-014. Not product code. Builds the om-archive-v1 reference reader/writer, runs its unit
# tests, verifies the committed vectors byte-for-byte, runs the hostile corpus + import checks + mutation sweep, and
# writes verification/results/v014-archive-latest.json. `run.sh --long` runs a 200k-mutant sweep.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH
cd "$here/omarchive"
cargo test --locked --release -q >/dev/null
cargo build --locked --release -q
out=$(mktemp)
trap 'rm -f "$out"' EXIT
set +e
./target/release/omarchive-corpus --vectors "$here/vectors/om-archive-v1-vectors.json" "$@" >"$out"
rc=$?
set -e
python3 - "$out" "$here/criteria.json" "$root/verification/results/v014-archive-latest.json" "$(rustc --version)" <<'PY'
import json, sys
corpus = json.load(open(sys.argv[1])); crit = json.load(open(sys.argv[2]))
res = {"gates": crit["gates"], "criteria": crit["criteria"], "independence": crit["independence"],
       "tool_versions": {"rustc": sys.argv[4].split(" (")[0], "crates": "see verification/prototypes/v014-archive/omarchive/Cargo.lock"},
       "corpus": corpus,
       "summary": {"all_ok": corpus["all_ok"], "hostile_cases": corpus["hostile_cases"], "positive_cases": corpus["positive_cases"],
                   "import_checks": corpus["import_checks"], "vectors_match": corpus["vectors_match"],
                   "mutation_sweep": corpus["mutation_sweep"], "gate_status_effect": "evidence_only_no_gate_automatically_passed"}}
open(sys.argv[3], "w").write(json.dumps(res, indent=1) + "\n")
print(f"v014: all_ok={corpus['all_ok']} hostile={corpus['hostile_cases']} positive={corpus['positive_cases']} import_checks={corpus['import_checks']} vectors_match={corpus['vectors_match']} sweep={corpus['mutation_sweep']}")
PY
exit $rc
