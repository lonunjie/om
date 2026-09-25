#!/usr/bin/env python3
"""Record verification evidence in machine/architecture_verification_gates.jsonl, then re-render the gate view.

  python3 tools/om_gate.py show V-001                     # criteria with [x]/[ ] and indexes
  python3 tools/om_gate.py apply PATCH.json               # apply a patch (below), render, print a summary

PATCH.json maps a gate id to operations:
  {"V-001": {"met": [0, 5, "exact criterion string"],     # pre_engineering_criteria by index or exact text
             "feasibility_met": [0],                       # feasibility_criteria by index or exact text
             "evidence": ["path", ...],                    # appended if absent (paths must exist)
             "set": {"pre_engineering_status": "passed", "pre_engineering_independence_status": "satisfied", ...}}}

Only canonical criterion strings are ever written (ADR-0134 §6). The validator still decides whether the result
is consistent; run `make check` afterwards.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "machine/architecture_verification_gates.jsonl"


def load() -> list[dict]:
    return [json.loads(line) for line in LEDGER.read_text(encoding="utf-8").splitlines() if line.strip()]


def save(gates: list[dict]) -> None:
    LEDGER.write_text("".join(json.dumps(g, ensure_ascii=False, separators=(",", ":")) + "\n" for g in gates), encoding="utf-8")


def resolve(g: dict, key: str, ref) -> str:
    crits = g.get(key) or []
    if isinstance(ref, int):
        return crits[ref]
    if ref not in crits:
        raise SystemExit(f"{g['id']}: not a canonical {key} entry: {ref!r}")
    return ref


def show(gid: str) -> None:
    g = next(x for x in load() if x["id"] == gid)
    met, fmet = set(g.get("criteria_met") or []), set(g.get("feasibility_criteria_met") or [])
    print(f"{gid} {g['gate']}: pre_engineering={g['pre_engineering_status']} independence={g['pre_engineering_independence_status']} "
          f"feasibility={g['feasibility_status']} indep={json.dumps(g['independence'])}")
    for i, c in enumerate(g.get("pre_engineering_criteria") or []):
        print(f"  {i:2} [{'x' if c in met else ' '}] {c}")
    for i, c in enumerate(g.get("feasibility_criteria") or []):
        print(f"  F{i:1} [{'x' if c in fmet else ' '}] {c}")


def apply(patch: dict) -> None:
    gates = load()
    by = {g["id"]: g for g in gates}
    for gid, ops in patch.items():
        g = by[gid]
        for ref in ops.get("met", []):
            c = resolve(g, "pre_engineering_criteria", ref)
            g.setdefault("criteria_met", [])
            if c not in g["criteria_met"]:
                g["criteria_met"].append(c)
        for ref in ops.get("feasibility_met", []):
            c = resolve(g, "feasibility_criteria", ref)
            g.setdefault("feasibility_criteria_met", [])
            if c not in g["feasibility_criteria_met"]:
                g["feasibility_criteria_met"].append(c)
        for e in ops.get("evidence", []):
            if not (ROOT / e.rstrip("/")).exists():
                raise SystemExit(f"{gid}: evidence path does not exist: {e}")
            g.setdefault("evidence", [])
            if e not in g["evidence"]:
                g["evidence"].append(e)
        g["evidence"] = sorted(set(g.get("evidence") or []))
        for k, v in ops.get("set", {}).items():
            g[k] = v
    save(gates)
    subprocess.run([sys.executable, str(ROOT / "tools/render_gates.py")], check=True)
    for gid in patch:
        g = by[gid]
        n, m = len(g.get("pre_engineering_criteria") or []), len(set(g.get("criteria_met") or []) & set(g.get("pre_engineering_criteria") or []))
        f, fm = len(g.get("feasibility_criteria") or []), len(g.get("feasibility_criteria_met") or [])
        print(f"{gid}: pre-engineering {m}/{n} ({g['pre_engineering_status']}, independence {g['pre_engineering_independence_status']}); "
              f"feasibility {fm}/{f} ({g['feasibility_status']})")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "show":
        show(sys.argv[2])
    elif len(sys.argv) == 3 and sys.argv[1] == "apply":
        apply(json.loads(Path(sys.argv[2]).read_text()))
    else:
        print(__doc__)
        raise SystemExit(2)
