#!/usr/bin/env python3
"""V-015 focused provider-resource administration trace model (ADR-0138; R-287/R-294/R-295).

This is repo-local privacy evidence, not a real packet capture and not production code.  It checks the normative
Home API operation/auth table and emits explicit provider-observation traces for routine relay, descriptor, key,
RecoveryRecord and blob administration.  Routine provider-resource auth may reveal an opaque resource handle to the
provider that owns that resource, but it must not use AccountID, DeviceCredential or AccountControlState material as
authentication.  RecoveryRequest fan-out is intentionally separate: a participating mailbox Home sees AccountID and
that a recovery attempt is occurring, and the W0 metadata descriptors must declare both observations.

The model includes mutation controls so the check fails if identity/ACS auth is reintroduced, if unrelated resource
capabilities are reused, if RecoveryRecord read/write capabilities collapse, or if recovery fan-out disclosure is
hidden.
"""
from __future__ import annotations

import hashlib
import json
import re
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOME_API = ROOT / "spec/protocol/home-api.md"
DESC = ROOT / "spec/metadata/w0-metadata-exposure-descriptors.json"
OUT = ROOT / "verification/results/provider-resource-admin-trace-latest.json"

ROUTINE = {
    "mint deposit capability": ("home_relay_mailbox", "relay_admin_cap", "relay_admin"),
    "revoke deposit capability": ("home_relay_mailbox", "relay_admin_cap", "relay_admin"),
    "publish descriptor": ("discovery_publisher", "directory_publish_cap", "descriptor_publish"),
    "key bundle publish": ("key_material_provider", "key_publish_cap", "key_publish"),
    "key generation revoke": ("key_material_provider", "key_publish_cap", "key_publish"),
    "RecoveryRecord put": ("recovery_record_store", "recovery_record_write_cap", "recovery_record_write"),
    "RecoveryRecord get": ("recovery_record_store", "recovery_record_read_cap", "recovery_record_read"),
    "blob location repair": ("blob_store", "blob_repair_cap", "blob_repair"),
}
MAILBOX_ROLES = ("home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home")
FORBIDDEN_AUTH_MATERIAL = {
    "account_id",
    "device_credential",
    "device_credential_signature",
    "acs_generation",
    "acs_state_hash",
    "acs_controller_signature",
    "recovery_policy_vote",
}


def parse_operation_table(text: str) -> dict[str, dict[str, str]]:
    rows: dict[str, dict[str, str]] = {}
    for line in text.splitlines():
        if not line.startswith("|") or line.startswith("|---") or "| Operation |" in line:
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5:
            continue
        op, binding, auth, body, failures = cells[:5]
        rows[op] = {"binding": binding, "auth": auth, "body": body, "failures": failures}
    return rows


def auth_tokens(auth: str) -> set[str]:
    return set(re.findall(r"`([A-Za-z0-9_]+)`", auth))


def cap_bytes(provider: str, family: str) -> str:
    # Deterministic test-only stand-in for a 256-bit opaque CSPRNG/KDF capability.
    return hashlib.sha256(f"om-v015-provider-cap:{provider}:{family}".encode()).hexdigest()


def build_traces(rows: dict[str, dict[str, str]], *, mutation: str | None = None) -> list[dict]:
    providers = {
        "home_relay_mailbox": "home-A",
        "discovery_publisher": "directory-A",
        "key_material_provider": "keys-A",
        "recovery_record_store": "recovery-A",
        "blob_store": "blob-A",
    }
    traces: list[dict] = []
    for op, (role, expected_auth, family) in ROUTINE.items():
        spec = rows.get(op, {})
        trace = {
            "kind": "routine_provider_admin",
            "operation": op,
            "provider_role": role,
            "provider": providers[role],
            "auth_class": expected_auth,
            "auth_material": ["opaque_provider_resource_capability"],
            "capability_fingerprint": cap_bytes(providers[role], family),
            "resource_family": family,
            "payload_metadata": [],
            "spec_auth": spec.get("auth", ""),
            "binding": spec.get("binding", ""),
        }
        # This is payload/route metadata, not authentication material.  Keeping it explicit prevents the model from
        # accidentally claiming that key publication is anonymous to its own KeyMaterialProvider.
        if op in {"key bundle publish", "key generation revoke"}:
            trace["payload_metadata"].append("stable_device_correlator")
        traces.append(trace)

    # Signed RecoveryRequest fan-out is deliberately not a routine provider-resource administration operation.
    fanout = rows.get("RecoveryRequest fan-out", {})
    for role in MAILBOX_ROLES:
        traces.append({
            "kind": "recovery_request_fanout",
            "operation": "RecoveryRequest fan-out",
            "provider_role": role,
            "provider": f"{role}-A",
            "auth_class": "recovery_auth",
            "auth_material": ["signed_recovery_request"],
            "provider_observations": ["stable_account_id", "recovery_attempt"],
            "spec_auth": fanout.get("auth", ""),
            "binding": fanout.get("binding", ""),
        })

    if mutation == "identity_auth":
        traces[2]["auth_class"] = "device_auth"
        traces[2]["auth_material"].append("device_credential")
    elif mutation == "account_id_auth":
        traces[0]["auth_material"].append("account_id")
    elif mutation == "acs_auth":
        traces[3]["auth_material"].extend(["acs_generation", "acs_controller_signature"])
    elif mutation == "capability_reuse":
        traces[2]["capability_fingerprint"] = traces[0]["capability_fingerprint"]
    elif mutation == "recovery_caps_collapsed":
        put = next(t for t in traces if t.get("operation") == "RecoveryRecord put")
        get = next(t for t in traces if t.get("operation") == "RecoveryRecord get")
        get["capability_fingerprint"] = put["capability_fingerprint"]
    elif mutation == "fanout_hides_account":
        for t in traces:
            if t["kind"] == "recovery_request_fanout":
                t["provider_observations"] = ["recovery_attempt"]
    elif mutation == "fanout_hides_attempt":
        for t in traces:
            if t["kind"] == "recovery_request_fanout":
                t["provider_observations"] = ["stable_account_id"]
    return traces


def evaluate(rows: dict[str, dict[str, str]], descriptors: dict[str, dict[str, str]], traces: list[dict]) -> list[str]:
    bad: list[str] = []

    # Normative API table must use the expected provider-resource class for every routine operation.
    for op, (_, expected_auth, _) in ROUTINE.items():
        row = rows.get(op)
        if not row:
            bad.append(f"Home API operation missing: {op}")
            continue
        tokens = auth_tokens(row["auth"])
        if expected_auth not in tokens:
            bad.append(f"{op}: normative auth does not include {expected_auth}: {row['auth']}")
        if "device_auth" in tokens or "account_auth" in tokens or "acs_auth" in tokens:
            bad.append(f"{op}: identity/ACS authentication reintroduced in normative table: {sorted(tokens)}")

    fanout = rows.get("RecoveryRequest fan-out")
    if not fanout or "recovery_auth" not in auth_tokens(fanout["auth"]):
        bad.append("RecoveryRequest fan-out must use recovery_auth")

    routine = [t for t in traces if t["kind"] == "routine_provider_admin"]
    for t in routine:
        if t["auth_class"] not in {v[1] for v in ROUTINE.values()}:
            bad.append(f"{t['operation']}: unexpected routine auth class {t['auth_class']}")
        forbidden = FORBIDDEN_AUTH_MATERIAL & set(t["auth_material"])
        if forbidden:
            bad.append(f"{t['operation']}: forbidden identity/ACS auth material {sorted(forbidden)}")
        if len(bytes.fromhex(t["capability_fingerprint"])) < 32:
            bad.append(f"{t['operation']}: provider-resource capability below 256-bit model floor")

    # Reuse within one operation family is expected; reuse across unrelated provider/resource families is not.
    cap_scopes: dict[str, set[tuple[str, str]]] = {}
    for t in routine:
        cap_scopes.setdefault(t["capability_fingerprint"], set()).add((t["provider"], t["resource_family"]))
    for fp, scopes in cap_scopes.items():
        if len(scopes) > 1:
            bad.append(f"provider-resource capability reused across unrelated scopes: {sorted(scopes)} ({fp[:12]})")

    # RecoveryRecord read and write authority is deliberately split even for the same provider/record family.
    put = next((t for t in routine if t["operation"] == "RecoveryRecord put"), None)
    get = next((t for t in routine if t["operation"] == "RecoveryRecord get"), None)
    if put and get and put["capability_fingerprint"] == get["capability_fingerprint"]:
        bad.append("RecoveryRecord read/write capabilities collapsed into one bearer secret")

    # Fan-out is the explicit AccountID/recovery-attempt exception and must agree with machine descriptors.
    fanout_traces = [t for t in traces if t["kind"] == "recovery_request_fanout"]
    for t in fanout_traces:
        obs = set(t["provider_observations"])
        if "stable_account_id" not in obs:
            bad.append(f"{t['provider_role']}: recovery fan-out hides stable_account_id")
        if "recovery_attempt" not in obs:
            bad.append(f"{t['provider_role']}: recovery fan-out hides recovery_attempt")
        d = descriptors.get(t["provider_role"], {})
        if d.get("stable_account_id") == "no_by_design":
            bad.append(f"{t['provider_role']}: descriptor incorrectly forbids recovery AccountID observation")
        if d.get("recovery_attempt") == "no_by_design":
            bad.append(f"{t['provider_role']}: descriptor incorrectly forbids recovery-attempt observation")

    # Storage remains opaque to AccountID despite its recovery activity metadata.
    recovery_desc = descriptors.get("recovery_record_store", {})
    if recovery_desc.get("stable_account_id") != "no_by_design":
        bad.append("RecoveryRecord store descriptor must keep stable_account_id=no_by_design")

    return bad


def main() -> int:
    text = HOME_API.read_text()
    rows = parse_operation_table(text)
    desc_doc = json.loads(DESC.read_text())
    descriptors = {d["role"]: d["exposures"] for d in desc_doc["descriptors"]}

    traces = build_traces(rows)
    violations = evaluate(rows, descriptors, traces)
    mutation_names = (
        "identity_auth",
        "account_id_auth",
        "acs_auth",
        "capability_reuse",
        "recovery_caps_collapsed",
        "fanout_hides_account",
        "fanout_hides_attempt",
    )
    mutation_samples: dict[str, list[str]] = {}
    mutation_caught: dict[str, bool] = {}
    for name in mutation_names:
        problems = evaluate(rows, descriptors, build_traces(rows, mutation=name))
        mutation_samples[name] = problems[:3]
        mutation_caught[name] = bool(problems)

    passed = not violations and all(mutation_caught.values())
    payload = {
        "model": "provider_resource_admin_trace",
        "gates": ["V-015"],
        "requirements": ["R-287", "R-294", "R-295"],
        "trace_class": "repo_local_provider_observation_model_not_packet_capture",
        "routine_trace_count": sum(t["kind"] == "routine_provider_admin" for t in traces),
        "recovery_fanout_trace_count": sum(t["kind"] == "recovery_request_fanout" for t in traces),
        "traces": traces,
        "violations": violations,
        "security_mutations_caught": mutation_caught,
        "security_mutation_samples": mutation_samples,
        "passed": passed,
        "gate_status_effect": "repo_local_criterion_evidence_only_independent_review_still_required",
    }
    OUT.write_text(json.dumps(payload, indent=1, sort_keys=True) + "\n")
    print(json.dumps({
        "passed": passed,
        "routine_traces": payload["routine_trace_count"],
        "recovery_fanout_traces": payload["recovery_fanout_trace_count"],
        "violations": violations,
        "security_mutations_caught": mutation_caught,
    }, indent=1, sort_keys=True))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
