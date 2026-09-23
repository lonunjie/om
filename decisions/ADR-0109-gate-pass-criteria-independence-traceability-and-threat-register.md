# ADR-0109 — Gate pass criteria, evidence independence, requirement traceability and a threat register

- **Status:** Accepted
- **Date:** 2026-09-24
- **Source:** [`HARDENING_REVIEW_20260924.md`](../docs/reviews/HARDENING_REVIEW_20260924.md)

## Decision

Every verification gate declares pre-engineering criteria, pre-release criteria, an independence level and unlock scopes. Evidence produced solely by the author/agent that designed the thing under test cannot pass a gate. A second, independent author, implementation, tool or reviewer is required at the level the gate names (R-175). Every requirement is traced in [`machine/requirement_traceability.json`](../machine/requirement_traceability.json) to a gate, release gate, validator check, conformance obligation or named review (R-177). The threat model gains a structured register, [`machine/threat_register.jsonl`](../machine/threat_register.jsonl), whose references are validator-checked (R-178). Gates V-018 (security-UX comprehension), V-019 (PQ group ReInit), V-020 (home onboarding and provider portability) and V-021 (endpoint coercion and platform leakage) are added. The validator enforces these structures.
