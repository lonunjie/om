#!/usr/bin/env python3
"""Bounded V-003/V-015 evidence for ADR-0147 bootstrap lifecycle/restore closure.

Architecture-state evidence only. It derives the request deadline solely from authenticated
route/key expiries, keeps the eight-candidate OPEN budget non-replenishable across rejection/GC,
pins COMPLETE_REPLAY_ONLY expiry/GC, and makes historical bootstrap state inert on restore.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/sealed-delivery-adr0147-lifecycle-restore-latest.json"
MAX_CANDIDATES = 8
DEFAULT_SKEW = 300


def fp(obj: object) -> str:
    return hashlib.sha256(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@dataclass(frozen=True)
class Wrapper:
    key_id: str
    delivery_id: str
    body: str
    expires_at: int

    @property
    def fingerprint(self) -> str:
        return fp(self.__dict__)

    @property
    def tuple(self) -> tuple[str, str, str]:
        return (self.key_id, self.delivery_id, self.fingerprint)


@dataclass
class Replay:
    fingerprint: str
    wrapper_expires_at: int
    state: str = "RESERVED"  # RESERVED | COMPLETE | REJECTED


@dataclass
class World:
    now: int = 1000
    route_expires: int = 2000
    key_expires: int = 1900
    skew: int = DEFAULT_SKEW
    state: str = "OPEN"  # OPEN | COMPLETE_REPLAY_ONLY | CANCELLED | DECLINED | EXPIRED
    private_key_present: bool = True
    replay: dict[tuple[str, str], Replay] = field(default_factory=dict)
    admitted: set[tuple[str, str, str]] = field(default_factory=set)
    winner: tuple[str, str] | None = None
    winner_wrapper_expires: int | None = None
    inert_history: list[dict] = field(default_factory=list)
    mutation_independent_request_expires: int | None = None
    mutation_prune_replenishes_budget: bool = False
    mutation_restore_live_state: bool = False
    trace: list[dict] = field(default_factory=list)

    @property
    def bootstrap_acceptance_deadline(self) -> int:
        if self.mutation_independent_request_expires is not None:
            return min(self.route_expires, self.key_expires, self.mutation_independent_request_expires)
        return min(self.route_expires, self.key_expires)

    def _rk(self, w: Wrapper) -> tuple[str, str]:
        return (w.key_id, w.delivery_id)

    def admit(self, w: Wrapper) -> str:
        rk = self._rk(w)
        old = self.replay.get(rk)
        if old:
            if old.fingerprint != w.fingerprint:
                return "collision"
            if old.state == "COMPLETE":
                if self.now <= old.wrapper_expires_at + self.skew:
                    return "complete_idempotent_ack"
                return "winner_wrapper_expired"
            if old.state == "REJECTED":
                return "rejected_terminal"
            if self.state != "OPEN":
                return "bootstrap_closed"
            return "resume_reserved" if self.private_key_present else "key_missing_reserved"

        if self.state == "EXPIRED":
            return "bootstrap_expired"
        if self.state != "OPEN":
            return "bootstrap_closed"
        if not self.private_key_present:
            return "key_missing"
        if w.expires_at > self.key_expires or w.expires_at > self.route_expires or w.expires_at > self.bootstrap_acceptance_deadline:
            return "expiry_out_of_context"
        if self.now > w.expires_at + self.skew:
            return "wrapper_expired"

        used = len(self.replay) if self.mutation_prune_replenishes_budget else len(self.admitted)
        if used >= MAX_CANDIDATES:
            return "candidate_limit"
        self.admitted.add(w.tuple)
        self.replay[rk] = Replay(w.fingerprint, w.expires_at)
        self.trace.append({"op": "admit", "tuple": w.tuple, "used": len(self.admitted), "deadline": self.bootstrap_acceptance_deadline})
        return "reserved"

    def reject(self, w: Wrapper) -> str:
        r = self.replay.get(self._rk(w))
        if not r or r.fingerprint != w.fingerprint:
            return "not_reserved"
        r.state = "REJECTED"
        self.trace.append({"op": "reject", "tuple": w.tuple})
        return "rejected"

    def compact_rejected(self) -> int:
        doomed = [rk for rk, r in self.replay.items() if r.state == "REJECTED"]
        for rk in doomed:
            r = self.replay[rk]
            del self.replay[rk]
            if self.mutation_prune_replenishes_budget:
                self.admitted = {t for t in self.admitted if not (t[0] == rk[0] and t[1] == rk[1] and t[2] == r.fingerprint)}
        self.trace.append({"op": "compact_rejected", "removed": len(doomed), "admitted_budget_used": len(self.admitted)})
        return len(doomed)

    def complete(self, winner: Wrapper) -> str:
        if self.state != "OPEN":
            return "lost_race"
        rk = self._rk(winner)
        r = self.replay.get(rk)
        if not r or r.fingerprint != winner.fingerprint or r.state != "RESERVED":
            return "winner_not_reserved"
        r.state = "COMPLETE"
        for k, other in self.replay.items():
            if k != rk and other.state == "RESERVED":
                other.state = "REJECTED"
        self.winner = rk
        self.winner_wrapper_expires = winner.expires_at
        self.state = "COMPLETE_REPLAY_ONLY"
        self.private_key_present = False
        self.trace.append({"op": "complete", "winner": rk, "state": self.state, "private_key_present": False})
        return "closed"

    def close_without_winner(self, terminal: str) -> str:
        if self.state != "OPEN":
            return "lost_race"
        for r in self.replay.values():
            if r.state == "RESERVED":
                r.state = "REJECTED"
        self.state = terminal
        self.private_key_present = False
        self.trace.append({"op": "close", "terminal": terminal})
        return "closed"

    def advance_and_gc(self, now: int) -> None:
        self.now = now
        if self.state == "COMPLETE_REPLAY_ONLY" and self.now > self.bootstrap_acceptance_deadline + self.skew:
            self.state = "EXPIRED"
            self.replay.clear()
            self.admitted.clear()
            self.winner = None
            self.winner_wrapper_expires = None
            self.trace.append({"op": "expire_and_gc", "now": now})
        else:
            self.trace.append({"op": "advance", "now": now, "state": self.state})

    def history_backup(self) -> dict:
        return {
            "state": self.state,
            "private_key_present": self.private_key_present,
            "replay": {f"{k[0]}:{k[1]}": vars(v).copy() for k, v in self.replay.items()},
            "admitted": list(self.admitted),
            "winner": self.winner,
        }

    @classmethod
    def restore_historical(cls, snapshot: dict, *, mutation_restore_live_state: bool = False) -> "World":
        w = cls(state="EXPIRED", private_key_present=False, mutation_restore_live_state=mutation_restore_live_state)
        w.inert_history.append(snapshot)
        if mutation_restore_live_state:
            w.state = snapshot["state"]
            w.private_key_present = snapshot["private_key_present"]
            for k, v in snapshot["replay"].items():
                kid, did = k.split(":", 1)
                w.replay[(kid, did)] = Replay(**v)
            w.admitted = {tuple(x) for x in snapshot["admitted"]}
            w.winner = tuple(snapshot["winner"]) if snapshot.get("winner") else None
        w.trace.append({"op": "restore_historical", "live_state_restored": mutation_restore_live_state, "state": w.state, "private_key_present": w.private_key_present})
        return w


def scenarios() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def rec(name: str, got, expected, trace=None, resource_trace=None) -> None:
        ok = got == expected
        row = {"id": name, "ok": ok, "got": got, "expected": expected}
        if trace is not None: row["trace"] = trace
        if resource_trace is not None: row["resource_trace"] = resource_trace
        rows.append(row)
        if not ok: failures.append(f"{name}: expected {expected!r}, got {got!r}")

    w = World(route_expires=2100, key_expires=1800)
    good = Wrapper("k", "d0", "good", 1800)
    too_late = Wrapper("k", "d1", "late", 1801)
    rec("deadline_is_authenticated_min_route_and_key", [w.bootstrap_acceptance_deadline, w.admit(good), w.admit(too_late)], [1800, "reserved", "expiry_out_of_context"], w.trace,
        {"route_expires": 2100, "key_expires": 1800, "derived_deadline": 1800, "independent_request_expiry": None})

    w = World()
    first = [Wrapper("k", f"d{i}", str(i), 1700) for i in range(8)]
    got = [w.admit(x) for x in first]
    w.reject(first[0]); removed = w.compact_rejected(); ninth = w.admit(Wrapper("k", "d8", "8", 1700))
    rec("rejected_gc_does_not_replenish_lifetime_budget", [got, removed, len(w.admitted), ninth], [["reserved"]*8, 1, 8, "candidate_limit"], w.trace,
        {"max_candidates": 8, "durable_lifetime_slots_used": len(w.admitted), "live_replay_records_after_compaction": len(w.replay)})

    w = World(route_expires=2200, key_expires=2000)
    winner = Wrapper("k", "win", "W", 1500); loser = Wrapper("k", "lose", "L", 1700)
    a=w.admit(winner); b=w.admit(loser); c=w.complete(winner)
    w.now=1801  # later than winner expiry+skew (1800), before request deadline+skew (2300)
    winner_retry_mid = w.admit(winner); loser_retry_mid = w.admit(loser); mid_state=w.state
    w.advance_and_gc(2300); at_boundary=w.state
    w.advance_and_gc(2301); after=w.state; post_gc=w.admit(winner)
    rec("winner_ack_window_and_request_replay_only_gc_are_distinct",
        [a,b,c,winner_retry_mid,loser_retry_mid,mid_state,at_boundary,after,post_gc],
        ["reserved","reserved","closed","winner_wrapper_expired","rejected_terminal","COMPLETE_REPLAY_ONLY","COMPLETE_REPLAY_ONLY","EXPIRED","bootstrap_expired"], w.trace,
        {"winner_wrapper_ack_through": 1800, "request_replay_only_through": 2300, "final_records": len(w.replay)})

    w = World(); old = Wrapper("k", "old", "old", 1700); w.admit(old); snap_open = w.history_backup()
    restored = World.restore_historical(snap_open); retry = restored.admit(old)
    rec("historical_open_reserved_private_key_backup_is_inert", [restored.state, restored.private_key_present, len(restored.replay), retry], ["EXPIRED", False, 0, "bootstrap_expired"], restored.trace)

    w = World(); win=Wrapper("k", "win", "W", 1700); w.admit(win); w.complete(win); snap_complete=w.history_backup()
    restored = World.restore_historical(snap_complete); retry=restored.admit(win)
    rec("historical_replay_only_backup_is_inert", [restored.state, restored.private_key_present, len(restored.replay), retry], ["EXPIRED", False, 0, "bootstrap_expired"], restored.trace)

    return rows, failures


def mutations() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    w = World(route_expires=2000, key_expires=1900, mutation_independent_request_expires=1300)
    candidate = Wrapper("k", "d", "body", 1500)
    got = [w.bootstrap_acceptance_deadline, w.admit(candidate)]
    caught = got == [1300, "expiry_out_of_context"]  # demonstrates divergence from authenticated min=1900
    rows.append({"id": "M1_independent_request_expires_diverges_from_v1", "caught": caught, "mutated_result": got, "normative_deadline": 1900})
    if not caught: failures.append("M1 did not expose independent request_expires divergence")

    w = World(mutation_prune_replenishes_budget=True)
    xs=[Wrapper("k",f"d{i}",str(i),1700) for i in range(8)]
    for x in xs: w.admit(x)
    w.reject(xs[0]); w.compact_rejected(); ninth=w.admit(Wrapper("k","d8","8",1700))
    caught = ninth == "reserved" and len(w.admitted) == 8
    rows.append({"id": "M2_pruning_replenishes_lifetime_candidate_budget", "caught": caught, "mutated_result": ninth, "trace": w.trace})
    if not caught: failures.append("M2 did not expose candidate-budget replenishment")

    w = World(); old=Wrapper("k","old","old",1700); w.admit(old); snap=w.history_backup()
    restored=World.restore_historical(snap, mutation_restore_live_state=True); retry=restored.admit(old)
    caught = restored.private_key_present and retry == "resume_reserved"
    rows.append({"id": "M3_stale_backup_resurrects_open_reserved_key_state", "caught": caught, "mutated_result": retry, "trace": restored.trace})
    if not caught: failures.append("M3 did not expose stale-backup resurrection")

    w = World(); win=Wrapper("k","win","W",1700); w.admit(win); w.complete(win); snap=w.history_backup()
    restored=World.restore_historical(snap, mutation_restore_live_state=True); retry=restored.admit(win)
    caught = restored.state == "COMPLETE_REPLAY_ONLY" and retry == "complete_idempotent_ack"
    rows.append({"id": "M4_stale_backup_resurrects_replay_only_state", "caught": caught, "mutated_result": retry, "trace": restored.trace})
    if not caught: failures.append("M4 did not expose stale replay-only resurrection")

    return rows, failures


def structural_checks() -> tuple[dict, list[str]]:
    adr = (ROOT / "decisions/ADR-0147-bootstrap-lifetime-restore-and-durable-timer-closure.md").read_text()
    profile = (ROOT / "docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md").read_text()
    backup = (ROOT / "docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md").read_text()
    time = json.loads((ROOT / "spec/registries/om-time-fields.json").read_text())
    rules = "\n".join(time.get("rules", []))
    reqs = {json.loads(x)["id"]: json.loads(x) for x in (ROOT / "machine/requirements.jsonl").read_text().splitlines() if x.strip()}
    checks = {
        "adr_derived_deadline": "bootstrap_acceptance_deadline = min(" in adr,
        "adr_no_hidden_request_expiry": "no independent hidden `request_expires` value" in adr,
        "adr_non_replenishable_budget": "does not free its slot" in adr and "MUST NOT make a previously consumed slot available again" in adr,
        "adr_replay_only_gc_deadline": "bootstrap_acceptance_deadline` plus the registry's frozen expiry-decision skew" in adr,
        "profile_derived_deadline": "min(reply_route.expires_at, reply_delivery_key.expires_at)" in profile,
        "profile_restore_inert": "Historical backup/restore never reactivates" in profile,
        "backup_mentions_bootstrap_nonrestore": "MessageRequest" in backup and "bootstrap" in backup.lower(),
        "default_skew_300": "normatively frozen at 300 s" in rules,
        "requirements_present": all(r in reqs for r in ("R-320","R-321","R-323","R-324")),
    }
    failures = [f"structural check failed: {k}" for k,v in checks.items() if not v]
    return checks, failures


def main() -> int:
    s, sf = scenarios(); m, mf = mutations(); st, stf = structural_checks(); failures=sf+mf+stf
    result = {
        "checker": "sealed_delivery_adr0147_lifecycle_restore",
        "gates": ["V-003","V-015"],
        "requirements": ["R-320","R-321","R-323","R-324"],
        "scope": "ADR-0147 derived bootstrap deadline, durable candidate budget, replay-only expiry/GC and historical-restore exclusion",
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
