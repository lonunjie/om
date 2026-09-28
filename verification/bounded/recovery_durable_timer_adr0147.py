#!/usr/bin/env python3
"""Bounded V-002 evidence for ADR-0147 crash-safe verifier-local recovery timers.

Architecture-state evidence only. It models the required elapsed-time lower bound across
runtime segments and proves that reboot downtime, wall-clock changes and historical timer
restore cannot shorten a recovery-alone activation interval. Negative mutations deliberately
credit reboot downtime or trust restored historical elapsed state and must be detected.
"""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/recovery-durable-timer-adr0147-latest.json"


@dataclass
class Timer:
    required: int = 100
    observed: bool = False
    durable_elapsed: int = 0
    segment_elapsed: int = 0
    wall: int = 1_000_000
    checkpoint_wall: int = 1_000_000
    record_generation: int = 0
    mutation_credit_reboot_downtime: bool = False
    mutation_trust_restored_elapsed: bool = False
    trace: list[dict] = field(default_factory=list)

    def observe(self) -> None:
        self.observed = True
        self.durable_elapsed = 0
        self.segment_elapsed = 0
        self.record_generation += 1
        self.checkpoint_wall = self.wall
        self.trace.append({"op": "observe", "durable": 0, "wall": self.wall})

    def runtime(self, seconds: int) -> None:
        assert seconds >= 0
        self.segment_elapsed += seconds
        self.wall += seconds
        self.trace.append({"op": "runtime", "seconds": seconds, "elapsed_lb": self.elapsed_lower_bound})

    def wall_jump(self, seconds: int) -> None:
        self.wall += seconds
        self.trace.append({"op": "wall_jump", "seconds": seconds, "wall": self.wall, "elapsed_lb": self.elapsed_lower_bound})

    def checkpoint(self) -> None:
        self.durable_elapsed = min(self.required, self.durable_elapsed + self.segment_elapsed)
        self.segment_elapsed = 0
        self.record_generation += 1
        self.checkpoint_wall = self.wall
        self.trace.append({"op": "checkpoint", "durable": self.durable_elapsed, "generation": self.record_generation})

    def restart(self, *, downtime: int, clean: bool) -> None:
        if clean:
            self.checkpoint()
        # A crash loses the uncheckpointed current monotonic segment.
        self.segment_elapsed = 0
        before = self.durable_elapsed
        self.wall += downtime
        if self.mutation_credit_reboot_downtime:
            self.durable_elapsed = min(self.required, self.durable_elapsed + max(0, self.wall - self.checkpoint_wall))
        self.checkpoint_wall = self.wall
        self.trace.append({
            "op": "restart", "clean": clean, "downtime": downtime,
            "durable_before": before, "durable_after": self.durable_elapsed,
            "elapsed_lb": self.elapsed_lower_bound,
        })

    def restore_historical_timer(self, *, historical_elapsed: int, historical_generation: int) -> None:
        # ADR-0147: backup/private-sync timer state is not elapsed-time authority. A detected
        # historical/rolled-back record restarts (or otherwise lengthens) the interval.
        if self.mutation_trust_restored_elapsed:
            self.durable_elapsed = min(self.required, historical_elapsed)
            self.record_generation = historical_generation
        else:
            self.durable_elapsed = 0
            self.segment_elapsed = 0
            self.record_generation += 1
        self.checkpoint_wall = self.wall
        self.trace.append({
            "op": "restore_historical_timer", "historical_elapsed": historical_elapsed,
            "historical_generation": historical_generation,
            "trusted": self.mutation_trust_restored_elapsed,
            "durable_after": self.durable_elapsed,
        })

    def lose_or_corrupt_record(self) -> None:
        self.durable_elapsed = 0
        self.segment_elapsed = 0
        self.record_generation += 1
        self.checkpoint_wall = self.wall
        self.trace.append({"op": "timer_record_missing_or_corrupt", "durable_after": 0})

    @property
    def elapsed_lower_bound(self) -> int:
        return min(self.required, self.durable_elapsed + self.segment_elapsed)

    def can_accept(self) -> bool:
        return self.observed and self.elapsed_lower_bound >= self.required


def scenarios() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def record(name: str, got, expected, trace) -> None:
        ok = got == expected
        rows.append({"id": name, "ok": ok, "got": got, "expected": expected, "trace": trace})
        if not ok:
            failures.append(f"{name}: expected {expected!r}, got {got!r}")

    t = Timer(); t.observe(); t.runtime(40); t.restart(downtime=10_000, clean=True)
    t.runtime(59)
    before = t.can_accept(); t.runtime(1); after = t.can_accept()
    record("clean_restart_downtime_not_credited", [before, after, t.elapsed_lower_bound], [False, True, 100], t.trace)

    t = Timer(); t.observe(); t.runtime(40); t.checkpoint(); t.runtime(30); t.restart(downtime=50_000, clean=False)
    lost_segment_lb = t.elapsed_lower_bound; t.runtime(59); before = t.can_accept(); t.runtime(1); after = t.can_accept()
    record("crash_loses_uncheckpointed_elapsed_but_never_accelerates", [lost_segment_lb, before, after], [40, False, True], t.trace)

    t = Timer(); t.observe(); t.runtime(30); t.wall_jump(999_999); a = t.can_accept(); t.wall_jump(-1_500_000); t.runtime(69); b = t.can_accept(); t.runtime(1); c = t.can_accept()
    record("wall_clock_jumps_do_not_credit_elapsed", [a, b, c, t.elapsed_lower_bound], [False, False, True, 100], t.trace)

    t = Timer(); t.observe(); t.runtime(75); t.checkpoint(); t.lose_or_corrupt_record(); t.runtime(99); a = t.can_accept(); t.runtime(1); b = t.can_accept()
    record("missing_or_corrupt_record_restarts_interval", [a, b], [False, True], t.trace)

    t = Timer(); t.observe(); t.runtime(60); t.checkpoint(); t.restore_historical_timer(historical_elapsed=95, historical_generation=1); t.runtime(99); a = t.can_accept(); t.runtime(1); b = t.can_accept()
    record("historical_or_rolled_back_timer_state_is_not_authority", [a, b], [False, True], t.trace)

    return rows, failures


def mutations() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    t = Timer(mutation_credit_reboot_downtime=True); t.observe(); t.runtime(10); t.checkpoint(); t.restart(downtime=90, clean=False)
    caught = t.can_accept() and t.elapsed_lower_bound == 100
    rows.append({"id": "M1_credit_reboot_downtime_from_wall_clock", "caught": caught, "mutated_accepts": t.can_accept(), "trace": t.trace})
    if not caught:
        failures.append("M1 did not expose reboot-downtime acceleration")

    t = Timer(mutation_trust_restored_elapsed=True); t.observe(); t.runtime(20); t.checkpoint(); t.restore_historical_timer(historical_elapsed=99, historical_generation=999); t.runtime(1)
    caught = t.can_accept()
    rows.append({"id": "M2_trust_restored_elapsed_as_newer_authority", "caught": caught, "mutated_accepts": t.can_accept(), "trace": t.trace})
    if not caught:
        failures.append("M2 did not expose restored-timer acceleration")

    return rows, failures


def structural_checks() -> tuple[dict, list[str]]:
    adr = (ROOT / "decisions/ADR-0147-bootstrap-lifetime-restore-and-durable-timer-closure.md").read_text()
    registry = json.loads((ROOT / "spec/registries/om-time-fields.json").read_text())
    row = next(x for x in registry["fields"] if x["object"] == "RecoveryPolicy" and x["field"] == "activation_policy.delay_seconds")
    reqs = {json.loads(x)["id"]: json.loads(x) for x in (ROOT / "machine/requirements.jsonl").read_text().splitlines() if x.strip()}
    checks = {
        "adr_elapsed_lower_bound": "elapsed-time lower bound" in adr,
        "adr_unmeasured_downtime_not_counted": "Unmeasured powered-off/reboot time does not count" in adr,
        "adr_restored_state_fails_safe": "restored historical timer state fails safe" in adr,
        "registry_per_verifier": "each verifier measures from its own first valid RecoveryRequest observation" in row["skew_policy"],
        "registry_restart_lower_bound": "durable elapsed lower bound" in row["bad_clock_behavior"],
        "r322_present": "R-322" in reqs and "unmeasured reboot/offline time never accelerate" in reqs["R-322"]["requirement"],
    }
    failures = [f"structural check failed: {k}" for k, v in checks.items() if not v]
    return checks, failures


def main() -> int:
    s, sf = scenarios(); m, mf = mutations(); st, stf = structural_checks()
    failures = sf + mf + stf
    result = {
        "checker": "recovery_durable_timer_adr0147",
        "gates": ["V-002"],
        "requirements": ["R-322"],
        "scope": "ADR-0147 verifier-local crash-safe monotonic elapsed-time lower bound across restart/restore",
        "passed": not failures,
        "summary": {"scenarios": len(s), "scenarios_passed": sum(x["ok"] for x in s), "mutations": len(m), "mutations_caught": sum(x["caught"] for x in m)},
        "structural_checks": st,
        "scenarios": s,
        "mutations": m,
        "failures": failures,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
