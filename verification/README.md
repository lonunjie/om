# OM Architecture Verification Evidence

This directory contains **pre-engineering architecture-verification artifacts**. It is intentionally separate from product implementation.

The architecture contract remains locked: `engineering_authorized=false`. **Evidence is not gate passage.** A bounded model, research result, or dry run may increase confidence and create regression fixtures, but a gate is marked `passed` only when its complete machine-ledger requirement is satisfied.

## Evidence classes

- `bounded/` — executable bounded reference models/adversarial enumerations using standard Python;
- `tla/` — TLA+ formal models/configurations for authority state machines;
- `cbor/` — independent deterministic-CBOR foundation encoders/fixtures;
- `vectors/` — regression/golden transition scenarios derived from architecture invariants;
- `results/` — machine-readable captured results;
- `FEASIBILITY_RECONNAISSANCE_20260923.md` — standards/runtime feasibility reconnaissance for gates that require real implementations.

Run all currently executable phase-1 evidence with:

```sh
python3 verification/run_phase1.py
```

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

V-001, V-003, V-006, V-007, V-010 and V-012 remain fully open. `FEASIBILITY_RECONNAISSANCE_20260923.md` narrows the experiments for V-001/V-006/V-007/V-010/V-012 but is not pass evidence. V-003 still needs its dedicated per-device 1:1 session model/fuzzer.

## Known verification-artifact gaps discovered in this checkpoint

1. **TLC is not present in the current execution environment.** `AccountControlState.tla` is therefore authored but not TLC-executed here. `TLAPLUS_TOOL_PIN.md` records the selected verifier/tool pin for a reproducible later run.
2. **V-005 cannot pass yet** because per-object CDDL/numeric field registries and domain-separation registries for all signed/hashed v1 objects have not yet been frozen. This is a verification deliverable gap, not a reopening of the deterministic-CBOR architecture decision.
3. Browser/runtime/media/plugin/parser gates require empirical implementation evidence. Standards availability or a plausible design does not satisfy them.

Engineering remains unauthorized until every blocking gate is actually passed and a later explicit ADR unlocks engineering.

## 2026-09-24 two-phase gates and independence

Gates now carry `pre_engineering_criteria` and `pre_release_criteria`, an `independence` level per phase, and `unlock_scopes` (ADR-0108/0109). The human-readable view is `GATE_PASS_CRITERIA.md`. Bounded models written by the design's author, including everything in `bounded/`, count as **supporting evidence**. They cannot satisfy a phase alone: each phase needs the independent tool, reviewer, implementation, study or external review it names.

New in this checkpoint: `bounded/continuity_assessment.py` (1,620 cases, supporting V-002/V-018) reproduces the loss/takeover tolerance table in `ACCOUNT_CONTINUITY_AND_CONTROL.md` §11.
