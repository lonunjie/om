# Group Commit and State Convergence

- **Status:** Canonical architecture; W1 wire/runtime verification still blocks `groups` authorization
- **Checkpoint:** 2026-09-23 05:45 PHT

## 1. Separation

OM private-group security uses MLS, while OM group policy represents human/account-level roles and permissions. These two state machines must advance coherently.

```text
OM GroupControlState
        <->
MLS group epoch/tree
```

Neither is allowed to silently outrun the other for membership/role-affecting operations.

## 2. Commit conflict problem

Two authorized members can create valid MLS Commits from the same epoch while offline/partitioned. MLS requires the application to establish a mechanism for resolving conflicting Commits for an epoch.

OM therefore defines a **GroupCommitOrdering profile** rather than assuming the relay or OpenMLS library provides canonical global ordering.

## 3. Required properties of the ordering profile

It must:

- converge on one accepted successor for each canonical group epoch/branch;
- reject/rebase losing proposals/commits safely;
- work with replaceable relays and intermittent connectivity;
- not make one infrastructure operator permanent group authority;
- preserve evidence sufficient to diagnose forks;
- specify timeout/failover behavior if any temporary sequencer is used;
- resist replay and stale-epoch acceptance;
- define partition merge behavior.

The 2026-09-23 architecture resolution selected a **policy-neutral member CommitCoordinator** plus authenticated `GroupControlIntent` / `GroupCommitBinding`. The coordinator serializes same-epoch application/MLS transitions but does not receive constitutional authority merely by coordinating. Replacement/equivocation, crash and partition behavior remain verification gate V-004; the mechanism itself is no longer an open architecture choice.

### 3.1 Coordinator liveness and reductions (ADR-0128 rule 1)

The coordinator is operational, never authority. A coordinator that is offline, lost or malicious must not be able to
delay a security reduction indefinitely, so the following rules are normative:

1. `GroupControlState` records a coordinator **succession order** (a list of member devices), changed only under
   `room.manage_governance`.
2. `sequencing_timeout` is a GroupProfile parameter: a monotonic local duration, default 5 minutes, maximum 1 hour
   (calibrated by V-004). A member starts it when it publishes or first observes a pending intent.
3. **Reducing intents** are: removing a revoked or quarantined device (§6.1), removing a member, a self-leave,
   removing a service or bridge endpoint, and a Space-side reduction carried by a `SpaceDelegatedIntent`
   ([`SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md`](SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md)). If the coordinator has not sequenced a pending reducing intent
   within `sequencing_timeout`, any member holding the capability that authorizes it may commit it directly. Removing
   a revoked or quarantined device needs no capability: any member may commit it.
4. The same member may commit the coordinator's replacement. The replacement is always the next device in the
   succession order that is still a member, so the choice is deterministic and nobody picks a coordinator.
5. The coordinator never sequences its own removal or its own replacement.
6. Reductions are monotone (ADR-0122 rule 1): a directly committed reduction that loses a same-epoch race to another
   Commit is rebased onto the winner and stays applicable while control is frozen.
7. A rebased **non-reducing** intent is re-validated against its new parent state and dropped if it fails there. A
   demoted admin's pending operation therefore cannot land after the demotion.

If every member able to authorize a reduction is gone, a self-leave and the removal of revoked or quarantined devices
still proceed, because they need no capability beyond membership.

### 3.2 Same-parent ordering, fork detection and recovery (ADR-0173)

ADR-0173 is normative; this is its summary. V-004 verified it with [`GroupCommit.tla`](../../../verification/tla/GroupCommit.tla) and an OpenMLS probe.

1. A Commit's **parent** is `(GroupControlState hash, MLS epoch)`. It is *valid* if it is authorized at that parent; a direct
   Commit must be a reduction or a recovery. Receivers never check the author's `sequencing_timeout`.
2. Among valid Commits sharing a parent, the canonical successor is the minimum of `(non-reducing?, direct?, committer
   DeviceID, GroupCommitBinding hash)`. A reduction beats a non-reduction, a coordinator-sequenced Commit beats a direct one,
   and recovery sorts last. No receipt order, relay order or clock is involved.
3. A member merges the best valid Commit it has seen at once. A merged epoch cannot be rolled back, so a better Commit that
   arrives later leaves the member on a **losing branch**: it marks the branch `SUPERSEDED`, sends nothing on it, re-queues
   the ConversationEvents it sent there that the winning branch does not hold (idempotent by EventID), and rebases or
   revalidates its own intent (§3.1 rules 6-7).
4. **Recovery** is a roster-neutral Commit (`GroupCommitBinding` kind `recovery`, same GroupControlState hash) by any active
   member of the winning chain: an MLS re-add (fork-resolution) or the device's external commit. It re-adds only devices
   in the canonical roster. A receiver refuses an external commit whose joiner credential is not in the canonical roster
   before merging it.
5. A removal merged only on a losing branch is not a removal; the device stays `forked` and recovers by rule 4.
6. `sequencing_timeout` has a minimum of 30 seconds (default 5 minutes, maximum 1 hour).

## 4. Atomic binding to OM group policy

Membership/role-affecting changes use a `GroupControlIntent` (what is requested, and its authorization) and a
`GroupCommitBinding` (the MLS Commit that carries it). `GroupControlTransition` is the logical union of the pair:

```text
GroupControlTransition {
  conversation_id
  previous_group_state_hash
  previous_mls_epoch
  operation
  authorization
  mls_commit_hash
  resulting_mls_epoch
  resulting_group_state_hash
}
```

This is a later-wave group object rather than a frozen W0 wire object. Its exact encoding is owned by the groups freeze/conformance work before that scope can authorize; the architecture property is already fixed: a client must not commit the application membership change while failing to bind/complete the corresponding MLS transition, or vice versa.

## 5. Crash/retry/idempotency

ADR-0163 closes the local transaction semantics for membership/role-affecting transitions without freezing the later-wave wire encoding. A conforming implementation keeps a durable local transition journal:

```text
INTENT_DURABLE
  -> CANDIDATE_PREPARED
  -> PUBLISHED_UNCONFIRMED
  -> ACCEPTED
       \-> SUPERSEDED | ABANDONED
```

Rules:

1. Persist the authenticated `GroupControlIntent`, immutable target incarnation, authorization evidence and parent `(GroupControlState hash, MLS epoch)` before creating or publishing a Commit.
2. Before network publication, persist the **exact MLS Commit bytes/hash**, `GroupCommitBinding`, parent identifiers and enough tentative post-commit MLS/application state to resume or safely discard the candidate after restart. Tentative state is isolated from canonical Room state.
3. A relay/storage acknowledgement is transport progress only; it is never canonical group acceptance. If publication outcome is uncertain and the recorded parent is still canonical, retry the exact prepared commit/binding rather than generating a second same-parent semantic mutation merely because an acknowledgement was lost.
4. Acceptance under the GroupCommitOrdering profile atomically promotes MLS state, GroupControlState/application authority state, the canonical group head and the accepted-commit/dedupe marker. If one local transaction cannot span those stores, a durable recovery journal must complete the same promotion before application release/ack or further application-message sending.
5. No application message is sent under a tentative membership/control branch before that branch's Commit is accepted as canonical. "Tentative" means prepared but not yet merged (ADR-0173 rule 2): a merged Commit can still lose a same-parent race, and §3.2 governs what the member then does.
6. Duplicate observation of an accepted Commit/Binding is idempotent and never repeats application/control effects.
7. A losing same-epoch candidate becomes `SUPERSEDED`; its tentative state is never materialized. A still-applicable reducing intent is rebased per §3.1. A non-reducing intent is re-validated against the new parent and may be dropped.
8. After a long offline period or restart with a stale/unknown parent, reconcile the authenticated canonical group/control chain first. A prepared candidate whose parent is no longer canonical is superseded/abandoned before any new candidate is built.
9. If the exact prepared state required for safe retry is missing/corrupt after crash, fail closed: reconcile the canonical head and create a fresh authorized candidate only if the underlying intent remains valid. Do not reconstruct supposedly equivalent Commit bytes from partial state.
10. Device-removal/admin-role races are resolved by the existing parent-state authorization rule: reductions remain live/rebaseable; non-reducing operations are revalidated at their accepted parent.

V-004 still proves these rules under crash/partition/equivocation and validates the chosen MLS implementation/runtime behavior. It also freezes the W1 group wire objects before authorization; this section does not claim that evidence is complete.

## 6. New/revoked devices

Human membership remains account-level unless Room policy says otherwise, while MLS leaves are device-level. Adding/removing device leaves must validate current AccountControlState evidence and group policy.

A revoked device cannot regain future group secrets merely because an old KeyPackage/commit remains cached.

### 6.1 Account-control events (ADR-0128 rule 2)

1. When a member learns, at a validated AccountControlState head, that a device is revoked or quarantined, it MUST
   NOT send further application messages into the Room until a Commit removing that leaf is accepted. It proposes the
   Remove itself; §3.1 makes the Remove live.
2. Quarantine removes the leaf (this is what [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) §4 Stage A relies on). Lifting the
   quarantine re-adds the device only through a fresh add backed by a later authorized head.
3. While an account is in CONTROL_CONFLICT or SAFETY_FREEZE, none of its devices is added to any group. Removals
   still proceed.
4. Automatic new-device adds are GroupControlIntents deduplicated by `(AccountID, device_id, ACS generation)`, so
   several members racing to add the same device produce one add.

## 7. Freeze gate

ADR-0156 makes the encoding item enforceable: V-004 now requires frozen CDDL, registered fields/labels and two-codec vectors for GroupControlState, GroupControlIntent, GroupCommitBinding/GroupControlTransition and the pin/poll event bodies before `groups` can be authorized.

Before `groups` may be authorized, V-004 must still provide the evidence/wire freeze that exercises this already-selected architecture:

- formalize/model GroupCommitOrdering replacement/equivocation and conflict/rebase behavior;
- freeze GroupControlState/Intent/Binding/Transition and related group event bodies as CDDL + registries + two-codec vectors;
- exercise ADR-0163 crash/partial-publication/restart semantics under partition;
- prove AccountControlState-device changes and reduction liveness under coordinator failure/censorship;
- demonstrate the selected MLS library supports the required external ordering/profile behavior on target runtimes.

These are verification/encoding obligations, not permission for an implementation to choose different crash or authority semantics.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0121](../../../decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md) rule 4: `past_epoch_window` is a GroupProfile parameter (a count of epochs, default 2, maximum 8). Epoch secrets are deleted when the window moves. An undecryptable late event renders as an explicit gap. Divergence bounded by the window is a documented residual risk.
