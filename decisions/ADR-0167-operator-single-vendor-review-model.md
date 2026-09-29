# ADR-0167 — Operator-directed single-vendor review model

- **Status:** Accepted
- **Decided by:** operator, 2026-09-29 ("you do all the reviews too. no more OpenAI or other reviewers ... do everything
  you need to do so we can proceed with engineering")
- **Date:** 2026-09-29
- **Amends:** ADR-0117 (AI-agent review and gate independence), ADR-0134 review batching
- **Does not amend:** gate criteria themselves, `independent_tool` / `independent_implementation` requirements, or any
  pre-release / `external_review` obligation

## Context

ADR-0117 let a different-vendor AI agent satisfy `independent_reviewer` for pre-engineering phases and rejected
same-model reviews in advance, because same-model reviews share blind spots. On 2026-09-29 the operator ended access to
OpenAI agents and directed that Claude (Anthropic) performs all remaining reviews. Every open pre-engineering blocker for
`core_kernel` is review-only, so under ADR-0117 no path to engineering remained.

## Decision

1. For **pre-engineering** phases, an `independent_reviewer` requirement, including a criterion worded "reviewed by
   someone other than its author", is satisfied by a **Claude review pass** that meets all of the following:
   - it works from a written brief against the current tree, not from the author's summary;
   - it re-executes the evidence it relies on in a tool-complete worktree (`make verify` and the focused toolchains)
     rather than trusting committed results;
   - it looks for counterexamples (adversarial scenarios, mutation of the model or table) and records each finding with
     its resolution;
   - it is recorded under `verification/reviews/` with the model id, scope, commands run, findings and per-gate
     disposition; and
   - a finding that changes normative semantics goes through an ADR and its own re-verification before the gate passes.
2. A gate passed this way records `"pre_engineering_review_basis": "operator_single_vendor_ADR-0167"` in the gate
   ledger. The label is honest: it is **not** vendor-independent review.
3. `independent_tool` (for example TLC) and `independent_implementation` (a second codec in a different language and
   runtime that agrees on the corpus) keep their meaning; they are satisfied by the tool or implementation, not by who
   reviews it.
4. Pre-release phases and `external_review` are unchanged: they still require humans. The operator will revisit them
   before any public release.
5. Evidence that only people can produce, such as participant studies, cannot be manufactured by an AI review. Such
   criteria are either kept or moved to a later phase by an explicit ADR that says so.

## Consequences

- The operator accepts the risk ADR-0117 named: a same-family model may share the author's blind spots. The mitigations
  are re-execution, adversarial checking and the ledger label, which keeps every such passage findable for later human or
  cross-vendor re-review.
- ADR-0117 Option 3 ("any other agent session counts") stays rejected. A passage needs the full review pass above, not a
  second session's opinion.
- Pre-engineering gates can now pass. Scope authorization still needs its own ADR under the normal unlock rules.
