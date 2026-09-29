#!/usr/bin/env python3
"""Bounded sanity checks for ADR-0162 protocol evolution policy; not independent/formal evidence."""
from __future__ import annotations
import json
from pathlib import Path
R=Path(__file__).resolve().parents[2]
P=json.loads((R/"machine/protocol_evolution_policy.json").read_text())

CASES=[]
def case(name, cond): CASES.append((name,bool(cond)))

# Change classes
ids=[x["id"] for x in P["change_classes"]]
case("all_change_classes_unique", len(ids)==len(set(ids)))
case("all_change_classes_have_artifacts", all(x["required_artifacts"] for x in P["change_classes"]))
case("no_change_class_mutates_published_semantics", all(x["mutates_published_semantics"] is False for x in P["change_classes"]))

# Negotiation mechanics
case("floor_is_primary_order", P["negotiation"]["primary_order"]=="security_generation_for_conversation_type_descending")
case("profile_id_only_tiebreak", P["negotiation"]["tie_break"]=="smallest_numeric_profile_id_only_after_equal_security_generation")
case("no_preference_guessing", P["negotiation"]["no_preference_guessing"] is True)
case("empty_intersection_is_not_fallback", "safe_failure_per_underlying_protocol" in P["negotiation"]["empty_intersection"])
case("deprecated_new_traffic_rule_present", "exclude_deprecated_for_new_traffic" in P["negotiation"]["deprecated_rule"])

# Simulate selection: highest generation wins, ID only breaks equal-generation ties.
def select(candidates, floor):
    c=[x for x in candidates if x["gen"]>=floor and x["allowed"]]
    if not c: return None
    maxgen=max(x["gen"] for x in c); c=[x for x in c if x["gen"]==maxgen]
    nondep=[x for x in c if x["status"]!="deprecated"]
    if nondep: c=nondep
    return min(c,key=lambda x:x["id"])
case("higher_generation_beats_lower_id", select([
    {"id":2,"gen":1,"status":"supported","allowed":True}, {"id":100,"gen":2,"status":"supported","allowed":True}],1)["id"]==100)
case("equal_generation_smallest_id_wins", select([
    {"id":7,"gen":2,"status":"supported","allowed":True}, {"id":3,"gen":2,"status":"supported","allowed":True}],1)["id"]==3)
case("below_floor_rejected", select([{ "id":1,"gen":1,"status":"supported","allowed":True}],2) is None)
case("forbidden_local_policy_rejected", select([{ "id":1,"gen":2,"status":"supported","allowed":False}],1) is None)
case("deprecated_loses_to_nondeprecated_same_generation", select([
    {"id":2,"gen":3,"status":"deprecated","allowed":True}, {"id":9,"gen":3,"status":"supported","allowed":True}],1)["id"]==9)
case("deprecated_can_remain_for_continuity_when_only_candidate", select([
    {"id":2,"gen":3,"status":"deprecated","allowed":True}],3)["id"]==2)
case("profile_id_not_security_order", select([
    {"id":1,"gen":2,"status":"supported","allowed":True}, {"id":65530,"gen":1,"status":"supported","allowed":True}],1)["id"]==1)

# Extension evolution
E=P["extension_evolution"]
case("extension_registry_key_is_id_and_version", E["registry_key"]==["extension_id","version"])
case("criticality_immutable", E["criticality_immutable_per_extension_id"] is True)
case("bang_ids_permanently_critical", E["bang_ids_are_permanently_critical"] is True)
case("future_critical_uses_bang_id", E["new_critical_semantics_require_bang_id"] is True)
case("noncritical_new_version_requires_fallback", E["same_id_new_version_requires_noncritical_backward_compatible_fallback"] is True)
case("security_change_needs_new_lineage", E["authority_or_security_semantic_change_requires_new_id_or_profile"] is True)
case("logical_schema_cannot_expand_w0", E["logical_schema_cannot_expand_frozen_w0_wire"] is True)

# Forbidden shortcuts
F=set(P["agent_forbidden_shortcuts"])
for x in ["highest_version_as_security_rule","highest_profile_id_as_security_rule","in_place_crypto_profile_mutation","in_place_extension_criticality_change","silent_floor_downgrade","live_registry_or_project_website_as_protocol_authority"]:
    case(f"forbidden_{x}", x in F)

# Change-control completeness
required=set(P["required_change_control_steps"])
for x in ["identify_affected_surface","classify_change","locate_current_lineage","update_requirements_threats_invariants_conformance","run_protocol_evolution_validator","preserve_old_published_meaning","record_migration_or_fallback"]:
    case(f"change_control_{x}", x in required)

failed=[n for n,ok in CASES if not ok]
print(f"ADR-0162 protocol-evolution sanity: {len(CASES)} cases; {len(failed)} failed")
if failed:
    print("FAILED:")
    for n in failed: print(f"- {n}")
    raise SystemExit(1)
for n,_ in CASES: print(f"PASS {n}")
