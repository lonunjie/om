#!/usr/bin/env python3
"""Keep OM's Markdown navigable on GitHub and in a local checkout.

  python3 tools/om_docs.py fmt     # linkify `NAME.md` mentions, repair moved links (idempotent)
  python3 tools/om_docs.py check   # exit 1 if fmt would change anything or a link is unresolvable

Basenames are unique across docs, so a mention like `BLUEPRINT.md` always resolves.
Checkpoint records under docs/checkpoints/ are historical and are only link-repaired.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from omlib.docs import fix_links_by_basename, linkify, repo_index  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
NO_LINKIFY = ("docs/checkpoints/", "CHANGELOG.md")


def tracked_files() -> list[str]:
    try:
        out = subprocess.run(["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT,
                             check=True, capture_output=True, text=True).stdout
        files = [f for f in out.splitlines() if (ROOT / f).is_file()]
    except (subprocess.CalledProcessError, FileNotFoundError):  # exported drop without .git
        files = [p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*") if p.is_file() and ".git" not in p.parts]
    return sorted(f for f in files if not f.startswith(("design-drops/", "history/")) and "__pycache__" not in f)


def run(write: bool) -> int:
    files = tracked_files()
    names, allset = repo_index(ROOT, files)
    changed, unresolved = [], []
    for f in files:
        if not f.endswith(".md"):
            continue
        p = ROOT / f
        t0 = p.read_text(encoding="utf-8")
        t, bad = fix_links_by_basename(t0, f, ROOT, names)
        unresolved += [f"{f} -> {b}" for b in bad]
        if not f.startswith(NO_LINKIFY) and f != "CHANGELOG.md":
            t = linkify(t, f, names, allset)
        if t != t0:
            changed.append(f)
            if write:
                p.write_text(t, encoding="utf-8")
    for u in unresolved:
        print(f"unresolved link: {u}", file=sys.stderr)
    verb = "rewrote" if write else "would rewrite"
    print(f"om_docs: {verb} {len(changed)} file(s); {len(unresolved)} unresolved link(s)")
    if not write and changed:
        for c in changed:
            print(f"  {c}")
    return 1 if unresolved or (changed and not write) else 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    if cmd not in {"fmt", "check"}:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(run(cmd == "fmt"))
