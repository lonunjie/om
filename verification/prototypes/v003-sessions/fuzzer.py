#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

1. replay: every vector in verification/vectors/one-to-one-session-regression-vectors.json is replayed
   step by step against the Python reference machine (sessions.py). Each TLC step's outcome label
   must match the Python outcome (cross-implementation agreement) and the vector's invariant must be
   violated at the end with the vector's rule disabled.
2. fuzz: hypothesis drives long random schedules (choice sequences -> enabled actions) on a larger
   topology (2 + 3 devices, link/revoke on both accounts), then drains to quiescence and checks all
   invariants. With every rule enabled no violation may occur. With one rule disabled, hypothesis
   must independently find (and shrink) a violation; shrunk traces are written as fuzzer vectors.

Usage: fuzzer.py <results.json> [--long]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

from hypothesis import HealthCheck, Phase, given, settings, strategies as st

from sessions import FUZZ_TOPOLOGY, KNOBS, TLC_TOPOLOGY, Bounds, Infeasible, World

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
VECTORS = ROOT / "verification/vectors/one-to-one-session-regression-vectors.json"
FUZZ_VECTORS = HERE / "fuzzer_vectors.json"

# ----------------------------------------------------------------------------- replay


def bounds_from(c):
    return Bounds(max_msgs=int(c["MaxMsgs"]), max_sid=int(c["MaxSid"]), max_crash=int(c["MaxCrash"]),
                  max_restore=int(c["MaxRestore"]), max_dup=int(c["MaxDup"]), max_weak=int(c["MaxWeak"]),
                  max_inject=int(c["MaxInject"]))


def rules_from(c):
    return {k: c.get(k, "TRUE") == "TRUE" for k in KNOBS}


def find_env(w, step):
    lab = step[0]
    if lab == "RecvReset":
        _, s, frm, sid, m = step
        cands = [e for e in w.net if e[0] == "reset" and e[2] == s and e[1] == frm and e[3] == sid and e[4] == m]
    else:
        r, f = step[1], step[2]
        cands = [e for e in w.net if e[0] == "msg" and e[2] == r and e[1] == f]
        if lab in ("Recv", "RecvUndecryptable"):
            cands = [e for e in cands if e[3] == step[3] and e[6] == step[4]]
        elif lab == "RecvRejectedRevoked":
            cands = [e for e in cands if e[6] == step[3]]
        elif lab == "RecvRejectedBelowFloor":
            cands = [e for e in cands if e[3] == step[3]]
    if not cands:
        raise Infeasible(f"no envelope for {step}")
    return sorted(cands, key=repr)[0]


RECV = {"Recv", "RecvUndecryptable", "RecvRejectedRevoked", "RecvRejectedBelowFloor", "RecvReset"}
ENC = {"Encrypt", "Initiate", "EncryptRefusedFloor", "EncryptRefusedIneligible"}


def replay(actions, topo, bounds, rules):
    """Replay TLC labels; returns (matched_steps, python_labels, violations, mismatch)."""
    w = World(topo, bounds, rules)
    pending_keys = []
    for i, st_ in enumerate(actions):
        if st_[0] in RECV:
            pending_keys.append((i, st_))
    for i, step in enumerate(actions):
        lab = step[0]
        try:
            if lab == "Init":
                continue
            if lab == "ControlStep":
                got = w.control_step("B")
            elif lab == "Learn":
                got = w.learn(step[1], step[2])
            elif lab == "ServeWeakBundle":
                got = w.serve_weak(step[1])
            elif lab == "InjectRogueDevice":
                got = w.inject()
            elif lab == "NewMessage":
                got = w.new_message(step[1])
            elif lab in ENC:
                got = w.encrypt(step[1], step[2], step[3])
            elif lab in RECV:
                e = find_env(w, step)
                # a delivery is a duplicate if the same envelope is delivered again later in the trace
                later = any(j > i and find_key(a) == find_key(step) for j, a in pending_keys)
                got = w.recv(e, dup=later)
            elif lab == "Persist":
                got = w.persist(step[1])
            elif lab == "Crash":
                got = w.crash(step[1])
            elif lab == "TakeBackup":
                got = w.take_backup(step[1])
            elif lab == "Restore":
                got = w.restore(step[1])
            else:
                raise Infeasible(f"unknown label {lab}")
        except Infeasible as ex:
            return i, w.log, w.check(), f"step {i} infeasible: {ex}"
        w.log.append(got)
        if got != step:
            return i, w.log, w.check(), f"step {i}: TLC {step} vs Python {got}"
    return len(actions), w.log, w.check(), None


def find_key(step):
    lab = step[0]
    if lab == "RecvReset":
        return ("reset",) + tuple(step[1:])
    if lab in ("Recv", "RecvUndecryptable"):
        return ("msg", step[1], step[2], step[3], step[4])
    return (lab,) + tuple(step[1:])


def run_replays():
    data = json.loads(VECTORS.read_text())
    out = []
    for v in data["vectors"]:
        if "constants" in v:
            c = v["constants"]
            topo = TLC_TOPOLOGY if c["Senders"] == '{"a1", "b1"}' else \
                type(TLC_TOPOLOGY)(TLC_TOPOLOGY.accounts, frozenset({"a1", "b1", "b3"}))
            b, rules = bounds_from(c), rules_from(c)
        else:  # hand-recorded found traces: default bounds, rule under test disabled if named
            topo, b = TLC_TOPOLOGY, Bounds(max_restore=0)
            rules = {k: k != v.get("rule_disabled") for k in KNOBS}
        if v["classification"].startswith("model_bug"):
            n, labels, viol, mm = replay(v["actions"], topo, b, {k: True for k in KNOBS})
            ok = v["violates"] not in viol   # fixed model/machine must not reproduce the old bug
            out.append({"id": v["id"], "kind": "model_bug_fixed", "ok": ok, "steps_matched": n,
                        "steps": len(v["actions"]), "python_violations": viol, "note": mm})
            continue
        n, labels, viol, mm = replay(v["actions"], topo, b, rules)
        reproduced = mm is None and v["violates"] in viol
        rules_on = {k: True for k in KNOBS}
        n2, _, viol2, mm2 = replay(v["actions"], topo, b, rules_on)
        fixed = mm2 is not None or v["violates"] not in viol2
        out.append({"id": v["id"], "kind": "reproduce", "ok": reproduced and fixed, "steps_matched": n,
                    "steps": len(v["actions"]), "python_violations": viol, "mismatch": mm,
                    "with_all_rules": "infeasible at step %d" % n2 if mm2 else ("violates " + ",".join(viol2) if viol2 else "no violation")})
    return out

# ----------------------------------------------------------------------------- fuzzing


FUZZ_BOUNDS = Bounds(max_msgs=5, max_sid=12, max_crash=2, max_restore=1, max_dup=3, max_weak=1, max_inject=1)


def run_schedule(choices, rules, bounds=FUZZ_BOUNDS, topo=FUZZ_TOPOLOGY):
    w = World(topo, bounds, rules)
    for c in choices:
        acts = w.enabled()
        if not acts:
            break
        try:
            w.apply(acts[c % len(acts)])
        except Infeasible:
            continue
        bad = w.check()
        if bad:
            return w, bad
    w.drain()
    return w, w.check()


def fuzz(rules, examples, name):
    """Hypothesis search; returns (examples_run, first shrunk failure or None)."""
    found = {}
    count = {"n": 0}

    @settings(max_examples=examples, derandomize=True, database=None, deadline=None,
              suppress_health_check=list(HealthCheck), phases=[Phase.generate, Phase.shrink])
    @given(st.lists(st.integers(min_value=0, max_value=10_000), min_size=10, max_size=160))
    def prop(choices):
        count["n"] += 1
        w, bad = run_schedule(choices, rules)
        if bad:
            found["last"] = {"choices": choices, "violations": bad, "actions": [["Init"]] + w.log}
        assert not bad, bad

    try:
        prop()
        return count["n"], None
    except AssertionError:
        return count["n"], found.get("last")


def main():
    out_path = Path(sys.argv[1])
    long = "--long" in sys.argv
    n_all = 20_000 if long else 1_500
    n_gap = 3_000 if long else 400
    replays = run_replays()
    ok = all(r["ok"] for r in replays)
    for r in replays:
        print(f"replay {r['id']}: {'ok' if r['ok'] else 'FAIL'} ({r['steps_matched']}/{r['steps']} steps matched) {r.get('mismatch') or ''}")

    fuzz_res = {}
    fuzz_vectors = []
    n, fail = fuzz({k: True for k in KNOBS}, n_all, "all-rules")
    fuzz_res["all_rules"] = {"schedules": n, "violation": fail["violations"] if fail else None}
    ok &= fail is None
    print(f"fuzz all rules: {n} schedules, violation={fail['violations'] if fail else None}")
    if fail:
        fuzz_vectors.append({"id": "V003-FUZZ-all-rules", "classification": "unclassified_new_failure",
                             "violates": fail["violations"], "rule_disabled": None, "actions": fail["actions"]})
    for knob in KNOBS:
        n, fail = fuzz({k: k != knob for k in KNOBS}, n_gap, knob)
        fuzz_res[knob] = {"schedules": n, "violation": fail["violations"] if fail else None}
        print(f"fuzz without {knob}: {n} schedules, violation={fail['violations'] if fail else None}")
        if fail:
            fuzz_vectors.append({"id": f"V003-FUZZ-{knob}", "source": "hypothesis (shrunk), FUZZ_TOPOLOGY",
                                 "rule_disabled": knob, "violates": fail["violations"],
                                 "steps": len(fail["actions"]), "actions": fail["actions"]})
    FUZZ_VECTORS.write_text(json.dumps({"gate": "V-003", "topology": "sessions.FUZZ_TOPOLOGY",
                                        "bounds": FUZZ_BOUNDS.__dict__, "vectors": fuzz_vectors}, indent=1) + "\n")
    out_path.write_text(json.dumps({"replays": replays, "fuzz": fuzz_res, "long": long, "ok": ok}, indent=1, sort_keys=True) + "\n")
    print("fuzzer:", "OK" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
