#!/usr/bin/env python3
"""Bounded structural evidence for ADR-0140 AsyncKeyBundle v1 semantics.

Architecture/encoding check only. This does not replace V-001 cryptographic-runtime
or V-005 two-codec execution on a Rust-capable runner.
"""
from __future__ import annotations

import gzip
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "verification" / "cbor"))
from python_canonical import enc  # noqa: E402


def bx(n: int, byte: int) -> dict:
    return {"$bytes": (bytes([byte]) * n).hex()}


def sha(label: str, body) -> bytes:
    return hashlib.sha256(enc([label, body])).digest()


def generation_core(device, profile, generation, ec, pq, fallback):
    return sha(
        "OM-PREKEY-GENERATION-CORE-v1",
        [device, profile, generation, ec, pq, fallback],
    )


def otk_context(device, profile, generation, pool, kind, key_id, public_key):
    return enc([1, device, profile, generation, pool, kind, key_id, public_key])


def observe(state: dict, account: bytes, device: bytes, profile: int, generation: int, core: bytes):
    key = (account, device, profile)
    old = state.get(key)
    if old is None:
        state[key] = (generation, core)
        return "accept-first"
    old_generation, old_core = old
    if generation < old_generation:
        return "reject-rollback"
    if generation == old_generation:
        if core != old_core:
            return "reject-equivocation"
        return "accept-equal"
    state[key] = (generation, core)
    return "accept-advance"


def merge(a, b):
    if a is None:
        return b, "take-right"
    if b is None:
        return a, "take-left"
    if a[0] > b[0]:
        return a, "take-left"
    if b[0] > a[0]:
        return b, "take-right"
    if a[1] != b[1]:
        return None, "security-conflict"
    return a, "equal"


def require(cond: bool, msg: str, failures: list[str]):
    if not cond:
        failures.append(msg)


def main() -> int:
    failures: list[str] = []

    registry = json.loads((ROOT / "spec/registries/om-numeric-fields.json").read_text())
    cddl = (ROOT / "spec/cddl/keys.cddl").read_text()
    ds = json.loads((ROOT / "spec/registries/om-domain-separation.json").read_text())
    schema = json.loads((ROOT / "spec/protocol/async-key-material.logical.schema.json").read_text())
    session = json.loads((ROOT / "spec/protocol/one-to-one-session-state.logical.schema.json").read_text())

    # Registry/CDDL guardrails: keys 3/4 are retired, not reinterpreted.
    akb = registry["structs"]["AsyncKeyBundle"]
    reserved = {int(x["key"]): x for x in akb.get("reserved", [])}
    require(3 in reserved and 4 in reserved, "AsyncKeyBundle retired keys 3/4 not reserved", failures)
    fields = {int(x["key"]): x for x in akb["fields"]}
    require(3 not in fields and 4 not in fields, "AsyncKeyBundle keys 3/4 still active", failures)
    require("acs_generation" not in cddl and "acs_state_hash" not in cddl,
            "keys.cddl still exposes ACS fields in AsyncKeyBundle", failures)

    labels = {x["label"]: x for x in ds["labels"]}
    require("OM-PREKEY-GENERATION-CORE-v1" in labels, "generation-core label missing", failures)
    require("OM-PREKEY-SIG-v1" in labels, "prekey-signature label missing", failures)
    prekey_construction = labels.get("OM-PREKEY-SIG-v1", {}).get("construction", "")
    for token in ["subject_device_id", "crypto_profile", "bundle_generation", "provider_pool_id", "kind", "key_id", "public_key"]:
        require(token in prekey_construction, f"OTK signature construction missing {token}", failures)

    # Logical specs must expose recipient freshness as durable relationship security state.
    schema_text = json.dumps(schema, sort_keys=True)
    session_text = json.dumps(session, sort_keys=True)
    for token in ["highest_generation", "generation_core_hash"]:
        require(token in schema_text, f"async-key schema missing {token}", failures)
        require(token in session_text, f"session state missing {token}", failures)

    # Bounded freshness transitions. Pool is deliberately absent from the floor key.
    account = bytes([0xA1]) * 32
    device = bytes([0xD1]) * 16
    profile = 1
    pool_a, pool_b = bx(16, 0x31), bx(16, 0x32)
    d = bx(16, 0xD1)
    ec1, pq1, fb1 = bx(64, 0x11), bx(64, 0x12), bx(64, 0x13)
    ec2, pq2, fb2 = bx(64, 0x21), bx(64, 0x22), bx(64, 0x23)
    core1 = generation_core(d, profile, 1, ec1, pq1, fb1)
    core2 = generation_core(d, profile, 2, ec2, pq2, fb2)
    core2_alt = generation_core(d, profile, 2, bx(64, 0x24), pq2, fb2)
    state = {}
    transitions = [
        observe(state, account, device, profile, 1, core1),
        # Same generation/core through another provider pool is valid because pool is not part of generation core.
        observe(state, account, device, profile, 1, core1),
        observe(state, account, device, profile, 2, core2),
        observe(state, account, device, profile, 1, core1),
        observe(state, account, device, profile, 2, core2_alt),
    ]
    require(transitions == ["accept-first", "accept-equal", "accept-advance", "reject-rollback", "reject-equivocation"],
            f"freshness transition mismatch: {transitions}", failures)

    # Monotonic private-state merge: an old restored backup cannot lower a surviving floor.
    merged, outcome = merge((1, core1), (2, core2))
    require(outcome == "take-right" and merged == (2, core2), "restore merge lowered freshness floor", failures)
    merged, outcome = merge((2, core2), (2, core2_alt))
    require(outcome == "security-conflict" and merged is None, "equal-generation conflict not detected", failures)

    # Every context coordinate affects the one-time-prekey signature preimage.
    key_id = bx(16, 0x41)
    pub = bx(32, 0x42)
    base = otk_context(d, profile, 2, pool_a, 3, key_id, pub)
    variants = {
        "device": otk_context(bx(16, 0xD2), profile, 2, pool_a, 3, key_id, pub),
        "profile": otk_context(d, 2, 2, pool_a, 3, key_id, pub),
        "generation": otk_context(d, profile, 3, pool_a, 3, key_id, pub),
        "pool": otk_context(d, profile, 2, pool_b, 3, key_id, pub),
        "kind": otk_context(d, profile, 2, pool_a, 4, key_id, pub),
        "key_id": otk_context(d, profile, 2, pool_a, 3, bx(16, 0x43), pub),
        "public_key": otk_context(d, profile, 2, pool_a, 3, key_id, bx(32, 0x44)),
    }
    for name, value in variants.items():
        require(value != base, f"OTK context mutation {name} did not change signed preimage", failures)

    # Current canonical V-005 vectors include explicit retired-field negatives.
    with gzip.open(ROOT / "verification/vectors/v005-w0-object-vectors.json.gz", "rt") as f:
        vectors = json.load(f)
    ids = {v["id"] for v in vectors["vectors"]}
    for vid in [
        "AsyncKeyBundle/neg/retired-acs-generation-key-3",
        "AsyncKeyBundle/neg/retired-acs-state-hash-key-4",
    ]:
        require(vid in ids, f"missing V-005 negative {vid}", failures)

    result = {
        "gate_relationship": ["V-001", "V-003", "V-005", "V-015"],
        "scope": "ADR-0140 structural architecture semantics; not a substitute for Rust/wasm crypto execution",
        "passed": not failures,
        "checks": {
            "retired_provider_visible_acs_fields": not any("keys 3/4" in x or "ACS fields" in x for x in failures),
            "generation_floor_global_across_provider_pools": transitions[:3] == ["accept-first", "accept-equal", "accept-advance"],
            "rollback_rejected": transitions[3] == "reject-rollback",
            "same_generation_equivocation_rejected": transitions[4] == "reject-equivocation",
            "restore_merge_monotonic": merge((1, core1), (2, core2))[1] == "take-right",
            "equal_generation_merge_conflict": merge((2, core2), (2, core2_alt))[1] == "security-conflict",
            "otk_context_all_coordinates_bound": all(v != base for v in variants.values()),
            "v005_retired_key_negatives_present": all(v in ids for v in [
                "AsyncKeyBundle/neg/retired-acs-generation-key-3",
                "AsyncKeyBundle/neg/retired-acs-state-hash-key-4",
            ]),
        },
        "freshness_trace": transitions,
        "otk_context_mutations": sorted(variants),
        "failures": failures,
    }
    out = ROOT / "verification/results/async-prekey-adr0140-latest.json"
    out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
