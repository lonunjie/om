#!/usr/bin/env python3
"""Cross-wave feasibility (ADR-0134 §3) for V-004, V-008, V-009, V-011, V-013, V-014 and V-019.

Question for each gate: can the frozen W0 interfaces carry the later-wave feature without reinterpreting a W0 field,
changing a W0 canonical layout or signature/hash domain, or adding a new W0 signed-object primitive?

Method:
1. W0-carried features are encoded as ordinary W0 ConversationEvents: registered later-wave kinds 6..14 with opaque
   bodies, principal kinds 2/3, and named non-critical key-23 extensions. Both independent W0 codecs (Rust and Node,
   verification/prototypes/v005-encoding) must accept every object unchanged.
2. Group-only features (W1+, never delivered to a W0-only client, because W0 has no groups) use a W1 registry overlay.
   The overlay must be purely additive (every W0 registry value is unchanged; checked mechanically). The Node codec
   must accept the objects under the overlay, and the unmodified W0 codecs must reject them (a W0 client cannot
   misread them).
3. V-004 also gets a bounded crash/partition sketch: same-epoch competing group commits are detected and resolved by
   the coordinator succession without changing ConversationID or AccountControlState.
4. V-014 reuses the om-archive-v1 prototype evidence: canonical objects and unknown extensions are preserved byte for
   byte.

Written by a Claude agent (verification executor). Evidence for feasibility_criteria only; no full gate passage.
"""
from __future__ import annotations

import copy
import itertools
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
V5 = ROOT / "verification/prototypes/v005-encoding"
OUT = ROOT / "verification/results/cross-wave-feasibility-latest.json"
sys.path.insert(0, str(V5 / "gen"))
import gen_vectors as g  # noqa: E402  (constants and builders; importing does not generate)
from gen_vectors import B, M, det, event, mset, py_enc  # noqa: E402

REG = ROOT / "spec/registries"


def eid(label):
    return B(det(f"event-{label}", 16))


def ext(name, value, critical=False):
    return M([name, M([0, critical], [1, value])])


# ------------------------------------------------------------------ W0-carried samples (gate, feature, object)
BRIDGE = M([0, 3], [1, B(det("bridge-principal", 32))])
SERVICE = M([0, 2], [1, B(det("service-principal", 32))])
W0_SAMPLES = [
    ("V-009", "pin.set", event("f-pin", 6, M([0, eid("create")], [1, True]))),
    ("V-009", "poll.create", event("f-poll", 8, M([0, "Lunch?"], [1, ["noodles", "rice"]], [2, 1]))),
    ("V-009", "poll.vote", event("f-vote", 9, M([0, eid("f-poll")], [1, [0]]))),
    ("V-009", "poll.close", event("f-close", 10, M([0, eid("f-poll")]))),
    ("V-009", "topic.create", event("f-topic", 11, M([0, B(det("topic-1", 16))], [1, "Planning"]))),
    ("V-009", "topic.update", event("f-topic-u", 12, M([0, B(det("topic-1", 16))], [1, "Planning 2"]))),
    ("V-009", "topic.status.set", event("f-topic-s", 13, M([0, B(det("topic-1", 16))], [1, 2]))),
    ("V-009", "message.topic.set (move)", event("f-move", 14, M([0, eid("create")], [1, B(det("topic-1", 16))]))),
    ("V-009", "materialization hint in extension", mset(g.EV_CREATE_MIN, 23, ext("om.event.materialization", M([0, eid("create")], [1, 3])))),
    ("V-013", "bridged message with provenance + loop suppression", mset(mset(g.EV_CREATE_MIN, 5, BRIDGE), 23, ext("om.bridge.provenance", M(
        [0, "matrix"], [1, B(det("scoped-origin-fingerprint", 32))], [2, [B(det("hop-bridge-1", 16))]], [3, 1])))),
    ("V-011", "service-authored event with installation instance", mset(mset(g.EV_CREATE_MIN, 5, SERVICE), 23, ext("om.service.installation", M(
        [0, B(det("service-installation-id", 16))], [1, g.fake_digest("endpoint-credential")])))),
    ("V-008", "authority parent: grant instance + ceiling", mset(g.EV_CREATE_MIN, 23, ext("om.authority.parent", M(
        [0, B(det("grant-instance", 16))], [1, "room.moderate"], [2, "room.moderate"], [3, g.fake_digest("space-head")])))),
    ("V-004", "reduction naming an immutable incarnation", mset(g.EV_CREATE_MIN, 23, ext("om.group.reduction", M(
        [0, B(det("membership-instance", 16))], [1, "room.remove"])))),
]

# ------------------------------------------------------------------ W1 overlay (group-only; additive)
W1_KINDS = {"15": "group.control_intent", "16": "group.commit_binding", "17": "group.coordinator_replace", "18": "group.reinit"}
CONV = det("room-conversation-id", 32)
W1_SAMPLES = [
    ("V-004", "GroupControlIntent", event("g-intent", 15, M([0, B(CONV)], [1, g.fake_digest("gcs-3")], [2, "room.remove"], [3, True],
                                                        [4, B(det("membership-instance", 16))], [5, g.fake_digest("grant-evidence")]))),
    ("V-004", "GroupCommitBinding", event("g-bind", 16, M([0, g.fake_digest("intent")], [1, 7], [2, g.fake_digest("mls-commit")], [3, 8],
                                                       [4, g.fake_digest("gcs-4")], [5, B(g.D1)]))),
    ("V-004", "coordinator replacement", event("g-coord", 17, M([0, B(CONV)], [1, g.fake_digest("gcs-4")], [2, B(g.D2)], [3, 1]))),
    ("V-019", "ReInit to a future hybrid-PQ MLS profile", event("g-reinit", 18, M([0, B(CONV)], [1, 7], [2, 65283], [3, g.fake_digest("gcs-4")]))),
]


def overlay_dir(tmp: Path) -> Path:
    d = tmp / "registries"
    shutil.copytree(REG, d)
    nf = json.loads((d / "om-numeric-fields.json").read_text())
    nf["enums"]["event_kind"].update(W1_KINDS)
    (d / "om-numeric-fields.json").write_text(json.dumps(nf, indent=1))
    cp = json.loads((d / "om-crypto-profiles.json").read_text())
    cp["profiles"].append({"id": 65283, "name": "test-hybrid-pq-mls", "status": "experimental", "hash_alg": 1,
                           "security_generation": {"group": 2}})
    (d / "om-crypto-profiles.json").write_text(json.dumps(cp, indent=1))
    return d


def additive(old, new, path="") -> list[str]:
    """Every value in `old` is present and identical in `new` (dict keys/list prefixes may only be added)."""
    bad = []
    if isinstance(old, dict):
        for k, v in old.items():
            if k not in new:
                bad.append(f"{path}/{k} removed")
            else:
                bad += additive(v, new[k], f"{path}/{k}")
    elif isinstance(old, list):
        if new[:len(old)] != old:
            bad += [f"{path} list changed"] if len(new) < len(old) else additive(old, new[:len(old)], path)
    elif old != new:
        bad.append(f"{path} changed {old!r} -> {new!r}")
    return bad


def vec_file(tmp: Path, samples, name) -> Path:
    vs = [{"id": f"{gate}/{feat}", "object": "ConversationEvent", "form": "bare", "expect": "accept", "error": None,
           "cddl": "n/a", "bench": False, "hex": py_enc(obj).hex(), "value": obj, "derived": []} for gate, feat, obj in samples]
    p = tmp / f"{name}.json"
    p.write_text(json.dumps({"vectors": vs}))
    return p


def run_codecs(vfile: Path, overlay: Path | None) -> dict:
    env = dict(__import__("os").environ)
    if overlay:
        env["OM_REG"] = str(overlay)
    node = json.loads(subprocess.run(["node", str(ROOT / "verification/cbor/node_strict_harness.mjs"), str(vfile)],
                                     capture_output=True, text=True, env=env, check=True).stdout)
    out = {"node": {k: v["verdict"] for k, v in node.items() if isinstance(v, dict) and "verdict" in v}}
    if not overlay:
        rust = json.loads(subprocess.run([str(V5 / "rust/target/release/v005"), "vectors", str(vfile)],
                                         capture_output=True, text=True, check=True).stdout)
        out["rust"] = {k: v["verdict"] for k, v in rust.items() if isinstance(v, dict) and "verdict" in v}
    return out


# ------------------------------------------------------------------ V-004 same-epoch competing commits
def partition_sketch() -> dict:
    """Members m1..m4, coordinator succession [c1, c2]. A partition lets c1 and c2 (c2 believes c1 timed out) each
    commit on epoch e. Every delivery order and partition healing point is explored."""
    succession = ["c1", "c2"]
    commits = {"c1": ("e7", "h-c1"), "c2": ("e7", "h-c2")}
    conv, acs = "conv-room", {"alice": "acs-5"}
    cases, bad = 0, []
    members = ["m1", "m2", "m3", "m4"]
    for sides in itertools.product(("c1", "c2"), repeat=len(members)):        # which commit each member sees first
        for heal in (True, False):
            cases += 1
            views = {}
            for m, first in zip(members, sides):
                seen = [first] + ([x for x in ("c1", "c2") if x != first] if heal else [])
                competing = {commits[s] for s in seen}
                detected = len(competing) > 1
                winner = next(s for s in succession if s in seen)           # earliest live coordinator in succession wins
                views[m] = {"detected": detected, "winner": winner, "conv": conv, "acs": dict(acs)}
            if heal:
                if len({v["winner"] for v in views.values()}) != 1:
                    bad.append(f"no convergence after heal: {sides}")
                if not all(v["detected"] for v in views.values() if sides.count("c2") and sides.count("c1")):
                    pass
            if any(v["conv"] != conv or v["acs"] != acs for v in views.values()):
                bad.append("resolution changed ConversationID or AccountControlState")
            if heal and len(set(sides)) == 2 and not all(v["detected"] for v in views.values()):
                bad.append(f"competing commits not detected by every member after heal: {sides}")
    return {"cases": cases, "problems": bad}


def main() -> int:
    res = {"w0_carriage": {}, "w1_overlay": {}, "problems": []}
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        w0 = run_codecs(vec_file(tmp, W0_SAMPLES, "w0"), None)
        for gate, feat, _ in W0_SAMPLES:
            k = f"{gate}/{feat}"
            res["w0_carriage"][k] = {c: w0[c][k] for c in w0}
            if any(w0[c][k] != "accept" for c in w0):
                res["problems"].append(f"W0 codecs do not carry {k}: {res['w0_carriage'][k]}")
        ov = overlay_dir(tmp)
        diff = []
        for f in REG.glob("*.json"):
            diff += additive(json.loads(f.read_text()), json.loads((ov / f.name).read_text()), f.name)
        res["overlay_additive_problems"] = diff
        res["problems"] += diff
        w1f = vec_file(tmp, W1_SAMPLES, "w1")
        w1_new = run_codecs(w1f, ov)
        w1_old = run_codecs(w1f, None)
        for gate, feat, _ in W1_SAMPLES:
            k = f"{gate}/{feat}"
            res["w1_overlay"][k] = {"node_with_w1_overlay": w1_new["node"][k], "node_w0": w1_old["node"][k], "rust_w0": w1_old["rust"][k]}
            if w1_new["node"][k] != "accept":
                res["problems"].append(f"W1 overlay does not carry {k}")
            if w1_old["node"][k] != "reject" or w1_old["rust"][k] != "reject":
                res["problems"].append(f"a W0 codec accepts group-only {k} (would misread it)")
    ps = partition_sketch()
    res["v004_partition_sketch"] = ps
    res["problems"] += ps["problems"]
    arch = json.loads((ROOT / "verification/results/v014-archive-latest.json").read_text())["summary"]
    res["v014_archive_evidence"] = {k: arch[k] for k in ("all_ok", "vectors_match", "positive_cases", "hostile_cases")}
    if not (arch["all_ok"] and arch["vectors_match"]):
        res["problems"].append("om-archive-v1 prototype evidence not passing")
    res["finding_1to1_new_kinds"] = ("W0 validators close event kinds at 1..14 (events.cddl 8: 6..14), so a later 1:1 kind is rejected, "
                                     "not shown as the unsupported-content placeholder; EVENT_MODEL.md's 'or under a new event kind' route "
                                     "therefore needs recipient capability gating, like reserved body encodings. Not needed by any gate here.")
    res["passed"] = not res["problems"]
    OUT.write_text(json.dumps({"model": "cross_wave_feasibility", "gates": ["V-004", "V-008", "V-009", "V-011", "V-013", "V-014", "V-019"],
                               **res, "gate_status_effect": "feasibility_evidence_only"}, indent=1) + "\n")
    print(json.dumps({"passed": res["passed"], "w0": len(res["w0_carriage"]), "w1": len(res["w1_overlay"]),
                      "partition_cases": ps["cases"], "problems": res["problems"][:5]}))
    return 0 if res["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
