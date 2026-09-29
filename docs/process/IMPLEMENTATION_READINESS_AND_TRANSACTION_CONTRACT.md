# OM Implementation Readiness and Transaction Contract

- **Status:** Canonical pre-engineering handoff contract
- **Checkpoint:** 2026-09-29 Run 5 / ADR-0160
- **Engineering:** not authorized; this document orders and constrains later implementation only

## 1. Purpose

OM already has scope-granular authorization gates. This document answers a different question: **once a named scope is authorized, what order and transaction/retry discipline should an implementation follow so it does not reinterpret the design?**

The machine-readable companion is [`machine/engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json). [`machine/repository_contract.json`](../../machine/repository_contract.json) remains authoritative for whether a scope is allowed to start.

## 2. Two dependency graphs, never one hidden graph

### Authorization dependencies

Only `repository_contract.json -> engineering_scopes[*].depends_on` can block authorization because another scope has not yet been authorized.

### Integration prerequisites

An end-to-end scenario may need another implementation to exist, but that does not turn the other component into protocol authority or a new authorization dependency. Examples:

- `reference_client_ui` can be built against a Core harness and does not require `provider_reference` authorization merely to implement UI semantics;
- `provider_reference` implements the frozen Home API and does not become an AccountID/control authority;
- `core_kernel` must be testable against provider/transport fakes and never import the reference provider;
- later scopes can use conformance harnesses for earlier interfaces without making one reference binary the compatibility definition.

## 3. Scope-level implementation order

After each scope independently satisfies its gates and receives an authorizing ADR, the dependency graph permits these layers:

| Layer | Scopes | Rule |
|---|---|---|
| 0 | `core_kernel` | protocol/core foundation; no reference UI/provider dependency |
| 1 | `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, `local_delivery` | may proceed in parallel after `core_kernel` authorization; each still needs its own gates |
| 2 | `calls`, `communities_publications`, `services_bots`, `plugins`, `bridges` | follow their declared `groups` or `reference_client_ui` dependency |

Wave labels order product delivery. They are not substitutes for this dependency graph and never authorize a scope.

### Core-kernel internal order

Within an authorized `core_kernel`, prefer this dependency direction:

1. bounded value types + canonical encoding/registries;
2. identity/DeviceCredential/AccountControlState validation and persistence primitives;
3. CryptoProfile registry + async key material + one-to-one session state;
4. immutable ConversationEvent validation/materialization and stable logical dedupe;
5. sealed delivery, relay selection/failover, blob identity/location and discovery/IntroGate flows;
6. bounded account-private sync and local repair/reconciliation;
7. narrow Core command/view API and WASM/native bridges.

The order is not permission to weaken a gate. It minimizes circular dependencies and keeps I/O/provider details above protocol validation.

## 4. Stable operation identity before effect

A mutation that can outlive one process/network attempt records its stable logical identity and security parent/context before the first externally visible effect.

Use the identity already defined for the operation. Examples include:

- `ConversationEvent.event_id` for a logical message/action;
- Home API `request_id` for provider mutations;
- `(delivery_key_id, delivery_id, wrapper fingerprint)` for sealed-delivery retry state;
- `GroupControlIntent` + exact MLS Commit/Binding hash for group transitions;
- object lineage `(id, version, previous_hash)` for mutable signed location/route state;
- request/bootstrap identifiers already frozen by the introduction profile.

Do not add a second operation merely because an acknowledgement was lost.

## 5. Canonical outcome and retry model

Core distinguishes **outcome** from **reason**.

| Outcome | Meaning | Same-operation automatic retry? |
|---|---|---|
| `Committed` | canonical/local durable effect is complete | no |
| `PendingAuthorization` | waiting for an authenticated authority/control transition | no blind retry; observe/reconcile |
| `PendingNetwork` | exact retry is safe but transport/provider is unavailable/backing off | yes, using the same logical id/idempotency key |
| `PendingReconciliation` | request may have taken effect; current authenticated outcome is unknown | reconcile or exact-id retry only; never create a new semantic operation merely due to timeout |
| `NeedsUserAction` | explicit user decision/credential/permission required | only after that action |
| `Rejected(reason)` | current operation instance is definitively invalid/denied/terminal | no unchanged retry |

### Reason classes

Stable Core reason classes include at least:

- malformed/invalid input;
- unsupported version/feature;
- unauthorized;
- revoked;
- expired;
- stale state/generation;
- conflict/equivocation;
- policy denied/admission denied;
- quota;
- rate limited;
- integrity/signature failure;
- temporarily unavailable;
- not found.

These are semantic categories, not a new wire registry. The Home API keeps [`om-home-errors.json`](../../spec/registries/om-home-errors.json). UI translates Core semantics to user language; diagnostics may retain bounded raw provider detail.

### Retry rules

1. `rate_limited`/temporary unavailability may exact-retry with the same idempotency identity and bounded backoff/`retry_after` where defined.
2. stale/conflict requires authenticated refresh/reconciliation before a new semantic mutation.
3. unauthorized/revoked/expired/integrity/policy failures are not auto-retried unchanged.
4. `not_found` is operation-specific. It is success-equivalent only where the canonical operation says so (for example an idempotent exact-target delete).
5. an unknown Home error remains a non-retryable protocol failure as frozen by the Home API.
6. a network timeout after bytes may have left the endpoint is **not** evidence of rejection.

## 6. Crash-consistent transaction rule

For accepted state that affects security, authorization, dedupe or remote retry behavior:

1. write intent/replay/idempotency context before the external effect when required;
2. stage uncommitted state separately from canonical state;
3. atomically commit all local state needed to recognize completion, or use a durable write-ahead/recovery journal with equivalent semantics;
4. only then release irreversible application effects or acknowledge in a way that permits peer/provider retry material to disappear;
5. on restart, replay/reconcile the journal before accepting new conflicting work.

A database transaction is one implementation. It is not the protocol requirement; crash-consistent equivalence is.

## 7. Group transition crash contract

[`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) is canonical. The local stages are:

`INTENT_DURABLE -> CANDIDATE_PREPARED -> PUBLISHED_UNCONFIRMED -> ACCEPTED | SUPERSEDED | ABANDONED`.

Important implementation consequences:

- exact commit bytes are durable before publication;
- relay acknowledgement is never canonical acceptance;
- tentative MLS/application state is isolated;
- no application message is sent on a tentative branch;
- accepted MLS state + GroupControlState + group head + dedupe marker promote atomically/equivalently;
- a losing branch is inert and cannot leak application/control effects;
- restart/offline recovery reconciles the canonical chain before rebase/retry;
- missing prepared state fails closed rather than reconstructing guessed Commit bytes.

V-004 still has to prove this under formal crash/partition/equivocation models and a real MLS implementation.

## 8. Scope contract consumption rule

Before implementing a scope, an agent reads its entry in [`machine/engineering_scope_contracts.json`](../../machine/engineering_scope_contracts.json), then follows every listed normative source. The scope contract is an index/guardrail, not a replacement for those sources.

A scope implementation MUST NOT:

- infer authorization from relay/provider success;
- invent an unfrozen wire field/object;
- create a reference-provider dependency in Core;
- treat wall/HLC time as authority;
- make a retry into a second semantic user action without the operation's explicit retry rule;
- restore historical security/authority state as live state;
- weaken a fail-closed rule to improve availability;
- silently choose between contradictory normative sources—raise an ADR instead.

## 9. Restart audit checklist

For every persistent state machine, implementation review answers all of these:

1. What durable record exists **before** the first external effect?
2. What if the process dies immediately before or after each durable write?
3. What if a request was accepted remotely but its response/ack was lost?
4. Which exact identifier makes replay idempotent?
5. What is the maximum replay/tombstone retention and what bounds it?
6. Which state is tentative and forbidden from UI/application release?
7. How is half-applied multi-store state recovered?
8. Can an old backup or restored clock/counter resurrect the operation?
9. What happens if authorization changes while the operation is pending?
10. Does a long-offline client reconcile canonical state before retrying?

If any answer depends on an unstated assumption, the implementation stops and raises an architecture issue rather than improvising.
