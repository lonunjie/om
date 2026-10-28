#!/usr/bin/env python3
"""Dump the V-002 exhaustive authorization enumeration as JSON lines for differential testing of the Rust verifier.

Each line: {"parent", "child", "context", "expect"}. Usage:
    python3 verification/bounded/acs_dump_cases.py /tmp/acs-cases.jsonl
    OM_ACS_EXHAUSTIVE=/tmp/acs-cases.jsonl cargo test -p om-core --test acs_authorization
The enumeration and the reference `authorize` are imported unchanged from acs_authorization_table.py.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import acs_authorization_table as t  # noqa: E402


def main(out: str) -> int:
    n = 0
    with open(out, "w", encoding="utf-8") as f:
        for p in t.parents():
            for _cls, _tgt, c in t.children(p):
                for ctx in t.contexts(p):
                    verdict, _why = t.authorize(p, c, ctx)
                    f.write(json.dumps({"parent": p, "child": c, "context": ctx, "expect": verdict}, sort_keys=True) + "\n")
                    n += 1
    print(f"wrote {n} cases to {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
