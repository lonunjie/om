# ADR-0161 — Dangerous algorithms and explicit failure paths

- **Status:** accepted normative implementation-readiness supplement
- **Checkpoint:** 2026-09-29 Run 6
- **Engineering:** not authorized
- **Supersedes:** no wire format or authority decision

## Context

Run 5 made crash/retry discipline and scope contracts explicit, but several security-sensitive behaviors were still distributed across prose, schemas, and bounded models. A conforming implementation could therefore agree with every document while choosing a subtly different order of checks or recovery behavior.

## Decision

Freeze a small set of implementation-facing algorithms as **normative ordering contracts**. They specify validation order, state transitions, failure handling, and forbidden shortcuts without prescribing a programming language, storage engine, cryptographic primitive implementation, or wire field not already frozen elsewhere.

The canonical algorithms are in [`docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md`](../docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md), with machine metadata in [`machine/dangerous_algorithm_contracts.json`](../machine/dangerous_algorithm_contracts.json).

The algorithms are supplements to their underlying normative sources. If a lower-level cryptographic/wire document and this supplement appear to conflict, implementation stops and raises an architecture issue; it must not choose the easier interpretation. Verification gates remain authoritative for whether these rules have been independently/formally verified.

## Scope

The supplement covers:

1. AccountControlState child validation and authorization;
2. one-to-one inbound session/sealed-delivery acceptance and crash-safe replay;
3. group Commit preparation/publication/recovery;
4. mutable RelaySet/BlobLocationRecord branch validation and deterministic selection;
5. Room HistoryBundle import isolation;
6. extension criticality/dispatch;
7. LAN frame/channel admission;
8. provider/network unknown-outcome reconciliation.

## Non-goals

- no new wire fields;
- no new cryptographic primitive or combiner;
- no change to the W0/W1 authorization gates;
- no claim that a pseudocode algorithm constitutes formal proof or interoperability evidence.

## Result

Engineering receives explicit ordering and failure semantics while V-002/V-003/V-004/V-005/V-024 retain their existing independent/runtime/two-codec obligations.
