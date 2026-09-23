# OM TLA+ verification models

## AccountControlState

[`AccountControlState.tla`](AccountControlState.tla) is the first normative formal-model artifact for gate V-002. It covers:

- controller-majority authorization from the current/pre-change controller set;
- quarantine without denominator shrink;
- quorum-authorized promotion and permanent revocation;
- revoked-controller non-resurrection in current state;
- same-parent constitutional proposal conflict leading to freeze;
- explicit quorum/recovery-authorized unfreeze.

This checkpoint **does not mark V-002 passed**. The model has been authored but TLC was not available in the execution environment. The repository therefore records the model and a reproducible tool pin, while the independently executable bounded Python model provides immediate regression evidence. A later verification run must execute TLC, preserve the exact tool digest/output/state counts, extend recovery/rollback/fork scenarios as needed, and attach any counterexamples as fixtures.

The Python bounded checker is intentionally independent in structure from the TLA+ module. Passing it does not substitute for TLC.
