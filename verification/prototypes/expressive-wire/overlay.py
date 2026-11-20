#!/usr/bin/env python3
"""ADR-0252 expressive-media wire: build the W1 slot registry overlay directory.

Copies spec/registries/ and merges spec/registries/om-w1-expressive-slots.json into om-numeric-fields.json (the merge rule
is stated in that file: fields appended, union variants added, 'enum_values' made W1-valid). Both strict prototype codecs
read the result through OM_REG. Nothing in the repository's spec/registries is modified. Usage: overlay.py OUT_DIR
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
    ov = json.loads((dst / "om-w1-expressive-slots.json").read_text())
    for struct, add in ov["struct_fields"].items():
        fields = nf["structs"][struct]["fields"]
        for f in add:
            if any(x["key"] == f["key"] for x in fields):
                raise SystemExit(f"overlay field {struct}.{f['key']} is already assigned")
            fields.append(f)
    for union, add in ov["union_variants"].items():
        variants = nf["structs"][union]["union"]["variants"]
        for value, name in add.items():
            if value in variants and variants[value] != name:
                raise SystemExit(f"overlay conflicts with {union}[{value}]: {variants[value]} vs {name}")
            variants[value] = name
    for enum, vals in ov["enum_values"].items():
        waves = nf["enum_value_waves"][enum]
        for v in vals:
            if waves.get(v) != "W1":
                raise SystemExit(f"overlay enables {enum}[{v}] but it is not marked W1 in enum_value_waves")
            del waves[v]
        if not waves:
            del nf["enum_value_waves"][enum]
    (dst / "om-numeric-fields.json").write_text(json.dumps(nf, indent=2, ensure_ascii=False) + "\n")
    return dst


if __name__ == "__main__":
    print(build(Path(sys.argv[1])))
