#!/usr/bin/env python3
"""ADR-0189 group carriage wire vectors. Feasibility evidence, not product code.

Generates positive and negative vectors for GroupRouteRecord (32), GroupTransportFrame (33) and the relationship-lane
ConversationEvent kinds 15 to 17 (group.invite, group.accept, group.decline). It reuses the V-005 generator as a module
(positive bytes from the unmodified verification/cbor/python_canonical.py encoder, negatives from the same raw emitter)
and the V-004 group objects (GroupControlState, GroupControlIntent, GroupCommitBinding) as frame-body ingredients.

The typed body of a GroupTransportFrame is a bstr in the registry (the codec schema has no cbor-in-bstr type), so the
frame bodies are exercised twice: the frame as an object, and the body structs as nested values inside the Accept event
(GroupRelease) and as the semantic fixtures below.

Usage: gen_group_transport_vectors.py OUT.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
sys.path.insert(0, str(REPO / "verification/prototypes/v004-group-wire"))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mset, py_enc, raw, setof  # noqa: E402
import gen_group_vectors as v4  # noqa: E402  (module level only builds objects; widens g.TYPES to the W1 types)

for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
TYPES = g.TYPES
T0 = g.T0
mget = g.mget

# ------------------------------------------------------------------ route records
URI = "https://relay.bob-home.example"


def relay(n, uri=URI, prio=0, **kw):
    cap = det(f"group-route-cap-{n}", 32)
    pairs = [[0, uri], [1, B(cap)], [2, prio], [4, T0 + 30 * 86400]]
    for k, v in kw.items():
        pairs.append([int(k[1:]), v])
    return M(*sorted(pairs, key=lambda p: p[0])), cap, uri


def dkm(n, first_relay_uri, first_cap, expires=None):
    # ADR-0189 1.3: fresh per Room; route_binding is the ADR-0142 digest over the first relay entry
    return M([0, 1], [1, B(det(f"group-delivery-key-id-{n}", 16))], [2, B(det(f"group-delivery-pk-{n}", 1216))],
             [3, g.sealed_route_binding(first_relay_uri, first_cap)], [4, expires or T0 + 30 * 86400])


def route(n, dev, relays=1, expires=None, **extra):
    rs, first = [], None
    for i in range(relays):
        r, cap, uri = relay(f"{n}-{i}", uri=f"https://relay{i}.bob-home.example", prio=i)
        rs.append(r)
        first = first or (uri, cap)
    pairs = [[2, B(dev)], [3, dkm(n, *first, expires=expires)], [4, rs]]
    for k, v in extra.items():
        pairs.append([int(k[1:]), v])
    return g.top("GroupRouteRecord", *sorted(pairs, key=lambda p: p[0]))


ROUTES = {
    "typical": route("a", v4.DEV[3]),
    "four-relays": route("b", v4.DEV[3], relays=4),
    "short-lived": route("c", v4.DEV[4], expires=T0 + 3600),
}
ROUTE_MIN = ROUTES["typical"]


def route_env(r):
    env, _ = g.envelope("GroupRouteRecord", r, ["device"])
    return env


def route_bytes(r):
    return B(py_enc(route_env(r)))


# ------------------------------------------------------------------ nested signed pieces
def cred(dev):
    return g.top("DeviceCredential", [2, B(dev)], [3, B(v4.ACC_B)], [4, 1], [5, B(det(f"sign-pk-{dev.hex()}", 32))],
                 [6, B(det(f"kem-pk-{dev.hex()}", 32))], [7, 1], [8, setof(["om.msg"])], [9, T0])


def cred_bytes(dev):
    env, _ = g.envelope("DeviceCredential", cred(dev), ["device"])
    return B(py_enc(env))


HEAD = g.top("AccountControlHead", [2, B(v4.ACC_B)], [3, 1], [4, v4.fd("acs-bob")], [5, 7])
HEAD_ENV, _ = g.envelope("AccountControlHead", HEAD, ["controller"])
HEAD_BYTES = B(py_enc(HEAD_ENV))


def release(dev, route_rec=None, kp=None):
    rec = route_rec or (ROUTES["typical"] if dev == v4.DEV[3] else route(f"r-{dev.hex()[:6]}", dev))
    return M([0, B(dev)], [1, B(kp or det(f"key-package-{dev.hex()}", 600))], [2, cred_bytes(dev)], [3, route_bytes(rec)])


def evidence(dev):
    return M([0, B(dev)], [1, cred_bytes(dev)], [2, route_bytes(route(f"r-{dev.hex()[:6]}", dev))])


REL1 = release(v4.DEV[3])
REL2 = release(v4.DEV[4])
INVITE_ID = B(det("invite-id-1", 16))

# ------------------------------------------------------------------ frames
GCI_ENV, _ = g.envelope("GroupControlIntent", v4.INTENTS["member-admit"], ["device"])
GCI_LEAVE_ENV, _ = g.envelope("GroupControlIntent", v4.INTENTS["member-leave"], ["device"])
GCB_ENV, _ = g.envelope("GroupCommitBinding", v4.BINDINGS["coordinator-sequenced"], ["device"])
GCS_ENV, _ = g.envelope("GroupControlState", v4.GCS_TYPICAL, ["device"])

BODY_COMMIT = M([0, B(py_enc(GCI_ENV))], [1, B(py_enc(GCB_ENV))], [2, B(det("mls-commit", 900))], [3, [evidence(v4.DEV[3])]])
BODY_COMMIT_MIN = M([0, B(py_enc(GCI_ENV))], [1, B(py_enc(GCB_ENV))], [2, B(det("mls-commit-min", 300))])
BODY_LEAVE = M([0, B(py_enc(GCI_LEAVE_ENV))])
BODY_APP = det("mls-private-message", 700)  # frame-body-app is one MLS PrivateMessage and nothing else
BODY_WELCOME = M([0, INVITE_ID], [1, B(det("mls-welcome", 2400))], [2, B(py_enc(GCS_ENV))],
                 [3, [M([0, B(v4.DEV[0])], [1, cred_bytes(v4.DEV[0])], [2, route_bytes(route("m0", v4.DEV[0]))])]])
BODY_ADMIT = M([0, INVITE_ID], [1, [REL1]], [2, HEAD_BYTES])
BODY_ROUTE_UPDATE = M([0, [route_bytes(ROUTES["typical"])]])

KIND = {"commit": 1, "leave_request": 2, "app": 3, "welcome": 4, "admit_request": 5, "route_update": 6}
BODIES = {"commit": BODY_COMMIT, "leave_request": BODY_LEAVE, "app": BODY_APP, "welcome": BODY_WELCOME,
          "admit_request": BODY_ADMIT, "route_update": BODY_ROUTE_UPDATE}


def body_bytes(b):
    return b if isinstance(b, bytes) else py_enc(b)


def frame(kind, body):
    return g.top("GroupTransportFrame", [2, KIND[kind]], [3, B(body_bytes(body))])


FRAMES = {k: frame(k, b) for k, b in BODIES.items()}
FRAMES["commit-without-added-devices"] = frame("commit", BODY_COMMIT_MIN)
# a Welcome that also carries the signed head of every account it names (D44 ruling): optional field 4
ACCOUNT_HEAD = M([0, B(v4.ACC_B)], [1, HEAD_BYTES], [2, B(det("account-state-body", 400))],
                 [3, [B(det("credential-body-0", 300))]])
BODY_WELCOME_HEADS = M([0, INVITE_ID], [1, B(det("mls-welcome", 2400))], [2, B(py_enc(GCS_ENV))],
                       [3, [M([0, B(v4.DEV[0])], [1, cred_bytes(v4.DEV[0])], [2, route_bytes(route("m0", v4.DEV[0]))])]],
                       [4, [ACCOUNT_HEAD]])
FRAMES["welcome-with-account-heads"] = frame("welcome", BODY_WELCOME_HEADS)
FRAME_MIN = FRAMES["leave_request"]

# ------------------------------------------------------------------ relationship-lane events (kinds 15 to 17)
CAPS = ["om.group.v1"]
EV = {
    "invite": g.event("g-invite", 15, M([0, INVITE_ID], [1, "Book club"], [2, 12]), k10=CAPS),
    "invite-no-member-count": g.event("g-invite-min", 15, M([0, INVITE_ID], [1, "G"]), k10=CAPS),
    "invite-name-256-bytes": g.event("g-invite-256", 15, M([0, INVITE_ID], [1, "n" * 256], [2, 512]), k10=CAPS),
    "accept": g.event("g-accept", 16, M([0, INVITE_ID], [1, [REL1]], [2, HEAD_BYTES]), k10=CAPS),
    "accept-two-devices": g.event("g-accept-2", 16, M([0, INVITE_ID], [1, [REL1, REL2]], [2, HEAD_BYTES]), k10=CAPS),
    "decline": g.event("g-decline", 17, M([0, INVITE_ID]), k10=CAPS),
}
BODY = lambda ev: mget(ev, 11)

# semantic fixtures: wire-valid objects whose rule lives in the semantic layer (ADR-0189 section 4 negatives).
STALE = route("stale", v4.DEV[3], expires=T0 - 3600)
OTHER_DEV_ROUTE = route("other", v4.DEV[5])
SEMANTIC = {
    "stale-route-in-accept": ("ConversationEvent", g.event("s-stale", 16, M([0, INVITE_ID], [1, [release(v4.DEV[3], STALE)]], [2, HEAD_BYTES]), k10=CAPS),
                              "route_expires_at is in the past against the receiver clock: refuse the release"),
    "route-for-another-device-id": ("ConversationEvent", g.event("s-other", 16, M([0, INVITE_ID], [1, [M([0, B(v4.DEV[3])], [1, B(det("kp-x", 600))], [2, cred_bytes(v4.DEV[3])], [3, route_bytes(OTHER_DEV_ROUTE)])]], [2, HEAD_BYTES]), k10=CAPS),
                                    "the route record names device DEV[5] but the release names DEV[3]: refuse the release"),
    "invite-without-required-capabilities": ("ConversationEvent", g.event("s-nocap", 15, M([0, INVITE_ID], [1, "Book club"])),
                                             "kinds 15 to 17 must carry required_capabilities [om.group.v1]"),
    "accept-for-unknown-invite-id": ("ConversationEvent", g.event("s-unk", 16, M([0, B(det("never-issued-invite", 16))], [1, [REL1]], [2, HEAD_BYTES]), k10=CAPS),
                                     "no Invite with this invite_id was sent to this person: ignore the Accept"),
    "welcome-without-accepted-invite": ("GroupTransportFrame", frame("welcome", M([0, B(det("never-accepted-invite", 16))], [1, B(det("mls-welcome", 2400))], [2, B(py_enc(GCS_ENV))],
                                                                              [3, [M([0, B(v4.DEV[0])], [1, cred_bytes(v4.DEV[0])], [2, route_bytes(route("m0", v4.DEV[0]))])]])),
                                        "no accepted invite with this invite_id: drop the Welcome"),
    "welcome-member-route-for-another-device": ("GroupTransportFrame", frame("welcome", M([0, INVITE_ID], [1, B(det("mls-welcome", 2400))], [2, B(py_enc(GCS_ENV))],
                                                                                         [3, [M([0, B(v4.DEV[0])], [1, cred_bytes(v4.DEV[0])], [2, route_bytes(OTHER_DEV_ROUTE)])]])),
                                                "the member route record names another device than its entry: drop the Welcome"),
    "commit-frame-non-roster-sender": ("GroupTransportFrame", FRAMES["commit"],
                                       "a frame carries no sender; the MLS layer authenticates the sender, so a Commit from a device outside the roster fails MLS, not the wire"),
}

POSITIVES = [("GroupRouteRecord", k, v, ["device"], k == "typical") for k, v in ROUTES.items()]
POSITIVES += [("GroupTransportFrame", k, v, None, k == "commit") for k, v in FRAMES.items()]
POSITIVES += [("ConversationEvent", "group-" + k, v, None, False) for k, v in EV.items()]
MINIMALS = {"GroupRouteRecord": ROUTE_MIN, "GroupTransportFrame": FRAME_MIN}


def gen():
    g.VECTORS.clear()
    for p in POSITIVES:
        g.emit_positive(*p)
    for name, base in MINIMALS.items():
        ext = mset(base, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
        g.emit_positive(name, "noncritical-extension-preserved", ext, ["device"] if TYPES[name]["signed"] else None, False)

    for name, base in MINIMALS.items():
        roles = ["device"] if TYPES[name]["signed"] else None
        for suffix, err, mut, cddl in g.body_mutations(name, base):
            if suffix == "nonminimal-int":
                mut = mset(base, 0, {"$nonmin": mget(base, 0), "w": 3})
            g.emit_negative(name, suffix, err, g.wrap_top(name, mut), cddl)
        g.emit_negative(name, "trailing-bytes", "trailing_bytes", g.wrap_top(name, base) + b"\x00")
        g.emit_negative(name, "truncated", "truncated", g.wrap_top(name, base)[:-1])
        if name == "GroupRouteRecord":
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
        else:
            big = mset(base, 23, M(["org.example.padding", M([0, False], [1, B(b"\x00" * TYPES[name]["max_bytes"])])]))
            g.emit_negative(name, "over-size-limit", "size_limit", g.wrap_top(name, big))

    def neg(name, suffix, err, mutated, cddl="invalid"):
        g.emit_negative(name, suffix, err, g.wrap_top(name, mutated), cddl)

    def negev(suffix, err, ev, cddl="invalid"):
        g.emit_negative("ConversationEvent", "group-" + suffix, err, raw(ev), cddl)

    R = "GroupRouteRecord"
    base = ROUTES["typical"]
    dk = mget(base, 3)
    rs = mget(base, 4)
    neg(R, "relays-empty", "value_range", mset(base, 4, []))
    neg(R, "relays-5", "value_range", mset(base, 4, [relay(f"x{i}")[0] for i in range(5)]))
    neg(R, "relays-missing", "missing_field", mdel(base, 4))
    neg(R, "device-id-15-bytes", "value_range", mset(base, 2, B(v4.DEV[3][:15])))
    neg(R, "device-id-17-bytes", "value_range", mset(base, 2, B(det("d", 17))))
    neg(R, "device-id-missing", "missing_field", mdel(base, 2))
    neg(R, "delivery-missing", "missing_field", mdel(base, 3))
    neg(R, "delivery-wrong-type", "wrong_type", mset(base, 3, B(det("dkm", 32))))
    neg(R, "delivery-key-id-15-bytes", "value_range", mset(base, 3, mset(dk, 1, B(det("k", 15)))))
    neg(R, "delivery-public-key-profile1-32-bytes", "value_range", mset(base, 3, mset(dk, 2, B(det("pk", 32)))), "valid")
    neg(R, "delivery-public-key-profile1-1217-bytes", "value_range", mset(base, 3, mset(dk, 2, B(det("pk", 1217)))), "valid")
    neg(R, "delivery-route-binding-alg-2", "value_range", mset(base, 3, mset(dk, 3, M([0, 2], [1, B(det("rb", 32))]))))
    neg(R, "delivery-route-binding-31-bytes", "value_range", mset(base, 3, mset(dk, 3, M([0, 1], [1, B(det("rb", 31))]))))
    neg(R, "delivery-expires-at-missing", "missing_field", mset(base, 3, mdel(dk, 4)))
    neg(R, "relay-uri-empty", "value_range", mset(base, 4, [mset(rs[0], 0, "")]))
    neg(R, "relay-capability-15-bytes", "value_range", mset(base, 4, [mset(rs[0], 1, B(det("c", 15)))]))
    neg(R, "relay-priority-256", "value_range", mset(base, 4, [mset(rs[0], 2, 256)]))
    neg(R, "relay-weight-65536", "value_range", mset(base, 4, [mset(rs[0], 3, 65536)]))
    neg(R, "relay-expires-at-missing", "missing_field", mset(base, 4, [mdel(rs[0], 4)]))
    neg(R, "relay-unknown-key-7", "unknown_field", mset(base, 4, [mset(rs[0], 7, 1)]))
    neg(R, "relay-transports-unsorted", "set_order", mset(base, 4, [mset(rs[0], 5, ["https", "wss"])]), "valid")
    neg(R, "unknown-key-5", "unknown_field", mset(base, 5, 1))
    F = "GroupTransportFrame"
    base = FRAMES["leave_request"]
    neg(F, "frame-kind-0", "value_range", mset(base, 2, 0))
    neg(F, "frame-kind-7", "value_range", mset(base, 2, 7))
    neg(F, "frame-kind-missing", "missing_field", mdel(base, 2))
    neg(F, "frame-kind-wrong-type", "wrong_type", mset(base, 2, "commit"))
    neg(F, "body-empty", "value_range", mset(base, 3, B(b"")))
    neg(F, "body-missing", "missing_field", mdel(base, 3))
    neg(F, "body-wrong-type", "wrong_type", mset(base, 3, M([0, 1])))
    neg(F, "unknown-key-4", "unknown_field", mset(base, 4, 1))
    # a frame over the sealed bucket (ADR-0189 1.2 / 1.6, at most 131,068 bytes of frame): the whole object is over the
    # type's registered limit. Built from two 65,000-byte strings so that the V-005 prototype codecs, whose global string
    # limit is 65,536 bytes, reach the same object-size verdict as the product codec.
    pad = lambda n: M([0, False], [1, B(b"\x00" * 65000)])
    over = mset(mset(FRAMES["welcome"], 3, B(b"\x00" * 65000)), 23, M(["org.example.padding-a", pad(0)], ["org.example.padding-b", pad(1)]))
    g.emit_negative(F, "welcome-over-the-bucket", "size_limit", raw(over))

    # relationship-lane events
    inv = EV["invite"]
    bi = BODY(inv)
    negev("invite-id-15-bytes", "value_range", mset(inv, 11, mset(bi, 0, B(det("i", 15)))))
    negev("invite-id-missing", "missing_field", mset(inv, 11, mdel(bi, 0)))
    negev("invite-room-name-missing", "missing_field", mset(inv, 11, mdel(bi, 1)))
    negev("invite-room-name-empty", "value_range", mset(inv, 11, mset(bi, 1, "")))
    negev("invite-room-name-257-bytes", "value_range", mset(inv, 11, mset(bi, 1, "n" * 257)))
    negev("invite-member-count-0", "value_range", mset(inv, 11, mset(bi, 2, 0)))
    negev("invite-member-count-513", "value_range", mset(inv, 11, mset(bi, 2, 513)))
    negev("invite-unknown-key-3", "unknown_field", mset(inv, 11, mset(bi, 3, 1)))
    negev("invite-carries-conversation-id", "unknown_field", mset(inv, 11, mset(bi, 4, B(v4.CONV))))
    negev("invite-null-member-count", "null_forbidden", mset(inv, 11, mset(bi, 2, None)))
    acc = EV["accept"]
    ba = BODY(acc)
    negev("accept-releases-empty", "value_range", mset(acc, 11, mset(ba, 1, [])))
    negev("accept-releases-17", "value_range", mset(acc, 11, mset(ba, 1, [release(v4.DEV[3])] * 17)))
    negev("accept-releases-missing", "missing_field", mset(acc, 11, mdel(ba, 1)))
    negev("accept-account-head-missing", "missing_field", mset(acc, 11, mdel(ba, 2)))
    negev("accept-account-head-not-an-envelope", "trailing_bytes", mset(acc, 11, mset(ba, 2, B(det("not-an-envelope", 20)))))
    negev("accept-invite-id-17-bytes", "value_range", mset(acc, 11, mset(ba, 0, B(det("i", 17)))))
    rel = REL1
    negev("accept-release-device-id-15-bytes", "value_range", mset(acc, 11, mset(ba, 1, [mset(rel, 0, B(det("d", 15)))])))
    negev("accept-release-credential-missing", "missing_field", mset(acc, 11, mset(ba, 1, [mdel(rel, 2)])))
    negev("accept-release-route-missing", "missing_field", mset(acc, 11, mset(ba, 1, [mdel(rel, 3)])))
    negev("accept-release-credential-not-an-envelope", "wrong_type", mset(acc, 11, mset(ba, 1, [mset(rel, 2, B(b"\x01"))])))
    negev("accept-release-route-is-another-object-type", "wrong_object_type", mset(acc, 11, mset(ba, 1, [mset(rel, 3, cred_bytes(v4.DEV[3]))])))
    negev("accept-release-unknown-key-4", "unknown_field", mset(acc, 11, mset(ba, 1, [mset(rel, 4, 1)])))
    negev("accept-route-record-delivery-key-wrong-length", "value_range", mset(acc, 11, mset(ba, 1, [mset(rel, 3, route_bytes(mset(ROUTES["typical"], 3, mset(mget(ROUTES["typical"], 3), 2, B(det("pk", 32))))))])), "valid")
    dec = EV["decline"]
    negev("decline-invite-id-missing", "missing_field", mset(dec, 11, M()))
    negev("decline-unknown-key-1", "unknown_field", mset(dec, 11, M([0, INVITE_ID], [1, 1])))
    negev("decline-invite-id-15-bytes", "value_range", mset(dec, 11, M([0, B(det("i", 15))])))
    negev("invite-kind-with-accept-body", "wrong_type", mset(inv, 11, BODY(acc)))
    negev("accept-kind-with-decline-body", "missing_field", mset(acc, 11, BODY(dec)))
    negev("kind-18-is-not-assigned", "value_range", mset(dec, 8, 18))

    vs = g.VECTORS
    # semantic fixtures: wire-valid, listed so both codecs prove the bytes parse; the rule is not a codec rule
    for key, (name, obj, rule) in SEMANTIC.items():
        before = len(g.VECTORS)
        g.emit_positive(name, "semantic-" + key, obj, None, False)
        g.VECTORS[-1]["semantic_rule"] = rule
        assert len(g.VECTORS) == before + 1
    return vs


def main(out):
    vs = [v for v in gen() if v is not None]
    ids = [v["id"] for v in vs]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    Path(out).write_text(json.dumps({"vectors": vs}, indent=1) + "\n")
    pos = sum(v["expect"] == "accept" for v in vs)
    print(f"wrote {len(vs)} vectors ({pos} positive, {len(vs) - pos} negative) to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
