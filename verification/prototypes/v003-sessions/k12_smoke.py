#!/usr/bin/env python3
"""Focused K12 cross-model smoke check for the Python V-003 reference machine.

Runs without Hypothesis so a clean design drop can verify the ADR-0136 initiator-binding regression even when the
pinned fuzz environment is unavailable.  The full `run.sh` remains the authoritative V-003 prototype runner.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

from sessions import KNOBS, TLC_TOPOLOGY, Bounds, World

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
VECTORS = ROOT / "verification/vectors/one-to-one-session-regression-vectors.json"
OUT = ROOT / "verification/results/v003-k12-python-smoke-latest.json"


def bounds_from(c: dict[str, str]) -> Bounds:
    return Bounds(
        max_msgs=int(c["MaxMsgs"]), max_sid=int(c["MaxSid"]), max_crash=int(c["MaxCrash"]),
        max_restore=int(c["MaxRestore"]), max_dup=int(c["MaxDup"]), max_weak=int(c["MaxWeak"]),
        max_inject=int(c["MaxInject"]), max_forge=int(c.get("MaxForge", 0)),
    )


def run(bind: bool, vector: dict) -> dict:
    c = vector["constants"]
    senders = frozenset(re.findall(r'"(\w+)"', c["Senders"]))
    topo = type(TLC_TOPOLOGY)(TLC_TOPOLOGY.accounts, senders)
    rules = {k: c.get(k, "TRUE") == "TRUE" for k in KNOBS}
    rules["BindInitiator"] = bind
    w = World(topo, bounds_from(c), rules)
    forged = w.forge_initiation("b1", "a1")
    env = next(e for e in w.net if e[0] == "msg" and e[1] == "b1" and e[2] == "a1")
    received = w.recv(env)
    violations = w.check()
    return {"bind_initiator": bind, "forge_action": forged, "receive_action": received, "violations": violations}


def main() -> int:
    vectors = json.loads(VECTORS.read_text())["vectors"]
    vector = next(v for v in vectors if v["id"] == "V003-TLC-K12")
    vulnerable = run(False, vector)
    fixed = run(True, vector)
    passed = (
        vulnerable["receive_action"][-1] == "created"
        and "NoMisattributedSession" in vulnerable["violations"]
        and fixed["receive_action"][0] == "RecvRejectedUnboundInitiator"
        and "NoMisattributedSession" not in fixed["violations"]
    )
    payload = {
        "model": "v003_k12_python_reference_smoke",
        "gate": "V-003",
        "vector": "V003-TLC-K12",
        "passed": passed,
        "vulnerable_rule_disabled": vulnerable,
        "fixed_rule_enabled": fixed,
        "scope": "focused deterministic replay; full Hypothesis/TLC suite still belongs to verification/prototypes/v003-sessions/run.sh",
        "gate_status_effect": "evidence_only_no_gate_automatically_passed",
    }
    OUT.write_text(json.dumps(payload, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"passed": passed, "disabled": vulnerable, "enabled": fixed}, indent=1, sort_keys=True))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
