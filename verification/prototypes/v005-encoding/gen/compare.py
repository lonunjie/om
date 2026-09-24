#!/usr/bin/env python3
"""Feasibility prototype for V-005. Not product code. Not for production.

compare.py VECTORS.json IMPL_NAME RESULTS.json [IMPL_NAME RESULTS.json ...]
Checks each implementation's per-vector results against the expected verdict,
error class, exact bytes (re-encode + encode-from-value) and derived
signature/hash inputs. Prints a JSON summary; exit 1 on any mismatch.
"""
import json
import sys


def check(vectors, res):
    bad = []
    for v in vectors:
        r = res.get(v["id"])
        if r is None:
            bad.append((v["id"], "missing result")); continue
        if r["verdict"] != v["expect"]:
            bad.append((v["id"], f"verdict {r['verdict']} ({r.get('error')}) != {v['expect']}")); continue
        if v["expect"] == "reject":
            if r["error"] != v["error"]:
                bad.append((v["id"], f"error {r['error']} != {v['error']}"))
            continue
        for k in ("reencode_equal", "value_encode_equal", "body_value_encode_equal"):
            if k in r and r[k] is not True and r[k] is not None:
                bad.append((v["id"], f"{k} false"))
        if r.get("reencode_equal") is not True:
            bad.append((v["id"], "reencode not byte-identical"))
        exp = [{k: x[k] for k in x if k in ("kind", "hex", "input_hex", "sha256")} for x in v["derived"]]
        got = [{k: x[k] for k in x if k in ("kind", "hex", "input_hex", "sha256")} for x in r.get("derived", [])]
        if exp != got:
            bad.append((v["id"], "derived signature/hash inputs differ"))
    return bad


def main(argv):
    vectors = json.load(open(argv[1]))["vectors"]
    out, ok = {}, True
    for name, path in zip(argv[2::2], argv[3::2]):
        bad = check(vectors, json.load(open(path)))
        ok &= not bad
        out[name] = {"vectors": len(vectors), "mismatches": len(bad), "first_mismatches": bad[:15]}
    print(json.dumps(out, indent=1))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
