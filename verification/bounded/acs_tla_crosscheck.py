#!/usr/bin/env python3
"""V-002 / ADR-0137: AccountControl accepted-set equivalence.

This verifier has two layers:

1. replay committed TLC traces through the executable authorization table; and
2. independently enumerate the bounded constitutional accepted set for selective
   SAFETY_FREEZE resolution in *both* directions.  The second layer exists because
   trace replay proves only ``TLA transition -> executable accepts`` and would miss
   an executable-only authority path (review F-10).

The bounded equivalence layer mirrors the ResolveFreeze guard directly from the TLA
source, checks that the expected guard clauses are still present, and compares every
candidate parent/child/signer/recovery combination against ``authorize``.  It also
runs named negative controls for outsider signers, recovery votes, challenge use on
ordinary transitions, mixed freeze-resolution changes, and fork-bridge separation.

Deterministic. Evidence only; it does not pass V-002 by itself.
"""
from __future__ import annotations

import itertools
import json
import re
import sys
from copy import deepcopy
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from acs_authorization_table import authorize, majority, state  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
VEC = ROOT / "verification/vectors"
RES = ROOT / "verification/results/acs-tla-crosscheck-latest.json"
TLA = ROOT / "verification/tla/AccountControl.tla"
DELAY = 2


def parse(v: str):
    toks = re.findall(r'"[^"]*"|\|->|[\[\]{}<>(),]|[A-Za-z_][A-Za-z0-9_]*|-?\d+', v)
    pos = 0

    def val():
        nonlocal pos
        t = toks[pos]; pos += 1
        if t.startswith('"'): return t[1:-1]
        if t in ("TRUE", "FALSE"): return t == "TRUE"
        if re.fullmatch(r"-?\d+", t): return int(t)
        if t == "{":
            out = []
            while toks[pos] != "}":
                out.append(val())
                if toks[pos] == ",": pos += 1
            pos += 1; return frozenset(out)
        if t == "<" and toks[pos] == "<":
            pos += 1; out = []
            while not (toks[pos] == ">" and toks[pos + 1] == ">"):
                out.append(val())
                if toks[pos] == ",": pos += 1
            pos += 2; return tuple(out)
        if t == "[":
            rec = {}
            while toks[pos] != "]":
                k = toks[pos]; assert toks[pos + 1] == "|->", v; pos += 2
                rec[k] = val()
                if toks[pos] == ",": pos += 1
            pos += 1; return rec
        return t
    return val()


def acs(vs: dict) -> dict:
    return state(sorted(vs["controllers"]), quarantined=sorted(vs["quarantined"]), revoked=sorted(vs["revoked"]),
                 policy=f"p{vs['epoch']}", gen=vs["gen"], retired={"reason": "deleted"} if vs["retired"] else None)


def step_ctx(last: dict, frozen: bool) -> dict:
    ctx = {"signers": sorted(last["signers"])}
    if last["rv"]:
        ctx["recovery"] = {"elapsed": DELAY if last["kind"] == "recovery_delayed" else 0, "delay": DELAY, "veto": False}
    if frozen: ctx["frozen"] = True
    return ctx


def check_trace(path: Path) -> tuple[dict, list[str]]:
    keep = ("controllers", "quarantined", "revoked", "epoch", "gen", "retired", "frozen", "last")
    trace = [{k: parse(s["vars"][k]) for k in keep} for s in json.loads(path.read_text())["trace"]]
    stats = {"steps": 0, "positive": 0, "negative": 0, "skipped": 0}
    bad: list[str] = []
    for i in range(1, len(trace)):
        pv, cv = trace[i - 1], trace[i]
        if cv["gen"] != pv["gen"] + 1: continue
        stats["steps"] += 1
        last, kind = cv["last"], cv["last"]["kind"]
        if kind.startswith("legacy") or (kind == "resolve" and pv["quarantined"] == cv["quarantined"]):
            stats["skipped"] += 1; continue
        p, c, ctx = acs(pv), acs(cv), step_ctx(last, pv["frozen"])
        tag = f"{path.name} step {i + 1} {kind} ctx={ctx}"
        v, why = authorize(p, c, ctx); stats["positive"] += 1
        if v != "accept": bad.append(f"TLA step not accepted by table ({v}: {why}): {tag}")
        n = len(pv["controllers"]); need = 1 if kind == "quarantine" else majority(n)
        if last["signers"] and len(last["signers"]) + last["rv"] == need and not (n == 1 and kind != "quarantine"):
            v2, why2 = authorize(p, c, dict(ctx, signers=sorted(last["signers"])[1:])); stats["negative"] += 1
            if v2 == "accept": bad.append(f"one signer fewer still accepted ({why2}): {tag}")
        if kind in ("lift", "resolve"):
            v3, why3 = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY, "delay": DELAY, "veto": False})); stats["negative"] += 1
            if v3 != "reject": bad.append(f"lift/resolution with recovery not rejected ({v3}: {why3}): {tag}")
        if kind == "recovery_delayed":
            v4, _ = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY - 1, "delay": DELAY, "veto": False})); stats["negative"] += 1
            if v4 != "pending": bad.append(f"recovery one tick early is {v4}, not pending: {tag}")
            v5, _ = authorize(p, c, dict(ctx, recovery={"elapsed": DELAY, "delay": DELAY, "veto": True})); stats["negative"] += 1
            if v5 != "reject": bad.append(f"vetoed recovery is {v5}, not reject: {tag}")
    return stats, bad


# ResolveFreeze guard clauses that make selective restoration the TLA rule, not a Python-only convention.
TLA_REQUIRED = [
    "R \\subseteq quarantined /\\ R # {}",
    "R \\subseteq V",
    "V \\subseteq Signable(p)",
    "Cardinality(V) >= Majority(controllers)",
    "Cardinality(controllers \\ (quarantined \\ R)) >= Majority(controllers)",
    "quarantined' = quarantined \\ R",
    'last\' = [kind |-> "resolve", signers |-> V, rv |-> 0, challenge |-> V \\cap quarantined]',
]


def tla_source_guard() -> list[str]:
    src = TLA.read_text()
    block = src.split("ResolveFreeze(p, V, R) ==", 1)[1].split("\n\n", 1)[0]
    return [f"ResolveFreeze source missing clause: {x}" for x in TLA_REQUIRED if x not in block]


def tla_freeze_accept(parent_q: set[str], child_q: set[str], controllers: set[str], signers: set[str], recovery: bool,
                      extra: str) -> bool:
    """Constitutional portion of ResolveFreeze, with all controllers owner-signable.

    TLA party/liveness availability dimensions are deliberately normalized here: no controller is lost/offline and
    all are owner devices, so Signable(owner)=controllers. This compares the authorization transition relation,
    while TLC remains responsible for environment/reachability behavior.
    """
    maj = majority(len(controllers))
    restored = parent_q - child_q
    return (
        bool(parent_q) and len(controllers - parent_q) < maj and
        extra == "none" and not recovery and
        child_q <= parent_q and bool(restored) and restored <= signers and
        signers <= controllers and len(signers) >= maj and
        len(controllers - child_q) >= maj
    )


def child_for(parent: dict, child_q: set[str], extra: str) -> dict:
    c = deepcopy(parent); c["generation"] += 1; c["quarantined"] = sorted(child_q)
    if extra == "promote":
        c["control"] = sorted(set(c["control"]) | {"new"}); c["key_epochs"]["new"] = 1
    elif extra == "revoke":
        victim = sorted(set(c["control"]) - child_q)[0] if (set(c["control"]) - child_q) else sorted(c["control"])[0]
        c["control"] = sorted(set(c["control"]) - {victim}); c["quarantined"] = sorted(set(c["quarantined"]) - {victim})
        c["revoked"] = sorted(set(c["revoked"]) | {victim}); c["key_epochs"].pop(victim, None)
    elif extra == "recovery_policy": c["recovery_policy"] = "p1"
    elif extra == "constitutional_policy": c["constitutional_policy"] += 1
    elif extra == "raise_min_profile": c["min_one_to_one_security_generation"] += 1
    return c


def bounded_freeze_equivalence() -> dict:
    mismatches, table_only, tla_only = [], 0, 0
    checked = accepted_tla = accepted_table = 0
    extras = ("none", "promote", "revoke", "recovery_policy", "constitutional_policy", "raise_min_profile")
    for n in (2, 3, 4):
        ctl = {f"d{i}" for i in range(1, n + 1)}; maj = majority(n)
        for qn in range(1, n):
            for qtuple in itertools.combinations(sorted(ctl), qn):
                q = set(qtuple)
                if not (0 < len(ctl - q) < maj): continue
                p = state(sorted(ctl), quarantined=sorted(q), policy="p0", constitution=1, min_gen=1, gen=1)
                universe = sorted(ctl | {"outsider"})
                for child_bits in range(1 << n):
                    cq = {d for i, d in enumerate(sorted(ctl)) if child_bits & (1 << i)}
                    for sn in range(len(universe) + 1):
                        for st in itertools.combinations(universe, sn):
                            signers = set(st)
                            for recovery, extra in itertools.product((False, True), extras):
                                checked += 1
                                tv = tla_freeze_accept(q, cq, ctl, signers, recovery, extra)
                                ctx = {"signers": sorted(signers), "frozen": True}
                                if recovery: ctx["recovery"] = {"elapsed": 2, "delay": 2, "veto": False}
                                ev, _ = authorize(p, child_for(p, cq, extra), ctx)
                                av = ev == "accept"
                                accepted_tla += int(tv); accepted_table += int(av)
                                if tv != av:
                                    if av: table_only += 1
                                    else: tla_only += 1
                                    if len(mismatches) < 20:
                                        mismatches.append({"N": n, "parent_q": sorted(q), "child_q": sorted(cq),
                                                           "signers": sorted(signers), "recovery": recovery,
                                                           "extra": extra, "tla": tv, "table": av})
    return {"checked": checked, "accepted_tla": accepted_tla, "accepted_table": accepted_table,
            "table_only": table_only, "tla_only": tla_only, "equal": not mismatches and table_only == 0 and tla_only == 0,
            "mismatch_examples": mismatches}


def explicit_negatives() -> dict[str, bool]:
    p = state(["d1", "d2", "d3"], quarantined=["d2", "d3"], gen=1)
    good = deepcopy(p); good["generation"] = 2; good["quarantined"] = ["d3"]
    cases = {}
    cases["partial_restore_below_majority"] = authorize(
        state(["d1", "d2", "d3", "d4"], quarantined=["d2", "d3", "d4"], gen=1),
        state(["d1", "d2", "d3", "d4"], quarantined=["d3", "d4"], gen=2),
        {"signers": ["d1", "d2", "d3"], "frozen": True})[0] == "reject"
    cases["recovery_vote_in_freeze"] = authorize(p, good, {"signers": ["d1", "d2"], "frozen": True,
        "recovery": {"elapsed": 2, "delay": 2, "veto": False}})[0] == "reject"
    cases["outsider_challenge_signer"] = authorize(p, good, {"signers": ["d1", "d2", "outsider"], "frozen": True})[0] == "reject"
    ordinary_p = state(["d1", "d2", "d3"], quarantined=["d3"], gen=1)
    ordinary_c = deepcopy(ordinary_p); ordinary_c["generation"] = 2; ordinary_c["quarantined"] = []
    cases["quarantined_challenge_on_ordinary_lift"] = authorize(ordinary_p, ordinary_c, {"signers": ["d1", "d3"]})[0] == "reject"
    for extra in ("promote", "revoke", "recovery_policy", "constitutional_policy", "raise_min_profile"):
        cases[f"freeze_plus_{extra}"] = authorize(p, child_for(p, {"d3"}, extra), {"signers": ["d1", "d2"], "frozen": True})[0] == "reject"
    fork = deepcopy(p); fork["generation"] = 2; fork["fork_ack"] = ["hA", "hB"]
    cases["quarantine_freeze_not_fork_bridge"] = authorize(p, fork, {"signers": ["d1", "d2"], "frozen": True, "trusted_heads": ["hA"]})[0] == "reject"
    return cases


def main() -> int:
    paths = sorted(VEC.glob("tla-AccountControl_*.json"))
    per, bad = {}, []
    for pth in paths:
        st, b = check_trace(pth); per[pth.name] = st; bad += b
    kinds_needed = {"quarantine", "lift", "resolve", "recovery_cosigned", "recovery_delayed"}
    seen = set()
    for pth in paths:
        for s in json.loads(pth.read_text())["trace"]: seen.add(parse(s["vars"]["last"])["kind"])
    missing = sorted(kinds_needed - seen)
    totals = {k: sum(s[k] for s in per.values()) for k in ("steps", "positive", "negative", "skipped")}
    source_problems = tla_source_guard()
    equiv = bounded_freeze_equivalence()
    negatives = explicit_negatives()
    negative_failures = sorted(k for k, ok in negatives.items() if not ok)
    ok = bool(paths) and not bad and not missing and not source_problems and equiv["equal"] and not negative_failures
    payload = {
        "gate": "V-002", "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        "source": "verification/bounded/acs_tla_crosscheck.py", "all_ok": ok, "traces": len(paths),
        "trace_replay": {"totals": totals, "per_trace": per, "step_kinds_seen": sorted(seen),
                         "missing_step_kinds": missing, "failures": bad},
        "tla_source_guard_problems": source_problems,
        "bounded_freeze_accepted_set_equivalence": equiv,
        "explicit_negatives": negatives,
        "negative_failures": negative_failures,
        "fresh_tlc_note": "This script compares the committed TLA source/trace corpus. TLC itself is run by tools/run_tlc.py when tla2tools is available.",
    }
    RES.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "traces": len(paths), **totals, "accepted_set_equal": equiv["equal"],
                      "bounded_cases": equiv["checked"], "table_only": equiv["table_only"], "tla_only": equiv["tla_only"],
                      "negative_failures": negative_failures, "failures": len(bad) + len(source_problems)}))
    for b in (bad + source_problems)[:10]: print("FAIL", b, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
