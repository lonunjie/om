# Architecture Contract and Drift Control

- **Status:** Canonical architecture-process contract
- **Checkpoint:** 2026-09-22 23:02 PHT
- **Phase:** Architecture hardening / pre-engineering

## 1. Principle

OM's philosophy remains prose because values require human interpretation. The repository's operational architecture contract is machine-readable and validated by code.

> **If a security/authority/wire commitment is important enough to rely on, it should be represented in the executable repository contract wherever practical.**

## 2. Authoritative machine artifacts

- `machine/repository_contract.json` — phase, canonical file classes, required invariants, schema policy and validation rules;
- `machine/architecture_map.json` — domain-to-document/schema/decision/requirement/open-question map;
- `machine/file_registry.json` — every current non-history repository file and its role/license/domain;
- `machine/stewardship.json` — project maturity/custody roles without pretending unfilled redundancy exists;
- `machine/decisions.jsonl` — compact accepted/superseded decisions;
- `machine/requirements.jsonl` — normative MUST/SHOULD obligations;
- `machine/open_questions.jsonl` — unresolved gates that prevent freeze where blocking;
- `MANIFEST.json` — byte/hash inventory of the complete checkpoint.

## 3. Validator

`tools/validate_architecture.py` MUST fail when detectable drift occurs, including:

- a current file appears without file-registry classification;
- a registry entry points to a missing file;
- JSON/JSONL becomes unparsable or record IDs duplicate;
- an ADR is missing from `decisions/INDEX.md`;
- a local Markdown link points to a missing current file;
- more than one current ProviderCapabilities schema is exposed;
- the canonical identity document reintroduces key-derived AccountID construction;
- current authority documents use a superseded generic account-authority term where `AccountControlState` is required;
- the phase changes from pre-engineering without the architecture-freeze decision/contract being updated;
- mandatory constitutional files disappear;
- the manifest does not match the checkpoint contents.

## 4. Architecture-before-engineering lock

This checkpoint does **not** authorize product engineering.

Allowed work before architecture freeze:

- research;
- standards/library feasibility spikes that do not establish production architecture by accident;
- threat modeling;
- formal/spec modeling used to validate architecture;
- wire/schema design;
- conformance-vector design;
- repository validation/tooling;
- documentation and ADR refinement.

Not authorized as the next phase:

- building the product vertical slice;
- committing to a production crypto library because coding has started;
- hardening provisional wire formats through implementation inertia;
- treating dev code as architectural evidence unless explicitly run as a feasibility experiment.

## 5. Exit condition

Engineering can begin only when a future checkpoint deliberately changes:

```text
phase = architecture_hardening_pre_engineering
engineering_authorized = false
```

to an engineering-authorized state after all blocking architecture questions are either:

1. resolved and encoded; or
2. explicitly classified as non-blocking with a bounded interface that cannot force a later incompatible protocol/security redesign.

That transition requires its own accepted ADR and a clean validator run.
