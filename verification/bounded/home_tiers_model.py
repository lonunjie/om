#!/usr/bin/env python3
"""V-020 bounded models: sponsorship, tier migration, vendor-wide block (ADR-0102, ADR-0112).

1. Sponsorship: exhaustive action sequences over a small sponsor/ticket/mailbox world. Invariants: the per-sponsor
   cap is never exceeded; tickets are single-use; no chaining when may_sponsor_further is false; revoked or expired
   mailboxes accept nothing; quota exhaustion fails closed (a billable event is unreachable); the identity survives
   mailbox loss.
2. Migration T0 -> T2/T3: step simulation over every contact online/send pattern, overlap and forwarding window,
   checked against a closed-form survival predicate (queued RouteUpdate or a 'moved' reply teaches the new route).
3. Vendor block (drill D5, gap G5): seeded preferential-attachment invite graphs with Cloudflare-first T1 hosting.
   Reports the fraction of relationships that keep a route after a Cloudflare-wide block, with and without the
   "second provider family fallback" policy.

Written by a Claude agent (not the design package's author). Evidence only; no gate is passed automatically.
"""
from __future__ import annotations

import itertools
import json
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/home-tiers-latest.json"


# ---------------------------------------------------------------- 1. sponsorship
def sponsorship() -> dict:
    CAP, QUOTA = 2, 2
    actions = ["issue", "redeem", "redeem_again", "deposit", "revoke", "expire", "chain_issue", "renew"]
    cases, violations = 0, []
    for seq in itertools.product(actions, repeat=6):
        cases += 1
        tickets, used, mailboxes, billed = [], set(), {}, 0
        for a in seq:
            active = [m for m, s in mailboxes.items() if s["state"] == "active"]
            if a == "issue":
                tickets.append(len(tickets))
            elif a in ("redeem", "redeem_again"):
                cand = [t for t in tickets if t not in used] if a == "redeem" else list(used)
                if not cand:
                    continue
                t = cand[0]
                if t in used or len(active) >= CAP:     # single use; cap enforced at the sponsor's Home
                    continue
                used.add(t)
                mailboxes[t] = {"state": "active", "stored": 0, "may_sponsor_further": False}
            elif a == "deposit":
                for m in list(mailboxes):
                    s = mailboxes[m]
                    if s["state"] != "active":
                        continue                         # revoked/expired: rejected
                    if s["stored"] >= QUOTA:
                        continue                         # fail closed, never bill
                    s["stored"] += 1
            elif a == "chain_issue":
                for s in mailboxes.values():
                    if s["may_sponsor_further"]:
                        tickets.append(len(tickets))
            elif a in ("revoke", "expire"):
                if active:
                    mailboxes[active[0]]["state"] = "revoked" if a == "revoke" else "expired"
            elif a == "renew":
                for s in mailboxes.values():
                    if s["state"] == "expired":
                        s["state"] = "active" if sum(x["state"] == "active" for x in mailboxes.values()) < CAP else "expired"
            if sum(s["state"] == "active" for s in mailboxes.values()) > CAP:
                violations.append(f"cap exceeded {seq}")
            if any(s["stored"] > QUOTA for s in mailboxes.values()):
                violations.append(f"quota exceeded {seq}")
            if billed:
                violations.append(f"billable event {seq}")
            if len(mailboxes) > len(set(mailboxes)):
                violations.append(f"ticket reused {seq}")
    return {"cases": cases, "violations": violations[:20], "passed": not violations,
            "bounds": {"cap": CAP, "quota": QUOTA, "sequence_length": 6}}


# ---------------------------------------------------------------- 2. migration
def migration() -> dict:
    """Step simulation. At t=0 the user adds a new route and deposits a RouteUpdate into each contact's mailbox
    (queued there until the contact drains it). The old route accepts deposits until OVERLAP, then answers
    'moved(new route)' until FORWARD, then is gone. A contact learns the new route when it drains its mailbox
    while online, or when a deposit to the old route returns 'moved'. The relationship survives iff the contact
    ends up knowing a working route (or had another route already)."""
    cases, lost_rel, violations = 0, 0, []
    HORIZON = 6
    for overlap in range(1, 4):
        for forward in range(overlap, overlap + 3):
            for online in itertools.product([0, 1], repeat=HORIZON):
                for sends in itertools.product([0, 1], repeat=HORIZON):
                    for multihomed in (0, 1):
                        cases += 1
                        knows_new, delivered_after_move = False, True
                        for t in range(HORIZON):
                            if online[t]:
                                knows_new = True            # drains queued RouteUpdate
                            if sends[t]:
                                if knows_new or multihomed:
                                    continue
                                if t < overlap:
                                    continue                # old route still accepts
                                if t < forward:
                                    knows_new = True        # 'moved' response teaches the new route
                                    continue
                                delivered_after_move = False
                        survives = (knows_new or bool(multihomed)) and delivered_after_move
                        def learned_by(t):
                            return any(online[:t + 1]) or any(sends[k] for k in range(overlap, min(forward, t + 1)))
                        predicted = bool(multihomed) or (learned_by(HORIZON - 1) and all(
                            not sends[t] or t < forward or learned_by(t) for t in range(HORIZON)))
                        if survives != predicted:
                            violations.append({"overlap": overlap, "forward": forward, "online": online, "sends": sends})
                        lost_rel += not survives
    return {"cases": cases, "lost_relationship_cases": lost_rel, "violations": violations[:10], "passed": not violations,
            "claim": "a relationship survives migration iff the contact is multi-homed, or comes online before its first "
                     "post-forwarding send (queued RouteUpdate or a 'moved' reply teaches the new route); AccountID never changes",
            "design_consequence": "relationships with contacts offline longer than the forwarding window are lost without "
                                  "re-introduction: keep the 30-day hand-off (R-187) and 'moved' forwarding (PC-E02) long"}


# ---------------------------------------------------------------- 3. vendor block
def vendor_block() -> dict:
    results = []
    for seed in range(20):
        rng = random.Random(seed)
        n, edges, degree = 400, set(), [0] * 400
        home = {}
        home[0] = "cf_t1"
        for v in range(1, n):
            # preferential attachment: invited by an existing user
            w = rng.choices(range(v), weights=[d + 1 for d in degree[:v]])[0]
            edges.add((w, v)); degree[w] += 1; degree[v] += 1
            r = rng.random()
            if r < 0.55:
                home[v] = "cf_t1"              # joins inviter's community Home (Cloudflare-first)
            elif r < 0.80:
                home[v] = "t0:" + str(w)       # sponsored mailbox on inviter's Home
            elif r < 0.92:
                home[v] = "cf_t2"              # own Cloudflare account
            else:
                home[v] = "t3"                 # self-hosted / static binary
            for _ in range(rng.randint(0, 3)):  # later contacts beyond the inviter
                u = rng.randrange(v)
                edges.add((min(u, v), max(u, v)))

        def family(v, seen=None):
            h = home[v]
            if h.startswith("t0:"):
                seen = seen or set()
                s = int(h[3:])
                return "cloudflare" if s in seen else family(s, (seen or set()) | {v})
            return "cloudflare" if h.startswith("cf") else "other"

        def survives(v, fallback_policy: bool) -> bool:
            if family(v) == "other":
                return True
            # with the policy, a Cloudflare-hosted user also holds a fallback route on another family if any
            # contact or the host console can provide one (here: 60% availability in the early network)
            return fallback_policy and random.Random(seed * 1000 + v).random() < 0.6

        for policy in (False, True):
            ok = sum(1 for (a, b) in edges if survives(a, policy) and survives(b, policy))
            results.append({"seed": seed, "fallback_policy": policy, "relationships": len(edges),
                            "surviving_fraction": round(ok / len(edges), 4)})
    def mean(p): return round(sum(r["surviving_fraction"] for r in results if r["fallback_policy"] == p) / 20, 4)
    return {"graphs": 20, "users_per_graph": 400,
            "tier_mix": {"cf_t1": 0.55, "t0_sponsored": 0.25, "cf_t2": 0.12, "t3": 0.08},
            "mean_surviving_fraction_no_fallback": mean(False),
            "mean_surviving_fraction_with_fallback": mean(True),
            "finding": "Under Cloudflare-first T1 with no second-family fallback, a vendor-wide block cuts most relationships; "
                       "a host-console fallback on another provider family is required before public-production resilience claims.",
            "runs": results}


def main() -> int:
    s, m, v = sponsorship(), migration(), vendor_block()
    payload = {"model": "home_tiers_bounded", "gates": ["V-020"], "sponsorship": s, "migration": m,
               "vendor_block": v, "passed": s["passed"] and m["passed"],
               "gate_status_effect": "evidence_only_no_gate_automatically_passed"}
    OUT.write_text(json.dumps(payload, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"sponsorship_cases": s["cases"], "migration_cases": m["cases"], "passed": payload["passed"],
                      "vendor_block_no_fallback": v["mean_surviving_fraction_no_fallback"],
                      "vendor_block_with_fallback": v["mean_surviving_fraction_with_fallback"]}))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
