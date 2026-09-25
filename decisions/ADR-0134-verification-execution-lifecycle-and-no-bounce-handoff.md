# ADR-0134 — Verification execution lifecycle and no-bounce handoff

- **Status:** Accepted
- **Date:** 2026-09-25
- **Decided by:** integrator under operator direction to harden design, avoid architecture/verification ping-pong, and hand repo-local execution to one verification executor
- **Amends:** ADR-0108, ADR-0109, ADR-0113, ADR-0117

## Context

OM now has zero blocking architecture-design questions, but the V-* ledger mixed four different kinds of evidence:

1. design/specification closure;
2. repo-local pre-engineering evidence (models, vectors, fuzz/property work and isolated feasibility prototypes);
3. evidence that can exist only after an engineering scope has been authorized and implemented; and
4. genuinely external evidence such as human participant studies or outside security review.

That mixture created two risks. First, a gate could accidentally require implementation evidence before authorizing the implementation scope, producing a chicken-and-egg lock. Second, repeated requests for a different AI reviewer could cause agents to bounce the repository between vendors without finishing the executable work.

## Decision

### 1. Four evidence phases are explicit

Every gate is interpreted using these evidence classes:

- **design-complete prerequisite** — normative architecture, registries, CDDL, authority rules and acceptance semantics. If this is missing, reopen architecture rather than hiding the choice in a prototype.
- **pre-engineering repo/local feasibility evidence** — formal/bounded models, vectors, clean-room/different-language codecs, fuzz/property harnesses, packet/metadata trace plans and isolated runtime feasibility prototypes under `verification/`. These may run before product engineering is authorized.
- **post-scope implementation evidence** — integration CI, production-build exclusion checks, real implementation fuzzing, multi-client integration, real-device performance and release implementation conformance. These belong in `pre_release_criteria` and MUST NOT block authorization of the very scope needed to create them.
- **external evidence** — participant studies, qualified outside security/crypto review, independent rebuilds or other evidence that cannot honestly be manufactured by the current repo executor. These remain explicit and are never simulated by an agent.

If a pre-engineering criterion is discovered to require product code from the blocked scope rather than an isolated verification prototype, move it to pre-release by ADR; do not silently implement product code to satisfy it.

### 2. Verification prototypes are allowed but are not product engineering

A blocked scope may have an isolated feasibility prototype under `verification/` when the gate explicitly calls for it. Such a prototype:

- exists only to answer the named gate question;
- MUST NOT establish new normative protocol semantics outside the frozen design;
- MUST be unmistakably non-production;
- does not authorize the blocked engineering scope;
- may be reused after authorization only through normal engineering review, never by silently treating prototype success as production readiness.

This is the intended meaning of V-001, V-006, V-007, V-010, V-012 and V-019 pre-engineering prototype criteria.

### 3. Cross-wave feasibility gets its own objective criteria

For every gate whose `interface_impact` names an earlier scope, `feasibility_criteria` is authoritative for `feasibility_status`. Full passage of the later-wave gate is not required.

`feasibility_status=evidenced` means all listed feasibility criteria are met and evidence is recorded showing that the already-frozen earlier-wave interfaces can carry the later feature without reinterpretation or a new signed-object primitive. It does **not** mean the later feature is implemented, interoperable or release-ready.

This removes the ambiguity that could otherwise force W0 to complete W1-W4 verification before `core_kernel` starts.

### 4. One executor finishes repo-local work before review is requested

The current handoff role is `verification_executor`. The executor MUST:

1. complete every non-review criterion it can satisfy in the repository/environment;
2. record objective evidence and update the machine ledger;
3. continue to the next gate when an `independent_reviewer` sign-off is still pending;
4. batch all remaining reviewer-only items at the end instead of returning the repository mid-run;
5. reopen architecture only when executable evidence demonstrates a contradiction in a frozen rule, not merely because implementation is inconvenient.

A reviewer finding that requires semantic changes invalidates the review only for the changed semantic artifact. Adding configs, vectors or harness cases that do not change the reviewed semantics does not automatically invalidate a prior design review.

### 5. Independence vocabulary is artifact-specific

The existing independence levels remain, with these clarifications:

- `independent_tool` means an independently maintained checker/runtime such as TLC, a fuzzer engine or sanitizer; the executor may invoke it.
- `independent_reviewer` remains a non-author review. Under ADR-0117, a different-vendor/model-family AI may satisfy this **only for pre-engineering**. It is requested after repo-local work is complete unless the review is needed to resolve a semantic ambiguity.
- `independent_implementation` is implementation diversity, not necessarily organizational diversity. For pre-engineering interop it may be satisfied by a cleanly separated second implementation in a different language/runtime with no shared codec/materializer implementation code and differential results captured. A gate that also needs human or cross-vendor judgment must separately name `independent_reviewer`.
- `external_review`, `participant_study` and `independent_facilitator` retain their ordinary human/external meaning and can never be self-satisfied by the executor.
- `upstream_kat` remains official standard-publisher known-answer evidence and needs no AI reviewer merely because the local harness was written by one agent.

### 6. Machine ledger is the checklist; free-text "remaining" lists are removed

`pre_engineering_criteria` and `pre_release_criteria` are the only canonical criterion lists. `criteria_met` MUST contain exact strings from those lists. The human-readable gate document derives unfinished work mechanically.

For interface-impact gates, `feasibility_criteria` / `feasibility_criteria_met` do the same for cross-wave feasibility.

No hidden free-text `remaining` list may carry a requirement that is absent from the canonical criteria.

### 7. Current executor handoff is one-way

[`verification/CLAUDE_OPUS_5_5_HANDOFF.md`](../verification/CLAUDE_OPUS_5_5_HANDOFF.md) binds the current `verification_executor` role to Claude Opus 5.5 for this pass. That operational document may change without changing protocol architecture. Claude is to exhaust repo-local verification work and return a consolidated evidence/review queue, not bounce individual items back to the design agent.

## Consequences

- The architecture remains frozen with `engineering_authorized=false` until the scoped unlock contract is satisfied.
- Core-kernel unlock still requires V-001, V-002, V-003, V-005, V-015, V-016 and always-required V-017 pre-engineering passage, plus `feasibility_status=evidenced` for V-004, V-008, V-009, V-011, V-013, V-014 and V-019.
- Later-wave human/platform work does not block `core_kernel` unless it has explicit `interface_impact`.
- V-018 remains honestly human: no AI-generated surrogate study can pass it.
- Review requests are consolidated after execution rather than becoming a workflow loop.
