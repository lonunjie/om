#!/usr/bin/env python3
"""check_semantic.py SEMANTIC.json IMPL_NAME RESULTS.json [IMPL_NAME RESULTS.json ...]: compare each checker with the
expectation (verdict and reason; for scenarios every key of `final`) and, pairwise, with each other. Exit 1 on any difference."""
import json
import sys


def main(argv):
    cases = json.load(open(argv[1]))["cases"]
    impls = {n: json.load(open(p)) for n, p in zip(argv[2::2], argv[3::2])}
    summary, ok = {}, True
    for n, res in impls.items():
        bad = []
        for c in cases:
            r = res.get(c["id"])
            if r is None:
                bad.append((c["id"], "missing")); continue
            if c["kind"] == "scenario":
                for k, v in c["final"].items():
                    if r.get(k) != v:
                        bad.append((c["id"], f"{k}: expected {v!r} got {r.get(k)!r}"))
            elif r["verdict"] != c["expect"] or r["reason"] != c["reason"]:
                bad.append((c["id"], f"expected {c['expect']}/{c['reason']} got {r['verdict']}/{r['reason']}"))
        summary[n] = {"cases": len(cases), "mismatches": len(bad), "first": bad[:10]}
        ok &= not bad
    names = list(impls)
    if len(names) > 1:
        diff = [c["id"] for c in cases if len({json.dumps(impls[n].get(c["id"]), sort_keys=True) for n in names}) > 1 and c["kind"] != "scenario"]
        summary["pairwise_differences"] = diff
        ok &= not diff
    print(json.dumps(summary, indent=1))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
