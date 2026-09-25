#!/usr/bin/env python3
"""V-015 trace model: W0 flows -> what each provider role observes -> checked against its descriptor.

Each W0 flow is modeled at the level of the outer, provider-visible messages the design specifies
(opaque capabilities, sealed IntroGate requests, opaque push wakes, encrypted blobs, H(RecoveryHandle)).
A role observes a field when a message it handles carries it in the clear. Checks:

  1. no under-claim: a role never observes a field its descriptor marks no_by_design;
  2. no over-claim: a field marked "always" is observed every time the role takes part;
  3. METADATA_THREAT_MODEL.md §20 invariant 1-2: no provider role ever observes stable_account_id or
     conversation_id on default paths;
  4. mitigation claims: on the privacy-enhanced path no Home/relay-class role observes the client network address.

Abstract model of the specified flows (written by a Claude agent). Real packet/provider/browser traces are the
V-015 pre-release criterion; see docs/process/METADATA_TRACE_TEST_PLAN.md.
"""
from __future__ import annotations

import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DESC = ROOT / "spec/metadata/w0-metadata-exposure-descriptors.json"
OUT = ROOT / "verification/results/metadata-trace-latest.json"
HOME_CLASS = {"home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home", "key_material_provider", "blob_store",
              "introgate", "discovery_publisher", "recovery_record_store"}


def msg(role, *fields, ip=True):
    return (role, set(fields) | ({"client_network_address"} if ip else set()))


def flows(recipient_home: str, privacy_path: bool, multi_device: bool, push_on: bool):
    """Yield (flow_name, [(role, observed_fields)]) for one configuration."""
    ip = not privacy_path
    # ADR-0131 rule 4: the unencrypted Double Ratchet header travels inside the routed envelope
    send = [msg(recipient_home, "message_timing", "ciphertext_size", "ratchet_header_shape", ip=ip),  # deposit (opaque capability)
            msg(recipient_home, "message_timing", "ciphertext_size", "ratchet_header_shape", ip=True)]  # recipient drains
    if multi_device:
        send.append(msg(recipient_home, "message_timing", "ciphertext_size", "device_count", ip=True))
    if push_on:
        send.append(("push_provider", {"push_timing"} | ({"device_count"} if multi_device else set())))
    if recipient_home == "t0_sponsored_mailbox":
        send = [(r, f | {"sponsor_relationship"}) if r == recipient_home else (r, f) for r, f in send]
    if recipient_home == "t1_hosted_home":
        send = [(r, f | {"membership_or_roster"}) if r == recipient_home else (r, f) for r, f in send]
    yield "send_1to1", send
    yield "first_contact", [
        msg("discovery_publisher", "query_plaintext", ip=ip),
        msg("introgate", "message_timing", ip=ip),                                  # sealed request under a ticket
        msg("key_material_provider", "contact_graph_edge", ip=ip),                  # fetching B's bundle links fetcher to B
    ]
    yield "attachment", [msg("blob_store", "blob_size", ip=ip), msg("blob_store", "blob_size", "message_timing", ip=True)]
    yield "recovery", [msg("recovery_record_store", "recovery_attempt", ip=ip),
                       ("platform_credential_manager", {"recovery_attempt"})]
    yield "pwa_load", [msg("clienthost_origin", ip=True)]
    yield "lan_rendezvous", [msg("lan_observer", "lan_advertisement", ip=True)]


def main() -> int:
    spec = json.loads(DESC.read_text())
    desc = {d["role"]: d["exposures"] for d in spec["descriptors"]}
    homes = ["home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home"]
    violations, observed_any, cases = [], {r: set() for r in desc}, 0
    for home, privacy, multi, push in itertools.product(homes, (False, True), (False, True), (False, True)):
        for name, obs in flows(home, privacy, multi, push):
            cases += 1
            per_role = {}
            for role, fields in obs:
                per_role.setdefault(role, set()).update(fields)
            for role, fields in per_role.items():
                if role not in desc:
                    violations.append(f"{name}: role {role} has no descriptor")
                    continue
                observed_any[role] |= fields
                for f in fields:
                    if desc[role].get(f, "no_by_design") == "no_by_design":
                        violations.append(f"under-claim: {role} observes {f} in {name} ({home}, privacy={privacy})")
                if not privacy:
                    for f, v in desc[role].items():
                        if v == "always" and f not in fields and not (f == "client_network_address"):
                            violations.append(f"over-claim: {role} marks {f} always but {name} ({home}) does not reveal it")
                if {"stable_account_id", "conversation_id"} & fields:
                    violations.append(f"§20 invariant: {role} sees canonical id in {name}")
            if privacy:
                first = obs[0]
                if first[0] in HOME_CLASS and "client_network_address" in first[1]:
                    violations.append(f"privacy path leaks sender address to {first[0]} in {name}")
    unused_always = [f"{r}.{f}" for r, e in desc.items() for f, v in e.items() if v == "always" and f not in observed_any[r]]
    violations += [f"over-claim: {x} never observed in any flow" for x in unused_always]
    payload = {"model": "metadata_trace_w0", "gates": ["V-015"], "cases": cases, "roles": sorted(desc),
               "violations": violations, "passed": not violations,
               "gate_status_effect": "evidence_only_no_gate_automatically_passed"}
    OUT.write_text(json.dumps(payload, indent=1) + "\n")
    print(json.dumps({"cases": cases, "roles": len(desc), "passed": payload["passed"], "violations": violations[:8]}, indent=1))
    return 0 if not violations else 1


if __name__ == "__main__":
    raise SystemExit(main())
