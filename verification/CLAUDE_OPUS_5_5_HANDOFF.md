# Claude Opus 5.5 — Verification Executor Handoff

- **Role for this pass:** `verification_executor` under ADR-0134
- **Starting state:** architecture design has zero blocking items; product engineering remains unauthorized
- **Mission:** finish repo-local V-* pre-engineering evidence and cross-wave feasibility evidence. Do not redesign OM unless evidence proves a contradiction.

## Non-negotiable workflow rule

**Do not bounce this repository back to OpenAI or another reviewer one gate at a time.**

For every gate, first complete all model/vector/prototype/harness/documentation work that the repository and your environment can complete. If a criterion ultimately needs an independent reviewer, record it as review-pending and continue. At the end, produce one consolidated review queue containing only artifacts whose remaining blocker is genuinely independent review.

If a frozen architecture rule makes a criterion impossible or contradictory, stop only that criterion, capture the counterexample/evidence, and continue unrelated gates. Reopen architecture only with a concrete contradiction trace, not an implementation preference.

## What is allowed before engineering unlock

Allowed now:

- TLA+/bounded/property models;
- deterministic vectors and clean-room/different-language reference implementations;
- fuzz/conformance harnesses;
- isolated non-production feasibility prototypes under `verification/`;
- browser/runtime/platform feasibility experiments required by a gate;
- standards/KAT validation;
- documentation and machine-ledger updates that report evidence.

Not allowed now:

- product implementation under future `om-core`, `om-ui`, provider or client production packages;
- silently promoting a verification prototype into production code;
- changing protocol/authority semantics merely to make a prototype easier;
- marking human/external evidence as complete without the required humans/external reviewer.

## First target: make `core_kernel` eligible for an unlock ADR

Before `core_kernel` can be authorized, finish pre-engineering passage for:

- V-001 production crypto path
- V-002 AccountControlState formal model
- V-003 one-to-one session model
- V-005 canonical encoding vectors
- V-015 metadata/privacy path model
- V-016 decentralized abuse/moderation authority model
- V-017 continuity drill (always-required gate)

In addition, obtain **cross-wave feasibility only** (not full gate passage) for:

- V-004 groups
- V-008 Room/Space authority
- V-009 event materialization
- V-011 service principals
- V-013 bridges
- V-014 archive portability
- V-019 PQ group ReInit

The exact cross-wave target is each gate's `feasibility_criteria`. Do not complete all later-wave pre-engineering criteria merely to unlock W0 unless they are useful anyway.

## Independence: no ping-pong

- If a gate requires only `independent_tool`, run the named independent tool and capture its version/result.
- If it requires `independent_implementation`, ADR-0134 permits a cleanly separated different-language/runtime implementation with no shared implementation code; capture differential results. Do not request cross-vendor review unless the gate also says `independent_reviewer`.
- If it requires `independent_reviewer` and the relevant semantic artifact was authored by Anthropic, finish every other criterion first. Add the artifact and exact question to one final review queue.
- If an existing non-Anthropic review already covers the unchanged semantic artifact, cite it. Adding test configs/vectors without changing semantics does not invalidate it.
- Pre-release `external_review` and V-018 human studies are not your blockers for core engineering and must not be simulated.

## Gate ledger discipline

[`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl) is authoritative.

For a criterion to count:

1. evidence file/result exists;
2. the criterion's exact canonical string is placed in `criteria_met`;
3. required independence for the phase is satisfied and recorded;
4. `pre_engineering_status` becomes `passed` only when every pre-engineering criterion and the phase's independence are satisfied;
5. `feasibility_status` becomes `evidenced` only when every `feasibility_criteria` item is in `feasibility_criteria_met` with evidence.

Do not maintain a separate hand-written "remaining" checklist. The generated gate view derives unfinished criteria from the machine ledger.

## Current design decisions that MUST be exercised, not reopened casually

The verification pass must include the final hardening decisions already frozen by ADR-0131..0133, especially:

- dual Ed25519 + ML-DSA-65 PQXDH binding;
- exact durable last-resort replay ledger with fail-closed 4096 cap;
- AES-256-GCM-SIV OM-DR derivation;
- no v1 Double Ratchet header-encryption claim;
- verifier-local delayed recovery against withheld RecoveryRequest;
- immutable-incarnation targeting for rebasable reductions;
- symmetric IntroIdentityBinding before relationship `ACTIVE`;
- pinned draft-ietf-hpke-pq-05 MessageRequest HPKE profile;
- remote mini-app plaintext crossing is egress;
- bilateral signed 1:1 continuous-plaintext-egress consent;
- group-call media keys isolated to joined participant DeviceIDs;
- frozen Home API/error/provider-capability contracts and release-transparency roles.

If executable evidence shows one of these rules is internally inconsistent, capture the smallest failing trace and identify the exact normative clauses in conflict. Otherwise implement the verification evidence as written.

## Later-scope work

After the core unlock evidence is complete, continue gates whose environments are available. V-006/V-007/V-010/V-012/V-021/V-022/V-023 may need browser/runtime/platform work. V-018 requires real participants and an independent facilitator and therefore remains external until that study is organized.

Do not let V-018 or a pre-release external-review requirement hold up completion of unrelated repo-local evidence.

## Required final report from this pass

Return one report containing:

1. gate table: pre-engineering status and feasibility status;
2. evidence added per criterion;
3. counterexamples/design contradictions, if any;
4. one consolidated `independent_reviewer` queue after all executable work is exhausted;
5. external/human-only items separately identified;
6. whether `core_kernel` now meets the machine prerequisites for an explicit unlock ADR.

Do **not** self-authorize engineering. If all prerequisites are met, report that `core_kernel` is *eligible* for the operator/integrator to accept an unlock ADR.
