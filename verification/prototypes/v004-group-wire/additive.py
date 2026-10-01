#!/usr/bin/env python3
"""Check that the registry edits of the group wire freeze are purely additive against the pre-freeze commit.

Prints JSON {base, files: {file: {additions, problems}}}; exit 1 on any problem. Needs git; if the base commit is not
available (a source export) the check is reported as skipped."""
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
BASE = "31e35af"   # last commit before the V-004 wire freeze
FILES = ["om-numeric-fields", "om-object-types", "om-domain-separation", "om-room-capabilities"]


def additive(old, new, path=""):
    bad = []
    if isinstance(old, dict):
        for k, v in old.items():
            if k not in new:
                bad.append(f"{path}/{k} removed")
            else:
                bad += additive(v, new[k], f"{path}/{k}")
    elif isinstance(old, list):
        if len(new) < len(old):
            bad.append(f"{path} shortened")
        else:
            for i, v in enumerate(old):
                bad += additive(v, new[i], f"{path}[{i}]")
    elif old != new:
        bad.append(f"{path} changed {old!r} -> {new!r}")
    return bad


def count(old, new):
    if isinstance(old, dict):
        return sum(count(old[k], new[k]) for k in old if k in new) + len(set(new) - set(old))
    if isinstance(old, list):
        return sum(count(a, b) for a, b in zip(old, new)) + max(len(new) - len(old), 0)
    return 0


def space_intent_copy_matches():
    """group.cddl repeats SpaceDelegatedIntent (space-publication.cddl) under a 'group-' name; they must stay identical."""
    def body(text, name):
        i = text.index(name + " = {")
        t = re.sub(r"\s+", " ", re.sub(r";[^\n]*", "", text[i:text.index("\n}", i)]))
        t = t.split("= ", 1)[1].replace("group-", "").replace(",", "").replace("capability-id", "C").replace("space-id", "bstr .size 32")
        return t.strip()
    sp = (REPO / "spec/cddl/space-publication.cddl").read_text()
    gp = (REPO / "spec/cddl/group.cddl").read_text()
    return body(sp, "SpaceDelegatedIntent") == body(gp, "group-space-delegated-intent")


def main():
    out = {"base": BASE, "files": {}, "space_delegated_intent_copy_identical": space_intent_copy_matches()}
    ok = out["space_delegated_intent_copy_identical"]
    try:
        for f in FILES:
            old = json.loads(subprocess.run(["git", "show", f"{BASE}:spec/registries/{f}.json"], cwd=REPO, capture_output=True, text=True, check=True).stdout)
            new = json.loads((REPO / f"spec/registries/{f}.json").read_text())
            bad = additive(old, new)
            out["files"][f] = {"additions": count(old, new), "problems": bad}
            ok &= not bad
    except (subprocess.CalledProcessError, FileNotFoundError):
        out["skipped"] = f"base commit {BASE} not available"
    print(json.dumps(out, indent=1))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
