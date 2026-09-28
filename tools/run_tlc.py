#!/usr/bin/env python3
"""Run every TLA+ config listed in verification/tla/models*.json with the pinned TLC.

Checks each outcome against its expectation, writes verification/results/tlc-latest.json (distinct-state
counts and depth are deterministic), and turns every expected counterexample into a regression vector in
verification/vectors/tla-<config>.json. Exit 1 on any unexpected outcome.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TLA = ROOT / "verification/tla"
VEC = ROOT / "verification/vectors"


def parse_trace(out: str) -> list[dict]:
    states, cur, key = [], None, None
    for line in out.splitlines():
        m = re.match(r"State (\d+): <(\w+)", line)
        if m:
            cur = {"step": int(m.group(1)), "action": m.group(2), "vars": {}}
            states.append(cur)
            continue
        if cur is None:
            continue
        m = re.match(r"/\\ (\w+) = (.*)", line)
        if m:
            key = m.group(1)
            cur["vars"][key] = m.group(2).strip()
        elif key and line.startswith("  ") and not line.startswith("  States"):
            cur["vars"][key] += " " + line.strip()
        elif not line.strip():
            key = None
    return states


def run(entry: dict) -> dict:
    env = dict(os.environ, CFG=str(TLA / entry["config"]),
               WORKERS="1" if entry["expect"] == "violation" else "auto")  # 1 worker: deterministic shortest trace
    p = subprocess.run([str(ROOT / "tools/tlc.sh"), str(TLA / f"{entry['module']}.tla")],
                       capture_output=True, text=True, env=env)
    out = p.stdout + p.stderr
    viol = re.search(r"Invariant (\w+) is violated", out) or re.search(r"Temporal properties were violated|Action property (\w+)", out)
    counts = re.findall(r"([\d,]+) distinct states found, 0 states left", out)
    depth = re.search(r"depth of the complete state graph search is (\d+)", out)
    res = {
        "config": entry["config"], "gate": entry.get("gate"), "claim": entry["claim"], "expect": entry["expect"],
        "outcome": "violation" if viol else ("pass" if "No error has been found" in out else "error"),
        "violated": viol.group(1) if viol and viol.lastindex else None,
        "distinct_states": int(counts[-1].replace(",", "")) if counts and not viol else None,
        "depth": int(depth.group(1)) if depth and not viol else None,
    }
    ok = res["outcome"] == entry["expect"] and (entry["expect"] != "violation" or res["violated"] == entry.get("invariant"))
    res["as_expected"] = ok
    if res["outcome"] == "violation":
        trace = parse_trace(out)
        res["trace_length"] = len(trace)
        VEC.mkdir(exist_ok=True)
        (VEC / f"tla-{entry['config'][:-4]}.json").write_text(json.dumps(
            {"gate": entry.get("gate"), "source": f"verification/tla/{entry['config']}",
             "kind": "golden" if entry["invariant"].startswith("Trap") else "residual-risk",
             "claim": entry["claim"], "violated": res["violated"], "trace": trace}, indent=1) + "\n")
    if res["outcome"] == "error":
        print(out[-3000:], file=sys.stderr)
    return res


def main() -> int:
    # Bootstrap/validate the pinned TLC tool exactly once before the model loop. If it is unavailable,
    # preserve the previously captured tlc-latest.json instead of replacing it with 80+ identical errors.
    pre = subprocess.run([str(ROOT / "tools/tlc.sh"), "--preflight"], cwd=ROOT, text=True,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if pre.returncode:
        sys.stderr.write(pre.stdout); sys.stderr.write(pre.stderr)
        return pre.returncode
    if pre.stdout:
        print(pre.stdout.strip())
    entries = [e for m in sorted(TLA.glob("models*.json")) for e in json.loads(m.read_text())["models"]]
    results = []
    for e in entries:
        r = run(e)
        results.append(r)
        print(f"{'ok ' if r['as_expected'] else 'BAD'} {r['config']:<42} {r['outcome']:<9} {r['violated'] or r['distinct_states']}")
    tool = "tla2tools 1.7.4 (sha1 bee4a54f3ee3d4afc347c3240ec2d9e93b075104)"
    (ROOT / "verification/results/tlc-latest.json").write_text(json.dumps(
        {"tool": tool, "gate_status_effect": "evidence_only_no_gate_automatically_passed",
         "all_as_expected": all(r["as_expected"] for r in results), "runs": results}, indent=2) + "\n")
    return 0 if all(r["as_expected"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
