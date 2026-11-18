# ADR-0227 — Safety-code replacement bound to its parent and quorum, kit delivery to an existing main device, and what a replaced code promises

- **Status:** Accepted (by operator delegation)
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (constitution-driven rulings: fail closed, user-held keys, recovery optional, simplicity for ordinary users; fill silences only, never amend audited text, never weaken a floor).
- **Class:** `additive_noncritical_extension` (ADR-0165). One new operation digit in the private controller-change ceremony registry, one new in-session message kind, and Core behavior. No frozen W0 object, label or registry value changes meaning.
- **Amends:** nothing. **Does not amend:** ADR-0199 section 5, ADR-0205, ADR-0216 (its operations 0 and 1 and its enablement conditions stand), ADR-0192, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md).
- **Scope:** `core_kernel` (ADR-0169).

## Design sources read, and not read

Read in full: ADR-0199, ADR-0202, ADR-0203, ADR-0205, ADR-0216, ADR-0225, ADR-0226 and the constitution documents on authority and the operatorless core. Consulted by search: ADR-0192, ADR-0028 to ADR-0030, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) and [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md). Not read in full: the controller-change CDDL and the device-link code.

## Decision

### 1. Replacing the safety code is staged against an exact parent and an exact authorization set

1. **Stage.** `ReplaceRecoveryKey` records, with the staged key, kit and policy: the verified head's hash (the **parent**), the recovery epoch it will advance, the controller set at that head, the votes the parent's committed policy requires for a recovery-policy change, and the hash of the exact candidate child state. This is also the anchor of ADR-0225 section 2.
2. **Refuse what cannot be authorized.** If the parent requires more than one controller vote and the ceremony of section 2 below is not available on the device, staging is refused with "needs your other main device" and nothing is shown or stored. A single-controller account stages as today.
3. **Recheck at confirmation.** `ConfirmRecoveryKeyStored` re-reads the current head. If it differs from the staged parent, a newer policy epoch exists, or any incoming recovery request is open, the staged key is dropped, the account is unchanged and the person is told to make a new code (the old code keeps working). The saved sheet is reported usable only after the child state is adopted, not when the person confirms they wrote it down.
4. **The old code's record.** After the child is adopted and the new record is acknowledged, the confirming device deletes the old record at every Home it was written to, using the old write capability with the delete of ADR-0201 (idempotent, exact retry, durable until success or the Home's retention ends). A Home answers a delete and an unknown locator identically, so deletion is housekeeping and is never presented as proof that the old code is dead.

### 2. Votes for a replacement are collected through the private controller-change ceremony

ADR-0216's ceremony registry gains **operation 3, recovery-policy change**, in the same private-link carrier and transcript as operations 0 and 1 (the link prefix `om:controller-change/` followed by operation digit 3). The receiving controller sees the exact transition, "Replace the safety code?", and the code comparison covers the exact candidate child hash, parent hash and epoch. It returns one vote on that child, as for removal. The originating controller holds both signatures only after both people compared the codes; the child is then verified as a successor with the ordinary verifier and adopted. A mismatched code, expiry, cancellation or changed parent ends the exchange with no change. The staged key remains staged for the person to retry until the parent changes (item 3 above) and the old code keeps working throughout.

### 3. A main device that lacks the kit receives it again over the session

ADR-0216 releases the kit once, with the promotion reply, and states that later kit changes use the controller distribution of ADR-0199 section 5. This ADR specifies that path as **repeatable**.

1. **Debt.** After a promotion commits, and after every kit change (a replacement), the originating device holds a durable **kit delivery debt** for each other eligible controller: the target DeviceID, the kit epoch and the kit digest. The debt is held until that target returns an authenticated `RECOVERY_KIT_ACK` for that digest.
2. **Carrier.** The kit is sent in the in-session kind `RECOVERY_KIT` (ADR-0199 section 5) inside the authenticated sibling session, never in a ceremony, and not as a state-bearing step of the ACS; the sibling session already carries what a target needs and is not bound to the head that was current at promotion. Sending repeats on the ordinary sibling sync tick until acknowledged.
3. **Receiver checks.** Before storing the kit in DeviceVault the target checks that it is an eligible controller in its own verified current head, that the kit's epoch equals the epoch of the policy that head commits (a kit of an earlier epoch is dropped and not acknowledged) and that the committed policy hash matches. A kit that fails any check is discarded without effect.
4. **Lifecycle.** A debt ends on acknowledgement, when the target is no longer an eligible controller in the current verified head, or when a newer kit epoch supersedes it (the new epoch has its own debt). A debt open for 30 days raises "main device needs setup" on the originating device and stays; it is never dropped by time alone.
5. **No rollback.** A published account state is never rolled back because a kit did not arrive; a main device without the kit is not a record writer and writes nothing (ADR-0199 section 5).
6. **Enablement conditions** of ADR-0216 (fork detection, a request reaching a second controller, the exclusion guard, the shared vector) remain prerequisites for turning kit distribution on.

### 4. What a replaced safety code promises

1. **Authority.** The replacement transition, signed under the committed policy, and the pins of devices and contacts that learned of it are the authority. After adoption, a controller on the new policy refuses a request signed under the old key, and any verifier that has pinned the new head refuses a successor that descends from the old parent.
2. **Wording.** The product promise is: "your old code stops working on your devices and for contacts that have heard about the change". It does not say the old code stops working everywhere or immediately: a device that has not yet learned of the change, or is partitioned, can still start a waiting recovery with the old code, which the devices that know refuse (ADR-0202 section 1: there is no proof of global freshness during a partition).
3. **No new terminal signal at the old place.** The delete of section 1 item 4 is the only action at the old locator; no state is written there that a reader could distinguish from "not found". Controllers keep draining the earlier epoch's inbox for alerts (ADR-0226 section 1 item 3), so a use of the old code still produces an alert.
4. **Test required.** An old-code attempt that runs the strict timer to completion on a fresh profile, with the owner's surviving controller on the new policy and a contact that pinned the replacement: the controller and the contact refuse, and the fresh profile is left restricted or with an unaccepted fork, never an adopted account.

## Where did authority go?

Nowhere new. The parent policy decides each vote; the ceremony authenticates two devices and one exact transition; the sibling session already authenticates a kit delivery; a kit gives access to the record and the inbox, never a vote or a recovery signature.

## Evidence required

Core: a second controller's vote through operation 3, an intervening head change, an incoming request between stage and confirmation, refusal when a quorum is needed and the ceremony is absent, the old record's delete retried after restart; a lost promotion reply with later head change followed by a redelivered kit, a stale-epoch kit refused, a removed target ending its debt; the old-code test of section 4. Vectors for operation 3 in both codecs.

## Review requirement

The new operation touches the account's authority path (a recovery-policy change by two controllers). It needs the same specialist review ADR-0216 names before the quorum ceremony is turned on.

## Traces

Related: ADR-0192, ADR-0199, ADR-0201, ADR-0202, ADR-0205, ADR-0216, ADR-0225, ADR-0226.
