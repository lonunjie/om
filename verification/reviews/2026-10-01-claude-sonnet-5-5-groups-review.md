# Groups verification review (V-004, V-009, V-019) — Claude Sonnet 5.5, 2026-10-01

- **Reviewer:** Claude Sonnet 5.5 (`claude-sonnet-5-5`), ADR-0167 operator review model (not vendor-independent).
- **Disclosure:** the same agent session built the evidence and wrote this review (the V-019 prototype was built by a subagent of the same session and re-run by the
  reviewer). A fresh-session or cross-vendor re-check of the three gates is recommended before any public release; the operator may want it before accepting ADR-0177.
- **Tree:** om `main` after `31e35af` plus the commit that carries this file.
- **Scope:** pre-engineering criteria of V-009 (all 5), V-004 (criteria 6 and 7, completing 8/8) and V-019 (criterion 0). These three gates unlock `groups`.
- **Brief:** work from the written gate criteria and ADR-0167 §1, re-execute rather than trust committed results, look for counterexamples by mutating the
  evidence, record each finding with its resolution.

## Re-executed (commands and outcome)

| Command | Outcome |
|---|---|
| `verification/prototypes/v009-materializer/run.sh` | PASS: 266 cases agree, 188,531 Rust and 124,800 reference delivery orders converge, 266 authority tables agree, 47 expectations and 3 forwarding vectors hold |
| `verification/prototypes/v004-group-wire/run.sh` (all steps, both CDDL tools) | PASS: 383 object vectors agree on the Rust and Node strict codecs; the frozen W0 corpus (758 vectors) is unchanged on both; 105 semantic cases agree across the Python and Node checkers; Rust `cddl` 0.10.7 and Ruby `cddl` 0.12.14 agree on 456 checks and no generated example is rejected for a CDDL-expressible reason; registries changed by addition only |
| `verification/prototypes/v019-hybrid-pq-migration/run.sh` | PASS: 20 vectors; native and wasm32-unknown-unknown outputs are byte-identical |
| `make verify` | preflight, all 51 evidence scripts and every TLC config pass (GroupCommit.cfg 676,654 states pass; its five mutation configs each give their counterexample); its final validator step failed only because this review and ADR-0177 were not yet registered, and after `om_registry sync`, `make check`, `make manifest` and `make registries` (registry immutability against `checkpoint/20260929-0902`) pass |

## Counterexample search (mutations; each must be caught)

| Mutation | Caught by |
|---|---|
| V-009: the Rust order key ignores the epoch; `expires_after` overrides the absolute expiry | 15 expectation failures and 14 comparison disagreements |
| V-004 wire: `PollVote.selected_option_ids` max 32 -> 31; `sequencing_timeout` min 30 -> 1 | both codecs reject a positive vector and accept a negative one (2 mismatches each) |
| V-004 semantic: the Python checker drops the reducing-flag rule; the Node checker drops the `author_removed` gate | 2 and 2 case mismatches |
| V-019: one expectation flipped in the prototype | `run.sh` exits 1 and leaves the previous results file untouched (run by the subagent, the exit code re-checked) |

## Findings

| ID | Severity | Summary | Resolution |
|---|---|---|---|
| F-27 | Major | The W0 `SignedEnvelope.body` limit (64 KiB) binds every signed W1 object; the handoff's caps (2048 members, 4096 items) were unreachable | ADR-0176: 512 members, 1024 items, split long histories; a larger-envelope profile is an open design point |
| F-28 | Minor | The handoff's `transition_binding_hash` in `GroupControlState` is circular | dropped (ADR-0176) |
| F-29 | Major | No text said which intents are reductions, so ADR-0173's "a direct Commit must be a reduction" could not be checked | ADR-0176 §3 classification; T-161 records the state-dependent residual |
| F-30 | Minor | The room-operation registry had no invite redemption or join-policy operation | appended (ADR-0176) |
| F-31 | Minor | ADR-0174 rule 3 said milliseconds and allowed `expires_after = 0`; the wire says seconds and at least 1 | corrected in the same session (`31e35af`), including R-399 |
| F-32 | Minor | The V-009 random2 family found two real bugs: the reference dropped a close or vote ordered before its poll's create (REF-4), and the materializer applied a topic status ordered before the winning create (MAT-1) | both fixed; ADR-0174 rule 4 |
| F-33 | Minor | A first draft inserted two operations in the middle of [`om-room-capabilities.json`](../../spec/registries/om-room-capabilities.json) (not additive), and the CDDL for the embedded `SpaceDelegatedIntent` was too loose (generated examples hit the codec's nested decode) | operations appended; a `group-space-delegated-intent` copy with a drift check against the Space rule |
| G-V19-1..8 | Design | OpenMLS has no ReInit; after migration an epoch window never closes, the old group stays live, no cryptographic old-to-new binding, concurrent migrations diverge, KeyPackage readiness, per-suite leaf keys, equal-generation replacement | ADR-0175 rules 1-5 (R-404..R-406); the migration wire stays open |

## Per-gate disposition

**V-009 `baseline_event_materialization_convergence`: accept all five criteria.** Different language and runtime (Rust vs Python, ADR-0167 §3), not a different
author; both sides now share nothing but the corpus, and every new behavior has hand-written expectations checked on both. The Python reference was extended
by the same agent that wrote the Rust side, which is the principal weakness (below).

**V-004 `group_commit_control_model`: accept criteria 6 and 7.** Criterion 6: frozen CDDL, registered fields and labels, and vectors on which two independently
written codecs agree, for GroupControlState, GroupControlIntent, GroupCommitBinding (the frozen pair that makes the GroupControlTransition union) and the
pin/poll bodies; the W1 overlay is additive and the W0 corpus is untouched. Criterion 7: the bundle binds Room, bundle author, recipient incarnation and
state/epoch (gates), and 19 scenarios show a claimed EventID never reaches the live dedupe set or a reducer. With the TLC model (an independent tool,
ADR-0173) the gate's independence requirement `independent_tool` plus `independent_reviewer` is met under ADR-0167.

**V-019 `pq_group_reinit_migration`: accept criterion 0 as an OM-level authenticated migration.** The literal RFC 9420 ReInit proposal is not implemented by the
library and cannot be shown; ADR-0175 states that "ReInit" in R-174 means the migration. If the operator reads the criterion strictly, V-019 stays open.

## Limits (what this review does not establish)

- One session and one model family wrote the evidence and this review. The codecs, checkers and materializers share no code, but the same author chose the rules.
- Signatures are the V-005 insecure test profile; no signature verification, real MLS Commit bytes or network are in the wire evidence. The bundle admission is a model.
- The V-009 corpus generator and the adapter are shared between the two materializers; the 47 hand-written expectations are the guard.
- V-019 authentication stays Ed25519 (the KEM is hybrid), the suite is a draft code point, OpenMLS and the PQ crates are pre-1.0.
- Not checked: group sizes above the 64 KiB body limit, crash/restart during a migration, a second MLS implementation.

## Result

V-009, V-004 and V-019 pass pre-engineering under the ADR-0167 model. Pre-release items stay open: the V-004 multi-client partition harness on two
implementations, the V-009 corpus run by every conforming implementation, and the V-019 interoperable migration. ADR-0177 proposes the `groups` unlock.
