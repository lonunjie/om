#!/usr/bin/env python3
"""Feasibility prototype for V-003. Not product code. Not for production.

Dedicated restore + redelivery scenario (V-003 criterion "restore+redelivery is covered by a dedicated executable
scenario or an explicitly equivalent bounded trace"). It uses the reference machine (sessions.py) with ADR-0131
restore semantics (`World.restore_as`: the restored installation is a fresh DeviceID enrolled by the account's control
chain; the old DeviceID is revoked there) and explores EVERY interleaving, within bounds, of:

  a1 sends up to 3 messages; b1 takes a backup; b1 is lost and restored as b1r (control step + restore_as); a1 learns
  the new control state early, late or never before sending; the relay duplicates an envelope (redelivery), keeps
  delivering to the lost b1, or hands b1's queue to the restored installation; a1 may crash once.

Checked in every reachable state: S1 NoDoubleAccept, S2 NoKeyReuse, S3 NoSendToIneligible, S4 NoAcceptFromKnownRevoked,
S5 NoSilentDowngrade, RestoredIgnoresOldMailbox; at quiescence C1 DeliveredAtQuiescence with NO restore exemption
(the old same-DeviceID `restore` exempted every intended delivery, which is why this criterion was open), plus:
R1 the restored device's per-peer floor is >= the backed-up floor and >= the ACS minimum (ADR-0120 K3);
R2 history in the backup is never accepted a second time by the restored device.
It also measures the documented residual: messages whose only intended recipient was the lost device after its
backup point and before the sender learned the restore are not delivered to the restored device (history gap).

Usage: restore_redelivery.py [out.json]
"""
from __future__ import annotations

import copy
import json
import sys
from pathlib import Path

from sessions import ACS_MIN_PROFILE, Bounds, Infeasible, Topology, World

HERE = Path(__file__).resolve().parent
TOPO = Topology({"A": [frozenset({"a1"})], "B": [frozenset({"b1"}), frozenset({"b1r"})]}, frozenset({"a1"}))
BOUNDS = Bounds(max_msgs=3, max_sid=6, max_crash=1, max_restore=1, max_dup=1, max_weak=0, max_inject=0)


def fingerprint(w: World) -> str:
    return repr((sorted(w.gen.items()), [(d, w.dev[d].snap()) for d in sorted(w.dev)], sorted(w.P.items(), key=repr),
                 sorted(w.Bk.items(), key=repr), sorted(w.net, key=repr), w.next_sid, w.msg_count, w.crashes, w.dups,
                 sorted(w.dead), sorted(w.restored_from.items()), sorted((d, sorted(a.items())) for d, a in w.acc.items())))


def actions(w: World) -> list[tuple]:
    out = []
    for a in w.enabled():
        if a[0] in ("restore", "inject", "serve_weak"):
            continue
        if a[0] == "take_backup" and a[1] != "b1":
            continue
        if a[0] in ("persist", "crash") and a[1] != "a1":
            continue
        out.append(a)
    if "b1r" not in w.restored_from:
        out.append(("restore_as", "b1", "b1r"))
    for e in sorted(w.net, key=repr):
        if e[0] == "msg" and e[2] in w.dead:
            out.append(("misdeliver_old_mailbox", e))
    return out


def extra_checks(w: World, stats: dict) -> list[str]:
    bad = []
    if "b1r" in w.restored_from:
        bk = w.Bk["b1"]
        for p, f in bk["floor"].items():
            if w.dev["b1r"].floor.get(p, 0) < f or w.eff_floor("b1r", p) < max(f, ACS_MIN_PROFILE):
                bad.append("R1_FloorSurvivesRestore")
        if any(c > 1 for c in w.acc["b1r"].values()):
            bad.append("R2_BackupHistoryNotReaccepted")
        if w.quiescent():
            gap = {m for (m, r) in w.intended if r == "b1" and m not in w.dev["b1r"].seen}
            stats["max_history_gap"] = max(stats["max_history_gap"], len(gap))
            if any(r == "b1r" for (_, r) in w.intended):
                stats["quiescent_states_with_delivery_to_restored"] += 1
    return bad


def explore(rules: dict | None = None) -> dict:
    root = World(TOPO, BOUNDS, {})
    root.k.update(rules or {})
    stats = {"states": 0, "quiescent": 0, "restored_states": 0, "misdeliveries": 0, "dup_after_restore": 0,
             "quiescent_states_with_delivery_to_restored": 0, "max_history_gap": 0}
    seen: set[str] = set()
    stack = [root]
    first_bad = None
    while stack:
        w = stack.pop()
        fp = fingerprint(w)
        if fp in seen:
            continue
        seen.add(fp)
        stats["states"] += 1
        stats["quiescent"] += w.quiescent()
        stats["restored_states"] += "b1r" in w.restored_from
        bad = w.check() + extra_checks(w, stats)
        if bad and first_bad is None:
            first_bad = {"violated": bad, "trace": w.log}
            break
        for act in actions(w):
            nw = copy.deepcopy(w)
            try:
                label = nw.apply(act)
            except Infeasible:
                continue
            if label[0] == "RecvNotAddressedToMe":
                stats["misdeliveries"] += 1
            if act[0] == "recv" and act[2] and "b1r" in w.restored_from:
                stats["dup_after_restore"] += 1
            stack.append(nw)
    return {"stats": stats, "violation": first_bad}


def scripted() -> dict:
    """One readable golden trace: backup, loss, restore, late learning, redelivery of the old queue, duplicate delivery
    to the restored device, and delivery of the post-restore message exactly once."""
    w = World(TOPO, BOUNDS, {})
    steps = [("new_message", "a1")]
    w.apply(steps[0])
    for a in [("encrypt", "a1", 1, "b1")]:
        w.apply(a)
    w.apply(("recv", next(iter(w.net)), False))
    w.apply(("take_backup", "b1"))
    w.apply(("new_message", "a1"))
    w.apply(("encrypt", "a1", 2, "b1"))                 # in flight to b1 when b1 is lost
    w.apply(("control_step", "B"))                       # b1 revoked, b1r enrolled
    w.apply(("restore_as", "b1", "b1r"))
    w.apply(("misdeliver_old_mailbox", next(e for e in w.net if e[2] == "b1")))
    w.apply(("learn", "a1", "B"))
    w.apply(("new_message", "a1"))
    w.apply(("encrypt", "a1", 3, "b1r"))
    e = next(e for e in w.net if e[2] == "b1r")
    w.apply(("recv", e, True))                           # redelivered duplicate
    w.apply(("recv", e, False))
    return {"trace": w.log, "violations": w.check() + extra_checks(w, {"max_history_gap": 0, "quiescent_states_with_delivery_to_restored": 0}),
            "b1r_seen": sorted(w.dev["b1r"].seen), "accepts_b1r": w.acc["b1r"], "quiescent": w.quiescent()}


def main() -> int:
    ex = explore()
    mut = explore({"RestoreImportMerges": False})   # mutation: the overwrite import must be caught
    sc = scripted()
    ok = ex["violation"] is None and not sc["violations"] and sc["accepts_b1r"] == {1: 1, 3: 1} and sc["quiescent"] \
        and ex["stats"]["misdeliveries"] > 0 and ex["stats"]["dup_after_restore"] > 0 \
        and ex["stats"]["quiescent_states_with_delivery_to_restored"] > 0 and mut["violation"] is not None
    out = {"ok": ok, "semantics": "ADR-0131 rule 2 restore = fresh DeviceID; no delivery exemption", "exhaustive": ex,
           "mutation_overwrite_import": {"violated": mut["violation"] and mut["violation"]["violated"],
                                         "trace": mut["violation"] and mut["violation"]["trace"]},
           "scripted_golden_trace": sc,
           "residual": {"history_gap_max_messages": ex["stats"]["max_history_gap"],
                        "meaning": "messages intended only for the lost device after its backup point and before the sender learned the "
                                   "restore are not re-encrypted to the restored device; they reach it only through history sync from "
                                   "another device of the account (MULTI_DEVICE_SYNC.md) or not at all"}}
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "work/restore-redelivery.json"
    path.parent.mkdir(exist_ok=True)
    path.write_text(json.dumps(out, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"ok": ok, **ex["stats"], "violation": ex["violation"] and ex["violation"]["violated"]}))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
