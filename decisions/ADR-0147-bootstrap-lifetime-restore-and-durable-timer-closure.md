# ADR-0147 — Bootstrap lifetime, restore and durable verifier-timer closure

- **Status:** Accepted
- **Date:** 2026-09-28
- **Scope:** Run 5A normative design closure before executable hardening; no wire-object change and no engineering authorization
- **Amends:** ADR-0039 backup/restore boundary, ADR-0127/0131 recovery-observation timing, ADR-0144/0145/0146 MessageRequest bootstrap lifecycle

## Context

A fresh pre-review pass over ADR-0144 through ADR-0146 found four places where the intended safety property was clear but the canonical source of truth or restart behavior was not yet explicit.

First, ADR-0146 requires a bootstrap `SealedDeliveryEnvelope.expires_at` to be no later than the pending request/bootstrap acceptance expiry. `MessageRequestPayload` has no independent request-expiry field, and the durable pending-request tuple introduced by ADR-0144 does not define one. The focused Run-4 model used a separate `request_expires` input, so two conforming implementations could derive different acceptance windows despite identical wire objects.

Second, ADR-0146 limits an `OPEN` MessageRequest to eight distinct bootstrap candidates over the lifetime of the request, but did not explicitly say whether rejecting or garbage-collecting an earlier candidate frees a slot. A resource bound whose slots can be replenished by tombstone eviction is not a lifetime bound.

Third, ADR-0145/0146 define `COMPLETE_REPLAY_ONLY -> EXPIRED` and bounded replay/tombstone retention, but the exact transition/garbage-collection deadline was not fully pinned. That leaves room for either premature deletion, which breaks lost-ack retry handling, or excessive retention, which extends durable state beyond the authenticated authority window.

Fourth, the recovery design correctly makes a recovery-alone activation delay verifier-local and measured using monotonic elapsed time from first valid observation. Ordinary monotonic clocks reset on restart. The design did not yet define a conservative crash/reboot persistence rule, nor did it explicitly forbid an old backup from resurrecting live MessageRequest bootstrap keys/candidate state. Those omissions can turn safe local state into rollback or delay-shortening ambiguity.

All four issues are endpoint-local semantics. The safest closure does not add a wire field, provider authority, cryptographic primitive or global time service.

## Decisions

### 1. MessageRequest bootstrap acceptance has one derived deadline; there is no hidden independent request-expiry field

For an `OPEN` MessageRequest, define:

```text
bootstrap_acceptance_deadline = min(
    reply_route.expires_at,
    reply_delivery_key.expires_at
)
```

The values are the authenticated values carried in the signed `MessageRequestPayload`. The pending-request store MAY cache the derived deadline, but the cache is not a third authority value and MUST equal the minimum above when reconstructed or checked.

OM-Crypto-v1 therefore has **no independent hidden `request_expires` value**. A future profile that needs a deadline stricter than both route and key lifetimes must add an authenticated protocol field through the ordinary versioned wire-change process; it may not introduce endpoint-local divergence under v1.

For a new bootstrap wrapper, acceptance requires:

```text
wrapper.expires_at <= reply_delivery_key.expires_at
wrapper.expires_at <= reply_route.expires_at
wrapper.expires_at <= bootstrap_acceptance_deadline
```

The third comparison is intentionally redundant after the definition above. It remains stated as the MessageRequest lifecycle invariant and must reduce to the same authenticated minimum in implementations and executable models.

Cancellation, decline and terminal invalidation remain earlier state-machine closures. They can end acceptance before the derived deadline but cannot extend it. The frozen wall-clock skew policy applies when deciding whether a signed `expires_at` instant is currently acceptable; skew never changes the ordering requirement among the three authenticated expiry values.

### 2. The eight-candidate budget is durable and non-replenishable while the request is `OPEN`

Each newly admitted distinct bootstrap candidate consumes one of the request's eight lifetime candidate slots before durable `RESERVED` allocation. The candidate identity is the ADR-0146 tuple:

```text
(delivery_key_id, delivery_id, canonical-wrapper-fingerprint)
```

While the MessageRequest remains `OPEN`:

- an exact retry of an already admitted tuple consumes no additional slot;
- a rejected, failed or otherwise terminal candidate **does not free its slot**;
- garbage collection, compaction or storage optimization MUST NOT make a previously consumed slot available again; and
- sufficient durable identity/accounting state for all admitted candidates is retained to distinguish exact retries from a ninth distinct candidate.

Because the maximum is eight, the reference semantic model is a durable set of up to eight admitted candidate tuples plus their replay state. An implementation MAY use an equivalent representation, but it must preserve the same accepted/rejected set across crash, compaction and restart.

After the request leaves `OPEN`, no new candidate can be admitted, so the lifetime-budget counter no longer grants any capability. Candidate tombstones may then age out only under §3.

### 3. `COMPLETE_REPLAY_ONLY -> EXPIRED` and tombstone garbage collection are tied to authenticated deadlines

When a winner commits, the request records the winner's authenticated `wrapper.expires_at` and the derived `bootstrap_acceptance_deadline` together with the non-secret replay identity needed by ADR-0145.

The winning exact wrapper may receive only idempotent completed-retry acknowledgement while that **wrapper itself** is still within its frozen acceptance/skew window. The broader request tombstone does not make an expired wrapper acceptable.

The request remains `COMPLETE_REPLAY_ONLY` until the derived `bootstrap_acceptance_deadline` plus the registry's frozen expiry-decision skew has passed. During that interval it can reject non-winning/new bootstrap attempts without reconstructing live key authority. At that point it transitions to `EXPIRED`; all request-scoped replay/candidate tombstones may be garbage-collected when no other independently required audit/debug retention policy applies.

A non-winning candidate tombstone need not survive longer than both:

1. the candidate wrapper's own authenticated expiry/skew window; and
2. the period during which request-local state still needs that tuple for the eight-candidate accounting rule.

Consequently:

- while `OPEN`, admitted-candidate identity/accounting cannot be pruned in a way that replenishes the budget;
- after closure, request state itself is sufficient to reject new logical candidates, but exact-wrapper replay behavior must remain deterministic for any candidate still inside its acceptance window; and
- after `EXPIRED`, replay/tombstone retention is not authority and may be removed.

Loss or corruption of required durable replay/request state is fail-closed. It never permits reconstructing an `OPEN` request or accepting another bootstrap response.

### 4. Historical backup/restore cannot resurrect live first-contact transport state

ADR-0039's “restore history, never authority” rule explicitly includes MessageRequest bootstrap transport state.

History backups, portable archives and recovery snapshots MUST NOT restore any of the following into live endpoint state:

- a request-scoped `reply_delivery_key` private key;
- an `OPEN` MessageRequest bootstrap transaction or its live candidate budget;
- `RESERVED` bootstrap replay records;
- a `COMPLETE_REPLAY_ONLY` request as an authority-bearing live request; or
- request-scoped replay/candidate tombstones as permission to accept a wrapper.

If such data exists in an old implementation-specific backup, it is inert historical/debug material after restore. It is not merged into the live receive path.

A restored installation establishes its current account/device state under the existing restore rules and uses fresh sessions/prekeys. A still-needed first-contact attempt is re-issued as a **new MessageRequest with a fresh request key/reply key/id/route context**. An old backup therefore cannot undo a later cancel/decline/completion, resurrect a destroyed bootstrap private key, or reopen a previously closed eight-candidate budget.

This rule is about historical backup/restore. Ordinary crash recovery of the same live endpoint continues to use the crash-consistent local database rules of ADR-0143 through ADR-0146; crash recovery is not treated as “restore from backup”.

### 5. Security-critical verifier-local monotonic durations have a crash-safe conservative persistence rule

For recovery-alone activation and any other canonical security transition explicitly defined as a **durable verifier-local monotonic duration**, the verifier maintains a local timer record bound to the exact transition/request digest and required duration.

The semantic timer value is an **elapsed-time lower bound**:

```text
elapsed_lower_bound = durable_elapsed_from_completed_runtime_segments
                      + elapsed_on_the_current_monotonic_runtime_segment
```

The rules are:

1. Within one runtime/boot, elapsed time comes from a monotonic clock, never from wall-clock subtraction.
2. The verifier periodically and at relevant clean state transitions checkpoints a conservative `durable_elapsed` lower bound. A crash may lose uncheckpointed elapsed time; that can only make acceptance later, never earlier.
3. After restart, the verifier resumes from the persisted lower bound and starts a new monotonic runtime segment. **Unmeasured powered-off/reboot time does not count** toward the required delay.
4. An implementation MAY count downtime only when it has a reviewed platform monotonic/secure elapsed-time primitive whose value cannot be advanced by ordinary wall-clock changes or restored from an older snapshot. This is an optimization, not a v1 interoperability requirement.
5. A forward/backward wall-clock jump, Home timestamp, custodian timestamp or request-supplied timestamp never credits elapsed activation time.
6. Missing, corrupt, rolled-back or restored historical timer state fails safe by restarting the verifier's required interval (or otherwise using a demonstrably no-shorter lower bound). It MUST NOT reconstruct a larger elapsed value from wall time.
7. Verifier-local timer credit is not transferred between distinct verifiers through backup/private sync. Each verifier retains the ADR-0131 rule that its interval begins from its own first valid observation.

The result may delay acceptance after a crash; that availability cost is intentional. OM prefers a false-late recovery activation to a false-early constitutional transition.

This rule also applies wherever a canonical field says `skew_policy: as RecoveryPolicy.delay_seconds`, including the recovery-linked AccountRetirement contest interval, unless a later ADR explicitly supplies a different crash-safe primitive.

### 6. Normative prose and registries must use the same semantics

The following interpretations are retired:

- `reply_route` as a physically one-deposit capability;
- a separate endpoint-local `request_expires` value for MessageRequest bootstrap;
- deletion of a rejected candidate freeing one of the eight lifetime slots;
- restoration of historical bootstrap state into live request authority; and
- “live controllers” as the only parties measuring a recovery-alone activation delay.

The time registry, discovery/delivery architecture, backup policy, security invariants and machine requirements are amended to match this ADR.

### 7. Run 5B must provide executable evidence before the independent-review batch is reactivated

This Run 5A closes the normative choices but intentionally does **not** claim executable evidence for the new composition rules.

Before the package is again marked ready for the different-vendor reviewer, repo-local evidence must add at least:

- derivation of the bootstrap deadline from route/key expiries, with a mutation that introduces an independent `request_expires` value;
- rejected/tombstoned candidate pruning followed by a ninth lifetime candidate, which must still fail;
- winner and non-winner replay behavior across wrapper expiry, request deadline+skew and `COMPLETE_REPLAY_ONLY -> EXPIRED`;
- stale historical backup containing `OPEN`, `RESERVED`, bootstrap private-key and replay-only records, none of which may become live after restore;
- recovery activation across clean restart and crash, proving unmeasured downtime or wall-clock jumps cannot shorten the verifier-local delay; and
- negative mutations that credit reboot downtime from wall time or accept restored elapsed-timer state as newer authority.

Until that evidence is current, the affected V-002/V-003/V-015 repo-local criteria are open and the prior external-review handoff is paused. Engineering remains unauthorized.

## Consequences

OM-Crypto-v1 wire bytes remain unchanged. Bootstrap acceptance now has a single derivable source of truth, the eight-candidate budget is genuinely lifetime-bounded, replay-only state has a precise terminal lifetime, historical restore cannot resurrect live first-contact state, and verifier-local recovery delays remain conservative across reboot/crash.

The changes deliberately prefer delayed recovery or re-issued first contact over rollback-sensitive convenience. No provider becomes time authority, no new central service is introduced, and no historical backup becomes a source of live cryptographic authority.
