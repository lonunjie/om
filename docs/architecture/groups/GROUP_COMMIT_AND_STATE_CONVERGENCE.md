# Group Commit and State Convergence

- **Status:** Canonical architecture direction; sequencing profile blocks group freeze
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

Exact encoding is deferred, but the security property is not: a client must not commit the application membership change while failing to bind/complete the corresponding MLS transition, or vice versa.

## 5. Crash/retry/idempotency

The transition protocol must specify:

- crash after local commit creation but before publication;
- publication acknowledged by one relay but not another;
- duplicate commit delivery;
- receipt of a losing conflicting commit;
- rebase of pending application operations;
- device removal racing with an admin role change;
- recovery after long offline periods.

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

Private-group architecture is not frozen until:

- GroupCommitOrdering is selected/formalized;
- GroupControlTransition encoding and authorization are specified;
- conflict/rebase tests exist;
- crash/partial-publication behavior is specified;
- AccountControlState-device changes are integrated;
- the selected MLS library is proven to support the required external ordering/profile behavior on all target runtimes;
- reductions are shown to stay live under coordinator failure or censorship (§3.1).

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0121](../../../decisions/ADR-0121-event-ordering-authorization-epoch-and-materialization.md) rule 4: `past_epoch_window` is a GroupProfile parameter (a count of epochs, default 2, maximum 8). Epoch secrets are deleted when the window moves. An undecryptable late event renders as an explicit gap. Divergence bounded by the window is a documented residual risk.
