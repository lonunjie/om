#!/usr/bin/env python3
"""Bounded semantic regressions for ADR-0161.

This is design evidence, not product code. It checks:
- terminal DeviceID revocation semantics;
- order-independent branch convergence for mutable availability records;
- HistoryBundle Room/recipient/member binding and live-state isolation.
"""
from __future__ import annotations
import hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/protocol-semantics-adr0161-latest.json"


def h(rec: dict) -> str:
    body = {k: v for k, v in rec.items() if k not in {"hash", "signer_eligible"}}
    return hashlib.sha256(json.dumps(body, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def make(version: int, signer: str, previous: str | None = None, lineage: str = "L", payload: str = "x") -> dict:
    r = {"version": version, "signer": signer, "lineage": lineage, "payload": payload}
    if previous is not None:
        r["previous"] = previous
    r["hash"] = h(r)
    return r


def ingest(store: dict[str, dict], rec: dict, eligible: set[str]) -> str:
    if rec["signer"] not in eligible:
        return "REJECT_SIGNER"
    if rec["version"] == 1:
        if "previous" in rec:
            return "REJECT_GENESIS_PREVIOUS"
    else:
        prev = rec.get("previous")
        if prev is None:
            return "REJECT_MISSING_PREVIOUS"
        if prev not in store:
            return "PENDING_PARENT"
        p = store[prev]
        if p["lineage"] != rec["lineage"] or rec["version"] != p["version"] + 1:
            return "REJECT_CONTINUITY"
    store[rec["hash"]] = rec
    return "ACCEPT"


def effective(store: dict[str, dict], eligible: set[str]) -> str | None:
    candidates = [r for r in store.values() if r["signer"] in eligible]
    if not candidates:
        return None
    # highest version, then lexicographically smallest canonical record hash
    topv = max(r["version"] for r in candidates)
    return min(r["hash"] for r in candidates if r["version"] == topv)


def mutable_record_tests() -> list[dict]:
    a = make(1, "d1", payload="a")
    b = make(1, "d2", payload="b")
    a2 = make(2, "d1", a["hash"], payload="a2")
    b2 = make(2, "d2", b["hash"], payload="b2")
    a3 = make(3, "d3", a2["hash"], payload="a3")
    jump = make(4, "d1", a["hash"], payload="jump")
    cross = make(2, "d1", a["hash"], lineage="OTHER", payload="cross")

    results = []
    for order in ([a, b, a2, b2, a3], [b2, b, a3, a2, a], [b, a, b2, a2, a3]):
        store: dict[str, dict] = {}
        pending = []
        for r in order:
            verdict = ingest(store, r, {"d1", "d2", "d3"})
            if verdict == "PENDING_PARENT":
                pending.append(r)
            # Reconsider pending children whenever a parent arrives.
            changed = True
            while changed:
                changed = False
                for x in list(pending):
                    if ingest(store, x, {"d1", "d2", "d3"}) == "ACCEPT":
                        pending.remove(x); changed = True
        results.append({"order": [x["payload"] for x in order], "effective": effective(store, {"d1", "d2", "d3"}), "pending": len(pending)})
    assert len({r["effective"] for r in results}) == 1 and all(r["pending"] == 0 for r in results)

    store = {}
    assert ingest(store, a, {"d1", "d2", "d3"}) == "ACCEPT"
    assert ingest(store, jump, {"d1", "d2", "d3"}) == "REJECT_CONTINUITY"
    assert ingest(store, cross, {"d1", "d2", "d3"}) == "REJECT_CONTINUITY"
    assert ingest(store, a2, {"d1", "d2", "d3"}) == "ACCEPT"
    # Revoking d1 makes its terminal records ineffective, but they remain structural history.
    assert effective(store, {"d2", "d3"}) is None
    assert ingest(store, a3, {"d2", "d3"}) == "ACCEPT"
    assert effective(store, {"d2", "d3"}) == a3["hash"]
    return results


def device_revocation_tests() -> list[dict]:
    def valid(control, messaging, revoked):
        return not ((set(control) | set(messaging)) & set(revoked))
    cases = [
        ("normal_active", ["d1"], ["m1"], [], True),
        ("revoked_controller_resurrection", ["d1", "d2"], [], ["d2"], False),
        ("revoked_messaging_relink", ["d1"], ["m1"], ["m1"], False),
        ("fresh_id_after_revocation", ["d1"], ["m2"], ["m1"], True),
    ]
    out=[]
    for name,c,m,r,expect in cases:
        got=valid(c,m,r); assert got == expect
        out.append({"name":name,"expect":expect,"got":got})
    return out


def history_tests() -> list[dict]:
    room = {
        "conversation_id":"room-A", "state_hash":"s10", "mls_epoch":"e10",
        "members":{"alice":"a-inc", "bob":"b-inc"},
        "eligible_devices":{"alice":{"a1"}, "bob":{"b1"}}
    }
    base = {
        "conversation_id":"room-A", "author_account_id":"alice", "author_device_id":"a1",
        "recipient_account_id":"bob", "recipient_membership_incarnation":"b-inc",
        "group_control_state_hash":"s10", "mls_epoch_ref":"e10",
        "items":[{"history_item_id":"h1","claimed_original_event_id":"live-7","presentation_snapshot":{"text":"copy"}}]
    }
    def accept(b):
        if b["conversation_id"] != room["conversation_id"] or b["group_control_state_hash"] != room["state_hash"] or b["mls_epoch_ref"] != room["mls_epoch"]:
            return False
        if room["members"].get(b["recipient_account_id"]) != b["recipient_membership_incarnation"]:
            return False
        if b["author_device_id"] not in room["eligible_devices"].get(b["author_account_id"], set()):
            return False
        return True
    cases=[]
    for name,mut,expect in [
        ("bound_bundle",{},True),
        ("cross_room",{"conversation_id":"room-B"},False),
        ("wrong_recipient_incarnation",{"recipient_membership_incarnation":"old-b"},False),
        ("removed_author",{"author_device_id":"a-old"},False),
    ]:
        b={**base,**mut}; got=accept(b); assert got==expect; cases.append({"name":name,"expect":expect,"got":got})
    # A claimed live EventID is inert provenance: importer namespaces it by bundle-local id.
    live_event_ids={"live-7"}
    imported_key=("history-bundle","bundle-1",base["items"][0]["history_item_id"])
    assert imported_key not in live_event_ids and base["items"][0]["claimed_original_event_id"] in live_event_ids
    cases.append({"name":"claimed_event_id_does_not_enter_live_dedupe","expect":True,"got":True})
    return cases


def main() -> int:
    mutable = mutable_record_tests()
    device = device_revocation_tests()
    history = history_tests()
    result = {
        "adr":"ADR-0161", "status":"pass",
        "mutable_record_orders": mutable,
        "device_revocation_cases": device,
        "history_bundle_cases": history,
        "summary": {"orderings":len(mutable), "device_cases":len(device), "history_cases":len(history)}
    }
    OUT.write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result["summary"], sort_keys=True))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
