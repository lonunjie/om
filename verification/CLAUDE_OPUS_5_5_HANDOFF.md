# Claude Opus 5.5 — current OM handoff

**Current status:** **ACTIVE after Run 7 protocol-evolution hardening; latest architecture ADR is ADR-0165.** Architecture remains pre-engineering with `design_open=0`, `engineering_authorized=false` and 0/12 scopes authorized. Runs 5–7 added implementation-readiness, dangerous-algorithm and protocol-evolution guardrails without changing frozen W0 wire shape. They make crash/restart, lost-response retry identity, group Commit local journaling, dangerous-transition ordering, compatibility evolution and scope dependency/handoff contracts explicit. V-004 must verify the ADR-0163 group crash/retry model in addition to its existing formal/runtime/wire/independent obligations; no gate is self-closed by this checkpoint.

The bounded different-vendor `independent_reviewer` handoff under ADR-0117 remains:

[`reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md`](reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md)

Use the machine ledgers as truth. In particular:

- use `make verify-drop` for a raw exported ZIP; use `make verify` only after importing into a Git worktree with the full required toolchain;
- V-021 now has 11/11 repo-local criteria evidenced, including [`ui-integration-adr0162-latest.json`](results/ui-integration-adr0162-latest.json), but required independent/pre-release obligations remain open;
- do not redo already-complete repo-local work merely because an independent-review field is still open;
- retain the earlier ADR-0157 V-005 strict two-codec obligation until the Rust-side decoder is actually run;
- retain V-024 until the LAN CDDL/vectors/models/runtime and independent real-LAN interoperability work is actually completed;
- do not broadly redesign OM without a concrete contradiction/counterexample;
- do not set `engineering_authorized=true`; if all review prerequisites pass, report only eligibility for the explicitly scoped unlock process.

The machine truth remains [`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl); this file is only a stable operational entry point retained because the repository contract names it.

## 2026-09-29 Run 7 — protocol-evolution and agent guardrails

Before changing normative protocol/profile/extension material, read [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) and [`machine/protocol_evolution_policy.json`](../machine/protocol_evolution_policy.json). Published meanings are immutable; negotiation is `security_generation`-first; extension criticality is immutable; and incompatible changes require new immutable lineage/profile/extension or an explicit fork. ADR-0165 change classes determine the required ledgers and continuity artifacts. Engineering remains unauthorized.
