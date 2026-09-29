# ADR-0160 — Implementation-readiness contracts, crash-safe group transitions and outcome/retry semantics

- **Status:** Accepted
- **Date:** 2026-09-29
- **Scope:** Run 5 implementation-readiness hardening; no product implementation
- **Amends:** ADR-0108, ADR-0120, ADR-0121, ADR-0128, ADR-0134, ADR-0139, ADR-0156
- **Does not authorize:** any engineering scope or satisfy independent/runtime/two-codec verification obligations

## Context

The design is closed and the remaining verification gates are explicit, but an implementation-readiness pass found three places where a future engineering agent could still make incompatible choices without obviously violating a wire schema.

1. [`machine/repository_contract.json`](../machine/repository_contract.json) correctly defines **authorization dependencies**, but the handoff did not separately state build/integration ordering. That invites chicken-and-egg interpretations such as making `core_kernel` depend on a concrete provider implementation, or treating UI/provider availability as authority prerequisites.
2. Crash/retry rules are strong for established one-to-one sealed delivery, bootstrap and several recovery paths, but the group architecture still listed crash/partial-publication cases as things the transition protocol “must specify”. The mechanism was selected, yet a future implementation could still choose different persistence/acknowledgement behavior around an MLS Commit.
3. Home provider errors are typed, but the Core/UI boundary used only illustrative `Committed/Pending/Rejected` states. A timeout after an externally visible mutation could therefore be misreported as a rejection and retried with a new semantic identity, creating duplicate effects or divergent local state.

These are implementation-contract ambiguities, not reasons to change W0 wire formats or cryptographic authority.

## Decisions

### 1. Authorization dependencies and integration prerequisites are distinct

`machine/repository_contract.json -> engineering_scopes[*].depends_on` remains the **only authorization dependency graph**. It answers whether a scope may be authorized after its gates pass.

[`machine/engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json) is added as a machine-readable implementation-readiness view. For every scope it records:

- the exact authorization dependencies copied from the repository contract;
- normative source documents;
- integration prerequisites that may be needed for end-to-end testing but are **not** new authorization dependencies;
- restart/crash obligations;
- error/retry obligations;
- forbidden shortcuts.

A provider implementation is therefore not allowed to become a hidden prerequisite for `core_kernel`; a reference UI may use harnesses/fakes while its own scope is developed; and integration dependencies never grant protocol authority.

The scope graph has three topological implementation layers once scopes are separately authorized:

1. `core_kernel`;
2. `reference_client_ui`, `provider_reference`, `groups`, `portability`, `expressive_media`, `local_delivery`;
3. `calls`, `communities_publications`, `services_bots`, `plugins`, `bridges`.

This ordering is guidance derived from the existing dependency graph, not authorization by wave order.

### 2. Mutating operations have a durable logical identity before external effect

For a mutating operation that can survive/retry across a process crash or network uncertainty, Core persists the operation's stable logical identity and security-relevant parent/context **before the first externally visible effect**.

The stable identity is the operation's already-defined protocol identity where one exists: for example EventID, request_id, delivery tuple/fingerprint, GroupControlIntent identity/commit hash, record generation/parent, or another scope-frozen idempotency key. ADR-0160 does **not** add a new W0 wire field.

Rules:

- a transport timeout/disconnect after send is an **unknown outcome**, not proof of rejection;
- an exact retry reuses the same logical identity whenever that operation's contract permits exact retry;
- an implementation MUST NOT generate a second semantic operation merely because acknowledgement was lost;
- if an operation has no safe idempotent retry contract, the client reconciles current authenticated state before creating a new operation;
- a new semantic identity is used only for a genuinely new attempt required by the protocol (for example resealing on a different route where the sealed-delivery profile requires a fresh delivery id), while the higher-level EventID/logical action remains stable where specified.

### 3. Durable commit precedes irreversible local release/ack

When protocol acceptance changes durable canonical/security state, implementations use one transaction or an equivalent write-ahead/recovery journal so a crash cannot expose half of the accepted transition.

Before application/UI release or an acknowledgement that would let the remote side discard retry material, the implementation durably records all local state required to recognize the operation as complete and avoid repeating its effects.

This generalizes—not replaces—the stricter one-to-one/bootstrap rules in ADR-0143..0147. A scope-specific rule wins where it is stronger.

### 4. GroupControlIntent / MLS Commit uses a crash-safe local transition journal

The `groups` scope uses the following local implementation states for membership/role-affecting changes. These states are local implementation semantics, not new wire objects:

`INTENT_DURABLE -> CANDIDATE_PREPARED -> PUBLISHED_UNCONFIRMED -> ACCEPTED`

with terminal alternatives `SUPERSEDED` or `ABANDONED`.

Normative behavior:

1. **Intent first.** The authenticated `GroupControlIntent`, its immutable target incarnation, authorization evidence and parent `(GroupControlState hash, MLS epoch)` are durable before creating/publishing a Commit.
2. **Prepare before publish.** Before any relay/network publication, persist the exact MLS Commit bytes/hash, the `GroupCommitBinding`, parent identifiers and enough tentative post-commit MLS/application state to resume or safely discard the candidate after restart. Tentative state is isolated from canonical Room state.
3. **Relay acknowledgement is not canonical acceptance.** A relay saying “stored” only proves transport progress. It does not authorize or commit Room state.
4. **Exact publication retry.** After crash or uncertain publication, if the recorded parent is still canonical and the candidate remains eligible, retransmit the exact prepared commit/binding rather than generating a second same-parent semantic mutation solely due to timeout.
5. **Canonical promotion is atomic.** When the ordering profile accepts the candidate, the implementation atomically promotes the new MLS state, GroupControlState/application authority state, canonical group head and accepted-commit/dedupe marker. Where those stores cannot share one transaction, a durable recovery journal MUST make replay converge to the same completed state before application messages are sent or the operation is shown as committed.
6. **No tentative-branch messaging.** Application messages are not sent under a tentative membership/control branch before its Commit is accepted as canonical.
7. **Duplicate accepted delivery is idempotent.** Re-observing the same accepted commit/binding does not repeat application/control effects.
8. **Losing branch is inert.** A same-epoch losing commit becomes `SUPERSEDED`; its tentative MLS/application state is never materialized as canonical. A still-applicable reducing intent is rebased as already required by ADR-0128. A non-reducing intent is revalidated against the new parent and may be dropped.
9. **Long-offline restart reconciles first.** A client returning after an unknown/stale parent first reconstructs the authenticated canonical group/control chain. Prepared candidates whose parent is no longer canonical are superseded/abandoned before any rebase/retry.
10. **Irrecoverable prepared state fails closed.** If exact prepared commit state needed for safe retry cannot be recovered, the implementation does not invent equivalent bytes from partial state. It reconciles the canonical head and constructs a fresh authorized candidate only if the underlying intent remains valid.

V-004 still owns the formal crash/partition model, MLS-library feasibility, two-codec group wire freeze and independent review. This ADR supplies the implementation semantics that those tests must exercise; it does not mark them passed.

### 5. Core exposes outcome class separately from reason

The UI/Core boundary uses stable semantic outcome classes:

- `Committed`
- `PendingAuthorization`
- `PendingNetwork`
- `PendingReconciliation`
- `NeedsUserAction`
- `Rejected(reason)`

`Rejected` is terminal for the **current operation instance** unless its reason explicitly requires changed authenticated state/input. A network timeout after possible transmission is never mapped directly to `Rejected`; it becomes `PendingNetwork` or `PendingReconciliation` according to whether exact retry is safe.

Reason categories are semantic, provider-neutral values such as malformed/invalid input, unsupported, unauthorized, revoked, expired, stale state, conflict, policy denied, quota, rate limited, integrity failure, unavailable and not found. Provider-specific details remain diagnostics.

The Home API keeps its frozen `HomeError` numeric registry. Core maps those codes into this semantic model; it does not rewrite the Home wire registry. `stale_generation`/conflict-like failures require authenticated refresh/reconciliation before a new semantic operation, while `rate_limited`/`retryable` may exact-retry with the same request id according to the API contract. Unknown Home errors remain non-retryable protocol failures as already specified.

### 6. `Pending` state is durable when loss would duplicate or misreport an effect

UI optimism may render immediately, but any operation whose disappearance after restart could cause duplicate network/control effects or falsely show success must have enough durable Core state to recover as `Pending*`, `Committed` or `Rejected` after restart.

A UI shell may not convert “process restarted” into “operation failed” and then originate a new semantic operation without asking Core to reconcile.

### 7. The implementation-readiness contract is executable repository state

The architecture validator checks that:

- all and only the 12 engineering scopes appear in [`machine/engineering_scope_contracts.json`](../machine/engineering_scope_contracts.json);
- its authorization dependencies exactly match [`repository_contract.json`](../machine/repository_contract.json);
- the dependency graph is acyclic and the declared implementation layers are a valid topological layering;
- every listed normative source exists;
- each scope declares restart, retry and forbidden-shortcut clauses;
- the group crash-journal and Core outcome rules remain present in their canonical documents.

This guard prevents the handoff contract from silently drifting after design closure.

## Consequences

- Engineering agents get a deterministic build/dependency map without granting hidden authority to reference infrastructure.
- Lost acknowledgements and process crashes are represented as uncertainty/reconciliation rather than false rejection.
- The selected group architecture now has a concrete crash-safe local transaction contract before V-004 runs its formal/runtime proof.
- No W0 wire format, cryptographic primitive, AccountID rule or provider-authority rule changes.
- Verification gates remain authoritative and open where they were open; this pass does not self-approve human, Rust, MLS-runtime, two-codec or interoperability evidence.
