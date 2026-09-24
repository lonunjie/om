#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

Combines work/tlc-summary.json (tlc_runs.py) and work/fuzz.json (fuzzer.py) into the deterministic
verification/results/v003-sessions-latest.json (no timestamps, no absolute paths, no timings).
Usage: results.py <tool version strings...>
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OUT = ROOT / "verification/results/v003-sessions-latest.json"


def main() -> int:
    tlc = json.loads((HERE / "work/tlc-summary.json").read_text())
    fz = json.loads((HERE / "work/fuzz.json").read_text())
    pass_ok = all(r["complete"] and r["violated"] is None for r in tlc["pass"].values())
    gaps_ok = all(r["reproduced"] for r in {**tlc["gaps"], **tlc["probes"]}.values())
    replay_ok = all(r["ok"] for r in fz["replays"])
    fuzz_ok = fz["fuzz"]["all_rules"]["violation"] is None
    ok = pass_ok and gaps_ok and replay_ok and fuzz_ok
    ev_model = ["verification/tla/OneToOneSessions.tla", "verification/tla/models-v003.json",
                "verification/prototypes/v003-sessions/sessions.py", "verification/prototypes/v003-sessions/fuzzer.py"]
    ev_vec = ["verification/vectors/one-to-one-session-regression-vectors.json",
              "verification/prototypes/v003-sessions/fuzzer_vectors.json",
              "verification/prototypes/v003-sessions/found_traces.json"]
    criteria = [
        {"gate": "V-003", "criterion": "executable model/fuzzer of the per-device 1:1 session manager covering crossed "
                                       "initiation, add/revoke races, stale/reset sessions, dedupe and persistence",
         "status": "met" if ok else "partial", "evidence": ev_model,
         "note": "TLA+ model (TLC, bounded: 3 real devices, 2-3 messages, <=4 sessions) plus an independently written "
                 "Python reference machine driven by hypothesis on a larger topology (5 devices, 5 messages). Rules "
                 "K1..K11 all on: every pass config exhausts its state space and the all-rules fuzz finds nothing."},
        {"gate": "V-003", "criterion": "every discovered failing trace captured as a vector",
         "status": "met" if gaps_ok and replay_ok else "partial", "evidence": ev_vec,
         "note": "3 traces found against earlier model versions, 11 TLC gap/probe counterexamples (one per disabled "
                 "rule + black-hole probe), fuzzer-shrunk traces per disabled rule; TLC traces replay step-for-step "
                 "on the Python machine."},
        {"gate": "V-003", "criterion": "model reviewed by someone other than its author", "status": "unmet",
         "evidence": [], "note": "Needs an independent reviewer (ADR-0117 decides whether an AI reviewer counts)."},
    ]
    res = {
        "gates": ["V-003"],
        "criteria": criteria,
        "independence": "Model, reference machine and fuzzer written by a Claude agent; the design package was authored "
                        "by a ChatGPT design agent and Claude agents; ADR-0117 (proposed) decides whether an AI reviewer "
                        "counts as independent. The Python machine was written from the docs, not translated from the "
                        "TLA+ model; step-label agreement on every TLC trace is cross-implementation evidence only.",
        "tool_versions": sorted(sys.argv[1:]),
        "tlc": tlc,
        "fuzz": fz,
        "summary": {"ok": ok, "pass_configs": len(tlc["pass"]), "gap_configs": len(tlc["gaps"]),
                    "probe_configs": len(tlc["probes"]), "replays": len(fz["replays"]),
                    "design_gaps": sorted(k for k in tlc["gaps"]) + sorted(tlc["probes"])},
    }
    OUT.write_text(json.dumps(res, indent=1, sort_keys=True) + "\n")
    print("results:", "OK" if ok else "FAIL", OUT.relative_to(ROOT))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
