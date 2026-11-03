#!/usr/bin/env python3
"""Design gate: an agent must read the design for a domain before it changes code in it.

`machine/architecture_map.json` maps domains to their canonical docs and ADRs, and `code_paths` maps code paths to the
domains they implement. This tool turns that into three things:

  needs [PATH ...]      print the domains, docs and ADRs a change to those paths (default: staged files) must be read against
  commit-msg FILE       the git hook: a commit that stages code in a mapped path needs two trailers in its message,
                          Design-Read: <domain>, <domain>, ...   (every domain `needs` printed; ids only, no prose)
                          Design-Departures: none | <ADR-nnnn / waiver / R-nnn ids>
                        A departure is anything shipped that differs from a normative doc; it must cite the record that
                        accepts it (an ADR or a waiver), never be silent.
  audit                 every commit since `design_gate_since` that touched mapped code carries the trailers (catches --no-verify
                        and clones without the hook; runs in `make check`)
  selfcheck             every tracked code file is covered by a rule; every rule's domains exist (runs in `make check`)

The trailers are an attestation and an audit trail, not proof of reading: the rule is in AGENTS.md ("Design gate"),
and `git commit --no-verify` is a violation of it. Install the hook once per clone: `make hooks`.
"""
from __future__ import annotations

import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAP = ROOT / "machine/architecture_map.json"
CODE_ROOTS = ("crates/", "infra/", "packages/", "apps/", "spec/cddl/", "deploy/")
SKIP_PARTS = {"node_modules", "dist", "test-results", "target", "__pycache__", ".wrangler"}


def load() -> dict:
    return json.loads(MAP.read_text())


def match(rule_glob: str, path: str) -> bool:
    prefix = rule_glob[:-3] if rule_glob.endswith("/**") else None
    return path.startswith(prefix + "/") if prefix else fnmatch.fnmatch(path, rule_glob)


def domains_for(paths: list[str], amap: dict) -> dict[str, list[str]]:
    """domain -> paths that pulled it in"""
    out: dict[str, list[str]] = {}
    for p in paths:
        for rule in amap.get("code_paths", []):
            if match(rule["glob"], p):
                for dom in rule["domains"]:
                    out.setdefault(dom, []).append(p)
    return out


def staged() -> list[str]:
    r = subprocess.run(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMRD"], cwd=ROOT, capture_output=True, text=True, check=True)
    return [x for x in r.stdout.splitlines() if x]


def show(doms: dict[str, list[str]], amap: dict) -> None:
    for dom in sorted(doms):
        d = amap["domains"][dom]
        print(f"{dom}  ({len(doms[dom])} path(s))")
        for doc in d.get("docs", []):
            print(f"    {doc}")
        if d.get("adrs"):
            print("    " + ", ".join(d["adrs"]))


def cmd_needs(args: list[str]) -> int:
    amap = load()
    paths = args or staged()
    doms = domains_for(paths, amap)
    if not doms:
        print("no mapped code path: the design gate does not apply")
        return 0
    show(doms, amap)
    print("\nAfter reading them, commit with:\n  Design-Read: " + ", ".join(sorted(doms)) + "\n  Design-Departures: none")
    return 0


TRAILER = lambda name, text: re.search(rf"^{name}:[ \t]*(.+)$", text, re.M | re.I)  # noqa: E731


def cmd_commit_msg(file: str) -> int:
    text = Path(file).read_text()
    if re.match(r"^(Merge|Revert) ", text):
        return 0
    amap = load()
    paths = staged()
    doms = domains_for(paths, amap)
    if not doms:
        return 0
    missing = []
    read = TRAILER("Design-Read", text)
    have = {x.strip() for x in read.group(1).split(",")} if read else set()
    left = sorted(set(doms) - have)
    if left:
        missing.append("Design-Read must list every domain this change touches; missing: " + ", ".join(left))
    dep = TRAILER("Design-Departures", text)
    if not dep:
        missing.append("Design-Departures: none | <ADR/waiver/requirement ids that accept the departure>")
    elif dep.group(1).strip().lower() != "none" and not re.search(r"\b(ADR-\d{4}|PR-\d+|R-\d+|D-\d+|D\d+)\b", dep.group(1)):
        missing.append("Design-Departures names no accepting record (ADR-nnnn, PR-nnn, R-nnn, D-nnn); write 'none' only if nothing differs from the design")
    if not missing:
        return 0
    print("design gate: this commit changes code in mapped paths, so the design must be read first (AGENTS.md, 'Design gate').", file=sys.stderr)
    for m in missing:
        print("  - " + m, file=sys.stderr)
    print("\nRead these (python3 tools/design_gate.py needs):", file=sys.stderr)
    for dom in sorted(doms):
        print(f"  {dom}: " + ", ".join(Path(x).name for x in amap['domains'][dom].get('docs', [])), file=sys.stderr)
    return 1


def cmd_audit() -> int:
    amap = load()
    since = amap.get("design_gate_since")
    if not since:
        return 0
    r = subprocess.run(["git", "log", "--no-merges", "--format=%H%x00%h%x00%B%x01", f"{since}..HEAD"], cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        print("design_gate: cannot read history (shallow clone?); audit skipped")
        return 0
    bad = 0
    for rec in r.stdout.split("\x01"):
        if not rec.strip():
            continue
        full, short, msg = rec.strip("\n").split("\x00", 2)
        files = subprocess.run(["git", "diff-tree", "--no-commit-id", "--name-only", "-r", full], cwd=ROOT, capture_output=True, text=True).stdout.split()
        doms = domains_for(files, amap)
        if not doms:
            continue
        read = TRAILER("Design-Read", msg)
        have = {x.strip() for x in read.group(1).split(",")} if read else set()
        dep = TRAILER("Design-Departures", msg)
        if (set(doms) - have) or not dep:
            print(f"design_gate: commit {short} changed design-bound code without Design-Read/Design-Departures trailers")
            bad += 1
    if bad == 0:
        print("design_gate: audit OK (every design-bound commit since the gate carries its trailers)")
    return 1 if bad else 0


def tracked() -> list[str]:
    r = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True)
    return [x for x in r.stdout.splitlines() if x.startswith(CODE_ROOTS) and not (SKIP_PARTS & set(Path(x).parts))]


def cmd_selfcheck() -> int:
    amap = load()
    bad = 0
    for rule in amap.get("code_paths", []):
        for dom in rule["domains"]:
            if dom not in amap["domains"]:
                print(f"design_gate: rule {rule['glob']} names unknown domain {dom}")
                bad += 1
    uncovered = [p for p in tracked() if not any(match(r["glob"], p) for r in amap.get("code_paths", []))]
    for p in uncovered[:20]:
        print(f"design_gate: {p} is code but no code_paths rule covers it (add one to machine/architecture_map.json)")
    bad += len(uncovered)
    if bad == 0:
        print("design_gate: OK (every code path maps to a design domain)")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    cmd, rest = argv[1], argv[2:]
    if cmd == "needs":
        return cmd_needs(rest)
    if cmd == "commit-msg" and rest:
        return cmd_commit_msg(rest[0])
    if cmd == "selfcheck":
        return cmd_selfcheck()
    if cmd == "audit":
        return cmd_audit()
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
