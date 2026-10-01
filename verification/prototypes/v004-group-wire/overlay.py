#!/usr/bin/env python3
"""V-004 group wire freeze: build the W1 registry overlay directory.

Copies spec/registries/ and merges spec/registries/om-w1-event-bodies.json into om-numeric-fields.json (the merge
rule is stated in that file). Both strict codecs read the result through OM_REG. Nothing in the repository's
spec/registries is modified. Usage: overlay.py OUT_DIR
"""
import json
import shutil
import sys
from pathlib import Path

REG = Path(__file__).resolve().parents[3] / "spec" / "registries"


def build(dst: Path) -> Path:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(REG, dst)
    nf = json.loads((dst / "om-numeric-fields.json").read_text())
    ov = json.loads((dst / "om-w1-event-bodies.json").read_text())
    for target, table in ov["dispatch"].items():
        struct, field = target.split(".")
        fld = next(f for f in nf["structs"][struct]["fields"] if f["name"] == field)
        t = fld["type"]["dispatch"]["table"]
        for kind, name in table.items():
            if kind in t and t[kind] != name:
                raise SystemExit(f"overlay conflicts with {target}[{kind}]: {t[kind]} vs {name}")
            t[kind] = name
    (dst / "om-numeric-fields.json").write_text(json.dumps(nf, indent=2, ensure_ascii=False) + "\n")
    return dst


if __name__ == "__main__":
    print(build(Path(sys.argv[1])))
