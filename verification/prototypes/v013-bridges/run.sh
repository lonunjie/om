#!/usr/bin/env bash
# Feasibility prototype for V-013. Not product code. Builds and runs the bridge explorer (bridgesim) and writes
# verification/results/v013-bridges-latest.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH
cd "$here/bridgesim"
cargo build --locked --release -q
out=$(mktemp)
trap 'rm -f "$out"' EXIT
set +e
./target/release/bridgesim >"$out"
rc=$?
set -e
python3 - "$out" "$here/criteria.json" "$root/verification/results/v013-bridges-latest.json" "$(rustc --version)" <<'PY'
import json, sys
sim = json.load(open(sys.argv[1])); crit = json.load(open(sys.argv[2]))
res = {"gates": crit["gates"], "criteria": crit["criteria"], "independence": crit["independence"],
       "tool_versions": {"rustc": sys.argv[4].split(" (")[0], "crates": "see verification/prototypes/v013-bridges/bridgesim/Cargo.lock"},
       "model": sim,
       "summary": {"all_ok": sim["all_ok"], "scenarios": len(sim["scenarios"]),
                   "fault_injections_caught": sum(1 for s in sim["scenarios"] if s["expect"] and s["ok"]),
                   "states_explored_correct_contract": sum(s["states"] for s in sim["scenarios"] if isinstance(s["states"], int)),
                   "gate_status_effect": "evidence_only_no_gate_automatically_passed"}}
open(sys.argv[3], "w").write(json.dumps(res, indent=1) + "\n")
print("v013:", res["summary"])
PY
exit $rc
