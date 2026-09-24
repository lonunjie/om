# ADR-0127 — Account authority transition rules

- **Status:** Accepted
- **Date:** 2026-09-24
- **Decided by:** integrator under operator delegation 2026-09-24 ("do everything and complete it"), from the
  architecture review
- **Source:** [`2026-09-24-claude-opus-architecture-r1.md`](../verification/reviews/2026-09-24-claude-opus-architecture-r1.md)
  (F-R1-1, F-R1-2, F-R1-4, F-R1-5, F-R1-7)
- **Amends:** ADR-0028, ADR-0029, ADR-0105, ADR-0111; [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md),
  [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](../docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md), [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md),
  [`IDENTITY_CONTINUITY.md`](../docs/architecture/identity/IDENTITY_CONTINUITY.md), [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md)

## Context

The older identity prose let a RecoveryPolicy, a guardian quorum or a single trusted device exercise authority
without the ADR-0111 delay, veto and one-vote rule, and it never said which signers authorize which AccountControlState
change. The TLA model ([`AccountControl.tla`](../verification/tla/AccountControl.tla)) already encoded most of the right answers.

## Decision

1. **Recovery is one vote everywhere.** Wherever a RecoveryPolicy can authorize a transition (permanent revocation,
   retirement, promotion, recovery-policy change, fork or freeze resolution), it is one vote toward the parent
   state's majority. It counts immediately only with at least one current, non-quarantined controller co-signing.
   Otherwise the transition completes only after the policy's activation delay (at least 24 h) with no RecoveryVeto
   from any live controller. This applies to `AccountRetirement` with `authorization_kind = recovery_policy`. Freeze
   and fork resolution take a **device-only** majority; recovery never counts there. Delay and veto are mandatory
   whenever recovery is on; no profile can turn them off.
2. **Transition table.** [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §4.1 is the normative per-delta table. Signers are
   counted against the **parent** state. A child state that combines deltas needs the strongest row among them.
   Defaults: link messaging device = 1 active controller; promote = majority; quarantine a control device = 1 active,
   non-quarantined controller (reversible by majority); remove a messaging device = 1 active controller; permanent
   revocation = majority or a rule-1 recovery vote; recovery-policy change = majority; raise the crypto floor = 1
   controller; lower it = forbidden; retirement = majority or a rule-1 recovery vote. Messaging devices are removed,
   never quarantined (the ACS has no field for it).
3. **Continuity proofs.** `AccountRetirement(reason_class = moved, successor_account_id)` authorized under rule 1 is
   the only cryptographic continuity proof that clients render as "moved". The "existing trusted device" and
   "guardians co-sign a successor" paths are deleted. Guardian recovery keeps the same AccountID through
   RECOVERY §10.3. Anything else is human evidence (IDENTITY_CONTINUITY Level 3).
4. **Fork bridge.** A bridge state's parent is the last common ancestor. It carries the critical extension
   `om.acs.fork_ack` (ADR-0126) listing every observed branch-head digest, and needs a device-only majority of the
   common ancestor's controllers. A client accepts a state that descends from the latest trusted head it knows **or**
   is a valid bridge whose `fork_ack` covers that head.
5. **LegacyPolicy is deferred to W1.** Legacy activation is not in the W0 `core_kernel` scope list. W0 verifiers
   reject `AccountRetirement` with `authorization_kind = legacy_policy`. The four fixes (a verifiable commitment,
   a committed minimum contest window, the owner-inactivity precondition and a cancellable pending state) are design
   queue item A-030. The commitment will live in a key-23 AccountControlState extension, so W0 interfaces carry it
   unchanged (ADR-0113). The inactivity precondition and the non-terminal `LegacyRetirementPending` state are already
   normative text.

## Consequences

- R-171 is amended; R-200, R-201, R-204, R-205, R-207 and R-215 are new. V-002 gains remaining items: mirror the table,
  reject successor pointers without constitutional authority, and cover recovery-alone retirement with delay and veto.
- T-050 and T-051 record the threats these rules close.
