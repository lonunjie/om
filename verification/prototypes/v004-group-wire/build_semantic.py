#!/usr/bin/env python3
"""Write semantic.json (cases) and add the wire-valid semantic objects to the object vector file. build_semantic.py OBJECT_VECTORS.json SEMANTIC.json"""
import json
import sys

import gen_semantic as gs
import gen_group_vectors as gv
import gen_vectors as g


def main(vec_path, sem_path):
    cases = gs.all_cases()
    doc = json.load(open(vec_path))
    g.VECTORS.clear()
    for name, suffix, value, bench in gs.extra_vectors():
        g.emit_positive(name, suffix, value, ["device"], bench)
    doc["vectors"] += g.VECTORS
    json.dump(doc, open(vec_path, "w"), indent=1)
    json.dump({"cases": cases}, open(sem_path, "w"), indent=1)
    print(f"{len(cases)} semantic cases, {len(g.VECTORS)} added object vectors", file=sys.stderr)


if __name__ == "__main__":
    main(*sys.argv[1:3])
