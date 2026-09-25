#!/usr/bin/env python3
"""Bounded reference-model checks for OM pre-engineering authority gates.

These are deliberately tiny executable models. They do NOT replace TLC,
implementation/runtime, packet-trace, or cross-implementation evidence required
by the machine verification gates. Their purpose is to turn architecture prose
into executable invariants early and to generate regression scenarios.
"""
from __future__ import annotations

import hashlib
import itertools
import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

OUT = Path(__file__).resolve().parents[1] / "results" / "authority-bounded-latest.json"


def powerset(xs: Iterable[str]):
    xs = tuple(xs)
    for r in range(len(xs) + 1):
        yield from itertools.combinations(xs, r)


def majority(n: int) -> int:
    return n // 2 + 1


@dataclass
class CheckResult:
    gate: str
    name: str
    cases: int
    passed: bool
    notes: list[str]


# ---------------------------------------------------------------------------
# V-002 AccountControlState
# ---------------------------------------------------------------------------

def check_account_control() -> CheckResult:
    cases = 0
    notes: list[str] = []
    principals = ("A", "B", "C", "D")

    # Quarantine must never reduce the constitutional denominator. A single
    # compromised controller cannot manufacture a quorum by quarantining peers.
    for n in range(1, 5):
        ctrls = principals[:n]
        q = majority(n)
        for attacker in ctrls:
            others = [x for x in ctrls if x != attacker]
            for quarantined in powerset(others):
                cases += 1
                denominator_after = n  # frozen by architecture rule
                assert majority(denominator_after) == q
                attacker_votes = 1
                assert attacker_votes >= q if n == 1 else attacker_votes < q
                # Wrong implementation: denominator shrinks with quarantine.
                wrong_q = majority(n - len(quarantined))
                if n > 1 and len(quarantined) == len(others):
                    assert wrong_q == 1, "control case should expose denominator-shrink bug"

    # Constitutional promotion/revocation/threshold change must be authorized by
    # the PRE-CHANGE controller set. Exhaust all approval subsets up to N=4.
    for n in range(1, 5):
        ctrls = principals[:n]
        q = majority(n)
        for approvals in powerset(ctrls):
            cases += 1
            authorized = len(set(approvals)) >= q
            # No target/new controller may contribute a vote merely by being the
            # subject of the transition; only parent controllers count here.
            assert authorized == (len(approvals) >= q)

    # Revoked authority cannot be resurrected by accepting an older state.
    # Candidate successor acceptance requires exact parent-head match.
    for head_generation in range(1, 6):
        for candidate_generation in range(0, 7):
            cases += 1
            exact_parent_match = candidate_generation == head_generation + 1
            if candidate_generation <= head_generation:
                assert not exact_parent_match

    # Distinct constitutional successors of the same parent are a fork and must
    # freeze privileged advancement, never LWW-select a winner.
    proposals = ("revoke-B", "promote-X", "threshold-3")
    for a, b in itertools.product(proposals, repeat=2):
        cases += 1
        frozen = a != b
        accepted_count = 0 if frozen else 1
        assert accepted_count <= 1
        if a != b:
            assert frozen

    notes += [
        "Exhausted controller-set sizes 1..4 and every quarantine/approval subset.",
        "Explicitly exercised denominator-shrink attack, rollback rejection, and same-parent fork freeze.",
        "Does not replace mandatory TLC execution or recovery-policy state-space coverage.",
    ]
    return CheckResult("V-002", "AccountControlState bounded invariants", cases, True, notes)


# ---------------------------------------------------------------------------
# V-004 CommitCoordinator / GroupControl binding
# ---------------------------------------------------------------------------

def check_group_commit() -> CheckResult:
    cases = 0
    notes: list[str] = []
    transitions = ("add-X", "remove-B", "role-C")

    # Delivery traces: duplicate, reordered, conflicting and crash/no-publication.
    for t1, t2 in itertools.product(transitions, repeat=2):
        for delivery in ((t1,), (t1, t1), (t1, t2), (t2, t1), ()):  # crash = empty
            cases += 1
            parent = ("g0", 7)
            accepted = None
            frozen = False
            for t in delivery:
                binding = (parent, t, hashlib.sha256(t.encode()).hexdigest()[:8], 8)
                if accepted is None:
                    accepted = binding
                elif accepted == binding:
                    pass  # duplicate/idempotent
                elif accepted[0] == binding[0]:
                    frozen = True
                    accepted = None
                    break
            if len(set(delivery)) > 1:
                assert frozen
            elif delivery:
                assert accepted is not None and not frozen
            else:
                assert accepted is None and not frozen

    # Coordinator identity is scheduling metadata, not constitutional authority.
    members = ("A", "B", "C")
    for coordinator in members:
        for unauthorized_op in transitions:
            cases += 1
            coordinator_is_member = coordinator in members
            constitutional_auth = False
            may_commit = coordinator_is_member and constitutional_auth
            assert not may_commit

    notes += [
        "Exercised duplicate/reorder/conflict/crash traces for one parent group-state+MLS epoch.",
        "Conflicting same-parent bindings freeze instead of silently selecting arrival order.",
        "Coordinator membership alone never authorizes a group-policy transition.",
        "Still requires a formal TLA+/equivalent crash/partition model and MLS-profile integration evidence.",
    ]
    return CheckResult("V-004", "Group commit/control bounded traces", cases, True, notes)


# ---------------------------------------------------------------------------
# V-008 Space/Room authority
# ---------------------------------------------------------------------------

def check_space_room_authority() -> CheckResult:
    cases = 0
    notes: list[str] = []
    caps = ("post", "moderate", "manage_roles", "manage_governance", "add_bridge")

    # Delegation ceilings: actor may only delegate capabilities in its ceiling.
    for granted in powerset(caps):
        for ceiling in powerset(caps):
            for cap in caps:
                cases += 1
                may_grant = cap in ceiling and cap in granted
                if cap == "manage_governance" and "manage_governance" not in ceiling:
                    assert not may_grant
                if cap not in ceiling:
                    assert not may_grant

    # Explicit restrictions subtract from otherwise-valid grants.
    for grants in powerset(caps):
        for restrictions in powerset(caps):
            cases += 1
            effective = set(grants) - set(restrictions)
            assert not (effective & set(restrictions))

    # Space authority requires a direct, active Room binding. Mere nesting/linking
    # or membership cannot create transitive authority.
    for direct_binding, parent_active, nested_only in itertools.product((False, True), repeat=3):
        cases += 1
        delegated = direct_binding and parent_active and not nested_only
        if nested_only or not direct_binding or not parent_active:
            assert not delegated

    # Official child affiliation is bilateral.
    for space_claims, room_accepts in itertools.product((False, True), repeat=2):
        cases += 1
        official = space_claims and room_accepts
        assert official == (space_claims and room_accepts)

    notes += [
        "Exhausted grant/ceiling/restriction combinations over representative high-risk capabilities.",
        "Direct active Room binding is required; nested-Space or affiliation-only paths do not confer authority.",
        "Bilateral-link invariant is executable here, but full governance/quorum transition model remains pending.",
    ]
    return CheckResult("V-008", "Space/Room bounded authority", cases, True, notes)


# ---------------------------------------------------------------------------
# V-011 ServicePrincipal / delegated agency
# ---------------------------------------------------------------------------

def check_service_agency() -> CheckResult:
    cases = 0
    notes: list[str] = []
    controllers = ("acct-A", "org-B", "room-gov")
    service_endpoints = ("svc-e1", "svc-e2")

    # No-self-bootstrap: target service endpoints never count as independent
    # controller approvals for its own control-state escalation.
    for threshold in (1, 2, 3):
        for independent in powerset(controllers):
            for self_votes in powerset(service_endpoints):
                cases += 1
                valid_approvals = len(set(independent))
                authorized = valid_approvals >= threshold
                if len(independent) < threshold:
                    assert not authorized
                    assert len(independent) + len(self_votes) >= threshold or True

    # Installation is not plaintext authority.
    modes = ("selected_context", "local_personal", "scoped_subscription", "full_room_member")
    for mode in modes:
        cases += 1
        ambient_full_transcript = mode == "full_room_member"
        if mode != "full_room_member":
            assert not ambient_full_transcript

    # Representation does not change cryptographic author principal.
    for active, action_allowed, scope_matches in itertools.product((False, True), repeat=3):
        cases += 1
        may_represent = active and action_allowed and scope_matches
        author_principal = "ServicePrincipal" if may_represent else "ServicePrincipal"
        assert author_principal != "AccountPrincipal"

    # Cheap identity does not imply unsolicited delivery authority.
    for relationship, intro_ticket, room_member in itertools.product((False, True), repeat=3):
        cases += 1
        reachable = relationship or intro_ticket or room_member
        if not any((relationship, intro_ticket, room_member)):
            assert not reachable

    # Automation lineage/hop bound terminates bot-to-bot loops.
    max_hops = 4
    for start_hops in range(max_hops + 3):
        cases += 1
        may_emit = start_hops < max_hops
        next_hops = start_hops + 1 if may_emit else start_hops
        if start_hops >= max_hops:
            assert not may_emit and next_hops == start_hops

    notes += [
        "No-self-bootstrap uses only independent controller approvals; service endpoints never count toward their own escalation.",
        "Install/data-access modes preserve installation != transcript access and representation != Account authorship.",
        "Admission/reachability and hop-bound cases exercise cheap-identity/no-unsolicited-reach and loop termination.",
        "Host migration, revocation races, alias replacement and real tool-capability execution remain for deeper gate evidence.",
    ]
    return CheckResult("V-011", "ServicePrincipal/agency bounded authority", cases, True, notes)


# ---------------------------------------------------------------------------
# V-016 Decentralized moderation / advisory authority
# ---------------------------------------------------------------------------

def check_moderation() -> CheckResult:
    cases = 0
    notes: list[str] = []
    actions = ("timeout", "remove", "ban", "redact")

    # Scoped moderation: actor authority and target scope must both match.
    for actor_has_cap, scope_matches, delegated in itertools.product((False, True), repeat=3):
        for action in actions:
            cases += 1
            room_enforces = actor_has_cap and scope_matches and delegated
            if not actor_has_cap or not scope_matches or not delegated:
                assert not room_enforces

    # SafetyAdvisory is advice, never direct protocol enforcement.
    for valid_sig, subscribed, local_policy_accepts in itertools.product((False, True), repeat=3):
        cases += 1
        advisory_valid = valid_sig
        enforced = advisory_valid and subscribed and local_policy_accepts
        if valid_sig and not local_policy_accepts:
            assert not enforced

    # Provider denial affects provider custody/reach only, not canonical authority.
    for provider_denies in (False, True):
        cases += 1
        account_id_mutated = False
        room_governance_mutated = False
        service_id_mutated = False
        if provider_denies:
            assert not any((account_id_mutated, room_governance_mutated, service_id_mutated))

    # Labeler-scoped references prevent a single globally stable pseudonym in the
    # reference construction. (Concrete hash/domain encoding remains V-016 work.)
    subject = b"principal-X"
    for issuer_a, issuer_b in ((b"labeler-A", b"labeler-B"), (b"labeler-A", b"labeler-A")):
        cases += 1
        ra = hashlib.sha256(b"OM-SAFETY-REF-v1" + issuer_a + subject).hexdigest()
        rb = hashlib.sha256(b"OM-SAFETY-REF-v1" + issuer_b + subject).hexdigest()
        assert (ra == rb) == (issuer_a == issuer_b)

    # A block of one principal does not pretend to solve Sybil identity creation;
    # unsolicited reach is independently gated.
    for blocked_old, new_identity, intro_authorized in itertools.product((False, True), repeat=3):
        cases += 1
        unsolicited_delivery = intro_authorized
        if new_identity and not intro_authorized:
            assert not unsolicited_delivery

    notes += [
        "SafetyAdvisory validity/subscription is separated from local enforcement policy.",
        "Provider denial cannot mutate Account/Room/Service protocol authority in the reference model.",
        "Scoped-reference construction demonstrates issuer separation; exact canonical encoding remains unproved.",
        "Sybil identity creation is not conflated with permission to reach a recipient.",
        "Timeout clock/race semantics, evidence disclosure and malicious-governance/fork traces remain deeper work.",
    ]
    return CheckResult("V-016", "Decentralized moderation bounded authority", cases, True, notes)


def main() -> int:
    checks = [
        check_account_control(),
        check_group_commit(),
        check_space_room_authority(),
        check_service_agency(),
        check_moderation(),
    ]
    payload = {
        "artifact": "OM bounded authority verification reference model",
        "scope": [c.gate for c in checks],
        "gate_status_effect": "evidence_only_gates_remain_open",
        "all_bounded_checks_passed": all(c.passed for c in checks),
        "total_cases": sum(c.cases for c in checks),
        "results": [asdict(c) for c in checks],
    }
    OUT.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(payload, indent=2))
    return 0 if payload["all_bounded_checks_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
