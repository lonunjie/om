#!/usr/bin/env python3
"""ADR-0252: run the Python semantic checker (checks.py) over every expectation in the vector file.
Prints a JSON summary; exit 1 on any disagreement. Usage: check_vectors.py VECTORS.json
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import checks as ck  # noqa: E402

doc = json.load(open(sys.argv[1]))
bad, n = [], 0
by_id = {v["id"]: v for v in doc["vectors"]}


def expect(i, got, verdict, rule):
    global n
    n += 1
    if got != (verdict, rule):
        bad.append({"id": i, "got": list(got), "want": [verdict, rule]})


for v in doc["vectors"]:
    s = v.get("semantic")
    if not s or v["expect"] != "accept":
        continue
    if v["object"] == "ConversationEvent":
        expect(v["id"], ck.check_event(v["value"]), s["verdict"], s["rule"])
    elif v["object"] == "ContentPackRelease":
        expect(v["id"], ck.check_release(v["value"]), s["verdict"], s["rule"])
    else:
        sigs = [(x["$map"][0][1]["$bytes"], x["$map"][1][1], x["$map"][2][1]) for x in v["value"]["$map"][1][1]]
        parent = by_id[v["parent"]]["body_value"] if "parent" in v else None
        expect(v["id"], ck.check_state(v["body_value"], sigs, parent), s["verdict"], s["rule"])
for r in doc["claims"]:
    expect(r["id"], ck.check_claims(r["kind"], r["media_profile_id"], r["media_type"], r["plaintext_size"], r["width"], r["height"], r["duration_ms"]), r["expect"], r["rule"])
for r in doc["container"]:
    f = r["facts"]
    expect(r["id"], ck.check_container(r["resource_profile"], f), r["expect"], r["rule"])
for r in doc["lineage"]:
    got = ck.check_release_link(r["previous"], r["release"]) if r["kind"] == "release_link" else ck.check_endorsement(r["state"], r["release"])
    expect(r["id"], got, r["expect"], r["rule"])
for r in doc["pack_ref"]:
    expect(r["id"], ck.check_pack_ref(r["asset"], r["release"]), r["expect"], r["rule"])
for r in doc["emit_gate"]:
    expect(r["id"], ck.emit_gate(r["slot_capability"], r["media_profile_id"], r["devices"]), r["expect"], r["rule"])
print(json.dumps({"checks": n, "disagreements": bad[:20], "disagreement_count": len(bad)}, indent=1))
sys.exit(1 if bad else 0)
