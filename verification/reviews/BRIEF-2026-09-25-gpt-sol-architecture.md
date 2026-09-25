# Review brief: Claude-authored architecture changes (for gpt-sol)

You are an independent reviewer under ADR-0117. The original design documents were written by a ChatGPT agent;
**your independent scope is the material Claude agents wrote**, because a different vendor must review it. Read
[`AGENTS.md`](../../AGENTS.md), [`SNAPSHOT.md`](../../SNAPSHOT.md), [`docs/checkpoints/CHECKPOINT_20260925_0010.md`](../../docs/checkpoints/CHECKPOINT_20260925_0010.md), ADR-0109 and ADR-0117 first.

## In scope (Claude-authored)

1. **Decisions ADR-0110 .. ADR-0130** and their ledger records (D-132 onward).
2. **Doc sections headed "2026-09-24 ..."** and the in-place edits made while applying the architecture review
   (see `git log` from `b33c518` onward, and each finding's `Resolution:` line in
   `verification/reviews/2026-09-24-claude-opus-architecture-r1..r4.md`).
3. **Frozen W0 spec:** `spec/cddl/`, `spec/registries/` (ADR-0119, ADR-0126), [`spec/registries/om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json)
   (ADR-0128).
4. **The review itself:** were the R1–R4 reviews right, and were their fixes applied correctly and completely? Did a
   fix introduce a new contradiction, weaken a security property, or give authority to someone ("Where did authority
   go?")?

Findings about the ChatGPT-authored text are welcome, but mark them `same-vendor` (they don't count as independent).
The V-series prototypes are reviewed separately by Codex; skip them unless a spec change broke one.

## What to look for (priority order)

Security flaws and authority leaks; contradictions between ADRs, docs, ledgers and the frozen spec; rulings that
are wrong or underspecified enough that two honest implementations would diverge; violations of the AGENTS.md
absolute rules and the constitution. Skip style. Prefer fewer solid findings.

## How to return results

- Add **only** new files under `verification/reviews/`: `2026-09-25-gpt-sol-architecture.md` (one file, or one per
  area if large). Don't edit other files; put proposed text changes inside the findings.
- Header: reviewer (gpt-sol), exact model id, date, the base commit from `OM_EXPORT.json`, scope.
- A verdict of two or three sentences, then findings: `### F-GS-<n> — <title>`, `Severity:` blocker|major|minor,
  `Where:` file:line or §, `Problem:`, `Proposed fix:`, and an empty `Resolution:` line for the integrator.
- Return the whole repository as a zip with the same layout (leave `OM_EXPORT.json` unchanged).
