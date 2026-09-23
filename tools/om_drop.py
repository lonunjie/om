#!/usr/bin/env python3
"""Move OM's design between this git repository and zip "design drops".

  python3 tools/om_drop.py export [--ref HEAD] [--out-dir design-drops]
  python3 tools/om_drop.py import design-drops/<drop>.zip [--base <commit>] [--stamp YYYYMMDD-HHMM] [--no-merge]

export  Packages a committed ref as design-drops/om-design-<stamp>-<sha>.zip. The zip contains the repository
        layout plus OM_EXPORT.json (base commit) and MANIFEST.json (integrity inventory), and must validate.
import  Commits the drop on a branch cut from its base (OM_EXPORT.json, --base, or, for a legacy flat drop, the
        newest history/<stamp> it carries), tags it checkpoint/<stamp>, and merges that branch into the current
        branch. Git's three-way merge keeps concurrent repository work and surfaces real conflicts.
        Afterwards it normalizes links (om_docs fmt) and runs the validator.

The repository is canonical; zips are transport. history/ in a drop is ignored because git is the history.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from omlib.docs import TEXT_SUFFIXES, rewrite_legacy_tokens  # noqa: E402
from omlib.layout import REPO_ONLY, is_legacy_managed, is_stripped, map_legacy_path  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
PHT = timezone(timedelta(hours=8))
FORMAT = "om-design-drop/2"


def git(*args: str, cwd: Path = ROOT, check: bool = True, env: dict | None = None) -> str:
    r = subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True, env=env)
    if check and r.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed:\n{r.stderr.strip()}")
    return r.stdout.strip()


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def is_repo_only(path: str) -> bool:
    return any(path == r.rstrip("/") or (r.endswith("/") and path.startswith(r)) for r in REPO_ONLY)


def stamp_from_contract(root: Path) -> str | None:
    try:
        snap = json.loads((root / "machine/repository_contract.json").read_text(encoding="utf-8"))["snapshot"]
        return datetime.fromisoformat(snap).astimezone(PHT).strftime("%Y%m%d-%H%M")
    except Exception:
        return None


# --------------------------------------------------------------------------- export

def cmd_export(a: argparse.Namespace) -> int:
    sha = git("rev-parse", a.ref)
    short = sha[:10]
    if git("status", "--porcelain"):
        print("note: uncommitted changes are NOT included; export packages the committed ref only", file=sys.stderr)
    stamp = a.stamp or datetime.now(PHT).strftime("%Y%m%d-%H%M")
    name = f"om-design-{stamp}-PHT-{short}"
    out_dir = (ROOT / a.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        pkg = Path(tmp) / name
        pkg.mkdir()
        tar = subprocess.run(["git", "archive", "--format=tar", sha], cwd=ROOT, capture_output=True, check=True).stdout
        subprocess.run(["tar", "-x", "-C", str(pkg)], input=tar, check=True)
        for p in sorted(pkg.rglob("*"), reverse=True):
            rp = p.relative_to(pkg).as_posix()
            if is_repo_only(rp):
                shutil.rmtree(p) if p.is_dir() else p.unlink()
        (pkg / "OM_EXPORT.json").write_text(json.dumps({
            "format": FORMAT,
            "base_commit": sha,
            "base_ref": a.ref,
            "exported_at": datetime.now(PHT).isoformat(timespec="seconds"),
            "note": "Keep this file unchanged. It tells tools/om_drop.py import which commit your edits start from.",
        }, indent=2) + "\n", encoding="utf-8")
        subprocess.run([sys.executable, "tools/build_manifest.py"], cwd=pkg, check=True, capture_output=True)
        v = subprocess.run([sys.executable, "tools/validate_architecture.py"], cwd=pkg, text=True, capture_output=True)
        if v.returncode != 0:
            print(v.stdout + v.stderr, file=sys.stderr)
            raise SystemExit("export aborted: the packaged tree does not validate")
        commit_time = int(git("show", "-s", "--format=%ct", sha))
        zdt = datetime.fromtimestamp(commit_time, PHT).timetuple()[:6]
        zpath = out_dir / f"{name}.zip"
        with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for p in sorted(x for x in pkg.rglob("*") if x.is_file()):
                zi = zipfile.ZipInfo(f"{name}/{p.relative_to(pkg).as_posix()}", date_time=zdt)
                zi.compress_type = zipfile.ZIP_DEFLATED
                zi.external_attr = (0o755 if os.access(p, os.X_OK) else 0o644) << 16
                z.writestr(zi, p.read_bytes())
    print(v.stdout.strip())
    print(f"exported {a.ref} ({short}) -> {zpath.relative_to(ROOT) if zpath.is_relative_to(ROOT) else zpath}")
    print(f"sha256 {sha256_file(zpath)}  {zpath.stat().st_size:,} bytes")
    return 0


# --------------------------------------------------------------------------- import

def package_root(extracted: Path) -> Path:
    entries = [p for p in extracted.iterdir() if p.name not in {"__MACOSX", ".DS_Store"}]
    if len(entries) == 1 and entries[0].is_dir():
        return entries[0]
    return extracted


def drop_layout(pkg: Path) -> str:
    if (pkg / "OM_EXPORT.json").exists() or (pkg / "spec").is_dir() or (pkg / "docs/architecture").is_dir():
        return "repo"
    if (pkg / "machine").is_dir() and (pkg / "BLUEPRINT.md").exists():
        return "legacy"
    raise SystemExit("cannot recognize the drop layout (expected OM repository layout or legacy flat package)")


def check_drop_manifest(pkg: Path) -> str:
    mp = pkg / "MANIFEST.json"
    if not mp.exists():
        return "absent"
    try:
        man = json.loads(mp.read_text(encoding="utf-8"))
    except Exception as e:
        return f"unreadable ({e})"
    bad = []
    for row in man.get("files", []):
        p = pkg / row.get("path", "")
        if not p.is_file():
            bad.append(f"missing {row.get('path')}")
        elif p.stat().st_size != row.get("bytes") or sha256_file(p) != row.get("sha256"):
            bad.append(f"changed {row.get('path')}")
    for b in bad[:15]:
        print(f"  manifest: {b}", file=sys.stderr)
    return "ok" if not bad else f"{len(bad)} mismatch(es): the drop was edited after its manifest was built"


def resolve_base(a: argparse.Namespace, pkg: Path, layout: str) -> str:
    if a.base:
        return git("rev-parse", a.base + "^{commit}")
    if (pkg / "OM_EXPORT.json").exists():
        base = json.loads((pkg / "OM_EXPORT.json").read_text(encoding="utf-8")).get("base_commit", "")
        if git("cat-file", "-e", f"{base}^{{commit}}", check=False) == "" and base:
            return base
        raise SystemExit(f"OM_EXPORT.json base_commit {base!r} is not in this repository; pass --base")
    if layout == "legacy" and (pkg / "history").is_dir():
        stamps = sorted(p.name for p in (pkg / "history").iterdir() if re.fullmatch(r"\d{8}(-\d{4})?", p.name))
        for s in reversed(stamps):
            if git("rev-parse", "-q", "--verify", f"refs/tags/checkpoint/{s}", check=False):
                return git("rev-parse", f"checkpoint/{s}^{{commit}}")
    raise SystemExit("cannot determine the drop's base commit; pass --base <commit or checkpoint tag>")


def cmd_import(a: argparse.Namespace) -> int:
    zpath = Path(a.zip).resolve()
    if git("status", "--porcelain", "--untracked-files=no"):
        raise SystemExit("working tree has uncommitted changes; commit or stash them first")
    branch_now = git("rev-parse", "--abbrev-ref", "HEAD")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        with zipfile.ZipFile(zpath) as z:
            for n in z.namelist():
                if n.startswith("/") or ".." in Path(n).parts:
                    raise SystemExit(f"refusing unsafe path in zip: {n}")
            z.extractall(tmp / "x")
        pkg = package_root(tmp / "x")
        layout = drop_layout(pkg)
        manifest = check_drop_manifest(pkg)
        base = resolve_base(a, pkg, layout)
        stamp = a.stamp or stamp_from_contract(pkg) or datetime.now(PHT).strftime("%Y%m%d-%H%M")
        tag = f"checkpoint/{stamp}"
        if git("rev-parse", "-q", "--verify", f"refs/tags/{tag}", check=False):
            raise SystemExit(f"tag {tag} already exists; the drop may already be imported (pass --stamp to override)")
        files = {}
        for p in pkg.rglob("*"):
            if not p.is_file():
                continue
            rp = p.relative_to(pkg).as_posix()
            if is_stripped(rp) or is_repo_only(rp):
                continue
            files[map_legacy_path(rp) if layout == "legacy" else rp] = p
        print(f"drop: {zpath.name}  layout={layout}  files={len(files)}  manifest={manifest}")
        print(f"base: {base[:10]}  ->  {tag}")

        wt = tmp / "wt"
        git("worktree", "add", "--detach", str(wt), base)
        try:
            for f in git("ls-files", cwd=wt).splitlines():
                if is_repo_only(f) or f in {"CLAUDE.md", "GEMINI.md"}:
                    continue
                if layout == "repo" or is_legacy_managed(f):
                    (wt / f).unlink()
            for rp, src in files.items():
                dst = wt / rp
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dst)
            git("add", "-A", cwd=wt)
            if not git("status", "--porcelain", cwd=wt):
                print("drop is identical to its base; nothing to import")
                return 0
            when = stamp_to_iso(stamp)
            env = dict(os.environ, GIT_AUTHOR_DATE=when, GIT_COMMITTER_DATE=when) if when else None
            git("commit", "-q", "-m",
                f"checkpoint: import design drop {zpath.name}\n\n"
                f"Base: {base}\nLayout: {layout}\nManifest: {manifest}\n", cwd=wt, env=env)
            drop_commit = git("rev-parse", "HEAD", cwd=wt)
            git("tag", "-a", tag, drop_commit, "-m", f"OM design checkpoint {stamp} (from {zpath.name})")
            git("branch", "-f", f"drop/{stamp}", drop_commit)
        finally:
            git("worktree", "remove", "--force", str(wt), check=False)

    print(f"committed {drop_commit[:10]} on drop/{stamp}, tagged {tag}")
    if a.no_merge:
        print(f"not merged (--no-merge). To merge:  git merge --no-ff drop/{stamp}")
        return 0
    pre = git("rev-parse", "HEAD")
    r = subprocess.run(["git", "merge", "--no-ff", "--no-edit", "-m", f"Merge design drop {stamp} into {branch_now}",
                        f"drop/{stamp}"], cwd=ROOT, text=True, capture_output=True)
    if r.returncode != 0:
        print(r.stdout + r.stderr, file=sys.stderr)
        print(f"\nMERGE CONFLICTS. Resolve them, then: git commit && python3 tools/om_drop.py normalize {pre[:10]}",
              file=sys.stderr)
        return 3
    return normalize(pre, layout)


def stamp_to_iso(stamp: str) -> str | None:
    m = re.fullmatch(r"(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})", stamp)
    return f"{m[1]}-{m[2]}-{m[3]}T{m[4]}:{m[5]}:00+08:00" if m else None


def normalize(pre: str, layout: str = "repo") -> int:
    """After a drop merge: rewrite legacy path tokens in changed files, fix links, validate, commit."""
    changed = [f for f in git("diff", "--name-only", pre, "HEAD").splitlines() if (ROOT / f).is_file()]
    if layout == "legacy":
        legacy = set(json.loads((ROOT / "tools/layout.json").read_text())["root_files"]) | {
            f for f in git("ls-files").splitlines()}
        for f in changed:
            p = ROOT / f
            if p.suffix in TEXT_SUFFIXES:
                t0 = p.read_text(encoding="utf-8")
                t = rewrite_legacy_tokens(t0, legacy, p.suffix)
                if t != t0:
                    p.write_text(t, encoding="utf-8")
    subprocess.run([sys.executable, "tools/om_docs.py", "fmt"], cwd=ROOT, check=False)
    v = subprocess.run([sys.executable, "tools/validate_architecture.py"], cwd=ROOT, text=True, capture_output=True)
    print((v.stdout + v.stderr).strip())
    if git("status", "--porcelain"):
        git("add", "-A")
        git("commit", "-q", "-m", "Normalize imported design drop (links, legacy paths)")
        print("committed normalization")
    if v.returncode != 0:
        print("\nValidator FAILED after import: fix the errors above before pushing.", file=sys.stderr)
        return 1
    print("import complete: review with  git log --stat -3  and push when satisfied")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("export", help="package a committed ref as a design-drop zip")
    e.add_argument("--ref", default="HEAD")
    e.add_argument("--out-dir", default="design-drops")
    e.add_argument("--stamp", help="YYYYMMDD-HHMM (default: now, PHT)")
    i = sub.add_parser("import", help="merge a design-drop zip into the current branch")
    i.add_argument("zip")
    i.add_argument("--base", help="commit/tag the drop was derived from (overrides OM_EXPORT.json)")
    i.add_argument("--stamp", help="checkpoint stamp YYYYMMDD-HHMM (default: drop's contract snapshot)")
    i.add_argument("--no-merge", action="store_true", help="only create drop/<stamp> and the tag")
    n = sub.add_parser("normalize", help="finish an import after resolving merge conflicts")
    n.add_argument("pre", help="commit the branch was at before the merge")
    n.add_argument("--legacy", action="store_true")
    a = ap.parse_args()
    if a.cmd == "export":
        return cmd_export(a)
    if a.cmd == "import":
        return cmd_import(a)
    return normalize(git("rev-parse", a.pre), "legacy" if a.legacy else "repo")


if __name__ == "__main__":
    raise SystemExit(main())
