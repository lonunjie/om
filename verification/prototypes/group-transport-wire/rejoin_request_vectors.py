#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""ADR-0207 wire feasibility check using the independent Rust and Node codecs.

Run after building verification/prototypes/v005-encoding/rust in release mode.
The legacy overlay and comparison files are temporary. This checks encoding only.
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from gen_group_transport_vectors import REPO, emit_rejoin_vectors, g


V5 = REPO / "verification/prototypes/v005-encoding"
W1_OVERLAY = REPO / "verification/prototypes/v004-group-wire/overlay.py"
CODECS = {
    "rust": [V5 / "rust/target/release/v005", "vectors"],
    "node": ["node", REPO / "verification/cbor/node_strict_harness.mjs"],
}
KIND = 18
OBJECT = "ConversationEvent"


def remove_rejoin_kind(registry):
    path = registry / "om-numeric-fields.json"
    fields = json.loads(path.read_text())
    kind = str(KIND)
    del fields["enums"]["event_kind"][kind]
    structs = fields["structs"]
    body_field = next(field for field in structs[OBJECT]["fields"] if field["name"] == "body")
    del body_field["type"]["dispatch"]["table"][kind]
    del structs["GroupRejoinRequestBody"]
    path.write_text(json.dumps(fields))


def compare(work, name, registry, cases):
    vector_path = work / f"{name}.json"
    vector_path.write_text(json.dumps({"vectors": cases}))
    args = []
    for codec, command in CODECS.items():
        with (work / f"{name}-{codec}.json").open("w") as result:
            subprocess.run([*command, vector_path], env={**os.environ, "OM_REG": str(registry)},
                           stdout=result, check=True)
        args.extend([codec, result.name])
    print(name, flush=True)
    subprocess.run([sys.executable, V5 / "gen/compare.py", vector_path, *args], check=True)


def main():
    emit_rejoin_vectors()
    cases = g.VECTORS
    with tempfile.TemporaryDirectory(prefix="om-rejoin-wire-") as tmp:
        work = Path(tmp)
        w1 = work / "w1"
        subprocess.run([sys.executable, W1_OVERLAY, w1], stdout=subprocess.DEVNULL, check=True)
        unsupported = [{**cases[0], "expect": "reject", "error": "value_range"}]
        compare(work, "legacy-w0", REPO / "spec/registries", unsupported)
        compare(work, "w1-rejoin", w1, cases)
        remove_rejoin_kind(w1)
        compare(work, "legacy-w1", w1, unsupported)


if __name__ == "__main__":
    main()
