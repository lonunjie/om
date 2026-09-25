#!/usr/bin/env bash
# Feasibility prototype for V-008. Not product code. Runs every ScopedPermissions TLC config listed in
# verification/tla/models-v008-permissions.json and writes verification/results/v008-permissions-latest.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 "$here/tlc_gate.py" v008-permissions "$here/criteria.json"
