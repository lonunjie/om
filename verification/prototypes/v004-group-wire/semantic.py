#!/usr/bin/env python3
"""V-004 group wire freeze (ADR-0176 section 3): Python semantic checker. Feasibility evidence, not product code.

semantic.py SEMANTIC.json > results.json

Evaluates the cross-field rules that registry and CDDL cannot express, on diagnostic object values
({"$map": [[key, value], ...]}, {"$bytes": hex}). Written from the rule list in ADR-0176 section 3 without reading
semantic.mjs (the Node checker); the two must agree on every case.
"""
import json
import sys

OP = {"member.admit": 1, "member.remove": 2, "member.leave": 3, "device.add": 4, "device.remove_revoked": 5, "profile.set": 6,
      "disappearing_timer.set": 7, "role.define": 8, "role.assign": 9, "invite_link.issue": 10, "invite_link.revoke": 11,
      "invite_link.redeem": 12, "admission.set": 13, "security_policy.set": 14, "governance.set": 15, "coordinator.replace": 16}
ALWAYS_REDUCING = {2, 3, 5, 11, 16}
NEVER_REDUCING = {1, 4, 6, 7, 10, 12}
KINDS_ALLOWED = {1: {1, 2}, 2: {1, 2}, 3: {3}, 4: {1, 4}, 5: {4}, 6: {1}, 7: {1}, 8: {1, 2}, 9: {1, 2}, 10: {1}, 11: {1, 3},
                 12: {5}, 13: {1}, 14: {1}, 15: {1}, 16: {3}}
NEEDS_ACS_HEAD = {4, 5}


def mp(v):
    return {k: x for k, x in v["$map"]}


def hexof(v):
    return v["$bytes"]


def dig(v):
    return hexof(mp(v)[1])


def pk(p):
    m = mp(p)
    return f"{m[0]}:{hexof(m[1])}"


def check_state(v):
    s = mp(v)
    version, prev = s[3], s.get(4)
    if (version == 1) != (prev is None):
        return "version_previous_mismatch"
    mem = [mp(x) for x in s[6]]
    principals = [pk(m[0]) for m in mem]
    if len(set(principals)) != len(principals):
        return "membership_principal_duplicate"
    incs = [hexof(m[1]) for m in mem]
    if len(set(incs)) != len(incs):
        return "membership_incarnation_duplicate"
    defs = [hexof(mp(r)[0]) for r in s.get(7, [])]
    if len(set(defs)) != len(defs):
        return "role_definition_duplicate"
    for m in mem:
        for r in m.get(2, []):
            if hexof(r) not in defs:
                return "role_unknown"
    for m in mem:
        kind = mp(m[0])[0]
        ep = m[3]
        if (kind == 1) != (ep == 1):
            return "endpoint_policy_kind_mismatch"
    succ = [hexof(d) for d in s[10]]
    if len(set(succ)) != len(succ):
        return "succession_duplicate"
    pol = mp(s[9])
    if (pol[2] == 2) != (3 in pol):
        return "history_window_mismatch"
    by_principal = {pk(m[0]): hexof(m[1]) for m in mem}
    refs = [hexof(mp(i)[0]) for i in s.get(11, [])]
    if len(set(refs)) != len(refs):
        return "invite_join_ref_duplicate"
    for inv in (mp(i) for i in s.get(11, [])):
        if 5 in inv and inv[6] > inv[5]:
            return "invite_uses_exceed"
        if by_principal.get(pk(inv[1])) != hexof(inv[2]):
            return "invite_issuer_not_member"
    gov = mp(s[8])
    if any(pk(p) not in by_principal for p in gov[0]):
        return "governance_principal_not_member"
    if gov[1] > len(gov[0]):
        return "governance_threshold_exceeds"
    return None


def check_intent(v):
    i = mp(v)
    op, args, authz = i[6], mp(i[8]), mp(i[9])
    reducing = i[7]
    if op in ALWAYS_REDUCING and not reducing:
        return "reducing_flag_mismatch"
    if op in NEVER_REDUCING and reducing:
        return "reducing_flag_mismatch"
    kind = authz[0]
    if kind not in KINDS_ALLOWED[op]:
        return "authorization_kind_not_allowed"
    if op == 1 and 4 in args and kind != 1:
        return "authorization_kind_not_allowed"
    has_head, has_space, has_join = 1 in authz, 2 in authz, 3 in authz
    need_head = kind == 4 or op in NEEDS_ACS_HEAD
    if has_head != need_head or has_space != (kind == 2) or has_join != (kind == 5):
        return "authorization_field_mismatch"
    if op == 12 and hexof(authz[3]) != hexof(args[0]):
        return "join_ref_mismatch"
    if op == 14:
        if not args:
            return "security_policy_empty"
        if 2 in args and 1 in args and args[1] != 2:
            return "history_window_mismatch"
    if op == 15:
        if not args:
            return "governance_set_empty"
        if 1 in args and len({hexof(d) for d in args[1]}) != len(args[1]):
            return "succession_duplicate"
    if op == 16 and hexof(args[0]) == hexof(args[1]):
        return "coordinator_same_device"
    if op == 10 and mp(args[0])[6] != 0:
        return "invite_uses_nonzero"
    return None


def check_binding(v):
    b = mp(v)
    kind = b[3]
    reducing = b[4]
    prev, res = dig(b[5]), dig(b[11])
    if kind == 3:
        if 9 in b:
            return "recovery_has_intent"
        if prev != res:
            return "recovery_state_changed"
        if reducing:
            return "recovery_reducing"
    else:
        if 9 not in b:
            return "intent_hash_missing"
        if prev == res:
            return "state_unchanged"
        if kind == 2 and not reducing:
            return "direct_not_reducing"
        if 13 in b:
            return "readded_outside_recovery"
    if b[8] != b[7] + 1:
        return "epoch_step"
    return None


def check_bundle(v):
    b = mp(v)
    if mp(b[4])[0] != 1:
        return "author_not_account"
    if mp(b[6])[0] != 1:
        return "recipient_not_account"
    if pk(b[4]) == pk(b[6]):
        return "recipient_is_author"
    seen = []
    for it in (mp(x) for x in b[11]):
        iid = hexof(it[0])
        if iid in seen:
            return "item_id_duplicate"
        if 4 not in it and 5 not in it:
            return "item_empty"
        if 6 in it and hexof(it[6]) not in seen:
            return "reply_to_unknown_or_forward"
        seen.append(iid)
    return None


def admit(room, store, bundle_value, signer):
    b = mp(bundle_value)
    author, recipient = pk(b[4]), pk(b[6])
    if signer != hexof(b[5]):
        return "reject", "signer_not_author_device"
    if hexof(b[3]) != room["conversation_id"]:
        return "reject", "wrong_conversation"
    if recipient != room["local"]["account"]:
        return "reject", "wrong_recipient"
    if hexof(b[7]) != room["local"]["incarnation"]:
        return "reject", "wrong_recipient_incarnation"
    sh = dig(b[8])
    st = room["states"].get(sh)
    if st is None:
        return "reject", "unknown_state"
    if st["mls_epoch"] != b[9]:
        return "reject", "epoch_mismatch"
    if author not in st["members"]:
        return "reject", "author_not_member"
    if st["members"].get(recipient) != hexof(b[7]):
        return "reject", "recipient_not_member"
    latest = room["states"][room["latest_state"]]
    if latest["members"].get(author) != st["members"][author]:
        return "reject", "author_removed"
    if latest["members"].get(recipient) != hexof(b[7]):
        return "reject", "recipient_removed"
    if hexof(b[5]) not in room["eligible_devices"].get(author, []):
        return "reject", "author_device_ineligible"
    why = check_bundle(bundle_value)
    if why:
        return "reject", why
    bid, nonce = hexof(b[2]), hexof(b[10])
    if bid in store["bundle_ids"] or nonce in store["nonces"]:
        return "duplicate", None
    store["bundle_ids"].add(bid)
    store["nonces"].add(nonce)
    for it in (mp(x) for x in b[11]):
        # inert archive namespace: never the live dedupe set, never a reducer
        store["history"].append({"ns": ["bundle", bid, hexof(it[0])], "label": "shared by " + author})
    return "accept", None


def run_scenario(c):
    room = c["room"]
    store = {"bundle_ids": set(), "nonces": set(), "history": []}
    live = set(room["live_events"])
    reducers = json.loads(json.dumps(room["reducers"]))
    verdicts = []
    for step in c["steps"]:
        if "live" in step:
            e = step["live"]
            if e["event_id"] in live:
                verdicts.append("duplicate_live")
                continue
            live.add(e["event_id"])
            if e["kind"] == "pin.set":
                reducers["pins"][e["target"]] = e["present"]
            verdicts.append("live")
        else:
            v, why = admit(room, store, step["import"], step["signer"])
            verdicts.append(v if why is None else f"{v}:{why}")
    return {"verdicts": verdicts, "history_items": len(store["history"]), "bundles_imported": len(store["bundle_ids"]),
            "live_events": sorted(live), "reducers": reducers, "labels": [h["label"] for h in store["history"]]}


def main(path):
    out = {}
    for c in json.load(open(path))["cases"]:
        k = c["kind"]
        if k == "scenario":
            out[c["id"]] = run_scenario(c)
            continue
        why = {"state": check_state, "intent": check_intent, "binding": check_binding, "bundle": check_bundle}[k](c["value"])
        out[c["id"]] = {"verdict": "accept" if why is None else "reject", "reason": why}
    print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    main(sys.argv[1])
