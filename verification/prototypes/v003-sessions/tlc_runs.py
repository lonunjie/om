#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

Runs TLC (pinned 1.7.4 via tools/tlc.sh) on verification/tla/OneToOneSessions.tla in:
  * PASS configs  - every session-manager rule enabled; TLC must exhaust the state space cleanly;
  * GAP configs   - one rule disabled; TLC must find a counterexample violating the expected
                    invariant. Each counterexample becomes a regression vector.
  * FOUND vectors - counterexamples TLC found in earlier versions of the passing model
                    (recorded in found_traces.json; see README "Failing traces found").
Writes vectors to verification/vectors/one-to-one-session-regression-vectors.json and a summary to
work/tlc-summary.json. Deterministic: gap runs use one TLC worker (BFS => shortest trace).
"""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
MODEL = ROOT / "verification/tla/OneToOneSessions.tla"
MAIN_CFG = ROOT / "verification/tla/OneToOneSessions.cfg"
VECTORS = ROOT / "verification/vectors/one-to-one-session-regression-vectors.json"
WORK = HERE / "work" / "tlc"

KNOBS = ["PersistBeforeSend", "DedupeDurable", "FloorFromACS", "RetryOnReset", "RecheckOnEncrypt",
         "ValidateAgainstACS", "ReconcileControlHead", "RestoreDropsSessions", "RejectFromRevoked",
         "PrekeyUntilConfirmed"]
INVARIANTS = ["TypeOK", "NoDoubleAccept", "NoKeyReuse", "NoSendToIneligible", "NoAcceptFromKnownRevoked",
              "NoSilentDowngrade", "DeliveredAtQuiescence", "SamePreferredAtQuiescence"]
BASE = {"Senders": '{"a1", "b1"}', "MaxMsgs": 2, "MaxSid": 4, "MaxCrash": 1, "MaxRestore": 0, "MaxDup": 1,
        "MaxWeak": 1, "MaxInject": 1}

PASS = {
    "pass-main": {},  # the committed OneToOneSessions.cfg
    "pass-restore": {"MaxCrash": 0, "MaxRestore": 1, "MaxInject": 0, "MaxDup": 0, "MaxWeak": 0},
    "pass-revoked-sender": {"Senders": '{"a1", "b1", "b3"}', "MaxCrash": 0, "MaxWeak": 0, "MaxInject": 0},
}
# gap id -> (knob disabled, bound overrides, expected violated invariant, classification, doc basis)
GAPS = {
    "K1": ("PersistBeforeSend", {"MaxMsgs": 2, "MaxWeak": 0, "MaxInject": 0}, "NoKeyReuse",
           "design_gap_mechanism_unstated",
           "ONE_TO_ONE_SESSION_MANAGEMENT.md §7 requires no nonce/key reuse after restore but no doc states write-ahead ordering for ratchet state after a crash"),
    "K2": ("DedupeDurable", {"MaxWeak": 0, "MaxInject": 0}, "NoDoubleAccept",
           "design_gap_mechanism_unstated",
           "R-089 requires one conversation event per logical message; durability of the EventID dedupe record relative to the relay ack is unspecified"),
    "K3": ("FloorFromACS", {"MaxCrash": 0, "MaxRestore": 1, "MaxInject": 0}, "NoSilentDowngrade",
           "design_gap",
           "CRYPTOGRAPHY_AND_AGILITY.md 'Downgrade floor' keeps the floor as local relationship state; a backup restore rolls it back and a weak bundle is then accepted silently. No doc says where the floor survives restore"),
    "K4": ("RetryOnReset", {"MaxCrash": 0, "MaxRestore": 1, "MaxWeak": 0, "MaxInject": 0}, "DeliveredAtQuiescence",
           "design_gap",
           "ONE_TO_ONE_SESSION_MANAGEMENT.md §2 lists 'session reset/recovery' but no doc requires re-encrypting messages the peer could not decrypt; they are silently lost"),
    "K5": ("RecheckOnEncrypt", {"MaxCrash": 0, "MaxWeak": 0, "MaxInject": 0}, "NoSendToIneligible",
           "design_gap_partially_stated",
           "ONE_TO_ONE_SESSION_MANAGEMENT.md §5 'Stale sender view ... must be defined'; a fanout/retry queued before the sender learned a revocation still encrypts to the revoked device"),
    "K6": ("ValidateAgainstACS", {"MaxCrash": 0, "MaxWeak": 0}, "NoSendToIneligible",
           "negative_control_doc_explicit",
           "ONE_TO_ONE_SESSION_MANAGEMENT.md §3: provider device lists are not authority (stated; this vector guards it)"),
    "K7": ("ReconcileControlHead", {"MaxWeak": 0, "MaxInject": 0}, "NoSendToIneligible",
           "design_gap_partially_stated",
           "ACCOUNT_CONTINUITY_AND_CONTROL.md §7/§9 cover the own account's head after restore; nothing requires a device's validated view of a PEER's AccountControlState head to be durable, so a crash re-authorizes a revoked peer device"),
    "K8": ("RestoreDropsSessions", {"MaxCrash": 0, "MaxRestore": 1, "MaxWeak": 0, "MaxInject": 0}, "NoKeyReuse",
           "negative_control_doc_explicit",
           "ONE_TO_ONE_SESSION_MANAGEMENT.md §7 forbids rollback that causes key reuse (stated; this vector guards it)"),
    "K9": ("RejectFromRevoked", {"Senders": '{"a1", "b1", "b3"}', "MaxCrash": 0, "MaxWeak": 0, "MaxInject": 0},
           "NoAcceptFromKnownRevoked", "design_gap_partially_stated",
           "MULTI_DEVICE_SYNC.md 'Revocation' covers what a revoked device stops receiving; no doc says receivers must reject traffic FROM a device they know is revoked (a compromised revoked device can keep injecting messages)"),
    "K10": ("PrekeyUntilConfirmed", {"MaxCrash": 0, "MaxWeak": 0, "MaxInject": 0}, "SamePreferredAtQuiescence",
            "design_gap",
            "Q-050 names an Initiating state but no doc says messages on an unconfirmed session must carry the handshake header; when a later message overtakes the first one, the receiver resets a healthy session and the pair ends preferring different sessions"),
}
PROBES = {
    "P1-black-hole": ({"MaxWeak": 0, "MaxInject": 0}, "NoBlackHoleAtQuiescence", "design_gap",
                      "After a peer loses a session (crash/restore) the other side keeps preferring it until its next send triggers reset+retry: preference is lazily repaired. Acceptable only if reset+retry (K4) is normative"),
}


def cfg_text(consts, invariants):
    lines = ["SPECIFICATION Spec", "CONSTANTS"]
    lines += [f"  {k} = {v}" for k, v in consts.items()]
    lines += ["CONSTRAINT StateBound", "INVARIANTS"] + [f"  {i}" for i in invariants]
    return "\n".join(lines) + "\n"


def consts_for(over, off=None):
    c = {**BASE, **over}
    for k in KNOBS:
        c[k] = "FALSE" if k == off else "TRUE"
    return c


def run_tlc(name, cfg, workers):
    d = WORK / name
    if d.exists():
        shutil.rmtree(d)
    d.mkdir(parents=True)
    shutil.copy(MODEL, d / MODEL.name)
    (d / "OneToOneSessions.cfg").write_text(cfg)
    args = [str(ROOT / "tools/tlc.sh"), str(d / MODEL.name)] + (["-workers", "1"] if workers == 1 else [])
    p = subprocess.run(args, capture_output=True, text=True)
    out = p.stdout + p.stderr
    (d / "tlc.out").write_text(out)
    return out


ACT = re.compile(r"^/\\ lastAct = (<<.*>>)\s*$")


def parse(out):
    res = {"violated": None, "distinct_states": None, "depth": None, "complete": "No error has been found" in out}
    m = re.search(r"Invariant (\w+) is violated", out)
    if m:
        res["violated"] = m.group(1)
    m = re.search(r"([\d,]+) states generated, ([\d,]+) distinct states found", out)
    if m:
        res["distinct_states"] = int(m.group(2).replace(",", ""))
    m = re.search(r"depth of the complete state graph search is (\d+)", out)
    if m:
        res["depth"] = int(m.group(1))
    trace = []
    for line in out.splitlines():
        a = ACT.match(line.strip())
        if a:
            trace.append(json.loads(a.group(1).replace("<<", "[").replace(">>", "]")))
    res["trace"] = trace
    if "Error:" in out and not res["violated"]:
        res["error"] = next(l for l in out.splitlines() if l.startswith("Error:"))
    return res


def main():
    only = set(sys.argv[1:])
    summary, vectors, ok = {"pass": {}, "gaps": {}, "probes": {}}, [], True
    for name, over in PASS.items():
        if only and name not in only:
            continue
        cfg = MAIN_CFG.read_text() if name == "pass-main" else cfg_text(consts_for(over), INVARIANTS)
        r = parse(run_tlc(name, cfg, workers=0))
        good = r["complete"] and r["violated"] is None
        ok &= good
        summary["pass"][name] = {"complete": r["complete"], "violated": r["violated"],
                                 "distinct_states": r["distinct_states"], "depth": r["depth"],
                                 "constants": {k: str(v) for k, v in (consts_for(over)).items()}}
        print(f"{name}: {'OK' if good else 'FAIL'} distinct={r['distinct_states']} depth={r['depth']} violated={r['violated']}")
    for gid, (knob, over, inv, cls, basis) in {**GAPS, **{k: (None, *v) for k, v in PROBES.items()}}.items():
        if only and gid not in only:
            continue
        invs = ["TypeOK", inv]  # target only: a disabled rule may break several invariants
        c = consts_for(over, knob)
        r = parse(run_tlc(gid, cfg_text(c, invs), workers=1))
        good = r["violated"] == inv
        ok &= good
        bucket = "gaps" if knob else "probes"
        summary[bucket][gid] = {"knob_disabled": knob, "expected": inv, "violated": r["violated"], "reproduced": good,
                                "trace_length": len(r["trace"])}
        print(f"{gid}: {'reproduced' if good else 'NOT reproduced'} expected={inv} got={r['violated']} steps={len(r['trace'])}")
        vectors.append({"id": f"V003-TLC-{gid}", "source": "TLC counterexample (OneToOneSessions.tla)",
                        "rule_disabled": knob, "violates": inv, "classification": cls, "doc_basis": basis,
                        "constants": {k: str(v) for k, v in c.items()}, "actions": r["trace"]})
    found = json.loads((HERE / "found_traces.json").read_text())
    if not only:
        VECTORS.write_text(json.dumps({"gate": "V-003",
                                       "description": "Regression vectors for the per-device 1:1 session manager. Each is an action trace; replay_fuzzer.py replays them against the Python reference machine.",
                                       "vectors": found["vectors"] + vectors}, indent=1) + "\n")
        (HERE / "work" / "tlc-summary.json").write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    print("TLC:", "ALL OK" if ok else "FAILURES")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
