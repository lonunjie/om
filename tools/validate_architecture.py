#!/usr/bin/env python3
"""Validate OM's machine-enforced architecture contract.

This is intentionally stricter than a documentation linter.  It fails when the
current architecture tree gains an unmapped file, authority definitions drift,
ADRs/questions become dangling, the pre-engineering lock is violated, or the
checkpoint integrity manifest no longer matches the bytes on disk.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
CURRENT_EXCLUDED_PARTS = {"history", "__pycache__"}
ADR_RE = re.compile(r"^ADR-(\d{4})-.*\.md$")
LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
ID_PATTERNS = {
    "machine/decisions.jsonl": re.compile(r"^D-\d{3}$"),
    "machine/requirements.jsonl": re.compile(r"^R-\d{3}$"),
    "machine/resolved_questions.jsonl": re.compile(r"^Q-\d{3}$"),
    "machine/architecture_design_queue.jsonl": re.compile(r"^A-\d{3}$"),
    "machine/architecture_verification_gates.jsonl": re.compile(r"^V-\d{3}$"),
    "machine/public_release_gates.jsonl": re.compile(r"^PR-\d{3}$"),
    "machine/deferred_product_questions.jsonl": re.compile(r"^DP-\d{3}$"),
}

errors: list[str] = []
warnings: list[str] = []


def fail(msg: str) -> None:
    errors.append(msg)


def warn(msg: str) -> None:
    warnings.append(msg)


def rel(p: Path) -> str:
    return p.relative_to(ROOT).as_posix()


def current_files() -> list[Path]:
    out = []
    for p in ROOT.rglob("*"):
        if not p.is_file():
            continue
        rp = p.relative_to(ROOT)
        if any(part in CURRENT_EXCLUDED_PARTS for part in rp.parts):
            continue
        out.append(p)
    return sorted(out)


def load_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as e:
        fail(f"JSON parse failed: {rel(path)}: {e}")
        return None


def load_jsonl(path: Path) -> list[dict]:
    rows = []
    seen = set()
    pat = ID_PATTERNS.get(rel(path))
    try:
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            try:
                obj = json.loads(line)
            except Exception as e:
                fail(f"JSONL parse failed: {rel(path)}:{n}: {e}")
                continue
            if not isinstance(obj, dict) or not isinstance(obj.get("id"), str):
                fail(f"JSONL record lacks string id: {rel(path)}:{n}")
                continue
            rid = obj["id"]
            if pat and not pat.match(rid):
                fail(f"Malformed id {rid!r} in {rel(path)}:{n}")
            if rid in seen:
                fail(f"Duplicate id {rid} in {rel(path)}")
            seen.add(rid)
            rows.append(obj)
    except Exception as e:
        fail(f"Could not read {rel(path)}: {e}")
    return rows


def check_json_parse() -> None:
    for p in current_files():
        if p.suffix == ".json":
            load_json(p)
        elif p.suffix == ".jsonl":
            # authoritative machine JSONL gets deeper checks below; still parse all.
            if rel(p) not in ID_PATTERNS:
                try:
                    for n, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
                        if line.strip(): json.loads(line)
                except Exception as e:
                    fail(f"JSONL parse failed: {rel(p)}:{n}: {e}")


def check_registry(contract: dict) -> None:
    reg_path = ROOT / contract["current_file_registry"]
    reg = load_json(reg_path)
    if not isinstance(reg, dict):
        return
    entries = reg.get("files", [])
    paths = [x.get("path") for x in entries if isinstance(x, dict)]
    if len(paths) != len(set(paths)):
        fail("machine/file_registry.json contains duplicate paths")
    actual = {rel(p) for p in current_files()}
    declared = set(paths)
    for p in sorted(actual - declared):
        fail(f"current file is not registered: {p}")
    for p in sorted(declared - actual):
        fail(f"registered current file does not exist: {p}")
    required_fields = {"path", "class", "domain", "license", "authority"}
    for i, ent in enumerate(entries):
        if not isinstance(ent, dict):
            fail(f"file_registry entry {i} is not an object")
            continue
        miss = required_fields - set(ent)
        if miss:
            fail(f"file_registry entry {ent.get('path', i)} missing {sorted(miss)}")


def check_machine_records():
    decisions = load_jsonl(ROOT / "machine/decisions.jsonl")
    reqs = load_jsonl(ROOT / "machine/requirements.jsonl")
    resolved = load_jsonl(ROOT / "machine/resolved_questions.jsonl")
    design = load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")
    verify = load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")
    release = load_jsonl(ROOT / "machine/public_release_gates.jsonl")
    deferred = load_jsonl(ROOT / "machine/deferred_product_questions.jsonl")

    if (ROOT / "machine/open_questions.jsonl").exists():
        fail("generic machine/open_questions.jsonl must not be current after typed-gate migration")

    expected = {f"Q-{i:03d}" for i in list(range(1,28)) + list(range(29,62))}
    got = {q.get("id") for q in resolved}
    if got != expected:
        fail(f"resolved_questions must cover former Q-001..Q-061 (except Q-028); missing={sorted(expected-got)} extra={sorted(got-expected)}")
    for q in resolved:
        if q.get("status") != "resolved" or not q.get("resolution"):
            fail(f"{q.get('id')} lacks resolved status/resolution")
    for q in design:
        if q.get("status") not in {"open","resolved","superseded"}:
            fail(f"{q.get('id')} invalid design-queue status")
        if not q.get("question"):
            fail(f"{q.get('id')} lacks design question")
    for g in verify:
        if g.get("status") not in {"open","passed","failed","waived_by_adr"}:
            fail(f"{g.get('id')} invalid verification status")
    for g in release:
        if g.get("status") not in {"open","passed","failed","waived_by_adr"}:
            fail(f"{g.get('id')} invalid release-gate status")
    return (
        {d["id"] for d in decisions}, {r["id"] for r in reqs},
        {q["id"] for q in resolved}, {q["id"] for q in design},
        {q["id"] for q in verify}, {q["id"] for q in release},
        {q["id"] for q in deferred}, design, verify
    )


def check_phase(contract: dict, design: list[dict], verify: list[dict]) -> None:
    authorized = contract.get("engineering_authorized")
    design_blockers = [q["id"] for q in design if q.get("blocks_engineering") and q.get("status") == "open"]
    verify_blockers = [q["id"] for q in verify if q.get("blocks_engineering") and q.get("status") != "passed"]
    if authorized and (design_blockers or verify_blockers):
        fail(f"engineering_authorized=true with blockers design={design_blockers} verification={verify_blockers}")
    if contract.get("phase") == "architecture_hardening_pre_engineering" and authorized is not False:
        fail("pre-engineering phase must have engineering_authorized=false")
    if not authorized:
        for name in ["SNAPSHOT.md", "ENGINEER_START_HERE.md"]:
            text = (ROOT / name).read_text(encoding="utf-8").lower()
            if "not authorized" not in text and "not yet authorized" not in text:
                fail(f"{name} does not visibly state that engineering is not authorized")


def check_identity(contract: dict) -> None:
    text = (ROOT / "IDENTITY_PROTOCOL.md").read_text(encoding="utf-8")
    forbidden = ["genesis_device_signing_public_key", "AccountID = HASH("]
    for token in forbidden:
        if token in text:
            fail(f"IDENTITY_PROTOCOL.md contains forbidden superseded AccountID construct: {token}")
    if "cryptographically secure random" not in text or "256" not in text:
        fail("IDENTITY_PROTOCOL.md does not visibly define random 256-bit AccountID")
    authority_docs = [
        "IDENTITY_PROTOCOL.md", "ACCOUNT_CONTINUITY_AND_CONTROL.md", "CRYPTOGRAPHY_AND_AGILITY.md",
        "EVENT_MODEL.md", "GROUP_CHAT_ARCHITECTURE.md", "MULTI_DEVICE_SYNC.md",
    ]
    for name in authority_docs:
        if re.search(r"\bAccountState\b", (ROOT / name).read_text(encoding="utf-8")):
            fail(f"{name} contains superseded AccountState authority term")


def check_provider_schema(contract: dict) -> None:
    files = sorted((ROOT / "protocol").glob("provider-capabilities*.json"))
    wanted = ROOT / contract["schema_singletons"]["ProviderCapabilities"]
    if files != [wanted]:
        fail("ProviderCapabilities must have exactly one current schema: " + ", ".join(rel(p) for p in files))
    for p in (ROOT / "protocol").glob("*.json"):
        if re.search(r"-v\d+\.", p.name):
            fail(f"version-suffixed current protocol schema must move to history: {rel(p)}")


def check_adrs(decision_ids: set[str]) -> None:
    files = list((ROOT / "decisions").glob("ADR-*.md"))
    nums: dict[str, list[str]] = {}
    for p in files:
        m = ADR_RE.match(p.name)
        if not m:
            fail(f"malformed ADR filename: {rel(p)}")
            continue
        aid = f"ADR-{m.group(1)}"
        nums.setdefault(aid, []).append(p.name)
    for aid, names in nums.items():
        if len(names) != 1:
            fail(f"duplicate ADR number {aid}: {names}")
    idx = (ROOT / "decisions/INDEX.md").read_text(encoding="utf-8")
    for aid, names in nums.items():
        if names[0] not in idx:
            fail(f"ADR not indexed: {names[0]}")
    # ADR IDs do not have to appear in decisions.jsonl one-for-one; they are separate ledgers.


def check_architecture_map(resolved_ids: set[str], design_ids: set[str], verify_ids: set[str], release_ids: set[str], deferred_ids: set[str]) -> None:
    amap = load_json(ROOT / "machine/architecture_map.json")
    if not isinstance(amap, dict): return
    known_adrs = {f"ADR-{ADR_RE.match(p.name).group(1)}" for p in (ROOT/"decisions").glob("ADR-*.md") if ADR_RE.match(p.name)}
    for domain, obj in amap.get("domains", {}).items():
        for key in ("docs", "schemas"):
            for path in obj.get(key, []):
                if not (ROOT/path).exists(): fail(f"architecture_map {domain}.{key} references missing file {path}")
        for aid in obj.get("adrs", []):
            if aid not in known_adrs: fail(f"architecture_map {domain} references missing ADR {aid}")
        for qid in obj.get("resolved_questions", []):
            if qid not in resolved_ids: fail(f"architecture_map {domain} references missing resolved question {qid}")
        for qid in obj.get("design_queue", []):
            if qid not in design_ids: fail(f"architecture_map {domain} references missing design item {qid}")
        for qid in obj.get("verification_gates", []):
            if qid not in verify_ids: fail(f"architecture_map {domain} references missing verification gate {qid}")
        for qid in obj.get("release_gates", []):
            if qid not in release_ids: fail(f"architecture_map {domain} references missing release gate {qid}")
        for qid in obj.get("deferred_questions", []):
            if qid not in deferred_ids: fail(f"architecture_map {domain} references missing deferred question {qid}")


def check_project_identity_and_legal_boundary() -> None:
    naming = (ROOT / "NAMING.md").read_text(encoding="utf-8")
    for phrase in ["Open-source Messenger", "OpenMessenger", "OwnMessenger", "OurMessenger", "OneMessenger", "OhMessenger"]:
        if phrase not in naming:
            fail(f"NAMING.md missing canonical OM meaning: {phrase}")
    # Country-specific research may remain in history only; current canonical architecture must not make one country the protocol home.
    legal = (ROOT / "LEGAL_AND_OPERATIONAL_BOUNDARIES.md").read_text(encoding="utf-8").lower()
    if "protocol home jurisdiction" not in legal and "protocol-home" not in legal:
        fail("legal boundary does not state jurisdiction-neutral protocol position")
    if "payment processor" not in legal or "merchant-of-record" not in legal:
        fail("legal boundary does not state core commerce/operator separation")


def check_required_files(contract: dict) -> None:
    paths = set(contract.get("constitution_files", [])) | set(contract.get("machine_truth_files", [])) | set(contract.get("required_new_hardening_docs", []))
    paths |= {"LICENSE.md", "CONTRIBUTING.md", "tools/validate_architecture.py", "tools/build_manifest.py", "ARCHITECTURE_RESOLUTIONS_20260923.md", "MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md", "NAMING.md", "machine/resolved_questions.jsonl", "machine/architecture_design_queue.jsonl", "machine/architecture_verification_gates.jsonl", "machine/public_release_gates.jsonl", "machine/deferred_product_questions.jsonl"}
    for path in sorted(paths):
        if not (ROOT/path).exists(): fail(f"required architecture file missing: {path}")
    for license_name in ["Apache-2.0.txt", "MPL-2.0.txt", "AGPL-3.0-or-later.txt", "CC-BY-SA-4.0.txt"]:
        if not (ROOT/"LICENSES"/license_name).exists(): fail(f"license text missing: LICENSES/{license_name}")


def check_markdown_links() -> None:
    for p in current_files():
        if p.suffix.lower() != ".md": continue
        text = p.read_text(encoding="utf-8")
        for raw in LINK_RE.findall(text):
            target = raw.strip()
            if target.startswith("<") and target.endswith(">"):
                target = target[1:-1]
            # Markdown title suffix: (path "title")
            target = target.split(' "', 1)[0].split(" '", 1)[0]
            if not target or target.startswith(("#", "http://", "https://", "mailto:", "data:", "sandbox:")):
                continue
            target = unquote(target.split("#",1)[0].split("?",1)[0])
            if not target: continue
            resolved = (p.parent / target).resolve()
            try: resolved.relative_to(ROOT.resolve())
            except ValueError:
                fail(f"markdown link escapes repo: {rel(p)} -> {raw}")
                continue
            if not resolved.exists():
                fail(f"broken local markdown link: {rel(p)} -> {raw}")


def sha256(path: Path) -> str:
    h=hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda:f.read(1024*1024), b""): h.update(b)
    return h.hexdigest()


def check_manifest(contract: dict) -> None:
    mp = ROOT / "MANIFEST.json"
    man = load_json(mp)
    if not isinstance(man, dict): return
    if man.get("snapshot") != contract.get("snapshot"):
        fail(f"MANIFEST snapshot {man.get('snapshot')} != contract snapshot {contract.get('snapshot')}")
    excludes = set(man.get("manifest_excludes", []))
    expected_excludes = set(contract.get("manifest_excludes", []))
    if excludes != expected_excludes:
        fail(f"MANIFEST excludes {sorted(excludes)} != contract excludes {sorted(expected_excludes)}")
    actual: dict[str, Path] = {}
    for p in ROOT.rglob("*"):
        if p.is_file() and "__pycache__" not in p.parts:
            rp=rel(p)
            if rp not in excludes: actual[rp]=p
    rows=man.get("files", [])
    indexed={r.get("path"):r for r in rows if isinstance(r, dict)}
    if len(indexed) != len(rows): fail("MANIFEST has duplicate or malformed path entries")
    for path in sorted(actual.keys()-indexed.keys()): fail(f"MANIFEST missing file: {path}")
    for path in sorted(indexed.keys()-actual.keys()): fail(f"MANIFEST references absent file: {path}")
    for path in sorted(actual.keys() & indexed.keys()):
        p=actual[path]; r=indexed[path]
        if r.get("bytes") != p.stat().st_size: fail(f"MANIFEST byte count mismatch: {path}")
        if r.get("sha256") != sha256(p): fail(f"MANIFEST sha256 mismatch: {path}")


def validate_0718_extension_guardrails() -> None:
    schema_path = ROOT / "protocol/conversation-event.logical.schema.json"
    schema = load_json(schema_path)
    if not isinstance(schema, dict):
        return
    props = schema.get("properties", {})
    required = set(schema.get("required", []))
    if "author_principal" not in props or "author_principal" not in required:
        fail("07:18 guard: ConversationEvent must require typed author_principal")
    if "author_endpoint_id" not in props or "author_endpoint_id" not in required:
        fail("07:18 guard: ConversationEvent must require author_endpoint_id")
    if "author_account_id" in props or "author_device_id" in props:
        fail("07:18 guard: ConversationEvent must not reintroduce human-only author_account_id/author_device_id")

    ptext = (ROOT / "PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md").read_text(encoding="utf-8")
    for phrase in ("no ambient", "Plaintext", "network", "canonical"):
        if phrase.lower() not in ptext.lower():
            fail(f"07:18 guard: plugin architecture lost required boundary phrase {phrase!r}")


def main() -> int:
    validate_0718_extension_guardrails()
    contract = load_json(ROOT / "machine/repository_contract.json")
    if not isinstance(contract, dict):
        print("architecture validation FAILED: repository_contract unreadable", file=sys.stderr); return 1
    check_json_parse()
    _, _, resolved_ids, design_ids, verify_ids, release_ids, deferred_ids, design, verify = check_machine_records()
    decision_ids={}
    check_required_files(contract)
    check_registry(contract)
    check_phase(contract, design, verify)
    check_identity(contract)
    check_provider_schema(contract)
    check_adrs(set())
    check_architecture_map(resolved_ids, design_ids, verify_ids, release_ids, deferred_ids)
    check_project_identity_and_legal_boundary()
    check_markdown_links()
    check_manifest(contract)

    if warnings:
        for w in warnings: print(f"WARN: {w}")
    if errors:
        for e in errors: print(f"ERROR: {e}", file=sys.stderr)
        print(f"architecture validation FAILED: {len(errors)} error(s), {len(warnings)} warning(s)", file=sys.stderr)
        return 1
    print(f"architecture validation OK: {len(current_files())} current files; design_open={sum(1 for q in design if q.get('status')=='open')}; verification_open={sum(1 for q in verify if q.get('status')!='passed')}; engineering_authorized={contract.get('engineering_authorized')}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
