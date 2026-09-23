#!/usr/bin/env python3
"""Render verification/GATE_PASS_CRITERIA.md from machine/architecture_verification_gates.jsonl.

  python3 tools/render_gates.py          # rewrite the file
  python3 tools/render_gates.py --check  # exit 1 if the file is stale (used by the validator)

The machine ledger is authoritative; never hand-edit the rendered file.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "machine/architecture_verification_gates.jsonl"
OUT = ROOT / "verification/GATE_PASS_CRITERIA.md"

HEADER = """# Verification Gate Pass Criteria

- **Status:** Canonical verification-process view, **generated** by `tools/render_gates.py` from
  `machine/architecture_verification_gates.jsonl` (the machine ledger is authoritative; do not hand-edit)
- **Decisions:** ADR-0108, ADR-0109, ADR-0113, ADR-0115, ADR-0117 (proposed)

## Rules

1. **Evidence is not passage.** A gate phase passes only when every listed criterion is met and recorded with evidence links.
2. **Independence.** Evidence produced solely by the author/agent who designed the thing under test cannot pass a phase. Each phase names the minimum independence:
   - `independent_tool`: a checker not written by the design author (TLC, fuzzers, sanitizers);
   - `independent_reviewer`: a named reviewer other than the author signs off in an ADR or review record (whether a different-vendor AI agent may count is ADR-0117, proposed);
   - `independent_implementation`: a second implementation/model written by a different author or in a different language agrees;
   - `upstream_kat`: official known-answer tests from the standard's publisher;
   - `external_review`: an external security/crypto review organization or qualified outside reviewer;
   - `participant_study` / `independent_facilitator`: moderated sessions with real target users, facilitated by someone other than the flow's designer.
3. **Two phases.** `pre_engineering_status` gates scope authorization; overall `status` = passed requires both phases and gates public releases/claims.
4. **Cross-wave feasibility (ADR-0113).** A gate with `interface_impact` must reach `feasibility_status: evidenced` before the listed scopes can be authorized, even though its own scope is a later wave.
5. **Waivers** (`waived_by_adr`) must name the ADR, the risk accepted and the compensating control.

## Summary

| Gate | Name | Scopes | Pre-engineering | Overall | Feasibility | W0 interface impact |
|---|---|---|---|---|---|---|
"""


def render() -> str:
    gates = [json.loads(l) for l in LEDGER.read_text(encoding="utf-8").splitlines() if l.strip()]
    out = [HEADER]
    for g in gates:
        out.append(f"| {g['id']} | `{g['gate']}` | {', '.join(g.get('unlock_scopes') or ['none'])} | "
                   f"{g.get('pre_engineering_status')} | {g.get('status')} | {g.get('feasibility_status')} | "
                   f"{', '.join(g.get('interface_impact') or []) or '—'} |\n")
    out.append("\n## Gates\n")
    for g in gates:
        ind = g.get("independence", {})
        out.append(f"\n### {g['id']} — `{g['gate']}`\n\n")
        out.append(f"{g.get('requirement', '')}\n\n")
        out.append(f"- **Scopes blocked:** {', '.join(g.get('unlock_scopes') or []) or 'none (always required pre-engineering; blocks public production claims)'}\n")
        out.append(f"- **Status:** pre-engineering `{g.get('pre_engineering_status')}`, overall `{g.get('status')}`, feasibility `{g.get('feasibility_status')}`\n")
        out.append(f"- **Independence:** pre-engineering {', '.join(ind.get('pre_engineering', []))}; pre-release {', '.join(ind.get('pre_release', []))}\n")
        if g.get("evidence"):
            out.append("- **Evidence:** " + "; ".join(f"`{e}`" for e in g["evidence"]) + "\n")
        met = set(g.get("criteria_met", []))
        out.append("\nPre-engineering criteria:\n\n")
        for c in g.get("pre_engineering_criteria", []):
            out.append(f"- [{'x' if c in met else ' '}] {c}\n")
        out.append("\nPre-release criteria:\n\n")
        for c in g.get("pre_release_criteria", []):
            out.append(f"- [{'x' if c in met else ' '}] {c}\n")
        if g.get("remaining"):
            out.append("\nWhat remains (needs someone other than the author):\n\n")
            for r in g["remaining"]:
                out.append(f"- {r}\n")
    return "".join(out)


if __name__ == "__main__":
    text = render()
    if "--check" in sys.argv:
        stale = not OUT.exists() or OUT.read_text(encoding="utf-8") != text
        if stale:
            print("verification/GATE_PASS_CRITERIA.md is stale: run python3 tools/render_gates.py", file=sys.stderr)
        raise SystemExit(1 if stale else 0)
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")
