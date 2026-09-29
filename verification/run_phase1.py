#!/usr/bin/env python3
"""Run checkpoint-local phase-1 architecture verification evidence."""
from __future__ import annotations
import argparse, json, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNS = [
    ([sys.executable, str(ROOT / "verification/bounded/authority_models.py")], "verification/bounded/authority_models.py"),
    ([sys.executable, str(ROOT / "verification/bounded/conversation_convergence.py")], "verification/bounded/conversation_convergence.py"),
    ([sys.executable, str(ROOT / "verification/bounded/wire_boundary_models.py")], "verification/bounded/wire_boundary_models.py"),
    ([sys.executable, str(ROOT / "verification/cbor/run_cross_impl.py")], "verification/cbor/run_cross_impl.py"),
    ([sys.executable, str(ROOT / "verification/bounded/continuity_dry_run.py")], "verification/bounded/continuity_dry_run.py"),
    ([sys.executable, str(ROOT / "verification/bounded/continuity_assessment.py")], "verification/bounded/continuity_assessment.py"),
    ([sys.executable, str(ROOT / "verification/bounded/home_tiers_model.py")], "verification/bounded/home_tiers_model.py"),
    ([sys.executable, str(ROOT / "verification/bounded/metadata_trace_model.py")], "verification/bounded/metadata_trace_model.py"),
    ([sys.executable, str(ROOT / "verification/bounded/introgate_sybil_model.py")], "verification/bounded/introgate_sybil_model.py"),
    ([sys.executable, str(ROOT / "verification/bounded/acs_authorization_table.py")], "verification/bounded/acs_authorization_table.py"),
    ([sys.executable, str(ROOT / "verification/bounded/acs_tla_crosscheck.py")], "verification/bounded/acs_tla_crosscheck.py"),
    ([sys.executable, str(ROOT / "verification/bounded/metadata_w0_rules.py")], "verification/bounded/metadata_w0_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/provider_resource_admin_trace.py")], "verification/bounded/provider_resource_admin_trace.py"),
    ([sys.executable, str(ROOT / "verification/bounded/abuse_w0_rules.py")], "verification/bounded/abuse_w0_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/privacy_pass_issuance.py")], "verification/bounded/privacy_pass_issuance.py"),
    ([sys.executable, str(ROOT / "verification/bounded/async_prekey_adr0140_rules.py")], "verification/bounded/async_prekey_adr0140_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/design_consistency_adr0141_rules.py")], "verification/bounded/design_consistency_adr0141_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/provider_negative_capabilities.py")], "verification/bounded/provider_negative_capabilities.py"),
    ([sys.executable, str(ROOT / "verification/bounded/relationship_context_surface.py")], "verification/bounded/relationship_context_surface.py"),
    ([sys.executable, str(ROOT / "verification/bounded/sealed_delivery_adr0142_rules.py")], "verification/bounded/sealed_delivery_adr0142_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/sealed_delivery_adr0144_composition.py")], "verification/bounded/sealed_delivery_adr0144_composition.py"),
    ([sys.executable, str(ROOT / "verification/bounded/sealed_delivery_adr0145_closure.py")], "verification/bounded/sealed_delivery_adr0145_closure.py"),
    ([sys.executable, str(ROOT / "verification/bounded/sealed_delivery_adr0146_final_closure.py")], "verification/bounded/sealed_delivery_adr0146_final_closure.py"),
    ([sys.executable, str(ROOT / "verification/bounded/recovery_durable_timer_adr0147.py")], "verification/bounded/recovery_durable_timer_adr0147.py"),
    ([sys.executable, str(ROOT / "verification/bounded/recovery_timer_adr0154.py")], "verification/bounded/recovery_timer_adr0154.py"),
    ([sys.executable, str(ROOT / "verification/bounded/sealed_delivery_adr0147_lifecycle_restore.py")], "verification/bounded/sealed_delivery_adr0147_lifecycle_restore.py"),
    ([sys.executable, str(ROOT / "verification/bounded/account_private_sync_adr0148.py")], "verification/bounded/account_private_sync_adr0148.py"),
    ([sys.executable, str(ROOT / "verification/bounded/account_private_sync_adr0149_hardening.py")], "verification/bounded/account_private_sync_adr0149_hardening.py"),
    ([sys.executable, str(ROOT / "verification/bounded/extensions_adr0148.py")], "verification/bounded/extensions_adr0148.py"),
    ([sys.executable, str(ROOT / "verification/bounded/extensions_adr0149_hardening.py")], "verification/bounded/extensions_adr0149_hardening.py"),
    ([sys.executable, str(ROOT / "verification/bounded/endpoint_profile_adr0148.py")], "verification/bounded/endpoint_profile_adr0148.py"),
    ([sys.executable, str(ROOT / "verification/bounded/profile_context_adr0149_hardening.py")], "verification/bounded/profile_context_adr0149_hardening.py"),
    ([sys.executable, str(ROOT / "verification/bounded/clienthost_update_adr0148.py")], "verification/bounded/clienthost_update_adr0148.py"),
    ([sys.executable, str(ROOT / "verification/bounded/clienthost_update_adr0149_authority.py")], "verification/bounded/clienthost_update_adr0149_authority.py"),
    ([sys.executable, str(ROOT / "verification/bounded/lan_route_adr0150.py")], "verification/bounded/lan_route_adr0150.py"),
    ([sys.executable, str(ROOT / "verification/bounded/rich_list_adr0150.py")], "verification/bounded/rich_list_adr0150.py"),
    ([sys.executable, str(ROOT / "verification/bounded/lan_privacy_adr0150.py")], "verification/bounded/lan_privacy_adr0150.py"),
    ([sys.executable, str(ROOT / "verification/bounded/platform_reference_adr0150.py")], "verification/bounded/platform_reference_adr0150.py"),
    ([sys.executable, str(ROOT / "verification/bounded/android_wake_adr0152.py")], "verification/bounded/android_wake_adr0152.py"),
    ([sys.executable, str(ROOT / "verification/bounded/desktop_personrecord_adr0152.py")], "verification/bounded/desktop_personrecord_adr0152.py"),
    ([sys.executable, str(ROOT / "verification/bounded/design_closure_set1_rules.py")], "verification/bounded/design_closure_set1_rules.py"),
    ([sys.executable, str(ROOT / "verification/bounded/design_closure_set2_rules.py")], "verification/bounded/design_closure_set2_rules.py"),
    ([sys.executable, str(ROOT / "verification/prototypes/v003-sessions/k12_smoke.py")], "verification/prototypes/v003-sessions/k12_smoke.py"),
    ([sys.executable, str(ROOT / "verification/prototypes/v003-sessions/sealed_delivery_state.py"),
      str(ROOT / "verification/results/v003-sealed-delivery-latest.json")],
     "verification/prototypes/v003-sessions/sealed_delivery_state.py"),
    ([sys.executable, str(ROOT / "verification/bounded/protocol_semantics_adr0158.py")], "verification/bounded/protocol_semantics_adr0158.py"),
    ([sys.executable, str(ROOT / "verification/bounded/ui_integration_adr0159.py")], "verification/bounded/ui_integration_adr0159.py"),
    ([sys.executable, str(ROOT / "verification/bounded/dangerous_algorithms_adr0161.py")], "verification/bounded/dangerous_algorithms_adr0161.py"),
    ([sys.executable, str(ROOT / "verification/bounded/protocol_evolution_adr0162.py")], "verification/bounded/protocol_evolution_adr0162.py"),
    # Keep the Rust-dependent aggregate last so all cheap deterministic evidence is refreshed first.
    ([sys.executable, str(ROOT / "verification/bounded/cross_wave_feasibility.py")], "verification/bounded/cross_wave_feasibility.py"),
]



def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--profile", choices=("full", "drop"), default="full",
                    help="drop skips the Rust-dependent aggregate so a fresh exported checkpoint can be checked portably")
    ap.add_argument("--start", type=int, default=1, help="1-based first producer to run")
    ap.add_argument("--end", type=int, default=0, help="1-based last producer to run; 0 means through the selected profile end")
    a = ap.parse_args()
    runs = RUNS if a.profile == "full" else RUNS[:-1]
    if a.start < 1 or a.start > len(runs):
        ap.error("--start is outside the selected producer range")
    end = len(runs) if a.end == 0 else a.end
    if end < a.start or end > len(runs):
        ap.error("--end is outside the selected producer range")
    runs = runs[a.start-1:end]
    summary_path = ROOT / ("verification/results/phase1-summary.json" if a.profile == "full" else "verification/results/phase1-drop-summary.json")
    rows=[]
    for idx, (argv, label) in enumerate(runs, a.start):
        print(f"[{idx:02d}/{len(RUNS if a.profile == 'full' else RUNS[:-1]):02d}] {label} ...", flush=True)
        started = time.monotonic()
        cp=subprocess.run(argv, cwd=ROOT, text=True, capture_output=True)
        elapsed = round(time.monotonic() - started, 3)
        rows.append({"script": label, "returncode": cp.returncode})
        print(f"[{idx:02d}/{len(runs):02d}] {label}: {'PASS' if cp.returncode == 0 else 'FAIL'} ({elapsed:.3f}s)", flush=True)
        if cp.returncode:
            sys.stderr.write(cp.stdout); sys.stderr.write(cp.stderr)
            payload={"phase1_passed":False,"profile":a.profile,"runs":rows}
            summary_path.write_text(json.dumps(payload,indent=2)+'\n')
            print(json.dumps(payload, indent=2))
            return cp.returncode
    evidence = {
        "authority": json.loads((ROOT/'verification/results/authority-bounded-latest.json').read_text()),
        "conversation": json.loads((ROOT/'verification/results/conversation-convergence-latest.json').read_text()),
        "wire": json.loads((ROOT/'verification/results/wire-boundary-bounded-latest.json').read_text()),
        "cbor": json.loads((ROOT/'verification/results/cbor-cross-impl-latest.json').read_text()),
        "continuity": json.loads((ROOT/'verification/results/continuity-dry-run-latest.json').read_text()),
        "assessment": json.loads((ROOT/'verification/results/continuity-assessment-latest.json').read_text()),
        "home_tiers": json.loads((ROOT/'verification/results/home-tiers-latest.json').read_text()),
    }
    payload={
        "phase1_passed":True,
        "profile":a.profile,
        "gate_status_effect":"evidence_only_no_gate_automatically_passed",
        "bounded_cases": evidence['authority']['total_cases'] + evidence['conversation']['cases'] + evidence['wire']['total_cases'] + evidence['assessment']['cases'] + evidence['home_tiers']['sponsorship']['cases'] + evidence['home_tiers']['migration']['cases'],
        "vendor_block_surviving_fraction": {"no_fallback": evidence['home_tiers']['vendor_block']['mean_surviving_fraction_no_fallback'], "with_second_family_fallback": evidence['home_tiers']['vendor_block']['mean_surviving_fraction_with_fallback']},
        "continuity_assessment_cases": evidence['assessment']['cases'],
        "cross_impl_cbor_vectors": evidence['cbor']['vectors_compared'],
        "offline_continuity_artifacts": evidence['continuity']['artifact_count'],
        "gates_with_new_evidence": ["V-002","V-003","V-004","V-005","V-008","V-009","V-011","V-013","V-014","V-015","V-016","V-017","V-020","V-021","V-023"],
        "runs":rows,
    }
    summary_path.write_text(json.dumps(payload,indent=2)+'\n')
    print(json.dumps(payload, indent=2))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
