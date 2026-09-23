# Account Continuity and Control

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-21 21:32 PHT
- **Purpose:** Define account authority, device enrollment, quarantine/revocation, fork handling, anti-rollback, and recovery authorization without making one ordinary device or provider an account monarch.

## 1. Core distinction

OM separates three planes that conventional messengers often blur:

1. **message/data plane** — receives/decrypts conversation content;
2. **device-control plane** — authorizes bounded account-management actions;
3. **constitutional plane** — changes who is allowed to exercise control, changes recovery policy, or changes security floors.

A device can participate in messaging without necessarily being a constitutional controller.

> **Linking a device is not the same as giving that device constitutional authority over the account.**

This closes a takeover path in which one compromised device adds attacker-controlled devices, manufactures a quorum, and removes every legitimate device.

## 2. AccountControlState

Security/control state is **not** a CRDT and is **not** ordered by chat HLC timestamps. It is an authenticated, monotonic hash chain.

Conceptual state:

```text
AccountControlState {
  account_id
  generation                 // monotonically increasing
  previous_state_hash
  active_control_devices[]
  quarantined_control_devices[]
  messaging_devices[]
  permanently_revoked_devices[]
  constitutional_policy      // threshold/rule over pre-change controllers
  recovery_policy_commitment
  minimum_crypto_profile
  control_key_epochs
  optional transparency/witness commitments
  authorization_signatures[]
}
```

`generation` is only meaningful together with the authenticated hash chain and valid authorization. A larger number alone has no authority.

### Security rule

A client MUST NOT accept a state that:

- does not descend from the latest trusted head it knows;
- violates the authorization rule of the **parent** state;
- lowers an authenticated crypto/security floor without an explicitly permitted emergency migration procedure;
- silently removes enough controllers to make a previously unauthorized coalition authoritative.

## 3. Device classes

### Messaging device

May send/receive according to its granted conversation capabilities. It need not participate in constitutional account governance.

### Control device

Holds a control credential and may participate in quarantine/revocation/account-state authorization.

### Recovery authority

Exists only when the user configured a `RecoveryPolicy`. It is logically above ordinary control-device disputes but does not automatically decrypt history.

A product may make the user's first device both a messaging and control device for usability. Additional devices can initially be messaging-capable and optionally be promoted to controller status under constitutional rules.

## 4. Two-stage revocation

### Stage A — emergency quarantine

Goal: stop a suspected device from receiving **future secrets** quickly without letting a single compromised controller permanently rewrite ownership.

A valid emergency quarantine can cause compliant peers/providers to stop giving the target:

- new 1:1 ratchet/session material where controllable;
- new MLS group key material after removal/rekey;
- newly rotated RelaySet/private-delivery capabilities;
- new backup/recovery material;
- new account-control secrets.

A quarantine **does not erase** data the target already learned.

### Stage B — permanent revocation

Permanent revocation requires stronger authority:

- the constitutional threshold defined by the **pre-change** AccountControlState; or
- a valid configured RecoveryPolicy authorization.

Permanent revocation can finalize key rotations, device removal, future membership, and control-set changes.

### Unilateral-quarantine abuse guard

A compromised controller must not be able to quarantine every other controller serially and thereby manufacture sole authority.

OM therefore uses these rules:

1. quarantine does **not** remove the target from the constitutional denominator;
2. one controller cannot use quarantines it initiated to satisfy a quorum;
3. promotion of a new control device uses the parent state's controller set/recovery authority;
4. multiple unilateral quarantines that would disable a configured safety threshold trigger a **safety freeze** and require stronger authorization;
5. contested quarantine cannot be resolved by last-write-wins.

Implementations may choose a bounded challenge window for UX, but an uncontested quarantine without quorum may remain quarantined indefinitely rather than becoming permanent solely because time passed.

## 5. Contest and safety freeze

A quarantined control device retains a narrow, control-only **challenge capability** for the dispute window/state. It does not continue receiving ordinary conversation secrets.

If conflicting valid control actions occur—for example A quarantines B while B quarantines A—OM enters a safe conflict state:

```text
NORMAL -> CONTROL_CONFLICT -> SAFETY_FREEZE
```

While frozen:

- ordinary existing messaging may continue where keys remain valid and policy permits;
- no contested controller promotion/removal becomes final;
- no recovery-policy replacement becomes final;
- no security floor is lowered;
- resolution requires a valid constitutional quorum or RecoveryPolicy.

There is no last-write-wins authority rule.

## 6. Device enrollment and promotion

### Link messaging device

A currently authorized control device may authorize a new messaging device using an authenticated, short-lived linking ceremony. Product policy may require a second confirmation for sensitive accounts.

### Promote to control device

Promotion changes the future takeover threshold and is therefore constitutional. It MUST be authorized under the current parent state's constitutional policy or RecoveryPolicy.

A newly linked device cannot immediately vote itself or attacker-linked devices into control.

## 7. AccountControlHead gossip

A provider can replay an old-but-valid state. To detect this without creating a global account registry, OM exposes a compact opaque head:

```text
AccountControlHead {
  account_id_or_relationship_scoped_subject
  generation
  state_hash
  epoch
  signature
}
```

Established contacts, user's other relays/devices, optional witnesses, or recovery guardians may remember heads. They do not need account contents or device lists.

Normal messages may piggyback a relationship-scoped assertion of the current control generation/hash. A restored device that sees a lower state than several previously seen heads knows it is being rolled back or partitioned.

This is detection, not external authority: witnesses cannot create a valid successor state.

## 8. Fork handling

Concurrent constitutional changes can produce two valid descendants of one state. OM treats this as a security fork, not a mergeable chat conflict.

Resolution requires a **bridge state** authorized strongly enough under the last common trusted state or a valid RecoveryPolicy. Clients preserve evidence of both branches for diagnosis/audit and refuse to silently choose the numerically larger generation.

## 9. Edge cases

### One-device identity, no recovery (explicit opt-out only since ADR-0111)

Recovery is on by default ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10), so this case arises only when the user explicitly turned recovery off. If that device is lost: identity is unrecoverable. If it is compromised: the attacker controls the account. This is an explicit consequence of the user's disposable/no-recovery policy; no protocol can infer the original human after all independent evidence is gone.

### Two controllers, one lost

With default recovery, the survivor plus the RecoveryRoot form a majority and reconstitute the controller set immediately ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.4). Without recovery: the survivor can quarantine the lost controller. If the constitutional rule requires two controllers and no RecoveryPolicy exists, the lost controller may remain quarantined indefinitely and constitutional changes may remain unavailable. The product can warn before users choose such a policy.

### Offline controller returns after months

It must fetch/verify the AccountControlState chain or enough authenticated checkpoints to reach the current head before exercising control authority. Old chat history does not confer old control authority.

### Restored backup contains old control keys

Those keys are treated as historical material until the current AccountControlHead/state is reconciled. A backup cannot resurrect revoked authority.

## 10. Implementation gate

Before production claims, this design requires:

- formal authorization-state transition table;
- golden vectors for valid/invalid state transitions;
- fork and rollback tests;
- compromised-controller simulations;
- enrollment/promotion tests;
- recovery override tests;
- independent security review.

## 2026-09-23 AccountControl resolution

Default controller authorization is majority `floor(N/2)+1`. Temporary quarantine cannot shrink that denominator or permanently revoke/promote/change threshold. Competing valid descendants trigger safety freeze rather than timestamp/LWW choice. Relationship-scoped head gossip detects equivocation; fork resolution explicitly acknowledges branches under common-ancestor authority. `V-002` formally validates these invariants.

## 11. ContinuityAssessment (ADR-0106, default changed by ADR-0111)

Most people have one phone, or one phone plus one laptop. Under the majority rule, those are the configurations where
one lost device used to mean identity loss or a permanent constitutional lockout. Since ADR-0111, identity recovery is
**on by default**, and the RecoveryRoot counts as one vote ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.4):

| Controllers | Recovery | Lost devices before identity loss | Lost devices before constitution lock | Takeover requires |
|---|---|---|---|---|
| 1 | **default (on)** | never while the custodian survives (72 h delay) | never | 1 device, or custodian + 72 h with no live device vetoing |
| 2 | **default (on)** | never while the custodian survives | never (survivor + recovery = majority) | 2 devices, or 1 device + custodian, or custodian + delay without veto |
| 3 | **default (on)** | never while the custodian survives | never | 2 devices, or 1 device + custodian, or custodian + delay without veto |
| 1 | off (explicit) | **1: identity lost** | 1 | 1 device |
| 2 | off (explicit) | 2 | **1: constitution locked** | 2 devices |
| any | + guardians M-of-N | adds a method that survives device and custodian loss | — | adds: M guardians colluding + delay without veto |

[`verification/bounded/continuity_assessment.py`](../../../verification/bounded/continuity_assessment.py) reproduces this table. The TLA+ model `AccountControl.tla` checks the activation, veto and one-vote rules.

Core MUST compute a local, non-authoritative **ContinuityAssessment** from the current AccountControlState and RecoveryPolicy:

```text
ContinuityAssessment {
  lost_devices_to_identity_loss      // null = unbounded while a recovery custodian survives
  lost_devices_to_constitution_lock
  takeover_paths[]                   // e.g. ["2 devices", "1 device + custodian", "custodian + 72h without veto"]
  recovery: on(custodians[]) | pending | off_explicit
  acknowledged_by_user_at?           // local only; needed only for off_explicit and single-device custody
}
```

Rules (R-165, amended by R-183..R-186):

1. Identity recovery is **on** by default (ADR-0111). A default account needs no acknowledgement.
2. The persistent, non-blocking continuity check appears only when recovery is **pending**, **explicitly off**, or when the only custodian lives on the only controller device.
3. Turning recovery off shows the consequence in plain language and needs a one-time confirmation. The confirmation is local UX state, never protocol state, and is re-requested when the assessment worsens.
4. The assessment never grants authority and is never sent to providers.
5. Wording is human: "Recovery is on. It's saved with your passkeys." / "Save a recovery key (print it or keep it in your password manager)."

## 12. 2026-09-24 retirement and device-link visibility

- Account closure, deletion, moved-account pointers and LegacyPolicy are defined in [`ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md`](ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md) (terminal `AccountRetirement` transition; V-002 extended).
- Every device enrollment is announced on the account's other active devices (R-166, [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2). A linking ceremony that finalizes without that notice is a failing trace.
