#!/usr/bin/env python3
"""Rebuild OM checkpoint MANIFEST.json deterministically.

Used by `tools/om_drop.py export`: the manifest covers every file in an exported drop except the
contract's manifest_excludes. It is an integrity inventory, not a signature. In the git repository
itself, git is the integrity record and no MANIFEST.json is kept.
"""
from __future__ import annotations
import hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "MANIFEST.json"
CONTRACT = json.loads((ROOT / "machine/repository_contract.json").read_text(encoding="utf-8"))
SNAPSHOT = CONTRACT["snapshot"]
EXCLUDES = set(CONTRACT.get("manifest_excludes", ["MANIFEST.json"]))
SKIP_PARTS = {"__pycache__", ".git", "design-drops", "node_modules", "target", ".venv"}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def main() -> int:
    rows = []
    for p in sorted(x for x in ROOT.rglob("*") if x.is_file()):
        rel = p.relative_to(ROOT).as_posix()
        if rel in EXCLUDES or SKIP_PARTS & set(p.relative_to(ROOT).parts) or p.name == ".DS_Store":
            continue
        rows.append({"path": rel, "bytes": p.stat().st_size, "sha256": sha256(p)})
    data = {
        "snapshot": SNAPSHOT,
        "manifest_excludes": sorted(EXCLUDES),
        "files": rows,
    }
    OUT.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} with {len(rows)} files")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
