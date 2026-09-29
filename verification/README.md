# OM Architecture Verification Evidence

This directory contains **pre-engineering architecture-verification artifacts**. It is intentionally separate from product implementation.

The architecture contract remains locked: `engineering_authorized=false`. **Evidence is not gate passage.** A bounded model, research result, or dry run may increase confidence and create regression fixtures, but a gate is marked `passed` only when its complete machine-ledger requirement is satisfied.

## Evidence classes

- `bounded/` — executable bounded reference models/adversarial enumerations using standard Python;
- `tla/` — TLA+ formal models/configurations for authority state machines;
- `cbor/` — independent deterministic-CBOR foundation encoders/fixtures;
- `vectors/` — regression/golden transition scenarios derived from architecture invariants;
- `results/` — machine-readable captured results;
- [`FEASIBILITY_RECONNAISSANCE_20260923.md`](FEASIBILITY_RECONNAISSANCE_20260923.md) — standards/runtime feasibility reconnaissance for gates that require real implementations.

Two verification entry points intentionally have different environment contracts:

```sh
make verify-drop     # fresh exported ZIP; no Git history, Rust, or TLC required
make verify          # imported Git repo; full phase-1 evidence + TLC + registry-history checks
make preflight       # report Rust/TLC/Hypothesis/tool availability without changing evidence
```

`verify-drop` runs the deterministic portable evidence profile and writes [`phase1-drop-summary.json`](results/phase1-drop-summary.json). It does not
claim the Rust-dependent cross-wave aggregate, TLC execution, or registry-history immutability. `verify` retains
those stronger requirements. The continuity manifest's working-tree check is deliberately Git-independent; only
`check-registries`, continuity-bundle publication, and other history comparisons require Git.

For V-003 Hypothesis or the complete V-005 Rust/CDDL/Wasm prototype, use `tools/verify_preflight.py --profile v003`
or `--profile v005` first. The preflight reports when an uncached dependency would require network access rather
than silently fetching it. TLC itself is pinned and may bootstrap its JAR once; [`tools/run_tlc.py`](../tools/run_tlc.py) preflights that
once before entering the model loop.

## 2026-09-28 pre-independent-review state

The current portable path additionally includes ADR-0159 UI integration evidence for the conversation-first Omnibar and hardened theme/visual semantics. The Run-9B portable path includes all ADR-0147 through ADR-0153 portable hardening evidence. For the `core_kernel` batch, V-001 and V-005 have completed their repo-local pre-engineering criteria; V-002 is 9/10 and V-003 is 13/14 with only their explicit independent-review criteria unchecked; V-015 is 16/16 repo-local, V-016 is 9/9 and V-017 is 3/3 with phase independence still open. V-021 is 10/10 repo-local and V-023 is 9/9 repo-local for the reference-client scope, each still requiring its own independent review. Run 9B adds replaceable Android wake evidence plus desktop/PersonRecord endpoint-safety evidence; ADR-0153 clarifies that unconfirmed third-party wake-registration retirement has no invented universal time bound. The final reviewer should not redo completed repo-local work merely because the independent-review fields remain open.

The sections below preserve earlier evidence history and should not be read as the current remaining-work list.

## 2026-09-23 phase-1 evidence

The current runner completed **7,772 bounded/adversarial cases**, **14 cross-implementation deterministic-CBOR foundation vectors**, and an **offline 60-artifact continuity dry run**. No invariant violation was found in those bounded models. No verification gate is automatically passed by this result.

| Gate | Evidence in this checkpoint | What remains before pass |
|---|---|---|
| V-002 | AccountControlState bounded invariants; TLA+ model/config authored; regression vectors | run TLC; expand recovery/fork/rollback state space; preserve TLC counts/counterexamples |
| V-004 | duplicate/reorder/conflict/crash group-control traces | formal crash/partition/equivocation model; MLS-profile integration evidence |
| V-005 | independently written Python + Node.js deterministic-CBOR encoders agree on 14 foundation vectors | freeze CDDL/numeric-field/domain-separation registries for every signed/hashed v1 object; object-specific positive/negative vectors |
| V-008 | 6,156 bounded capability/ceiling/restriction/direct-binding cases within combined authority run | full Room/Space governance transitions, quorum/revocation races and formal model |
| V-009 | 864 permutation/duplication convergence cases for pins/redaction/polls/topics | full EventClock/malformed/authorization-race corpus and independent materializer |
| V-011 | no-self-bootstrap, representation, installation/data-access, reach and automation-hop checks | host migration/revocation races, alias replacement and concrete tool-capability execution |
| V-013 | BridgePrincipal/provenance, scoped origin fingerprints, loop/reflection suppression and fidelity/E2EE-boundary cases | real bridge adapters, multi-bridge integration and trace evidence |
| V-014 | hostile ZIP/import boundary corpus and stale-authority/non-routable-history checks | two independent archive implementations, encrypted corruption/wrong-key tests, real foreign importers |
| V-015 | abstract outer-routing/provider-capability/privacy-claim trace assertions | real packet/provider/browser traces across normal and privacy-enhanced paths |
| V-016 | scoped moderation/advisory/provider/Sybil authority checks | timeout/race/evidence/fork cases plus formal/adversarial resource model |
| V-017 | offline continuity dry run over 60 current protocol/machine/normative artifacts | independent build/conformance, generations/forks, crypto/release-root/provider/capture disaster drills |

## Gates not yet exercised by executable phase-1 evidence

V-001, V-003, V-006, V-007, V-010 and V-012 remain fully open. [`FEASIBILITY_RECONNAISSANCE_20260923.md`](FEASIBILITY_RECONNAISSANCE_20260923.md) narrows the experiments for V-001/V-006/V-007/V-010/V-012 but is not pass evidence. V-003 still needs its dedicated per-device 1:1 session model/fuzzer.

## Known verification-artifact gaps discovered in this checkpoint

1. **TLC is not present in the current execution environment.** [`AccountControl.tla`](tla/AccountControl.tla) is therefore authored but not TLC-executed here. [`TLAPLUS_TOOL_PIN.md`](TLAPLUS_TOOL_PIN.md) records the selected verifier/tool pin for a reproducible later run.
2. **V-005 cannot pass yet** because per-object CDDL/numeric field registries and domain-separation registries for all signed/hashed v1 objects have not yet been frozen. This is a verification deliverable gap, not a reopening of the deterministic-CBOR architecture decision.
3. Browser/runtime/media/plugin/parser gates require empirical implementation evidence. Standards availability or a plausible design does not satisfy them.

Engineering remains unauthorized until every blocking gate is actually passed and a later explicit ADR unlocks engineering.

## 2026-09-25 gate lifecycle and one-way execution

ADR-0134 separates design closure, repo-local pre-engineering evidence, post-scope implementation evidence and genuinely external evidence. The machine ledger is authoritative.

- A blocked scope may use an isolated, non-production feasibility prototype under `verification/` only when a gate requires it.
- A criterion that inherently needs the real blocked implementation belongs in `pre_release_criteria`.
- Interface-impact gates now carry explicit `feasibility_criteria`; those criteria alone determine `feasibility_status=evidenced`.
- `criteria_met` must exactly match canonical criteria. Free-text `remaining` lists are no longer authoritative and have been removed.
- `independent_reviewer` work is batched **after** the verification executor finishes all repo-local criteria; it is not a reason to bounce the repository between agents mid-run.
- `independent_implementation` may be a cleanly separated different-language/runtime implementation; cross-vendor review is needed only when the gate separately names `independent_reviewer`.
- V-018 remains a real human participant study and cannot be synthesized by an agent.

The repo-local `core_kernel` executor pass is complete to the current checkpoint. The active next step is the consolidated different-vendor review in [`reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md`](reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md); [`CLAUDE_OPUS_5_5_HANDOFF.md`](CLAUDE_OPUS_5_5_HANDOFF.md) is retained as a stable redirect. Product engineering remains unauthorized.
