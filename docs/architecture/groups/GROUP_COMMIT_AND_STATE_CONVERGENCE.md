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

The exact mechanism remains a blocking question. Candidates may include a group-elected/rotating sequencer with authenticated failover or a deterministic application-level conflict procedure proven to converge. A design is not accepted merely because it is easy to implement.

## 4. Atomic binding to OM group policy

Membership/role-affecting changes use a logical transition that binds application state to the resulting MLS state:

```text
GroupControlTransition {
  group_id
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

Human membership remains account-level unless group policy says otherwise, while MLS leaves are device-level. Adding/removing device leaves must validate current AccountControlState evidence and group policy.

A revoked device cannot regain future group secrets merely because an old KeyPackage/commit remains cached.

## 7. Freeze gate

Private-group architecture is not frozen until:

- GroupCommitOrdering is selected/formalized;
- GroupControlTransition encoding and authorization are specified;
- conflict/rebase tests exist;
- crash/partial-publication behavior is specified;
- AccountControlState-device changes are integrated;
- the selected MLS library is proven to support the required external ordering/profile behavior on all target runtimes.
