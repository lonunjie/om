#!/usr/bin/env python3
"""Bounded V-003/V-015 evidence for ADR-0146 final bootstrap/resource closure.

Architecture-state evidence only. Models atomic sibling terminalization, terminal-state races,
one-logical-response/bounded transport use and wrapper/key/route/request expiry ordering.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/sealed-delivery-adr0146-final-closure-latest.json"
MAX_CANDIDATES = 8


def fp(obj: object) -> str:
    return hashlib.sha256(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@dataclass(frozen=True)
class Wrapper:
    key_id: str
    delivery_id: str
    body: str
    expires_at: int = 1800
    route: str = "reply-route-1"

    @property
    def fingerprint(self) -> str:
        return fp(self.__dict__)


@dataclass
class Replay:
    fingerprint: str
    expires_at: int
    state: str = "RESERVED"  # RESERVED | COMPLETE | REJECTED


@dataclass
class World:
    now: int = 1000
    key_expires: int = 2000
    route_expires: int = 2000
    request_state: str = "OPEN"
    private_key_present: bool = True
    replay: dict[tuple[str, str], Replay] = field(default_factory=dict)
    winner: tuple[str, str] | None = None
    durable_effects: int = 0
    mutation_leave_siblings_reserved: bool = False
    mutation_physical_one_use: bool = False
    mutation_unbounded_candidates: bool = False
    mutation_ignore_expiry_context: bool = False
    deposits: int = 0

    def _existing(self, w: Wrapper):
        return self.replay.get((w.key_id, w.delivery_id))

    def admit(self, w: Wrapper) -> str:
        rk = (w.key_id, w.delivery_id)
        old = self.replay.get(rk)
        if old:
            if old.fingerprint != w.fingerprint:
                return "collision"
            if old.state == "REJECTED":
                return "rejected_terminal"
            if old.state == "COMPLETE":
                return "complete_idempotent_ack"
            if not self.private_key_present:
                return "key_missing_reserved"
            if self.mutation_physical_one_use and self.deposits >= 1:
                return "route_consumed"
            self.deposits += 1
            return "resume_reserved"

        if self.request_state != "OPEN":
            return "bootstrap_closed"
        if not self.private_key_present:
            return "key_missing"
        if not self.mutation_ignore_expiry_context:
            # ADR-0147: MessageRequest v1 has no independent request_expires; the
            # request acceptance deadline is exactly the authenticated route/key minimum.
            limit = min(self.key_expires, self.route_expires)
            if w.expires_at > limit or self.now > w.expires_at:
                return "expiry_out_of_context"
        if self.mutation_physical_one_use and self.deposits >= 1:
            return "route_consumed"
        if not self.mutation_unbounded_candidates:
            # Lifetime-per-request budget: terminal/rejected candidates still consumed admission work/storage.
            distinct = len(self.replay)
            if distinct >= MAX_CANDIDATES:
                return "candidate_limit"
        self.deposits += 1
        self.replay[rk] = Replay(w.fingerprint, w.expires_at)
        return "reserved"

    def close(self, terminal: str, winner: Wrapper | None = None) -> str:
        if self.request_state != "OPEN":
            return "lost_race"
        if terminal == "COMPLETE_REPLAY_ONLY":
            assert winner is not None
            rk = (winner.key_id, winner.delivery_id)
            r = self.replay.get(rk)
            if r is None or r.fingerprint != winner.fingerprint or r.state != "RESERVED":
                return "winner_not_reserved"
            r.state = "COMPLETE"
            self.winner = rk
            self.durable_effects += 1
        for rk, r in self.replay.items():
            if winner is not None and rk == (winner.key_id, winner.delivery_id):
                continue
            if r.state == "RESERVED" and not self.mutation_leave_siblings_reserved:
                r.state = "REJECTED"
        self.request_state = terminal
        self.private_key_present = False
        return "closed"


def scenario_rows():
    rows, failures = [], []
    def rec(name, got, exp):
        ok = got == exp
        rows.append({"id": name, "ok": ok, "got": got, "expected": exp})
        if not ok: failures.append(f"{name}: expected {exp!r}, got {got!r}")

    a = Wrapper("k", "a", "A")
    b = Wrapper("k", "b", "B")

    w = World()
    got = [w.admit(a), w.admit(b), w.close("COMPLETE_REPLAY_ONLY", a),
           w.replay[("k", "a")].state, w.replay[("k", "b")].state,
           w.private_key_present, w.admit(b)]
    rec("winner_atomically_terminalizes_reserved_siblings", got,
        ["reserved", "reserved", "closed", "COMPLETE", "REJECTED", False, "rejected_terminal"])

    w = World()
    got = [w.admit(a), w.close("CANCELLED"), w.close("COMPLETE_REPLAY_ONLY", a),
           w.replay[("k", "a")].state, w.private_key_present]
    rec("cancel_wins_same_open_cas", got, ["reserved", "closed", "lost_race", "REJECTED", False])

    w = World()
    got = [w.admit(a), w.close("COMPLETE_REPLAY_ONLY", a), w.close("CANCELLED"), w.admit(a)]
    rec("accept_wins_same_open_cas_and_exact_retry_survives", got,
        ["reserved", "closed", "lost_race", "complete_idempotent_ack"])

    w = World()
    got = [w.admit(a), w.admit(a), len(w.replay)]
    rec("exact_retry_does_not_consume_candidate_slot", got, ["reserved", "resume_reserved", 1])

    w = World()
    first8 = [w.admit(Wrapper("k", f"d{i}", str(i))) for i in range(8)]
    ninth = w.admit(Wrapper("k", "d8", "8"))
    rec("distinct_candidate_cap_is_eight", [first8, ninth, len(w.replay)], [["reserved"] * 8, "candidate_limit", 8])

    for name, kwargs in [
        ("wrapper_beyond_key_expiry", {"key_expires": 1500}),
        ("wrapper_beyond_route_expiry", {"route_expires": 1500}),
    ]:
        w = World(**kwargs)
        rec(name, [w.admit(a), len(w.replay)], ["expiry_out_of_context", 0])

    return rows, failures


def mutation_rows():
    rows, failures = [], []
    def caught(name, got, exp):
        ok = got == exp
        rows.append({"id": name, "caught": ok, "mutated_result": got})
        if not ok: failures.append(f"mutation {name} not observed: {got!r}")

    a = Wrapper("k", "a", "A")
    b = Wrapper("k", "b", "B")

    w = World(mutation_leave_siblings_reserved=True)
    w.admit(a); w.admit(b); w.close("COMPLETE_REPLAY_ONLY", a)
    caught("M1_sibling_left_reserved_after_key_destroy", [w.replay[("k", "b")].state, w.admit(b)],
           ["RESERVED", "key_missing_reserved"])

    w = World(mutation_physical_one_use=True)
    caught("M2_physical_one_use_breaks_exact_retry", [w.admit(a), w.admit(a)], ["reserved", "route_consumed"])

    w = World(mutation_unbounded_candidates=True)
    vals = [w.admit(Wrapper("k", f"d{i}", str(i))) for i in range(9)]
    caught("M3_unbounded_distinct_candidates_pin_state", [vals[-1], len(w.replay)], ["reserved", 9])

    w = World(key_expires=1500, mutation_ignore_expiry_context=True)
    caught("M4_wrapper_outlives_key_context", [w.admit(a), len(w.replay)], ["reserved", 1])

    return rows, failures


def structural_checks():
    adr = (ROOT / "decisions/ADR-0146-bootstrap-candidate-terminalization-bounded-reply-transport-and-expiry.md").read_text()
    cddl = (ROOT / "spec/cddl/discovery.cddl").read_text()
    profile = (ROOT / "docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md").read_text()
    invariant = (ROOT / "docs/architecture/safety/SECURITY_INVARIANTS.md").read_text()
    checks = {
        "adr_atomic_sibling_terminalization": "mark every other `RESERVED` candidate" in adr,
        "adr_candidate_cap_8": "at most **8 distinct candidate" in adr,
        "adr_expiry_context": "wrapper.expires_at <= delivery_key.expires_at" in adr,
        "cddl_not_physical_one_use": "one logical response; bounded transport use" in cddl,
        "profile_candidate_cap": "8 distinct candidate" in profile,
        "m8_adr0146": "ADR-0146" in invariant,
    }
    failures = [f"structural check failed: {k}" for k,v in checks.items() if not v]
    return checks, failures


def main() -> int:
    scenarios, sf = scenario_rows()
    mutations, mf = mutation_rows()
    structural, stf = structural_checks()
    failures = sf + mf + stf
    result = {
        "checker": "sealed_delivery_adr0146_final_closure",
        "gates": ["V-003", "V-015"],
        "scope": "ADR-0146 sibling terminalization, terminal-state race, bounded reply transport/candidates and wrapper authority-lifetime binding",
        "passed": not failures,
        "summary": {
            "scenarios": len(scenarios),
            "scenarios_passed": sum(1 for x in scenarios if x["ok"]),
            "mutations": len(mutations),
            "mutations_caught": sum(1 for x in mutations if x["caught"]),
        },
        "structural_checks": structural,
        "scenarios": scenarios,
        "mutations": mutations,
        "failures": failures,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
