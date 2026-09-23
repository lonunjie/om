#!/usr/bin/env python3
"""V-016 (W0 half): IntroGate / block / Sybil resource model (INVITES_INTROGATE_AND_SPAM.md §5-§10,
DECENTRALIZED_ABUSE_MODERATION.md §4).

Discrete-time simulation of one recipient's IntroGate under Sybil floods. The attacker has unlimited identities
(identity is cheap), a bounded number of network sources and a compute budget. Controls modeled: cheap validation
and replay rejection before any expensive work; per-source rate limit; per-recipient pending quota; PoW escalation
when the queue is under pressure; invite-bound tickets (a personal invite has its own reserved quota); local block;
referral-only mode.

Invariants (checked in every run):
  I1 expensive work (opening a sealed request) per tick <= per-recipient quota, independent of Sybil count;
  I2 no request reaches expensive work without passing cheap validation (replay/expired/forged tickets fail closed);
  I3 a blocked identity never reaches the recipient again (a new Sybil identity is a new subject);
  I4 (structural, not simulated) the model has no transition that promotes a MessageRequest without an explicit
     accept; conformance case PC-G02 and the V-003 session model cover promotion;
  I5 invite-bound first contacts are never starved by public-queue floods (reserved capacity).
Metrics: legitimate first-contact success for public (no invite) and invite-bound contacts, per attack level.

Written by a Claude agent (not the design package's author). Evidence only.
"""
from __future__ import annotations

import json
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/introgate-sybil-latest.json"


def run(seed: int, attack_rate: int, sources: int, pow_on: bool, referral_only: bool, ticks: int = 200) -> dict:
    rng = random.Random(seed)
    QUOTA, INVITE_RESERVE, PER_SOURCE = 5, 2, 2        # per tick
    POW_THRESHOLD, ATTACKER_POW_BUDGET = 3, 6           # queue pressure; attacker solves this many PoWs per tick
    blocked, used_tickets, expensive_log, violations = set(), set(), [], []
    legit = {"public": [0, 0], "invite": [0, 0]}        # [attempts, delivered]
    next_id = 0
    for t in range(ticks):
        arrivals = []
        # legitimate traffic: ~1 public and ~0.5 invite-bound first contacts per tick
        if rng.random() < 0.9:
            arrivals.append({"kind": "public", "legit": True, "src": f"L{rng.randrange(10_000)}", "valid": True})
        if rng.random() < 0.5:
            arrivals.append({"kind": "invite", "legit": True, "src": f"L{rng.randrange(10_000)}", "valid": True,
                             "ticket": f"inv{t}-{rng.random()}"})
        # attacker: fresh Sybil ids each time, a few replays/forgeries, limited sources
        for _ in range(attack_rate):
            next_id += 1
            a = {"kind": "public", "legit": False, "src": f"A{rng.randrange(sources)}", "id": f"sybil{next_id}",
                 "valid": rng.random() > 0.2}
            if rng.random() < 0.05 and used_tickets:
                a.update(kind="invite", ticket=rng.choice(sorted(used_tickets)), valid=True, replay=True)
            if rng.random() < 0.02 and blocked:
                a["id"] = rng.choice(sorted(blocked))     # blocked identity tries again
            arrivals.append(a)
        rng.shuffle(arrivals)
        # stage 1: cheap validation (I2)
        per_src, admitted = {}, []
        pressure = attack_rate > POW_THRESHOLD
        pow_budget = ATTACKER_POW_BUDGET
        for r in arrivals:
            if r.get("legit"):
                legit[r["kind"]][0] += 1
            if not r["valid"]:
                continue
            if r["kind"] == "invite":
                if r["ticket"] in used_tickets:
                    continue                               # replay fails closed
            elif referral_only and not r.get("legit_referral", r.get("legit") and rng.random() < 0.3):
                continue                                   # referral-only: only referred strangers pass
            if r.get("id") in blocked:
                continue                                   # I3
            per_src[r["src"]] = per_src.get(r["src"], 0) + 1
            if per_src[r["src"]] > PER_SOURCE:
                continue
            if pow_on and pressure and r["kind"] == "public":
                if not r.get("legit"):
                    if pow_budget <= 0:
                        continue
                    pow_budget -= 1                        # attacker compute is bounded; legit clients solve one PoW
            admitted.append(r)
        # stage 2: expensive work within quota; invite-bound tickets have reserved capacity (I5)
        invites = [r for r in admitted if r["kind"] == "invite"][:INVITE_RESERVE]
        public = [r for r in admitted if r["kind"] == "public"][:QUOTA - len(invites)]
        opened = invites + public
        expensive_log.append(len(opened))
        if len(opened) > QUOTA:
            violations.append(f"I1 t={t}")
        for r in opened:
            if r["kind"] == "invite":
                used_tickets.add(r["ticket"])
            if r.get("replay"):
                violations.append(f"I2 replay opened t={t}")
            if r.get("id") in blocked:
                violations.append(f"I3 blocked id opened t={t}")
            if r.get("legit"):
                legit[r["kind"]][1] += 1
            else:
                blocked.add(r["id"])                       # recipient blocks the spam it sees
    inv_rate = legit["invite"][1] / max(1, legit["invite"][0])
    if inv_rate < 0.95 and not referral_only:
        violations.append(f"I5 invite-bound success {inv_rate:.2f}")
    return {"attack_rate": attack_rate, "sources": sources, "pow": pow_on, "referral_only": referral_only, "seed": seed,
            "max_expensive_per_tick": max(expensive_log),
            "public_success": round(legit["public"][1] / max(1, legit["public"][0]), 3),
            "invite_success": round(inv_rate, 3), "violations": violations}


def main() -> int:
    runs = []
    for attack in (0, 5, 50, 500):
        for sources in (1, 20, 1000):
            for pow_on in (False, True):
                for ref in (False, True):
                    for seed in range(3):
                        runs.append(run(seed, attack, sources, pow_on, ref))
    violations = [v for r in runs for v in r["violations"]]

    def avg(key, **flt):
        rs = [r for r in runs if all(r[k] == v for k, v in flt.items())]
        return round(sum(r[key] for r in rs) / len(rs), 3)
    table = [{"attack_rate": a, "sources": s, "pow": p,
              "public_success": avg("public_success", attack_rate=a, sources=s, pow=p, referral_only=False),
              "invite_success": avg("invite_success", attack_rate=a, sources=s, pow=p, referral_only=False)}
             for a in (0, 5, 50, 500) for s in (1, 20, 1000) for p in (False, True)]
    payload = {"model": "introgate_sybil_resource", "gates": ["V-016"], "runs": len(runs),
               "max_expensive_per_tick": max(r["max_expensive_per_tick"] for r in runs), "quota": 5,
               "violations": violations[:20], "passed": not violations, "summary_table": table,
               "finding": "Expensive work stays bounded by the per-recipient quota at every Sybil count, and invite-bound "
                          "first contacts keep their reserved capacity. Public (no-invite) first contact is starved by "
                          "floods from many sources; PoW helps only while the attacker's compute is bounded. Public "
                          "reachability under attack is therefore best-effort by design, and invites or referrals are "
                          "the dependable path.",
               "gate_status_effect": "evidence_only_no_gate_automatically_passed"}
    OUT.write_text(json.dumps(payload, indent=1) + "\n")
    print(json.dumps({k: payload[k] for k in ("runs", "max_expensive_per_tick", "passed")}))
    for row in table:
        if row["pow"] or row["attack_rate"] in (0, 500):
            print(row)
    return 0 if not violations else 1


if __name__ == "__main__":
    raise SystemExit(main())
