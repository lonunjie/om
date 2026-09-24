#!/usr/bin/env python3
"""Feasibility prototype for V-005. Not product code. Not for production.

cddl_check.py VECTORS.json BUILD_DIR V005_BIN

1. Every vector marked cddl=valid|invalid is validated against spec/cddl with two
   independent CDDL tools: the Rust `cddl` crate CLI and the Ruby `cddl` gem
   (the RFC 8610 author's reference tool). Signed objects are checked twice:
   the body against its type rule, and (positives) the envelope against
   signed-<Type>, which exercises `.cbor` nesting.
2. The Ruby tool generates examples for every W0 root; the Rust validator reads
   them in lenient mode (key order/set order not enforced, because CDDL cannot
   express them). Any rejection outside the classes CDDL cannot express is a
   CDDL/registry disagreement.
Prints a JSON summary; exit 1 on disagreement.
"""
import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROTO = HERE.parent
TOOLS = PROTO / ".tools"
ENV = dict(os.environ, GEM_HOME=str(TOOLS / "gems"))
GEN_PER_ROOT = 12
# registry rules that CDDL (RFC 8610) cannot express; generated examples may legitimately hit them
NOT_EXPRESSIBLE = {"unknown_critical_extension", "float_forbidden", "tag_forbidden", "invalid_simple", "size_limit",
                   "depth_limit", "count_limit"}


def expand(h):
    if isinstance(h, str):
        return bytes.fromhex(h)
    return b"".join(bytes.fromhex(p) if isinstance(p, str) else bytes([int(p["fill"], 16)]) * p["len"] for p in h)


def envelope_body(b: bytes) -> bytes:
    # canonical SignedEnvelope: a2 01 <bstr head> body 02 ...
    assert b[0] == 0xA2 and b[1] == 0x01
    ib = b[2]; assert ib >> 5 == 2
    ai = ib & 31
    if ai < 24: n, off = ai, 3
    else:
        w = {24: 1, 25: 2, 26: 4, 27: 8}[ai]; n = int.from_bytes(b[3:3 + w], "big"); off = 3 + w
    return b[off:off + n]


def bundle(root, path):
    subprocess.run([str(PROTO / "tools/cddl_bundle.sh"), root, str(path)], check=True)


# Known defect in the Rust `cddl` crate 0.10.7: in a `1* KEY => VALUE` map member, KEY only matches
# when it is a bare inline type; a named rule (`ext-id`) or a control operator (`tstr .size (1..128)`)
# never matches ("unexpected key"). The Rust tool therefore validates against a copy where that one
# key is inlined as bare `tstr`; the 1..128 bound is still checked by the Ruby tool and by both
# strict codecs. Recorded in the summary.
RUST_SPEC_WORKAROUNDS = [("{ 1* ext-id => ext-entry }", "{ 1* tstr => ext-entry }")]


def rust_validate(spec, files):
    text = spec.read_text()
    for old, new in RUST_SPEC_WORKAROUNDS:
        assert text.count(old) == 1, f"workaround target not found: {old}"
        text = text.replace(old, new)
    spec = spec.with_name("spec.rust.cddl")
    spec.write_text(text)
    out = {}
    for f in files:
        r = subprocess.run([str(TOOLS / "bin/cddl"), "validate", "--cddl", str(spec), "--cbor", str(f)],
                           capture_output=True, text=True)
        # cddl 0.10.7 CLI exits 0 even when validation fails; the verdict is in the log line
        log = r.stdout + r.stderr
        out[f.name] = "[ERROR]" not in log and "is successful" in log
    return out


def ruby_validate(spec, files):
    r = subprocess.run(["ruby", str(PROTO / "tools/cddl_ruby.rb"), "validate", str(spec), *map(str, files)],
                       capture_output=True, text=True, env=ENV, check=True)
    return json.loads(r.stdout)


def main(vec_path, build, v005):
    build = Path(build) / "cddl"
    if build.exists():
        subprocess.run(["rm", "-rf", str(build)], check=True)
    build.mkdir(parents=True)
    types = {t["name"]: t for t in json.loads((PROTO.parents[2] / "spec/registries/om-object-types.json").read_text())["types"]}
    vectors = json.load(open(vec_path))["vectors"]
    cases = defaultdict(list)  # root -> [(vector id, expected valid?, bytes)]
    for v in vectors:
        if v["cddl"] not in ("valid", "invalid"):
            continue
        b = expand(v["hex"]); want = v["cddl"] == "valid"
        if types[v["object"]]["signed"]:
            cases[v["object"]].append((v["id"] + "#body", want, envelope_body(b)))
            if v["expect"] == "accept":
                cases["signed-" + v["object"]].append((v["id"] + "#envelope", True, b))
        else:
            cases[v["object"]].append((v["id"], want, b))
    disagreements, n_checked = [], 0
    for root, cs in sorted(cases.items()):
        d = build / root; d.mkdir()
        spec = d / "spec.cddl"; bundle(root, spec)
        files = []
        for i, (_, _, b) in enumerate(cs):
            f = d / f"v{i:03d}.cbor"; f.write_bytes(b); files.append(f)
        rr, rb = rust_validate(spec, files), ruby_validate(spec, files)
        for (vid, want, _), f in zip(cs, files):
            n_checked += 1
            for tool, res in (("rust-cddl", rr), ("ruby-cddl", rb)):
                if res[f.name] != want:
                    disagreements.append({"vector": vid, "tool": tool, "expected_valid": want, "got_valid": res[f.name]})
    # generated examples -> Rust validator (lenient)
    gen_summary, gen_bad = {}, []
    for name, t in sorted(types.items()):
        root = ("signed-" if t["signed"] else "") + name
        d = build / ("gen-" + root); d.mkdir()
        spec = d / "spec.cddl"; bundle(root, spec)
        subprocess.run(["ruby", str(PROTO / "tools/cddl_ruby.rb"), "generate", str(spec), str(GEN_PER_ROOT), str(d)],
                       check=True, env=ENV, capture_output=True)
        files = sorted(d.glob("gen-*.cbor"))
        res = json.loads(subprocess.run([v005, "lenient", name, *map(str, files)], capture_output=True, text=True, check=True).stdout)
        acc = sum(1 for x in res.values() if x == "accept")
        classes = sorted({x for x in res.values() if x != "accept"})
        gen_summary[root] = {"generated": len(files), "accepted": acc, "rejected_classes": classes}
        for fn, x in res.items():
            if x != "accept" and x not in NOT_EXPRESSIBLE:
                gen_bad.append({"root": root, "file": fn, "class": x})
    summary = {
        "vector_checks": n_checked,
        "tools": ["cddl (Rust crate) 0.10.7", "cddl (Ruby gem) 0.12.14"],
        "rust_tool_spec_workarounds": [{"from": a, "to": b, "reason": "cddl 0.10.7 only matches bare inline key types in KEY => VALUE members"}
                                       for a, b in RUST_SPEC_WORKAROUNDS],
        "disagreements": disagreements,
        "generated_examples": gen_summary,
        "generated_unexpected_rejections": gen_bad[:20],
        "generated_unexpected_rejection_count": len(gen_bad),
    }
    print(json.dumps(summary, indent=1, sort_keys=True))
    return 0 if not disagreements and not gen_bad else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:4]))
