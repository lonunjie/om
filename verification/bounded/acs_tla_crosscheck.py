#!/usr/bin/env python3
"""V-002: replay every AccountControl.tla trace step through the §4.1 table model (review finding F-2).

AccountControl.tla (the TLC model) and acs_authorization_table.py (the executable §4.1 table) are two encodings of the
same constitution. This script keeps them from silently diverging. It reads each TLC counterexample trace that
tools/run_tlc.py writes to verification/vectors/tla-AccountControl_*.json. The TLA variable `last` records the
signers and recovery vote of each constitutional step. For every step it rebuilds the parent and child
AccountControlState in the table model's vocabulary and requires:

1. positive: `authorize(parent, child, ctx)` accepts the step exactly as TLC took it;
2. negative (mutations of the same step): one signer fewer at the threshold is rejected; adding a recovery vote to a lift
   or freeze resolution is rejected; a recovery-alone completion one tick early is only pending.

LegacyPolicy steps (arm/activate) and fork-freeze resolutions (§8 fork bridge, covered by the table's own vectors)
have no row-for-row TLA counterpart here and are counted as skipped. Written by a Claude agent (verification
executor). Deterministic.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from acs_authorization_table import authorize, majority, state  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
VEC = ROOT / "verification/vectors"
RES = ROOT / "verification/results/acs-tla-crosscheck-latest.json"
DELAY = 2  # every AccountControl config uses Delay = 2


def parse(v: str):
    """Parse the TLC value syntax used in traces: records, sets, sequences, strings, integers, booleans."""
    toks = re.findall(r'"[^"]*"|\|->|[\[\]{}<>(),]|[A-Za-z_][A-Za-z0-9_]*|-?\d+', v)
    pos = 0

    def val():
        nonlocal pos
        t = toks[pos]
        pos += 1
        if t.startswith('"'):
            return t[1:-1]
        if t in ("TRUE", "FALSE"):
            return t == "TRUE"
        if re.fullmatch(r"-?\d+", t):
            return int(t)
        if t == "{":
            out = []
            while toks[pos] != "}":
                out.append(val())
                if toks[pos] == ",":
                    pos += 1
            pos += 1
            return frozenset(out)
        if t == "<" and toks[pos] == "<":
            pos += 1
            out = []
            while not (toks[pos] == ">" and toks[pos + 1] == ">"):
                out.append(val())
                if toks[pos] == ",":
                    pos += 1
            pos += 2
            return tuple(out)
        if t == "[":
            rec = {}
            while toks[pos] != "]":
                k = toks[pos]
                assert toks[pos + 1] == "|->", v
                pos += 2
                rec[k] = val()
                if toks[pos] == ",":
                    pos += 1
            pos += 1
            return rec
        return t
    return val()


def acs(vs: dict) -> dict:
    return state(sorted(vs["controllers"]), quarantined=sorted(vs["quarantined"]), revoked=sorted(vs["revoked"]),
                 policy=f"p{vs['epoch']}", gen=vs["gen"],
                 retired={"reason": "deleted"} if vs["retired"] else None)


def step_ctx(last: dict, frozen: bool) -> dict:
    ctx = {"signers": sorted(last["signers"])}
    if last["rv"]:
        ctx["recovery"] = {"elapsed": DELAY if last["kind"] == "recovery_delayed" else 0, "delay": DELAY, "veto": False}
    if frozen:
        ctx["frozen"] = True
    return ctx


def check_trace(path: Path) -> tuple[dict, list[str]]:
    keep = ("controllers", "quarantined", "revoked", "epoch", "gen", "retired", "frozen", "last")
    trace = [{k: parse(s["vars"][k]) for k in keep} for s in json.loads(path.read_text())["trace"]]
    stats = {"steps": 0, "positive": 0, "negative": 0, "skipped": 0}
    bad: list[str] = []
    for i in range(1, len(trace)):
        pv, cv = trace[i - 1], trace[i]
        if cv["gen"] != pv["gen"] + 1:
            continue  # environment step, veto, fork detection: no new AccountControlState
        stats["steps"] += 1
        last = cv["last"]
        kind = last["kind"]
        if kind.startswith("legacy") or (kind == "resolve" and pv["quarantined"] == cv["quarantined"]):
            stats["skipped"] += 1
            continue
        p, c = acs(pv), acs(cv)
        ctx = step_ctx(last, pv["frozen"])
        tag = f"{path.name} step {i + 1} {kind} ctx={ctx}"
        v, why = authorize(p, c, ctx)
        stats["positive"] += 1
        if v != "accept":
            bad.append(f"TLA step not accepted by §4.1 table ({v}: {why}): {tag}")
        n = len(pv["controllers"])
        need = 1 if kind == "quarantine" else majority(n)
        if last["signers"] and len(last["signers"]) + last["rv"] == need and not (n == 1 and kind != "quarantine"):
            fewer = dict(ctx, signers=sorted(last["signers"])[1:])
            v2, why2 = authorize(p, c, fewer)
            stats["negative"] += 1
            if v2 == "accept":
                bad.append(f"one signer fewer still accepted ({why2}): {tag}")
        if kind in ("lift", "resolve"):
            v3, why3 = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY, "delay": DELAY, "veto": False}))
            stats["negative"] += 1
            if v3 != "reject":
                bad.append(f"lift/resolution with a recovery vote not rejected ({v3}: {why3}): {tag}")
        if kind == "recovery_delayed":
            v4, _ = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY - 1, "delay": DELAY, "veto": False}))
            stats["negative"] += 1
            if v4 != "pending":
                bad.append(f"recovery alone one tick early is {v4}, not pending: {tag}")
            v5, _ = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY, "delay": DELAY, "veto": True}))
            stats["negative"] += 1
            if v5 != "reject":
                bad.append(f"vetoed recovery alone is {v5}, not reject: {tag}")
    return stats, bad


def main() -> int:
    paths = sorted(VEC.glob("tla-AccountControl_*.json"))
    per, bad = {}, []
    for pth in paths:
        st, b = check_trace(pth)
        per[pth.name] = st
        bad += b
    kinds_needed = {"quarantine", "lift", "resolve", "recovery_cosigned", "recovery_delayed"}
    seen = set()
    for pth in paths:
        for s in json.loads(pth.read_text())["trace"]:
            seen.add(parse(s["vars"]["last"])["kind"])
    missing = sorted(kinds_needed - seen)
    tot = {k: sum(s[k] for s in per.values()) for k in ("steps", "positive", "negative", "skipped")}
    ok = bool(paths) and not bad and not missing
    RES.write_text(json.dumps({
        "gate": "V-002", "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        "source": "verification/bounded/acs_tla_crosscheck.py", "all_ok": ok, "traces": len(paths),
        "totals": tot, "per_trace": per, "step_kinds_seen": sorted(seen), "missing_step_kinds": missing,
        "failures": bad}, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "traces": len(paths), **tot, "missing": missing, "failures": len(bad)}))
    for b in bad[:10]:
        print("FAIL", b, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
