# Claude Opus 5.5 — current OM handoff

**Current status:** **ACTIVE after Run 9B / ADR-0153.** Run 9B completes the two repo-local criteria opened by ADR-0152: V-015 is now 16/16 repo-local with replaceable Android wake/privacy evidence, and V-021 is 10/10 repo-local with desktop semantic-parity/ProfileExecutionContext and PersonRecord export-safety evidence. ADR-0153 removes the unsupported claim that an unconfirmable third-party push registration has a universal finite retirement bound; stale wake state remains metadata-only and never fetch/message/account authority. Required independent-review/pre-release obligations remain open. Engineering remains unauthorized.

The bounded different-vendor `independent_reviewer` handoff under ADR-0117 is:

[`reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md`](reviews/BRIEF-2026-09-27-final-core-kernel-independent-review.md)

Run 9A is **not** review-ready. After Run 9B reactivates the handoff, read the brief plus the machine ledger before reviewing. The review procedure will remain:

- use `make verify-drop` for a raw exported ZIP;
- use `make verify` only after importing into a Git worktree with the full required toolchain;
- review V-002, V-003, V-015, V-016 and V-017 as one consolidated batch;
- include ADR-0150/0151 evidence in [`lan-route-adr0150-latest.json`](results/lan-route-adr0150-latest.json), [`lan-privacy-adr0150-latest.json`](results/lan-privacy-adr0150-latest.json), [`rich-list-adr0150-latest.json`](results/rich-list-adr0150-latest.json) and [`platform-reference-adr0150-latest.json`](results/platform-reference-adr0150-latest.json);
- include the ADR-0147 evidence in [`recovery-durable-timer-adr0147-latest.json`](results/recovery-durable-timer-adr0147-latest.json) and [`sealed-delivery-adr0147-lifecycle-restore-latest.json`](results/sealed-delivery-adr0147-lifecycle-restore-latest.json), plus ADR-0148/0149 account-private-sync evidence in [`account-private-sync-adr0148-latest.json`](results/account-private-sync-adr0148-latest.json) and [`account-private-sync-adr0149-hardening-latest.json`](results/account-private-sync-adr0149-hardening-latest.json);
- do not redo already-complete repo-local work merely because an independent-review field is still open;
- do not broadly redesign OM without a concrete contradiction/counterexample;
- do not set `engineering_authorized=true`; if all review prerequisites pass, report only that `core_kernel` is eligible for an explicit scoped unlock ADR.

The machine truth remains [`machine/architecture_verification_gates.jsonl`](../machine/architecture_verification_gates.jsonl); this file is only a stable operational entry point retained because the repository contract names it.
