#!/usr/bin/env python3
"""V-004 group wire freeze (ADR-0176). Feasibility evidence, not product code.

Generates positive and negative object vectors for the W1 `groups` wire: GroupControlState (28), GroupControlIntent
(29), GroupCommitBinding (30), GroupHistoryBundle (31) and the typed pin/poll ConversationEvent bodies. It reuses
the V-005 generator as a module (its TYPES filter is widened to the W1 and W3 types), so positive bytes come from the
unmodified verification/cbor/python_canonical.py encoder and negatives from the same raw emitter.

Usage: gen_group_vectors.py OUT.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mset, py_enc, raw, setof, top  # noqa: E402

# widen the V-005 W0 filter to the later-wave types this freeze needs (28-31, and SpaceDelegatedIntent 25 for the
# space_delegated authorization evidence)
for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
TYPES = g.TYPES

CONV = det("room-conversation-id", 32)
ACC_A, ACC_B, ACC_C = (det(f"group-account-{n}", 32) for n in "abc")
SVC = det("group-service-principal", 32)
PA, PB, PC = (M([0, 1], [1, B(a)]) for a in (ACC_A, ACC_B, ACC_C))
PSVC = M([0, 2], [1, B(SVC)])
DEV = [det(f"group-device-{i}", 16) for i in range(1, 9)]
INC = lambda n: B(det(f"membership-incarnation-{n}", 16))
ROLE = lambda n: B(det(f"role-{n}", 16))
JOIN = lambda n: B(det(f"join-ref-{n}", 16))
T0 = g.T0


def fd(label):
    return g.fake_digest(label)


# ------------------------------------------------------------------ state
def membership(p, inc, roles=None, ep=1):
    pairs = [[0, p], [1, INC(inc)], [3, ep]]
    if roles:
        pairs.insert(2, [2, setof(roles)])
    return M(*pairs)


ROLE_ADMIN, ROLE_MEMBER = ROLE("admin"), ROLE("member")
ROLEDEF_ADMIN = M([0, ROLE_ADMIN], [1, "Admin"], [2, setof(["room.manage_roles", "room.invite", "room.manage_admission", "room.remove_member", "room.post", "room.react"])],
                  [3, setof(["room.post", "room.react"])])
ROLEDEF_MEMBER = M([0, ROLE_MEMBER], [1, "Member"], [2, setof(["room.post", "room.reply", "room.react"])])
POLICY = M([0, 2], [1, 1], [2, 1], [5, 1], [6, 300], [7, 2])
POLICY_FULL = M([0, 3], [1, 2], [2, 2], [3, 604800], [4, 86400], [5, 1], [6, 3600], [7, 8])
GOV = M([0, setof([PA])], [1, 1])
GOV_FULL = M([0, setof([PA, PB])], [1, 2], [2, setof([2, 15, 16])])
INVITE_1 = M([0, JOIN(1)], [1, PA], [2, INC("a")], [3, 1], [4, T0 + 7 * 86400], [5, 20], [6, 3])
INVITE_2 = M([0, JOIN(2)], [1, PB], [2, INC("b")], [3, 2], [4, T0 + 86400], [6, 0])

GCS_GENESIS = top("GroupControlState", [2, B(CONV)], [3, 1], [5, M([0, "Book club"])],
                  [6, [membership(PA, "a", [ROLE_ADMIN])]], [7, [ROLEDEF_ADMIN]], [8, GOV], [9, POLICY], [10, [B(DEV[0])]], [12, 0])
GCS_GENESIS_HASH = g.digest_of("OM-GROUP-CONTROL-HASH-v1", py_enc(GCS_GENESIS))
GCS_MIN = top("GroupControlState", [2, B(CONV)], [3, 1], [5, M([0, "G"])], [6, [membership(PA, "a")]], [8, GOV], [9, POLICY], [10, [B(DEV[0])]], [12, 0])
GCS_TYPICAL = top("GroupControlState", [2, B(CONV)], [3, 5], [4, GCS_GENESIS_HASH],
                  [5, M([0, "Book club"], [1, "https://blobs.example.net/avatar/1"], [2, "Monthly reading group"])],
                  [6, setof([membership(PA, "a", [ROLE_ADMIN]), membership(PB, "b", [ROLE_MEMBER]), membership(PC, "c"), membership(PSVC, "svc", None, 2)])],
                  [7, setof([ROLEDEF_ADMIN, ROLEDEF_MEMBER])], [8, GOV_FULL], [9, POLICY_FULL],
                  [10, [B(DEV[0]), B(DEV[1]), B(DEV[2])]], [11, setof([INVITE_1, INVITE_2])], [12, 7])
GCS_TYPICAL_HASH = g.digest_of("OM-GROUP-CONTROL-HASH-v1", py_enc(GCS_TYPICAL))
GCS_NEXT = top("GroupControlState", [2, B(CONV)], [3, 6], [4, GCS_TYPICAL_HASH], [5, M([0, "Book club"])],
               [6, setof([membership(PA, "a", [ROLE_ADMIN]), membership(PB, "b", [ROLE_MEMBER])])], [8, GOV], [9, POLICY], [10, [B(DEV[0])]], [12, 8])
GCS_NEXT_HASH = g.digest_of("OM-GROUP-CONTROL-HASH-v1", py_enc(GCS_NEXT))


# ------------------------------------------------------------------ intents
def auth(kind, **kw):
    pairs = [[0, kind]]
    for k, v in kw.items():
        pairs.append([int(k[1:]), v])
    return M(*pairs)


A_CAP, A_MEM = auth(1), auth(3)
ACS_HEAD = fd("acs-head-revoked-device")
A_ACS = auth(4, k1=ACS_HEAD)
SPACE_INTENT_BODY = top("SpaceDelegatedIntent", [2, B(det("space-id", 32))], [3, fd("space-state")], [4, 4], [5, B(CONV)], [6, "control.member.remove"],
                        [7, True], [8, 1], [9, B(det("space-evidence", 48))], [10, PB], [11, INC("b")])
SPACE_ENV, _ = g.envelope("SpaceDelegatedIntent", SPACE_INTENT_BODY, ["device"])
A_SPACE = auth(2, k2=B(py_enc(SPACE_ENV)))
A_INVITE = auth(5, k3=JOIN(1))


def intent(suffix, op, args, reducing, authz, prev=GCS_TYPICAL_HASH, ver=5, iid=None):
    return top("GroupControlIntent", [2, B(CONV)], [3, prev], [4, ver], [5, B(det(f"intent-{suffix}", 16))], [6, op], [7, reducing], [8, args], [9, authz])


INTENTS = {
    "member-admit": intent("admit", 1, M([0, PC], [1, INC("c2")], [2, [ROLE_MEMBER]], [3, 1]), False, A_CAP),
    "member-admit-via-link": intent("admit-link", 1, M([0, PC], [1, INC("c3")], [3, 1], [4, JOIN(1)]), False, A_CAP),
    "member-remove": intent("remove", 2, M([0, PB], [1, INC("b")]), True, A_CAP),
    "member-remove-space-delegated": intent("remove-space", 2, M([0, PB], [1, INC("b")]), True, A_SPACE),
    "member-leave": intent("leave", 3, M([0, PB], [1, INC("b")]), True, A_MEM),
    "device-add": intent("dev-add", 4, M([0, PB], [1, INC("b")], [2, B(DEV[3])], [3, 4]), False, A_ACS),
    "device-remove-revoked": intent("dev-rm", 5, M([0, PB], [1, INC("b")], [2, B(DEV[3])], [3, 4]), True, A_ACS),
    "profile-set": intent("profile", 6, M([0, M([0, "New name"], [2, "d"])]), False, A_CAP),
    "disappearing-timer-set": intent("timer", 7, M([0, 3600]), False, A_CAP),
    "disappearing-timer-off": intent("timer-off", 7, M([0, 0]), False, A_CAP),
    "role-define": intent("role-def", 8, M([0, ROLEDEF_MEMBER]), False, A_CAP),
    "role-assign": intent("role-asg", 9, M([0, PB], [1, INC("b")], [2, setof([ROLE_ADMIN, ROLE_MEMBER])]), False, A_CAP),
    "role-assign-clear": intent("role-clr", 9, M([0, PB], [1, INC("b")], [2, []]), False, A_CAP),
    "invite-link-issue": intent("inv-issue", 10, M([0, mset(INVITE_1, 6, 0)]), False, A_CAP),
    "invite-link-revoke": intent("inv-revoke", 11, M([0, JOIN(1)]), True, A_CAP),
    "invite-link-redeem": intent("inv-redeem", 12, M([0, JOIN(1)], [1, PC], [2, INC("c4")], [3, 1]), False, A_INVITE),
    "admission-set": intent("adm", 13, M([0, 3]), False, A_CAP),
    "security-policy-set": intent("sec", 14, M([0, 2], [1, 2], [2, 600], [3, 1]), False, A_CAP),
    "security-policy-set-one-field": intent("sec1", 14, M([1, 1]), False, A_CAP),
    "governance-set": intent("gov", 15, M([0, GOV_FULL], [1, [B(DEV[1]), B(DEV[0])]]), False, A_CAP),
    "coordinator-replace": intent("coord", 16, M([0, B(DEV[0])], [1, B(DEV[1])]), True, A_MEM),
}
INTENT_MIN = INTENTS["admission-set"]


# ------------------------------------------------------------------ commit bindings
def binding(suffix, kind, reducing, intent_hash, readded=None, prev=GCS_TYPICAL_HASH, res=GCS_NEXT_HASH, pe=7, re=8, ver=5):
    pairs = [[2, B(CONV)], [3, kind], [4, reducing], [5, prev], [6, ver], [7, pe], [8, re], [10, fd(f"mls-commit-{suffix}")], [11, res], [12, B(DEV[0])]]
    if intent_hash is not None:
        pairs.append([9, intent_hash])
    if readded:
        pairs.append([13, setof([B(d) for d in readded])])
    return top("GroupCommitBinding", *sorted(pairs, key=lambda p: p[0]))


def intent_hash(i):
    return g.digest_of("OM-GROUP-INTENT-HASH-v1", py_enc(i))


BINDINGS = {
    "coordinator-sequenced": binding("seq", 1, False, intent_hash(INTENTS["admission-set"])),
    "direct-reduction": binding("direct", 2, True, intent_hash(INTENTS["member-remove"])),
    "recovery": binding("recovery", 3, False, None, readded=[DEV[2], DEV[3]], res=GCS_TYPICAL_HASH, re=8),
    "recovery-without-readd-list": binding("recovery-min", 3, False, None, res=GCS_TYPICAL_HASH),
}


# ------------------------------------------------------------------ history bundle
ATT = g.ATT
ITEM1 = M([0, B(det("hist-1", 16))], [4, "Welcome to the group"])
ITEM2 = M([0, B(det("hist-2", 16))], [1, B(det("claimed-event", 16))], [2, PB], [3, T0 - 3600], [4, "see the photo"], [5, [ATT]], [6, B(det("hist-1", 16))])
BUNDLE = top("GroupHistoryBundle", [2, B(det("bundle-1", 16))], [3, B(CONV)], [4, PA], [5, B(DEV[0])], [6, PC], [7, INC("c")], [8, GCS_TYPICAL_HASH], [9, 7],
             [10, B(det("bundle-nonce", 16))], [11, [ITEM1, ITEM2]])
BUNDLE_MIN = top("GroupHistoryBundle", [2, B(det("bundle-2", 16))], [3, B(CONV)], [4, PA], [5, B(DEV[0])], [6, PC], [7, INC("c")], [8, GCS_TYPICAL_HASH], [9, 7],
                 [10, B(det("bundle-nonce-2", 16))], [11, [ITEM1]])

# ------------------------------------------------------------------ typed W1 event bodies
EID = lambda n: B(det(f"event-{n}", 16))
OPT = lambda i, label: M([0, i], [1, label])
RICHQ = M([0, 1], [1, [M([0, 1], [1, [M([0, 1], [1, "Lunch?"])]])]])
EV = {
    "pin-set": g.event("pin", 6, M([0, EID("create")], [1, True])),
    "pin-clear": g.event("unpin", 6, M([0, EID("create")], [1, False])),
    "poll-create": g.event("poll", 8, M([0, RICHQ], [1, [OPT(0, "noodles"), OPT(1, "rice"), OPT(2, "salad")]], [2, 1], [4, 2])),
    "poll-create-multiple": g.event("poll-m", 8, M([0, RICHQ], [1, [OPT(1, "a"), OPT(5, "b"), OPT(9, "c")]], [2, 2], [3, 2], [4, 3])),
    "poll-create-32-options": g.event("poll-32", 8, M([0, RICHQ], [1, [OPT(i, f"o{i}") for i in range(32)]], [2, 2], [4, 1])),
    "poll-vote": g.event("vote", 9, M([0, EID("poll")], [1, [1]])),
    "poll-vote-retract": g.event("vote-r", 9, M([0, EID("poll")], [1, []])),
    "poll-vote-multiple": g.event("vote-m", 9, M([0, EID("poll-m")], [1, setof([1, 9])])),
    "poll-vote-32": g.event("vote-32", 9, M([0, EID("poll-32")], [1, setof(list(range(32)))])),
    "poll-close": g.event("close", 10, M([0, EID("poll")])),
    "redact": g.event("redact", 7, M([0, EID("create")], [1, "spam"])),
    "topic-create": g.event("tc", 11, M([0, B(det("topic-1", 16))], [1, "Planning"])),
    "topic-update": g.event("tu", 12, M([0, B(det("topic-1", 16))], [1, "Planning 2"])),
    "topic-status": g.event("ts", 13, M([0, B(det("topic-1", 16))], [1, 3])),
    "message-topic-set": g.event("mts", 14, M([0, EID("create")], [1, B(det("topic-1", 16))])),
    "message-topic-clear": g.event("mtc", 14, M([0, EID("create")])),
}
EVENT_BODY = lambda ev: mget(ev, 11)
mget = g.mget

POSITIVES = [("GroupControlState", "genesis", GCS_GENESIS, ["device"], False), ("GroupControlState", "minimal", GCS_MIN, ["device"], False),
             ("GroupControlState", "typical", GCS_TYPICAL, ["device"], True), ("GroupControlState", "after-removal", GCS_NEXT, ["device"], False)]
POSITIVES += [("GroupControlIntent", k, v, ["device"], k == "member-remove") for k, v in INTENTS.items()]
POSITIVES += [("GroupCommitBinding", k, v, ["device"], k == "direct-reduction") for k, v in BINDINGS.items()]
POSITIVES += [("GroupHistoryBundle", "typical", BUNDLE, ["device"], True), ("GroupHistoryBundle", "minimal", BUNDLE_MIN, ["device"], False)]
POSITIVES += [("ConversationEvent", "w1-" + k, v, None, False) for k, v in EV.items()]

MINIMALS = {"GroupControlState": GCS_MIN, "GroupControlIntent": INTENT_MIN, "GroupCommitBinding": BINDINGS["coordinator-sequenced"], "GroupHistoryBundle": BUNDLE_MIN}


def gen():
    g.VECTORS.clear()
    for p in POSITIVES:
        g.emit_positive(*p)
    # positive: unknown non-critical extension is accepted and preserved
    for name, base in MINIMALS.items():
        ext = mset(base, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
        g.emit_positive(name, "noncritical-extension-preserved", ext, ["device"], False)

    # generic mutations on every group type (minimal instance), as V-005 does for W0 types
    for name, base in MINIMALS.items():
        for suffix, err, mut, cddl in g.body_mutations(name, base):
            if suffix == "nonminimal-int":  # type ids 28-31 already need a 2-byte head, so widen to 3 bytes
                mut = mset(base, 0, {"$nonmin": mget(base, 0), "w": 3})
            g.emit_negative(name, suffix, err, g.wrap_top(name, mut), cddl)
        g.emit_negative(name, "trailing-bytes", "trailing_bytes", g.wrap_top(name, base) + b"\x00")
        g.emit_negative(name, "truncated", "truncated", g.wrap_top(name, base)[:-1])
        big = mset(base, 23, M(["org.example.padding", M([0, False], [1, B(b"\x00" * TYPES[name]["max_bytes"])])]))
        g.emit_negative(name, "over-size-limit", "size_limit", g.wrap_top(name, big))
        srt = sorted(base["$map"], key=lambda p: p[0])
        body = raw({"$rawmap": [srt[1], srt[0]] + srt[2:]})
        env, _ = g.envelope(name, None, None, body_bytes=body)
        g.emit_negative(name, "noncanonical-inner-body", "unsorted_keys", raw(env))
        env2, _ = g.envelope(name, None, None, body_bytes=raw(base) + b"\x00")
        g.emit_negative(name, "inner-body-trailing-bytes", "trailing_bytes", raw(env2))
        env3, _ = g.envelope(name, base)
        g.emit_negative(name, "envelope-no-signatures", "value_range", raw(mset(env3, 2, [])))
        g.emit_negative(name, "envelope-unknown-key", "unknown_field", raw(mset(env3, 3, 0)))
        g.emit_negative(name, "envelope-indefinite-body", "indefinite_length", raw(mset(env3, 1, {"$indef": mget(env3, 1)})))
        sig = mget(env3, 2)[0]
        g.emit_negative(name, "signature-role-out-of-range", "value_range", raw(mset(env3, 2, [mset(sig, 2, 7)])))
        g.emit_negative(name, "signature-profile-zero", "value_range", raw(mset(env3, 2, [mset(sig, 1, 0)])))

    def neg(name, suffix, err, mutated, cddl="invalid"):
        g.emit_negative(name, suffix, err, g.wrap_top(name, mutated), cddl)

    def negev(suffix, err, ev, cddl="invalid"):
        g.emit_negative("ConversationEvent", "w1-" + suffix, err, raw(ev), cddl)

    S = "GroupControlState"
    neg(S, "version-zero", "value_range", mset(GCS_TYPICAL, 3, 0))
    neg(S, "membership-empty", "value_range", mset(GCS_TYPICAL, 6, []))
    neg(S, "membership-unsorted", "set_order", mset(GCS_TYPICAL, 6, list(reversed(mget(GCS_TYPICAL, 6)))), "valid")
    neg(S, "membership-duplicate-entry", "set_order", mset(GCS_TYPICAL, 6, [mget(GCS_TYPICAL, 6)[0]] * 2), "valid")
    many = [membership(M([0, 1], [1, B(det(f"many-{i}", 32))]), f"many-{i}") for i in range(513)]
    neg(S, "membership-513", "value_range", mset(GCS_TYPICAL, 6, setof(many)))
    # the W0 SignedEnvelope body limit (64 KiB) binds before any count limit for large states (ADR-0176 finding F-27)
    wide = [membership(M([0, 1], [1, B(det(f"wide-{i}", 32))]), f"wide-{i}", [ROLE(j) for j in range(40)]) for i in range(450)]
    neg(S, "membership-exceeds-envelope-body-limit", "size_limit", mset(GCS_TYPICAL, 6, setof(wide)), "n/a")
    neg(S, "membership-incarnation-15-bytes", "value_range", mset(GCS_TYPICAL, 6, [M([0, PA], [1, B(det("x", 15))], [3, 1])]))
    neg(S, "endpoint-policy-3", "value_range", mset(GCS_TYPICAL, 6, [M([0, PA], [1, INC("a")], [3, 3])]))
    neg(S, "role-ids-unsorted", "set_order", mset(GCS_TYPICAL, 6, [M([0, PA], [1, INC("a")], [2, list(reversed(setof([ROLE_ADMIN, ROLE_MEMBER])))], [3, 1])]), "valid")
    neg(S, "role-ids-empty", "value_range", mset(GCS_TYPICAL, 6, [M([0, PA], [1, INC("a")], [2, []], [3, 1])]))
    neg(S, "sequencing-timeout-29", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 6, 29)))
    neg(S, "sequencing-timeout-3601", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 6, 3601)))
    neg(S, "past-epoch-window-9", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 7, 9)))
    neg(S, "join-rule-4", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 0, 4)))
    neg(S, "history-policy-0", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 2, 0)))
    neg(S, "security-profile-zero", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 5, 0)))
    neg(S, "disappearing-timer-zero", "value_range", mset(GCS_TYPICAL, 9, mset(POLICY, 4, 0)))
    neg(S, "policy-missing-timeout", "missing_field", mset(GCS_TYPICAL, 9, mdel(POLICY, 6)))
    neg(S, "policy-reserved-service-policy-8", "unknown_field", mset(GCS_TYPICAL, 9, mset(POLICY, 8, 1)))
    neg(S, "policy-reserved-space-binding-9", "unknown_field", mset(GCS_TYPICAL, 9, mset(POLICY, 9, 1)))
    neg(S, "profile-name-empty", "value_range", mset(GCS_TYPICAL, 5, M([0, ""])))
    neg(S, "governance-threshold-zero", "value_range", mset(GCS_TYPICAL, 8, mset(GOV, 1, 0)))
    neg(S, "governance-principals-unsorted", "set_order", mset(GCS_TYPICAL, 8, mset(GOV_FULL, 0, list(reversed(mget(GOV_FULL, 0))))), "valid")
    neg(S, "protected-operations-unsorted", "set_order", mset(GCS_TYPICAL, 8, mset(GOV_FULL, 2, [16, 15, 2])), "valid")
    neg(S, "protected-operation-17", "value_range", mset(GCS_TYPICAL, 8, mset(GOV_FULL, 2, [17])))
    neg(S, "succession-empty", "value_range", mset(GCS_TYPICAL, 10, []))
    neg(S, "succession-65", "value_range", mset(GCS_TYPICAL, 10, [B(det(f"succ-{i}", 16)) for i in range(65)]))
    neg(S, "succession-device-15-bytes", "value_range", mset(GCS_TYPICAL, 10, [B(det("d", 15))]))
    neg(S, "invites-empty-optional-set", "value_range", mset(GCS_TYPICAL, 11, []))
    neg(S, "invites-unsorted", "set_order", mset(GCS_TYPICAL, 11, list(reversed(setof([INVITE_1, INVITE_2])))), "valid")
    neg(S, "invite-max-uses-zero", "value_range", mset(GCS_TYPICAL, 11, [mset(INVITE_1, 5, 0)]))
    neg(S, "invite-basis-3", "value_range", mset(GCS_TYPICAL, 11, [mset(INVITE_1, 3, 3)]))
    neg(S, "invite-missing-issuer-incarnation", "missing_field", mset(GCS_TYPICAL, 11, [mdel(INVITE_1, 2)]))
    neg(S, "role-definitions-empty-optional", "value_range", mset(GCS_TYPICAL, 7, []))
    neg(S, "mls-epoch-negative", "wrong_type", mset(GCS_TYPICAL, 12, -1))
    neg(S, "mls-epoch-missing", "missing_field", mdel(GCS_TYPICAL, 12))
    neg(S, "conversation-id-31-bytes", "value_range", mset(GCS_TYPICAL, 2, B(CONV[:31])))
    neg(S, "previous-hash-alg-2", "value_range", mset(GCS_TYPICAL, 4, M([0, 2], [1, B(det("x", 32))])))
    neg(S, "reserved-key-13", "unknown_field", mset(GCS_TYPICAL, 13, 1))
    neg(S, "retired-transition-binding-hash", "unknown_field", mset(GCS_TYPICAL, 14, fd("circular-binding-hash")))

    I = "GroupControlIntent"
    base = INTENTS["member-remove"]
    neg(I, "operation-0", "value_range", mset(base, 6, 0))
    neg(I, "operation-17", "value_range", mset(base, 6, 17))
    neg(I, "args-do-not-match-operation", "missing_field", mset(base, 6, 1))
    neg(I, "args-missing-target-incarnation", "missing_field", mset(base, 8, M([0, PB])))
    neg(I, "args-unknown-key", "unknown_field", mset(base, 8, M([0, PB], [1, INC("b")], [2, 1])))
    neg(I, "args-wrong-type", "wrong_type", mset(base, 8, [1, 2]))
    neg(I, "reducing-missing", "missing_field", mdel(base, 7))
    neg(I, "reducing-not-bool", "wrong_type", mset(base, 7, 1))
    neg(I, "authorization-kind-6", "value_range", mset(base, 9, auth(6)))
    neg(I, "authorization-kind-missing", "missing_field", mset(base, 9, M([1, ACS_HEAD])))
    neg(I, "authorization-unknown-key", "unknown_field", mset(base, 9, M([0, 1], [4, 1])))
    neg(I, "authorization-space-intent-not-envelope", "trailing_bytes", mset(base, 9, auth(2, k2=B(det("not-an-envelope", 20)))))
    neg(I, "intent-id-15-bytes", "value_range", mset(base, 5, B(det("i", 15))))
    neg(I, "previous-version-zero", "value_range", mset(base, 4, 0))
    neg(I, "previous-hash-missing", "missing_field", mdel(base, 3))
    neg(I, "target-incarnation-17-bytes", "value_range", mset(base, 8, M([0, PB], [1, B(det("i", 33))])))
    neg(I, "invite-join-ref-15-bytes", "value_range", mset(INTENTS["invite-link-revoke"], 8, M([0, B(det("j", 15))])))
    neg(I, "invite-uses-field-missing", "missing_field", mset(INTENTS["invite-link-issue"], 8, M([0, mdel(INVITE_1, 6)])))
    neg(I, "role-assign-unsorted", "set_order", mset(INTENTS["role-assign"], 8, M([0, PB], [1, INC("b")], [2, list(reversed(setof([ROLE_ADMIN, ROLE_MEMBER])))])), "valid")
    neg(I, "role-assign-duplicate", "set_order", mset(INTENTS["role-assign"], 8, M([0, PB], [1, INC("b")], [2, [ROLE_ADMIN, ROLE_ADMIN]])), "valid")
    neg(I, "admission-join-rule-0", "value_range", mset(INTENTS["admission-set"], 8, M([0, 0])))
    neg(I, "security-policy-timer-zero", "value_range", mset(INTENTS["security-policy-set"], 8, M([2, 0])))
    neg(I, "governance-succession-empty", "value_range", mset(INTENTS["governance-set"], 8, M([1, []])))
    neg(I, "timer-negative", "wrong_type", mset(INTENTS["disappearing-timer-set"], 8, M([0, -1])))
    neg(I, "coordinator-replace-device-15-bytes", "value_range", mset(INTENTS["coordinator-replace"], 8, M([0, B(DEV[0][:15])], [1, B(DEV[1])])))
    neg(I, "member-admit-endpoint-policy-0", "value_range", mset(INTENTS["member-admit"], 8, M([0, PC], [1, INC("c2")], [3, 0])))

    C = "GroupCommitBinding"
    base = BINDINGS["coordinator-sequenced"]
    neg(C, "binding-kind-0", "value_range", mset(base, 3, 0))
    neg(C, "binding-kind-4", "value_range", mset(base, 3, 4))
    neg(C, "resulting-epoch-zero", "value_range", mset(base, 8, 0))
    neg(C, "previous-version-zero", "value_range", mset(base, 6, 0))
    neg(C, "reducing-missing", "missing_field", mdel(base, 4))
    neg(C, "mls-commit-hash-missing", "missing_field", mdel(base, 10))
    neg(C, "resulting-hash-missing", "missing_field", mdel(base, 11))
    neg(C, "committer-15-bytes", "value_range", mset(base, 12, B(det("c", 15))))
    neg(C, "readded-empty-optional-set", "value_range", mset(BINDINGS["recovery"], 13, []))
    neg(C, "readded-unsorted", "set_order", mset(BINDINGS["recovery"], 13, list(reversed(setof([B(DEV[2]), B(DEV[3])])))), "valid")
    neg(C, "readded-duplicate", "set_order", mset(BINDINGS["recovery"], 13, [B(DEV[2]), B(DEV[2])]), "valid")
    neg(C, "intent-hash-alg-2", "value_range", mset(base, 9, M([0, 2], [1, B(det("x", 32))])))
    neg(C, "unknown-key-14", "unknown_field", mset(base, 14, 1))

    H = "GroupHistoryBundle"
    base = BUNDLE
    neg(H, "items-empty", "value_range", mset(base, 11, []))
    neg(H, "item-id-15-bytes", "value_range", mset(base, 11, [M([0, B(det("h", 15))], [4, "x"])]))
    neg(H, "item-id-missing", "missing_field", mset(base, 11, [M([4, "x"])]))
    neg(H, "item-text-empty", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [4, ""])]))
    neg(H, "item-text-4097-bytes", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [4, "a" * 4097])]))
    neg(H, "item-attachments-33", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [5, [ATT] * 33])]))
    neg(H, "item-attachments-empty-optional", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [4, "x"], [5, []])]))
    neg(H, "item-claimed-time-negative", "wrong_type", mset(base, 11, [M([0, B(det("h", 16))], [3, -1], [4, "x"])]))
    neg(H, "item-claimed-event-id-32-bytes", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [1, B(det("e", 32))], [4, "x"])]))
    neg(H, "item-reply-to-15-bytes", "value_range", mset(base, 11, [M([0, B(det("h", 16))], [4, "x"], [6, B(det("r", 15))])]))
    neg(H, "item-unknown-key", "unknown_field", mset(base, 11, [M([0, B(det("h", 16))], [4, "x"], [7, 1])]))
    neg(H, "items-1025", "value_range", mset(base, 11, [M([0, B(det(f"h{i}", 16))], [4, "x"]) for i in range(1025)]))
    neg(H, "creation-nonce-15-bytes", "value_range", mset(base, 10, B(det("n", 15))))
    neg(H, "recipient-incarnation-missing", "missing_field", mdel(base, 7))
    neg(H, "state-hash-missing", "missing_field", mdel(base, 8))
    neg(H, "bundle-id-17-bytes", "value_range", mset(base, 2, B(det("b", 17))))
    neg(H, "author-principal-kind-4", "value_range", mset(base, 4, M([0, 4], [1, B(ACC_A)])))

    # typed W1 event bodies (validated only under the W1 overlay)
    pollc = EV["poll-create"]
    bodyc = EVENT_BODY(pollc)
    negev("pin-present-not-bool", "wrong_type", mset(EV["pin-set"], 11, M([0, EID("create")], [1, 1])))
    negev("pin-missing-present", "missing_field", mset(EV["pin-set"], 11, M([0, EID("create")])))
    negev("pin-target-15-bytes", "value_range", mset(EV["pin-set"], 11, M([0, B(det("t", 15))], [1, True])))
    negev("pin-unknown-key", "unknown_field", mset(EV["pin-set"], 11, M([0, EID("create")], [1, True], [2, 1])))
    negev("poll-one-option", "value_range", mset(pollc, 11, mset(bodyc, 1, [OPT(0, "only")])))
    negev("poll-33-options", "value_range", mset(pollc, 11, mset(bodyc, 1, [OPT(i, f"o{i}") for i in range(33)])))
    negev("poll-option-id-256", "value_range", mset(pollc, 11, mset(bodyc, 1, [OPT(0, "a"), OPT(256, "b")])))
    negev("poll-option-label-empty", "value_range", mset(pollc, 11, mset(bodyc, 1, [OPT(0, "a"), OPT(1, "")])))
    negev("poll-selection-mode-3", "value_range", mset(pollc, 11, mset(bodyc, 2, 3)))
    negev("poll-results-visibility-0", "value_range", mset(pollc, 11, mset(bodyc, 4, 0)))
    negev("poll-max-selections-1", "value_range", mset(EV["poll-create-multiple"], 11, mset(EVENT_BODY(EV["poll-create-multiple"]), 3, 1)))
    negev("poll-max-selections-33", "value_range", mset(EV["poll-create-multiple"], 11, mset(EVENT_BODY(EV["poll-create-multiple"]), 3, 33)))
    negev("poll-question-not-richtext", "wrong_type", mset(pollc, 11, mset(bodyc, 0, "plain text")))
    negev("poll-question-missing", "missing_field", mset(pollc, 11, mdel(bodyc, 0)))
    negev("poll-null-option-label", "null_forbidden", mset(pollc, 11, mset(bodyc, 1, [OPT(0, "a"), M([0, 1], [1, None])])))
    negev("vote-selection-unsorted", "set_order", mset(EV["poll-vote-multiple"], 11, M([0, EID("poll-m")], [1, [9, 1]])), "valid")
    negev("vote-selection-duplicate", "set_order", mset(EV["poll-vote-multiple"], 11, M([0, EID("poll-m")], [1, [1, 1]])), "valid")
    negev("vote-option-256", "value_range", mset(EV["poll-vote"], 11, M([0, EID("poll")], [1, [256]])))
    negev("vote-33-selections", "value_range", mset(EV["poll-vote"], 11, M([0, EID("poll")], [1, list(range(33))])))
    negev("vote-selection-missing", "missing_field", mset(EV["poll-vote"], 11, M([0, EID("poll")])))
    negev("vote-target-32-bytes", "value_range", mset(EV["poll-vote"], 11, M([0, B(det("p", 32))], [1, [1]])))
    negev("close-unknown-key", "unknown_field", mset(EV["poll-close"], 11, M([0, EID("poll")], [1, 1])))
    negev("close-missing-target", "missing_field", mset(EV["poll-close"], 11, M()))
    negev("redact-reason-empty", "value_range", mset(EV["redact"], 11, M([0, EID("create")], [1, ""])))
    negev("topic-status-0", "value_range", mset(EV["topic-status"], 11, M([0, B(det("topic-1", 16))], [1, 0])))
    return g.VECTORS


def main(out):
    vs = gen()
    ids = [v["id"] for v in vs]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    Path(out).write_text(json.dumps({"vectors": vs}, indent=1) + "\n")
    pos = sum(v["expect"] == "accept" for v in vs)
    print(f"wrote {len(vs)} vectors ({pos} positive, {len(vs) - pos} negative) to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
