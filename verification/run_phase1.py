#!/usr/bin/env python3
"""Run checkpoint-local phase-1 architecture verification evidence."""
from __future__ import annotations
import json, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = [
    ROOT / "verification/bounded/authority_models.py",
    ROOT / "verification/bounded/conversation_convergence.py",
    ROOT / "verification/bounded/wire_boundary_models.py",
    ROOT / "verification/cbor/run_cross_impl.py",
    ROOT / "verification/bounded/continuity_dry_run.py",
    ROOT / "verification/bounded/continuity_assessment.py",
]


def main() -> int:
    rows=[]
    for script in SCRIPTS:
        cp=subprocess.run([sys.executable, str(script)], cwd=ROOT, text=True, capture_output=True)
        rows.append({"script": str(script.relative_to(ROOT)), "returncode": cp.returncode})
        if cp.returncode:
            sys.stderr.write(cp.stdout); sys.stderr.write(cp.stderr)
            payload={"phase1_passed":False,"runs":rows}
            (ROOT/'verification/results/phase1-summary.json').write_text(json.dumps(payload,indent=2)+'\n')
            print(json.dumps(payload, indent=2))
            return cp.returncode
    evidence = {
        "authority": json.loads((ROOT/'verification/results/authority-bounded-latest.json').read_text()),
        "conversation": json.loads((ROOT/'verification/results/conversation-convergence-latest.json').read_text()),
        "wire": json.loads((ROOT/'verification/results/wire-boundary-bounded-latest.json').read_text()),
        "cbor": json.loads((ROOT/'verification/results/cbor-cross-impl-latest.json').read_text()),
        "continuity": json.loads((ROOT/'verification/results/continuity-dry-run-latest.json').read_text()),
        "assessment": json.loads((ROOT/'verification/results/continuity-assessment-latest.json').read_text()),
    }
    payload={
        "phase1_passed":True,
        "gate_status_effect":"evidence_only_no_gate_automatically_passed",
        "bounded_cases": evidence['authority']['total_cases'] + evidence['conversation']['cases'] + evidence['wire']['total_cases'] + evidence['assessment']['cases'],
        "continuity_assessment_cases": evidence['assessment']['cases'],
        "cross_impl_cbor_vectors": evidence['cbor']['vectors_compared'],
        "offline_continuity_artifacts": evidence['continuity']['artifact_count'],
        "gates_with_new_evidence": ["V-002","V-004","V-005","V-008","V-009","V-011","V-013","V-014","V-015","V-016","V-017"],
        "runs":rows,
    }
    (ROOT/'verification/results/phase1-summary.json').write_text(json.dumps(payload,indent=2)+'\n')
    print(json.dumps(payload, indent=2))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
