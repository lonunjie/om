#!/usr/bin/env python3
"""ADR-0168 V-005 independent-implementation evidence for the V1 presentation extensions.

Builds the clean-room Rust implementation in verification/prototypes/v005-extensions-rust (written from
MESSENGER_BASELINE_V1 and the registries by a different author than the Python/Node pair) and requires it to agree with
every expected verdict/error in the ADR-0148 voice/location corpus and the ADR-0150 rich-list corpus, and to produce the
same deterministic-CBOR extension-entry bytes as the Python canonical encoder. Needs cargo (Rust-dependent profile).
"""
from __future__ import annotations
import json, os, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "verification/prototypes/v005-extensions-rust"
OUT = ROOT / "verification/results/extensions-independent-rust-latest.json"
V148 = ROOT / "verification/vectors/adr0148-v1-extension-vectors.json"
V150 = ROOT / "verification/vectors/adr0150-rich-list-vectors.json"
REG = ROOT / "spec/registries/om-numeric-fields.json"
sys.path.insert(0, str(ROOT / "verification/bounded")); sys.path.insert(0, str(ROOT / "verification/cbor"))
from python_canonical import enc  # noqa: E402
import extensions_adr0148 as ex148  # noqa: E402
import rich_list_adr0150 as rl150  # noqa: E402


def main() -> int:
    env = dict(os.environ, PATH=f"/opt/homebrew/opt/rustup/bin:{Path.home()}/.cargo/bin:" + os.environ.get("PATH", ""))
    b = subprocess.run(["cargo", "build", "--release", "-q", "--offline"], cwd=CRATE, env=env, text=True, capture_output=True)
    if b.returncode:
        sys.stderr.write(b.stderr); return b.returncode
    r = subprocess.run([str(CRATE / "target/release/v005-extensions"), str(REG), str(V148), str(V150)], text=True, capture_output=True)
    if r.returncode:
        sys.stderr.write(r.stderr); return r.returncode
    rust = json.loads(r.stdout)
    verdict_mismatch, byte_mismatch = [], []
    for raw in json.loads(V148.read_text()):
        v = ex148.expand(raw)
        want = enc({'$map': [[v['extension_id'], {'$map': [[0, v['critical']], [1, v['payload']]]}]]}).hex()
        got = rust[raw["id"]]
        if (got["verdict"], got["error"]) != (raw["expected_verdict"], raw.get("expected_error")): verdict_mismatch.append(raw["id"])
        if got["cbor_hex"] != want: byte_mismatch.append(raw["id"])
    for v in json.loads(V150.read_text()):
        want = enc({'$map': [['om.rich-list.v1', {'$map': [[0, v['critical']], [1, rl150.payload(v)]]}]]}).hex()
        got = rust[v["id"]]
        if (got["verdict"], got["error"]) != (v["expected_verdict"], v.get("expected_error")): verdict_mismatch.append(v["id"])
        if got["cbor_hex"] != want: byte_mismatch.append(v["id"])
    ok = not verdict_mismatch and not byte_mismatch
    OUT.write_text(json.dumps({
        "adr": "ADR-0168", "gate": "V-005", "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        "implementation": "verification/prototypes/v005-extensions-rust (Rust, clean-room from MESSENGER_BASELINE_V1 + registries)",
        "compared_with": "expected vector verdicts/errors and the Python canonical-CBOR entry bytes (the Python/Node pair already agree)",
        "vectors": len(rust), "verdict_mismatches": verdict_mismatch, "byte_mismatches": byte_mismatch, "passed": ok,
        "independence": "different author (Claude) and language/runtime (Rust) from the OpenAI-authored Python/Node pair; "
                        "independent_implementation per ADR-0117 amendment and ADR-0167 §3"}, indent=2) + "\n")
    print(json.dumps({"passed": ok, "vectors": len(rust), "verdict_mismatches": len(verdict_mismatch), "byte_mismatches": len(byte_mismatch)}))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
