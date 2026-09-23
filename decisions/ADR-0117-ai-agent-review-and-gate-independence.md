# ADR-0117 — Can a different AI agent count as an independent reviewer?

- **Status:** Proposed. **Awaiting operator decision.** Until accepted, agent reviews are recorded as
  `agent_review` evidence and do not satisfy `independent_reviewer`.
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
