"""Legacy flat-drop -> repository layout mapping (data lives in tools/layout.json)."""
from __future__ import annotations

import fnmatch
import json
from functools import lru_cache
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1]
LAYOUT_FILE = TOOLS / "layout.json"
# Paths that never travel inside a design drop (repo plumbing only).
REPO_ONLY = (".git/", ".github/", "design-drops/")


@lru_cache(maxsize=1)
def layout() -> dict:
    return json.loads(LAYOUT_FILE.read_text(encoding="utf-8"))


def map_legacy_path(p: str) -> str:
    """Map a path from the legacy flat layout to the repository layout."""
    L = layout()
    for old, new in L["dir_moves"].items():
        if p.startswith(old):
            return new + p[len(old):]
    if "/" in p:
        return p  # unknown subdirectory: keep as-is
    if p in L["root_keep"]:
        return p
    for pat, dest in L["root_patterns"].items():
        if fnmatch.fnmatch(p, pat):
            return dest + p
    dest = L["root_files"].get(p)
    if dest is None:
        dest = L["unknown_root_doc_dir"] if p.endswith(".md") else ""
    return dest + p if dest.endswith("/") or dest == "" else dest


def legacy_paths_for(repo_files: list[str]) -> set[str]:
    """Inverse mapping: the legacy path each current repository file would have had."""
    L = layout()
    inv_dirs = sorted(((new, old) for old, new in L["dir_moves"].items() if old != new), key=lambda x: -len(x[0]))
    by_new = {map_legacy_path(k): k for k in list(L["root_files"]) + list(L["root_keep"])}
    out = set(L["root_files"]) | set(L["root_keep"])
    for f in repo_files:
        if f in by_new:
            out.add(by_new[f])
            continue
        for new, old in inv_dirs:
            if f.startswith(new):
                out.add(old + f[len(new):])
                break
        else:
            out.add(f.rsplit("/", 1)[-1] if f.startswith("docs/checkpoints/CHECKPOINT_") else f)
    return out


def legacy_managed_prefixes() -> set[str]:
    """Repository paths that a legacy drop owns (used to scope deletions)."""
    L = layout()
    out = set(L["dir_moves"].values()) | set(L["root_keep"])
    out |= set(L["root_patterns"].values())
    for p in L["root_files"]:
        out.add(map_legacy_path(p))
    return out


def is_legacy_managed(repo_path: str) -> bool:
    for pref in legacy_managed_prefixes():
        if repo_path == pref or (pref.endswith("/") and repo_path.startswith(pref)):
            return True
    return False


def is_stripped(p: str) -> bool:
    parts = p.split("/")
    if parts[0] == "history" or "__pycache__" in parts:
        return True
    if parts[-1] == ".DS_Store" or p.endswith(".pyc"):
        return True
    return p in ("MANIFEST.json", "OM_EXPORT.json")
