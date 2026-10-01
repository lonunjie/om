#!/usr/bin/env python3
"""V-004 group wire freeze: validate the group vectors against spec/cddl (common..delivery + group.cddl) with the
two independent CDDL tools V-005 uses (Rust `cddl` 0.10.7, Ruby `cddl` 0.12.14), then feed generated examples to the
Rust codec in lenient mode under the W1 overlay.

cddl_check_group.py VECTORS.json BUILD_DIR V005_BIN OVERLAY_DIR
Prints a JSON summary; exit 1 on disagreement.
"""
import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
V5GEN = REPO / "verification/prototypes/v005-encoding/gen"
sys.path.insert(0, str(V5GEN))
import cddl_check as cc  # noqa: E402  (the V-005 helpers; importing runs nothing)

FILES = ["common", "account", "keys", "events", "discovery", "delivery", "group"]
GROUP_TYPES = ["GroupControlState", "GroupControlIntent", "GroupCommitBinding", "GroupHistoryBundle"]
ROOT_OF = {"ConversationEvent": "ConversationEventW1"}   # the W1 vectors validate against the W1 event rule


def bundle(root, path):
    text = f"root = {root}\n" + "".join((REPO / f"spec/cddl/{f}.cddl").read_text() + "\n" for f in FILES)
    Path(path).write_text(text)


def main(vec_path, build, v005, overlay, only_generated=False):
    build = Path(build) / "cddl"
    subprocess.run(["rm", "-rf", str(build)], check=True)
    build.mkdir(parents=True)
    types = {t["name"]: t for t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]}
    vectors = json.load(open(vec_path))["vectors"]
    cases = defaultdict(list)
    for v in vectors:
        if v["cddl"] not in ("valid", "invalid"):
            continue
        b = cc.expand(v["hex"]); want = v["cddl"] == "valid"
        root = ROOT_OF.get(v["object"], v["object"])
        if types[v["object"]]["signed"]:
            cases[root].append((v["id"] + "#body", want, cc.envelope_body(b)))
            if v["expect"] == "accept":
                cases["signed-" + root].append((v["id"] + "#envelope", True, b))
        else:
            cases[root].append((v["id"], want, b))
    disagreements, n_checked = [], 0
    for root, cs in sorted(cases.items()):
        if only_generated:
            break
        d = build / root; d.mkdir()
        spec = d / "spec.cddl"; bundle(root, spec)
        files = []
        for i, (_, _, b) in enumerate(cs):
            f = d / f"v{i:03d}.cbor"; f.write_bytes(b); files.append(f)
        rr, rb = cc.rust_validate(spec, files), cc.ruby_validate(spec, files)
        for (vid, want, _), f in zip(cs, files):
            n_checked += 1
            for tool, res in (("rust-cddl", rr), ("ruby-cddl", rb)):
                if res[f.name] != want:
                    disagreements.append({"vector": vid, "tool": tool, "expected_valid": want, "got_valid": res[f.name]})
    gen_summary, gen_bad = {}, []
    env = dict(os.environ, OM_REG=str(overlay))
    for name in GROUP_TYPES + ["ConversationEvent"]:
        root = ROOT_OF.get(name, name)
        r = ("signed-" + root) if types[name]["signed"] else root     # as V-005: signed objects are generated as envelopes
        d = build / ("gen-" + r); d.mkdir()
        spec = d / "spec.cddl"; bundle(r, spec)
        subprocess.run(["ruby", str(cc.PROTO / "tools/cddl_ruby.rb"), "generate", str(spec), str(cc.GEN_PER_ROOT), str(d)],
                       check=True, env=cc.ENV, capture_output=True)
        files = sorted(d.glob("gen-*.cbor"))
        res = json.loads(subprocess.run([v005, "lenient", name, *map(str, files)], capture_output=True, text=True, check=True, env=env).stdout)
        acc = sum(1 for x in res.values() if x == "accept")
        classes = sorted({x for x in res.values() if x != "accept"})
        gen_summary[r] = {"generated": len(files), "accepted": acc, "rejected_classes": classes}
        for fn, x in res.items():
            if x != "accept" and x not in cc.NOT_EXPRESSIBLE:
                gen_bad.append({"root": r, "file": fn, "class": x})
    summary = {
        "vector_checks": n_checked,
        "tools": ["cddl (Rust crate) 0.10.7", "cddl (Ruby gem) 0.12.14"],
        "rust_tool_spec_workarounds": [{"from": a, "to": b} for a, b in cc.RUST_SPEC_WORKAROUNDS],
        "disagreements": disagreements,
        "generated_examples": gen_summary,
        "generated_unexpected_rejections": gen_bad[:20],
        "generated_unexpected_rejection_count": len(gen_bad),
    }
    print(json.dumps(summary, indent=1, sort_keys=True))
    return 0 if not disagreements and not gen_bad else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:5], only_generated="--only-generated" in sys.argv))
