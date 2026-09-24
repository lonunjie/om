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
    contract = load_json(ROOT / "machine/repository_contract.json")
    if isinstance(contract, dict) and contract.get("engineering_authorized") is not False:
        fail("09:20 guard: phase1 evidence does not authorize engineering")


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
        for field in ("pre_engineering_criteria", "pre_release_criteria", "independence", "unlock_scopes", "pre_engineering_status"):
            if field not in g:
                fail(f"09:24 guard: {gid} missing {field}")
        if g.get("pre_engineering_status") not in {"open", "passed", "failed", "waived_by_adr"}:
            fail(f"09:24 guard: {gid} invalid pre_engineering_status")
        if not isinstance(g.get("interface_impact"), list) or g.get("feasibility_status") not in {"open", "evidenced"}:
            fail(f"ADR-0113 guard: {gid} must declare interface_impact (list) and feasibility_status (open|evidenced)")
        for s_ in g.get("interface_impact", []) or []:
            if s_ not in scopes:
                fail(f"ADR-0113 guard: {gid} interface_impact names unknown scope {s_}")
        if g.get("feasibility_status") == "evidenced" and not g.get("evidence"):
            fail(f"ADR-0113 guard: {gid} feasibility evidenced without evidence links")
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
    open_design = [q["id"] for q in design if q.get("status") == "open" and q.get("blocks_engineering")]
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
            if open_design:
                fail(f"09:24 guard: scope {name} authorized with open design blockers {open_design}")
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
    summary = load_json(ROOT / "verification/results/phase1-summary.json")
    if isinstance(summary, dict) and int(summary.get("continuity_assessment_cases", 0)) < 1620:
        fail("09:24 guard: continuity assessment evidence missing or regressed")


def main() -> int:
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
