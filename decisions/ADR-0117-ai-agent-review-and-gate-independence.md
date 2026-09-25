# ADR-0117 — Can a different AI agent count as an independent reviewer?

- **Status:** Accepted (Option 2)
- **Decided by:** operator, 2026-09-24 ("i lean to agree ... check it and accept it if good")
- **Date:** 2026-09-24

## Context

ADR-0109 says evidence produced solely by the design's author cannot pass a gate. The design and most models were
written by AI agents (a ChatGPT design agent and Claude agents) directed by one operator. Many pre-engineering criteria
ask for "reviewed by someone other than its author". No human reviewer is available yet (see ADR-0114).

## Options

1. **Humans only.** Every `independent_reviewer` criterion waits for a human reviewer. This is the most defensible
   choice, and it is slow.
2. **Cross-vendor agent review for pre-engineering only.** An agent from a *different vendor or model family* than
   the artifact's author may satisfy `independent_reviewer` for **pre-engineering** phases. Each review is recorded in
   `verification/reviews/` with the model id, the prompt, the findings and how each finding was resolved. Pre-release
   phases and `external_review` always need humans.
3. **Any other agent session counts.** Rejected in advance: same-model reviews share blind spots.

## Recommendation

Option 2. It keeps pre-engineering moving while a human review community forms. It is honest because the review
record names the model, and it never lets an AI review substitute for pre-release or external review.

## Decision

Option 2 is accepted. A review by an AI agent from a **different vendor or model family** than the artifact's author
satisfies `independent_reviewer` for **pre-engineering** phases only. It is recorded in `verification/reviews/` with the
model id, the prompt, the findings and how each finding was resolved. Pre-release phases and `external_review` always
need humans.

Consequences for current evidence:

- The design documents were written by a ChatGPT agent. A Claude or Gemini agent may review them.
- The V-001..V-016 prototypes and models were written by Claude agents. They need a non-Anthropic reviewer, such as an
  OpenAI Codex or Google Gemini agent. A different Claude session does not count (Option 3 stays rejected).
- A reviewer may not have authored any part of the artifact it reviews, including its vectors.

## 2026-09-25 execution amendment — reviews are batched, not bounced

ADR-0134 changes workflow, not the independence threshold. The verification executor must finish every non-review criterion it can complete before asking for an `independent_reviewer`. Review-only blockers are consolidated into one final queue. A reviewer finding that changes the semantics of a reviewed artifact requires re-review of that changed semantic artifact; adding configs/vectors that do not alter its semantics does not automatically invalidate the review.

`independent_implementation` is distinct from `independent_reviewer`: a different-language/runtime clean-room implementation can satisfy implementation diversity under ADR-0109/0134 without cross-vendor review unless the gate separately requires a reviewer.
