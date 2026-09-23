"""Markdown/text rewriting used by the relayout and by `om_docs.py fmt`.

Everything here is idempotent: running it twice changes nothing the second time.
"""
from __future__ import annotations

import os
import posixpath
import re
from pathlib import Path

from .layout import layout, map_legacy_path

LINK_RE = re.compile(r"(?<!!)\[([^\]]*)\]\(([^)\s]+)((?:\s+\"[^\"]*\")?)\)")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
MOVED_DIRS = tuple(k for k, v in layout()["dir_moves"].items() if k != v)
TOKEN_RE = re.compile(r"(?<![\w./-])((?:%s)[\w*.{}/-]*)" % "|".join(re.escape(d) for d in MOVED_DIRS))
TEXT_SUFFIXES = {".md", ".json", ".jsonl", ".py", ".mjs", ".js", ".tla", ".cfg", ".txt", ".toml", ".yml", ".yaml", ".html", ".cddl"}


def _is_external(t: str) -> bool:
    return t.startswith(("#", "http://", "https://", "mailto:", "data:", "sandbox:"))


def relink_legacy(text: str, legacy_path: str, new_path: str, legacy_files: set[str]) -> str:
    """Rewrite relative links written against the legacy layout for a file now at new_path."""
    legacy_dirs = {posixpath.dirname(p) for p in legacy_files}
    old_dir = posixpath.dirname(legacy_path)
    new_dir = posixpath.dirname(new_path)

    def sub(m: re.Match) -> str:
        label, target, title = m.groups()
        if _is_external(target):
            return m.group(0)
        path, sep, frag = target.partition("#")
        tgt = posixpath.normpath(posixpath.join(old_dir, path)) if path else ""
        if not path or (tgt not in legacy_files and tgt not in legacy_dirs):
            return m.group(0)
        mapped = map_legacy_path(tgt) if tgt in legacy_files else _map_dir(tgt)
        newrel = posixpath.relpath(mapped, new_dir or ".")
        if path.endswith("/") and not newrel.endswith("/"):
            newrel += "/"
        return f"[{label}]({newrel}{sep}{frag}{title})"

    return _outside_fences(text, lambda line: LINK_RE.sub(sub, line))


def _map_dir(d: str) -> str:
    probe = map_legacy_path(d + "/x")
    return probe[:-2]


def rewrite_legacy_tokens(text: str, legacy_files: set[str], suffix: str) -> str:
    """Rewrite path tokens under moved legacy directories, plus exact root-file strings in code/data."""
    legacy_dirs = {posixpath.dirname(p) for p in legacy_files if "/" in p}

    def tok(m: re.Match) -> str:
        t = m.group(1)
        core = t.rstrip(".,;:")
        tail = t[len(core):]
        bare = core.rstrip("/")
        if core in legacy_files:
            return map_legacy_path(core) + tail
        if bare in legacy_dirs:
            return _map_dir(bare) + core[len(bare):] + tail
        if "*" in core and posixpath.dirname(core) in legacy_dirs:
            d = posixpath.dirname(core)
            return _map_dir(d) + core[len(d):] + tail
        return t

    text = TOKEN_RE.sub(tok, text)
    if suffix in {".json", ".jsonl", ".py", ".mjs", ".js", ".toml", ".yml", ".yaml"}:
        roots = [p for p in legacy_files if "/" not in p and map_legacy_path(p) != p]
        for p in sorted(roots, key=len, reverse=True):
            text = re.sub(r'(["\'])%s\1' % re.escape(p), lambda m, p=p: f"{m.group(1)}{map_legacy_path(p)}{m.group(1)}", text)
    return text


def _outside_fences(text: str, fn) -> str:
    out, fenced = [], False
    for line in text.splitlines(keepends=True):
        if FENCE_RE.match(line):
            fenced = not fenced
            out.append(line)
            continue
        out.append(line if fenced else fn(line))
    return "".join(out)


BACKTICK_RE = re.compile(r"(?<![\[\w])`([\w./-]+\.(?:md|json|jsonl|tla|cfg|py|mjs|cddl|toml))`(?!\]|\()")


def linkify(text: str, rel_path: str, basenames: dict[str, list[str]], all_files: set[str]) -> str:
    """Turn `NAME.md` / `dir/file.json` mentions into relative links when the target is unambiguous."""
    here = posixpath.dirname(rel_path)

    def sub(m: re.Match) -> str:
        name = m.group(1)
        if name in all_files:
            target = name
        elif "/" not in name and len(basenames.get(name, [])) == 1:
            target = basenames[name][0]
        else:
            return m.group(0)
        if target == rel_path:
            return m.group(0)
        return f"[`{name}`]({posixpath.relpath(target, here or '.')})"

    def line_fn(line: str) -> str:
        if line.lstrip().startswith("#"):
            return line
        # leave text that is already inside a link label alone
        parts = re.split(r"(\[[^\]]*\]\([^)]*\))", line)
        return "".join(p if i % 2 else BACKTICK_RE.sub(sub, p) for i, p in enumerate(parts))

    return _outside_fences(text, line_fn)


def fix_links_by_basename(text: str, rel_path: str, root: Path, basenames: dict[str, list[str]]) -> tuple[str, list[str]]:
    """Repair broken relative links whose basename is unique in the repo. Returns (text, unresolved)."""
    here = posixpath.dirname(rel_path)
    unresolved: list[str] = []

    def sub(m: re.Match) -> str:
        label, target, title = m.groups()
        if _is_external(target):
            return m.group(0)
        path, sep, frag = target.partition("#")
        if not path:
            return m.group(0)
        if (root / here / path).exists():
            return m.group(0)
        cands = basenames.get(posixpath.basename(path.rstrip("/")), [])
        if len(cands) == 1:
            return f"[{label}]({posixpath.relpath(cands[0], here or '.')}{sep}{frag}{title})"
        unresolved.append(target)
        return m.group(0)

    return _outside_fences(text, lambda line: LINK_RE.sub(sub, line)), unresolved


def repo_index(root: Path, files: list[str]) -> tuple[dict[str, list[str]], set[str]]:
    names: dict[str, list[str]] = {}
    for f in files:
        names.setdefault(posixpath.basename(f), []).append(f)
    return names, set(files)
