#!/usr/bin/env bash
# Feasibility prototype for V-016 (Room half). Not product code. Runs every RoomModeration TLC config listed in
# verification/tla/models-v016-room-moderation.json; writes verification/results/v016-room-moderation-latest.json.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 "$here/../v008-permissions/tlc_gate.py" v016-room-moderation "$here/criteria.json"
