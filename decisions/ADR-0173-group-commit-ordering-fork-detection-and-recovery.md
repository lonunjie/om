# ADR-0173 — GroupCommitOrdering: same-parent order, fork detection and fork recovery (W1)

- **Status:** Accepted
- **Date:** 2026-10-01
- **Decided by:** design agent (Claude, sole design agent, ADR-0167), from the V-004 verification below
- **Source:** [`GroupCommit.tla`](../verification/tla/GroupCommit.tla) (TLC 1.7.4, 8 configs), the OpenMLS probe
  [`verification/prototypes/v004-mls-ordering/`](../verification/prototypes/v004-mls-ordering/README.md)
- **Amends:** [`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../docs/architecture/groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) (new §3.2, §5 rule 5), ADR-0128 rule 1, ADR-0163 §4 rules 6 and 8
- **Does not decide:** the W1 wire encoding of `GroupCommitBinding`, `GroupControlIntent` and `GroupControlState`. That is the V-004 wire freeze.

## Context

ADR-0074, ADR-0128 and ADR-0163 pick a member CommitCoordinator, make reductions live through direct commits after
`sequencing_timeout`, and require a crash-safe journal. They leave four things undecided, and V-004 had to decide them
before it could model "no silent authority fork":

1. **No ordering rule.** When two valid Commits share a parent (a coordinator that equivocates, a direct reduction racing
   the coordinator, two direct reductions in a partition), nothing says which is canonical. ADR-0128 rule 6 says the loser
   is "rebased onto the winner" without naming the winner.
2. **"Accepted" in a partition.** ADR-0163 §4 rules 5 and 6 say no application message is sent under a tentative branch
   before its Commit is accepted. A direct reduction exists because the coordinator is unreachable, so the partition cannot
   wait for anyone. The probe shows why this cannot be papered over: once a member merges a Commit, MLS cannot process the
   other branch's Commit for that epoch (`s2_partition_fork_and_readd`: both directions are rejected), and the branches
   have different secrets.
3. **A removal on the losing branch.** A device removed by a Commit that later loses is still a member of the canonical
   group, and the model's first counterexample was a device stuck "out" on a branch that did not win.
4. **Fork recovery.** The library has a recovery path (OpenMLS `fork-resolution`: re-add), but nothing says who may use it
   or which devices it may re-add.

## Decision

1. **Canonical successor.** The *parent* of a Commit is its `(GroupControlState hash, MLS epoch)`. A Commit is *valid* if it
   is authorized at its parent state (GroupControlState plus AccountControlState evidence) and, for a direct Commit, is a
   reduction or a recovery. Receivers never check whether the author's `sequencing_timeout` elapsed: the timeout is client
   behaviour that avoids needless forks, not a validity rule. Among valid Commits that share a parent, the canonical
   successor is the minimum of

   `(non-reducing?, direct?, committer DeviceID, GroupCommitBinding hash)`

   compared in that order, where a reduction sorts before a non-reduction and a coordinator-sequenced Commit sorts before a
   direct one. A recovery Commit is non-reducing and direct, so it sorts last. The key depends only on bytes of the Commit
   and its binding. It never depends on receipt order, relay order, wall clock or HLC (absolute rule on clocks).
   *Why this order:* a coordinator can win a race with its own non-reducing Commit, but never against a reduction (model
   invariant `ReductionOutranksNonReduction`; the `hash_only` mutation violates it). Reductions are monotone, so a coordinator
   keeps a reduction from landing only by publishing another reduction, and the roster is finite.
2. **Acceptance is immediate.** A member merges the best valid Commit it has seen at its parent without waiting for
   confirmation, and the author of a direct reduction merges its own. This keeps reductions live under partition and
   accepts that a better Commit may arrive later. ADR-0163 §4 rule 6 is amended: "tentative" means *not yet merged*. A
   Commit prepared but not yet merged still sends no application message under it; a merged Commit may later lose.
3. **A losing branch is detected and left.** A member that has merged a Commit and later sees a better valid Commit at the
   same parent marks its branch `SUPERSEDED`, sends no further application message on it, and queues every
   ConversationEvent it sent on that branch that no member of the winning branch is known to hold. Events are idempotent by
   EventID and ordered by EventClock, not by MLS epoch, so re-sending them under the winning branch loses nothing but
   latency. A reduction the member's own Commit carried is rebased (ADR-0128 rule 6); a non-reducing one is revalidated at
   the new parent (ADR-0163 §4 rule 8).
4. **Recovery is roster-neutral and roster-gated.** A member that is on the winning chain re-adds each device of the
   losing branch that is in the *canonical* GroupControlState roster, by MLS re-add (`fork-resolution`) or by that device's
   external commit. The recovery Commit changes no GroupControlState (`resulting_group_state_hash` equals the previous one)
   and is recorded as a `GroupCommitBinding` of kind `recovery`. Devices outside the canonical roster are never re-added,
   and a receiver stages an external commit and refuses it **before merge** when the joiner credential is not in the
   canonical roster (`s3_external_commit_policy`; the `readd_removed` mutation violates `ActiveInRoster`). Any active
   member of the winning chain may issue the recovery; if two do, rule 1 orders them.
5. **A removal on a losing branch is not a removal.** A device removed only on a branch that loses is a member of the
   canonical group and recovers by rule 4. It stays `forked` and sends nothing meanwhile. Only a removal on the canonical
   chain makes a device `out`.
6. **The journal is load-bearing.** An honest author never publishes two different Commits for one parent. ADR-0163 §4
   rules 2, 3 and 9 are what guarantee it: the `journal_loss` mutation (a restart that forgets the prepared Commit lets
   the author publish a second one) violates `NoHonestSelfEquivocation`. A coordinator equivocation is the same event
   from a faulty author and is ordered by rule 1.
7. **Calibration** (V-004 owns these two numbers). `past_epoch_window`: OpenMLS 0.9.0 with `max_past_epochs = n` decrypts a
   message sent in epoch e until epoch e+n and not after, so the ADR-0121 default 2 and maximum 8 are implementable as
   written. `sequencing_timeout`: because it is not a validity rule (rule 1), safety does not depend on it; it only trades
   reduction latency against fork frequency. The default 5 minutes and maximum 1 hour stand, and a **minimum of 30 seconds**
   is added so a mis-set profile cannot make every member race the coordinator.

## Verified, and not

- **Model.** [`GroupCommit.tla`](../verification/tla/GroupCommit.tla) (3 devices, a faulty coordinator that may go offline or equivocate, two racing reduction
  intents, bounded extra commits) passes under weak fairness: every alive honest member ends on the canonical chain or
  `forked`/`out`, every alive device in the canonical roster is `active`, every reduction is applied or moot, no honest
  self-equivocation. 676,654 distinct states for the main config; five mutations each produce a counterexample.
- **Library.** OpenMLS 0.9.0 does what rules 1-5 need, on native and on wasm32-unknown-unknown under Node with identical
  results; iOS and Android were type-checked only.
- **Not verified.** Ordering by DeviceID is grindable only among same-class Commits, which a member authorized for both could
  already choose between; it gains no authority. Partitions larger than two, more than three devices, and multi-epoch
  forks are modeled only by TLC's bounded exploration. Performance is V-022.

## Residual risk

- A device removed by the winning branch can read the losing branch's traffic until that branch merges. It is bounded by
  the partition and by `sequencing_timeout` plus delivery, and is the price of keeping reductions live (T-159).
- A member authorized for direct reductions can force fork-and-recover cycles by publishing competing valid reductions.
  Each costs one recovery Commit and cannot change authority (T-160).

## Consequences

- R-393..R-396, T-158..T-160 and D-194 record this. GROUP_COMMIT §3.2 states the rules; §5 rule 5 and ADR-0163 §4 rule 6 point here.
- The wire freeze must give `GroupCommitBinding` a `binding_kind` (coordinator-sequenced, direct reduction, recovery) and a
  `reducing` flag so the key is computable from the binding alone.
- V-004 criteria for the formal model, the MLS library and partition convergence now have repo-local evidence. The wire
  freeze, the GroupHistoryBundle freeze and the independent review remain open, so `groups` stays unauthorized.
