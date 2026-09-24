#!/usr/bin/env bash
# Feasibility prototype for V-005. Not product code. Not for production.
# Usage: cddl_bundle.sh ROOT_RULE OUT_FILE  -> writes "root = ROOT_RULE" + all spec/cddl/*.cddl
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../../.." && pwd)"
{
  echo "root = $1"
  for f in common account keys events discovery delivery; do cat "$repo/spec/cddl/$f.cddl"; echo; done
} > "$2"
