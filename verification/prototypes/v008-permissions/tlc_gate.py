#!/usr/bin/env python3
"""Run the TLC configs of one prototype slug and write verification/results/<slug>-latest.json.

Usage: tlc_gate.py <slug> <criteria.json>
  - reads verification/tla/models-<slug>.json (same format as models.json);
  - runs each entry through tools/run_tlc.run() (pinned TLC; counterexamples -> verification/vectors/tla-<cfg>.json);
  - merges the deterministic TLC outcomes with the slug's criteria file and exits 1 on any unexpected outcome.
Feasibility tooling written by a Claude agent. Not product code.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
import run_tlc  # noqa: E402  (repository TLC runner: pinned jar, trace parsing, vector writing)


def main() -> int:
    slug, crit_path = sys.argv[1], Path(sys.argv[2])
    manifest = sys.argv[3] if len(sys.argv) > 3 else f"models-{slug}.json"
    manifest = json.loads((ROOT / "verification/tla" / manifest).read_text())
    runs = []
    for e in manifest["models"]:
        r = run_tlc.run(e)
        runs.append(r)
        print(f"{'ok ' if r['as_expected'] else 'BAD'} {r['config']:<52} {r['outcome']:<9} {r['violated'] or r['distinct_states']}",
              flush=True)
    crit = json.loads(crit_path.read_text())
    ok = all(r["as_expected"] for r in runs)
    out = {
        "gates": crit["gates"],
        "criteria": crit["criteria"],
        "independence": crit["independence"],
        "tool_versions": {"tlc": "tla2tools 1.7.4 (sha1 bee4a54f3ee3d4afc347c3240ec2d9e93b075104)", "java": "OpenJDK 21"},
        "tlc_runs": runs,
        "summary": {
            "all_tlc_outcomes_as_expected": ok,
            "configs": len(runs),
            "pass_configs_distinct_states": sum(r["distinct_states"] or 0 for r in runs if r["expect"] == "pass"),
            "vectors_written": sorted(f"verification/vectors/tla-{r['config'][:-4]}.json" for r in runs if r["outcome"] == "violation"),
            "gate_status_effect": "evidence_only_no_gate_automatically_passed",
        },
    }
    (ROOT / f"verification/results/{slug}-latest.json").write_text(json.dumps(out, indent=1) + "\n")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
