#!/usr/bin/env python3
"""Validate OM's machine-enforced architecture contract.

This is intentionally stricter than a documentation linter.  It fails when the
current architecture tree gains an unmapped file, authority definitions drift,
ADRs/questions become dangling, the pre-engineering lock is violated, or the
checkpoint integrity manifest no longer matches the bytes on disk.
"""
from __future__ import annotations

import hashlib
import gzip
import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
# Not part of the architecture tree: git history replaces history/, drops are raw inputs,
# and build/tool caches are local.
CURRENT_EXCLUDED_PARTS = {"history", "__pycache__", "design-drops", ".git", "node_modules", "target", ".venv", ".pytest_cache", ".hypothesis", ".claude"}
CURRENT_EXCLUDED_FILES = {".DS_Store", "OM_EXPORT.json"}
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
    "machine/threat_register.jsonl": re.compile(r"^T-\d{3}$"),
}

errors: list[str] = []
warnings: list[str] = []


def fail(msg: str) -> None:
    errors.append(msg)


def warn(msg: str) -> None:
    warnings.append(msg)


def rel(p: Path) -> str:
    return p.relative_to(ROOT).as_posix()


_CURRENT: list[Path] | None = None


def current_files() -> list[Path]:
    """Tracked + untracked-not-ignored files in a git checkout; a filtered walk in an export."""
    global _CURRENT
    if _CURRENT is not None:
        return _CURRENT
    try:
        out = subprocess.run(["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT,
                             check=True, capture_output=True, text=True).stdout.splitlines()
        cands = [ROOT / f for f in out]
    except (subprocess.CalledProcessError, FileNotFoundError):
        cands = list(ROOT.rglob("*"))
    res = []
    for p in cands:
        if not p.is_file():
            continue
        rp = p.relative_to(ROOT)
        if any(part in CURRENT_EXCLUDED_PARTS for part in rp.parts) or rp.name in CURRENT_EXCLUDED_FILES or p.suffix == ".pyc":
            continue
        res.append(p)
    _CURRENT = sorted(res)
    return _CURRENT


def D(name: str) -> Path:
    """Resolve a document by repo path or by its (unique) basename, independent of folder layout."""
    if "/" in name or (ROOT / name).is_file():
        return ROOT / name
    hits = [p for p in current_files() if p.name == name]
    if len(hits) != 1:
        fail(f"document {name!r} must exist exactly once (found {len(hits)})")
        return ROOT / name
    return hits[0]


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
    skip = set(contract.get("manifest_excludes", []))
    actual = {rel(p) for p in current_files()} - skip
    declared = set(paths)
    for p in sorted(actual - declared):
        fail(f"current file is not registered: {p} (run: python3 tools/om_registry.py sync)")
    for p in sorted(declared - actual):
        if p.startswith(".github/") and not (ROOT / ".git").exists():
            continue  # repo-only CI plumbing is not shipped in exported drops
        fail(f"registered current file does not exist: {p}")
    required_fields = {"path", "class", "domain", "license", "authority"}
    for i, ent in enumerate(entries):
        if not isinstance(ent, dict):
            fail(f"file_registry entry {i} is not an object")
            continue
        miss = required_fields - set(ent)
        if miss:
            fail(f"file_registry entry {ent.get('path', i)} missing {sorted(miss)}")
        elif ent.get("domain") == "unassigned":
            fail(f"file_registry entry {ent['path']} has domain 'unassigned'")


def check_dangerous_algorithm_contracts() -> None:
    p = ROOT / "machine/dangerous_algorithm_contracts.json"
    data = load_json(p)
    if not isinstance(data, dict):
        return
    if data.get("format") != "om.dangerous-algorithm-contracts/1":
        fail("dangerous_algorithm_contracts.json has unexpected format")
    algs = data.get("algorithms")
    if not isinstance(algs, list) or len(algs) != 8:
        fail("dangerous_algorithm_contracts.json must contain exactly 8 canonical algorithms")
        return
    ids = [a.get("id") for a in algs if isinstance(a, dict)]
    if len(ids) != len(set(ids)):
        fail("dangerous_algorithm_contracts.json contains duplicate algorithm ids")
    required = {"id", "title", "sources", "verification_gates", "forbidden_shortcuts", "states"}
    for a in algs:
        if not isinstance(a, dict) or required - set(a):
            fail(f"dangerous algorithm contract missing fields: {a.get('id') if isinstance(a, dict) else a}")
            continue
        if not a["forbidden_shortcuts"] or not a["states"]:
            fail(f"dangerous algorithm {a['id']} lacks failure/state detail")
        for src in a["sources"]:
            if not (ROOT / src).exists():
                fail(f"dangerous algorithm {a['id']} references missing source {src}")
        if any(g not in {f"V-{i:03d}" for i in range(1,25)} for g in a["verification_gates"]):
            fail(f"dangerous algorithm {a['id']} references malformed verification gate")


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
    # 2026-09-24 (ADR-0108): unlock is scope-granular; the global flag may only be
    # true when at least one scope is validly authorized (checked in detail by
    # validate_0924_scope_and_gate_guardrails).
    scopes = contract.get("engineering_scopes", {})
    authorized_scopes = [k for k, v in scopes.items() if v.get("authorized")]
    if authorized and not authorized_scopes:
        fail("engineering_authorized=true but no engineering scope is authorized")
    if authorized and design_blockers:
        fail(f"engineering_authorized=true with open design blockers {design_blockers}")
    if not scopes and authorized and verify_blockers:
        fail(f"engineering_authorized=true with blockers verification={verify_blockers}")
    if contract.get("phase") == "architecture_hardening_pre_engineering" and authorized is not False:
        fail("pre-engineering phase must have engineering_authorized=false")
    if not authorized:
        for name in ["SNAPSHOT.md", "ENGINEER_START_HERE.md"]:
            text = D(name).read_text(encoding="utf-8").lower()
            if "not authorized" not in text and "not yet authorized" not in text:
                fail(f"{name} does not visibly state that engineering is not authorized")


def check_identity(contract: dict) -> None:
    text = D("IDENTITY_PROTOCOL.md").read_text(encoding="utf-8")
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
        if re.search(r"\bAccountState\b", D(name).read_text(encoding="utf-8")):
            fail(f"{name} contains superseded AccountState authority term")


def check_provider_schema(contract: dict) -> None:
    files = sorted((ROOT / "spec/protocol").glob("provider-capabilities*.json"))
    wanted = ROOT / contract["schema_singletons"]["ProviderCapabilities"]
    if files != [wanted]:
        fail("ProviderCapabilities must have exactly one current schema: " + ", ".join(rel(p) for p in files))
    for p in (ROOT / "spec/protocol").glob("*.json"):
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
    naming = D("NAMING.md").read_text(encoding="utf-8")
    for phrase in ["Open-source Messenger", "OpenMessenger", "OwnMessenger", "OurMessenger", "OneMessenger", "OhMessenger", "0M", "zero total control"]:
        if phrase not in naming:
            fail(f"NAMING.md missing canonical OM meaning: {phrase}")
    # Country-specific research may remain in history only; current canonical architecture must not make one country the protocol home.
    legal = D("LEGAL_AND_OPERATIONAL_BOUNDARIES.md").read_text(encoding="utf-8").lower()
    if "protocol home jurisdiction" not in legal and "protocol-home" not in legal:
        fail("legal boundary does not state jurisdiction-neutral protocol position")
    if "payment processor" not in legal or "merchant-of-record" not in legal:
        fail("legal boundary does not state core commerce/operator separation")


def check_required_files(contract: dict) -> None:
    paths = set(contract.get("constitution_files", [])) | set(contract.get("machine_truth_files", [])) | set(contract.get("required_new_hardening_docs", []))
    paths |= {"LICENSE.md", "CONTRIBUTING.md", "tools/validate_architecture.py", "tools/build_manifest.py", "ARCHITECTURE_RESOLUTIONS_20260923.md", "MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md", "NAMING.md", "machine/resolved_questions.jsonl", "machine/architecture_design_queue.jsonl", "machine/architecture_verification_gates.jsonl", "machine/public_release_gates.jsonl", "machine/deferred_product_questions.jsonl"}
    for path in sorted(paths):
        if path == "MANIFEST.json":
            continue  # generated only inside exported drops (tools/om_drop.py export)
        if "/" in path:
            if not (ROOT/path).exists(): fail(f"required architecture file missing: {path}")
        elif not any(p.name == path for p in current_files()):
            fail(f"required architecture file missing: {path}")
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
    if not mp.exists():
        return  # git checkout: git is the integrity record; exports carry MANIFEST.json
    man = load_json(mp)
    if not isinstance(man, dict): return
    if man.get("snapshot") != contract.get("snapshot"):
        fail(f"MANIFEST snapshot {man.get('snapshot')} != contract snapshot {contract.get('snapshot')}")
    excludes = set(man.get("manifest_excludes", []))
    expected_excludes = set(contract.get("manifest_excludes", []))
    if excludes != expected_excludes:
        fail(f"MANIFEST excludes {sorted(excludes)} != contract excludes {sorted(expected_excludes)}")
    actual: dict[str, Path] = {}
    for p in current_files():
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
    schema_path = D("conversation-event.logical.schema.json")
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

    ptext = D("PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md").read_text(encoding="utf-8")
    for phrase in ("no ambient", "Plaintext", "network", "canonical"):
        if phrase.lower() not in ptext.lower():
            fail(f"07:18 guard: plugin architecture lost required boundary phrase {phrase!r}")



def validate_0756_zero_control_interop_portability_guardrails() -> None:
    naming = D("NAMING.md").read_text(encoding="utf-8")
    if "0M" not in naming or "zero total control" not in naming.lower():
        fail("07:56 guard: NAMING.md must preserve 0M / zero total control")

    service = D("SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md").read_text(encoding="utf-8")
    for phrase in ("permissionless", "RepresentationGrant", "no central issuer", "identity is cheap; unsolicited delivery"):
        if phrase.lower() not in service.lower():
            fail(f"07:56 guard: service creation architecture lost required phrase {phrase!r}")

    bridge = D("INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md").read_text(encoding="utf-8")
    for phrase in ("BridgePrincipal", "ForeignProvenance", "E2EE ends", "InteropTrace"):
        if phrase.lower() not in bridge.lower():
            fail(f"07:56 guard: bridge architecture lost required phrase {phrase!r}")

    archive = D("IMPORT_EXPORT_AND_USER_ESCAPE.md").read_text(encoding="utf-8")
    for phrase in ("om-archive-v1", "data and evidence", "ImportedHistoryRecord", "MUST NOT contain raw long-term"):
        if phrase.lower() not in archive.lower():
            fail(f"07:56 guard: archive architecture lost required phrase {phrase!r}")

    schema = load_json(D("conversation-event.logical.schema.json"))
    if isinstance(schema, dict):
        props = schema.get("properties", {})
        if "representation" not in props or "foreign_provenance" not in props:
            fail("07:56 guard: ConversationEvent must retain representation and foreign_provenance hooks")



def validate_0821_metadata_abuse_longevity_guardrails() -> None:
    metadata = D("METADATA_THREAT_MODEL.md").read_text(encoding="utf-8")
    for phrase in ("surface-specific", "opaque capabilities", "randomized", "global passive observer", "V-015"):
        if phrase.lower() not in metadata.lower():
            fail(f"08:21 guard: metadata architecture lost required phrase {phrase!r}")

    abuse = D("DECENTRALIZED_ABUSE_MODERATION.md").read_text(encoding="utf-8")
    for phrase in ("SafetyAdvisory", "no global reputation score", "provider denial", "Sybil", "V-016"):
        if phrase.lower() not in abuse.lower():
            fail(f"08:21 guard: abuse architecture lost required phrase {phrase!r}")

    longevity = D("PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md").read_text(encoding="utf-8")
    for phrase in ("Protocol Continuity Bundle", "never reused", "dominant client", "0M", "V-017"):
        if phrase.lower() not in longevity.lower():
            fail(f"08:21 guard: longevity architecture lost required phrase {phrase!r}")

    design = load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")
    for qid in ("A-013", "A-014", "A-015"):
        matches = [q for q in design if q.get("id") == qid]
        if len(matches) != 1 or matches[0].get("status") != "resolved":
            fail(f"08:21 guard: {qid} must remain design-resolved in this checkpoint")

    provider = load_json(D("provider-capabilities.logical.schema.json"))
    if isinstance(provider, dict):
        kinds = provider.get("ProviderCapabilities", {}).get("provider_kind", [])
        if "safety_advisory" not in kinds:
            fail("08:21 guard: ProviderCapabilities must retain optional safety_advisory role")


def validate_0920_verification_evidence_guardrails() -> None:
    summary = load_json(ROOT / "verification/results/phase1-summary.json")
    if not isinstance(summary, dict):
        return
    if summary.get("phase1_passed") is not True:
        fail("09:20 guard: phase1 verification runner must remain clean")
    if summary.get("gate_status_effect") != "evidence_only_no_gate_automatically_passed":
        fail("09:20 guard: verification evidence must not imply automatic gate passage")
    if int(summary.get("bounded_cases", 0)) < 7772:
        fail("09:20 guard: phase1 bounded evidence count regressed below 7772")
    if int(summary.get("cross_impl_cbor_vectors", 0)) < 14:
        fail("09:20 guard: deterministic-CBOR cross-implementation vectors regressed below 14")
    if int(summary.get("offline_continuity_artifacts", 0)) < 60:
        fail("09:20 guard: continuity dry-run artifact set regressed below 60")
    expected = {"V-002","V-004","V-005","V-008","V-009","V-011","V-013","V-014","V-015","V-016","V-017"}
    got = set(summary.get("gates_with_new_evidence", []))
    if not expected.issubset(got):
        fail(f"09:20 guard: phase1 evidence gate set regressed: missing={sorted(expected-got)}")
    for req in [
        "verification/tla/AccountControl.tla",
        "verification/tla/models.json",
        "verification/vectors/account-control-regression-vectors.json",
        "verification/FEASIBILITY_RECONNAISSANCE_20260923.md",
        "verification/results/cbor-cross-impl-latest.json",
    ]:
        if not (ROOT / req).exists():
            fail(f"09:20 guard: verification artifact missing: {req}")
    gates = {g.get("id"): g for g in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    for gid in expected:
        if not gates.get(gid, {}).get("evidence"):
            fail(f"09:20 guard: {gid} lost its recorded evidence links")
    # Evidence never authorizes engineering by itself (ADR-0109); only a scope ADR does (ADR-0169). check_phase and the
    # 09:24 scope guard verify that every authorized scope has its authorizing ADR, passed gates and feasibility.
    contract = load_json(ROOT / "machine/repository_contract.json")
    if isinstance(contract, dict) and contract.get("engineering_authorized") and not any(
            s.get("authorized") and s.get("authorizing_adr") for s in contract.get("engineering_scopes", {}).values()):
        fail("09:20 guard: engineering_authorized without an ADR-authorized scope")


def validate_0924_scope_and_gate_guardrails() -> None:
    contract = load_json(ROOT / "machine/repository_contract.json")
    if not isinstance(contract, dict):
        return
    scopes = contract.get("engineering_scopes")
    if not isinstance(scopes, dict) or not scopes:
        fail("09:24 guard: repository_contract must define engineering_scopes")
        return
    waves = {"W0": 0, "W1": 1, "W2": 2, "W3": 3, "W4": 4}
    gates = load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")
    gate_ids = {g["id"] for g in gates}
    always = contract.get("always_required_pre_engineering_gates", [])
    for gid in always:
        if gid not in gate_ids:
            fail(f"09:24 guard: always-required gate {gid} does not exist")
    ok_pre = {"passed", "waived_by_adr"}
    for g in gates:
        gid = g.get("id")
        for field in ("pre_engineering_criteria", "pre_release_criteria", "independence", "unlock_scopes", "pre_engineering_status", "pre_engineering_evidence_class", "executor_role", "review_batching", "pre_engineering_independence_status", "feasibility_criteria", "feasibility_criteria_met"):
            if field not in g:
                fail(f"ADR-0134 guard: {gid} missing {field}")
        if g.get("pre_engineering_status") not in {"open", "passed", "failed", "waived_by_adr"}:
            fail(f"09:24 guard: {gid} invalid pre_engineering_status")
        if g.get("pre_engineering_independence_status") not in {"open", "satisfied", "waived_by_adr"}:
            fail(f"ADR-0134 guard: {gid} invalid pre_engineering_independence_status")
        pre = g.get("pre_engineering_criteria") or []
        rel = g.get("pre_release_criteria") or []
        met = g.get("criteria_met") or []
        unknown_met = [c for c in met if c not in pre and c not in rel]
        if unknown_met:
            fail(f"ADR-0134 guard: {gid} criteria_met contains non-canonical strings: {unknown_met}")
        if g.get("pre_engineering_status") == "passed":
            missing = [c for c in pre if c not in met]
            if missing:
                fail(f"ADR-0134 guard: {gid} pre-engineering passed with unmet criteria {missing}")
            if g.get("pre_engineering_independence_status") != "satisfied":
                fail(f"ADR-0134 guard: {gid} pre-engineering passed without satisfied independence evidence")
        if not isinstance(g.get("interface_impact"), list) or g.get("feasibility_status") not in {"open", "evidenced"}:
            fail(f"ADR-0113 guard: {gid} must declare interface_impact (list) and feasibility_status (open|evidenced)")
        for s_ in g.get("interface_impact", []) or []:
            if s_ not in scopes:
                fail(f"ADR-0113 guard: {gid} interface_impact names unknown scope {s_}")
        if g.get("feasibility_status") == "evidenced" and not g.get("evidence"):
            fail(f"ADR-0113 guard: {gid} feasibility evidenced without evidence links")
        fcrit = g.get("feasibility_criteria") or []
        fmet = g.get("feasibility_criteria_met") or []
        unknown_fmet = [c for c in fmet if c not in fcrit]
        if unknown_fmet:
            fail(f"ADR-0134 guard: {gid} feasibility_criteria_met contains non-canonical strings: {unknown_fmet}")
        if g.get("interface_impact") and not fcrit:
            fail(f"ADR-0134 guard: {gid} has interface_impact but no explicit feasibility_criteria")
        if not g.get("interface_impact") and fcrit:
            fail(f"ADR-0134 guard: {gid} has feasibility_criteria but no interface_impact")
        if g.get("feasibility_status") == "evidenced":
            missing_f = [c for c in fcrit if c not in fmet]
            if missing_f:
                fail(f"ADR-0134 guard: {gid} feasibility evidenced with unmet criteria {missing_f}")
        if not g.get("pre_engineering_criteria") or not g.get("pre_release_criteria"):
            fail(f"09:24 guard: {gid} must list criteria for both phases")
        ind = g.get("independence", {})
        if not isinstance(ind, dict) or not ind.get("pre_engineering") or not ind.get("pre_release"):
            fail(f"09:24 guard: {gid} must declare independence for both phases")
        for s in g.get("unlock_scopes", []):
            if s not in scopes:
                fail(f"09:24 guard: {gid} references unknown scope {s}")
        if g.get("status") == "passed" and g.get("pre_engineering_status") not in ok_pre:
            fail(f"09:24 guard: {gid} overall passed without passed pre-engineering phase")
        if g.get("status") == "passed" and not g.get("evidence"):
            fail(f"09:24 guard: {gid} passed without evidence links")
    design = load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")
    open_design = [q for q in design if q.get("status") == "open" and q.get("blocks_engineering")]
    for name, sc in scopes.items():
        for field in ("wave", "authorized", "depends_on"):
            if field not in sc:
                fail(f"09:24 guard: scope {name} missing {field}")
        if sc.get("wave") not in waves:
            fail(f"09:24 guard: scope {name} has invalid wave")
            continue
        for dep in sc.get("depends_on", []):
            if dep not in scopes:
                fail(f"09:24 guard: scope {name} depends on unknown scope {dep}")
            elif waves.get(scopes[dep].get("wave"), 9) > waves[sc["wave"]]:
                fail(f"09:24 guard: scope {name} depends on later-wave scope {dep}")
        if sc.get("authorized"):
            need = [g for g in gates if name in g.get("unlock_scopes", []) or g["id"] in always]
            bad = [g["id"] for g in need if g.get("pre_engineering_status") not in ok_pre]
            if bad:
                fail(f"09:24 guard: scope {name} authorized with unmet pre-engineering gates {bad}")
            unfeasible = [g["id"] for g in gates if name in (g.get("interface_impact") or []) and g.get("feasibility_status") != "evidenced"]
            if unfeasible:
                fail(f"ADR-0113 guard: scope {name} authorized before cross-wave feasibility evidence on {unfeasible}")
            # RELEASE_WAVES §5.3 / ADR-0156: an open blocking item blocks the scopes it lists; an item
            # that lists no scope blocks every scope.
            blockers = [q["id"] for q in open_design if not q.get("scopes") or name in q["scopes"]]
            if blockers:
                fail(f"09:24 guard: scope {name} authorized with open design blockers {blockers}")
            for dep in sc.get("depends_on", []):
                if not scopes.get(dep, {}).get("authorized"):
                    fail(f"09:24 guard: scope {name} authorized before dependency {dep}")
            if not sc.get("authorizing_adr") or not any((ROOT / "decisions").glob(f"{sc.get('authorizing_adr')}-*.md")):
                fail(f"09:24 guard: scope {name} authorized without authorizing_adr")


def registered_suites() -> dict:
    reg = load_json(ROOT / "machine/conformance_suites.json")
    if not isinstance(reg, dict):
        return {}
    return {s.get("name"): s for s in reg.get("suites", []) if isinstance(s, dict)}


def validate_0924_traceability_and_threats() -> None:
    reqs = {r["id"] for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    suites = registered_suites()
    gates = {g["id"] for g in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    rel = {g["id"] for g in load_jsonl(ROOT / "machine/public_release_gates.jsonl")}
    tr = load_json(ROOT / "machine/requirement_traceability.json")
    if not isinstance(tr, dict):
        fail("09:24 guard: requirement_traceability.json unreadable")
        return
    traced = {}
    for row in tr.get("traces", []):
        traced[row.get("requirement")] = row.get("verified_by", [])
    for rid in sorted(reqs - set(traced)):
        fail(f"09:24 guard: requirement {rid} is not traced")
    for rid in sorted(set(traced) - reqs):
        fail(f"09:24 guard: traceability references unknown requirement {rid}")
    def valid_target(t: str) -> bool:
        if t in gates or t in rel or t == "validator":
            return True
        if t.startswith("conformance:"):
            # ADR-0130 rule 10: only registered suites with a spec document count as proof obligations
            return t.split(":", 1)[1] in suites
        if t.startswith("review:"):
            target = t.split(":", 1)[1]
            return (ROOT / target).exists() or any(p.name == target for p in current_files())
        return False
    for rid, targets in traced.items():
        if not targets:
            fail(f"09:24 guard: requirement {rid} has no verification target")
        for t in targets:
            if not valid_target(t):
                fail(f"09:24 guard: requirement {rid} has invalid target {t}")
    for th in load_jsonl(ROOT / "machine/threat_register.jsonl"):
        tid = th.get("id")
        if not th.get("mitigation_docs"):
            fail(f"09:24 guard: {tid} lacks mitigation docs")
        for d in th.get("mitigation_docs", []):
            if not (ROOT / d).exists():
                fail(f"09:24 guard: {tid} mitigation doc missing: {d}")
        for r in th.get("requirements", []):
            if r not in reqs:
                fail(f"09:24 guard: {tid} references unknown requirement {r}")
        for g in th.get("gates", []):
            if g.startswith("conformance:"):
                if g.split(":", 1)[1] not in suites:
                    fail(f"ADR-0130 guard: {tid} names unregistered conformance suite {g}")
            elif g not in gates and g not in rel and g != "validator":
                fail(f"09:24 guard: {tid} references unknown gate {g}")
        if not th.get("residual_risk"):
            fail(f"09:24 guard: {tid} must state residual risk")


def validate_0130_conformance_and_release_gates() -> None:
    """ADR-0130 rules 7 and 10: registered conformance suites; machine-applicable public release gates."""
    contract = load_json(ROOT / "machine/repository_contract.json")
    if not isinstance(contract, dict):
        return
    scopes = contract.get("engineering_scopes", {})
    reg = load_json(ROOT / "machine/conformance_suites.json")
    if not isinstance(reg, dict):
        fail("ADR-0130 guard: machine/conformance_suites.json missing or unreadable")
        return
    statuses = set(reg.get("statuses", []))
    names = set()
    for s in reg.get("suites", []):
        n = s.get("name")
        if not n or n in names:
            fail(f"ADR-0130 guard: conformance suite name missing or duplicated: {n!r}")
        names.add(n)
        if s.get("status") not in statuses:
            fail(f"ADR-0130 guard: conformance suite {n} has invalid status {s.get('status')!r}")
        doc = s.get("spec_doc")
        if not doc or not (ROOT / doc).is_file():
            fail(f"ADR-0130 guard: conformance suite {n} spec document missing: {doc}")
        for sc in s.get("scopes", []):
            if sc not in scopes:
                fail(f"ADR-0130 guard: conformance suite {n} names unknown scope {sc}")
            elif scopes[sc].get("authorized") and s.get("status") != "specified":
                fail(f"ADR-0130 guard: scope {sc} authorized while conformance suite {n} is only {s.get('status')}")
    vocab = contract.get("public_release_vocabulary", {})
    kinds, claims = set(vocab.get("distribution_kinds", [])), set(vocab.get("claims", []))
    prg = load_jsonl(ROOT / "machine/public_release_gates.jsonl")
    for g in prg:
        gid = g.get("id")
        ap = g.get("applies_to")
        if not isinstance(ap, dict) or not (ap.get("scopes") or ap.get("distribution_kinds") or ap.get("claims")):
            fail(f"ADR-0130 guard: {gid} must declare a non-empty applies_to (scopes, distribution_kinds, claims)")
            continue
        for sc in ap.get("scopes", []):
            if sc not in scopes:
                fail(f"ADR-0130 guard: {gid} applies_to unknown scope {sc}")
        for k in ap.get("distribution_kinds", []):
            if k not in kinds:
                fail(f"ADR-0130 guard: {gid} applies_to unknown distribution kind {k}")
        for c in ap.get("claims", []):
            if c not in claims:
                fail(f"ADR-0130 guard: {gid} applies_to unknown claim {c}")
        if not g.get("criteria") or not g.get("independence"):
            fail(f"ADR-0130 guard: {gid} must list criteria and independence")
    ok = {"passed", "waived_by_adr"}
    for r in contract.get("public_releases", []):
        rid = r.get("id")
        if r.get("status") not in set(vocab.get("release_statuses", [])):
            fail(f"ADR-0130 guard: public release {rid} has invalid status {r.get('status')!r}")
            continue
        if r.get("status") == "planned":
            continue
        rs, rk, rc = set(r.get("scopes", [])), r.get("distribution_kind"), set(r.get("claims", []))
        for g in prg:
            ap = g.get("applies_to") or {}
            hit = rs & set(ap.get("scopes", [])) or rk in ap.get("distribution_kinds", []) or rc & set(ap.get("claims", []))
            if hit and g.get("status") not in ok:
                fail(f"ADR-0130 guard: public release {rid} is {r.get('status')} while applicable {g.get('id')} is {g.get('status')}")


def validate_0924_hardening_content_guardrails() -> None:
    checks = {
        "HOME_ONBOARDING_TIERS_AND_PROVIDER_DIVERSITY.md": ("Sponsored starter mailbox", "monoculture", "no tier is privileged", "V-020"),
        "ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md": ("non-suppressible", "does not promise", "OS/cloud device backup", "V-021"),
        "VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md": ("never authority", "peer-local", "no remote disable"),
        "ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md": ("terminal", "LegacyPolicy", "can **never**"),
        "RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md": ("Circular lock", "pre_engineering_criteria", "unlocks nothing"),
        "CRYPTOGRAPHY_AND_AGILITY.md": ("not mitigated in v1", "Adoption triggers", "ReInit"),
        "SECURITY_PRIVACY_PROFILES.md": ("No preset may claim PQ confidentiality for groups",),
        "ACCOUNT_CONTINUITY_AND_CONTROL.md": ("ContinuityAssessment", "on** by default"),
        "RECOVERY_AND_BACKUP_POLICY.md": ("identity recovery is on by default", "one vote", "veto", "RecoveryHandle"),
    }
    for name, phrases in checks.items():
        text = D(name).read_text(encoding="utf-8").lower()
        for ph in phrases:
            if ph.lower() not in text:
                fail(f"09:24 guard: {name} lost required phrase {ph!r}")
    overclaim = re.compile(r"\b(v1\s+)?(private\s+)?groups?\s+(are|is)\s+(post-quantum|pq)[- ](secure|protected|hybrid)", re.I)
    for p in current_files():
        if p.suffix == ".md" and "research" not in p.parts and overclaim.search(p.read_text(encoding="utf-8")):
            fail(f"09:24 guard: {rel(p)} appears to claim PQ confidentiality for groups")
    validate_pq_claims()


# Dated records may quote superseded claims: decision history, research, review/report/checkpoint records,
# prototype evidence and the changelog (ADR-0130 rule 12, F-R3-7, F-R4-11).
PQ_CLAIM_EXEMPT = ("decisions/", "docs/research/", "docs/reviews/", "docs/reports/", "docs/checkpoints/",
                   "verification/reviews/", "verification/prototypes/", "verification/FEASIBILITY_RECONNAISSANCE_",
                   "CHANGELOG.md", "machine/decisions.jsonl", "machine/resolved_questions.jsonl")
PQ_CLAIM_SCANNED_MACHINE = ("machine/requirements.jsonl", "machine/architecture_verification_gates.jsonl",
                            "machine/public_release_gates.jsonl", "machine/threat_register.jsonl",
                            "machine/architecture_design_queue.jsonl")
_TRIPLE = re.compile(r"triple[- ]ratchet|\bSPQR\b", re.I)
# a Triple Ratchet/SPQR mention is fine when the line places it in v2, ADR-0118, the deferral, a reference design,
# a struck-through old text, or a negation
_TRIPLE_OK = re.compile(r"v2|ADR-0118|deferred|reference design|~~|\bno\b|\bnot\b|without (the )?spqr", re.I)
_PQ = re.compile(r"\bPQC?\b|post-quantum|quantum-(safe|secure|proof|resistant)", re.I)
_GROUP = re.compile(r"\bgroups?\b|group calls?", re.I)
_PQ_GROUP_OK = re.compile(r"classical|ADR-0107|\bno\b|\bnot\b|never|cannot|n't|experimental|until|\bwhen\b|refuse|"
                          r"ReInit|migrat|pluggable|evolving|draft|R-173|V-019|standardi", re.I)


def validate_pq_claims() -> None:
    """No Triple Ratchet/SPQR or PQ post-compromise claim for OM-Crypto-v1, no PQ claim for v1 groups (R-276)."""
    for p in current_files():
        rp = rel(p)
        if rp.startswith(PQ_CLAIM_EXEMPT):
            continue
        if not (p.suffix == ".md" or rp in PQ_CLAIM_SCANNED_MACHINE):
            continue
        text = p.read_text(encoding="utf-8")
        if p.suffix == ".jsonl":
            # one record per line: check each string value on its own, so a marker in another field does not excuse it
            units = []
            for n, raw in enumerate(text.splitlines(), 1):
                rec = json.loads(raw) if raw.strip() else None
                # resolved/superseded records are decision history
                stack = [rec] if isinstance(rec, dict) and rec.get("status") not in ("resolved", "superseded") else []
                while stack:
                    x = stack.pop()
                    if isinstance(x, dict):
                        stack.extend(x.values())
                    elif isinstance(x, list):
                        stack.extend(x)
                    elif isinstance(x, str):
                        units.append((n, x))
        else:
            units = list(enumerate(text.splitlines(), 1))
        for n, line in units:
            if _TRIPLE.search(line) and not _TRIPLE_OK.search(line):
                fail(f"ADR-0130 guard: {rp}:{n} presents Triple Ratchet/SPQR as an OM-Crypto-v1 property (v1 is PQXDH + Double Ratchet, ADR-0118)")
            if _PQ.search(line) and _GROUP.search(line) and not _PQ_GROUP_OK.search(line):
                fail(f"ADR-0130 guard: {rp}:{n} pairs a post-quantum claim with groups without saying v1 groups are classical (ADR-0107)")
    summary = load_json(ROOT / "verification/results/phase1-summary.json")
    if isinstance(summary, dict) and int(summary.get("continuity_assessment_cases", 0)) < 1620:
        fail("09:24 guard: continuity assessment evidence missing or regressed")




def validate_0143_crash_safe_sealed_delivery_guardrails() -> None:
    """ADR-0143: a durable outer reservation must be resumable without weakening replay/inner-auth safety."""
    adr = (ROOT / "decisions/ADR-0143-crash-safe-sealed-delivery-replay-and-retry.md").read_text(encoding="utf-8")
    profile = (ROOT / "docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md").read_text(encoding="utf-8")
    session = (ROOT / "docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md").read_text(encoding="utf-8")
    schema = load_json(ROOT / "spec/protocol/one-to-one-session-state.logical.schema.json")
    for token in ("RESERVED", "COMPLETE", "REJECTED", "exact-wrapper retry", "crash-consistent"):
        if token.lower() not in adr.lower(): fail(f"ADR-0143 guard: ADR missing {token!r}")
    for name, text in (("delivery profile", profile), ("session management", session)):
        for token in ("RESERVED", "COMPLETE", "exact"):
            if token.lower() not in text.lower(): fail(f"ADR-0143 guard: {name} missing {token!r}")
    state = schema.get("OneToOneSessionState", {}) if isinstance(schema, dict) else {}
    sr = str(state.get("sealed_delivery_replay_state", ""))
    if not all(tok in sr for tok in ("RESERVED", "COMPLETE", "REJECTED")):
        fail("ADR-0143 guard: logical session schema lacks replay stages")
    reqs = {r.get("id") for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    if "R-314" not in reqs: fail("ADR-0143 guard: R-314 missing")
    gates = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    v3 = gates.get("V-003", {})
    sealed_criterion = "session state enforces per-device/per-route sealed-delivery key refresh/revocation; exact-wrapper-fingerprint replay state RESERVED/COMPLETE/REJECTED; resumable same-route RESERVED after crash; same-key/id different-wrapper collision rejection; crash-consistent inner K2/session-dedupe + COMPLETE before release/ack; idempotent COMPLETE retry; fresh id on cross-route reseal; outer-open is not sender auth; and no clear-sender fallback (ADR-0142/0143, R-314)"
    if sealed_criterion not in v3.get("criteria_met", []):
        fail("ADR-0143 guard: V-003 sealed-delivery state criterion not backed by current evidence")
    result = load_json(ROOT / "verification/results/v003-sealed-delivery-latest.json")
    summary = result.get("summary", {}) if isinstance(result, dict) else {}
    if result.get("ok") is not True or summary.get("scenarios") != 15 or summary.get("passed") != 15 or summary.get("mutations") != 5 or summary.get("mutations_caught") != 5:
        fail("ADR-0143 guard: focused V-003 sealed-delivery state result missing/regressed")
    ids = {x.get("id") for x in result.get("scenarios", []) if isinstance(x, dict)} if isinstance(result, dict) else set()
    for sid in ("crash_after_reserve_then_exact_retry", "crash_after_complete_before_release", "same_key_id_delivery_id_different_wrapper_rejected", "route_migration_reseals_fresh_id_and_dedupes_event", "outer_open_never_authenticates_sender"):
        if sid not in ids: fail(f"ADR-0143 guard: focused V-003 scenario missing {sid}")

def validate_0145_bootstrap_retry_closure_guardrails() -> None:
    """ADR-0145: one logical bootstrap response and COMPLETE replay must survive completed-key retirement."""
    adr = (ROOT / "decisions/ADR-0145-bootstrap-single-response-and-complete-retry-closure.md").read_text(encoding="utf-8")
    profile = (ROOT / "docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md").read_text(encoding="utf-8")
    session = (ROOT / "docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md").read_text(encoding="utf-8")
    discovery = (ROOT / "docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md").read_text(encoding="utf-8")
    inv = (ROOT / "docs/architecture/safety/SECURITY_INVARIANTS.md").read_text(encoding="utf-8")
    for token in ("one valid logical bootstrap response", "COMPLETE_REPLAY_ONLY", "first consults existing replay state", "private bootstrap key may be destroyed", "crash-consistent"):
        if token.lower() not in adr.lower(): fail(f"ADR-0145 guard: ADR missing {token!r}")
    for token in ("before requiring live key material", "complete", "reserved", "idempotent"):
        if token.lower() not in profile.lower(): fail(f"ADR-0145 guard: delivery profile missing {token!r}")
    for token in ("exactly one logical", "complete", "replay-only", "reserved"):
        if token.lower() not in session.lower(): fail(f"ADR-0145 guard: session management missing {token!r}")
    for token in ("exactly one logical recipient bootstrap response", "crash-consistent commit wins", "replay/request tombstone"):
        if token.lower() not in discovery.lower(): fail(f"ADR-0145 guard: discovery lifecycle missing {token!r}")
    if "`M7`" not in inv or "ADR-0142/0144/0145" not in inv:
        fail("ADR-0145 guard: M7 does not name ADR-0145 closure")
    reqs = {r.get("id") for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    threats = {r.get("id") for r in load_jsonl(ROOT / "machine/threat_register.jsonl")}
    if "R-317" not in reqs: fail("ADR-0145 guard: R-317 missing")
    if "T-118" not in threats: fail("ADR-0145 guard: T-118 missing")
    gates = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    v3 = gates.get("V-003", {})
    criterion = "ADR-0145 closure evidence proves exactly one logical MessageRequest bootstrap response, atomic winning response/imported-established-state + COMPLETE + request closure, exact COMPLETE lost-ack retry after private bootstrap-key destruction via replay-before-key lookup, RESERVED key retention, and negative mutations for key-before-replay, second-response acceptance and split completion/request closure (R-317)"
    if criterion not in v3.get("pre_engineering_criteria", []) or criterion not in v3.get("criteria_met", []):
        fail("ADR-0145 guard: V-003 closure criterion missing/not evidenced")
    result = load_json(ROOT / "verification/results/sealed-delivery-adr0145-closure-latest.json")
    summary = result.get("summary", {}) if isinstance(result, dict) else {}
    if result.get("passed") is not True or summary.get("scenarios") != 5 or summary.get("scenarios_passed") != 5 or summary.get("mutations") != 3 or summary.get("mutations_caught") != 3:
        fail("ADR-0145 guard: focused closure result missing/regressed")
    ids = {x.get("id") for x in result.get("scenarios", []) if isinstance(x, dict)} if isinstance(result, dict) else set()
    for sid in ("complete_lost_ack_retry_after_key_destroy", "different_wrapper_after_complete_is_closed", "multi_device_race_exactly_one_winner", "bootstrap_completion_atomic_crash_state", "reserved_retry_retains_key"):
        if sid not in ids: fail(f"ADR-0145 guard: focused scenario missing {sid}")

def validate_0144_pre_review_sealed_delivery_guardrails() -> None:
    """ADR-0144: bootstrap scope is request-local and stale ACS defers rather than terminally rejects."""
    adr = (ROOT / "decisions/ADR-0144-pre-review-sealed-delivery-bootstrap-and-stale-head-closure.md").read_text(encoding="utf-8")
    session = (ROOT / "docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md").read_text(encoding="utf-8")
    discovery = (ROOT / "docs/architecture/discovery/INVITES_INTROGATE_AND_SPAM.md").read_text(encoding="utf-8")
    inv = (ROOT / "docs/architecture/safety/SECURITY_INVARIANTS.md").read_text(encoding="utf-8")
    for token in ("fresh", "reply_delivery_key.expires_at", "reply_route.expires_at", "RESERVED", "stale", "monotonic", "REJECTED"):
        if token.lower() not in adr.lower(): fail(f"ADR-0144 guard: ADR missing {token!r}")
    for token in ("stays", "reserved", "stale", "refresh"):
        if token.lower() not in session.lower(): fail(f"ADR-0144 guard: session composition missing {token!r}")
    for token in ("fresh reply key pair", "route-binding", "expiry", "not reused"):
        if token.lower() not in discovery.lower(): fail(f"ADR-0144 guard: discovery bootstrap scope missing {token!r}")
    if "`M7`" not in inv or "ADR-0142/0143/0144" not in inv:
        fail("ADR-0144 guard: security invariant map missing M7 or amended M6")
    reqs = {r.get("id") for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    threats = {r.get("id") for r in load_jsonl(ROOT / "machine/threat_register.jsonl")}
    for rid in ("R-315", "R-316"):
        if rid not in reqs: fail(f"ADR-0144 guard: {rid} missing")
    for tid in ("T-115", "T-116", "T-117"):
        if tid not in threats: fail(f"ADR-0144 guard: {tid} missing")
    queue = {q.get("id"): q for q in load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")}
    if "keeping recovery off by default" in str(queue.get("A-020", {}).get("question", "")).lower():
        fail("ADR-0144 guard: A-020 still embeds superseded recovery-off default")
    gates = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    v3 = gates.get("V-003", {})
    criterion = "ADR-0144 composition evidence covers fresh request-scoped reply_delivery_key tuple/profile/route/expiry binding and no authority promotion, plus RESERVED -> authenticated stale-ACS refresh -> COMPLETE and the negative mutation that prematurely converts stale/unknown relationship state to REJECTED (R-315, R-316)"
    if criterion not in v3.get("pre_engineering_criteria", []):
        fail("ADR-0144 guard: V-003 composition criterion missing")

def validate_0142_sealed_delivery_guardrails() -> None:
    """ADR-0142: established OM-Crypto-v1 1:1 relay delivery is mandatory sealed delivery."""
    num = load_json(ROOT / "spec/registries/om-numeric-fields.json")
    types = load_json(ROOT / "spec/registries/om-object-types.json")
    crypto = load_json(ROOT / "spec/registries/om-crypto-profiles.json")
    ds = load_json(ROOT / "spec/registries/om-domain-separation.json")
    if not all(isinstance(x, dict) for x in (num, types, crypto, ds)):
        fail("ADR-0142 guard: registries unreadable"); return
    objs = {x.get("name"): x for x in types.get("types", []) if isinstance(x, dict)}
    for name, tid, signed in (("SealedDeliveryKeyRecord",22,True),("SealedDeliveryEnvelope",23,False)):
        x=objs.get(name,{})
        if x.get("type_id") != tid or bool(x.get("signed")) != signed:
            fail(f"ADR-0142 guard: {name} type/signedness drift")
    structs=num.get("structs",{})
    mrp={x.get("key"):x for x in structs.get("MessageRequestPayload",{}).get("fields",[]) if isinstance(x,dict)}
    if 9 not in mrp or mrp[9].get("name") != "reply_delivery_key" or mrp[9].get("req") is not True:
        fail("ADR-0142 guard: MessageRequestPayload.reply_delivery_key is not required")
    p1=next((x for x in crypto.get("profiles",[]) if x.get("id")==1),{})
    h=p1.get("sealed_delivery_hpke",{})
    expected={"spec":"draft-ietf-hpke-pq-05","mode":"base","kem":"MLKEM768-X25519","kem_id":25722,"kdf":"HKDF-SHA256","aead":"ChaCha20Poly1305","enc_bytes":1120,"public_key_bytes":1216,"plaintext_buckets":[1024,4096,16384,65536,131072],"aead_tag_bytes":16}
    for k,v in expected.items():
        if h.get(k) != v: fail(f"ADR-0142 guard: sealed_delivery_hpke {k} drift")
    labels={x.get("label"):x for x in ds.get("labels",[]) if isinstance(x,dict)}
    for lab in ("OM-SEALED-DELIVERY-KEY-SIG-v1","OM-SEALED-DELIVERY-ROUTE-v1","OM-SEALED-DELIVERY-HPKE-INFO-v1","OM-SEALED-DELIVERY-HPKE-AAD-v1"):
        if labels.get(lab,{}).get("status") != "frozen": fail(f"ADR-0142 guard: domain label not frozen: {lab}")
    delivery=(ROOT/"spec/cddl/delivery.cddl").read_text(encoding="utf-8")
    discovery=(ROOT/"spec/cddl/discovery.cddl").read_text(encoding="utf-8")
    for tok in ("SealedDeliveryKeyRecord = {","SealedDeliveryEnvelope = {","delivery-key-material = {"):
        if tok not in delivery: fail(f"ADR-0142 guard: delivery CDDL missing {tok}")
    if "9: delivery-key-material" not in discovery: fail("ADR-0142 guard: discovery CDDL missing reply delivery key")
    meta=load_json(ROOT/"spec/metadata/w0-metadata-exposure-descriptors.json")
    ex={d.get("role"):d.get("exposures",{}) for d in meta.get("descriptors",[]) if isinstance(d,dict)} if isinstance(meta,dict) else {}
    for role in ("home_relay_mailbox","t0_sponsored_mailbox","t1_hosted_home"):
        if ex.get(role,{}).get("stable_device_correlator") != "no_by_design" or ex.get(role,{}).get("ratchet_header_shape") != "no_by_design":
            fail(f"ADR-0142 guard: {role} clear sender/header exposure reintroduced")
    reqs={q.get("id") for q in load_jsonl(ROOT/"machine/requirements.jsonl")}; threats={q.get("id") for q in load_jsonl(ROOT/"machine/threat_register.jsonl")}
    for rid in ("R-293","R-308","R-309","R-310","R-311","R-312","R-313"):
        if rid not in reqs: fail(f"ADR-0142 guard: {rid} missing")
    for tid in ("T-111","T-112","T-113","T-114"):
        if tid not in threats: fail(f"ADR-0142 guard: {tid} missing")
    gates={q.get("id"):q for q in load_jsonl(ROOT/"machine/architecture_verification_gates.jsonl")}
    v15=json.dumps(gates.get("V-015",{}),sort_keys=True)
    if "unencrypted Double Ratchet header fields/shape" in v15: fail("ADR-0142 guard: V-015 retains superseded clear-header criterion")
    v005=load_json(ROOT/"verification/results/v005-node-adr0142-latest.json")
    if not isinstance(v005,dict) or v005.get("passed_for_node_slice") is not True or int(v005.get("vector_count",0)) < 735 or v005.get("mismatch_count") != 0:
        fail("ADR-0142 guard: current sealed-delivery V-005 Node evidence missing/stale")
    result=load_json(ROOT/"verification/results/sealed-delivery-adr0142-latest.json")
    if not isinstance(result,dict) or result.get("passed") is not True:
        fail("ADR-0142 guard: bounded sealed-delivery result missing/not passing")


def validate_0141_semantic_consistency_guardrails() -> None:
    num = load_json(ROOT / "spec/registries/om-numeric-fields.json")
    legacy = next((x for x in num.get("extensions", []) if x.get("name") == "om.acs.legacy_policy"), None) if isinstance(num, dict) else None
    if not legacy or legacy.get("critical") is not True: fail("ADR-0141 guard: om.acs.legacy_policy must be critical")
    acs13 = next((x for x in num.get("structs", {}).get("AccountControlState", {}).get("fields", []) if x.get("key") == 13), None) if isinstance(num, dict) else None
    if not acs13 or "proposed" in str(acs13.get("note", "")).lower(): fail("ADR-0141 guard: AccountControlState device_credentials registry row remains proposed")
    cddl = (ROOT / "spec/cddl/account.cddl").read_text(encoding="utf-8")
    if "device_credentials (set; proposed addition)" in cddl or "non-critical key-23 extension" in cddl or "preserve/ignore it" in cddl: fail("ADR-0141 guard: frozen/critical AccountControlState semantics drift")
    ds = load_json(ROOT / "spec/registries/om-domain-separation.json")
    proposed = [x.get("label") for x in ds.get("labels", []) if x.get("wave") == "W0" and x.get("status") == "proposed"] if isinstance(ds, dict) else ["registry unreadable"]
    if proposed: fail(f"ADR-0141 guard: assigned W0 domain labels still proposed: {proposed}")
    labels = {x.get("label"): x for x in ds.get("labels", []) if isinstance(x, dict)} if isinstance(ds, dict) else {}
    for lab in ("OM-RECOVERY-PROVIDER-METADATA-KEY-v1", "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1", "OM-RECOVERY-RECORD-AEAD-AD-v1", "OM-RECOVERY-RECORD-KEY-v1"):
        x = labels.get(lab)
        if not x or x.get("status") != "frozen" or "reserved" in str(x.get("construction", "")).lower(): fail(f"ADR-0141 guard: recovery construction not frozen/filled: {lab}")
    tf = load_json(ROOT / "spec/registries/om-time-fields.json"); rules = "\n".join(tf.get("rules", [])) if isinstance(tf, dict) else ""
    if "normatively frozen at 300 s" not in rules or "Proposed; per-object constants remain policy work" in rules: fail("ADR-0141 guard: default 300-second skew is not frozen")
    home = (ROOT / "spec/protocol/home-api.md").read_text(encoding="utf-8").lower()
    for token in ("url path", "query string", "cookie", "referrer-policy: no-referrer", "cross-origin redirect", "cache-control: no-store", "analytics", "crash reports"):
        if token not in home: fail(f"ADR-0141 guard: Home API capability hygiene missing {token!r}")
    rr = load_json(ROOT / "spec/protocol/recovery-record.logical.schema.json"); required = set(rr.get("required", [])) if isinstance(rr, dict) else set()
    for fld in ("record_generation", "recovery_epoch", "last_known_acs_head"):
        if fld not in required: fail(f"ADR-0141 guard: RecoveryRecord required field missing {fld}")
    rdoc = (ROOT / "docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md").read_text(encoding="utf-8").lower()
    for token in ("advisory and potentially stale", "high-water mark", "relationshiprecoverykey", "revalidate"):
        if token not in rdoc: fail(f"ADR-0141 guard: recovery semantics missing {token!r}")
    call = (ROOT / "spec/protocol/call-session.logical.schema.json").read_text(encoding="utf-8")
    if "is open (A-035)" in call or "MLS exporter over (CallID" in call or "joined participant DeviceIDs" not in call or "Room MLS epoch possession alone" not in call: fail("ADR-0141 guard: resolved group-call isolation semantics drift")
    callsdoc = (ROOT / "docs/architecture/calls/CALLS_AND_REALTIME_MEDIA.md").read_text(encoding="utf-8")
    if "Call keys come from an MLS exporter" in callsdoc or "Group call keys derive through the MLS exporter" in callsdoc: fail("ADR-0141 guard: calls architecture reverted to superseded MLS-exporter media-key source")
    reqs = {q.get("id"): q for q in load_jsonl(ROOT / "machine/requirements.jsonl")}; threats = {q.get("id"): q for q in load_jsonl(ROOT / "machine/threat_register.jsonl")}
    for rid in ("R-302","R-303","R-304","R-305","R-306","R-307"):
        if rid not in reqs: fail(f"ADR-0141 guard: {rid} missing")
    for tid in ("T-107","T-108","T-109","T-110"):
        if tid not in threats: fail(f"ADR-0141 guard: {tid} missing")
    queue = {q.get("id"): q for q in load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")}
    if "non-critical om.acs.legacy_policy" in str(queue.get("A-030", {}).get("resolution", "")):
        fail("ADR-0141 guard: A-030 retains superseded non-critical LegacyPolicy resolution")
    v005_path = ROOT / ("verification/results/v005-node-adr0142-latest.json" if (ROOT / "verification/results/v005-node-adr0142-latest.json").exists() else "verification/results/v005-node-adr0141-latest.json")
    v005 = load_json(v005_path)
    if not isinstance(v005, dict) or v005.get("passed_for_node_slice") is not True or int(v005.get("vector_count", 0)) < 681 or v005.get("mismatch_count") != 0:
        fail("ADR-0141 guard: current V-005 Node evidence no longer preserves ADR-0141 coverage")
    result = load_json(ROOT / "verification/results/design-consistency-adr0141-latest.json")
    if not isinstance(result, dict) or result.get("passed") is not True: fail("ADR-0141 guard: bounded semantic consistency result missing/not passing")

def validate_0140_async_prekey_guardrails() -> None:
    """ADR-0140: v1 bundles hide ACS linkage, reject rollback, and context-bind one-time prekeys."""
    reg = load_json(ROOT / "spec/registries/om-numeric-fields.json")
    ds = load_json(ROOT / "spec/registries/om-domain-separation.json")
    if not isinstance(reg, dict) or not isinstance(ds, dict):
        return
    akb = reg.get("structs", {}).get("AsyncKeyBundle", {})
    active = {x.get("key"): x for x in akb.get("fields", []) if isinstance(x, dict)}
    reserved = {x.get("key"): x for x in akb.get("reserved", []) if isinstance(x, dict)}
    for k, old_name in ((3, "acs_generation"), (4, "acs_state_hash")):
        if k in active:
            fail(f"ADR-0140 guard: retired AsyncKeyBundle key {k} remains active")
        if reserved.get(k, {}).get("name") != old_name:
            fail(f"ADR-0140 guard: retired AsyncKeyBundle key {k} must remain reserved as {old_name}")
    for k in (2, 5, 6, 7, 8, 12, 13):
        if k not in active:
            fail(f"ADR-0140 guard: required AsyncKeyBundle key {k} missing")

    keys = (ROOT / "spec/cddl/keys.cddl").read_text(encoding="utf-8")
    m = re.search(r"AsyncKeyBundle\s*=\s*\{(.*?)\n\}", keys, re.S)
    if not m:
        fail("ADR-0140 guard: AsyncKeyBundle CDDL block missing")
    else:
        body = m.group(1)
        if re.search(r"^\s*[34]\s*:", body, re.M):
            fail("ADR-0140 guard: retired ACS fields remain in AsyncKeyBundle CDDL")

    labels = {x.get("label"): x for x in ds.get("labels", []) if isinstance(x, dict)}
    core = labels.get("OM-PREKEY-GENERATION-CORE-v1")
    if not core or core.get("status") != "frozen":
        fail("ADR-0140 guard: frozen generation-core domain label missing")
    item = labels.get("OM-PREKEY-SIG-v1", {}).get("construction", "")
    for token in ("subject_device_id", "bundle_generation", "provider_pool_id", "key_id", "public_key"):
        if token not in item:
            fail(f"ADR-0140 guard: one-time-prekey signature construction missing {token}")

    doc = (ROOT / "docs/architecture/crypto/ASYNC_KEY_MATERIAL.md").read_text(encoding="utf-8")
    for token in ("highest_generation", "relationship-pinned ACS", "first-observation", "provider_pool_id", "revokeGeneration"):
        if token.lower() not in doc.lower():
            fail(f"ADR-0140 guard: ASYNC_KEY_MATERIAL.md missing {token!r}")

    # Current canonical encoding vectors must make the retired fields syntactically invalid.
    vp = ROOT / "verification/vectors/v005-w0-object-vectors.json.gz"
    try:
        vec = json.loads(gzip.decompress(vp.read_bytes()))
        byid = {x.get("id"): x for x in vec.get("vectors", [])}
        for vid in ("AsyncKeyBundle/neg/retired-acs-generation-key-3", "AsyncKeyBundle/neg/retired-acs-state-hash-key-4"):
            if byid.get(vid, {}).get("expect") != "reject" or byid.get(vid, {}).get("error") != "unknown_field":
                fail(f"ADR-0140 guard: missing negative vector {vid}")
    except Exception as e:
        fail(f"ADR-0140 guard: cannot inspect V-005 vectors: {e}")

    reqs = {r.get("id") for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    threats = {r.get("id") for r in load_jsonl(ROOT / "machine/threat_register.jsonl")}
    for rid in ("R-298", "R-299", "R-300", "R-301"):
        if rid not in reqs: fail(f"ADR-0140 guard: {rid} missing")
    for tid in ("T-104", "T-105", "T-106"):
        if tid not in threats: fail(f"ADR-0140 guard: {tid} missing")
    # Gate text and provider contracts must not preserve the superseded provider-visible ACS design.
    gates = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    v15 = json.dumps(gates.get("V-015", {}), sort_keys=True)
    if "ACS-hash/device linkage" in v15 or "v2 unlinkability target" in v15:
        fail("ADR-0140 guard: V-015 still describes superseded provider-visible ACS linkage")
    home = (ROOT / "spec/protocol/home-api.md").read_text(encoding="utf-8")
    for token in ("relationship-pinned trusted peer ACS", "recipient's durable", "best-effort cleanup"):
        if token.lower() not in home.lower():
            fail(f"ADR-0140 guard: Home API missing provider freshness rule {token!r}")
    result = load_json(ROOT / "verification/results/async-prekey-adr0140-latest.json")
    if not isinstance(result, dict) or result.get("passed") is not True:
        fail("ADR-0140 guard: current bounded prekey semantics result missing/not passing")



def validate_0166_consolidation_guardrails() -> None:
    """ADR-0166: keep the reviewed ADR-0157/0158 semantics after consolidating the parallel Run line.

    The parallel Run-1 ADR-0157 proposed a '!' critical-ID class, an AccountHistorySnapshot name and a TLS-exporter LAN
    binding. ADR-0166 adopts the independently reviewed alternatives instead; these checks stop the superseded variants
    from silently returning through a later merge.
    """
    numeric = load_json(ROOT / "spec/registries/om-numeric-fields.json") or {}
    if "om.critical." not in json.dumps(numeric):
        fail("ADR-0166 guard: numeric-field registry lost the ADR-0157 om.critical. reserved prefix")
    node = (ROOT / "verification/cbor/node_strict_harness.mjs").read_text(encoding="utf-8")
    rust = (ROOT / "verification/prototypes/v005-encoding/rust/src/schema.rs").read_text(encoding="utf-8")
    for label, src in (("Node", node), ("Rust", rust)):
        if "om.critical." not in src:
            fail(f"ADR-0166 guard: {label} strict decoder lacks unknown om.critical.* rejection")
        if "startsWith('!')" in src or "starts_with('!')" in src:
            fail(f"ADR-0166 guard: {label} strict decoder reintroduced the superseded '!' critical class")

    ds = load_json(ROOT / "spec/registries/om-domain-separation.json") or {}
    labels = {r.get("label") for r in ds.get("labels", []) if isinstance(r, dict)}
    for label in ("OM-LAN-TLS-EXPORTER-v1", "OM-LAN-CHANNEL-PROOF-v1", "OM-LAN-RENDEZVOUS-TOKEN-v1"):
        if label in labels:
            fail(f"ADR-0166 guard: superseded Run-1 LAN label {label} is registered")
    for label in ("OM-LAN-PAIR-SEED-v1", "OM-LAN-RENDEZVOUS-TAG-v1"):
        if label not in labels:
            fail(f"ADR-0166 guard: ADR-0158 LAN label {label} missing")

    design = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_design_queue.jsonl")}
    if design.get("A-055", {}).get("status") != "resolved" or "ADR-0158" not in str(design.get("A-055", {}).get("resolution", "")):
        fail("ADR-0166 guard: A-055 must stay resolved by ADR-0158")

    gates = {r.get("id"): r for r in load_jsonl(ROOT / "machine/architecture_verification_gates.jsonl")}
    v4 = json.dumps(gates.get("V-004", {}), sort_keys=True)
    if "GroupHistoryBundle" not in v4 or "R-363" not in v4:
        fail("ADR-0166 guard: V-004 must freeze GroupHistoryBundle with its ADR-0161 binding criterion (R-347, R-363)")

    group = (ROOT / "docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md").read_text(encoding="utf-8")
    sync = (ROOT / "docs/architecture/crypto/MULTI_DEVICE_SYNC.md").read_text(encoding="utf-8")
    if "GroupHistoryBundle" not in group or "DeviceHistorySnapshot" not in sync:
        fail("ADR-0166 guard: DeviceHistorySnapshot/GroupHistoryBundle naming split is missing")
    for rel in ("docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md", "docs/architecture/crypto/MULTI_DEVICE_SYNC.md",
                "docs/constitution/PROTOCOL_EXTENSIBILITY.md", "docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md",
                "docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md"):
        text = (ROOT / rel).read_text(encoding="utf-8")
        if "AccountHistorySnapshot" in text or "`!` ID" in text or "`!` namespace" in text:
            fail(f"ADR-0166 guard: {rel} reintroduces superseded Run-1 terminology")

def validate_0163_implementation_readiness_guardrails() -> None:
    """ADR-0163: scope contracts + crash/retry handoff stay mechanically aligned."""
    contract = load_json(ROOT / "machine/repository_contract.json") or {}
    impl = load_json(ROOT / "machine/engineering_scope_contracts.json")
    if not isinstance(impl, dict):
        fail("ADR-0163 guard: engineering_scope_contracts.json unreadable")
        return
    repo_scopes = contract.get("engineering_scopes") or {}
    impl_scopes = impl.get("scopes") or {}
    if set(impl_scopes) != set(repo_scopes):
        fail(f"ADR-0163 guard: implementation scope set drift: repo={sorted(repo_scopes)} impl={sorted(impl_scopes)}")
        return

    # Authorization dependencies in the implementation index are mirrors, never a second policy source.
    for name, sc in impl_scopes.items():
        expected = repo_scopes[name].get("depends_on", [])
        actual = sc.get("authorization_depends_on")
        if actual != expected:
            fail(f"ADR-0163 guard: {name} authorization dependencies differ from repository_contract: {actual!r} != {expected!r}")
        for field in ("normative_sources", "restart_contract", "error_retry_contract", "forbidden_shortcuts"):
            value = sc.get(field)
            if not isinstance(value, list) or not value:
                fail(f"ADR-0163 guard: {name} missing non-empty {field}")
        for src in sc.get("normative_sources", []):
            if not (ROOT / src).exists():
                fail(f"ADR-0163 guard: {name} normative source missing: {src}")

    # Dependency graph must be acyclic.
    visiting, done = set(), set()
    def visit(n: str) -> None:
        if n in done: return
        if n in visiting:
            fail(f"ADR-0163 guard: authorization dependency cycle at {n}")
            return
        visiting.add(n)
        for dep in repo_scopes[n].get("depends_on", []):
            if dep not in repo_scopes:
                fail(f"ADR-0163 guard: {n} depends on unknown scope {dep}")
            else:
                visit(dep)
        visiting.remove(n); done.add(n)
    for n in repo_scopes: visit(n)

    # Declared implementation layers must cover every scope once and obey dependencies.
    layers = impl.get("implementation_layers")
    if not isinstance(layers, list) or not layers:
        fail("ADR-0163 guard: implementation_layers missing")
    else:
        flat = [x for layer in layers if isinstance(layer, list) for x in layer]
        if len(flat) != len(set(flat)) or set(flat) != set(repo_scopes):
            fail("ADR-0163 guard: implementation_layers must cover each engineering scope exactly once")
        pos = {name: i for i, layer in enumerate(layers) if isinstance(layer, list) for name in layer}
        for name, sc in repo_scopes.items():
            for dep in sc.get("depends_on", []):
                if pos.get(dep, 10**6) >= pos.get(name, -1):
                    fail(f"ADR-0163 guard: implementation layer puts dependency {dep} no earlier than {name}")

    outcomes = impl.get("operation_outcomes", [])
    for required in ("Committed", "PendingNetwork", "PendingReconciliation", "Rejected"):
        if required not in outcomes:
            fail(f"ADR-0163 guard: operation outcome {required} missing")

    group = (ROOT / "docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md").read_text(encoding="utf-8")
    for token in ("INTENT_DURABLE", "CANDIDATE_PREPARED", "PUBLISHED_UNCONFIRMED", "No application message", "exact MLS Commit bytes/hash"):
        if token not in group:
            fail(f"ADR-0163 guard: group crash contract missing {token!r}")
    ui = (ROOT / "docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md").read_text(encoding="utf-8")
    for token in ("PendingReconciliation", "timeout after send", "second semantic operation"):
        if token not in ui:
            fail(f"ADR-0163 guard: UI/Core outcome contract missing {token!r}")
    home = (ROOT / "spec/protocol/home-api.md").read_text(encoding="utf-8")
    for token in ("across provider process restart", "same `request_id`", "outcome as unknown"):
        if token not in home:
            fail(f"ADR-0163 guard: Home idempotency contract missing {token!r}")
    handoff = (ROOT / "docs/process/ENGINEERING_HANDOFF.md").read_text(encoding="utf-8")
    if "`local_delivery`" not in handoff or "Direct gates" not in handoff or "ADR-0163" not in handoff:
        fail("ADR-0163 guard: engineering handoff lost 12-scope/direct-gate/readiness clarification")

    reqs = {r.get("id") for r in load_jsonl(ROOT / "machine/requirements.jsonl")}
    threats = {r.get("id") for r in load_jsonl(ROOT / "machine/threat_register.jsonl")}
    decisions = {r.get("id") for r in load_jsonl(ROOT / "machine/decisions.jsonl")}
    for rid in ("R-372", "R-373", "R-374", "R-375"):
        if rid not in reqs: fail(f"ADR-0163 guard: {rid} missing")
    for tid in ("T-146", "T-147"):
        if tid not in threats: fail(f"ADR-0163 guard: {tid} missing")
    if "D-184" not in decisions:
        fail("ADR-0163 guard: D-184 missing")

def validate_run4_release_candidate_integration_guardrails() -> None:
    """Run-4 bookkeeping/integration guards; no new product/protocol semantics."""
    deferred = {r.get("id"): r for r in load_jsonl(ROOT / "machine/deferred_product_questions.jsonl")}
    dp7 = deferred.get("DP-007", {})
    q7 = str(dp7.get("question", ""))
    for token in ("aggregate/decoded-size", "256-asset", "32 MiB"):
        if token not in q7:
            fail(f"Run-4 guard: DP-007 lost clarified frozen/deferred theme-limit boundary {token!r}")

    contract = load_json(ROOT / "machine/repository_contract.json") or {}
    scopes = list((contract.get("engineering_scopes") or {}).keys())
    engineer = (ROOT / "docs/process/ENGINEER_START_HERE.md").read_text(encoding="utf-8")
    if len(scopes) != 12 or "The twelve scopes are" not in engineer:
        fail("Run-4 guard: engineering handoff scope count must match the 12 machine-owned scopes")
    for scope in scopes:
        if f"`{scope}`" not in engineer:
            fail(f"Run-4 guard: ENGINEER_START_HERE omits engineering scope {scope}")

    license_doc = (ROOT / "LICENSE.md").read_text(encoding="utf-8")
    if "structured JSON design/reference artifacts under `docs/ux-design/`" not in license_doc:
        fail("Run-4 guard: LICENSE.md must explicitly cover structured docs/ux-design JSON artifacts")

def main() -> int:
    validate_0163_implementation_readiness_guardrails()
    validate_run4_release_candidate_integration_guardrails()
    validate_0166_consolidation_guardrails()
    validate_0145_bootstrap_retry_closure_guardrails()
    validate_0144_pre_review_sealed_delivery_guardrails()
    validate_0143_crash_safe_sealed_delivery_guardrails()
    validate_0142_sealed_delivery_guardrails()
    validate_0141_semantic_consistency_guardrails()
    validate_0140_async_prekey_guardrails()
    validate_0718_extension_guardrails()
    validate_0756_zero_control_interop_portability_guardrails()
    validate_0821_metadata_abuse_longevity_guardrails()
    validate_0920_verification_evidence_guardrails()
    validate_0924_scope_and_gate_guardrails()
    validate_0924_traceability_and_threats()
    validate_0924_hardening_content_guardrails()
    validate_0130_conformance_and_release_gates()
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
    r = subprocess.run([sys.executable, str(ROOT / "tools/render_gates.py"), "--check"], capture_output=True, text=True)
    if r.returncode != 0:
        fail(r.stderr.strip() or "GATE_PASS_CRITERIA.md is stale")

    if warnings:
        for w in warnings: print(f"WARN: {w}")
    if errors:
        for e in errors: print(f"ERROR: {e}", file=sys.stderr)
        print(f"architecture validation FAILED: {len(errors)} error(s), {len(warnings)} warning(s)", file=sys.stderr)
        return 1
    scopes = contract.get('engineering_scopes', {})
    print(f"architecture validation OK: {len(current_files())} current files; design_open={sum(1 for q in design if q.get('status')=='open')}; verification_open={sum(1 for q in verify if q.get('status')!='passed')}; pre_engineering_open={sum(1 for q in verify if q.get('pre_engineering_status') not in ('passed','waived_by_adr'))}; engineering_authorized={contract.get('engineering_authorized')}; scopes_authorized={sum(1 for v in scopes.values() if v.get('authorized'))}/{len(scopes)}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
