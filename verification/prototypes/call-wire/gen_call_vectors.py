#!/usr/bin/env python3
"""ADR-0218 call-signal wire vectors. Feasibility evidence, not product code.

Positive and negative vectors for CallSignal (object type 44, W2) and, in a separate "structs" list, for the nested
CallKeyPackage. It reuses the V-005 generator as a module (positive bytes from the unmodified canonical encoder, negatives
from the same raw emitter). "Semantic" vectors are wire-valid signals that break a per-kind cross-field rule or a
receiver-state rule (call.cddl header); they carry a semantic_rule field and are positives for the codecs.

Usage: gen_call_vectors.py OUT.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mset, raw  # noqa: E402

for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
mget = g.mget
NAME = "CallSignal"
CALL = B(det("call-session", 16))
SECRET = B(det("call-secret", 32))
SDP = B(b"v=0\r\no=- 46117317 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n")
KEYPKG = M([0, SECRET], [1, 1])


def sig(*pairs):
    return g.top(NAME, *sorted(pairs, key=lambda p: p[0]))


def base(kind, seq=0, *extra):
    return sig([2, kind], [3, CALL], [4, seq], *extra)


OFFER = base(1, 0, [5, 1], [6, SDP], [7, KEYPKG])
ANSWER = base(2, 1, [6, SDP], [9, 1])
REJECT = base(3, 1, [8, 1])
END = base(4, 7, [8, 6])
ICE = base(5, 9, [6, SDP])
MEDIA = base(6, 10, [9, 3])
POSITIVES = {
    "offer-voice": OFFER,
    "offer-video": base(1, 0, [5, 2], [6, SDP], [7, KEYPKG]),
    "answer-audio": ANSWER,
    "answer-audio-video": base(2, 1, [6, SDP], [9, 3]),
    "answer-no-media": base(2, 1, [6, SDP], [9, 0]),
    "reject-declined": REJECT,
    "reject-busy": base(3, 1, [8, 2]),
    "end-hangup": END,
    "end-answered-elsewhere": base(4, 2, [8, 3]),
    "end-timeout": base(4, 1, [8, 4]),
    "end-failed": base(4, 3, [8, 5]),
    "end-canceled": base(4, 1, [8, 7]),
    "ice-update": ICE,
    "media-update-video-on": MEDIA,
    "media-update-all-off": base(6, 11, [9, 0]),
    "seq-max": base(6, 4294967295, [9, 1]),
    "sdp-1-byte": base(5, 2, [6, B(b"v")]),
    "sdp-16384-bytes": base(5, 2, [6, B(b"a" * 16384)]),
}
MIN = END

# wire-valid, refused by a one-to-one receiver
SEMANTIC = {
    "offer-without-sdp": (base(1, 0, [5, 1], [7, KEYPKG]), "an offer needs sdp, call_kind and key_package: drop"),
    "offer-without-key-package": (base(1, 0, [5, 1], [6, SDP]), "an offer without a key_package cannot start media: drop"),
    "offer-without-call-kind": (base(1, 0, [6, SDP], [7, KEYPKG]), "an offer needs call_kind: drop"),
    "offer-with-reason": (base(1, 0, [5, 1], [6, SDP], [7, KEYPKG], [8, 1]), "an offer carries no reason: drop"),
    "offer-with-media-flags": (base(1, 0, [5, 1], [6, SDP], [7, KEYPKG], [9, 1]), "an offer carries no media_flags: drop"),
    "answer-without-media-flags": (base(2, 1, [6, SDP]), "an answer needs sdp and media_flags: drop"),
    "answer-with-key-package": (base(2, 1, [6, SDP], [9, 1], [7, KEYPKG]), "the callee never sends a call secret: drop"),
    "reject-with-reason-hangup": (base(3, 1, [8, 6]), "a reject carries declined or busy only: drop"),
    "reject-without-reason": (base(3, 1), "a reject needs a reason: drop"),
    "end-without-reason": (base(4, 1), "an end needs a reason: drop"),
    "end-with-reason-declined": (base(4, 1, [8, 1]), "an end never carries declined or busy: drop"),
    "end-with-sdp": (base(4, 1, [8, 6], [6, SDP]), "an end carries no sdp: drop"),
    "ice-update-without-sdp": (base(5, 2), "an ICE restart needs sdp: drop"),
    "media-update-without-flags": (base(6, 2), "a media_update needs media_flags: drop"),
    "participant-update-reserved": (base(7, 2), "kind 7 is reserved for group calls: a one-to-one receiver refuses it"),
    "key-epoch-reserved": (base(8, 2), "kind 8 is reserved for group calls: a one-to-one receiver refuses it"),
    "offer-seq-not-zero": (base(1, 5, [5, 1], [6, SDP], [7, KEYPKG]), "the first signal of a call has seq 0: drop"),
    "replayed-seq": (base(4, 0, [8, 6]), "a seq equal to or below the highest accepted seq of that sender in that call: drop, no effect"),
    "unknown-call-session": (sig([2, 2], [3, B(det("other-call", 16))], [4, 1], [6, SDP], [9, 1]), "no call with this id and the signal is not an offer: drop"),
}


def struct_vectors():
    out = []

    def add(suffix, expect, error, value, cddl):
        b = raw(value) if not isinstance(value, bytes) else value
        out.append({"id": f"CallKeyPackage/{suffix}", "struct": "CallKeyPackage", "expect": expect, "error": error,
                    "cddl": cddl, "hex": b.hex(), "note": ""})

    add("typical", "accept", None, KEYPKG, "valid")
    add("neg/secret-missing", "reject", "missing_field", mdel(KEYPKG, 0), "invalid")
    add("neg/suite-missing", "reject", "missing_field", mdel(KEYPKG, 1), "invalid")
    add("neg/secret-31-bytes", "reject", "value_range", mset(KEYPKG, 0, B(det("x", 31))), "invalid")
    add("neg/secret-33-bytes", "reject", "value_range", mset(KEYPKG, 0, B(det("x", 33))), "invalid")
    add("neg/suite-0", "reject", "value_range", mset(KEYPKG, 1, 0), "invalid")
    add("neg/suite-2", "reject", "value_range", mset(KEYPKG, 1, 2), "invalid")
    add("neg/unknown-key-2", "reject", "unknown_field", mset(KEYPKG, 2, 1), "invalid")
    add("neg/secret-text", "reject", "wrong_type", mset(KEYPKG, 0, "secret"), "invalid")
    add("neg/null-secret", "reject", "null_forbidden", mset(KEYPKG, 0, None), "invalid")
    add("neg/unsorted-keys", "reject", "unsorted_keys", {"$rawmap": [KEYPKG["$map"][1], KEYPKG["$map"][0]]}, "n/a")
    add("neg/trailing-bytes", "reject", "trailing_bytes", raw(KEYPKG) + b"\x00", "n/a")
    return out


def gen():
    g.VECTORS.clear()
    for k, v in POSITIVES.items():
        g.emit_positive(NAME, k, v, None, k == "offer-voice")
    ext = mset(MIN, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
    g.emit_positive(NAME, "noncritical-extension-preserved", ext, None, False)
    for suffix, err, mut, cddl in g.body_mutations(NAME, MIN):
        if suffix == "nonminimal-int":
            mut = mset(MIN, 0, {"$nonmin": mget(MIN, 0), "w": 3})
        g.emit_negative(NAME, suffix, err, g.wrap_top(NAME, mut), cddl)
    g.emit_negative(NAME, "trailing-bytes", "trailing_bytes", raw(MIN) + b"\x00")
    g.emit_negative(NAME, "truncated", "truncated", raw(MIN)[:-1])
    pad = lambda: M([0, False], [1, B(b"\x00" * 65000)])
    g.emit_negative(NAME, "over-size-limit", "size_limit", raw(mset(MIN, 23, M(["org.example.padding-a", pad()]))))

    def neg(suffix, err, mutated, cddl="invalid"):
        g.emit_negative(NAME, suffix, err, raw(mutated), cddl)

    for k, label in ((10, "group-reserved-10"), (15, "group-reserved-15"), (16, "unassigned-16"), (22, "unassigned-22"),
                     (24, "above-the-extension-key")):
        neg(f"unknown-key-{k}-{label}", "unknown_field", mset(OFFER, k, 1))
    neg("unknown-key-time-field", "unknown_field", mset(OFFER, 11, g.T0))
    neg("object-type-is-request", "wrong_object_type", mset(MIN, 0, 4))
    for key, name in ((2, "kind"), (3, "call-session-id"), (4, "seq")):
        neg(f"{name}-missing", "missing_field", mdel(MIN, key))
        neg(f"{name}-null", "null_forbidden", mset(MIN, key, None))
    neg("kind-0", "value_range", mset(MIN, 2, 0))
    neg("kind-9", "value_range", mset(MIN, 2, 9))
    neg("kind-text", "wrong_type", mset(MIN, 2, "offer"))
    neg("call-session-id-15-bytes", "value_range", mset(MIN, 3, B(det("c", 15))))
    neg("call-session-id-17-bytes", "value_range", mset(MIN, 3, B(det("c", 17))))
    neg("call-session-id-text", "wrong_type", mset(MIN, 3, "call"))
    neg("seq-4294967296", "value_range", mset(MIN, 4, 4294967296))
    neg("seq-negative", "wrong_type", mset(MIN, 4, -1))
    neg("seq-text", "wrong_type", mset(MIN, 4, "1"))
    neg("call-kind-0", "value_range", mset(OFFER, 5, 0))
    neg("call-kind-3", "value_range", mset(OFFER, 5, 3))
    neg("call-kind-null", "null_forbidden", mset(OFFER, 5, None))
    neg("sdp-empty", "value_range", mset(OFFER, 6, B(b"")))
    neg("sdp-16385-bytes", "value_range", mset(OFFER, 6, B(b"a" * 16385)))
    neg("sdp-text", "wrong_type", mset(OFFER, 6, "v=0"))
    neg("sdp-null", "null_forbidden", mset(OFFER, 6, None))
    neg("key-package-not-a-map", "wrong_type", mset(OFFER, 7, SECRET))
    neg("key-package-null", "null_forbidden", mset(OFFER, 7, None))
    neg("key-package-secret-missing", "missing_field", mset(OFFER, 7, mdel(KEYPKG, 0)))
    neg("key-package-suite-missing", "missing_field", mset(OFFER, 7, mdel(KEYPKG, 1)))
    neg("key-package-secret-31-bytes", "value_range", mset(OFFER, 7, mset(KEYPKG, 0, B(det("x", 31)))))
    neg("key-package-suite-2", "value_range", mset(OFFER, 7, mset(KEYPKG, 1, 2)))
    neg("key-package-unknown-key", "unknown_field", mset(OFFER, 7, mset(KEYPKG, 2, 1)))
    neg("reason-0", "value_range", mset(END, 8, 0))
    neg("reason-8", "value_range", mset(END, 8, 8))
    neg("reason-null", "null_forbidden", mset(END, 8, None))
    neg("media-flags-4", "value_range", mset(MEDIA, 9, 4))
    neg("media-flags-negative", "wrong_type", mset(MEDIA, 9, -1))
    neg("media-flags-null", "null_forbidden", mset(MEDIA, 9, None))
    neg("extensions-key-23-not-a-map", "wrong_type", mset(MIN, 23, [1]))

    for key, (obj, rule) in SEMANTIC.items():
        before = len(g.VECTORS)
        g.emit_positive(NAME, "semantic-" + key, obj, None, False)
        g.VECTORS[-1]["semantic_rule"] = rule
        assert len(g.VECTORS) == before + 1
    return g.VECTORS


def main(out):
    vs = [v for v in gen() if v is not None]
    ids = [v["id"] for v in vs]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    structs = struct_vectors()
    Path(out).write_text(json.dumps({"vectors": vs, "structs": structs}, indent=1) + "\n")
    pos = sum(v["expect"] == "accept" for v in vs)
    print(f"wrote {len(vs)} vectors ({pos} positive, {len(vs) - pos} negative) and {len(structs)} struct vectors to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
