#!/usr/bin/env python3
"""Partial offline continuity drill for V-017.

Builds a self-contained local evidence bundle from current protocol definitions,
then verifies it without network access. This is NOT a completed V-017 drill;
it proves only that the current checkpoint contains enough local material to
construct and independently integrity-check a continuity snapshot.
"""
from __future__ import annotations
import hashlib, json, tempfile, shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "verification/results/continuity-dry-run-latest.json"

INCLUDE_FILES = [
    "CANONICAL_ENCODING.md",
    "CRYPTOGRAPHY_AND_AGILITY.md",
    "PROTOCOL_EXTENSIBILITY.md",
    "PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md",
    "LICENSE.md",
    "machine/decisions.jsonl",
    "machine/requirements.jsonl",
    "machine/resolved_questions.jsonl",
]


def digest(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def integrity_root(rows):
    h = hashlib.sha256()
    for r in sorted(rows, key=lambda x: x["path"]):
        h.update(r["path"].encode()); h.update(b"\0"); h.update(r["sha256"].encode()); h.update(b"\n")
    return h.hexdigest()


def main():
    paths = [ROOT / p for p in INCLUDE_FILES]
    paths += sorted((ROOT / "protocol").glob("*.json"))
    paths += sorted((ROOT / "LICENSES").glob("*.txt"))
    missing = [str(p.relative_to(ROOT)) for p in paths if not p.exists()]
    assert not missing, missing

    rows = [{"path": str(p.relative_to(ROOT)), "bytes": p.stat().st_size, "sha256": digest(p)} for p in paths]
    root_hash = integrity_root(rows)

    parsed_json = 0
    parsed_jsonl_records = 0
    with tempfile.TemporaryDirectory(prefix="om-continuity-") as td:
        td = Path(td)
        for p in paths:
            rel = p.relative_to(ROOT)
            dst = td / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, dst)
        # Deliberately operate only on copied local bytes from here onward.
        for r in rows:
            p = td / r["path"]
            assert p.stat().st_size == r["bytes"]
            assert digest(p) == r["sha256"]
            if p.suffix == ".json":
                json.loads(p.read_text()); parsed_json += 1
            elif p.suffix == ".jsonl":
                for line in p.read_text().splitlines():
                    if line.strip(): json.loads(line); parsed_jsonl_records += 1
        assert integrity_root(rows) == root_hash

    payload = {
        "gate": "V-017",
        "gate_status_effect": "partial_evidence_only_gate_remains_open",
        "offline_copy_verified": True,
        "artifact_count": len(rows),
        "protocol_json_parsed": parsed_json,
        "machine_jsonl_records_parsed": parsed_jsonl_records,
        "integrity_root_sha256": root_hash,
        "included": [r["path"] for r in rows],
        "remaining": [
            "independent implementation/conformance reconstruction",
            "multi-generation and explicit fork-lineage exercise",
            "catastrophic authentication primitive break tabletop",
            "release-root loss/rotation drill",
            "provider disappearance/migration drill",
            "dominant proprietary extension capture drill",
            "independent mirrored distribution/rebuild path",
        ],
    }
    OUT.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({k:v for k,v in payload.items() if k != "included"}, indent=2))

if __name__ == "__main__": main()
