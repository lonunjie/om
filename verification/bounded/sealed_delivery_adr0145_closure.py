#!/usr/bin/env python3
"""Bounded V-003 evidence for ADR-0145 bootstrap single-response/retry closure.

Architecture-state evidence only. It models the requester-side MessageRequest bootstrap lifecycle,
exact COMPLETE retry lookup before key access, multi-device candidate races and crash-consistent
winner/import/request closure. It is not product code or cryptographic-runtime proof.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/sealed-delivery-adr0145-closure-latest.json"


def fp(obj: object) -> str:
    return hashlib.sha256(json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


@dataclass(frozen=True)
class Wrapper:
    key_id: str
    delivery_id: str
    body: str
    route: str = "reply-route-1"
    expires_at: int = 2000

    @property
    def fingerprint(self) -> str:
        return fp(self.__dict__)


@dataclass
class Replay:
    fingerprint: str
    route: str
    expires_at: int
    state: str = "RESERVED"  # RESERVED | COMPLETE | REJECTED


@dataclass
class BootstrapWorld:
    request_state: str = "OPEN"  # OPEN | COMPLETE_REPLAY_ONLY | EXPIRED
    private_key_present: bool = True
    replay: dict[tuple[str, str], Replay] = field(default_factory=dict)
    winner: tuple[str, str, str] | None = None
    imported_established_state: bool = False
    durable_inner_effects: int = 0
    acknowledgements: int = 0
    mutation_key_lookup_before_replay: bool = False
    mutation_allow_second_after_complete: bool = False
    mutation_split_completion_commit: bool = False

    def reserve_candidate(self, w: Wrapper) -> str:
        rk = (w.key_id, w.delivery_id)
        old = self.replay.get(rk)
        if old:
            if old.fingerprint != w.fingerprint:
                return "collision"
            if old.state == "RESERVED":
                if not self.private_key_present:
                    return "key_missing_reserved"
                return "resume_reserved"
            if old.state == "COMPLETE":
                return self.transport_receive(w)
            return "rejected_terminal"
        if self.request_state != "OPEN" and not self.mutation_allow_second_after_complete:
            return "bootstrap_closed"
        if not self.private_key_present:
            return "key_missing"
        self.replay[rk] = Replay(w.fingerprint, w.route, w.expires_at)
        return "reserved"

    def commit_candidate(self, w: Wrapper) -> str:
        rk = (w.key_id, w.delivery_id)
        r = self.replay.get(rk)
        if r is None or r.fingerprint != w.fingerprint or r.state != "RESERVED":
            return "not_reserved"
        if self.request_state != "OPEN" and not self.mutation_allow_second_after_complete:
            r.state = "REJECTED"
            return "bootstrap_closed_loser"

        # Authenticated bootstrap response and imported established state are assumed valid here.
        # The normal path commits all of them atomically with COMPLETE + request closure.
        r.state = "COMPLETE"
        self.imported_established_state = True
        self.durable_inner_effects += 1
        if self.mutation_split_completion_commit:
            # Unsafe mutation: crash can observe imported state + COMPLETE while request is still OPEN.
            return "split_complete_request_still_open"

        self.request_state = "COMPLETE_REPLAY_ONLY"
        self.winner = (w.key_id, w.delivery_id, w.fingerprint)
        self.private_key_present = False
        return "bootstrap_complete"

    def transport_receive(self, w: Wrapper) -> str:
        rk = (w.key_id, w.delivery_id)

        # Unsafe mutation: legacy ordering tries the destroyed key before durable COMPLETE replay lookup.
        if self.mutation_key_lookup_before_replay and not self.private_key_present:
            return "key_missing_before_replay"

        r = self.replay.get(rk)
        if r is not None:
            if r.fingerprint != w.fingerprint:
                return "collision"
            if r.route != w.route:
                return "route_mismatch"
            if r.state == "COMPLETE":
                self.acknowledgements += 1
                return "complete_idempotent_ack"
            if r.state == "REJECTED":
                return "rejected_terminal"
            if not self.private_key_present:
                return "key_missing_reserved"
            return "resume_reserved"

        if self.request_state != "OPEN" and not self.mutation_allow_second_after_complete:
            return "bootstrap_closed"
        return self.reserve_candidate(w)

    def crash_recover(self) -> str:
        # All fields in this bounded model are durable. This method exposes the observable safety tuple.
        return f"{self.request_state}|imported={int(self.imported_established_state)}|key={int(self.private_key_present)}|effects={self.durable_inner_effects}"


def scenario_rows() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def record(name: str, got, expected):
        ok = got == expected
        rows.append({"id": name, "ok": ok, "got": got, "expected": expected})
        if not ok:
            failures.append(f"{name}: expected {expected!r}, got {got!r}")

    winner = Wrapper("bootstrap-k1", "delivery-a", "binding-A")
    other = Wrapper("bootstrap-k1", "delivery-b", "binding-B")

    # 1. COMPLETE retry is recognized after the private bootstrap key is destroyed.
    w = BootstrapWorld()
    got = [w.reserve_candidate(winner), w.commit_candidate(winner), w.private_key_present,
           w.transport_receive(winner), w.durable_inner_effects, w.acknowledgements]
    record("complete_lost_ack_retry_after_key_destroy", got,
           ["reserved", "bootstrap_complete", False, "complete_idempotent_ack", 1, 1])

    # 2. A different/new wrapper cannot become a second logical bootstrap after completion.
    w = BootstrapWorld()
    got = [w.reserve_candidate(winner), w.commit_candidate(winner), w.transport_receive(other)]
    record("different_wrapper_after_complete_is_closed", got,
           ["reserved", "bootstrap_complete", "bootstrap_closed"])

    # 3. Two valid recipient-device candidates may both reserve, but first atomic commit wins exactly once.
    w = BootstrapWorld()
    got = [w.reserve_candidate(winner), w.reserve_candidate(other), w.commit_candidate(other),
           w.commit_candidate(winner), w.durable_inner_effects, w.winner]
    record("multi_device_race_exactly_one_winner", got,
           ["reserved", "reserved", "bootstrap_complete", "bootstrap_closed_loser", 1,
            (other.key_id, other.delivery_id, other.fingerprint)])

    # 4. Crash recovery sees either a fully open pre-commit state or a complete replay-only post-commit state.
    w = BootstrapWorld()
    pre = [w.reserve_candidate(winner), w.crash_recover()]
    post_commit = w.commit_candidate(winner)
    post = w.crash_recover()
    record("bootstrap_completion_atomic_crash_state", [pre, post_commit, post],
           [["reserved", "OPEN|imported=0|key=1|effects=0"], "bootstrap_complete",
            "COMPLETE_REPLAY_ONLY|imported=1|key=0|effects=1"])

    # 5. RESERVED exact retry still requires and retains the private key until completion/expiry.
    w = BootstrapWorld()
    got = [w.reserve_candidate(winner), w.private_key_present, w.transport_receive(winner)]
    record("reserved_retry_retains_key", got, ["reserved", True, "resume_reserved"])

    return rows, failures


def mutation_rows() -> tuple[list[dict], list[str]]:
    rows: list[dict] = []
    failures: list[str] = []

    def caught(name: str, got, bad_expected):
        ok = got == bad_expected
        rows.append({"id": name, "caught": ok, "mutated_result": got})
        if not ok:
            failures.append(f"mutation {name} was not observable: {got!r}")

    winner = Wrapper("bootstrap-k1", "delivery-a", "binding-A")
    other = Wrapper("bootstrap-k1", "delivery-b", "binding-B")

    w = BootstrapWorld(mutation_key_lookup_before_replay=True)
    w.reserve_candidate(winner); w.commit_candidate(winner)
    caught("M1_key_lookup_before_complete_replay", w.transport_receive(winner), "key_missing_before_replay")

    w = BootstrapWorld(mutation_allow_second_after_complete=True)
    w.reserve_candidate(winner); w.commit_candidate(winner)
    # The mutation leaves the private key destroyed; make the unsafe second-branch authorization visible by
    # simulating a buggy retained/reloaded key. The security defect is that request closure no longer blocks it.
    w.private_key_present = True
    bad = [w.reserve_candidate(other), w.commit_candidate(other), w.durable_inner_effects]
    caught("M2_second_logical_bootstrap_allowed", bad, ["reserved", "bootstrap_complete", 2])

    w = BootstrapWorld(mutation_split_completion_commit=True)
    bad1 = [w.reserve_candidate(winner), w.commit_candidate(winner), w.crash_recover()]
    # After crash the unsafe OPEN state still permits a second valid candidate.
    bad2 = [w.reserve_candidate(other), w.commit_candidate(other), w.durable_inner_effects]
    caught("M3_completion_request_close_split", [bad1, bad2],
           [["reserved", "split_complete_request_still_open", "OPEN|imported=1|key=1|effects=1"],
            ["reserved", "split_complete_request_still_open", 2]])

    return rows, failures


def structural_checks() -> tuple[dict, list[str]]:
    failures: list[str] = []
    adr = (ROOT / "decisions/ADR-0145-bootstrap-single-response-and-complete-retry-closure.md").read_text()
    profile = (ROOT / "docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md").read_text()
    discovery = (ROOT / "docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md").read_text()
    invariant = (ROOT / "docs/architecture/safety/SECURITY_INVARIANTS.md").read_text()
    checks = {
        "adr_one_logical_response": "only one valid logical bootstrap response" in adr,
        "adr_complete_replay_before_key": "first consults existing replay state" in adr,
        "profile_replay_before_key": "consult durable replay state" in profile and "before requiring live key material" in profile,
        "discovery_atomic_winner": "crash-consistent commit wins" in discovery,
        "m7_adr0145": "ADR-0142/0144/0145" in invariant,
    }
    for name, ok in checks.items():
        if not ok:
            failures.append(f"structural check failed: {name}")
    return checks, failures


def main() -> int:
    scenarios, sf = scenario_rows()
    mutations, mf = mutation_rows()
    structural, stf = structural_checks()
    failures = sf + mf + stf
    result = {
        "checker": "sealed_delivery_adr0145_closure",
        "gate": "V-003",
        "scope": "ADR-0145 exactly-one MessageRequest bootstrap response, COMPLETE replay-before-key lookup, race and crash-atomic closure; bounded architecture evidence only",
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
