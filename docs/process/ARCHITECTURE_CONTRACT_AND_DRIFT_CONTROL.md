# Architecture Contract and Drift Control

- **Status:** Canonical architecture-process contract
- **Checkpoint:** 2026-09-23 08:21 PHT
- **Phase:** Architecture hardening / pre-engineering

## 1. Principle

OM's philosophy remains human-readable prose. Operational architecture is mapped into machine-readable records and validated by code.

> **If a security/authority/wire/project-boundary commitment is important enough to rely on, represent it in the executable repository contract wherever practical.**

## 2. Typed machine truth

- `machine/decisions.jsonl` — accepted/superseded decisions;
- `machine/requirements.jsonl` — normative MUST/SHOULD obligations;
- `machine/resolved_questions.jsonl` — dispositions for the former Q-001..Q-061 set;
- `machine/architecture_design_queue.jsonl` — genuinely undecided protocol/product-architecture questions; open blocking entries prevent engineering;
- `machine/architecture_verification_gates.jsonl` — architecture is chosen but must be proven before engineering when `blocks_engineering=true`;
- `machine/public_release_gates.jsonl` — legal/platform/security/release checks that gate only the affected public feature/distribution;
- `machine/deferred_product_questions.jsonl` — safely bounded UI/implementation measurements;
- `machine/architecture_map.json` — domains to docs/schemas/ADRs/gates;
- `machine/file_registry.json` — every current non-history file and its role/license/domain;
- `machine/stewardship.json` — actual project custody stage;
- `machine/repository_contract.json` — phase and validator invariants;
- `MANIFEST.json` — byte/hash inventory.

A generic `open_questions.jsonl` is intentionally no longer current because it mixed fundamentally different work classes.

## 3. Engineering lock

Product engineering is **not authorized**.

Allowed work includes research, specification, formal modeling, architecture-focused feasibility experiments, conformance vectors, threat modeling and repository tooling.

> **2026-09-24:** the rule below is superseded by the scope-granular, two-phase rule in §6 (ADR-0108). As written it was circular, because some gates need real implementations to pass.

Engineering can begin only after a future ADR deliberately authorizes it and:

1. no open `architecture_design_queue` item blocks engineering;
2. every blocking architecture verification gate is passed;
3. canonical definitions are internally consistent; and
4. `python3 tools/validate_architecture.py` passes.

Public-release gates need not all block private architecture experiments, but the affected public feature/claim/distribution cannot ship until its gate passes.

## 4. Drift checks

The validator must detect at least:

- unregistered current files;
- malformed/duplicate machine records;
- unindexed ADRs;
- broken local links;
- duplicate current ProviderCapabilities schema;
- key-derived AccountID or legacy competing AccountState authority;
- incomplete former-question resolution mapping;
- premature engineering authorization;
- disappearance of the seven OM meanings;
- reintroduction of a country as protocol-global legal home;
- missing required constitutional/machine files;
- manifest byte/hash drift.

## 5. Evidence can still change a decision

“Resolved” does not mean dogma. Standards, formal proofs, adversarial review or platform reality may disprove a decision. Reopening it requires an ADR and machine-contract update so the change is explicit and traceable rather than implementation drift.

## 6. 2026-09-24 scoped unlock, two-phase gates and traceability (ADR-0108, ADR-0109)

The single all-gates lock in §3 was circular: several blocking gates need real implementations to pass. From this checkpoint:

- each verification gate carries `pre_engineering_criteria`, `pre_release_criteria`, `pre_engineering_status`, `independence` and `unlock_scopes`;
- `machine/repository_contract.json` → `engineering_scopes` lists W0..W4 scopes, all `authorized: false`;
- a scope may be authorized only by an ADR naming it, once every gate listing the scope, and every gate in `always_required_pre_engineering_gates`, has `pre_engineering_status` passed or waived by ADR;
- a gate's overall `status` becomes `passed` only when both phases pass. Public releases/claims depend on overall status;
- `machine/requirement_traceability.json` maps every requirement to its proof obligation;
- `machine/threat_register.jsonl` is the structured threat model;
- the validator enforces all of the above (`validate_0924_*` guards).

See `RELEASE_WAVES_AND_SCOPED_ENGINEERING_UNLOCK.md` and `verification/GATE_PASS_CRITERIA.md`.
