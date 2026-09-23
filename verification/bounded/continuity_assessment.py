#!/usr/bin/env python3
"""Bounded ContinuityAssessment model (ADR-0106 as amended by ADR-0111; R-165, R-183..R-185).

Enumerates controller counts, quarantine states and RecoveryPolicy shapes under the majority rule
floor(N/2)+1, with identity recovery ON by default. A recovery method is one vote; it can vote immediately
only together with >= 1 current controller, and alone only after the activation delay with no veto.
Checks:

  1. quarantine never lowers the device count needed for takeover (denominator rule, R-032);
  2. recovery never lowers any loss tolerance and never makes device-only takeover cheaper;
  3. a custodian alone never achieves immediate takeover (R-185);
  4. a custodian is worth exactly one vote: devices needed with custodian == max(threshold-1, 1);
  5. a default account (recovery on, custodian off-device) never needs acknowledgement; acknowledgement is
     needed iff recovery is off_explicit or pending, or the only custodian lives on the only controller;
  6. with recovery on, no configuration has identity loss or constitutional lockout from device loss alone;
  7. adding a controller never lowers the takeover threshold.

Evidence only: this does not pass V-002 (see GATE_PASS_CRITERIA.md); AccountControl.tla is the formal model.
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/continuity-assessment-latest.json"
UNBOUNDED = None  # JSON-friendly "not bounded by device loss while a custodian survives"


def threshold(n: int) -> int:
    return n // 2 + 1


def assess(n: int, state: str, custodians: tuple = (), guardians: tuple | None = None,
           quarantined: int = 0, custodian_on_only_device: bool = False) -> dict:
    t = threshold(n)  # the denominator stays n regardless of quarantine
    on = state == "on" and (custodians or guardians)
    lost_identity = UNBOUNDED if on else n
    lost_lock = UNBOUNDED if on else n - t + 1
    paths = [f"{t} device(s)"]
    devices_with_custodian = None
    if on:
        devices_with_custodian = max(t - 1, 1)
        if devices_with_custodian < t:
            paths.append(f"{devices_with_custodian} device(s) + custodian")
        paths.append("custodian + full delay with no live controller vetoing")
        if guardians:
            paths.append(f"{guardians[0]} of {guardians[1]} guardians colluding + delay with no veto")
    needs_ack = (state in ("off_explicit", "pending")) or (on and n == 1 and custodian_on_only_device)
    return {
        "controllers": n, "quarantined": quarantined, "recovery_state": state,
        "custodians": list(custodians) + ([f"guardians {guardians[0]}-of-{guardians[1]}"] if guardians else []),
        "threshold": t,
        "lost_devices_to_identity_loss": lost_identity,
        "lost_devices_to_constitution_lock": lost_lock,
        "devices_for_immediate_takeover": t,
        "devices_for_immediate_takeover_with_custodian": devices_with_custodian,
        "custodian_alone_immediate": False,  # by construction of R-185; asserted below against the rule
        "takeover_paths": paths,
        "needs_acknowledgement": needs_ack,
    }


def ge(a, b) -> bool:
    """a >= b where None means unbounded."""
    if a is None:
        return True
    if b is None:
        return False
    return a >= b


def immediate_takeover_possible(n: int, devices: int, has_custodian: bool) -> bool:
    """Direct encoding of R-185: votes = devices + (1 if custodian and devices >= 1)."""
    votes = devices + (1 if has_custodian and devices >= 1 else 0)
    return votes >= threshold(n)


def main() -> int:
    shapes = [("off_explicit", (), None), ("pending", (), None),
              ("on", ("synced_passkey",), None), ("on", ("recovery_key",), None),
              ("on", ("synced_passkey", "recovery_key"), None)]
    shapes += [("on", ("synced_passkey",), (m, k)) for k in range(2, 8) for m in range(2, k + 1)]
    cases, violations, table = 0, [], []
    for n in range(1, 10):
        for state, cust, guard in shapes:
            for on_device in (False, True):
                base = assess(n, state, cust, guard, custodian_on_only_device=on_device)
                cases += 1
                for q in range(n):
                    cases += 1
                    a = assess(n, state, cust, guard, q, on_device)
                    if a["devices_for_immediate_takeover"] < base["devices_for_immediate_takeover"]:
                        violations.append(f"quarantine lowered takeover n={n} q={q}")
                off = assess(n, "off_explicit")
                if not (ge(base["lost_devices_to_identity_loss"], off["lost_devices_to_identity_loss"])
                        and ge(base["lost_devices_to_constitution_lock"], off["lost_devices_to_constitution_lock"])):
                    violations.append(f"recovery lowered loss tolerance n={n} {state}{cust}")
                if base["devices_for_immediate_takeover"] < off["devices_for_immediate_takeover"]:
                    violations.append(f"recovery made device-only takeover cheaper n={n}")
                has_c = state == "on"
                if immediate_takeover_possible(n, 0, has_c):
                    violations.append(f"custodian alone immediate n={n}")
                if has_c:
                    need = min(d for d in range(0, n + 1) if immediate_takeover_possible(n, d, True))
                    if need != base["devices_for_immediate_takeover_with_custodian"]:
                        violations.append(f"one-vote rule mismatch n={n}: model {need} vs assessment "
                                          f"{base['devices_for_immediate_takeover_with_custodian']}")
                    if need != max(threshold(n) - 1, 1):
                        violations.append(f"custodian not worth exactly one vote n={n}")
                    if base["lost_devices_to_identity_loss"] is not None or base["lost_devices_to_constitution_lock"] is not None:
                        violations.append(f"recovery on still loses identity/locks by device loss n={n}")
                expect_ack = state in ("off_explicit", "pending") or (has_c and n == 1 and on_device)
                if base["needs_acknowledgement"] != expect_ack:
                    violations.append(f"ack rule mismatch n={n} {state} on_device={on_device}")
                if has_c and not on_device and base["needs_acknowledgement"]:
                    violations.append(f"default account needs acknowledgement n={n}")
                if n > 1:
                    prev = assess(n - 1, state, cust, guard, custodian_on_only_device=on_device)
                    if base["devices_for_immediate_takeover"] < prev["devices_for_immediate_takeover"]:
                        violations.append(f"adding controller lowered takeover n={n}")
                if n <= 3 and not on_device and guard is None and cust in ((), ("synced_passkey",)):
                    table.append(base)
    payload = {
        "model": "continuity_assessment_bounded",
        "rule": "majority floor(N/2)+1; quarantine does not shrink denominator; recovery on by default is one vote, immediate only with a controller co-sign (ADR-0111)",
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
        print(f"N={row['controllers']} {row['recovery_state']:<12} {','.join(row['custodians']) or '-':<15} "
              f"lost->identity={row['lost_devices_to_identity_loss']} lost->lock={row['lost_devices_to_constitution_lock']} "
              f"takeover={' | '.join(row['takeover_paths'])} ack={row['needs_acknowledgement']}")
    return 0 if not violations else 1


if __name__ == "__main__":
    raise SystemExit(main())
