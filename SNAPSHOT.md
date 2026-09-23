# OM Architecture Snapshot

Generated: **2026-09-23 09:20 PHT (UTC+08:00)**

## State

OM is in **architecture verification / pre-engineering**. **Engineering is not authorized.** `engineering_authorized=false`.

The architecture-design queue A-001..A-015 remains fully design-resolved (`design_open=0`). All blocking verification gates V-001..V-017 remain open (`verification_open=17`).

## Phase-1 verification evidence added in this checkpoint

- executable bounded/reference models now cover V-002, V-004, V-008, V-009, V-011, V-013, V-014, V-015, V-016 and part of V-017;
- independently written Python and Node.js deterministic-CBOR foundation encoders agree on 14 current fixtures for V-005 groundwork;
- `AccountControlState.tla` + TLC configuration now exist for V-002, but TLC was not available in the current execution environment and was **not** claimed as run;
- 7,772 bounded/adversarial cases completed without a modeled invariant violation;
- an offline 60-artifact continuity dry run completed and produced an integrity root, while the full V-017 disaster/fork/rebuild exercise remains open;
- feasibility reconnaissance narrows experiments for V-001/V-006/V-007/V-010/V-012 without treating standards availability as proof.

## Important gap found rather than hidden

V-005 cannot pass yet: OM has selected deterministic CBOR, but the checkpoint still lacks frozen per-object CDDL/numeric-field/domain-separation registries for **every** signed/hashed v1 object. The two-language CBOR test validates the encoding foundation only. This is now explicitly recorded as a verification deliverable.

## Gate accounting

- `design_open=0`
- `verification_open=17`
- `engineering_authorized=false`
- gates with new executable phase-1 evidence: V-002, V-004, V-005, V-008, V-009, V-011, V-013, V-014, V-015, V-016, V-017
- gates still awaiting their first dedicated executable/runtime harness: V-001, V-003, V-006, V-007, V-010, V-012

## Next verification work

1. run the pinned TLA+/TLC AccountControlState model and expand it through recovery/fork/rollback cases;
2. build the dedicated V-003 one-to-one session model/fuzzer;
3. freeze signed/hashed v1 object wire registries and expand V-005 to object-specific cross-implementation vectors;
4. prototype V-001/V-006/V-007/V-010 feasibility on the actual intended runtimes;
5. start parser/resource fuzzing for V-012;
6. progressively replace bounded abstractions for V-004/V-008/V-009/V-011/V-013..V-017 with independent/runtime/formal evidence required by each ledger gate.

No product engineering begins from this checkpoint.
