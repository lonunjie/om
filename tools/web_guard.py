#!/usr/bin/env python3
"""Fast, dependency-free guard for the web workspace (runs in `make check`, no Node needed).

Complements ESLint (AST rules) by scanning every source/asset file under packages/ and apps/ for things ESLint
cannot see: remote subresources in HTML/CSS, inline scripts/handlers in HTML, and a production-bundle import of the
dev-only core harness. See docs/process/ENGINEERING_REPOSITORY_LAYOUT.md "TypeScript/web rules".
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKIP = {"node_modules", "dist", "dist-types", ".vite", "coverage", "playwright-report", "test-results"}
REMOTE = re.compile(r"""(?:src|href|action|poster|srcset)\s*=\s*["']?\s*(?:https?:)?//|url\(\s*["']?\s*(?:https?:)?//|@import\s+["']?\s*(?:https?:)?//""", re.I)
INLINE_HANDLER = re.compile(r"""\son[a-z]+\s*=""", re.I)
INLINE_SCRIPT = re.compile(r"<script(?![^>]*\bsrc=)[^>]*>\s*\S", re.I)
# Anchors are navigation, not subresources: the landing page links to the project repository and site.
ANCHOR_TAG = re.compile(r"<a\s[^>]*>", re.I)
HARNESS_IMPORT = re.compile(r"""['"]@om/core-harness['"]""")
TEST_SUPPORT_IMPORT = re.compile(r"""['"]@om/core-wasm/test-support['"]|['"]\./test-support['"]""")


def files():
    for top in ("packages", "apps"):
        base = ROOT / top
        if not base.exists():
            continue
        for p in base.rglob("*"):
            if p.is_file() and not (set(p.relative_to(ROOT).parts) & SKIP):
                yield p


def main() -> int:
    errs: list[str] = []
    for p in files():
        rel = p.relative_to(ROOT).as_posix()
        if p.suffix not in {".html", ".css", ".ts", ".tsx", ".js", ".mjs", ".json", ".webmanifest"}:
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        if p.suffix in {".html", ".css"}:
            if REMOTE.search(ANCHOR_TAG.sub("", text) if p.suffix == ".html" else text):
                errs.append(f"{rel}: remote subresource (no CDN/remote fonts/themes; ENGINEERING_REPOSITORY_LAYOUT.md)")
        if p.suffix == ".html":
            if INLINE_HANDLER.search(text):
                errs.append(f"{rel}: inline event handler (CSP forbids it)")
            if INLINE_SCRIPT.search(text):
                errs.append(f"{rel}: inline script (CSP forbids it)")
        # The dev-only harness may be imported by tests and by packages/om-core-harness itself, never by app/UI source.
        if p.suffix in {".ts", ".tsx"} and ".test." not in p.name and not rel.startswith("packages/om-core-harness/"):
            if HARNESS_IMPORT.search(text) and "/dev/" not in rel and not rel.endswith(("main.dev.tsx",)):
                errs.append(f"{rel}: imports @om/core-harness outside tests/dev entry (dev-only, never ships)")
        # The real Core binding may ship; its in-memory test provider/store may not.
        if p.suffix in {".ts", ".tsx"} and ".test." not in p.name and not rel.startswith("packages/om-core-wasm/src/test-support") and "/dev/" not in rel:
            if TEST_SUPPORT_IMPORT.search(text):
                errs.append(f"{rel}: imports the Core test-support provider outside tests/dev (test-only, never ships)")
    for e in errs:
        print("ERROR: web guard:", e, file=sys.stderr)
    print(f"web guard: {'FAILED' if errs else 'OK'} ({len(errs)} error(s))")
    return 1 if errs else 0


if __name__ == "__main__":
    raise SystemExit(main())
