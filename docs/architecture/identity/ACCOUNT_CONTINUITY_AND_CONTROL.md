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

- neither descends from the latest trusted head it knows nor is a valid fork bridge state whose `om.acs.fork_ack`
  covers that head (§8);
- violates the authorization rule of the **parent** state (§4.1);
- lowers the crypto/security floor. The floor compares the `security_generation` of CryptoProfiles, not profile ids.
  The only non-increase is a SecurityAdvisory marking a profile `forbidden`, after which the floor moves to a
  non-forbidden profile of equal or higher generation, never lower ([`CRYPTOGRAPHY_AND_AGILITY.md`](../crypto/CRYPTOGRAPHY_AND_AGILITY.md), ADR-0126);
- silently removes enough controllers to make a previously unauthorized coalition authoritative.

## 3. Device classes

### Messaging device

May send/receive according to its granted conversation capabilities. It need not participate in constitutional account governance.

### Control device

Holds a control credential and may participate in quarantine/revocation/account-state authorization.

### Recovery authority

Exists when a `RecoveryPolicy` is on, which is the default (ADR-0111). It counts as **one vote** toward the parent
state's majority ([`RECOVERY_AND_BACKUP_POLICY.md`](RECOVERY_AND_BACKUP_POLICY.md) §10.4). It counts immediately only with at least one current,
non-quarantined controller co-signing. Alone, it acts only after the activation delay with no RecoveryVeto (§10.3).
It never resolves a freeze or fork, and it does not decrypt history. This "recovery vote" rule (ADR-0127) applies
wherever this document mentions a RecoveryPolicy.

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

A quarantined device also can no longer **issue** anything peers act on: peers reject RelaySets, BlobLocationRecords
and IntroIdentityBindings signed by a device that the current state lists as quarantined ([`MULTI_RELAY_AND_DELIVERY_RESILIENCE.md`](../delivery/MULTI_RELAY_AND_DELIVERY_RESILIENCE.md) §2.1).
How a quarantine or revocation reaches groups (members stop sending until the Remove commits) is in
[`GROUP_COMMIT_AND_STATE_CONVERGENCE.md`](../groups/GROUP_COMMIT_AND_STATE_CONVERGENCE.md) §6.1 (ADR-0128 rule 2).

A quarantine **does not erase** data the target already learned.

### Stage B — permanent revocation

Permanent revocation requires stronger authority:

- the constitutional threshold defined by the **pre-change** AccountControlState; or
- that threshold reached with a recovery vote (§3): immediately only with at least one current, non-quarantined
  controller co-signing, otherwise only after the activation delay with no RecoveryVeto.

Permanent revocation can finalize key rotations, device removal, future membership, and control-set changes.

### Unilateral-quarantine abuse guard

A compromised controller must not be able to quarantine every other controller serially and thereby manufacture sole authority.

OM therefore uses these rules:

1. quarantine does **not** remove the target from the constitutional denominator;
2. only active, non-quarantined controllers' signatures count toward a quorum, so quarantining other controllers
   never helps the quarantining controller reach one. This rule is derived from the committed parent state alone:
   no verifier needs, or may use, a record of who initiated a quarantine (ADR-0136 rule 3). The one exception is §5:
   a quarantined controller's challenge signature counts toward resolving a safety freeze;
3. promotion of a new control device uses the parent state's controller set/recovery authority;
4. a quarantine that would leave fewer non-quarantined controllers than the parent state's majority (that is, one
   that "disables the safety threshold") triggers a **safety freeze**, which only a device-only majority resolves (§5);
5. contested quarantine cannot be resolved by last-write-wins.

Implementations may choose a bounded challenge window for UX, but an uncontested quarantine without quorum may remain quarantined indefinitely rather than becoming permanent solely because time passed.

### 4.1 Authorization table (normative, ADR-0127)

The AccountControlState body does not record a transition type, so a verifier derives each **delta** by diffing the
child against its parent. Signers are counted against the **parent** state. "Majority" is `floor(N/2)+1` of the
parent's `active_control_devices`, with quarantined controllers still in N (§4 rule 1). "Recovery vote" follows §3.
A child that combines several deltas needs the strongest row among them. A delta not listed here is rejected.

| Delta (child vs parent) | Required signers (role `controller` unless noted) | Single signer allowed? |
|---|---|---|
| link a messaging device (add to `messaging_devices`, add its credential) | 1 active, non-quarantined controller, after the linking ceremony and enrollment notice (§12) | yes |
| promote to control device (add to `active_control_devices`) | majority, or majority reached with a recovery vote | only when N = 1 |
| quarantine a control device (add to `quarantined_control_devices`) | 1 active, non-quarantined controller other than the target; never the last voting controller (§13) | yes |
| lift a quarantine (remove from `quarantined_control_devices`) | majority of active, non-quarantined controllers (quarantined controllers stay in N, §4 rules 1–2); no recovery vote. Under a safety freeze, §5 applies instead | only when N = 1 |
| remove a messaging device (drop from `messaging_devices`, add to `permanently_revoked_devices`) | 1 active, non-quarantined controller | yes |
| permanently revoke a control device | majority, or majority reached with a recovery vote | only when N = 1 |
| change the recovery policy (`recovery_policy_commitment`) | majority, or majority reached with a recovery vote | only when N = 1 |
| change `constitutional_policy` | majority | only when N = 1 |
| raise `minimum_crypto_profile` (higher `security_generation`) | 1 active, non-quarantined controller | yes |
| lower `minimum_crypto_profile` | forbidden (the SecurityAdvisory case in §2 moves to an equal or higher generation) | — |
| rotate the signer's own `control_key_epochs` / `device_credentials` entry (same device id) | that device, not quarantined, signing with its current key | yes |
| change `transparency_commitment` | 1 active, non-quarantined controller | yes |
| fork bridge (§8) | device-only majority of the common ancestor's controllers; no recovery vote | only when N = 1 |
| `AccountRetirement` | majority, or majority reached with a recovery vote (delay and veto apply when recovery acts alone) | only when N = 1 |

Messaging devices are removed, never quarantined; the ACS has no field for it. Removal is reversible only by linking
the device again. A device's revocation is recorded only in the ACS (`permanently_revoked_devices`), never as a
timestamp on the device record. V-002 mirrors this table.
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
- resolution requires a **device-only** majority of the parent state's controllers (the TLA `ResolveFreeze`); a
  recovery vote never resolves a freeze. Quarantined controllers are part of that majority and sign with their
  challenge capability, so one compromised controller cannot make a freeze permanent by quarantining the others
  (ADR-0136 rule 2). A freeze entered by a quarantine is resolved by a child that lifts the quarantines; a fork is
  resolved by the §8 fork bridge.

There is no last-write-wins authority rule.

## 6. Device enrollment and promotion

### Link messaging device

A currently authorized, non-quarantined control device may authorize a new messaging device using an authenticated, short-lived linking ceremony (§4.1). A messaging device cannot link another device. Product policy may require a second confirmation for sensitive accounts.

### Promote to control device

Promotion changes the future takeover threshold and is therefore constitutional. It MUST be authorized under the current parent state's constitutional policy, where a RecoveryPolicy counts only as one recovery vote (§3, §4.1).

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

Resolution requires a **bridge state** (ADR-0127):

- its `previous_state_hash` is the last common ancestor of the branches;
- it carries the critical key-23 extension `om.acs.fork_ack`, listing the digest of every observed branch head
  ([`account.cddl`](../../../spec/cddl/account.cddl) `fork-ack`, ADR-0126);
- it is authorized by a **device-only** majority of the common ancestor's controllers. A recovery vote does not count.

A client that trusted a branch head accepts the bridge only if `fork_ack` covers that head (§2). A later branch head
that the bridge does not cover is a new fork. Clients preserve evidence of both branches for diagnosis/audit and refuse
to silently choose the numerically larger generation.

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

- formal authorization-state transition table (§4.1);
- golden vectors for valid/invalid state transitions;
- fork and rollback tests;
- compromised-controller simulations;
- enrollment/promotion tests;
- recovery override tests;
- independent security review.

## 2026-09-23 AccountControl resolution

Default controller authorization is majority `floor(N/2)+1`. Temporary quarantine cannot shrink that denominator or permanently revoke/promote/change threshold. Competing valid descendants trigger safety freeze rather than timestamp/LWW choice. Relationship-scoped head gossip detects equivocation; fork resolution explicitly acknowledges branches under common-ancestor authority (bridge state with `om.acs.fork_ack`, §8). `V-002` formally validates these invariants.

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

[`verification/bounded/continuity_assessment.py`](../../../verification/bounded/continuity_assessment.py) reproduces this table. The TLA+ model [`AccountControl.tla`](../../../verification/tla/AccountControl.tla) checks the activation, veto and one-vote rules.

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

## 13. 2026-09-24 invariants found by the V-002 model

The TLC model [`verification/tla/AccountControl.tla`](../../../verification/tla/AccountControl.tla) found two gaps in the prose rules. Both are now normative:

1. **Never leave zero voting controllers.** No transition (revocation, quarantine or recovery) may produce a state in
   which every remaining controller is quarantined. Otherwise nobody can vote, and a custodian holder wins by waiting
   out the recovery delay (trace: link h2, quarantine h1, revoke h2).
2. **Recovery targets stay fresh.** A pending recovery request is bound to its target device key and recovery epoch.
   Completion is rejected if that device was enrolled or revoked by another transition in the meantime.

## Recovery activation observation

ADR-0131 makes recovery-alone activation withholding-resistant. Every verifier that has not already observed the
RecoveryRequest for the full policy delay treats the successor as pending, even if another peer already accepted it.
There is no global trusted recovery clock. Controller co-signature is the only v1 immediate path.
