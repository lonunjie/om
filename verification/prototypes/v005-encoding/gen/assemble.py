#!/usr/bin/env python3
"""Feasibility prototype for V-005. Not product code. Not for production.

assemble.py BUILD_DIR OUT.json
Combines the per-step outputs of run.sh into one results file. Exit 1 if any
codec, fixture or CDDL check disagrees.
"""
import json
import platform
import statistics
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]


def ver(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, cwd=Path(__file__).parent.parent / "rust").stdout.strip().splitlines()[0]
    except (OSError, IndexError):
        return None


def load(p):
    return json.loads(p.read_text()) if p.exists() and p.stat().st_size else None


def bench_summary(native, wasm):
    if not native:
        return {"status": "not_run"}
    w = {r["id"]: r for r in wasm or []}
    rows, metrics = [], ("decode_ns", "decode_validate_ns", "encode_ns", "sig_hash_inputs_ns")
    for r in native:
        row = {"id": r["id"], "bytes": r["bytes"], "native": {m: r[m] for m in metrics}}
        if r["id"] in w:
            row["wasm32_wasip1"] = {m: w[r["id"]][m] for m in metrics}
        rows.append(row)

    def agg(target):
        out = {}
        for m in metrics:
            xs = [r[target][m] for r in rows if target in r]
            if xs:
                out[m] = {"median": statistics.median(xs), "max": max(xs)}
        return out

    ratios = [r["wasm32_wasip1"]["decode_validate_ns"] / r["native"]["decode_validate_ns"]
              for r in rows if "wasm32_wasip1" in r and r["native"]["decode_validate_ns"]]
    return {
        "status": "measured",
        "method": "median of 7 calibrated batches (>=15 ms each) per object; one representative "
                  "positive vector per W0 object type (bench=true); single thread",
        "host": {"cpu": ver("sysctl", "-n", "machdep.cpu.brand_string"), "os": f"{platform.system()} {platform.release()}"},
        "native_target": ver("rustc", "-vV") and next((l.split(": ")[1] for l in subprocess.run(
            ["rustc", "-vV"], capture_output=True, text=True, cwd=Path(__file__).parent.parent / "rust").stdout.splitlines()
            if l.startswith("host: ")), None),
        "wasm_runtime": ver("wasmtime", "--version"),
        "summary_native": agg("native"),
        "summary_wasm32_wasip1": agg("wasm32_wasip1"),
        "wasm32_over_native_decode_validate": {"median": round(statistics.median(ratios), 2), "max": round(max(ratios), 2)} if ratios else None,
        "per_object": rows,
    }


def main(build, out):
    b = Path(build)
    vec = json.loads((b / "w0-object-vectors.json").read_text())["vectors"]
    fixtures = json.loads((REPO / "verification/cbor/fixtures.json").read_text())
    rust_fx = load(b / "fixtures-rust.json") or {}
    # fixtures without an "expected" field are checked against the Python/Node golden output
    golden = json.loads((REPO / "verification/results/cbor-cross-impl-latest.json").read_text())["golden_vectors"]
    fx_bad = [f["id"] for f in fixtures if rust_fx.get(f["id"]) != f.get("expected", golden[f["id"]])]
    cmp = load(b / "compare.json") or {}
    cddl = load(b / "cddl.json") or {}
    ok = (not fx_bad and cmp and all(v["mismatches"] == 0 for v in cmp.values())
          and cddl and not cddl["disagreements"] and cddl["generated_unexpected_rejection_count"] == 0)
    res = {
        "gate": "V-005",
        "related_gates": ["V-022"],
        "gate_status_effect": "evidence_only_gate_remains_open",
        "prototype": "verification/prototypes/v005-encoding (feasibility prototype, not product code)",
        "reproduce": "verification/prototypes/v005-encoding/run.sh",
        "registries": "spec/registries/*.json (status: proposed_for_freeze; freeze requires an operator ADR)",
        "cddl": "spec/cddl/*.cddl",
        "vectors": {
            "file": "verification/vectors/v005-w0-object-vectors.json.gz",
            "total": len(vec),
            "positive": sum(v["expect"] == "accept" for v in vec),
            "negative": sum(v["expect"] == "reject" for v in vec),
            "object_types": len({v["object"] for v in vec}),
        },
        "toolchain": {"rustc": ver("rustc", "--version"), "node": ver("node", "--version"), "ruby": ver("ruby", "--version"),
                      "python": platform.python_version()},
        "existing_fixtures_rust": {"fixtures": len(fixtures), "mismatches": fx_bad},
        "codec_agreement": cmp,
        "cddl_check": cddl,
        "codec_cost_v022": bench_summary(load(b / "bench-native.json"), load(b / "bench-wasm32.json")),
        "all_checks_pass": bool(ok),
        "independence_caveat": "Rust and Node codecs, the registries and the vector generator were written by the same "
                               "agent in one session; agreement shows internal consistency, not independent review "
                               "(ADR-0117 still proposed).",
    }
    Path(out).write_text(json.dumps(res, indent=1) + "\n")
    print(f"wrote {out}: all_checks_pass={res['all_checks_pass']}", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:3]))
