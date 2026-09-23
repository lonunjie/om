#!/usr/bin/env python3
"""Protocol Continuity Bundle tooling (V-017, PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md §3-4).

  python3 tools/continuity_bundle.py build [--ref HEAD] [--out design-drops]   # deterministic .tar.gz
  python3 tools/continuity_bundle.py verify <bundle.tar.gz>                    # offline integrity check
  python3 tools/continuity_bundle.py check-registries [--base REF]             # immutability of assigned IDs

A bundle must let someone interpret published canonical bytes with no live OM server: normative specs and
schemas, canonical-encoding rules, vectors, registry and ledger snapshots, license texts, and the source
revision, all committed to one integrity root. `verify` uses only the bytes in the bundle.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_VERSION = "om-continuity-bundle/1"
# What a bundle carries (path prefixes or exact paths in the git tree).
INCLUDE = (
    "spec/", "verification/vectors/", "verification/cbor/fixtures.json", "LICENSES/", "LICENSE.md", "REUSE.toml",
    "machine/decisions.jsonl", "machine/requirements.jsonl", "machine/resolved_questions.jsonl",
    "machine/threat_register.jsonl", "machine/architecture_verification_gates.jsonl",
    "decisions/", "CHANGELOG.md", "SNAPSHOT.md",
)
NORMATIVE_DOCS = ("CANONICAL_ENCODING.md", "CRYPTOGRAPHY_AND_AGILITY.md", "PROTOCOL_EXTENSIBILITY.md",
                  "PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md", "IDENTITY_PROTOCOL.md",
                  "ACCOUNT_CONTINUITY_AND_CONTROL.md", "TIME_AND_EXPIRY_SEMANTICS.md")
# Ledgers whose ids may be amended but never deleted or re-used.
LEDGERS = ("machine/decisions.jsonl", "machine/requirements.jsonl", "machine/resolved_questions.jsonl",
           "machine/threat_register.jsonl", "machine/architecture_verification_gates.jsonl",
           "machine/public_release_gates.jsonl", "machine/architecture_design_queue.jsonl",
           "machine/deferred_product_questions.jsonl")
MUTABLE_REGISTRY_FIELDS = {"description", "notes", "note", "status", "deprecated", "comment"}


def git(*a: str) -> str:
    return subprocess.run(["git", *a], cwd=ROOT, check=True, capture_output=True, text=True).stdout


def git_bytes(ref: str, path: str) -> bytes:
    return subprocess.run(["git", "show", f"{ref}:{path}"], cwd=ROOT, check=True, capture_output=True).stdout


def integrity_root(rows: list[dict]) -> str:
    h = hashlib.sha256(b"om/continuity-bundle/integrity-root/v1\0")
    for r in sorted(rows, key=lambda x: x["path"]):
        h.update(r["path"].encode() + b"\0" + r["sha256"].encode() + b"\n")
    return h.hexdigest()


def select(ref: str) -> list[str]:
    files = git("ls-tree", "-r", "--name-only", ref).splitlines()
    out = [f for f in files if f.startswith(INCLUDE) or f in INCLUDE]
    out += [f for f in files if f.rsplit("/", 1)[-1] in NORMATIVE_DOCS]
    return sorted(set(out))


def cmd_build(a) -> int:
    sha = git("rev-parse", a.ref).strip()
    snap = json.loads(git_bytes(sha, "machine/repository_contract.json"))["snapshot"]
    rows, blobs = [], {}
    for f in select(sha):
        b = git_bytes(sha, f)
        blobs[f] = b
        rows.append({"path": f, "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()})
    kind = lambda p: ("schema" if p.startswith("spec/") else "vector" if "vectors/" in p or p.endswith("fixtures.json")
                      else "license" if p.startswith("LICENSES/") or p.startswith("LICENSE") or p == "REUSE.toml"
                      else "ledger" if p.startswith("machine/") else "decision" if p.startswith("decisions/")
                      else "normative_doc")
    bundle = {
        "bundle_version": BUNDLE_VERSION,
        "protocol_profile_id": "om/v1-draft",
        "canonical_encoding_profile": "om-dcbor-v1 (CANONICAL_ENCODING.md)",
        "normative_artifacts": [r for r in rows if kind(r["path"]) in ("schema", "normative_doc")],
        "conformance_vectors": [r for r in rows if kind(r["path"]) == "vector"],
        "registry_snapshots": [r for r in rows if kind(r["path"]) in ("ledger", "decision")
                               or r["path"].startswith("spec/registries/")],
        "crypto_profile_snapshot": next(({"path": r["path"], "sha256": r["sha256"]} for r in rows
                                         if r["path"].endswith("crypto-profile.logical.schema.json")), None),
        "security_status_at_publication": {"snapshot": snap, "engineering_authorized": False,
                                           "note": "pre-engineering design bundle; no production release"},
        "license_inventory": [r for r in rows if kind(r["path"]) == "license"],
        "reference_source_revision": {"vcs": "git", "commit": sha},
        "migration_guidance": {"path": "CHANGELOG.md"},
        "release_authentication": None,
        "integrity_root": integrity_root(rows),
        "files": rows,
    }
    name = f"om-continuity-bundle-{sha[:10]}"
    out = (ROOT / a.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    buf = io.BytesIO()
    mtime = int(git("show", "-s", "--format=%ct", sha))
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.PAX_FORMAT) as t:
        def add(path: str, data: bytes):
            ti = tarfile.TarInfo(f"{name}/{path}")
            ti.size, ti.mtime, ti.mode, ti.uid, ti.gid, ti.uname, ti.gname = len(data), mtime, 0o644, 0, 0, "", ""
            t.addfile(ti, io.BytesIO(data))
        add("BUNDLE.json", (json.dumps(bundle, indent=1, sort_keys=True) + "\n").encode())
        for f in sorted(blobs):
            add(f, blobs[f])
    import gzip
    gz = gzip.compress(buf.getvalue(), compresslevel=9, mtime=0)
    target = out / f"{name}.tar.gz"
    target.write_bytes(gz)
    print(f"built {target.name}: {len(rows)} files, integrity_root {bundle['integrity_root']}")
    print(f"sha256 {hashlib.sha256(gz).hexdigest()}")
    return 0


def cmd_verify(a) -> int:
    errors = []
    with tarfile.open(a.bundle, "r:gz") as t:
        members = {m.name.split("/", 1)[1]: m for m in t.getmembers() if m.isfile() and "/" in m.name}
        for n in members:
            if n.startswith("/") or ".." in n.split("/"):
                errors.append(f"unsafe path {n}")
        bundle = json.loads(t.extractfile(members["BUNDLE.json"]).read())
        req = ["bundle_version", "protocol_profile_id", "normative_artifacts", "registry_snapshots",
               "license_inventory", "integrity_root"]
        errors += [f"missing field {k}" for k in req if k not in bundle]
        if bundle.get("bundle_version") != BUNDLE_VERSION:
            errors.append(f"unknown bundle_version {bundle.get('bundle_version')}")
        for r in bundle["files"]:
            m = members.get(r["path"])
            if m is None:
                errors.append(f"missing {r['path']}")
                continue
            b = t.extractfile(m).read()
            if len(b) != r["bytes"] or hashlib.sha256(b).hexdigest() != r["sha256"]:
                errors.append(f"hash mismatch {r['path']}")
            if r["path"].endswith(".json"):
                try:
                    json.loads(b)
                except Exception as e:
                    errors.append(f"unparseable {r['path']}: {e}")
            if r["path"].endswith(".jsonl"):
                for i, line in enumerate(b.decode().splitlines(), 1):
                    if line.strip():
                        try:
                            json.loads(line)
                        except Exception as e:
                            errors.append(f"unparseable {r['path']}:{i}: {e}")
        extra = set(members) - {r["path"] for r in bundle["files"]} - {"BUNDLE.json"}
        errors += [f"uncommitted file in bundle {x}" for x in sorted(extra)]
        if integrity_root(bundle["files"]) != bundle["integrity_root"]:
            errors.append("integrity_root mismatch")
        if not any(r["path"].startswith("LICENSES/") for r in bundle["license_inventory"]):
            errors.append("no license texts")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    print(f"bundle {'INVALID' if errors else 'OK'}: {len(bundle.get('files', []))} files, root {bundle.get('integrity_root')}")
    return 1 if errors else 0


def flatten(obj, prefix="") -> dict:
    out = {}
    if isinstance(obj, dict):
        for k, v in obj.items():
            if k in MUTABLE_REGISTRY_FIELDS:
                continue
            out.update(flatten(v, f"{prefix}/{k}"))
    elif isinstance(obj, list):
        # lists of records keyed by id/name/key are compared by that key, not position
        keyed = all(isinstance(x, dict) and any(k in x for k in ("id", "key", "name", "label")) for x in obj) and obj
        for i, v in enumerate(obj):
            k = next((v[k] for k in ("id", "key", "name", "label") if k in v), i) if keyed else i
            out.update(flatten(v, f"{prefix}[{k}]"))
    else:
        out[prefix] = obj
    return out


def cmd_check_registries(a) -> int:
    base = a.base or (git("tag", "-l", "checkpoint/*", "--sort=-creatordate").split() or ["HEAD~1"])[0]
    errors = []
    for f in LEDGERS:
        try:
            old = {json.loads(l)["id"] for l in git_bytes(base, f).decode().splitlines() if l.strip()}
        except subprocess.CalledProcessError:
            continue
        new = {json.loads(l)["id"] for l in (ROOT / f).read_text().splitlines() if l.strip()}
        errors += [f"{f}: id {i} was deleted (ids are never removed; mark superseded instead)" for i in sorted(old - new)]
    old_adrs = {re.match(r"ADR-(\d{4})", p.split("/")[-1]).group(1): p.split("/")[-1]
                for p in git("ls-tree", "--name-only", f"{base}:decisions").split() if p.startswith("ADR-")}
    new_adrs = {re.match(r"ADR-(\d{4})", p.name).group(1): p.name for p in (ROOT / "decisions").glob("ADR-*.md")}
    for n, name in old_adrs.items():
        if n not in new_adrs:
            errors.append(f"ADR-{n} was deleted or renumbered ({name})")
    try:
        reg_files = git("ls-tree", "-r", "--name-only", base, "spec/registries").split()
    except subprocess.CalledProcessError:
        reg_files = []
    for f in reg_files:
        if not f.endswith(".json"):
            continue
        if not (ROOT / f).exists():
            errors.append(f"{f}: registry file deleted")
            continue
        old, new = flatten(json.loads(git_bytes(base, f))), flatten(json.loads((ROOT / f).read_text()))
        for k, v in old.items():
            if k not in new:
                errors.append(f"{f}: assigned entry {k} removed")
            elif new[k] != v:
                errors.append(f"{f}: assigned entry {k} changed {v!r} -> {new[k]!r}")
        # an assigned value must not be reused for a different name
        rev = {}
        for k, v in new.items():
            if isinstance(v, int) and not isinstance(v, bool):
                scope = k.rsplit("/", 1)[0]
                if (scope, v) in rev and rev[(scope, v)] != k:
                    errors.append(f"{f}: value {v} reused by {k} and {rev[(scope, v)]}")
                rev[(scope, v)] = k
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    print(f"registry immutability vs {base}: {'FAILED' if errors else 'OK'} ({len(errors)} problem(s))")
    return 1 if errors else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--ref", default="HEAD")
    b.add_argument("--out", default="design-drops")
    v = sub.add_parser("verify")
    v.add_argument("bundle")
    c = sub.add_parser("check-registries")
    c.add_argument("--base")
    a = ap.parse_args()
    return {"build": cmd_build, "verify": cmd_verify, "check-registries": cmd_check_registries}[a.cmd](a)


if __name__ == "__main__":
    raise SystemExit(main())
