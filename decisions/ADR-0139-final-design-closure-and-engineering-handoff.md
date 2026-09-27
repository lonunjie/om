# ADR-0139 — Final design closure and engineering handoff boundary

- **Status:** Accepted architecture design; final design-closure decision, no engineering authorization implied
- **Date:** 2026-09-26
- **Author:** GPT-5.6 Sol final architecture-closure pass
- **Amends:** ADR-0110, ADR-0119, ADR-0125, ADR-0130, ADR-0134, ADR-0137, ADR-0138
- **Scope:** architecture/specification only; no TLC, Rust, Wasm/native build or implementation evidence is claimed

## Context

After ADR-0138 closed the explicit architecture-design queue, the final closure pass audited normative wording, requirement/threat/decision/invariant traceability, and cross-boundary attack classes across all eleven engineering scopes. Two semantic ambiguities were material enough to close before engineering, while two other stale-status ambiguities needed an authoritative disposition:

1. SecurityAdvisory expiry, supersession and user-override text could be read as permission to lower an already-established relationship security floor.
2. permissive logical JSON schemas could be mistaken for wire authority even where W0 CDDL and registries are already frozen.
3. several canonical documents retained historical headings such as “open question”, “candidate” or “TBD” after later ADRs had already resolved or deliberately deferred those topics.
4. `OMA-ENC-1-candidate` was a verification placeholder and must not accidentally become a production archive-encryption profile by implementation inertia.

The pass also re-attacked authority laundering, optional security context, stable correlators, fail-open behavior, stale/ABA transitions, downgrade paths, recovery escalation, provider colocation, multi-device races, multi-account isolation, portability authority, offline/partition behavior, compromised-client blast radius, update/distribution capture and resource-exhaustion boundaries. No additional blocking architecture choice remained after the rules below.

## Decision

### 1. Frozen W0 wire authority has an explicit precedence order

For an object/version whose W0 wire form is frozen, the authoritative bytes and accepted fields are defined by the applicable canonical CDDL together with frozen registries and domain-separation/signature/hash rules. Logical JSON schemas are documentation/modeling views. They may help tools and architecture reasoning, but they do **not**:

- add a wire field;
- authorize an otherwise unknown property;
- alter critical-extension handling;
- change canonical encoding; or
- change the bytes covered by a hash or signature.

If a logical schema and frozen wire definition disagree, the frozen wire definition wins and the schema is a documentation defect to repair. A later object/version may evolve only through the normal registry/version/CryptoProfile process.

### 2. SecurityAdvisory policy cannot move an established relationship floor backward

A SecurityAdvisory may impose an additional temporary or persistent recommendation/policy floor above the relationship's stored cryptographic floor. Expiry, supersession or an allowed user override may stop applying **that additional advisory floor**. It cannot lower the relationship's already-established stored CryptoProfile/protocol floor, resurrect a forbidden profile, or reinterpret downgrade state.

A user override of a non-threshold advisory therefore means “do not apply this advisory's extra recommendation”, not “forget the cryptographic level this relationship has already established”. Root-threshold floor changes remain subject to their existing stronger rules.

### 3. Historical uncertainty in canonical documents is closed or reclassified

Canonical identity, discovery, multi-device and transparency documents now distinguish architecture decisions from later verification/release tasks. In particular:

- OM has no global mandatory key-transparency authority; an optional per-directory transparency mechanism is replaceable and any public transparency claim remains gated by PR-011/external review.
- multi-device sync architecture is settled as account-private authenticated E2EE synchronization and cannot carry AccountControlState authority through CRDT/LWW semantics; exact transport/codec implementation is engineering-owned under its gates.
- alias scope, verifier trust, recovery rotation/staleness and former continuity questions have explicit final dispositions rather than “open question” headings.
- stale `TBD` wording in frozen W0 logical views is removed where CDDL/registries already provide the answer.

A verification gate, implementation choice or release-claim gate is not an “open architecture question” merely because its evidence is still pending.

### 4. `OMA-ENC-1-candidate` is verification-only and cannot ship

The archive-encryption candidate is retained only as a V-014 verification fixture/placeholder. It is not a production CryptoProfile and does not authorize a portability implementation to ship its current parameters. Before the `portability` scope can be authorized, V-014 must freeze a reviewed archive-encryption profile, vectors, limits and migration story through the normal architecture-change process.

### 5. Security invariants are machine-traceable closure objects

[`machine/security_invariant_traceability.json`](../machine/security_invariant_traceability.json) is added as a design-closure audit artifact. Every current invariant in [`SECURITY_INVARIANTS.md`](../docs/architecture/safety/SECURITY_INVARIANTS.md) maps to current requirements, relevant threats/decisions, evidence gates or conformance obligations, and normative sources. The map does not mark those gates passed; it prevents security properties from existing only as prose.

### 6. The eleven-scope adversarial pass closes architecture, not verification

The final report [`docs/reports/2026-09-26-final-architecture-closure-and-adversarial-review.md`](../docs/reports/2026-09-26-final-architecture-closure-and-adversarial-review.md) records the cross-boundary attack pass across:

`core_kernel`, `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, `calls`, `communities_publications`, `services_bots`, `plugins`, and `bridges`.

The pass found no remaining blocking architecture choice after the repairs in this ADR and ADR-0138. This is **not** evidence that implementation-dependent verification gates are passed. Dynamic toolchains, real runtimes, interoperability, fuzzing, browser/platform behavior, performance, independent cryptographic review and user studies stay where the V-*/PR-* ledgers put them.

### 7. Architecture is design-frozen; implementation contradictions require an ADR

The canonical handoff is [`docs/process/ENGINEER_START_HERE.md`](../docs/process/ENGINEER_START_HERE.md) plus [`docs/process/ENGINEERING_HANDOFF.md`](../docs/process/ENGINEERING_HANDOFF.md). Engineering agents may choose implementation details only inside those boundaries. They must not silently reinterpret authority, privacy, downgrade, encoding, recovery, provider or extension semantics for convenience.

If implementation reality contradicts the frozen design, the response is evidence + an ADR + threat/requirement/migration/conformance updates, not an implementation-local exception.

### 8. Design closure does not authorize engineering

`design_open=0` means architecture choices are closed. It does not mean verification is complete. [`machine/repository_contract.json`](../machine/repository_contract.json) remains authoritative for named-scope authorization. All scopes remain unauthorized until their declared pre-engineering/feasibility/independence requirements are satisfied by the appropriate repo/tool-complete/external work.

## Consequences

- W0 parsers have one unambiguous wire authority.
- SecurityAdvisory lifecycle cannot become a downgrade mechanism.
- later-wave verification placeholders cannot silently become production protocol choices.
- security invariants have an explicit machine trace.
- the engineering handoff can be treated as design-frozen rather than “architecture still being invented”.
- future design edits require a real contradiction/new requirement and an ADR; polish alone is not a reason to reopen architecture.
- engineering remains unauthorized at this checkpoint.
