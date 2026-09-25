#!/usr/bin/env bash
# Feasibility prototype for V-003. Not product code. Not for production.
# TLC on every V-003 config (verification/tla/models-v003.json), replay of every regression vector on the
# Python reference machine, hypothesis fuzzing; writes verification/results/v003-sessions-latest.json.
#   run.sh          ~3 min (TLC ~2.5 min on 8 cores)
#   run.sh --long   more fuzz schedules (+~30 s)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$here/work"
cd "$here"
rc=0
python3 tlc_runs.py || rc=1
uv run --frozen --quiet python fuzzer.py work/fuzz.json "$@" || rc=1
python3 results.py "tla2tools=1.7.4" "hypothesis=6.140.2" \
  "python=$(uv run --frozen --quiet python -c 'import platform;print(platform.python_version())')" || rc=1
echo "v003-sessions: $( [[ $rc == 0 ]] && echo PASS || echo FAIL )"
exit $rc
