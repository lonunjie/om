#!/usr/bin/env python3
"""ADR-0252 expressive-media wire: the Python semantic checker. Feasibility evidence, not product code.

Evaluates the rules registry and CDDL cannot express (the header of spec/cddl/expressive.cddl and ADR-0252 section 3) on
diagnostic object values ({"$map": [[key, value], ...]}, {"$bytes": hex}) and on explicit fact tables. It is written from
the rules in the ADR and the registry spec/registries/om-expressive-media.json, without reading the Rust checker in
crates/om-codec/tests/expressive_vectors.rs; the two must agree on every vector.

Every function returns (verdict, rule): verdict is "valid", "invalid", "placeholder" (a wire-valid asset that renders as its
inert placeholder), "ignored" (a non-critical extension that becomes unusable; the event stays valid) or, for pack_ref,
"attributed" / "unattributed".
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/cbor"))
from python_canonical import enc as py_enc  # noqa: E402

REG = json.loads((REPO / "spec/registries/om-expressive-media.json").read_text())
KIND = {1: "sticker", 2: "custom_emoji", 3: "reaction_asset", 4: "loop_animation"}
PROFILES = {p["id"] for p in REG["media_profiles"]}
RESOURCE = {r["id"]: r for r in REG["resource_profiles"]}
LIM = REG["text_limits_scalar_values"]
CAP_REACTION = "om.expressive.reaction-asset.v1"
CAP_INLINE = "om.expressive.inline-emoji.v1"
EXT_ID = "om.expressive.asset.v1"
FORBIDDEN_IN_PACK = (5, 10, 15, 16)


def mp(v):
    return {k: x for k, x in v["$map"]}


def hexof(v):
    return v["$bytes"]


def ds_hash(label: str, body: bytes) -> bytes:
    return hashlib.sha256(py_enc([label, {"$bytes": body.hex()}])).digest()


def release_id(release_value) -> dict:
    return {"$map": [[0, 1], [1, {"$bytes": ds_hash("OM-CONTENT-PACK-RELEASE-HASH-v1", py_enc(release_value)).hex()}]]}


def controller_key_id(profile: int, public_key_hex: str) -> str:
    body = py_enc([profile, {"$bytes": public_key_hex}])
    return ds_hash("OM-CONTENT-PACK-CONTROLLER-KEY-ID-v1", body)[:16].hex()


# ---------------------------------------------------------------- asset claims (header-only)
def check_claims(kind: int, profile_id: str, media_type: str, size: int, width, height, duration):
    if profile_id not in PROFILES:
        return "placeholder", "unknown_profile"
    if media_type != "image/webp":
        return "invalid", "media_type"
    rp = RESOURCE[REG["kind_resource_profile"][KIND[kind]]]
    if size > rp["max_encoded_bytes"]:
        return "invalid", "size_over"
    if width is None or height is None:
        return "invalid", "dimensions_missing"
    if width > rp["max_canvas"]:
        return "invalid", "width_over"
    if height > rp["max_canvas"]:
        return "invalid", "height_over"
    if duration is None:
        if rp["animated"] == "required":
            return "invalid", "duration_required"
    elif duration > rp["max_duration_ms"]:
        return "invalid", "duration_over"
    return "valid", "-"


def check_texts(alt, fallback, item_id):
    if alt is not None and len(alt) > LIM["alt_text"]:
        return "invalid", "alt_text_scalars"
    if fallback is not None and len(fallback) > LIM["fallback_text"]:
        return "invalid", "fallback_text_scalars"
    if item_id is not None and len(item_id) > LIM["item_id"]:
        return "invalid", "item_id_scalars"
    return "valid", "-"


def check_asset(ref):
    m = mp(ref)
    pr = mp(m[15]) if 15 in m else {}
    v = check_texts(m[13], m.get(14), pr.get(2))
    if v[0] != "valid":
        return v
    return check_claims(m[11], m[12], m[1], m[2], m.get(7), m.get(8), m.get(9))


# ---------------------------------------------------------------- events
def walk_inlines(rich):
    out = []

    def inline(i):
        x = mp(i)
        if x[0] == 4:
            out.append(x)
        for c in x.get(4, []):
            inline(c)

    for b in mp(rich)[1]:
        bm = mp(b)
        if bm[0] in (1, 2):
            for i in bm[1]:
                inline(i)
    return out


def ext_value_ok(v):
    if not (isinstance(v, dict) and "$map" in v):
        return False
    m = mp(v)
    try:
        return (isinstance(m[0], int) and 0 <= m[0] <= 31 and m[1] in KIND and isinstance(m[2], str) and 1 <= len(m[2].encode()) <= 64
                and isinstance(m[3], str) and 1 <= len(m[3].encode()) <= 2048)
    except KeyError:
        return False


def check_event(ev):
    e = mp(ev)
    kind, body = e[8], mp(e[11])
    caps = e.get(10, [])
    if kind == 4 and 3 in body:
        if body[2] is not True:
            return "invalid", "reaction_present_false"
        if mp(body[3])[11] not in (2, 3):
            return "invalid", "reaction_kind"
        if CAP_REACTION not in caps:
            return "invalid", "reaction_capability"
        v = check_asset(body[3])
        if v[0] != "valid":
            return v
    rich = body.get(0) if kind == 1 else body.get(1) if kind == 2 else None
    if rich is not None:
        for x in walk_inlines(rich):
            if mp(x[6])[11] != 2:
                return "invalid", "inline_kind"
            if CAP_INLINE not in caps:
                return "invalid", "inline_capability"
            v = check_asset(x[6])
            if v[0] != "valid":
                return v
    ext = mp(e[23]).get(EXT_ID) if 23 in e else None
    if ext is not None:
        value = mp(ext)[1]
        if kind != 1:
            return "ignored", "ext_not_create"
        if not ext_value_ok(value):
            return "ignored", "ext_schema"
        xm = mp(value)
        atts = body[1]
        if xm[0] >= len(atts):
            return "ignored", "ext_index"
        att = mp(atts[xm[0]])
        pr = mp(xm[5]) if 5 in xm else {}
        v = check_texts(xm[3], xm.get(4), pr.get(2))
        if v[0] != "valid":
            return "ignored", v[1]
        v = check_claims(xm[1], xm[2], att[1], att[2], att.get(7), att.get(8), att.get(9))
        if v[0] == "invalid":
            return "ignored", v[1]
        return v
    return "valid", "-"


# ---------------------------------------------------------------- release
def check_release(rel):
    r = mp(rel)
    seq = r[3]
    if (seq == 1) != (4 not in r):
        return "invalid", "sequence_previous"
    if 11 in r and seq != 1:
        return "invalid", "forked_from_sequence"
    if len(r[5]) > LIM["title"]:
        return "invalid", "title_scalars"
    if 6 in r and len(r[6]) > LIM["description"]:
        return "invalid", "description_scalars"
    if 10 in r and not mp(r[10]):
        return "invalid", "licensing_empty"
    seen = set()
    for it in r[8]:
        i = mp(it)
        if len(i[0]) > LIM["item_id"]:
            return "invalid", "item_id_scalars"
        if i[0] in seen:
            return "invalid", "item_id_duplicate"
        seen.add(i[0])
        a = mp(i[2])
        if i[1] != a[11]:
            return "invalid", "item_kind"
        if any(k in a for k in FORBIDDEN_IN_PACK):
            return "invalid", "asset_forbidden_key"
        if len(i[5]) > LIM["accessibility_label"]:
            return "invalid", "label_scalars"
        if any(len(k) > LIM["keyword"] for k in i.get(4, [])):
            return "invalid", "keyword_scalars"
        if any(len(k) > LIM["emoji_association"] for k in i.get(3, [])):
            return "invalid", "emoji_scalars"
        v = check_asset(i[2])
        if v[0] == "invalid":
            return v
    return "valid", "-"


def check_release_link(prev, rel):
    p, r = mp(prev), mp(rel)
    if p[2] != r[2]:
        return "invalid", "pack_id_changed"
    if r[3] != p[3] + 1:
        return "invalid", "sequence_skip"
    if mp(r[4])[1] != mp(release_id(prev))[1]:
        return "invalid", "previous_release_mismatch"
    return "valid", "-"


# ---------------------------------------------------------------- control state
def check_state(state, sigs, parent=None):
    """state/parent: diagnostic body values; sigs: [(signer_hex, profile, role)]."""
    s = mp(state)
    if (s[3] == 1) != (4 not in s):
        return "invalid", "version_genesis"
    ctrls = [mp(c) for c in s[5]]
    ids = [hexof(c[0]) for c in ctrls]
    keys = [hexof(c[2]) for c in ctrls]
    if len(set(ids)) != len(ids) or len(set(keys)) != len(keys):
        return "invalid", "controllers_duplicate"
    if any(hexof(c[0]) != controller_key_id(c[1], hexof(c[2])) for c in ctrls):
        return "invalid", "controller_key_id"
    if s[6] > len(ctrls):
        return "invalid", "threshold"
    auth = ctrls
    threshold = s[6]
    if parent is not None:
        p = mp(parent)
        if s[3] != p[3] + 1:
            return "invalid", "version_step"
        if s[2] != p[2]:
            return "invalid", "pack_id_changed"
        if mp(s[4])[1] != {"$bytes": ds_hash("OM-CONTENT-PACK-CONTROL-HASH-v1", py_enc(parent)).hex()}:
            return "invalid", "previous_state_hash"
        same = s[7] == p[7]
        if not (s[8] == p[8] if same else s[8] == p[8] + 1):
            return "invalid", "release_step"
        auth = [mp(c) for c in p[5]]
        threshold = p[6]
    byid = {hexof(c[0]): c for c in auth}
    if any(role != 1 for _, _, role in sigs):
        return "invalid", "signature_role"
    if any(sid not in byid for sid, _, _ in sigs):
        return "invalid", "signer_unknown"
    if len({sid for sid, _, _ in sigs}) != len(sigs):
        return "invalid", "signer_duplicate"
    if any(prof != byid[sid][1] for sid, prof, _ in sigs):
        return "invalid", "signature_profile"
    if len(sigs) < threshold:
        return "invalid", "signature_threshold"
    return "valid", "-"


def check_endorsement(state, release):
    s, r = mp(state), mp(release)
    if s[2] != r[2]:
        return "invalid", "pack_id_changed"
    if mp(s[7])[1] != mp(release_id(release))[1]:
        return "invalid", "release_id_mismatch"
    if s[8] != r[3]:
        return "invalid", "release_sequence_mismatch"
    return "valid", "-"


# ---------------------------------------------------------------- pack_ref against a release
def check_pack_ref(asset, release):
    a, r = mp(asset), mp(release)
    ref = mp(a[15])
    if hexof(ref[0]) != hexof(r[2]):
        return "unattributed", "pack_id"
    if mp(ref[1])[1] != mp(release_id(release))[1]:
        return "unattributed", "release_id"
    items = {mp(i)[0]: mp(i) for i in r[8]}
    if ref[2] not in items:
        return "unattributed", "item_missing"
    ia = mp(items[ref[2]][2])
    for k in (0, 1, 2, 3, 4, 7, 8, 9, 11, 12):
        if ia.get(k) != a.get(k):
            return "unattributed", "asset_differs"
    return "attributed", "-"


# ---------------------------------------------------------------- container facts (ADR-0251 budget)
def check_container(resource_id: str, f):
    rp = RESOURCE[resource_id]
    b = REG["budget"]["profiles"][resource_id]
    cw, ch, frames = f["canvas_w"], f["canvas_h"], f["frames"]
    if cw == 0 or ch == 0 or len(frames) == 0:
        return "invalid", "header_unstated"
    if f["encoded_size"] > rp["max_encoded_bytes"]:
        return "invalid", "encoded_size"
    if cw * ch * len(frames) > REG["budget"]["multiplication_limit"]:
        return "invalid", "overflow"
    if cw > rp["max_canvas"] or ch > rp["max_canvas"]:
        return "invalid", "canvas"
    if len(frames) > b["max_frames"]:
        return "invalid", "frames"
    for x, y, w, h, _ in frames:
        if w == 0 or h == 0 or x + w > cw or y + h > ch:
            return "invalid", "frame_rect"
    if any(w * h > b["max_pixels_per_frame"] for _, _, w, h, _ in frames):
        return "invalid", "pixels_per_frame"
    if sum(w * h for _, _, w, h, _ in frames) > b["max_total_pixels"]:
        return "invalid", "total_pixels"
    total = sum(d for *_, d in frames)
    if total > rp["max_duration_ms"]:
        return "invalid", "duration"
    if len(frames) > 1 and len(frames) * 1000 > rp["max_fps"] * total:
        return "invalid", "frame_rate"
    if rp["animated"] == "required" and len(frames) < 2:
        return "invalid", "animation_required"
    return "valid", "-"


# ---------------------------------------------------------------- emission gate (ADR-0128 rule 18)
def emit_gate(slot_cap: str, profile_id: str, devices):
    if not devices:
        return "fallback", "no_recipient_devices"
    for caps in devices:
        if slot_cap not in caps or profile_id not in caps:
            return "fallback", "device_lacks_capability"
    return "emit", "-"
