#!/usr/bin/env python3
"""V-004 group wire freeze (ADR-0176 section 3): semantic vectors.

Every object here is wire-valid (the strict codecs must accept it); only a cross-field or cross-object rule that
neither the registry nor CDDL can express makes it unacceptable. Two independently written checkers
(semantic.py, semantic.mjs) evaluate the rules and must agree with each other and with the expectation.

Provides cases(): list of {"id", "kind": state|intent|binding|bundle|scenario, "object"/..., "expect", "reason"} and
extra_vectors(): [(object name, suffix, body value, bench)] to add to the object vector file.
"""
from __future__ import annotations

import copy

import gen_group_vectors as gv
from gen_group_vectors import (A_ACS, A_CAP, A_INVITE, A_MEM, A_SPACE, ACC_A, ACC_B, ACC_C, B, CONV, DEV, GCS_NEXT_HASH, GCS_TYPICAL,
                               GCS_TYPICAL_HASH, INC, INTENTS, JOIN, M, PA, PB, PC, PSVC, ROLE, T0, auth, det, fd, intent, mdel, mget,
                               mset, setof, top)
import gen_vectors as g

OBJECTS: list[tuple[str, str, dict, bool]] = []   # (name, suffix, body value) emitted as extra wire-valid vectors
CASES: list[dict] = []


def sem(kind, name, expect, reason, obj_name, value, **extra):
    """Register a semantic case over a wire-valid object."""
    suffix = f"semantic-{name}"
    OBJECTS.append((obj_name, suffix, value, False))
    CASES.append({"id": f"{obj_name}/{suffix}", "kind": kind, "object": obj_name, "value": value, "expect": expect, "reason": reason, **extra})


S = "GroupControlState"
base = GCS_TYPICAL
members = mget(base, 6)


def state_case(name, expect, reason, value):
    sem("state", name, expect, reason, S, value)


def build():
    CASES.clear(); OBJECTS.clear()
    # ---- GroupControlState ---------------------------------------------------------------------------------
    state_case("typical-is-consistent", "accept", None, base)
    state_case("genesis-consistent", "accept", None, gv.GCS_GENESIS)
    state_case("version-1-with-previous", "reject", "version_previous_mismatch", mset(gv.GCS_GENESIS, 4, GCS_TYPICAL_HASH))
    state_case("version-5-without-previous", "reject", "version_previous_mismatch", mdel(base, 4))
    state_case("membership-principal-twice", "reject", "membership_principal_duplicate",
               mset(base, 6, setof([gv.membership(PA, "a", [gv.ROLE_ADMIN]), gv.membership(PA, "a-again", None), gv.membership(PB, "b", [gv.ROLE_MEMBER])])))
    state_case("membership-incarnation-reused", "reject", "membership_incarnation_duplicate",
               mset(base, 6, setof([gv.membership(PA, "same", [gv.ROLE_ADMIN]), gv.membership(PB, "same", [gv.ROLE_MEMBER])])))
    state_case("role-not-defined", "reject", "role_unknown",
               mset(base, 6, setof([gv.membership(PA, "a", [gv.ROLE_ADMIN]), gv.membership(PB, "b", [ROLE("ghost")])])))
    dup_role = mset(base, 7, setof([gv.ROLEDEF_ADMIN, mset(gv.ROLEDEF_ADMIN, 1, "Admin 2")]))
    state_case("role-defined-twice", "reject", "role_definition_duplicate", dup_role)
    state_case("succession-repeats-a-device", "reject", "succession_duplicate", mset(base, 10, [B(DEV[0]), B(DEV[1]), B(DEV[0])]))
    state_case("recent-window-without-seconds", "reject", "history_window_mismatch", mset(base, 9, gv.mdel(gv.POLICY_FULL, 3)))
    state_case("window-without-recent-policy", "reject", "history_window_mismatch", mset(base, 9, mset(gv.POLICY_FULL, 2, 1)))
    state_case("invite-twice", "reject", "invite_join_ref_duplicate", mset(base, 11, setof([gv.INVITE_1, mset(gv.INVITE_1, 6, 1)])))
    state_case("invite-uses-exceed-max", "reject", "invite_uses_exceed", mset(base, 11, [mset(gv.INVITE_1, 6, 21)]))
    state_case("invite-issuer-not-a-member", "reject", "invite_issuer_not_member",
               mset(base, 11, [M([0, JOIN(9)], [1, M([0, 1], [1, B(det("outsider", 32))])], [2, INC("o")], [3, 1], [4, T0], [6, 0])]))
    state_case("invite-issuer-reincarnated", "reject", "invite_issuer_not_member", mset(base, 11, [mset(gv.INVITE_1, 2, INC("a-old"))]))
    state_case("governance-principal-not-member", "reject", "governance_principal_not_member",
               mset(base, 8, M([0, setof([PA, M([0, 1], [1, B(det("outsider", 32))])])], [1, 1])))
    state_case("governance-threshold-too-high", "reject", "governance_threshold_exceeds", mset(base, 8, M([0, setof([PA])], [1, 2])))
    state_case("account-principal-with-service-endpoints", "reject", "endpoint_policy_kind_mismatch",
               mset(base, 6, setof([gv.membership(PA, "a", [gv.ROLE_ADMIN]), gv.membership(PB, "b", [gv.ROLE_MEMBER], 2)])))
    state_case("service-principal-with-account-devices", "reject", "endpoint_policy_kind_mismatch",
               mset(base, 6, setof([gv.membership(PA, "a", [gv.ROLE_ADMIN]), gv.membership(PSVC, "svc", None, 1)])))

    # ---- GroupControlIntent --------------------------------------------------------------------------------
    def icase(name, expect, reason, value):
        sem("intent", name, expect, reason, "GroupControlIntent", value)

    for k, v in INTENTS.items():
        icase(f"all-{k}-consistent", "accept", None, v)

    def op_intent(op, args, reducing, authz, suffix="x"):
        return intent(suffix, op, args, reducing, authz)

    rm = INTENTS["member-remove"]
    icase("remove-not-marked-reducing", "reject", "reducing_flag_mismatch", mset(rm, 7, False))
    icase("admit-marked-reducing", "reject", "reducing_flag_mismatch", mset(INTENTS["member-admit"], 7, True))
    icase("leave-not-marked-reducing", "reject", "reducing_flag_mismatch", mset(INTENTS["member-leave"], 7, False))
    icase("profile-set-marked-reducing", "reject", "reducing_flag_mismatch", mset(INTENTS["profile-set"], 7, True))
    icase("role-assign-may-reduce", "accept", None, mset(INTENTS["role-assign"], 7, True))
    icase("governance-set-may-reduce", "accept", None, mset(INTENTS["governance-set"], 7, True))
    icase("leave-with-capability-basis", "reject", "authorization_kind_not_allowed", mset(INTENTS["member-leave"], 9, A_CAP))
    icase("remove-by-membership-basis", "reject", "authorization_kind_not_allowed", mset(rm, 9, A_MEM))
    icase("redeem-by-capability", "reject", "authorization_kind_not_allowed", mset(INTENTS["invite-link-redeem"], 9, A_CAP))
    icase("device-remove-by-capability-only", "reject", "authorization_kind_not_allowed", mset(INTENTS["device-remove-revoked"], 9, A_CAP))
    icase("issue-by-space-delegation", "reject", "authorization_kind_not_allowed", mset(INTENTS["invite-link-issue"], 9, A_SPACE))
    icase("device-add-without-acs-head", "reject", "authorization_field_mismatch", mset(INTENTS["device-add"], 9, auth(1)))
    icase("device-add-admin-approved-with-head", "accept", None, mset(INTENTS["device-add"], 9, auth(1, k1=fd("acs-head"))))
    icase("device-add-by-account-evidence", "accept", None, mset(INTENTS["device-add"], 9, A_ACS))
    icase("capability-with-stray-space-intent", "reject", "authorization_field_mismatch", mset(rm, 9, auth(1, k2=B(g.py_enc(gv.SPACE_ENV)))))
    icase("space-delegated-without-intent", "reject", "authorization_field_mismatch", mset(INTENTS["member-remove-space-delegated"], 9, auth(2)))
    icase("redeem-without-join-ref", "reject", "authorization_field_mismatch", mset(INTENTS["invite-link-redeem"], 9, auth(5)))
    icase("redeem-join-ref-differs-from-args", "reject", "join_ref_mismatch", mset(INTENTS["invite-link-redeem"], 9, auth(5, k3=JOIN(7))))
    icase("admit-via-link-by-membership", "reject", "authorization_kind_not_allowed", mset(INTENTS["member-admit-via-link"], 9, A_MEM))
    icase("security-policy-empty", "reject", "security_policy_empty", mset(INTENTS["security-policy-set"], 8, M()))
    icase("security-policy-window-without-recent", "reject", "history_window_mismatch", mset(INTENTS["security-policy-set"], 8, M([1, 3], [2, 600])))
    icase("governance-set-empty", "reject", "governance_set_empty", mset(INTENTS["governance-set"], 8, M()))
    icase("governance-set-succession-repeats", "reject", "succession_duplicate", mset(INTENTS["governance-set"], 8, M([1, [B(DEV[0]), B(DEV[0])]])))
    icase("coordinator-replaces-itself", "reject", "coordinator_same_device", mset(INTENTS["coordinator-replace"], 8, M([0, B(DEV[0])], [1, B(DEV[0])])))
    inv_used = mset(gv.INVITE_1, 6, 1)
    icase("issue-with-uses-already-counted", "reject", "invite_uses_nonzero", mset(INTENTS["invite-link-issue"], 8, M([0, inv_used])))

    # ---- GroupCommitBinding --------------------------------------------------------------------------------
    def bcase(name, expect, reason, value):
        sem("binding", name, expect, reason, "GroupCommitBinding", value)

    for k, v in gv.BINDINGS.items():
        bcase(f"all-{k}-consistent", "accept", None, v)
    seq, rec = gv.BINDINGS["coordinator-sequenced"], gv.BINDINGS["recovery"]
    bcase("sequenced-without-intent", "reject", "intent_hash_missing", mdel(seq, 9))
    bcase("recovery-with-intent", "reject", "recovery_has_intent", mset(rec, 9, gv.intent_hash(INTENTS["member-remove"])))
    bcase("recovery-changes-state", "reject", "recovery_state_changed", mset(rec, 11, GCS_NEXT_HASH))
    bcase("recovery-marked-reducing", "reject", "recovery_reducing", mset(rec, 4, True))
    bcase("sequenced-leaves-state-unchanged", "reject", "state_unchanged", mset(seq, 11, GCS_TYPICAL_HASH))
    bcase("epoch-skips", "reject", "epoch_step", mset(seq, 8, 9))
    bcase("epoch-repeats", "reject", "epoch_step", mset(seq, 8, 7))
    bcase("direct-not-reducing", "reject", "direct_not_reducing", mset(gv.BINDINGS["direct-reduction"], 4, False))
    bcase("readded-outside-recovery", "reject", "readded_outside_recovery", mset(seq, 13, [B(DEV[2])]))

    # ---- GroupHistoryBundle structure ----------------------------------------------------------------------
    def hcase(name, expect, reason, value):
        sem("bundle", name, expect, reason, "GroupHistoryBundle", value)

    bun = gv.BUNDLE
    i1, i2 = gv.ITEM1, gv.ITEM2
    hcase("typical-is-consistent", "accept", None, bun)
    hcase("item-id-twice", "reject", "item_id_duplicate", mset(bun, 11, [i1, mset(i2, 0, mget(i1, 0))]))
    hcase("reply-to-later-item", "reject", "reply_to_unknown_or_forward", mset(bun, 11, [mset(i1, 6, mget(i2, 0)), i2]))
    hcase("reply-to-itself", "reject", "reply_to_unknown_or_forward", mset(bun, 11, [mset(i1, 6, mget(i1, 0))]))
    hcase("reply-to-a-live-event-id", "reject", "reply_to_unknown_or_forward", mset(bun, 11, [i1, mset(i2, 6, B(det("event-live-7", 16)))]))
    hcase("item-without-text-or-attachments", "reject", "item_empty", mset(bun, 11, [M([0, B(det("hist-9", 16))])]))
    hcase("author-is-a-service-principal", "reject", "author_not_account", mset(bun, 4, PSVC))
    hcase("bundle-to-self", "reject", "recipient_is_author", mset(bun, 6, PA))
    return CASES


# ---------------------------------------------------------------------------------------------------------------
# GroupHistoryBundle admission scenarios (criterion 7). A scenario fixes the receiving client's Room view, then
# applies steps in order; each step is a live ConversationEvent or a bundle import.
def pk(p):
    m = {x[0]: x[1] for x in p["$map"]}
    return f"{m[0]}:{m[1]['$bytes']}"


def hx(b):
    return b.hex()


def room(**kw):
    members_a = {pk(PA): INC("a")["$bytes"], pk(PB): INC("b")["$bytes"], pk(PC): INC("c")["$bytes"]}
    r = {
        "conversation_id": CONV.hex(),
        "states": {
            mget(GCS_TYPICAL_HASH, 1)["$bytes"]: {"mls_epoch": 7, "members": members_a},
            mget(GCS_NEXT_HASH, 1)["$bytes"]: {"mls_epoch": 8, "members": {pk(PA): INC("a")["$bytes"], pk(PB): INC("b")["$bytes"]}},
        },
        "latest_state": mget(GCS_TYPICAL_HASH, 1)["$bytes"],
        "eligible_devices": {pk(PA): [hx(DEV[0])], pk(PB): [hx(DEV[1])], pk(PC): [hx(DEV[2])]},
        "local": {"account": pk(PC), "incarnation": INC("c")["$bytes"]},
        "live_events": [hx(det("event-live-7", 16))],
        "reducers": {"pins": {hx(det("event-live-7", 16)): True}, "polls": {}},
    }
    r.update(kw)
    return r


def bundle_step(name, value, signer=None):
    OBJECTS.append(("GroupHistoryBundle", f"semantic-scenario-{name}", value, False))
    return {"import": value, "signer": signer or hx(DEV[0]), "vector": f"GroupHistoryBundle/semantic-scenario-{name}"}


def live_pin(eid, target, present):
    return {"live": {"event_id": hx(det(eid, 16)), "kind": "pin.set", "target": hx(det(target, 16)), "present": present}}


def bundle_with(items, **fields):
    b = gv.BUNDLE
    for k, v in fields.items():
        b = mset(b, int(k[1:]), v)
    return mset(b, 11, items)


def scenarios():
    live7 = B(det("event-live-7", 16))
    claim_live = M([0, B(det("hist-c1", 16))], [1, live7], [2, PB], [3, T0 - 100], [4, "claims to be the live pin target"])
    claim_future = M([0, B(det("hist-c2", 16))], [1, B(det("event-future-9", 16))], [2, PA], [3, T0 - 50], [4, "claims an event that arrives live later"])
    out = []
    live_ids = hx(det("event-live-7", 16))
    both = sorted([live_ids, hx(det("event-future-9", 16))])
    pins7 = {"pins": {live_ids: True}, "polls": {}}

    def sc(name, room_ctx, steps, final):
        out.append({"id": f"scenario/{name}", "kind": "scenario", "expect": "accept", "reason": None, "room": room_ctx, "steps": steps, "final": final})

    rejected = lambda reason, **kw: {"verdicts": [f"reject:{reason}"], "history_items": 0, "bundles_imported": 0, "live_events": [live_ids], "reducers": pins7, **kw}
    ok = bundle_with([claim_live, claim_future])
    sc("bound-bundle-accepted", room(), [bundle_step("ok", ok)],
       {"verdicts": ["accept"], "history_items": 2, "bundles_imported": 1, "live_events": [live_ids], "reducers": pins7})
    sc("claimed-live-event-id-never-enters-dedupe", room(), [bundle_step("claim", ok)],
       {"verdicts": ["accept"], "live_events": [live_ids], "reducers": pins7})
    sc("claimed-id-does-not-suppress-the-live-event", room(), [bundle_step("claim2", ok), live_pin("event-future-9", "event-live-7", False)],
       {"verdicts": ["accept", "live"], "live_events": both, "reducers": {"pins": {live_ids: False}, "polls": {}}, "history_items": 2})
    sc("live-event-first-then-bundle", room(), [live_pin("event-future-9", "event-live-7", False), bundle_step("claim3", ok)],
       {"verdicts": ["live", "accept"], "live_events": both, "reducers": {"pins": {live_ids: False}, "polls": {}}, "history_items": 2})
    sc("display-label-names-the-bundle-author", room(), [bundle_step("label", ok)],
       {"verdicts": ["accept"], "labels": ["shared by " + pk(PA)] * 2})
    sc("cross-room-bundle", room(), [bundle_step("xroom", mset(ok, 3, B(det("another-room", 32))))], rejected("wrong_conversation"))
    sc("other-recipient", room(), [bundle_step("other", mset(ok, 6, PB))], rejected("wrong_recipient"))
    sc("recipient-re-added-incarnation", room(), [bundle_step("reinc", mset(ok, 7, INC("c-old")))], rejected("wrong_recipient_incarnation"))
    sc("unknown-state-hash", room(), [bundle_step("nostate", mset(ok, 8, fd("unknown-state")))], rejected("unknown_state"))
    sc("epoch-does-not-match-state", room(), [bundle_step("epoch", mset(ok, 9, 8))], rejected("epoch_mismatch"))
    sc("author-not-member-at-bound-state", room(), [bundle_step("author", mset(ok, 4, M([0, 1], [1, B(det("never-a-member", 32))])))], rejected("author_not_member"))
    sc("recipient-removed-at-latest-state", room(latest_state=mget(GCS_NEXT_HASH, 1)["$bytes"]), [bundle_step("recipgone", ok)], rejected("recipient_removed"))
    gone = room(latest_state=mget(GCS_NEXT_HASH, 1)["$bytes"])
    gone["states"][mget(GCS_NEXT_HASH, 1)["$bytes"]]["members"].pop(pk(PA))
    sc("author-removed-at-latest-state", gone, [bundle_step("authorgone", ok)], rejected("author_removed"))
    reinc = room(latest_state=mget(GCS_NEXT_HASH, 1)["$bytes"])
    reinc["states"][mget(GCS_NEXT_HASH, 1)["$bytes"]]["members"][pk(PA)] = INC("a-new")["$bytes"]
    sc("author-re-added-under-a-new-incarnation", reinc, [bundle_step("authorreinc", ok)], rejected("author_removed"))
    inelig = room(eligible_devices={pk(PA): [hx(DEV[5])], pk(PB): [hx(DEV[1])], pk(PC): [hx(DEV[2])]})
    sc("author-device-no-longer-eligible", inelig, [bundle_step("quarantined", ok)], rejected("author_device_ineligible"))
    sc("signer-is-not-the-author-device", room(), [bundle_step("signer", ok, signer=hx(DEV[4]))], rejected("signer_not_author_device"))
    sc("replayed-bundle-is-a-no-op", room(), [bundle_step("replay1", ok), bundle_step("replay2", ok)],
       {"verdicts": ["accept", "duplicate"], "history_items": 2, "bundles_imported": 1})
    sc("same-nonce-new-id-is-a-duplicate", room(), [bundle_step("nonce1", ok), bundle_step("nonce2", mset(ok, 2, B(det("bundle-other-id", 16))))],
       {"verdicts": ["accept", "duplicate"], "history_items": 2, "bundles_imported": 1})
    sc("structurally-bad-bundle-adds-nothing", room(), [bundle_step("dup", bundle_with([claim_live, mset(claim_future, 0, mget(claim_live, 0))]))],
       rejected("item_id_duplicate"))
    return out


def all_cases():
    cs = build()
    cs += scenarios()
    return cs


def extra_vectors():
    return list(OBJECTS)
