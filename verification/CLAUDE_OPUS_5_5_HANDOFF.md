# Claude Opus 5.5 — current OM handoff

**Current status:** **ACTIVE after ADR-0169 authorized engineering scope `core_kernel`.** ADR-0167 makes Claude the sole reviewer at the operator's direction; ADR-0168 fixed review findings F-14..F-17. Every other scope remains unauthorized; see [`SNAPSHOT.md`](../SNAPSHOT.md) for gate status and next work.

The bounded different-vendor `independent_reviewer` handoff under ADR-0117 remains:

[`reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md`](reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md)

Use the machine ledgers as truth. In particular:

- use `make verify-drop` for a raw exported ZIP; use `make verify` only after importing into a Git worktree with the full required toolchain;
- V-021 now has 11/11 repo-local criteria evidenced, including [`ui-integration-adr0162-latest.json`](results/ui-integration-adr0162-latest.json), but required independent/pre-release obligations remain open;
- do not redo already-complete repo-local work merely because an independent-review field is still open;
- V-005 has 13/13 repo-local criteria after R16A (including the ADR-0157 `om.critical.` convergence evidence); its independent-review/phase obligations remain open;
- V-024 is 3/4 repo-local after R16B (ADR-0158 CDDL, two-codec vectors, observer/first-contact models); real-browser WebRTC feasibility (or an explicit unavailable result) and independent review remain open;
- do not broadly redesign OM without a concrete contradiction/counterexample;
- do not set `engineering_authorized=true`; if all review prerequisites pass, report only eligibility for the explicitly scoped unlock process.

The machine truth remains [`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl); this file is only a stable operational entry point retained because the repository contract names it.

## 2026-09-29 Run 7 — protocol-evolution and agent guardrails

Before changing normative protocol/profile/extension material, read [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) and [`machine/protocol_evolution_policy.json`](../machine/protocol_evolution_policy.json). Published meanings are immutable; negotiation is `security_generation`-first; extension criticality is immutable; and incompatible changes require new immutable lineage/profile/extension or an explicit fork. ADR-0165 change classes determine the required ledgers and continuity artifacts. Engineering remains unauthorized.
