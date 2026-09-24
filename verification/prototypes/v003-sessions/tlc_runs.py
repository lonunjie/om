#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

Runs TLC (pinned 1.7.4 via tools/tlc.sh) on verification/tla/OneToOneSessions.tla with the committed
configs verification/tla/OneToOneSessions*.cfg, listed in verification/tla/models-v003.json (so
`make tlc` checks them too). `tlc_runs.py gen` regenerates those configs from the tables below;
a normal run fails if they have drifted. Config kinds:
  * PASS configs  - every session-manager rule enabled; TLC must exhaust the state space cleanly;
  * GAP configs   - one rule disabled; TLC must find a counterexample violating the expected
                    invariant. Each counterexample becomes a regression vector.
  * FOUND vectors - counterexamples TLC found in earlier versions of the passing model
                    (recorded in found_traces.json; see README "Failing traces found").
Writes vectors to verification/vectors/one-to-one-session-regression-vectors.json and a summary to
work/tlc-summary.json. Usage: tlc_runs.py [gen | <config name>...]. Deterministic: gap runs use one TLC worker (BFS => shortest trace).
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
MODEL = ROOT / "verification/tla/OneToOneSessions.tla"
TLA = ROOT / "verification/tla"
MODELS = TLA / "models-v003.json"
VECTORS = ROOT / "verification/vectors/one-to-one-session-regression-vectors.json"
WORK = HERE / "work" / "tlc"

KNOBS = ["PersistBeforeSend", "DedupeDurable", "FloorFromACS", "RetryOnReset", "RecheckOnEncrypt",
         "ValidateAgainstACS", "ReconcileControlHead", "RestoreDropsSessions", "RejectFromRevoked",
         "PrekeyUntilConfirmed"]
INVARIANTS = ["TypeOK", "NoDoubleAccept", "NoKeyReuse", "NoSendToIneligible", "NoAcceptFromKnownRevoked",
              "NoSilentDowngrade", "DeliveredAtQuiescence", "SamePreferredAtQuiescence"]
BASE = {"Senders": '{"a1", "b1"}', "MaxMsgs": 2, "MaxSid": 4, "MaxCrash": 1, "MaxRestore": 0, "MaxDup": 1,
        "MaxWeak": 1, "MaxInject": 1, "RestoreDevs": '{"a1", "b1", "b3"}'}

PASS = {
    "pass-main": {},  # OneToOneSessions.cfg
    "pass-restore": {"MaxSid": 3, "RestoreDevs": '{"a1"}', "MaxCrash": 0, "MaxRestore": 1, "MaxInject": 0, "MaxDup": 0, "MaxWeak": 0},
    "pass-revoked-sender": {"Senders": '{"a1", "b1", "b3"}', "MaxCrash": 0, "MaxWeak": 0, "MaxInject": 0},
}
# gap id -> (knob disabled, bound overrides, expected violated invariant, classification, doc basis)
GAPS = {
    "K1": ("PersistBeforeSend", {"Senders": '{"a1"}', "MaxMsgs": 3, "MaxSid": 2, "MaxDup": 0, "MaxWeak": 0,
                                 "MaxInject": 0}, "NoKeyReuse",
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
    "K8": ("RestoreDropsSessions", {"Senders": '{"a1"}', "MaxMsgs": 3, "MaxSid": 2, "MaxCrash": 0, "MaxRestore": 1,
                                    "RestoreDevs": '{"a1"}', "MaxDup": 0, "MaxWeak": 0, "MaxInject": 0}, "NoKeyReuse",
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


PASS_CLAIMS = {
    "pass-main": "all rules K1..K10 on: S1..S5, C1, C2 hold with crash, duplicate delivery, weak bundle, rogue device, link+revoke",
    "pass-restore": "all rules on: S1..S5, C1, C2 hold across one backup restore (backups on a1, which both sends and receives)",
    "pass-revoked-sender": "all rules on: S1..S5, C1, C2 hold when the later-revoked device b3 is also a sender",
}


def cfg_file(name):
    return "OneToOneSessions.cfg" if name == "pass-main" else f"OneToOneSessions_{name.replace('-', '_')}.cfg"


def cfg_text(consts, invariants, comment):
    lines = [f"\\* {c}" for c in comment]
    lines += ["SPECIFICATION Spec", "CONSTANTS"]
    lines += [f"  {k} = {v}" for k, v in consts.items()]
    lines += ["CONSTRAINT StateBound", "INVARIANTS"] + [f"  {i}" for i in invariants]
    return "\n".join(lines) + "\n"


def consts_for(over, off=None):
    c = {**BASE, **over}
    for k in KNOBS:
        c[k] = "FALSE" if k == off else "TRUE"
    return c


def configs():
    """Every V-003 TLC config: (name, cfg text, models-v003.json entry, vector metadata or None)."""
    gen_note = "Generated by verification/prototypes/v003-sessions/tlc_runs.py gen; edit the generator, not this file."
    out = []
    for name, over in PASS.items():
        c = consts_for(over)
        out.append((name, cfg_text(c, INVARIANTS, [f"V-003 {name}: {PASS_CLAIMS[name]}.", gen_note]),
                    {"module": "OneToOneSessions", "config": cfg_file(name), "expect": "pass", "gate": "V-003",
                     "claim": PASS_CLAIMS[name]}, None))
    for gid, (knob, over, inv, cls, basis) in {**GAPS, **{k: (None, *v) for k, v in PROBES.items()}}.items():
        c = consts_for(over, knob)
        what = f"{knob} (rule {gid}) disabled" if knob else f"design-gap probe {gid}, all rules on"
        claim = f"{what}: counterexample violates {inv} ({cls})"
        out.append((gid, cfg_text(c, ["TypeOK", inv], [f"V-003 {claim}.", gen_note]),
                    {"module": "OneToOneSessions", "config": cfg_file(gid), "expect": "violation", "invariant": inv,
                     "gate": "V-003", "claim": claim},
                    {"knob": knob, "inv": inv, "cls": cls, "basis": basis, "consts": c}))
    return out


def gen():
    ents = []
    for name, text, entry, _ in configs():
        (TLA / entry["config"]).write_text(text)
        ents.append(entry)
    MODELS.write_text(json.dumps({"purpose": "V-003 1:1 session manager (OneToOneSessions.tla). 'pass' configs enable "
                                  "every rule K1..K10; each 'violation' config disables one rule (or probes a design gap) "
                                  "and its counterexample is a regression vector. Generated by "
                                  "verification/prototypes/v003-sessions/tlc_runs.py gen.",
                                  "models": ents}, indent=1) + "\n")
    print(f"wrote {len(ents)} configs and {MODELS.relative_to(ROOT)}")


def run_tlc(cfg, workers):
    env = dict(os.environ, CFG=str(TLA / cfg), WORKERS=str(workers))
    t0 = time.monotonic()
    p = subprocess.run([str(ROOT / "tools/tlc.sh"), str(MODEL)], capture_output=True, text=True, env=env)
    out = p.stdout + p.stderr
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / (cfg[:-4] + ".out")).write_text(out)
    return out, time.monotonic() - t0


ACT = re.compile(r"^/\\ lastAct = (<<.*>>)\s*$")


def parse(out):
    res = {"violated": None, "distinct_states": None, "depth": None, "complete": "No error has been found" in out}
    m = re.search(r"Invariant (\w+) is violated", out)
    if m:
        res["violated"] = m.group(1)
    m = re.search(r"([\d,]+) states generated, ([\d,]+) distinct states found, 0 states left", out)
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
    args = sys.argv[1:]
    if args[:1] == ["gen"]:
        gen()
        return 0
    only = set(args)
    drift = [e["config"] for _, text, e, _ in configs() if (TLA / e["config"]).read_text() != text]
    if drift or json.loads(MODELS.read_text())["models"] != [e for _, _, e, _ in configs()]:
        print("committed configs differ from the generator; run: tlc_runs.py gen", drift, file=sys.stderr)
        return 1
    summary, vectors, ok = {"pass": {}, "gaps": {}, "probes": {}}, [], True
    for name, _, entry, meta in configs():
        if only and name not in only:
            continue
        out, secs = run_tlc(entry["config"], 1 if meta else "auto")  # 1 worker: deterministic shortest trace
        r = parse(out)
        if meta is None:
            good = r["complete"] and r["violated"] is None
            summary["pass"][name] = {"config": entry["config"], "complete": r["complete"], "violated": r["violated"],
                                     "distinct_states": r["distinct_states"], "depth": r["depth"]}
            print(f"{name}: {'OK' if good else 'FAIL'} distinct={r['distinct_states']} depth={r['depth']} "
                  f"violated={r['violated']} {secs:.0f}s {r.get('error', '')}")
        else:
            good = r["violated"] == meta["inv"]
            summary["gaps" if meta["knob"] else "probes"][name] = {
                "config": entry["config"], "knob_disabled": meta["knob"], "expected": meta["inv"],
                "violated": r["violated"], "reproduced": good, "trace_length": len(r["trace"])}
            print(f"{name}: {'reproduced' if good else 'NOT reproduced'} expected={meta['inv']} got={r['violated']} "
                  f"steps={len(r['trace'])} {secs:.0f}s {r.get('error', '')}")
            vectors.append({"id": f"V003-TLC-{name}", "source": f"TLC counterexample (verification/tla/{entry['config']})",
                            "rule_disabled": meta["knob"], "violates": meta["inv"], "classification": meta["cls"],
                            "doc_basis": meta["basis"], "constants": {k: str(v) for k, v in meta["consts"].items()},
                            "actions": r["trace"]})
        ok &= good
    found = json.loads((HERE / "found_traces.json").read_text())
    if not only:
        VECTORS.write_text(json.dumps({"gate": "V-003",
                                       "description": "Regression vectors for the per-device 1:1 session manager. Each is an action trace; fuzzer.py replays them against the Python reference machine (sessions.py).",
                                       "vectors": found["vectors"] + vectors}, indent=1) + "\n")
        (HERE / "work" / "tlc-summary.json").write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    print("TLC:", "ALL OK" if ok else "FAILURES")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
