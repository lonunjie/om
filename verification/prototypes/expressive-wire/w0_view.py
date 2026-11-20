#!/usr/bin/env python3
"""Derive the W0-receiver view of the expressive vectors: only the vectors that carry a w0_expect, with the verdict and error
a decoder WITHOUT the slot overlay must give (a W0 receiver rejects ReactionSet key 3 and inline type 4 and ignores the
non-critical extension). Usage: w0_view.py VECTORS.json OUT.json
"""
import json
import sys

doc = json.load(open(sys.argv[1]))
out = []
for v in doc["vectors"]:
    if "w0_expect" in v:
        w = dict(v)
        w["expect"], w["error"] = v["w0_expect"], v["w0_error"]
        if w["expect"] == "reject":
            w.pop("value", None)
            w["derived"] = []
        out.append(w)
json.dump({"vectors": out}, open(sys.argv[2], "w"), indent=1)
print(f"{len(out)} W0 vectors", file=sys.stderr)
