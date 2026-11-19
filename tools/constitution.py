#!/usr/bin/env python3
"""Search the OM constitution and the ADRs without reading them whole.

`docs/constitution/CONSTITUTION_DIGEST.md` is derived, non-normative material: numbered rules C-nn, each one sentence
ending in its source `[CODE §heading; ADR-nnnn]`. The nine originals stay authoritative. This tool keeps the digest
searchable and checked against them:

  digest                      print the digest
  find KEYWORD|TAG ... [--full] [-n N]
                              ranked rules (id, rule, source path#anchor); --full adds the cited original section (bounded)
  show C-nn [--source]        one rule; --source adds the cited original section(s) (bounded)
  adr KEYWORD ... [-n N]      find ADRs by id/title/status (id, title, status, path) from decisions/ and INDEX.md
  gen                         regenerate machine/constitution_index.json and machine/adr_index.json
  check                       sources resolve, ids unique, digest size within budget, both JSON files match their sources
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIGEST = ROOT / "docs/constitution/CONSTITUTION_DIGEST.md"
CONST_JSON = ROOT / "machine/constitution_index.json"
ADR_JSON = ROOT / "machine/adr_index.json"
ADR_DIR = ROOT / "decisions"
WORD_BUDGET = 3500
FULL_MAX_LINES = 60
FULL_MAX_CHARS = 3500

DOCS = {
    "PHIL": "PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE",
    "LIC": "LICENSING_AND_CONTRIBUTION_POLICY",
    "STEW": "PROJECT_STEWARDSHIP_AND_CONTINUITY",
    "OPLESS": "OPERATORLESS_CORE_AND_ECOSYSTEM",
    "PRIV": "PRIVACY_GOVERNANCE_AUTHORITY",
    "LEGAL": "LEGAL_AND_OPERATIONAL_BOUNDARIES",
    "EXT": "PROTOCOL_EXTENSIBILITY",
    "LONG": "PROTOCOL_LONGEVITY_AND_0M_CONTINUITY",
    "NAME": "NAMING",
}

RULE_RE = re.compile(r"^- \*\*(C-\d+) (.+?)\.\*\* (.*) \[([^\]]+)\]((?: #[\w-]+)*)$")
GROUP_RE = re.compile(r"^## .*?(?: \{tags: ([^}]*)\})?$")
HEAD_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
ADR_ID_RE = re.compile(r"ADR-\d{4}")


# ---------------------------------------------------------------- originals

def norm_heading(text: str) -> str:
    text = re.sub(r"[`*_]", "", text).strip()
    text = re.sub(r"^(\d+)\.\s", r"\1 ", text)
    return text.casefold()


def slug(text: str) -> str:
    s = re.sub(r"[`*_]", "", text).strip().casefold()
    s = re.sub(r"[^\w\s-]", "", s)
    return re.sub(r"\s", "-", s)


_doc_cache: dict[str, tuple[list[str], list[tuple[int, int, str]]]] = {}


def load_doc(code: str):
    if code not in _doc_cache:
        path = ROOT / "docs/constitution" / f"{DOCS[code]}.md"
        lines = path.read_text(encoding="utf-8").split("\n")
        heads, fence = [], False
        for i, line in enumerate(lines, 1):
            if re.match(r"^\s*(```|~~~)", line):
                fence = not fence
            m = None if fence else HEAD_RE.match(line)
            if m:
                heads.append((i, len(m.group(1)), m.group(2)))
        _doc_cache[code] = (lines, heads)
    return _doc_cache[code]


def resolve(code: str, ref: str) -> dict:
    """Find the one heading of document CODE that REF names; raise ValueError if none or ambiguous."""
    if code not in DOCS:
        raise ValueError(f"unknown source code {code!r}")
    lines, heads = load_doc(code)
    want = norm_heading(ref)
    hits = []
    for idx, (ln, level, text) in enumerate(heads):
        h = norm_heading(text)
        if h.startswith(want) and (len(h) == len(want) or not h[len(want)].isalnum()):
            hits.append(idx)
    if not hits:
        raise ValueError(f"{code} §{ref}: no such heading in {DOCS[code]}.md")
    if len(hits) > 1:
        raise ValueError(f"{code} §{ref}: ambiguous ({len(hits)} headings); lengthen the reference")
    idx = hits[0]
    ln, level, text = heads[idx]
    end = len(lines)
    for ln2, level2, _ in heads[idx + 1:]:
        if level2 <= level:
            end = ln2 - 1
            break
    while end > ln and not lines[end - 1].strip():
        end -= 1
    return {
        "file": f"docs/constitution/{DOCS[code]}.md",
        "heading": re.sub(r"[`*]", "", text),
        "anchor": slug(text),
        "lines": [ln, end],
    }


def section_text(src: dict) -> str:
    lines = (ROOT / src["file"]).read_text(encoding="utf-8").split("\n")
    a, b = src["lines"]
    chunk = lines[a - 1:b]
    out, size = [], 0
    for i, l in enumerate(chunk):
        if i >= FULL_MAX_LINES or size + len(l) > FULL_MAX_CHARS:
            out.append(f"[... truncated; {b - a + 1 - i} more lines: read {src['file']} lines {a}-{b}]")
            break
        out.append(l)
        size += len(l) + 1
    return "\n".join(out)


# ------------------------------------------------------------------- digest

def parse_digest() -> list[dict]:
    rules, group_tags = [], []
    for n, line in enumerate(DIGEST.read_text(encoding="utf-8").split("\n"), 1):
        if line.startswith("## "):
            m = GROUP_RE.match(line)
            group_tags = [t.strip() for t in (m.group(1) or "").split(",") if t.strip()] if m else []
            continue
        if not line.startswith("- **C-"):
            continue
        m = RULE_RE.match(line)
        if not m:
            raise ValueError(f"{DIGEST.name}:{n}: malformed rule line")
        rid, title, text, bracket, extra = m.groups()
        sources, adrs = [], []
        for part in (p.strip() for p in bracket.split(";")):
            if part.startswith("ADR-"):
                adrs += ADR_ID_RE.findall(part)
            else:
                code, _, ref = part.partition(" §")
                if not ref:
                    raise ValueError(f"{rid}: bad source {part!r} (want 'CODE §heading')")
                sources.append((code, ref))
        tags = list(dict.fromkeys(group_tags + [t[1:] for t in extra.split()]))
        rules.append({"id": rid, "title": title, "text": text, "tags": tags, "_sources": sources, "adrs": adrs, "_line": n})
    return rules


def build_constitution() -> tuple[dict, list[str]]:
    errors, entries, seen = [], [], set()
    for r in parse_digest():
        if r["id"] in seen:
            errors.append(f"{r['id']}: duplicate id (digest line {r['_line']})")
        seen.add(r["id"])
        if not r["_sources"]:
            errors.append(f"{r['id']}: no source")
        src = []
        for code, ref in r["_sources"]:
            try:
                src.append(resolve(code, ref))
            except ValueError as e:
                errors.append(f"{r['id']}: {e}")
        entries.append({"id": r["id"], "title": r["title"], "text": r["text"], "tags": r["tags"], "source": src, "adrs": r["adrs"]})
    return {
        "note": "Derived from docs/constitution/CONSTITUTION_DIGEST.md by tools/constitution.py gen. Non-normative: the originals win. Do not edit by hand.",
        "digest": "docs/constitution/CONSTITUTION_DIGEST.md",
        "rules": entries,
    }, errors


def load_rules() -> list[dict]:
    """Rules for search: from the generated JSON when it is current, else rebuilt from the digest."""
    data, errors = build_constitution()
    if errors:
        for e in errors:
            print(f"constitution: {e}", file=sys.stderr)
        raise SystemExit(1)
    return data["rules"]


# ---------------------------------------------------------------------- ADRs

def build_adr() -> dict:
    index_titles = {}
    idx = ADR_DIR / "INDEX.md"
    if idx.exists():
        for m in re.finditer(r"^- \[(ADR-\d{4}) — (.*)\]\(([^)]+)\)\s*$", idx.read_text(encoding="utf-8"), re.M):
            index_titles[m.group(1)] = m.group(2)
    entries = []
    for p in sorted(ADR_DIR.glob("ADR-*.md")):
        head = p.read_text(encoding="utf-8").split("\n")[:60]
        m = re.match(r"^# (ADR-\d{4}) — (.*)$", head[0])
        if not m:
            continue
        adr_id, title = m.groups()
        status = date = ""
        amends: list[str] = []
        for line in head[1:]:
            s = re.match(r"^[-* ]*\*\*Status:?\*\*:?\s*(.*)$", line)
            if s and not status:
                status = s.group(1).strip()
            d = re.match(r"^[-* ]*\*\*Date:?\*\*:?\s*(\d{4}-\d{2}-\d{2})", line)
            if d and not date:
                date = d.group(1)
            a = re.match(r"^[-* ]*\*\*Amends:?\*\*:?\s*(.*)$", line)
            if a:
                amends = sorted(set(ADR_ID_RE.findall(a.group(1))) - {adr_id})
        e = {"id": adr_id, "title": title, "status": status[:140], "date": date, "path": f"decisions/{p.name}", "amends": amends}
        it = index_titles.get(adr_id)
        if it and it != title:
            e["index_title"] = it
        entries.append(e)
    by = {}
    for e in entries:
        for t in e["amends"]:
            by.setdefault(t, []).append(e["id"])
    for e in entries:
        e["amended_by"] = sorted(by.get(e["id"], []))
    return {"note": "Derived from decisions/ADR-*.md and decisions/INDEX.md by tools/constitution.py gen. Do not edit by hand.", "adrs": entries}


# ------------------------------------------------------------------ commands

def words(q: list[str]) -> list[str]:
    return [w.casefold() for a in q for w in re.split(r"[\s,]+", a) if w]


def cmd_find(args: list[str]) -> int:
    full = "--full" in args
    limit = 8
    rest, it = [], iter(args)
    for a in it:
        if a == "--full":
            continue
        if a == "-n":
            limit = int(next(it, "8"))
        else:
            rest.append(a)
    q = words(rest)
    if not q:
        print("usage: constitution.py find <keywords or tags...> [--full] [-n N]", file=sys.stderr)
        return 2
    scored = []
    for r in load_rules():
        title, text = r["title"].casefold(), r["text"].casefold()
        score = 0
        for w in q:
            if w in r["tags"]:
                score += 5
            elif any(w in t for t in r["tags"]):
                score += 2
            if w == r["id"].casefold():
                score += 10
            if w in title:
                score += 3
            if w in text:
                score += 1
            if any(w in s["heading"].casefold() for s in r["source"]):
                score += 1
        if score:
            scored.append((score, r))
    scored.sort(key=lambda x: (-x[0], x[1]["id"]))
    if not scored:
        print("no match; try `constitution.py digest` or a tag (e.g. recovery, metadata, extensibility, licensing)")
        return 1
    for score, r in scored[:limit]:
        print(f"{r['id']}  {r['title']}. {r['text']}")
        for s in r["source"]:
            print(f"      {s['file']}#{s['anchor']}")
        if full:
            for s in r["source"]:
                print(f"\n--- {s['file']} lines {s['lines'][0]}-{s['lines'][1]} ({s['heading']}) ---\n{section_text(s)}\n")
    if len(scored) > limit:
        print(f"({len(scored) - limit} more; use -n)")
    return 0


def cmd_show(args: list[str]) -> int:
    ids = [a for a in args if not a.startswith("--")]
    if not ids:
        print("usage: constitution.py show C-nn [--source]", file=sys.stderr)
        return 2
    rid = ids[0].upper()
    if re.fullmatch(r"C-\d", rid):
        rid = "C-0" + rid[2:]
    rules = {r["id"]: r for r in load_rules()}
    r = rules.get(rid)
    if not r:
        print(f"no rule {rid}", file=sys.stderr)
        return 1
    print(f"{r['id']}  {r['title']}\n{r['text']}\ntags: {', '.join(r['tags'])}" + (f"\nadrs: {', '.join(r['adrs'])}" if r["adrs"] else ""))
    for s in r["source"]:
        print(f"source: {s['file']}#{s['anchor']} (lines {s['lines'][0]}-{s['lines'][1]})")
        if "--source" in args:
            print(f"\n{section_text(s)}\n")
    return 0


def cmd_adr(args: list[str]) -> int:
    limit = 10
    rest, it = [], iter(args)
    for a in it:
        if a == "-n":
            limit = int(next(it, "10"))
        else:
            rest.append(a)
    q = words(rest)
    if not q:
        print("usage: constitution.py adr <keywords or ADR-nnnn...> [-n N]", file=sys.stderr)
        return 2
    scored = []
    for e in build_adr()["adrs"]:
        title = (e["title"] + " " + e.get("index_title", "")).casefold()
        status = e["status"].casefold()
        score = 0
        for w in q:
            if w in (e["id"].casefold(), e["id"][4:].lstrip("0")) or w == e["id"].casefold():
                score += 10
            if w in title:
                score += 3
            if w in status:
                score += 1
        if score:
            scored.append((score, e))
    scored.sort(key=lambda x: (-x[0], x[1]["id"]))
    if not scored:
        print("no match")
        return 1
    for _, e in scored[:limit]:
        title = e.get("index_title", e["title"])
        print(f"{e['id']}  {title}\n      status: {e['status'] or '(none)'}  date: {e['date'] or '?'}\n      {e['path']}")
    if len(scored) > limit:
        print(f"({len(scored) - limit} more; use -n)")
    return 0


def dumps(obj: dict) -> str:
    return json.dumps(obj, indent=2, ensure_ascii=False) + "\n"


def cmd_gen() -> int:
    data, errors = build_constitution()
    if errors:
        for e in errors:
            print(f"constitution: {e}", file=sys.stderr)
        return 1
    CONST_JSON.write_text(dumps(data), encoding="utf-8")
    adr = build_adr()
    ADR_JSON.write_text(dumps(adr), encoding="utf-8")
    print(f"constitution: wrote {CONST_JSON.relative_to(ROOT)} ({len(data['rules'])} rules) and {ADR_JSON.relative_to(ROOT)} ({len(adr['adrs'])} ADRs)")
    return 0


def cmd_check() -> int:
    errors = []
    try:
        data, errs = build_constitution()
        errors += errs
    except ValueError as e:
        data, errors = None, errors + [str(e)]
    n_words = len(DIGEST.read_text(encoding="utf-8").split())
    if n_words > WORD_BUDGET:
        errors.append(f"digest is {n_words} words, over the {WORD_BUDGET} budget; shorten it")
    if data is not None and not errors:
        for path, want, label in ((CONST_JSON, data, "constitution"), (ADR_JSON, build_adr(), "adr")):
            have = path.read_text(encoding="utf-8") if path.exists() else ""
            if have != dumps(want):
                errors.append(f"{path.relative_to(ROOT)} is out of date with its source ({label}); run `python3 tools/constitution.py gen`")
    for e in errors:
        print(f"constitution: {e}", file=sys.stderr)
    if not errors:
        print(f"constitution: OK ({len(data['rules'])} rules, {n_words} words; every source heading resolves; indexes current)")
    return 1 if errors else 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    cmd, rest = argv[1], argv[2:]
    if cmd == "digest":
        sys.stdout.write(DIGEST.read_text(encoding="utf-8"))
        return 0
    table = {"find": cmd_find, "show": cmd_show, "adr": cmd_adr}
    if cmd in table:
        return table[cmd](rest)
    if cmd == "gen":
        return cmd_gen()
    if cmd == "check":
        return cmd_check()
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
