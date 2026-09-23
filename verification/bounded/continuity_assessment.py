#!/usr/bin/env python3
"""Bounded ContinuityAssessment model (ADR-0106, R-165).

Enumerates AccountControlState controller counts, RecoveryPolicy shapes and
quarantine states under the default majority rule floor(N/2)+1, computes the
loss/takeover tolerances Core must surface, and checks invariants:

  1. quarantine never lowers the takeover threshold (denominator rule);
  2. adding recovery never lowers any loss tolerance;
  3. adding a controller never lowers the takeover threshold;
  4. informed-off: a configuration needs explicit acknowledgement iff a single
     lost device causes identity loss or constitutional lockout;
  5. no configuration has a takeover threshold below 1.

Evidence only: this does not pass V-002 (see GATE_PASS_CRITERIA.md).
"""
from __future__ import annotations
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/continuity-assessment-latest.json"
INF = None  # JSON-friendly "unbounded by device loss"


def threshold(n: int) -> int:
    return n // 2 + 1


def assess(n: int, recovery: tuple, quarantined: int = 0) -> dict:
    kind = recovery[0]
    t = threshold(n)  # denominator is n regardless of quarantine (R-032)
    has_recovery = kind != "none"
    lost_identity = INF if has_recovery else n
    lost_lock = INF if has_recovery else n - t + 1
    takeover = t
    extra = None
    if kind == "kit":
        extra = "possession of the offline recovery kit"
    elif kind == "guardians":
        extra = f"{recovery[1]} of {recovery[2]} guardians colluding"
    needs_ack = (lost_identity == 1) or (lost_lock == 1)
    return {
        "controllers": n, "quarantined": quarantined, "recovery": kind if kind != "guardians" else f"guardians {recovery[1]}-of-{recovery[2]}",
        "threshold": t, "lost_devices_to_identity_loss": lost_identity,
        "lost_devices_to_constitution_lock": lost_lock,
        "compromised_devices_to_takeover": takeover, "alternate_takeover_path": extra,
        "needs_acknowledgement": needs_ack,
    }


def ge(a, b) -> bool:
    """a >= b where None means unbounded."""
    if a is None: return True
    if b is None: return False
    return a >= b


def main() -> int:
    recoveries = [("none",), ("kit",)] + [("guardians", m, k) for k in range(1, 8) for m in range(1, k + 1)]
    cases = 0
    violations: list[str] = []
    table = []
    for n in range(1, 10):
        for rec in recoveries:
            base = assess(n, rec)
            for q in range(0, n):
                cases += 1
                a = assess(n, rec, q)
                if a["compromised_devices_to_takeover"] < base["compromised_devices_to_takeover"]:
                    violations.append(f"quarantine lowered takeover n={n} q={q} rec={rec}")
            cases += 1
            if base["compromised_devices_to_takeover"] < 1:
                violations.append(f"takeover<1 n={n} rec={rec}")
            none = assess(n, ("none",))
            if not (ge(base["lost_devices_to_identity_loss"], none["lost_devices_to_identity_loss"]) and ge(base["lost_devices_to_constitution_lock"], none["lost_devices_to_constitution_lock"])):
                violations.append(f"recovery lowered loss tolerance n={n} rec={rec}")
            if n > 1:
                prev = assess(n - 1, rec)
                if base["compromised_devices_to_takeover"] < prev["compromised_devices_to_takeover"]:
                    violations.append(f"adding controller lowered takeover n={n} rec={rec}")
            expect_ack = base["lost_devices_to_identity_loss"] == 1 or base["lost_devices_to_constitution_lock"] == 1
            if base["needs_acknowledgement"] != expect_ack:
                violations.append(f"ack rule mismatch n={n} rec={rec}")
            if n <= 3 and rec[0] in ("none", "kit"):
                table.append(base)
    payload = {
        "model": "continuity_assessment_bounded",
        "rule": "majority floor(N/2)+1; quarantine does not shrink denominator",
        "cases": cases,
        "violations": violations,
        "passed": not violations,
        "common_configurations": table,
        "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        "supports_gates": ["V-002", "V-018"],
    }
    OUT.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: payload[k] for k in ("cases", "passed")}))
    for row in table:
        print(f"N={row['controllers']} recovery={row['recovery']:<5} lost->identity={row['lost_devices_to_identity_loss']} lost->lock={row['lost_devices_to_constitution_lock']} takeover={row['compromised_devices_to_takeover']} ack={row['needs_acknowledgement']}")
    return 0 if not violations else 1


if __name__ == "__main__":
    raise SystemExit(main())
