#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Start the V-012 AFL++ run detached on the build host, from a copy of the tree that the heavy-run rsync (--delete) never touches.
# Run it through the heavy wrapper so the tree is synced first:
#   cd ~/dig/lonunjie && bin/lonunjie-heavy -- bash verification/prototypes/expressive-fuzz/run_afl_detached.sh [run_fuzz.py args]
# Re-running resumes: the copy never deletes work/, so the ledger and queues carry over. Log: ~/build/afl-v012.log.
set -euo pipefail
SRC=$(cd "$(dirname "$0")/../../.." && pwd)
DST=${OM_AFL_DIR:-$HOME/build/afl-v012}
LOG=${OM_AFL_LOG:-$HOME/build/afl-v012.log}
mkdir -p "$DST"
rsync -a --exclude work --exclude target --exclude .git --exclude node_modules --exclude dist "$SRC"/ "$DST"/
cd "$DST/verification/prototypes/expressive-fuzz"
nohup systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 -p CPUWeight=60 \
  nice -n 10 python3 run_fuzz.py --engine afl --jobs 12 --slice-minutes 15 --write-summary "$@" >>"$LOG" 2>&1 &
echo "started AFL++ run in $DST, log $LOG"
