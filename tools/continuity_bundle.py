#!/usr/bin/env python3
"""Protocol Continuity Bundle tooling (V-017, PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md §3-4).

  python3 tools/continuity_bundle.py build [--ref HEAD] [--out design-drops]   # deterministic .tar.gz
  python3 tools/continuity_bundle.py verify <bundle.tar.gz>                    # offline integrity check
  python3 tools/continuity_bundle.py check-registries [--base REF]             # immutability of assigned IDs
  python3 tools/continuity_bundle.py check-manifest [--ref HEAD]               # manifest matches the repo

A bundle must let someone interpret published canonical bytes with no live OM server: normative specs and
schemas, canonical-encoding rules, vectors, registry and ledger snapshots, license texts, and the source
revision, all committed to one integrity root. `verify` uses only the bytes in the bundle.

Which docs are "normative" is not a basename allowlist (that missed real semantic docs — F-8, 2026-09-26
independent review) but an explicit, versioned manifest: MANIFEST_PATH below. `select`/`build` bundle exactly
its members (plus what INCLUDE already covers); `verify` requires every member and rejects an unresolved
normative markdown/CDDL/JSON reference from a bundled doc unless it is in the manifest's `exceptions`;
`check-manifest` recomputes the doc set from the repo tree and fails if the manifest has drifted from it.
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
import posixpath

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_VERSION = "om-continuity-bundle/1"
MANIFEST_PATH = "spec/continuity/om-crypto-v1-bundle-manifest.json"
# What a bundle carries (path prefixes or exact paths in the git tree).
INCLUDE = (
    "spec/", "verification/vectors/", "verification/cbor/fixtures.json", "LICENSES/", "LICENSE.md", "REUSE.toml",
    "machine/decisions.jsonl", "machine/requirements.jsonl", "machine/resolved_questions.jsonl",
    "machine/threat_register.jsonl", "machine/architecture_verification_gates.jsonl",
    "decisions/", "CHANGELOG.md", "SNAPSHOT.md",
)
LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")


def load_manifest(text: str) -> dict:
    return json.loads(text)


def manifest_doc_members(manifest: dict) -> list[str]:
    return sorted(manifest["members"]["architecture_and_constitution_docs"])


def normative_refs(text: str, doc_path: str) -> list[str]:
    """Relative links inside `doc_path` that point at a .md/.cddl/.json file in the repo tree."""
    here = posixpath.dirname(doc_path)
    out = []
    for target in LINK_RE.findall(text):
        if target.startswith(("#", "http://", "https://", "mailto:", "data:", "sandbox:")):
            continue
        path = target.split("#", 1)[0]
        if not path or not path.endswith((".md", ".cddl", ".json")):
            continue
        out.append(posixpath.normpath(posixpath.join(here, path)) if here else posixpath.normpath(path))
    return out


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


def git_bytes_or_worktree(ref: str, path: str) -> bytes:
    """Like git_bytes, but falls back to the worktree file for a path not yet committed at `ref`.

    Only the manifest itself should hit the fallback in practice (right after it is added, before the first
    commit that registers it); every other selected path is already tracked. This keeps `build` reproducible
    from a commit in the normal case while not hard-failing on the manifest's own bootstrap commit.
    """
    try:
        return git_bytes(ref, path)
    except subprocess.CalledProcessError:
        return (ROOT / path).read_bytes()


def integrity_root(rows: list[dict]) -> str:
    h = hashlib.sha256(b"om/continuity-bundle/integrity-root/v1\0")
    for r in sorted(rows, key=lambda x: x["path"]):
        h.update(r["path"].encode() + b"\0" + r["sha256"].encode() + b"\n")
    return h.hexdigest()


def select(ref: str, manifest: dict) -> list[str]:
    files = set(git("ls-tree", "-r", "--name-only", ref).splitlines()) | {MANIFEST_PATH}
    members = manifest_doc_members(manifest)
    missing = [m for m in members if m not in files]
    if missing:
        raise SystemExit("manifest member(s) not in tree " + ref + ": " + ", ".join(missing))
    out = [f for f in files if f.startswith(INCLUDE) or f in INCLUDE]
    out += members
    return sorted(set(out))


def cmd_build(a) -> int:
    sha = git("rev-parse", a.ref).strip()
    snap = json.loads(git_bytes(sha, "machine/repository_contract.json"))["snapshot"]
    manifest = load_manifest(git_bytes_or_worktree(sha, MANIFEST_PATH).decode())
    rows, blobs = [], {}
    for f in select(sha, manifest):
        b = git_bytes_or_worktree(sha, f)
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
        "security_status_at_publication": {"snapshot": snap,
                                           "engineering_authorized": bool(json.loads((ROOT / "machine/repository_contract.json").read_text())["engineering_authorized"]),
                                           "note": "design continuity bundle; engineering authorization is scope-granular and no production release is implied"},
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

        # F-8: the bundle must carry every manifest-listed normative doc, and every normative doc/spec/CDDL
        # reference inside a bundled normative doc must resolve inside the bundle or be a checked exception.
        bundled_paths = {r["path"] for r in bundle["files"]}
        manifest_member = members.get(MANIFEST_PATH)
        if manifest_member is None:
            errors.append(f"missing {MANIFEST_PATH} (normative doc manifest)")
        else:
            manifest = load_manifest(t.extractfile(manifest_member).read().decode())
            doc_members = manifest_doc_members(manifest)
            exceptions = {e["target"] for e in manifest.get("exceptions", [])}
            errors += [f"manifest member missing from bundle {p}" for p in doc_members if p not in bundled_paths]
            for p in doc_members:
                m = members.get(p)
                if m is None:
                    continue
                text = t.extractfile(m).read().decode("utf-8", errors="replace")
                for ref in normative_refs(text, p):
                    if ref in bundled_paths or ref in exceptions:
                        continue
                    errors.append(f"unresolved normative reference {p} -> {ref} (not bundled, not in manifest exceptions)")
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


# Fields that name a record (numeric keys, type ids, labels, names). Only these must be unique among sibling
# records; bounds, sizes, versions and references (for example two fields both bounded to 16 bytes, or a
# profile's hash_alg equal to its own id) legitimately repeat values.
IDENTIFIER_FIELDS = ("id", "key", "type_id", "label", "object_type", "name")
# sibling record lists that share one identifier namespace: a struct's reserved keys may not be reused by its fields
SHARED_NAMESPACES = (("fields", "reserved"),)


def identifier_reuse(obj, path="") -> list[str]:
    """Duplicate identifier values among sibling records in one list (or in lists sharing a namespace)."""
    out = []
    if isinstance(obj, dict):
        groups = [[k] for k, v in obj.items() if isinstance(v, list)]
        for shared in SHARED_NAMESPACES:
            members = [k for k in shared if isinstance(obj.get(k), list)]
            if len(members) > 1:
                groups = [g for g in groups if g[0] not in members] + [members]
        for g in groups:
            recs = [(k, r) for k in g for r in obj[k] if isinstance(r, dict)]
            for fld in IDENTIFIER_FIELDS:
                seen = {}
                for k, r in recs:
                    v = r.get(fld)
                    if v is None or isinstance(v, (dict, list, bool)):
                        continue
                    if v in seen:
                        out.append(f"identifier {fld}={v!r} reused in {path}/{k} (also in {path}/{seen[v]})")
                    else:
                        seen[v] = k
        for k, v in obj.items():
            out += identifier_reuse(v, f"{path}/{k}")
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            out += identifier_reuse(v, f"{path}[{i}]")
    return out


def cmd_check_registries(a) -> int:
    # ADR-0166: tags recording a non-adopted design line are provenance only, never the immutability baseline.
    lines = ROOT / "machine/checkpoint_lines.json"
    skip = {r["tag"] for r in json.loads(lines.read_text()).get("not_adopted", [])} if lines.exists() else set()
    tags = [t for t in git("tag", "-l", "checkpoint/*", "--sort=-creatordate").split() if t not in skip]
    base = a.base or (tags or ["HEAD~1"])[0]
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
            elif new[k] != v and not (isinstance(v, str) and v.startswith("reserved")):
                # a 'reserved...' placeholder (ADR-0119) may be filled once; anything else is immutable
                errors.append(f"{f}: assigned entry {k} changed {v!r} -> {new[k]!r}")
        # an assigned identifier must not be reused for a different record
        errors += [f"{f}: {e}" for e in identifier_reuse(json.loads((ROOT / f).read_text()))]
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    print(f"registry immutability vs {base}: {'FAILED' if errors else 'OK'} ({len(errors)} problem(s))")
    return 1 if errors else 0


def cmd_check_manifest(a) -> int:
    """Recompute the normative-doc set from the repo and fail if MANIFEST_PATH has drifted from it (F-8)."""
    errors = []
    if a.ref:
        sha = git("rev-parse", a.ref).strip()
        tree = git("ls-tree", "-r", "--name-only", sha).splitlines()
        manifest = load_manifest(git_bytes_or_worktree(sha, MANIFEST_PATH).decode())
        exists = lambda p: p in tree  # noqa: E731
    else:
        # Working-tree mode is intentionally Git-independent so an exported design-drop ZIP can
        # validate its continuity manifest before it has been imported into a repository.
        tree = [p.relative_to(ROOT).as_posix() for base in (ROOT / "docs/architecture", ROOT / "docs/constitution")
                if base.is_dir() for p in base.rglob("*.md") if p.is_file()]
        manifest = load_manifest((ROOT / MANIFEST_PATH).read_text())
        exists = lambda p: (ROOT / p).is_file()  # noqa: E731

    actual = sorted(p for p in tree if (p.startswith("docs/architecture/") or p.startswith("docs/constitution/"))
                     and p.endswith(".md"))
    listed = set(manifest_doc_members(manifest))
    errors += [f"reachable normative doc missing from manifest: {p}" for p in actual if p not in listed]
    errors += [f"manifest lists a doc no longer in the repo: {p}" for p in listed if p not in set(actual)]
    for e in manifest.get("exceptions", []):
        if not exists(e["target"]):
            errors.append(f"exception target no longer exists: {e['target']}")

    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    where = a.ref or "working tree"
    print(f"manifest check vs {where}: {'FAILED' if errors else 'OK'} ({len(listed)} doc(s), {len(errors)} problem(s))")
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
    m = sub.add_parser("check-manifest")
    m.add_argument("--ref", default=None, help="check a git ref instead of the working tree")
    a = ap.parse_args()
    return {"build": cmd_build, "verify": cmd_verify, "check-registries": cmd_check_registries,
            "check-manifest": cmd_check_manifest}[a.cmd](a)


if __name__ == "__main__":
    raise SystemExit(main())
