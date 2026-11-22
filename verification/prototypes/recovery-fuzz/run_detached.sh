#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Start the V-012 recovery and sanitized-expressive fuzz runs detached on the build host, from a copy of the tree that the
# heavy-run rsync (--delete) never touches. Run it through the heavy wrapper so the tree is synced first:
#   cd ~/dig/lonunjie && bin/lonunjie-heavy -- bash verification/prototypes/recovery-fuzz/run_detached.sh [run_fuzz.py args]
# Both suites run one after the other (one heavy run at a time). Re-running resumes: the copy never deletes work/, so the
# ledgers and corpora carry over. Log: ~/build/recovery-fuzz.log.
set -euo pipefail
SRC=$(cd "$(dirname "$0")/../../.." && pwd)
DST=${OM_FUZZ_DIR:-$HOME/build/recovery-fuzz-v012}
LOG=${OM_FUZZ_LOG:-$HOME/build/recovery-fuzz.log}
mkdir -p "$DST"
rsync -a --exclude work --exclude target --exclude .git --exclude node_modules --exclude dist "$SRC"/ "$DST"/
cd "$DST/verification/prototypes/recovery-fuzz"
nohup systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 -p CPUWeight=60 nice -n 10 \
  bash -c 'for suite in recovery expressive; do python3 run_fuzz.py --suite "$suite" --jobs 12 --slice-minutes 15 --write-summary "$@" || true; done' _ "$@" \
  >>"$LOG" 2>&1 &
echo "started the recovery and sanitized-expressive fuzz runs in $DST, log $LOG"
