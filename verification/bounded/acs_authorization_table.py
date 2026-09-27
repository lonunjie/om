#!/usr/bin/env python3
"""V-002: executable mirror of the AccountControlState authorization table.

ACCOUNT_CONTINUITY_AND_CONTROL.md §4.1 (normative, ADR-0127) says a verifier derives each delta by diffing child and
parent, counts signers against the parent, requires the strongest row among combined deltas, and rejects any
unlisted delta. This file:

1. parses the §4.1 table from the document and fails if a row is added, removed or reworded without the model
   knowing it (the mirror is mechanical, not a hand-copied list);
2. implements `authorize(parent, child, ctx)`, the verifier rule for every row, including the recovery vote
   (R-200: immediate only with >= 1 non-quarantined controller co-signing, otherwise after the delay with no veto),
   the R-201 successor pointer and the R-205 fork bridge (om.acs.fork_ack, device-only majority);
3. exhaustively enumerates bounded parents (1..3 controllers, quarantine subsets, one messaging device), every
   delta class and target, every signer subset and every recovery context, and checks safety invariants;
4. writes one or more positive and negative vectors per transition class to
   verification/vectors/acs-authorization-table-vectors.json and a summary to
   verification/results/acs-authorization-table-latest.json.

Written by a Claude agent (verification executor). Deterministic; no randomness.
"""
from __future__ import annotations

import itertools
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOC = ROOT / "docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md"
POLICY_FILE = ROOT / "spec/protocol/account-control-transition-policy.json"
VEC = ROOT / "verification/vectors/acs-authorization-table-vectors.json"
RES = ROOT / "verification/results/acs-authorization-table-latest.json"

# ADR-0137: the JSON policy is the machine-readable source of truth. This executable
# model derives its table-row map and authorization rule classes from that source
# instead of carrying a second hand-maintained authorization table.
POLICY = json.loads(POLICY_FILE.read_text(encoding="utf-8"))
AUTH_RULE = {
    "one_active_nonquarantined": "one",
    "one_active_nonquarantined_other_than_target": "one",
    "parent_majority_or_recovery_vote": "majority_rv",
    "parent_majority_active_nonquarantined": "majority",
    "parent_majority": "majority",
    "self_active_nonquarantined": "self",
    "device_only_parent_majority_common_ancestor": "bridge",
    "forbidden": "forbidden",
}
ROWS = {row["doc_prefix"]: row["id"] for row in POLICY["transitions"]}
RULE = {row["id"]: AUTH_RULE[row["authorization"]] for row in POLICY["transitions"]}
STRENGTH = {"self": 0, "one": 1, "majority_rv": 2, "majority": 3, "bridge": 4, "forbidden": 5}


def parse_table() -> list[tuple[str, str, str]]:
    text = DOC.read_text(encoding="utf-8")
    sec = text.split("### 4.1 Authorization table", 1)[1].split("\n## ", 1)[0]
    rows = []
    for line in sec.splitlines():
        if line.startswith("|") and not line.startswith("|---") and not line.startswith("| Delta"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            rows.append((cells[0], cells[1], cells[2]))
    return rows


def classify_row(delta: str) -> str | None:
    hits = [cls for pre, cls in ROWS.items() if delta.startswith(pre)]
    return hits[0] if len(hits) == 1 else None


def mirror_check() -> dict:
    rows = parse_table()
    classes = [classify_row(r[0]) for r in rows]
    problems = []
    if None in classes:
        problems.append(f"unmapped table rows: {[r[0] for r, c in zip(rows, classes) if c is None]}")
    if sorted(c for c in classes if c) != sorted(ROWS.values()):
        problems.append(f"table/model class sets differ: table={sorted(c for c in classes if c)} model={sorted(ROWS.values())}")
    single = {c: r[2] for r, c in zip(rows, classes) if c}
    for c, s in single.items():
        want = {"one": "yes", "self": "yes", "forbidden": "—"}.get(RULE[c], "only when N = 1")
        if s != want:
            problems.append(f"{c}: table says single signer '{s}', model rule {RULE[c]} implies '{want}'")
    signers = {c: r[1] for r, c in zip(rows, classes) if c}
    for c, text in signers.items():
        r = RULE[c]
        ok = {"one": "1 active, non-quarantined controller" in text,
              "majority_rv": text.startswith("majority, or majority reached with a recovery vote"),
              "majority": text.startswith("majority") and ("recovery vote" not in text or "no recovery vote" in text),
              "self": "that device" in text,
              "bridge": "device-only majority" in text and "no recovery vote" in text,
              "forbidden": text.startswith("forbidden")}[r]
        if not ok:
            problems.append(f"{c}: signer text '{text}' does not match model rule {r}")
    return {"policy": str(POLICY_FILE.relative_to(ROOT)), "rows": len(rows), "classes": sorted(c for c in classes if c), "problems": problems}


# ------------------------------------------------------------------ states and deltas
def state(control, quarantined=(), messaging=(), revoked=(), policy="p0", constitution=1, min_gen=1, keys=None,
          transparency="t0", gen=1, retired=None, fork_ack=None, parent="h0"):
    return {"control": sorted(control), "quarantined": sorted(quarantined), "messaging": sorted(messaging),
            "revoked": sorted(revoked), "recovery_policy": policy, "constitutional_policy": constitution,
            "min_security_generation": min_gen, "key_epochs": keys or {d: 1 for d in control},
            "transparency": transparency, "generation": gen, "retirement": retired, "fork_ack": fork_ack,
            "previous": parent}


def majority(n: int) -> int:
    return n // 2 + 1


def deltas(p: dict, c: dict) -> list[tuple[str, str | None]]:
    """Derive the delta classes of child c against parent p (§4.1 first paragraph)."""
    out: list[tuple[str, str | None]] = []
    P = {k: set(p[k]) for k in ("control", "quarantined", "messaging", "revoked")}
    C = {k: set(c[k]) for k in ("control", "quarantined", "messaging", "revoked")}
    for d in C["messaging"] - P["messaging"]:
        out.append(("link_messaging", d))
    for d in C["control"] - P["control"]:
        out.append(("promote", d))
    for d in C["quarantined"] - P["quarantined"]:
        out.append(("quarantine", d))
    for d in P["quarantined"] - C["quarantined"]:
        out.append(("lift_quarantine" if d in C["control"] else "revoke_control", d))
    for d in P["messaging"] - C["messaging"]:
        out.append(("remove_messaging" if d in C["revoked"] else "unlisted", d))
    for d in P["control"] - C["control"]:
        out.append(("revoke_control" if d in C["revoked"] else "unlisted", d))
    extra_revoked = C["revoked"] - P["revoked"] - (P["control"] - C["control"]) - (P["messaging"] - C["messaging"])
    out += [("unlisted", d) for d in extra_revoked]
    if P["revoked"] - C["revoked"]:
        out.append(("unlisted", "un-revoke"))
    if c["recovery_policy"] != p["recovery_policy"]:
        out.append(("recovery_policy", None))
    if c["constitutional_policy"] != p["constitutional_policy"]:
        out.append(("constitutional_policy", None))
    if c["min_security_generation"] > p["min_security_generation"]:
        out.append(("raise_min_profile", None))
    if c["min_security_generation"] < p["min_security_generation"]:
        out.append(("lower_min_profile", None))
    for d in set(p["key_epochs"]) & set(c["key_epochs"]):
        if c["key_epochs"][d] != p["key_epochs"][d]:
            out.append(("rotate_own_key", d))
    if c["transparency"] != p["transparency"]:
        out.append(("transparency", None))
    if c.get("retirement"):
        out.append(("retirement", None))
    if c.get("fork_ack") is not None:
        out.append(("fork_bridge", None))
    return out


def authorize(p: dict, c: dict, ctx: dict) -> tuple[str, str]:
    v, why = _authorize(p, c, ctx)
    if v != "reject":
        broken = after_invariants(c)
        if broken:  # §13 rule 1 applies to every row, not only to quarantine
            return "reject", f"§13: child would have {broken[0]}"
        voting_after = len(set(c["control"]) - set(c["quarantined"]))
        if voting_after < majority(len(c["control"])) and any(k == "quarantine" for k, _ in deltas(p, c)):
            return v, why + "; child enters SAFETY_FREEZE (§4 rule 4)"
    return v, why


def _authorize(p: dict, c: dict, ctx: dict) -> tuple[str, str]:
    """Return (verdict, reason); verdict in {accept, pending, reject}. `ctx`:
    signers: devices that signed as controllers; recovery: None or {"elapsed": int, "delay": int, "veto": bool};
    trusted_heads: heads this verifier already trusts (fork bridge); frozen: parent under safety freeze."""
    if p.get("retirement"):
        return "reject", "parent retired: AccountRetirement is terminal"
    if c["generation"] != p["generation"] + 1:
        return "reject", "generation must advance by exactly one"
    ds = deltas(p, c)
    if not ds:
        return "reject", "no delta"
    if any(k == "unlisted" for k, _ in ds):
        return "reject", "delta not listed in §4.1"
    rule = max((RULE[k] for k, _ in ds), key=STRENGTH.get)
    if rule == "forbidden":
        return "reject", "lowering minimum_crypto_profile is forbidden"
    control, quarantined = set(p["control"]), set(p["quarantined"])
    voting = control - quarantined
    signers = set(ctx.get("signers", ()))
    maj = majority(len(control))
    safety_frozen = bool(quarantined) and len(voting) < maj
    if safety_frozen:
        # §5 / ADR-0137: SAFETY_FREEZE is derived from the parent state, never from
        # caller-supplied provenance. Selective restoration is the sole exception
        # that can count quarantined challenge signatures.
        if rule == "bridge":
            return "reject", "a quarantine-induced safety freeze is not a fork freeze; fork bridge is a separate transition"
        if any(k != "lift_quarantine" for k, _ in ds):
            return "reject", "parent is under quarantine-induced SAFETY_FREEZE; only threshold-restoring quarantine lifts may resolve it"
        if ctx.get("recovery"):
            return "reject", "a recovery vote never resolves a freeze"
        if signers - control:
            return "reject", "a freeze-resolution signer is not a controller of the parent"
        if len(signers) < maj:
            return "reject", "freeze resolution needs a device-only parent majority"
        restored = set(p["quarantined"]) - set(c["quarantined"])
        if not restored:
            return "reject", "freeze resolution must restore at least one quarantined controller"
        if not restored <= signers:
            return "reject", "every restored controller must contribute its freeze-challenge signature"
        if set(c["quarantined"]) - set(p["quarantined"]):
            return "reject", "freeze resolution may not add new quarantines"
        if len(control - set(c["quarantined"])) < maj:
            return "reject", "selective restoration must exit SAFETY_FREEZE by restoring a voting majority"
        return "accept", "device-only parent majority resolves the freeze; selected challenge-signed controllers restored"
    if ctx.get("frozen") and rule != "bridge":
        return "reject", "a non-quarantine freeze is resolved only by the fork-bridge path"
    if signers - voting:
        return "reject", "a signer is not an active, non-quarantined controller of the parent"
    n, maj = len(control), majority(len(control))
    rec = ctx.get("recovery")
    # structural sanity of specific deltas
    for k, d in ds:
        if k == "quarantine" and (d not in control or d in signers):
            return "reject", "quarantine target must be another control device of the parent"
        if k == "rotate_own_key" and ({d} != signers or d in quarantined):
            return "reject", "only the device itself, not quarantined, may rotate its own key entry"
        if k == "promote" and d in set(p["revoked"]):
            return "reject", "a revoked device id is never reused"
    if rule == "bridge":
        ack = set(c["fork_ack"] or [])
        if len(ack) < 2:
            return "reject", "fork bridge must list every observed branch head (>= 2)"
        if not set(ctx.get("trusted_heads", ())) <= ack:
            return "reject", "fork_ack does not cover a head this verifier trusts"
        if rec:
            return "reject", "a recovery vote never counts for a fork bridge"
        return ("accept", "device-only majority of the common ancestor") if len(signers) >= maj else ("reject", "fork bridge needs a device-only majority")
    if c.get("retirement", {}) and c["retirement"].get("reason") == "moved" and not signers:
        return "reject", "R-201: a successor pointer needs device signers under the parent state (guardians alone cannot move an account)"
    if rule in ("one", "self"):
        if rec:
            return "reject", "a recovery vote is not a controller signature"
        return ("accept", "single active controller") if len(signers) >= 1 else ("reject", "no controller signature")
    if rule == "majority":
        if rec:
            return "reject", "this delta takes no recovery vote"
        return ("accept", "majority") if len(signers) >= maj else ("reject", f"majority {maj} of N={n} required")
    # majority_rv
    if not rec:
        return ("accept", "majority") if len(signers) >= maj else ("reject", f"majority {maj} of N={n} required")
    if rec.get("veto"):
        return "reject", "RecoveryVeto observed"
    if signers:
        return ("accept", "majority reached with a co-signed recovery vote") if len(signers) + 1 >= maj else ("reject", "recovery vote + co-signers below majority")
    if rec["elapsed"] < rec["delay"]:
        return "pending", "recovery alone: waiting for this verifier's own activation interval (R-278)"
    return "accept", "recovery alone after the full delay with no veto"


def after_invariants(c: dict) -> list[str]:
    bad = []
    if not (set(c["control"]) - set(c["quarantined"])):
        bad.append("zero voting controllers")
    if set(c["control"]) & set(c["revoked"]):
        bad.append("revoked device is a controller")
    return bad


# ------------------------------------------------------------------ bounded exhaustive enumeration
DEVS = ["d1", "d2", "d3"]
SPARE = "d4"
MSG = "m1"


def children(p: dict):
    """One child per (class, target) applied to parent p."""
    g = p["generation"] + 1

    def ch(**kw):
        base = dict(control=p["control"], quarantined=p["quarantined"], messaging=p["messaging"], revoked=p["revoked"],
                    policy=p["recovery_policy"], constitution=p["constitutional_policy"], min_gen=p["min_security_generation"],
                    keys=dict(p["key_epochs"]), transparency=p["transparency"], gen=g)
        base.update(kw)
        return state(**base)

    yield "link_messaging", None, ch(messaging=set(p["messaging"]) | {"m2"})
    if p["messaging"]:
        yield "remove_messaging", MSG, ch(messaging=set(p["messaging"]) - {MSG}, revoked=set(p["revoked"]) | {MSG})
    yield "promote", SPARE, ch(control=set(p["control"]) | {SPARE}, keys={**p["key_epochs"], SPARE: 1})
    for t in p["control"]:
        if t not in p["quarantined"]:
            yield "quarantine", t, ch(quarantined=set(p["quarantined"]) | {t})
        else:
            yield "lift_quarantine", t, ch(quarantined=set(p["quarantined"]) - {t})
        if len(p["control"]) > 1:
            k = dict(p["key_epochs"])
            k.pop(t)
            yield "revoke_control", t, ch(control=set(p["control"]) - {t}, quarantined=set(p["quarantined"]) - {t},
                                          revoked=set(p["revoked"]) | {t}, keys=k)
        yield "rotate_own_key", t, ch(keys={**p["key_epochs"], t: p["key_epochs"][t] + 1})
    yield "recovery_policy", None, ch(policy="p1")
    yield "constitutional_policy", None, ch(constitution=2)
    yield "raise_min_profile", None, ch(min_gen=p["min_security_generation"] + 1)
    yield "lower_min_profile", None, ch(min_gen=p["min_security_generation"] - 1)
    yield "transparency", None, ch(transparency="t1")
    r = ch()
    r["retirement"] = {"reason": "moved", "successor": "acct-2"}
    yield "retirement", "moved", r
    r2 = ch()
    r2["retirement"] = {"reason": "deleted"}
    yield "retirement", "deleted", r2
    b = ch()
    b["fork_ack"] = ["hA", "hB"]
    yield "fork_bridge", None, b


def parents():
    for n in (1, 2, 3):
        ctl = DEVS[:n]
        for qn in range(0, n):
            for q in itertools.combinations(ctl, qn):
                yield state(ctl, quarantined=q, messaging=[MSG], min_gen=2)


def contexts(p: dict):
    devs = p["control"] + [MSG]
    for k in range(0, len(devs) + 1):
        for s in itertools.combinations(devs, k):
            yield {"signers": list(s)}
            for elapsed, veto in ((0, False), (2, False), (2, True)):
                yield {"signers": list(s), "recovery": {"elapsed": elapsed, "delay": 2, "veto": veto}}
            yield {"signers": list(s), "trusted_heads": ["hA"]}
            yield {"signers": list(s), "trusted_heads": ["hC"]}


def explore() -> dict:
    counts = {"cases": 0, "accept": 0, "pending": 0, "reject": 0}
    per_class: dict[str, dict[str, int]] = {}
    violations: list[str] = []
    for p in parents():
        voting = set(p["control"]) - set(p["quarantined"])
        n, maj = len(p["control"]), majority(len(p["control"]))
        for cls, tgt, c in children(p):
            for ctx in contexts(p):
                v, why = authorize(p, c, ctx)
                counts["cases"] += 1
                counts[v] += 1
                per_class.setdefault(cls, {"accept": 0, "pending": 0, "reject": 0})[v] += 1
                s = set(ctx["signers"])
                rec = ctx.get("recovery")
                tag = f"{cls}/{tgt} N={n} q={p['quarantined']} ctx={ctx}"
                if v == "accept":
                    safety_resolution = len(voting) < maj and cls == "lift_quarantine"
                    if s - voting and not safety_resolution:
                        violations.append(f"accepted with a non-voting signer: {tag}")
                    if RULE[cls] in ("majority", "bridge") and len(s) < maj:
                        violations.append(f"majority-only delta accepted below majority: {tag}")
                    if RULE[cls] == "majority_rv" and len(s) + (1 if rec else 0) < maj and not (rec and not s):
                        violations.append(f"majority_rv delta accepted below majority: {tag}")
                    if RULE[cls] not in ("one", "self") and len(s) == 1 and not rec and n > 1:
                        violations.append(f"single signer accepted for a constitutional delta with N>1: {tag}")
                    if rec and not s and (rec["veto"] or rec["elapsed"] < rec["delay"]):
                        violations.append(f"recovery-alone accepted before delay or over a veto: {tag}")
                    if cls == "lower_min_profile":
                        violations.append(f"minimum profile lowered: {tag}")
                    if cls == "retirement" and tgt == "moved" and not s:
                        violations.append(f"R-201 successor pointer accepted without device signers: {tag}")
                    if cls == "fork_bridge" and (rec or not set(ctx.get("trusted_heads", [])) <= set(c["fork_ack"])):
                        violations.append(f"fork bridge accepted with a recovery vote or uncovered trusted head: {tag}")
                    if MSG in s:
                        violations.append(f"messaging device counted as controller: {tag}")
                    for b in after_invariants(c):
                        violations.append(f"accepted child breaks {b}: {tag}")
                if v == "pending" and not (rec and not s and not rec["veto"] and rec["elapsed"] < rec["delay"]):
                    violations.append(f"pending outside the recovery-alone interval: {tag}")
    return {"counts": counts, "per_class": per_class, "violations": violations}


# ------------------------------------------------------------------ vectors
def vectors() -> list[dict]:
    P3 = state(DEVS, messaging=[MSG], min_gen=2)
    P3q = state(DEVS, quarantined=["d3"], messaging=[MSG], min_gen=2)
    P1 = state(["d1"], messaging=[MSG], min_gen=2)
    kids3 = {(k, t): c for k, t, c in children(P3)}
    kids3q = {(k, t): c for k, t, c in children(P3q)}
    kids1 = {(k, t): c for k, t, c in children(P1)}
    rec = lambda e, v=False: {"elapsed": e, "delay": 2, "veto": v}
    V = []

    def add(name, cls, p, c, ctx, expect):
        got, why = authorize(p, c, ctx)
        assert got == expect, (name, got, why)
        V.append({"name": name, "class": cls, "parent": p, "child": c, "context": ctx, "expect": expect, "reason": why})

    add("link_messaging_by_one_controller", "link_messaging", P3, kids3[("link_messaging", None)], {"signers": ["d1"]}, "accept")
    add("link_messaging_by_messaging_device_rejected", "link_messaging", P3, kids3[("link_messaging", None)], {"signers": ["m1"]}, "reject")
    add("promote_majority", "promote", P3, kids3[("promote", SPARE)], {"signers": ["d1", "d2"]}, "accept")
    add("promote_single_signer_N3_rejected", "promote", P3, kids3[("promote", SPARE)], {"signers": ["d1"]}, "reject")
    add("promote_single_signer_N1", "promote", P1, kids1[("promote", SPARE)], {"signers": ["d1"]}, "accept")
    add("promote_one_controller_plus_recovery_vote", "promote", P3, kids3[("promote", SPARE)], {"signers": ["d1"], "recovery": rec(0)}, "accept")
    add("quarantine_by_other_controller", "quarantine", P3, kids3[("quarantine", "d2")], {"signers": ["d1"]}, "accept")
    add("quarantine_self_signed_rejected", "quarantine", P3, kids3[("quarantine", "d2")], {"signers": ["d2"]}, "reject")
    add("quarantine_by_quarantined_controller_rejected", "quarantine", P3q, kids3q[("quarantine", "d1")], {"signers": ["d3"]}, "reject")
    add("lift_quarantine_majority", "lift_quarantine", P3q, kids3q[("lift_quarantine", "d3")], {"signers": ["d1", "d2"]}, "accept")
    add("lift_quarantine_single_rejected", "lift_quarantine", P3q, kids3q[("lift_quarantine", "d3")], {"signers": ["d1"]}, "reject")
    add("lift_quarantine_with_recovery_vote_rejected", "lift_quarantine", P3q, kids3q[("lift_quarantine", "d3")], {"signers": ["d1"], "recovery": rec(2)}, "reject")
    add("lift_quarantine_counting_quarantined_signer_rejected", "lift_quarantine", P3q, kids3q[("lift_quarantine", "d3")], {"signers": ["d1", "d3"]}, "reject")
    P2q1 = state(["d1", "d2"], quarantined=["d2"], messaging=[MSG], min_gen=2)
    k2q1 = {(k, t): c for k, t, c in children(P2q1)}
    add("frozen_lift_with_challenge_signature_resolves", "lift_quarantine", P2q1, k2q1[("lift_quarantine", "d2")], {"signers": ["d1", "d2"], "frozen": True}, "accept")
    add("frozen_lift_without_challenge_signature_rejected", "lift_quarantine", P2q1, k2q1[("lift_quarantine", "d2")], {"signers": ["d1"], "frozen": True}, "reject")
    add("frozen_lift_with_recovery_vote_rejected", "lift_quarantine", P2q1, k2q1[("lift_quarantine", "d2")], {"signers": ["d1"], "recovery": rec(2), "frozen": True}, "reject")
    add("frozen_other_delta_rejected", "revoke_control", P2q1, k2q1[("revoke_control", "d2")], {"signers": ["d1", "d2"], "frozen": True}, "reject")
    # ADR-0137: selective restoration is allowed only when the child actually exits
    # SAFETY_FREEZE. A known-bad controller may remain quarantined.
    P3q2 = state(["d1", "d2", "d3"], quarantined=["d2", "d3"], messaging=[MSG], min_gen=2)
    C3q1 = state(["d1", "d2", "d3"], quarantined=["d3"], messaging=[MSG], min_gen=2, gen=2)
    add("frozen_selective_restore_to_majority", "lift_quarantine", P3q2, C3q1, {"signers": ["d1", "d2"], "frozen": True}, "accept")
    add("frozen_restore_without_restored_controller_challenge_rejected", "lift_quarantine", P3q2, C3q1, {"signers": ["d1", "d3"], "frozen": True}, "reject")
    P4q3 = state(["d1", "d2", "d3", "d4"], quarantined=["d2", "d3", "d4"], messaging=[MSG], min_gen=2)
    C4q2 = state(["d1", "d2", "d3", "d4"], quarantined=["d3", "d4"], messaging=[MSG], min_gen=2, gen=2)
    add("frozen_selective_restore_below_majority_rejected", "lift_quarantine", P4q3, C4q2, {"signers": ["d1", "d2", "d3"], "frozen": True}, "reject")
    add("remove_messaging_by_one_controller", "remove_messaging", P3, kids3[("remove_messaging", MSG)], {"signers": ["d2"]}, "accept")
    add("remove_messaging_unsigned_rejected", "remove_messaging", P3, kids3[("remove_messaging", MSG)], {"signers": []}, "reject")
    add("revoke_control_majority", "revoke_control", P3, kids3[("revoke_control", "d3")], {"signers": ["d1", "d2"]}, "accept")
    add("revoke_control_recovery_alone_pending", "revoke_control", P3, kids3[("revoke_control", "d3")], {"signers": [], "recovery": rec(0)}, "pending")
    add("revoke_control_recovery_alone_after_delay", "revoke_control", P3, kids3[("revoke_control", "d3")], {"signers": [], "recovery": rec(2)}, "accept")
    add("revoke_control_recovery_alone_vetoed", "revoke_control", P3, kids3[("revoke_control", "d3")], {"signers": [], "recovery": rec(2, True)}, "reject")
    add("revoke_control_single_signer_rejected", "revoke_control", P3, kids3[("revoke_control", "d3")], {"signers": ["d1"]}, "reject")
    add("recovery_policy_majority", "recovery_policy", P3, kids3[("recovery_policy", None)], {"signers": ["d1", "d3"]}, "accept")
    add("recovery_policy_single_rejected", "recovery_policy", P3, kids3[("recovery_policy", None)], {"signers": ["d1"]}, "reject")
    add("constitutional_policy_majority", "constitutional_policy", P3, kids3[("constitutional_policy", None)], {"signers": ["d1", "d2"]}, "accept")
    add("constitutional_policy_recovery_vote_rejected", "constitutional_policy", P3, kids3[("constitutional_policy", None)], {"signers": ["d1"], "recovery": rec(2)}, "reject")
    add("raise_min_profile_one_controller", "raise_min_profile", P3, kids3[("raise_min_profile", None)], {"signers": ["d2"]}, "accept")
    add("raise_min_profile_unsigned_rejected", "raise_min_profile", P3, kids3[("raise_min_profile", None)], {"signers": []}, "reject")
    add("lower_min_profile_forbidden_even_unanimous", "lower_min_profile", P3, kids3[("lower_min_profile", None)], {"signers": ["d1", "d2", "d3"]}, "reject")
    add("rotate_own_key_self", "rotate_own_key", P3, kids3[("rotate_own_key", "d2")], {"signers": ["d2"]}, "accept")
    add("rotate_other_device_key_rejected", "rotate_own_key", P3, kids3[("rotate_own_key", "d2")], {"signers": ["d1"]}, "reject")
    add("transparency_one_controller", "transparency", P3, kids3[("transparency", None)], {"signers": ["d3"]}, "accept")
    add("transparency_unsigned_rejected", "transparency", P3, kids3[("transparency", None)], {"signers": []}, "reject")
    add("fork_bridge_device_only_majority_covering_trusted_head", "fork_bridge", P3, kids3[("fork_bridge", None)], {"signers": ["d1", "d2"], "trusted_heads": ["hA"]}, "accept")
    add("fork_bridge_not_covering_trusted_head_rejected", "fork_bridge", P3, kids3[("fork_bridge", None)], {"signers": ["d1", "d2"], "trusted_heads": ["hC"]}, "reject")
    add("fork_bridge_with_recovery_vote_rejected", "fork_bridge", P3, kids3[("fork_bridge", None)], {"signers": ["d1"], "recovery": rec(2), "trusted_heads": ["hA"]}, "reject")
    add("fork_bridge_single_signer_rejected", "fork_bridge", P3, kids3[("fork_bridge", None)], {"signers": ["d1"], "trusted_heads": ["hA"]}, "reject")
    nob = dict(kids3[("fork_bridge", None)], fork_ack=["hA"])
    add("fork_bridge_listing_one_head_rejected", "fork_bridge", P3, nob, {"signers": ["d1", "d2"], "trusted_heads": ["hA"]}, "reject")
    add("retirement_moved_majority", "retirement", P3, kids3[("retirement", "moved")], {"signers": ["d1", "d2"]}, "accept")
    add("retirement_moved_single_device_rejected", "retirement", P3, kids3[("retirement", "moved")], {"signers": ["d1"]}, "reject")
    add("retirement_moved_by_recovery_alone_rejected", "retirement", P3, kids3[("retirement", "moved")], {"signers": [], "recovery": rec(2)}, "reject")
    add("retirement_moved_cosigned_recovery_vote", "retirement", P3, kids3[("retirement", "moved")], {"signers": ["d1"], "recovery": rec(0)}, "accept")
    add("retirement_deleted_recovery_alone_pending", "retirement", P3, kids3[("retirement", "deleted")], {"signers": [], "recovery": rec(1)}, "pending")
    add("retirement_deleted_recovery_alone_after_delay", "retirement", P3, kids3[("retirement", "deleted")], {"signers": [], "recovery": rec(2)}, "accept")
    add("retirement_deleted_recovery_alone_vetoed", "retirement", P3, kids3[("retirement", "deleted")], {"signers": [], "recovery": rec(2, True)}, "reject")
    retired = dict(P3, retirement={"reason": "deleted"})
    add("any_transition_after_retirement_rejected", "retirement", retired, dict(kids3[("transparency", None)]), {"signers": ["d1", "d2", "d3"]}, "reject")
    # Combined deltas need the strongest row; an unlisted delta is rejected.
    combo = dict(kids3[("promote", SPARE)], messaging=[MSG, "m2"])
    add("combined_link_and_promote_needs_majority", "promote", P3, combo, {"signers": ["d1"]}, "reject")
    unl = dict(kids3[("transparency", None)], revoked=["zz"])
    add("unlisted_delta_rejected", "unlisted", P3, unl, {"signers": ["d1", "d2", "d3"]}, "reject")
    P2q = state(["d1", "d2"], quarantined=["d1"], messaging=[MSG], min_gen=2)
    k2q = {(k, t): c for k, t, c in children(P2q)}
    add("revoke_last_voting_controller_rejected_s13", "revoke_control", P2q, k2q[("revoke_control", "d2")], {"signers": ["d2"], "recovery": rec(0)}, "reject")
    add("quarantine_last_voting_controller_rejected_s13", "quarantine", P2q, k2q[("quarantine", "d2")], {"signers": ["d2"]}, "reject")
    reuse = state(DEVS, messaging=[MSG], revoked=[SPARE], min_gen=2)
    add("revoked_device_id_never_repromoted", "promote", reuse, next(c for k, t, c in children(reuse) if k == "promote"), {"signers": ["d1", "d2", "d3"]}, "reject")
    return V


def main() -> int:
    mirror = mirror_check()
    exp = explore()
    vec = vectors()
    covered = sorted({v["class"] for v in vec} & set(ROWS.values()))
    missing = sorted(set(ROWS.values()) - set(covered))
    pos = {c: sum(1 for v in vec if v["class"] == c and v["expect"] != "reject") for c in ROWS.values()}
    neg = {c: sum(1 for v in vec if v["class"] == c and v["expect"] == "reject") for c in ROWS.values()}
    neg_only_ok = {"lower_min_profile"}
    lacking = sorted(c for c in ROWS.values() if neg[c] == 0 or (pos[c] == 0 and c not in neg_only_ok))
    VEC.write_text(json.dumps({
        "gate": "V-002", "source": "verification/bounded/acs_authorization_table.py",
        "table": "ACCOUNT_CONTINUITY_AND_CONTROL.md §4.1 (ADR-0127)",
        "semantics": "authorize(parent, child, context) -> accept | pending | reject; signers are controller signatures counted against the parent; recovery = RecoveryPolicy vote with this verifier's own elapsed observation (R-278)",
        "vectors": vec}, indent=1) + "\n")
    ok = not mirror["problems"] and not exp["violations"] and not missing and not lacking
    RES.write_text(json.dumps({
        "gate": "V-002", "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        "all_ok": ok, "table_mirror": mirror, "exhaustive": {"counts": exp["counts"], "per_class": exp["per_class"],
        "violations": exp["violations"][:20], "violation_count": len(exp["violations"])},
        "vectors": {"total": len(vec), "classes_covered": covered, "missing_classes": missing,
                    "positive_per_class": pos, "negative_per_class": neg, "classes_lacking_pos_or_neg": lacking}},
        indent=2, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, "rows": mirror["rows"], "cases": exp["counts"]["cases"], "violations": len(exp["violations"]),
                      "vectors": len(vec), "problems": mirror["problems"], "lacking": lacking}))
    for v in exp["violations"][:10]:
        print("VIOLATION", v, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
