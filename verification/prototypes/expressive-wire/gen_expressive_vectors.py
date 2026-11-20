#!/usr/bin/env python3
"""ADR-0252 expressive-media wire vectors. Feasibility evidence, not product code.

Positive and negative vectors for ExpressiveAssetRef in its three carriers (ReactionSet key 3, inline type 4, the
om.expressive.asset.v1 message extension), ContentPackRelease (object type 45) and ContentPackControlState (type 46), plus
fact tables for the header-only claims and container budget checks, lineage, endorsement, pack_ref and the emission gate. It
reuses the V-005 generator as a module (positive bytes from the unmodified canonical encoder, negatives from the same raw
emitter). "Semantic" expectations are carried in a `semantic` field {verdict, rule} that both checkers (checks.py, the Rust
test) must reproduce.

Usage: gen_expressive_vectors.py OUT.json
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
sys.path.insert(0, str(HERE))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mset, py_enc, raw, setof  # noqa: E402
import checks as ck  # noqa: E402

for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
mget = g.mget
TEST = g.TEST_PROFILE
CAP_R, CAP_I, WEBP = ck.CAP_REACTION, ck.CAP_INLINE, "om.expressive.webp-v1"


# ------------------------------------------------------------------ asset builders
def asset(n=1, kind=1, profile=WEBP, width=512, height=512, size=100000, duration=None, alt="A waving cat", fallback=None,
          pack=None, shared=False, locations=None, repair=None, media="image/webp", filename=None):
    pairs = [[0, B(det(f"expr-blob-{n}", 32))], [1, media], [2, size],
             [3, g.fake_digest(f"expr-ct-{n}")], [4, M([0, 1], [1, B(det(f"expr-key-{n}", 32))])],
             [7, width], [8, height], [11, kind], [12, profile], [13, alt]]
    if locations: pairs.append([5, locations])
    if filename: pairs.append([6, filename])
    if duration is not None: pairs.append([9, duration])
    if repair: pairs.append([10, repair])
    if fallback is not None: pairs.append([14, fallback])
    if pack is not None: pairs.append([15, pack])
    if shared: pairs.append([16, True])
    return M(*pairs)


def pack_ref(pack_id=None, rid=None, item="cat-wave"):
    return M([0, B(pack_id or det("pack-id-1", 32))], [1, rid or g.fake_digest("release-id-1")], [2, item])


def att(n=1, size=100000, w=512, h=512, d=None, media="image/webp"):
    pairs = [[0, B(det(f"expr-blob-{n}", 32))], [1, media], [2, size], [3, g.fake_digest(f"expr-ct-{n}")],
             [4, M([0, 1], [1, B(det(f"expr-key-{n}", 32))])]]
    if w is not None: pairs.append([7, w])
    if h is not None: pairs.append([8, h])
    if d is not None: pairs.append([9, d])
    return M(*pairs)


def rich(*inlines):
    return M([0, 1], [1, [M([0, 1], [1, list(inlines)])]])


def text(t):
    return M([0, 1], [1, t])


def emoji_inline(a=None, fb=":cat:"):
    return M([0, 4], [1, fb], [6, a or asset(2, kind=2, width=128, height=128)])


def ext(value, critical=False, name=ck.EXT_ID):
    return M([name, M([0, critical], [1, value])])


def ext_value(idx=0, kind=1, profile=WEBP, alt="A waving cat", fallback=None, pack=None, shared=False):
    pairs = [[0, idx], [1, kind], [2, profile], [3, alt]]
    if fallback is not None: pairs.append([4, fallback])
    if pack is not None: pairs.append([5, pack])
    if shared: pairs.append([6, True])
    return M(*pairs)


TARGET = B(det("event-create", 16))


def react(a, token="custom:cat", present=True, caps=(CAP_R,), eid="xr"):
    body = M([0, TARGET], [1, token], [2, present], *([[3, a]] if a is not None else []))
    kw = {"k10": setof(list(caps))} if caps else {}
    return g.event(eid, 4, body, **kw)


def create(content=None, atts=(), caps=(), extension=None, eid="xc", kind=1):
    if kind == 1:
        body = M(*([[0, content]] if content else []), [1, list(atts)])
    else:
        body = M([0, TARGET], [1, content])
    kw = {}
    if caps: kw["k10"] = setof(list(caps))
    if extension is not None: kw["k23"] = extension
    return g.event(eid, kind, body, **kw)


# ------------------------------------------------------------------ events
VECTORS = g.VECTORS
SEMANTIC = {}   # vector id -> (verdict, rule)
W0 = {}         # vector id -> (verdict, error) for a W0 receiver (no overlay)


def pos_event(suffix, ev, sem, w0=("reject", None)):
    n = len(VECTORS)
    g.emit_positive("ConversationEvent", suffix, ev, None, False)
    v = VECTORS[-1]
    assert len(VECTORS) == n + 1
    verdict, rule = ck.check_event(ev)
    assert (verdict, rule) == sem, (suffix, (verdict, rule), sem)
    v["semantic"] = {"verdict": verdict, "rule": rule}
    if w0[0] is not None:
        v["w0_expect"], v["w0_error"] = w0


def neg_event(suffix, err, mutated, cddl="invalid", w0=None):
    g.emit_negative("ConversationEvent", suffix, err, raw(mutated), cddl)
    if w0:
        VECTORS[-1]["w0_expect"], VECTORS[-1]["w0_error"] = w0


MIN_REACT = react(asset(1, kind=3, width=128, height=128, size=40000, fallback="\U0001F63B"))
MIN_INLINE = create(rich(text("hi "), emoji_inline()), caps=(CAP_I,))
MIN_EXT = create(atts=[att(1)], extension=ext(ext_value()))

SLOT_W0 = ("reject", "unknown_field")
INLINE_W0 = ("reject", "value_range")
POS_EVENTS = [
    ("reaction-custom-reaction-asset", MIN_REACT, ("valid", "-"), SLOT_W0),
    ("reaction-custom-emoji-kind", react(asset(3, kind=2, width=64, height=64, size=9000, duration=1200)), ("valid", "-"), SLOT_W0),
    ("reaction-with-pack-ref", react(asset(4, kind=3, width=128, height=128, pack=pack_ref(), fallback="\U0001F63B")), ("valid", "-"), SLOT_W0),
    ("reaction-unknown-profile-placeholder", react(asset(5, kind=3, profile="org.example.future-v9", width=9000, height=9000, size=99999999)),
     ("placeholder", "unknown_profile"), SLOT_W0),
    ("reaction-retract-without-asset", react(None, present=False, caps=()), ("valid", "-"), ("accept", None)),
    ("reaction-asset-at-ceiling", react(asset(6, kind=3, width=256, height=256, size=524288, duration=3000)), ("valid", "-"), SLOT_W0),
    ("create-inline-custom-emoji", MIN_INLINE, ("valid", "-"), INLINE_W0),
    ("create-inline-emoji-in-link", create(rich(M([0, 2], [3, "https://example.org/x"], [4, [emoji_inline(asset(7, kind=2, width=256, height=256, size=524288))]])), caps=(CAP_I,)),
     ("valid", "-"), INLINE_W0),
    ("edit-inline-custom-emoji", create(rich(emoji_inline(asset(8, kind=2, width=32, height=32, size=2048, alt="x"), "x")), caps=(CAP_I,), kind=2, eid="xe"), ("valid", "-"), INLINE_W0),
    ("create-sticker-extension", MIN_EXT, ("valid", "-"), ("accept", None)),
    ("create-sticker-extension-at-ceiling", create(atts=[att(2, size=1048576, w=512, h=512, d=3000)], extension=ext(ext_value())), ("valid", "-"), ("accept", None)),
    ("create-loop-extension", create(atts=[att(3, size=6291456, w=1024, h=1024, d=10000)], extension=ext(ext_value(kind=4, alt="Dancing robot"))), ("valid", "-"), ("accept", None)),
    ("create-extension-second-attachment", create(atts=[att(4, media="image/jpeg", w=100, h=100), att(5)], extension=ext(ext_value(idx=1, pack=pack_ref(), shared=True, fallback=":cat:"))),
     ("valid", "-"), ("accept", None)),
    ("create-extension-unknown-profile-placeholder", create(atts=[att(6, size=99999999, w=4000, h=4000)], extension=ext(ext_value(profile="org.example.future-v9"))),
     ("placeholder", "unknown_profile"), ("accept", None)),
    ("create-extension-index-out-of-range-ignored", create(atts=[att(7)], extension=ext(ext_value(idx=1))), ("ignored", "ext_index"), ("accept", None)),
    ("create-extension-attachment-not-webp-ignored", create(atts=[att(8, media="image/png")], extension=ext(ext_value())), ("ignored", "media_type"), ("accept", None)),
    ("create-extension-schema-failure-ignored", create(atts=[att(9)], extension=ext(M([0, 0], [1, 9], [2, WEBP], [3, "x"]))), ("ignored", "ext_schema"), ("accept", None)),
    ("create-extension-over-ceiling-ignored", create(atts=[att(10, size=1048577)], extension=ext(ext_value())), ("ignored", "size_over"), ("accept", None)),
    ("create-extension-loop-without-duration-ignored", create(atts=[att(11, size=1000, w=64, h=64)], extension=ext(ext_value(kind=4))), ("ignored", "duration_required"), ("accept", None)),
    ("reaction-extension-not-create-ignored", g.event("xx", 4, M([0, TARGET], [1, "x"], [2, True]), k23=ext(ext_value())), ("ignored", "ext_not_create"), ("accept", None)),
]

# wire-valid, refused by a receiver (semantic rules of the header of expressive.cddl)
SEM_NEG = [
    ("reaction-present-false-with-asset", react(asset(20, kind=3, width=64, height=64), present=False), "reaction_present_false"),
    ("reaction-kind-sticker", react(asset(21, kind=1)), "reaction_kind"),
    ("reaction-kind-loop", react(asset(22, kind=4, duration=1000)), "reaction_kind"),
    ("reaction-without-capability", react(asset(23, kind=3, width=64, height=64), caps=()), "reaction_capability"),
    ("reaction-with-inline-capability-only", react(asset(24, kind=3, width=64, height=64), caps=(CAP_I,)), "reaction_capability"),
    ("reaction-media-type-png", react(asset(25, kind=3, width=64, height=64, media="image/png")), "media_type"),
    ("reaction-canvas-257", react(asset(26, kind=3, width=257, height=100)), "width_over"),
    ("reaction-height-257", react(asset(27, kind=3, width=100, height=257)), "height_over"),
    ("reaction-size-524289", react(asset(28, kind=3, width=64, height=64, size=524289)), "size_over"),
    ("reaction-duration-3001", react(asset(29, kind=3, width=64, height=64, duration=3001)), "duration_over"),
    ("reaction-alt-513-scalars", react(asset(30, kind=3, width=64, height=64, alt="a" * 513)), "alt_text_scalars"),
    ("reaction-fallback-129-scalars", react(asset(31, kind=3, width=64, height=64, fallback="b" * 129)), "fallback_text_scalars"),
    ("reaction-pack-item-129-scalars", react(asset(32, kind=3, width=64, height=64, pack=pack_ref(item="c" * 129))), "item_id_scalars"),
    ("inline-kind-sticker", create(rich(emoji_inline(asset(33, kind=1, width=64, height=64))), caps=(CAP_I,)), "inline_kind"),
    ("inline-without-capability", create(rich(emoji_inline()), caps=()), "inline_capability"),
    ("inline-with-reaction-capability-only", create(rich(emoji_inline()), caps=(CAP_R,)), "inline_capability"),
    ("inline-canvas-257", create(rich(emoji_inline(asset(34, kind=2, width=257, height=64))), caps=(CAP_I,)), "width_over"),
    ("inline-nested-bad-second-emoji", create(rich(emoji_inline(), M([0, 3], [4, [emoji_inline(asset(35, kind=2, width=300, height=64))]], [5, g.BOB])), caps=(CAP_I,)), "width_over"),
    ("extension-alt-513-scalars", create(atts=[att(36)], extension=ext(ext_value(alt="d" * 513))), "alt_text_scalars"),
    ("extension-height-513", create(atts=[att(37, h=513)], extension=ext(ext_value())), "height_over"),
    ("extension-attachment-without-dimensions", create(atts=[att(38, w=None, h=None)], extension=ext(ext_value())), "dimensions_missing"),
]
SEM_NEG_VERDICT = {"extension-alt-513-scalars": "ignored", "extension-height-513": "ignored", "extension-attachment-without-dimensions": "ignored"}


# ------------------------------------------------------------------ packs
def item(i, kind=1, w=512, h=512, size=90000, dur=None, label=None, keywords=None, emoji=None, **kw):
    a = asset(100 + i, kind=kind, width=w, height=h, size=size, duration=dur, **kw)
    pairs = [[0, f"item-{i}"], [1, kind], [2, a], [5, label or f"Item {i}"]]
    if emoji: pairs.append([3, setof(emoji)])
    if keywords: pairs.append([4, setof(keywords)])
    return M(*pairs)


PACK_ID = det("pack-id-1", 32)
REL1 = g.top("ContentPackRelease", [2, B(PACK_ID)], [3, 1], [5, "Cats"], [7, [1]], [8, [item(1)]], [9, M()])
REL1_ID = ck.release_id(REL1)
REL2 = g.top("ContentPackRelease", [2, B(PACK_ID)], [3, 2], [4, REL1_ID], [5, "Cats"], [7, [1]],
             [8, [item(1), item(2, kind=2, w=128, h=128, size=20000, dur=900, keywords=["happy", "cat"], emoji=["\U0001F431"])]], [9, M()])
REL2_ID = ck.release_id(REL2)
REL_TYPICAL = g.top("ContentPackRelease", [2, B(det("pack-id-2", 32))], [3, 1], [5, "Reaction pack with a long name"],
                    [6, "Hand drawn reactions, emoji and loops."], [7, setof([1, 2, 3, 4])],
                    [8, [item(11), item(12, kind=2, w=256, h=256, size=300000), item(13, kind=3, w=128, h=128, size=50000, dur=2000),
                         item(14, kind=4, w=1024, h=1024, size=4000000, dur=9000, label="Dancing robot"),
                         item(15, profile="org.example.future-v9", w=9999, h=9999, size=99999999)]],
                    [9, M([0, "Example Studio"], [1, ["https://example.org/packs/cats"]])],
                    [10, M([0, "CC-BY-4.0"], [1, "https://creativecommons.org/licenses/by/4.0/"], [2, g.fake_digest("license-text")])])
FORK = g.top("ContentPackRelease", [2, B(det("pack-id-fork", 32))], [3, 1], [5, "Cats, remixed"], [7, [1]],
             [8, [item(1)]], [9, M()], [11, REL2_ID])
REL_512 = g.top("ContentPackRelease", [2, B(det("pack-id-512", 32))], [3, 1], [5, "Big"], [7, [4]],
                [8, [item(i, w=64, h=64, size=2048) for i in range(512)]], [9, M()])
POS_REL = [("minimal", REL1, ("valid", "-")), ("second-release", REL2, ("valid", "-")), ("typical", REL_TYPICAL, ("valid", "-")),
           ("fork", FORK, ("valid", "-")), ("512-items", REL_512, ("valid", "-"))]


def rel_with(base, **ch):
    """base release with fields set (key -> value) or removed (value None)."""
    r = base
    for k, v in ch.items():
        k = int(k[1:])
        r = mdel(r, k) if v is None else mset(r, k, v)
    return r


def with_item(base, it, key=8):
    return mset(base, key, [it])


BAD_REL = [
    ("sequence-1-with-previous", rel_with(REL1, k4=REL1_ID), "sequence_previous"),
    ("sequence-2-without-previous", rel_with(REL2, k4=None), "sequence_previous"),
    ("forked-from-at-sequence-2", rel_with(REL2, k11=REL1_ID), "forked_from_sequence"),
    ("title-257-scalars", rel_with(REL1, k5="t" * 257), "title_scalars"),
    ("description-4097-scalars", rel_with(REL1, k6="d" * 4097), "description_scalars"),
    ("licensing-empty", rel_with(REL1, k10=M()), "licensing_empty"),
    ("item-id-129-scalars", with_item(REL1, mset(item(1), 0, "i" * 129)), "item_id_scalars"),
    ("item-id-duplicate", mset(REL1, 8, [item(1), mset(item(2), 0, "item-1")]), "item_id_duplicate"),
    ("item-kind-differs-from-asset", with_item(REL1, mset(item(1), 1, 2)), "item_kind"),
    ("asset-with-locations", with_item(REL1, item(1, locations=["https://blobs.example.net/o/1"])), "asset_forbidden_key"),
    ("asset-with-repair-capability", with_item(REL1, item(1, repair=B(det("repair", 32)))), "asset_forbidden_key"),
    ("asset-with-pack-ref", with_item(REL1, item(1, pack=pack_ref())), "asset_forbidden_key"),
    ("asset-with-shared-blob", with_item(REL1, item(1, shared=True)), "asset_forbidden_key"),
    ("label-513-scalars", with_item(REL1, item(1, label="l" * 513)), "label_scalars"),
    ("keyword-65-scalars", with_item(REL1, item(1, keywords=["k" * 65])), "keyword_scalars"),
    ("emoji-association-33-scalars", with_item(REL1, item(1, emoji=["e" * 33])), "emoji_scalars"),
    ("asset-alt-513-scalars", with_item(REL1, item(1, alt="a" * 513)), "alt_text_scalars"),
    ("asset-canvas-513", with_item(REL1, item(1, w=513)), "width_over"),
    ("asset-size-over", with_item(REL1, item(1, size=1048577)), "size_over"),
    ("asset-media-type-gif", with_item(REL1, item(1, media="image/gif")), "media_type"),
    ("loop-asset-without-duration", with_item(REL1, item(1, kind=4, w=64, h=64)), "duration_required"),
    ("loop-asset-duration-10001", with_item(REL1, item(1, kind=4, w=64, h=64, dur=10001)), "duration_over"),
    ("reaction-asset-canvas-257", with_item(REL1, item(1, kind=3, w=257, h=64)), "width_over"),
]


def unicode_boundary_ok():
    # 512 four-byte scalars = 2048 bytes: exactly at the alt_text wire bound and the scalar limit
    return g.top("ContentPackRelease", [2, B(det("pack-id-uni", 32))], [3, 1], [5, "\U0001F431" * 256], [7, [1]],
                 [8, [item(1, alt="\U0001F431" * 512, fallback="\U0001F431" * 128, label="\U0001F431" * 512)]], [9, M()])


# ------------------------------------------------------------------ control states
def ctrl(n, profile=TEST):
    pk = det(f"pack-controller-pk-{n}", 32)
    return {"n": n, "profile": profile, "pk": pk, "id": bytes.fromhex(ck.controller_key_id(profile, pk.hex())),
            "val": M([0, B(bytes.fromhex(ck.controller_key_id(profile, pk.hex())))], [1, profile], [2, B(pk)])}


C = [ctrl(i) for i in range(1, 20)]


def state(version, controllers, threshold, rel_id, rel_seq, prev=None, pack=PACK_ID, ids=None):
    cs = setof([c["val"] for c in controllers])
    pairs = [[2, B(pack)], [3, version], [5, cs], [6, threshold], [7, rel_id], [8, rel_seq]]
    if prev is not None: pairs.append([4, prev])
    return g.top("ContentPackControlState", *pairs)


def state_hash(st):
    return M([0, 1], [1, B(ck.ds_hash("OM-CONTENT-PACK-CONTROL-HASH-v1", py_enc(st)))])


def emit_state(suffix, st, signers, sem, parent=None, role=1, negative=None):
    """signers: list of controller dicts (signature signer = key_id, profile = the controller's)."""
    name = "ContentPackControlState"
    body = py_enc(st)
    sigs = [M([0, B(c["id"])], [1, c["profile"]], [2, role], [3, B(g.TEST_SIG)]) for c in signers]
    env = M([1, B(body)], [2, sigs])
    top_bytes = py_enc(env)
    assert raw(env) == top_bytes and raw(st) == body
    t = g.TYPES[name]
    d = []
    for c in signers:
        tbs = g.ds_sig(t["signed"]["label"], c["profile"], role, c["id"], body)
        d.append({"kind": "sig_input", "label": t["signed"]["label"], "role": role, "hex": tbs.hex()})
    hi = g.ds_hash(t["hash"], body)
    d.append({"kind": "hash", "label": t["hash"], "input_hex": hi.hex(), "sha256": hashlib.sha256(hi).hexdigest()})
    verdict, rule = ck.check_state(st, [(c["id"].hex(), c["profile"], role) for c in signers],
                                   parent[1] if parent else None)
    assert (verdict, rule) == sem, (suffix, (verdict, rule), sem)
    v = {"id": f"{name}/{suffix}", "object": name, "form": "signed", "expect": "accept", "error": None, "cddl": "valid",
         "bench": False, "hex": top_bytes.hex(), "value": env, "body_value": st, "derived": d,
         "semantic": {"verdict": verdict, "rule": rule}}
    if parent:
        v["parent"] = parent[0]
    VECTORS.append(v)
    return f"{name}/{suffix}"


G1 = state(1, [C[0]], 1, REL1_ID, 1)
G3 = state(1, [C[0], C[1], C[2]], 2, REL1_ID, 1)
U2 = state(2, [C[0], C[1], C[2]], 2, REL2_ID, 2, prev=state_hash(G3))
ROT3 = state(3, [C[3], C[4]], 1, REL2_ID, 2, prev=state_hash(U2))
MAX16 = state(1, C[:16], 16, REL1_ID, 1)


def gen_states():
    ids = {}
    ids["genesis"] = emit_state("genesis", G1, [C[0]], ("valid", "-"))
    ids["three-of-two"] = emit_state("genesis-three-controllers-threshold-2", G3, [C[0], C[2]], ("valid", "-"))
    emit_state("update-new-release", U2, [C[1], C[2]], ("valid", "-"), parent=(ids["three-of-two"], G3))
    ids["update"] = "ContentPackControlState/update-new-release"
    emit_state("rotation-new-controllers", ROT3, [C[0], C[1]], ("valid", "-"), parent=(ids["update"], U2))
    emit_state("sixteen-controllers", MAX16, C[:16], ("valid", "-"))
    # semantic refusals
    emit_state("sem-genesis-with-previous", rel_with(G1, k4=state_hash(G1)), [C[0]], ("invalid", "version_genesis"))
    emit_state("sem-version-2-without-previous", rel_with(U2, k4=None), [C[1], C[2]], ("invalid", "version_genesis"))
    dup = M([0, B(C[0]["id"])], [1, TEST], [2, B(det("other-pk", 32))])
    emit_state("sem-controller-key-id-twice", mset(G3, 5, setof([C[0]["val"], dup, C[1]["val"]])), [C[0], C[1]], ("invalid", "controllers_duplicate"))
    emit_state("sem-controller-public-key-twice", mset(G3, 5, setof([C[0]["val"], M([0, B(det("fake-id", 16))], [1, TEST], [2, B(C[0]["pk"])]), C[1]["val"]])), [C[0], C[1]], ("invalid", "controllers_duplicate"))
    badid = M([0, B(det("wrong-key-id", 16))], [1, TEST], [2, B(C[0]["pk"])])
    emit_state("sem-controller-key-id-not-derived", mset(G1, 5, [badid]), [C[0]], ("invalid", "controller_key_id"))
    emit_state("sem-threshold-above-controller-count", rel_with(G3, k6=4), [C[0], C[1], C[2]], ("invalid", "threshold"))
    # chain refusals (parent = three-of-two genesis, then the update)
    p = (ids["three-of-two"], G3)
    emit_state("sem-version-skips", rel_with(U2, k3=3), [C[1], C[2]], ("invalid", "version_step"), parent=p)
    emit_state("sem-pack-id-changed", state(2, [C[0], C[1], C[2]], 2, REL2_ID, 2, prev=state_hash(G3), pack=det("pack-id-other", 32)), [C[1], C[2]], ("invalid", "pack_id_changed"), parent=p)
    emit_state("sem-previous-state-hash-wrong", rel_with(U2, k4=state_hash(G1)), [C[1], C[2]], ("invalid", "previous_state_hash"), parent=p)
    emit_state("sem-release-sequence-skips", state(2, [C[0], C[1], C[2]], 2, REL2_ID, 3, prev=state_hash(G3)), [C[1], C[2]], ("invalid", "release_step"), parent=p)
    emit_state("sem-release-changes-sequence-same", state(2, [C[0], C[1], C[2]], 2, REL2_ID, 1, prev=state_hash(G3)), [C[1], C[2]], ("invalid", "release_step"), parent=p)
    emit_state("sem-release-same-id-sequence-advances", state(2, [C[0], C[1], C[2]], 2, REL1_ID, 2, prev=state_hash(G3)), [C[1], C[2]], ("invalid", "release_step"), parent=p)
    emit_state("sem-signed-below-previous-threshold", U2, [C[1]], ("invalid", "signature_threshold"), parent=p)
    emit_state("sem-signed-by-new-controllers-only", ROT3, [C[3], C[4]], ("invalid", "signer_unknown"), parent=(ids["update"], U2))
    emit_state("sem-signer-unknown", U2, [C[1], C[5]], ("invalid", "signer_unknown"), parent=p)
    emit_state("sem-signer-twice", U2, [C[1], C[1]], ("invalid", "signer_duplicate"), parent=p)
    emit_state("sem-signature-role-device", U2, [C[1], C[2]], ("invalid", "signature_role"), parent=p, role=3)
    emit_state("sem-genesis-below-own-threshold", G3, [C[0]], ("invalid", "signature_threshold"))
    emit_state("sem-genesis-signed-by-outsider", G1, [C[6]], ("invalid", "signer_unknown"))
    return ids


# ------------------------------------------------------------------ struct vectors (ExpressiveAssetRef and friends)
def struct_vectors():
    out = []

    def add(struct, suffix, expect, error, value, cddl):
        b = raw(value) if not isinstance(value, bytes) else value
        out.append({"id": f"{struct}/{suffix}", "struct": struct, "expect": expect, "error": error, "cddl": cddl, "hex": b.hex(), "note": ""})

    A = asset(200, kind=1)
    add("ExpressiveAssetRef", "typical", "accept", None, A, "valid")
    add("ExpressiveAssetRef", "full", "accept", None, asset(201, kind=1, duration=2500, fallback=":x:", pack=pack_ref(), shared=True, locations=["https://b.example.net/o/1"], repair=B(det("rc", 32)), filename="cat.webp"), "valid")
    add("ExpressiveAssetRef", "unknown-profile", "accept", None, asset(202, profile="org.example.future-v9"), "valid")
    for k, name in ((0, "blob-id"), (1, "media-type"), (2, "plaintext-size"), (3, "digest"), (4, "encryption"), (7, "width"), (8, "height"), (11, "kind"), (12, "profile"), (13, "alt-text")):
        add("ExpressiveAssetRef", f"neg/{name}-missing", "reject", "missing_field", mdel(A, k), "invalid")
    add("ExpressiveAssetRef", "neg/kind-0", "reject", "value_range", mset(A, 11, 0), "invalid")
    add("ExpressiveAssetRef", "neg/kind-5", "reject", "value_range", mset(A, 11, 5), "invalid")
    add("ExpressiveAssetRef", "neg/kind-text", "reject", "wrong_type", mset(A, 11, "sticker"), "invalid")
    add("ExpressiveAssetRef", "neg/profile-empty", "reject", "value_range", mset(A, 12, ""), "invalid")
    add("ExpressiveAssetRef", "neg/profile-65-bytes", "reject", "value_range", mset(A, 12, "p" * 65), "invalid")
    add("ExpressiveAssetRef", "neg/alt-text-empty", "reject", "value_range", mset(A, 13, ""), "invalid")
    add("ExpressiveAssetRef", "neg/alt-text-2049-bytes", "reject", "value_range", mset(A, 13, "a" * 2049), "invalid")
    add("ExpressiveAssetRef", "alt-text-2048-bytes", "accept", None, mset(A, 13, "a" * 2048), "valid")
    add("ExpressiveAssetRef", "neg/fallback-513-bytes", "reject", "value_range", mset(A, 14, "f" * 513), "invalid")
    add("ExpressiveAssetRef", "neg/width-0", "reject", "value_range", mset(A, 7, 0), "invalid")
    add("ExpressiveAssetRef", "neg/width-65536", "reject", "value_range", mset(A, 7, 65536), "invalid")
    add("ExpressiveAssetRef", "neg/plaintext-size-0", "reject", "value_range", mset(A, 2, 0), "invalid")
    add("ExpressiveAssetRef", "neg/duration-0", "reject", "value_range", mset(A, 9, 0), "invalid")
    add("ExpressiveAssetRef", "neg/shared-blob-false", "reject", "value_range", mset(A, 16, False), "invalid")
    add("ExpressiveAssetRef", "neg/unknown-key-17", "reject", "unknown_field", mset(A, 17, 1), "invalid")
    add("ExpressiveAssetRef", "neg/account-id-key", "reject", "unknown_field", mset(A, 18, B(det("acct", 32))), "invalid")
    add("ExpressiveAssetRef", "neg/pack-ref-missing-item", "reject", "missing_field", mset(A, 15, mdel(pack_ref(), 2)), "invalid")
    add("ExpressiveAssetRef", "neg/pack-ref-pack-id-31-bytes", "reject", "value_range", mset(A, 15, mset(pack_ref(), 0, B(det("p", 31)))), "invalid")
    add("ExpressiveAssetRef", "neg/pack-ref-release-alg-2", "reject", "value_range", mset(A, 15, mset(pack_ref(), 1, M([0, 2], [1, B(det("r", 32))]))), "invalid")
    add("ExpressiveAssetRef", "neg/pack-ref-item-513-bytes", "reject", "value_range", mset(A, 15, mset(pack_ref(), 2, "i" * 513)), "invalid")
    add("ExpressiveAssetRef", "neg/pack-ref-unknown-key", "reject", "unknown_field", mset(A, 15, mset(pack_ref(), 3, 1)), "invalid")
    add("ExpressiveAssetRef", "neg/null-alt-text", "reject", "null_forbidden", mset(A, 13, None), "invalid")
    add("ExpressiveAssetRef", "neg/unsorted-keys", "reject", "unsorted_keys", {"$rawmap": [A["$map"][1], A["$map"][0]] + A["$map"][2:]}, "n/a")
    add("ExpressiveAssetRef", "neg/trailing-bytes", "reject", "trailing_bytes", raw(A) + b"\x00", "n/a")
    P = pack_ref()
    add("ExpressivePackRef", "typical", "accept", None, P, "valid")
    add("ExpressivePackRef", "neg/item-id-empty", "reject", "value_range", mset(P, 2, ""), "invalid")
    add("ExpressivePackRef", "neg/release-id-missing", "reject", "missing_field", mdel(P, 1), "invalid")
    E = ext_value()
    add("ExpressiveAssetV1Extension", "typical", "accept", None, E, "valid")
    add("ExpressiveAssetV1Extension", "neg/index-32", "reject", "value_range", mset(E, 0, 32), "invalid")
    add("ExpressiveAssetV1Extension", "neg/kind-5", "reject", "value_range", mset(E, 1, 5), "invalid")
    add("ExpressiveAssetV1Extension", "neg/alt-text-missing", "reject", "missing_field", mdel(E, 3), "invalid")
    add("ExpressiveAssetV1Extension", "neg/unknown-key-7", "reject", "unknown_field", mset(E, 7, 1), "invalid")
    IE = emoji_inline()
    add("InlineCustomEmoji", "typical", "accept", None, IE, "valid")
    add("InlineCustomEmoji", "neg/fallback-missing", "reject", "missing_field", mdel(IE, 1), "invalid")
    add("InlineCustomEmoji", "neg/asset-missing", "reject", "missing_field", mdel(IE, 6), "invalid")
    add("InlineCustomEmoji", "neg/url-key", "reject", "unknown_field", mset(IE, 3, "https://x.example"), "invalid")
    add("InlineCustomEmoji", "neg/type-3", "reject", "value_range", mset(IE, 0, 3), "invalid")
    PC = M([0, B(C[0]["id"])], [1, TEST], [2, B(C[0]["pk"])])
    add("PackController", "typical", "accept", None, PC, "valid")
    add("PackController", "neg/key-id-15-bytes", "reject", "value_range", mset(PC, 0, B(det("k", 15))), "invalid")
    add("PackController", "neg/public-key-31-bytes", "reject", "value_range", mset(PC, 2, B(det("k", 31))), "invalid")
    add("PackController", "neg/profile-0", "reject", "value_range", mset(PC, 1, 0), "invalid")
    return out


# ------------------------------------------------------------------ fact tables
def claims_table():
    rows = []

    def row(i, kind, profile=WEBP, media="image/webp", size=1000, w=64, h=64, d=None):
        v, r = ck.check_claims(kind, profile, media, size, w, h, d)
        rows.append({"id": f"claims/{i}", "kind": kind, "media_profile_id": profile, "media_type": media, "plaintext_size": size,
                     "width": w, "height": h, "duration_ms": d, "expect": v, "rule": r})

    for kind, name, canvas, size, dur in ((1, "sticker", 512, 1048576, 3000), (2, "custom-emoji", 256, 524288, 3000), (3, "reaction-asset", 256, 524288, 3000), (4, "loop-animation", 1024, 6291456, 10000)):
        d = dur if kind == 4 else None
        row(f"{name}/at-ceiling", kind, size=size, w=canvas, h=canvas, d=dur)
        row(f"{name}/width-over", kind, w=canvas + 1, d=d or 1000)
        row(f"{name}/height-over", kind, h=canvas + 1, d=d or 1000)
        row(f"{name}/size-over", kind, size=size + 1, d=d or 1000)
        row(f"{name}/duration-over", kind, d=dur + 1)
        row(f"{name}/static-one-pixel", kind, size=1, w=1, h=1, d=d or (None if kind != 4 else 1))
        row(f"{name}/media-type-png", kind, media="image/png", d=d or 1000)
        row(f"{name}/media-type-uppercase", kind, media="IMAGE/WEBP", d=d or 1000)
        row(f"{name}/unknown-profile-skips-ceilings", kind, profile="om.expressive.webp-v2", size=10**9, w=65535, h=65535, d=10**9)
    row("loop/duration-required", 4)
    row("loop/duration-1ms", 4, d=1)
    row("sticker/dimensions-missing", 1, w=None, h=None)
    return rows


def container_table():
    rows = []

    def row(i, res, canvas, frames, size=1000, ok_hint=None):
        f = {"encoded_size": size, "canvas_w": canvas[0], "canvas_h": canvas[1], "frames": frames}
        v, r = ck.check_container(res, f)
        rows.append({"id": f"container/{i}", "resource_profile": res, "facts": f, "expect": v, "rule": r})

    def full(n, w, h, d):  # n full-canvas frames of d ms
        return [[0, 0, w, h, d] for _ in range(n)]

    for res, cv, maxf, dur, mx in (("sticker.v1", 512, 90, 3000, 1048576), ("custom-emoji.v1", 256, 90, 3000, 524288), ("loop-animation.v1", 1024, 300, 10000, 6291456)):
        per = dur // maxf
        row(f"{res}/frames-at-budget", res, (cv, cv), full(maxf, cv, cv, per), size=mx)
        row(f"{res}/frames-one-over", res, (cv, cv), full(maxf + 1, cv, cv, per), size=mx)
        row(f"{res}/canvas-one-over", res, (cv + 1, cv), full(2, cv + 1, cv, 100))
        row(f"{res}/encoded-size-one-over", res, (cv, cv), full(2, cv, cv, 100), size=mx + 1)
        row(f"{res}/total-pixels-exact", res, (cv, cv), full(maxf, cv, cv, per), size=mx)
        row(f"{res}/duration-one-over", res, (cv, cv), full(2, cv, cv, dur // 2 + 1))
        row(f"{res}/frame-rect-outside-canvas", res, (cv, cv), [[1, 0, cv, cv, 100], [0, 0, cv, cv, 100]])
        row(f"{res}/frame-rate-31fps", res, (8, 8), full(31, 8, 8, 32) if dur >= 1000 else [], size=1000)
        row(f"{res}/single-static-frame", res, (cv, cv), full(1, cv, cv, 0), size=1000)
        row(f"{res}/zero-canvas", res, (0, cv), full(1, 1, 1, 0))
        row(f"{res}/no-frames", res, (cv, cv), [])
        row(f"{res}/overflow-guard", res, (65535, 65535), full(3, 1, 1, 100))
    row("loop-animation.v1/static-image", "loop-animation.v1", (64, 64), full(1, 64, 64, 0))
    row("sticker.v1/rate-exactly-30fps", "sticker.v1", (8, 8), [[0, 0, 8, 8, 100] for _ in range(30)] and full(30, 8, 8, 100))
    row("sticker.v1/rate-30fps-at-limit", "sticker.v1", (8, 8), full(90, 8, 8, 33) if False else full(30, 8, 8, 1000 // 30 + 1))
    return rows


def lineage_table(ids):
    rows = []

    def link(i, prev, rel):
        v, r = ck.check_release_link(prev, rel)
        rows.append({"id": f"release-link/{i}", "kind": "release_link", "previous": prev, "release": rel, "expect": v, "rule": r})

    def endorse(i, st, rel):
        v, r = ck.check_endorsement(st, rel)
        rows.append({"id": f"endorsement/{i}", "kind": "endorsement", "state": st, "release": rel, "expect": v, "rule": r})

    link("second-follows-first", REL1, REL2)
    link("skips-a-release", REL1, g.top("ContentPackRelease", [2, B(PACK_ID)], [3, 3], [4, REL1_ID], [5, "Cats"], [7, [1]], [8, [item(1)]], [9, M()]))
    link("previous-id-is-another-release", REL1, rel_with(REL2, k4=ck.release_id(REL_TYPICAL)))
    link("previous-id-of-the-wrong-sequence", REL1, rel_with(REL2, k4=ck.release_id(REL2)))
    link("different-pack", REL1, g.top("ContentPackRelease", [2, B(det("pack-id-x", 32))], [3, 2], [4, REL1_ID], [5, "Cats"], [7, [1]], [8, [item(1)]], [9, M()]))
    link("fork-cannot-extend-old-lineage", REL2, rel_with(FORK, k3=3, k4=REL2_ID))
    endorse("state-endorses-its-release", G1, REL1)
    endorse("state-update-endorses-second", U2, REL2)
    endorse("release-id-of-another-release", rel_with(G1, k7=REL2_ID), REL1)
    endorse("sequence-differs", rel_with(G1, k8=2), REL1)
    endorse("pack-differs", state(1, [C[0]], 1, REL1_ID, 1, pack=det("pack-id-x", 32)), REL1)
    return rows


def pack_ref_table():
    rows = []
    items = [item(1)]
    A_ok = mset(mget(items[0], 2), 15, pack_ref(rid=REL1_ID, item="item-1"))   # core fields as in the release item

    def row(i, asset_v, rel):
        v, r = ck.check_pack_ref(asset_v, rel)
        rows.append({"id": f"pack-ref/{i}", "asset": asset_v, "release": rel, "expect": v, "rule": r})

    row("matches-the-release", A_ok, REL1)
    row("release-id-of-another-release", mset(A_ok, 15, pack_ref(rid=REL2_ID, item="item-1")), REL1)
    row("pack-id-differs", mset(A_ok, 15, pack_ref(pack_id=det("pack-id-x", 32), rid=REL1_ID, item="item-1")), REL1)
    row("item-not-in-release", mset(A_ok, 15, pack_ref(rid=REL1_ID, item="item-9")), REL1)
    row("asset-key-differs", mset(mset(A_ok, 4, M([0, 1], [1, B(det("other-key", 32))])), 15, pack_ref(rid=REL1_ID, item="item-1")), REL1)
    row("asset-digest-differs", mset(A_ok, 3, g.fake_digest("other-ct")), REL1)
    row("asset-kind-differs", mset(A_ok, 11, 2), REL1)
    row("fresh-blob-per-send-differs", mset(A_ok, 0, B(det("fresh-blob", 32))), REL1)
    return rows


def gate_table():
    rows = []

    def row(i, slot, profile, devices):
        v, r = ck.emit_gate(slot, profile, devices)
        rows.append({"id": f"emit-gate/{i}", "slot_capability": slot, "media_profile_id": profile, "devices": devices, "expect": v, "rule": r})

    full = [CAP_R, CAP_I, WEBP]
    row("all-devices-advertise", CAP_R, WEBP, [full, full])
    row("one-device-lacks-slot", CAP_R, WEBP, [full, [WEBP]])
    row("one-device-lacks-profile", CAP_I, WEBP, [full, [CAP_I]])
    row("inline-slot-not-reaction-slot", CAP_I, WEBP, [[CAP_R, WEBP]])
    row("no-recipient-devices", CAP_R, WEBP, [])
    row("unknown-profile-never-emitted", CAP_R, "org.example.future-v9", [full])
    return rows


# ------------------------------------------------------------------ assembly
def signed_extra_negatives(name, base):
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


def gen():
    g.VECTORS.clear()
    # --- events
    for suffix, ev, sem, w0 in POS_EVENTS:
        pos_event(suffix, ev, sem, w0)
    for suffix, ev, rule in SEM_NEG:
        verdict = SEM_NEG_VERDICT.get(suffix, "invalid")
        w0 = ("accept", None) if suffix.startswith("extension") else SLOT_W0 if suffix.startswith("reaction") else INLINE_W0
        pos_event("semantic-" + suffix, ev, (verdict, rule), w0)
    ext_ev = create(atts=[att(1)], extension=ext(ext_value()))
    noncrit = mset(MIN_EXT, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
    g.emit_positive("ConversationEvent", "noncritical-extension-preserved", noncrit, None, False)
    VECTORS[-1]["semantic"] = {"verdict": "valid", "rule": "-"}; VECTORS[-1]["w0_expect"], VECTORS[-1]["w0_error"] = "accept", None
    # generic mutations on the minimal reaction event and on the minimal extension event
    for base_name, base in (("react", MIN_REACT), ("inline", MIN_INLINE), ("ext", MIN_EXT)):
        for suffix, err, mut, cddl in g.body_mutations("ConversationEvent", base):
            if base_name != "react" and suffix not in ("unknown-critical-extension", "unknown-critical-prefix-marked-noncritical", "duplicate-key", "unsorted-keys", "nonminimal-int", "indefinite-map"):
                continue
            g.emit_negative("ConversationEvent", f"expr-{base_name}/{suffix}", err, g.wrap_top("ConversationEvent", mut), cddl)
        g.emit_negative("ConversationEvent", f"expr-{base_name}/trailing-bytes", "trailing_bytes", raw(base) + b"\x00")
        g.emit_negative("ConversationEvent", f"expr-{base_name}/truncated", "truncated", raw(base)[:-1])
    # slot negatives on the wire
    A3 = asset(40, kind=3, width=64, height=64)
    neg_event("reaction-asset-kind-5", "value_range", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [3, mset(A3, 11, 5)])), w0=("reject", "unknown_field"))
    neg_event("reaction-asset-alt-text-missing", "missing_field", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [3, mdel(A3, 13)])), w0=("reject", "unknown_field"))
    neg_event("reaction-asset-not-a-map", "wrong_type", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [3, B(b"asset")])), w0=("reject", "unknown_field"))
    neg_event("reaction-asset-null", "null_forbidden", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [3, None])), w0=("reject", "unknown_field"))
    neg_event("reaction-key-4-unassigned", "unknown_field", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [4, A3])), w0=("reject", "unknown_field"))
    neg_event("reaction-asset-unknown-key-17", "unknown_field", mset(MIN_REACT, 11, M([0, TARGET], [1, "x"], [2, True], [3, mset(A3, 17, 1)])), w0=("reject", "unknown_field"))
    inl = lambda x: mset(MIN_INLINE, 11, M([0, rich(x)], [1, []]))
    neg_event("inline-emoji-fallback-missing", "missing_field", inl(mdel(emoji_inline(), 1)), w0=("reject", "value_range"))
    neg_event("inline-emoji-asset-missing", "missing_field", inl(mdel(emoji_inline(), 6)), w0=("reject", "value_range"))
    neg_event("inline-emoji-asset-kind-0", "value_range", inl(mset(emoji_inline(), 6, mset(asset(41, kind=2), 11, 0))), w0=("reject", "value_range"))
    neg_event("inline-emoji-url-key", "unknown_field", inl(mset(emoji_inline(), 3, "https://x.example/e.webp")), w0=("reject", "value_range"))
    neg_event("inline-emoji-children-key", "unknown_field", inl(mset(emoji_inline(), 4, [])), w0=("reject", "value_range"))
    neg_event("inline-type-5", "value_range", inl(mset(emoji_inline(), 0, 5)), w0=("reject", "value_range"))
    neg_event("inline-emoji-fallback-513-bytes", "value_range", inl(mset(emoji_inline(), 1, "f" * 513)), w0=("reject", "value_range"))
    neg_event("extension-marked-critical", "value_range", mset(MIN_EXT, 23, ext(ext_value(), critical=True)), cddl="n/a")
    neg_event("extension-unknown-critical-sibling", "unknown_critical_extension", mset(MIN_EXT, 23, M([ck.EXT_ID, M([0, False], [1, ext_value()])], ["org.example.must-understand", M([0, True], [1, 1])])), cddl="n/a")
    neg_event("attachment-carries-expressive-keys-in-w0-body", "unknown_field", mset(MIN_EXT, 11, M([1, [asset(42)]])), w0=("reject", "unknown_field"))
    # --- releases
    for suffix, rel, sem in POS_REL:
        g.emit_positive("ContentPackRelease", suffix, rel, None, suffix == "typical")
        VECTORS[-1]["semantic"] = dict(zip(("verdict", "rule"), ck.check_release(rel)))
        assert (VECTORS[-1]["semantic"]["verdict"], VECTORS[-1]["semantic"]["rule"]) == sem, suffix
    uni = unicode_boundary_ok()
    g.emit_positive("ContentPackRelease", "unicode-scalar-limits-at-the-edge", uni, None, False)
    VECTORS[-1]["semantic"] = dict(zip(("verdict", "rule"), ck.check_release(uni)))
    assert VECTORS[-1]["semantic"]["verdict"] == "valid", VECTORS[-1]["semantic"]
    ext_rel = mset(REL1, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
    g.emit_positive("ContentPackRelease", "noncritical-extension-preserved", ext_rel, None, False)
    VECTORS[-1]["semantic"] = {"verdict": "valid", "rule": "-"}
    for suffix, rel, rule in BAD_REL:
        g.emit_positive("ContentPackRelease", "semantic-" + suffix, rel, None, False)
        v, r = ck.check_release(rel)
        assert (v, r) == ("invalid", rule), (suffix, v, r)
        VECTORS[-1]["semantic"] = {"verdict": v, "rule": r}
    # wire negatives
    for suffix, err, mut, cddl in g.body_mutations("ContentPackRelease", REL1):
        if suffix == "nonminimal-int":
            mut = mset(REL1, 0, {"$nonmin": mget(REL1, 0), "w": 3})
        g.emit_negative("ContentPackRelease", suffix, err, g.wrap_top("ContentPackRelease", mut), cddl)
    g.emit_negative("ContentPackRelease", "trailing-bytes", "trailing_bytes", raw(REL1) + b"\x00")
    g.emit_negative("ContentPackRelease", "truncated", "truncated", raw(REL1)[:-1])
    pad = M([0, False], [1, B(b"\x00" * 65000)])
    g.emit_negative("ContentPackRelease", "over-size-limit", "size_limit",
                    raw(mset(REL1, 23, M(*[[f"org.example.padding-{i:02d}", pad] for i in range(33)]))))

    def rneg(suffix, err, mutated, cddl="invalid"):
        g.emit_negative("ContentPackRelease", suffix, err, raw(mutated), cddl)

    for key in (2, 3, 5, 7, 8, 9):
        rneg(f"field-{key}-missing", "missing_field", mdel(REL1, key))
    rneg("time-field-key-12", "unknown_field", mset(REL1, 12, g.T0))
    rneg("reserved-key-22", "unknown_field", mset(REL1, 22, 1))
    rneg("pack-id-31-bytes", "value_range", mset(REL1, 2, B(det("p", 31))))
    rneg("sequence-0", "value_range", mset(REL1, 3, 0))
    rneg("sequence-4294967296", "value_range", mset(REL1, 3, 4294967296))
    rneg("previous-release-alg-2", "value_range", mset(REL2, 4, M([0, 2], [1, B(det("r", 32))])))
    rneg("title-empty", "value_range", mset(REL1, 5, ""))
    rneg("title-1025-bytes", "value_range", mset(REL1, 5, "t" * 1025))
    rneg("description-16385-bytes", "value_range", mset(REL1, 6, "d" * 16385))
    rneg("pack-kinds-empty", "value_range", mset(REL1, 7, []))
    rneg("pack-kinds-unsorted", "set_order", mset(REL1, 7, [2, 1]), "valid")
    rneg("pack-kinds-duplicate", "set_order", mset(REL1, 7, [1, 1]), "valid")
    rneg("pack-kinds-5-entries", "value_range", mset(REL1, 7, [1, 2, 3, 4, 4]))
    rneg("pack-kind-5", "value_range", mset(REL1, 7, [5]))
    rneg("items-empty", "value_range", mset(REL1, 8, []))
    rneg("items-513", "value_range", mset(REL1, 8, [item(i, w=8, h=8, size=100) for i in range(513)]))
    rneg("provenance-identity-claim-key-2", "unknown_field", mset(REL1, 9, M([2, B(det("account", 32))])))
    rneg("provenance-account-id-key-3", "unknown_field", mset(REL1, 9, M([3, B(det("account", 32))])))
    rneg("provenance-source-urls-9", "value_range", mset(REL1, 9, M([1, ["https://x.example"] * 9])))
    rneg("licensing-unknown-key-3", "unknown_field", mset(REL1, 10, M([0, "MIT"], [3, 1])))
    rneg("item-asset-without-width", "missing_field", mset(REL1, 8, [mset(item(1), 2, mdel(asset(300), 7))]))
    rneg("item-kind-5", "value_range", mset(REL1, 8, [mset(item(1), 1, 5)]))
    rneg("item-label-missing", "missing_field", mset(REL1, 8, [mdel(item(1), 5)]))
    rneg("item-keywords-unsorted", "set_order", mset(REL1, 8, [mset(item(1), 4, ["b", "a"])]), "valid")
    rneg("item-keywords-33", "value_range", mset(REL1, 8, [mset(item(1), 4, setof([f"k{i:02d}" for i in range(33)]))]))
    rneg("item-executable-field", "unknown_field", mset(REL1, 8, [mset(item(1), 6, "https://x.example/run.js")]))
    rneg("item-protocol-hook-field", "unknown_field", mset(REL1, 8, [mset(item(1), 7, M([0, 1]))]))
    # --- control states
    gen_states()
    for suffix, err, mut, cddl in g.body_mutations("ContentPackControlState", G1):
        if suffix == "nonminimal-int":
            mut = mset(G1, 0, {"$nonmin": mget(G1, 0), "w": 3})
        g.emit_negative("ContentPackControlState", suffix, err, g.wrap_top("ContentPackControlState", mut), cddl)
    g.emit_negative("ContentPackControlState", "trailing-bytes", "trailing_bytes", g.wrap_top("ContentPackControlState", G1) + b"\x00")
    g.emit_negative("ContentPackControlState", "truncated", "truncated", g.wrap_top("ContentPackControlState", G1)[:-1])
    big = mset(G1, 23, M(["org.example.padding", M([0, False], [1, B(b"\x00" * 131072)])]))
    g.emit_negative("ContentPackControlState", "over-size-limit", "size_limit", g.wrap_top("ContentPackControlState", big))
    signed_extra_negatives("ContentPackControlState", G1)

    def sneg(suffix, err, mutated, cddl="invalid"):
        g.emit_negative("ContentPackControlState", suffix, err, g.wrap_top("ContentPackControlState", mutated), cddl)

    for key in (2, 3, 5, 6, 7, 8):
        sneg(f"field-{key}-missing", "missing_field", mdel(G1, key))
    sneg("state-version-0", "value_range", mset(G1, 3, 0))
    sneg("controllers-empty", "value_range", mset(G1, 5, []))
    sneg("controllers-17", "value_range", mset(G1, 5, setof([c["val"] for c in C[:17]])))
    sneg("controllers-unsorted", "set_order", mset(G3, 5, list(reversed(mget(G3, 5)))), "valid")
    sneg("controllers-duplicate-element", "set_order", mset(G1, 5, [C[0]["val"], C[0]["val"]]), "valid")
    sneg("threshold-0", "value_range", mset(G1, 6, 0))
    sneg("threshold-17", "value_range", mset(G1, 6, 17))
    sneg("controller-key-id-15-bytes", "value_range", mset(G1, 5, [mset(C[0]["val"], 0, B(det("k", 15)))]))
    sneg("controller-public-key-31-bytes", "value_range", mset(G1, 5, [mset(C[0]["val"], 2, B(det("k", 31)))]))
    sneg("controller-profile-0", "value_range", mset(G1, 5, [mset(C[0]["val"], 1, 0)]))
    sneg("controller-unknown-key-3", "unknown_field", mset(G1, 5, [mset(C[0]["val"], 3, 1)]))
    sneg("controller-account-id-key", "unknown_field", mset(G1, 5, [mset(C[0]["val"], 4, B(det("account", 32)))]))
    sneg("release-id-alg-2", "value_range", mset(G1, 7, M([0, 2], [1, B(det("r", 32))])))
    sneg("release-sequence-0", "value_range", mset(G1, 8, 0))
    sneg("unknown-key-9", "unknown_field", mset(G1, 9, 1))
    sneg("previous-state-hash-alg-2", "value_range", mset(U2, 4, M([0, 2], [1, B(det("r", 32))])))
    return g.VECTORS


def main(out):
    vs = [v for v in gen() if v is not None]
    ids = [v["id"] for v in vs]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    structs = struct_vectors()
    sids = [s["id"] for s in structs]
    assert len(sids) == len(set(sids)), "duplicate struct ids"
    doc = {"vectors": vs, "structs": structs, "claims": claims_table(), "container": container_table(),
           "lineage": lineage_table(None), "pack_ref": pack_ref_table(), "emit_gate": gate_table()}
    for k in ("claims", "container", "lineage", "pack_ref", "emit_gate"):
        ids = [r["id"] for r in doc[k]]
        assert len(ids) == len(set(ids)), f"duplicate ids in {k}"
    Path(out).write_text(json.dumps(doc, indent=1) + "\n")
    pos = sum(v["expect"] == "accept" for v in vs)
    print(f"wrote {len(vs)} object vectors ({pos} positive, {len(vs) - pos} negative), {len(structs)} struct vectors, "
          f"{len(doc['claims'])} claims, {len(doc['container'])} container, {len(doc['lineage'])} lineage, "
          f"{len(doc['pack_ref'])} pack_ref, {len(doc['emit_gate'])} emit-gate rows to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
