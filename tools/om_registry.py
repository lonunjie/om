#!/usr/bin/env python3
"""Keep machine/file_registry.json in step with the tree.

  python3 tools/om_registry.py sync     # add unregistered files (classified by folder), drop vanished ones
  python3 tools/om_registry.py check    # exit 1 if sync would change anything

Classification follows the folder layout (see AGENTS.md "Where things live"). Existing entries keep their
hand-set class/domain/authority; only the license is normalized to LICENSE.md. A new file whose domain cannot be
inferred (for example a new spec/ schema) is registered with domain "unassigned", and the validator rejects it
until someone assigns a real domain.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
ROOT = Path(__file__).resolve().parents[1]
REG = ROOT / "machine/file_registry.json"

ARCH_DOMAINS = {
    "identity": "identity_control_recovery", "crypto": "crypto_1to1", "groups": "groups",
    "discovery": "discovery_first_contact", "delivery": "relay_reachability", "storage": "storage_retention",
    "events": "time_ordering_events", "calls": "calls_media", "network": "network_lan_transport",
    "client": "client_endpoint_distribution", "safety": "content_abuse_evidence",
    "providers": "provider_deployment_supply_chain", "ux": "ux_accessibility_personalization",
    "ecosystem": "bots_agents_interop",
}
CODE = {".py", ".mjs", ".js", ".ts", ".rs", ".go", ".tla", ".cfg", ".sh", ".toml", ".lock", ".cddl", ".html", ".css"}
CC, AP = "CC-BY-SA-4.0", "Apache-2.0"
WEB_PLUMBING = {"package.json", "pnpm-workspace.yaml", "pnpm-lock.yaml", "tsconfig.json", "tsconfig.base.json", "eslint.config.js",
                "eslint.policy.js", "eslint.policy.d.ts", "vitest.config.ts"}


def license_for(path: str) -> str:
    """LICENSE.md: prose is CC-BY-SA-4.0; spec/machine/verification/tools and schemas are Apache-2.0."""
    if path.startswith("LICENSES/"):
        return "upstream-license-text"
    if path.startswith(("spec/", "machine/", "tools/", ".github/")) or path in {"Makefile", "REUSE.toml", ".gitignore"}:
        return AP
    if path.startswith(("packages/", "apps/")):
        return "MPL-2.0"  # shared client libraries and the reference web client (LICENSING_AND_CONTRIBUTION_POLICY.md)
    if path in WEB_PLUMBING:
        return AP
    if path.startswith("verification/") and not path.endswith(".md"):
        return AP
    if path.startswith("docs/ux-design/") and (path.endswith(".json") or "/prototype/" in path and not path.endswith(".md")):
        return AP
    return CC


def classify(path: str) -> dict:
    p = path
    name = p.rsplit("/", 1)[-1]
    ext = "." + name.rsplit(".", 1)[-1] if "." in name else ""

    def e(cls, domain, authority):
        return {"path": p, "class": cls, "domain": domain, "license": license_for(p), "authority": authority}

    if p.startswith("LICENSES/"):
        return e("license_text", "constitution_governance", "legal_reference")
    if p.startswith("decisions/"):
        return e("adr", "architecture_decisions", "decision_record" if name.startswith("ADR-") else "decision_index")
    if p.startswith("docs/checkpoints/"):
        return e("checkpoint", "architecture_process", "historical_checkpoint_record" if name.startswith("CHECKPOINT_") else "historical_index")
    if p.startswith("docs/reports/"):
        return e("report", "architecture_process", "working_record_non_normative")
    if p.startswith("docs/research/"):
        return e("research", "research", "evidence_non_normative")
    if p.startswith("docs/reviews/"):
        return e("architecture_doc", "architecture_process", "canonical_normative")
    if p.startswith("docs/architecture/"):
        folder = p.split("/")[2]
        return e("architecture_doc", ARCH_DOMAINS.get(folder, "unassigned"), "canonical_normative")
    if p.startswith("docs/constitution/"):
        return e("architecture_doc", "constitution_governance", "canonical_normative")
    if p.startswith(("docs/overview/", "docs/process/")) or p == "docs/README.md":
        return e("architecture_doc", "architecture_process", "canonical_normative")
    if p.startswith("docs/threat-model/"):
        return e("threat_model", "security_crosscutting", "normative_security_analysis")
    if p.startswith("docs/ux-design/"):
        auth = "logical_design_schema" if ext == ".json" else ("prototype_non_normative" if "/prototype/" in p else "design_reference")
        return e("design_artifact", "ux_accessibility_personalization", auth)
    if p.startswith("spec/"):
        return e("protocol_schema" if p.startswith("spec/protocol/") else "spec_artifact", "unassigned", "normative_logical_schema")
    if p.startswith("machine/"):
        return e("machine_truth", "architecture_process", "machine_truth")
    if p.startswith("verification/"):
        if ext == ".md":
            return e("verification_doc", "architecture_verification", "verification_evidence_non_normative")
        if p.startswith(("verification/results/", "verification/vectors/")) or (ext == ".json" and "/prototypes/" not in p):
            return e("verification_artifact", "architecture_verification", "machine_verification_evidence")
        return e("verification_tool", "architecture_verification", "executable_verification_evidence")
    if p.startswith(("packages/", "apps/")):
        return e("product_code", "client_endpoint_distribution", "product_implementation")
    if p in WEB_PLUMBING:
        return e("repo_plumbing", "architecture_process", "executable_guardrail")
    if p.startswith("tools/"):
        return e("architecture_tool", "architecture_process", "executable_guardrail")
    if p in {"CLAUDE.md", "GEMINI.md"}:
        return e("agent_pointer", "architecture_process", "pointer_non_normative")
    if p in {"Makefile", "REUSE.toml", ".gitignore"} or p.startswith(".github/"):
        return e("repo_plumbing", "architecture_process", "executable_guardrail")
    if p == "CHANGELOG.md":
        return e("changelog", "architecture_process", "historical_index")
    return e("architecture_doc", "architecture_process", "canonical_normative")


def current() -> list[str]:
    import validate_architecture as va  # reuse the validator's notion of "current files"
    return [va.rel(p) for p in va.current_files()]


def run(write: bool) -> int:
    reg = json.loads(REG.read_text(encoding="utf-8"))
    contract = json.loads((ROOT / "machine/repository_contract.json").read_text(encoding="utf-8"))
    skip = set(contract.get("manifest_excludes", []))
    files = [f for f in current() if f not in skip]
    have = {e["path"]: e for e in reg["files"]}
    added = [f for f in files if f not in have]
    removed = [p for p in have if p not in set(files)]
    relicensed = []
    out = []
    for f in sorted(files):
        if f in have:
            ent = dict(have[f])
            lic = license_for(f)
            if ent.get("license") != lic:
                relicensed.append(f"{f}: {ent.get('license')} -> {lic}")
                ent["license"] = lic
            out.append(ent)
        else:
            out.append(classify(f))
    for f in added:
        print(f"+ {f}  [{classify(f)['class']} / {classify(f)['domain']}]")
    for f in removed:
        print(f"- {f}")
    for r in relicensed:
        print(f"~ {r}")
    unassigned = [x["path"] for x in out if x["domain"] == "unassigned"]
    for u in unassigned:
        print(f"! {u}: set a real domain by hand", file=sys.stderr)
    dirty = bool(added or removed or relicensed)
    if write and dirty:
        reg["files"] = out
        REG.write_text(json.dumps(reg, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"om_registry: {len(added)} added, {len(removed)} removed, {len(relicensed)} relicensed"
          + ("" if write else " (dry run)"))
    return 1 if (not write and dirty) or unassigned else 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    if cmd not in {"sync", "check"}:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(run(cmd == "sync"))
