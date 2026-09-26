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
    rr = json.loads((HERE / "work/restore-redelivery.json").read_text())
    fl = json.loads((HERE / "work/floor.json").read_text())
    ok = pass_ok and gaps_ok and replay_ok and fuzz_ok and rr["ok"] and fl["ok"]
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
                 "K1..K12 all on: every pass config exhausts its state space and the all-rules fuzz finds nothing."},
        {"gate": "V-003", "criterion": "every discovered failing trace captured as a vector",
         "status": "met" if gaps_ok and replay_ok else "partial", "evidence": ev_vec,
         "note": "3 traces found against earlier model versions, 11 TLC gap/probe counterexamples (one per disabled "
                 "rule + black-hole probe), fuzzer-shrunk traces per disabled rule; TLC traces replay step-for-step "
                 "on the Python machine."},
        {"gate": "V-003", "criterion": "model reviewed by someone other than its author", "status": "unmet",
         "evidence": [], "note": "Needs an independent (non-Anthropic) reviewer under ADR-0117; batched in the final review queue."},
        {"gate": "V-003", "criterion": "restore+redelivery is covered by a dedicated executable scenario or an explicitly "
                                       "equivalent bounded trace, not only an informal argument",
         "status": "met" if rr["ok"] else "unmet",
         "evidence": ["verification/prototypes/v003-sessions/restore_redelivery.py", "verification/prototypes/v003-sessions/sessions.py"],
         "note": f"ADR-0131 restore (fresh DeviceID via the control chain, no delivery exemption) explored exhaustively: "
                 f"{rr['exhaustive']['stats']['states']} states, {rr['exhaustive']['stats']['misdeliveries']} old-queue redeliveries, "
                 f"{rr['exhaustive']['stats']['dup_after_restore']} duplicate deliveries after restore, no invariant violated; a scripted golden "
                 "trace delivers the post-restore message exactly once. Finding: the backup import must MERGE with a live installation "
                 "(the overwrite mutation loses an accepted message). Residual: messages sent only to the lost device between its backup "
                 f"and the sender learning the restore are not re-encrypted (max gap {rr['residual']['history_gap_max_messages']})."},
        {"gate": "V-003", "criterion": "negative vector rejects a peer profile below the current security_generation and rejects "
                                       "a floor move to a lower generation after a SecurityAdvisory (R-206)",
         "status": "met" if fl["ok"] else "unmet",
         "evidence": ["verification/prototypes/v003-sessions/floor.py", "verification/vectors/one-to-one-floor-vectors.json"],
         "note": f"{fl['vectors']} vectors driven by spec/registries/om-crypto-profiles.json (+ vector-only experimental profiles); "
                 f"negatives: {', '.join(fl['negative'])}."},
    ]
    res = {
        "restore_redelivery": {"ok": rr["ok"], "stats": rr["exhaustive"]["stats"], "residual": rr["residual"],
                               "mutation_overwrite_import": rr["mutation_overwrite_import"]["violated"]},
        "floor": fl,
        "gates": ["V-003"],
        "criteria": criteria,
        "independence": "Model, reference machine and fuzzer written by a Claude agent; the design package was authored "
                        "by a ChatGPT design agent and Claude agents; under ADR-0117 a non-Anthropic reviewer is required for "
                        "pre-engineering. The Python machine was written from the docs, not translated from the "
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
