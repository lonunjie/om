#!/usr/bin/env bash
# Run the pinned TLC model checker (verification/TLAPLUS_TOOL_PIN.md) on one TLA+ model.
#   tools/tlc.sh verification/tla/AccountControl.tla [extra TLC args]
#   CFG=verification/tla/AccountControl_live_owner.cfg tools/tlc.sh verification/tla/AccountControl.tla
# Fetches tla2tools.jar v1.7.4 into $OM_TOOL_CACHE (default ~/.cache/om) and refuses a jar whose
# SHA-1 differs from the pin. Needs Java 11+ (JAVA or java on PATH or Homebrew openjdk@21).
set -euo pipefail
PIN_VERSION=1.7.4
PIN_SHA1=bee4a54f3ee3d4afc347c3240ec2d9e93b075104
CACHE=${OM_TOOL_CACHE:-$HOME/.cache/om}
JAR=$CACHE/tla2tools-$PIN_VERSION.jar

model=${1:?usage: tools/tlc.sh path/to/Model.tla [tlc args]}; shift
cfg=${CFG:-${model%.tla}.cfg}
cfg=$(cd "$(dirname "$cfg")" && pwd)/$(basename "$cfg")

if [[ ! -f $JAR ]]; then
  mkdir -p "$CACHE"
  curl -fsSL -o "$JAR.tmp" "https://github.com/tlaplus/tlaplus/releases/download/v$PIN_VERSION/tla2tools.jar"
  mv "$JAR.tmp" "$JAR"
fi
got=$(shasum -a 1 "$JAR" | cut -d' ' -f1)
[[ $got == "$PIN_SHA1" ]] || { echo "tla2tools.jar SHA-1 $got != pinned $PIN_SHA1" >&2; exit 1; }

JAVA=${JAVA:-}
if [[ -z $JAVA ]]; then
  if [[ -x /opt/homebrew/opt/openjdk@21/bin/java ]]; then JAVA=/opt/homebrew/opt/openjdk@21/bin/java
  else JAVA=java; fi
fi
meta=$(mktemp -d)
trap 'rm -rf "$meta"' EXIT
cd "$(dirname "$model")"
exec "$JAVA" -XX:+UseParallelGC -cp "$JAR" tlc2.TLC -workers "${WORKERS:-auto}" -deadlock -metadir "$meta" \
  -config "$cfg" "$(basename "$model")" "$@"
