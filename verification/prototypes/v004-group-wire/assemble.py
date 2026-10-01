#!/usr/bin/env python3
"""Combine the per-step outputs of run.sh into verification/results/v004-group-wire-latest.json. Exit 1 on any disagreement.
assemble.py BUILD_DIR OUT.json"""
import hashlib
import json
import platform
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


def load(p):
    return json.loads(p.read_text()) if p.exists() and p.stat().st_size else None


def ver(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, cwd=REPO / "verification/prototypes/v005-encoding/rust").stdout.strip().splitlines()[0]
    except (OSError, IndexError):
        return None


def main(build, out):
    b = Path(build)
    vec = json.loads((b / "object-vectors.json").read_text())["vectors"]
    sem = json.loads((b / "semantic.json").read_text())["cases"]
    cmp = load(b / "compare.json") or {}
    w0cmp = load(b / "w0-compare.json") or {}
    sc = load(b / "semantic-compare.json") or {}
    cddl = load(b / "cddl.json")
    add = load(b / "additive.json") or {}
    rust_w0, node_w0 = load(b / "rust-w0.json") or {}, load(b / "node-w0.json") or {}
    kinds = Counter(c["kind"] for c in sem)
    events = [v for v in vec if v["id"].startswith(("ConversationEvent/w1-", "ConversationEvent/neg/w1-"))]
    w0 = {}
    for name, res in (("rust", rust_w0), ("node", node_w0)):
        accepted_pos = sum(1 for v in events if v["expect"] == "accept" and res.get(v["id"], {}).get("verdict") == "accept")
        accepted_neg = [v["id"] for v in events if v["expect"] == "reject" and res.get(v["id"], {}).get("verdict") == "accept"]
        w0[name] = {"w1_event_positives_accepted": accepted_pos, "w1_event_negatives_accepted_as_opaque": len(accepted_neg)}
    by_obj = Counter(v["object"] for v in vec)
    pos = sum(v["expect"] == "accept" for v in vec)
    canonical = (b / "object-vectors.json").read_bytes()
    codec_ok = bool(cmp) and all(x["mismatches"] == 0 for x in cmp.values())
    w0_ok = bool(w0cmp) and all(x["mismatches"] == 0 for x in w0cmp.values())
    sem_ok = bool(sc) and all(x["mismatches"] == 0 for k, x in sc.items() if k != "pairwise_differences") and not sc.get("pairwise_differences")
    add_ok = bool(add) and (add.get("skipped") or all(not f["problems"] for f in add["files"].values()))
    cddl_ok = None if cddl is None else (not cddl["disagreements"] and cddl["generated_unexpected_rejection_count"] == 0)
    w1_opaque_ok = all(x["w1_event_positives_accepted"] == sum(1 for v in events if v["expect"] == "accept") for x in w0.values())
    ok = codec_ok and w0_ok and sem_ok and add_ok and w1_opaque_ok and cddl_ok is not False
    res = {
        "gate": "V-004",
        "criteria": [6, 7],
        "adr": "ADR-0176",
        "prototype": "verification/prototypes/v004-group-wire (feasibility prototype, not product code)",
        "reproduce": "verification/prototypes/v004-group-wire/run.sh",
        "wire": {"cddl": "spec/cddl/group.cddl", "event_overlay": "spec/registries/om-w1-event-bodies.json",
                 "object_types": {"28": "GroupControlState", "29": "GroupControlIntent", "30": "GroupCommitBinding", "31": "GroupHistoryBundle"}},
        "vectors": {"total": len(vec), "positive": pos, "negative": len(vec) - pos, "by_object": dict(sorted(by_obj.items())),
                    "canonical_bytes": len(canonical), "canonical_sha256": hashlib.sha256(canonical).hexdigest(),
                    "file": "verification/vectors/v004-group-wire-object-vectors.json.gz"},
        "codec_agreement_w1_overlay": cmp,
        "w0_decoders_on_w1_event_vectors": w0,
        "frozen_w0_corpus_unchanged": w0cmp,
        "registry_additivity": add,
        "semantic": {"cases": len(sem), "by_kind": dict(kinds), "agreement": sc, "file": "verification/vectors/v004-group-wire-semantic.json.gz"},
        "cddl_check": cddl if cddl is not None else "not_run",
        "toolchain": {"rustc": ver("rustc", "--version"), "node": ver("node", "--version"), "ruby": ver("ruby", "--version"), "python": platform.python_version()},
        "all_checks_pass": bool(ok),
        "limits": [
            "Rust and Node strict codecs, the Python and Node semantic checkers and the generator were all written by the same agent family in one session; "
            "the codecs share no code but read the same registries (ADR-0134 section 5 independent_implementation, not independent review).",
            "Signatures are the V-005 insecure test profile (65535); no signature verification or MLS is exercised here.",
            "The semantic checkers operate on diagnostic object values; GroupHistoryBundle admission is a model of the receiver (Room view, live dedupe set, one pin reducer), not an implementation.",
            "The W0 SignedEnvelope body limit (64 KiB) bounds every signed W1 object; see ADR-0176 finding F-27.",
        ],
    }
    Path(out).write_text(json.dumps(res, indent=1) + "\n")
    print(f"wrote {out}: all_checks_pass={res['all_checks_pass']}", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:3]))
