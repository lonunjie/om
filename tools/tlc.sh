#!/usr/bin/env bash
# Run the pinned TLC model checker (verification/TLAPLUS_TOOL_PIN.md) on one TLA+ model.
#   tools/tlc.sh --preflight
#   tools/tlc.sh verification/tla/AccountControl.tla [extra TLC args]
#   CFG=verification/tla/AccountControl_live_owner.cfg tools/tlc.sh verification/tla/AccountControl.tla
# The preflight/bootstrap happens once in tools/run_tlc.py before any model loop, preventing repeated
# download attempts when the pinned JAR is absent or the network is unavailable.
set -euo pipefail
PIN_VERSION=1.7.4
PIN_SHA1=bee4a54f3ee3d4afc347c3240ec2d9e93b075104
CACHE=${OM_TOOL_CACHE:-$HOME/.cache/om}
JAR=$CACHE/tla2tools-$PIN_VERSION.jar

JAVA=${JAVA:-}
if [[ -z $JAVA ]]; then
  if [[ -x /opt/homebrew/opt/openjdk@21/bin/java ]]; then JAVA=/opt/homebrew/opt/openjdk@21/bin/java
  elif command -v java >/dev/null 2>&1; then JAVA=$(command -v java)
  else echo "TLC preflight: Java 11+ is required (set JAVA or install java)" >&2; exit 2; fi
fi
if ! "$JAVA" -version >/dev/null 2>&1; then
  echo "TLC preflight: Java is not executable: $JAVA" >&2; exit 2
fi

ensure_jar() {
  if [[ ! -f $JAR ]]; then
    command -v curl >/dev/null 2>&1 || { echo "TLC preflight: $JAR is absent and curl is unavailable" >&2; exit 2; }
    mkdir -p "$CACHE"
    rm -f "$JAR.tmp"
    echo "TLC preflight: pinned JAR is absent; attempting one download of tla2tools $PIN_VERSION" >&2
    if ! curl -fsSL -o "$JAR.tmp" "https://github.com/tlaplus/tlaplus/releases/download/v$PIN_VERSION/tla2tools.jar"; then
      rm -f "$JAR.tmp"
      echo "TLC preflight: download failed; provide the pinned JAR at $JAR or restore network access" >&2
      exit 2
    fi
    mv "$JAR.tmp" "$JAR"
  fi
  command -v shasum >/dev/null 2>&1 || { echo "TLC preflight: shasum is required to verify the pinned JAR" >&2; exit 2; }
  got=$(shasum -a 1 "$JAR" | cut -d' ' -f1)
  [[ $got == "$PIN_SHA1" ]] || { echo "tla2tools.jar SHA-1 $got != pinned $PIN_SHA1" >&2; exit 2; }
}

ensure_jar
if [[ ${1:-} == --preflight ]]; then
  echo "TLC preflight OK: $JAR (tla2tools $PIN_VERSION), JAVA=$JAVA"
  exit 0
fi

model=${1:?usage: tools/tlc.sh [--preflight] path/to/Model.tla [tlc args]}; shift
cfg=${CFG:-${model%.tla}.cfg}
cfg=$(cd "$(dirname "$cfg")" && pwd)/$(basename "$cfg")
# TLC's state queue can reach many GB: keep it in a per-run dir that is always removed.
meta=$(mktemp -d "${TMPDIR:-/tmp}/om-tlc.XXXXXX")
trap 'rm -rf "$meta"' EXIT INT TERM
cd "$(dirname "$model")"
"$JAVA" -XX:+UseParallelGC -cp "$JAR" tlc2.TLC -workers "${WORKERS:-auto}" -deadlock -metadir "$meta" \
  -config "$cfg" "$(basename "$model")" "$@"
