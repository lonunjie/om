#!/usr/bin/env python3
"""Bounded V-002 evidence for ADR-0154 verifier-local timer runtime segments and web-verifier liveness.

Architecture-state evidence only. Models one verifier's recovery-alone activation timer under the two
`durable_elapsed_clock` capabilities (boot_session, process_only) and the verifier's security profile.
Scenarios check the ADR-0154 credit rules; mutations deliberately break one rule each and must change
an outcome the spec fixes.
"""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/recovery-timer-adr0154-latest.json"
REQUIRED = 100


@dataclass
class Verifier:
    clock: str                      # "boot_session" | "process_only"
    profile: str = "recommended"    # "recommended" | "hardened" | "maximum"
    # mutation switches (each breaks exactly one ADR-0154 rule)
    m_process_death_breaks_boot: bool = False
    m_reboot_is_same_session: bool = False
    m_credit_from_last_checkpoint: bool = False
    m_skip_veto_sweep: bool = False
    m_wall_credit_under_hardened: bool = False
    m_boot_session_wall_across_reboot: bool = False
    durable: int = 0                # credited elapsed lower bound
    wall: int = 1_000_000
    wall_high_water: int = 1_000_000
    wall_at_checkpoint: int = 1_000_000
    wall_assisted: int = 0
    sweep_clear_this_segment: bool = False
    vetoed: bool = False
    trace: list = field(default_factory=list)

    def _wall_gap_allowed(self) -> bool:
        if self.clock == "boot_session":
            return False
        return self.profile == "recommended" or self.m_wall_credit_under_hardened

    def _credit_wall_gap(self) -> None:
        base = self.wall_at_checkpoint if self.m_credit_from_last_checkpoint else self.wall_high_water
        gap = max(0, self.wall - base)
        self.durable += gap
        self.wall_assisted += gap
        self.wall_high_water = max(self.wall_high_water, self.wall)

    def _checkpoint(self) -> None:
        self.wall_at_checkpoint = self.wall
        self.wall_high_water = max(self.wall_high_water, self.wall)

    def run(self, s: int) -> None:
        """App running: per-segment monotonic time is always credited."""
        self.durable += s
        self.wall += s
        self._checkpoint()
        self.trace.append(("run", s, self.elapsed))

    def wall_jump(self, d: int) -> None:
        self.wall += d
        self._checkpoint()
        self.trace.append(("wall_jump", d, self.elapsed))

    def process_exit(self, offline: int, wall_skew: int = 0) -> None:
        """App process dies; device stays up in the same OS boot session for `offline` real seconds.
        `wall_skew` models a wall-clock change made while the app is not running."""
        self.wall += offline + wall_skew
        self.sweep_clear_this_segment = False
        if self.clock == "boot_session" and not self.m_process_death_breaks_boot:
            self.durable += offline         # sleep-inclusive boot clock covers the gap
        elif self._wall_gap_allowed():
            self._credit_wall_gap()
        self._checkpoint()
        self.trace.append(("process_exit", offline, self.elapsed))

    def reboot(self, offline: int) -> None:
        self.wall += offline
        self.sweep_clear_this_segment = False
        if self.clock == "boot_session" and self.m_reboot_is_same_session:
            self.durable += offline
        elif self.clock == "boot_session" and self.m_boot_session_wall_across_reboot:
            self._credit_wall_gap()
        elif self._wall_gap_allowed():
            self._credit_wall_gap()
        self._checkpoint()
        self.trace.append(("reboot", offline, self.elapsed))

    def sweep(self, result: str) -> None:
        if result == "veto":
            self.vetoed = True
        self.sweep_clear_this_segment = result == "clear"
        self.trace.append(("sweep", result))

    @property
    def elapsed(self) -> int:
        return min(REQUIRED, self.durable)

    def can_accept(self) -> bool:
        if self.vetoed or self.elapsed < REQUIRED:
            return False
        if self.wall_assisted > 0 and not self.m_skip_veto_sweep:
            return self.sweep_clear_this_segment
        return True


def scenarios() -> list[dict]:
    rows = []

    def rec(name, got, exp, v):
        rows.append({"id": name, "ok": got == exp, "got": got, "expected": exp, "trace": v.trace})

    def s1(**m):
        v = Verifier("boot_session", **m); v.run(10); v.process_exit(90); return v, [v.can_accept(), v.wall_assisted]
    v, got = s1(); rec("S1_boot_session_process_death_keeps_credit", got, [True, 0], v)

    def s2(**m):
        v = Verifier("boot_session", **m); v.run(40); v.reboot(10_000); a = v.can_accept(); v.run(59); b = v.can_accept(); v.run(1)
        return v, [a, b, v.can_accept(), v.wall_assisted]
    v, got = s2(); rec("S2_boot_session_reboot_not_credited", got, [False, False, True, 0], v)

    def s3(**m):
        v = Verifier("process_only", **m); v.run(10); v.process_exit(90); a = v.can_accept(); v.sweep("clear")
        return v, [a, v.can_accept(), v.wall_assisted]
    v, got = s3(); rec("S3_web_gap_credit_requires_final_sweep", got, [False, True, 90], v)

    def s4(**m):
        # The clock is set back 500 s while running, then forward 500 s while the app is closed.
        # Real elapsed is 10 + 50 = 60. The high-water rule credits exactly the 50 s beyond the
        # highest wall time already seen; crediting from the last checkpoint would re-credit 500 s.
        v = Verifier("process_only", **m); v.run(10); v.wall_jump(-500); v.process_exit(50, wall_skew=+500)
        v.sweep("clear")
        return v, [v.elapsed, v.can_accept(), v.wall_assisted]
    v, got = s4(); rec("S4_backward_then_forward_jump_no_double_credit", got, [60, False, 50], v)

    def s5(**m):
        v = Verifier("process_only", profile="hardened", **m); v.run(10); v.process_exit(10_000); v.sweep("clear")
        return v, [v.elapsed, v.can_accept(), v.wall_assisted]
    v, got = s5(); rec("S5_hardened_web_verifier_monotonic_only", got, [10, False, 0], v)

    v = Verifier("process_only"); v.run(10); v.process_exit(90); v.sweep("unavailable"); a = v.can_accept(); v.sweep("veto"); b = v.can_accept(); v.sweep("clear")
    rec("S6_unavailable_sweep_pending_and_veto_final", [a, b, v.can_accept()], [False, False, False], v)

    v = Verifier("process_only"); v.run(10); v.process_exit(40); v.sweep("clear"); v.process_exit(50)
    rec("S7_sweep_must_be_in_current_segment", [v.elapsed, v.can_accept()], [100, False], v)

    v = Verifier("process_only", profile="maximum"); v.run(99); v.process_exit(5); v.run(1)
    rec("S8_maximum_accepts_on_monotonic_alone", [v.can_accept(), v.wall_assisted], [True, 0], v)
    return rows, {"s1": s1, "s2": s2, "s3": s3, "s4": s4, "s5": s5}


def mutations(fns) -> list[dict]:
    specs = [
        ("M1_process_death_treated_as_boot_break", "s1", {"m_process_death_breaks_boot": True}, [True, 0]),
        ("M2_new_boot_treated_as_same_session", "s2", {"m_reboot_is_same_session": True}, [False, False, True, 0]),
        ("M3_double_credit_after_backward_forward_jump", "s4", {"m_credit_from_last_checkpoint": True}, [60, False, 50]),
        ("M4_accept_wall_assisted_without_sweep", "s3", {"m_skip_veto_sweep": True}, [False, True, 90]),
        ("M5_wall_credit_under_hardened", "s5", {"m_wall_credit_under_hardened": True}, [10, False, 0]),
        ("M6_boot_session_wall_credit_across_reboot", "s2", {"m_boot_session_wall_across_reboot": True}, [False, False, True, 0]),
    ]
    rows = []
    for name, fn, mut, spec in specs:
        _, got = fns[fn](**mut)
        rows.append({"id": name, "caught": got != spec, "mutated": got, "spec": spec})
    return rows


def structural_checks() -> dict:
    adr = (ROOT / "decisions/ADR-0154-verifier-timer-runtime-segments-and-web-verifier-liveness.md").read_text()
    reg = json.loads((ROOT / "spec/registries/om-time-fields.json").read_text())
    row = next(x for x in reg["fields"] if x["object"] == "RecoveryPolicy" and x["field"] == "activation_policy.delay_seconds")
    reqs = {json.loads(x)["id"]: json.loads(x) for x in (ROOT / "machine/requirements.jsonl").read_text().splitlines() if x.strip()}
    threats = {json.loads(x)["id"] for x in (ROOT / "machine/threat_register.jsonl").read_text().splitlines() if x.strip()}
    inv = json.loads((ROOT / "machine/security_invariant_traceability.json").read_text())
    a3 = next(x for x in inv["invariants"] if x["id"] == "A3")
    return {
        "adr_defines_boot_session_segment": "Process death is not a segment break" in adr,
        "adr_high_water_rule": "max(0, wall_now - wall_high_water)" in adr,
        "adr_final_veto_sweep": "Final veto sweep" in adr,
        "adr_hardened_maximum_strict": "Strict under Hardened/Maximum Privacy" in adr,
        "registry_mentions_adr0154": "ADR-0154" in row["skew_policy"] and "T-132" in row["bad_clock_behavior"],
        "r340_present": "R-340" in reqs,
        "t132_present": "T-132" in threats,
        "a3_maps_r340_t132": "R-340" in a3["requirements"] and "T-132" in a3["threats"],
    }


def main() -> int:
    s, fns = scenarios()
    m = mutations(fns)
    st = structural_checks()
    failures = [f"scenario {x['id']}" for x in s if not x["ok"]]
    failures += [f"mutation not caught {x['id']}" for x in m if not x["caught"]]
    failures += [f"structural {k}" for k, v in st.items() if not v]
    result = {
        "checker": "recovery_timer_adr0154",
        "gates": ["V-002"],
        "requirements": ["R-340", "R-322"],
        "threats": ["T-132"],
        "scope": "ADR-0154 runtime-segment definition and guarded web-verifier wall-assisted gap credit",
        "passed": not failures,
        "summary": {"scenarios": len(s), "scenarios_passed": sum(x["ok"] for x in s),
                    "mutations": len(m), "mutations_caught": sum(x["caught"] for x in m)},
        "structural_checks": st,
        "scenarios": s,
        "mutations": m,
        "failures": failures,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True, default=list) + "\n")
    print(json.dumps(result["summary"]), "passed" if not failures else f"FAILED {failures}")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
