#!/usr/bin/env python3
"""ADR-0194: validate the LanFrameV1 vectors against spec/cddl (common, lan) with the two independent CDDL tools V-005
uses (Rust `cddl` 0.10.7, Ruby `cddl` 0.12.14). The codecs check fields and sizes; CDDL checks the kind-specific combinations.

cddl_check_lan_receipt.py VECTORS.json BUILD_DIR V005_BIN REGISTRY_DIR
Prints a JSON summary; exit 1 on disagreement.
"""
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
import cddl_check as cc  # noqa: E402  (the V-005 helpers; importing runs nothing)

FILES = ["common", "lan"]


def bundle(root, path):
    text = f"root = {root}\n" + "".join((REPO / f"spec/cddl/{f}.cddl").read_text() + "\n" for f in FILES)
    Path(path).write_text(text)


def main(vec_path, build, v005, registry):
    build = Path(build) / "cddl"
    subprocess.run(["rm", "-rf", str(build)], check=True)
    build.mkdir(parents=True)
    doc = json.load(open(vec_path))
    cases = [(v["id"], v["cddl"] == "valid", cc.expand(v["hex"])) for v in doc["structs"] if v["cddl"] in ("valid", "invalid")]
    d = build / "lan-frame-v1"
    d.mkdir()
    spec = d / "spec.cddl"
    bundle("lan-frame-v1", spec)
    files = []
    for i, (_, _, b) in enumerate(cases):
        f = d / f"v{i:03d}.cbor"
        f.write_bytes(b)
        files.append(f)
    rr, rb = cc.rust_validate(spec, files), cc.ruby_validate(spec, files)
    disagreements = []
    for (vid, want, _), f in zip(cases, files):
        for tool, res in (("rust-cddl", rr), ("ruby-cddl", rb)):
            if res[f.name] != want:
                disagreements.append({"vector": vid, "tool": tool, "expected_valid": want, "got_valid": res[f.name]})
    # LanFrameV1 is a registered struct, not a top-level object, so the prototype codecs have no pass over generated examples
    # here; the product codec and the Node decoder replay the same bytes.
    summary = {
        "vector_checks": len(cases) * 2,
        "tools": ["cddl (Rust crate) 0.10.7", "cddl (Ruby gem) 0.12.14"],
        "rust_tool_spec_workarounds": [{"from": a, "to": b} for a, b in cc.RUST_SPEC_WORKAROUNDS],
        "disagreements": disagreements,
        "note": "generated examples are not fed to a codec: LanFrameV1 is a struct, not a top-level object type",
    }
    print(json.dumps(summary, indent=1, sort_keys=True))
    return 0 if not disagreements else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:5]))
