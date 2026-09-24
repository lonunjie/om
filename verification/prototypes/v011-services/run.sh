#!/usr/bin/env bash
# Feasibility prototype for V-011. Not product code. (a) runs every ServiceAuthority TLC config in
# verification/tla/models-v011.json; (b) builds the Rust capability broker and runs the prompt-injection corpus plus a
# random-model sweep. Writes verification/results/v011-services-latest.json. `run.sh --long` = 100k-invocation sweep.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH
tlc_rc=0
python3 "$here/../v008-permissions/tlc_gate.py" v011-services "$here/criteria.json" models-v011.json || tlc_rc=$?
cd "$here/broker"
cargo build --locked --release -q
out=$(mktemp)
trap 'rm -f "$out"' EXIT
set +e
./target/release/capbroker "$here/injection-corpus.json" "$@" >"$out"
rc=$?
set -e
python3 - "$out" "$root/verification/results/v011-services-latest.json" "$(rustc --version)" <<'PY'
import json, sys
b = json.load(open(sys.argv[1])); p = sys.argv[2]; res = json.load(open(p))
res["tool_versions"]["rustc"] = sys.argv[3].split(" (")[0]
res["tool_versions"]["crates"] = "see verification/prototypes/v011-services/broker/Cargo.lock"
res["broker"] = b
res["summary"].update({"broker_all_ok": b["all_ok"], "injection_cases": b["cases"], "attack_cases": b["attack_cases"],
                       "categories": b["categories"], "random_model_sweep": {k: v for k, v in b["random_model_sweep"].items() if k != "oracle_violations"},
                       "random_model_oracle_violations": len(b["random_model_sweep"]["oracle_violations"])})
open(p, "w").write(json.dumps(res, indent=1) + "\n")
print("v011 broker:", {k: res["summary"][k] for k in ("broker_all_ok", "injection_cases", "attack_cases", "random_model_oracle_violations")})
PY
[[ $tlc_rc -eq 0 && $rc -eq 0 ]]
