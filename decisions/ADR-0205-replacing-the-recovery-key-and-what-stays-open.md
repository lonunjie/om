# ADR-0205 — Replacing the recovery key: accounts without a kit, a newer sheet checkpoint, and what stays open

- **Status:** Accepted by delegation, 2026-11-18 (fills silence; amends no audited text). Independent review of the composition remains a production gate.
- **Class:** `additive_noncritical_extension` under ADR-0165. One new intent; no new wire object (the new policy is the existing `RecoveryPolicy` committed through the existing control-state transition).
- **Scope:** ADR-0199 section 5 (last bullet: an account with no kit becomes recoverable through a `recovery_policy` change that commits a new RecoveryRoot and issues a new sheet), ADR-0203 (the sheet's owner checkpoint), ADR-0026 and ADR-0027 (recovery is a choice; keys are separate). Items 1 and 2 below record what is not built and why.

## Design sources read

Read in full: the design gate in [`AGENTS.md`](../AGENTS.md); ADR-0199 (sections 1 to 7); ADR-0202; ADR-0203 (sections 1 to 3); ADR-0026; ADR-0027. Read in part: ADR-0184 (the backup file and read-only history), `control.rs` (`verify_successor`, `authorize` by use). Not read: the rest of the identity and recovery documents the design gate lists ([`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) and others), [`ONE_TO_ONE_SESSION_MANAGEMENT.md`](../docs/architecture/crypto/ONE_TO_ONE_SESSION_MANAGEMENT.md) beyond the parts quoted by ADR-0199 and ADR-0190, and the constitution documents.

## Decision

1. **The RecoveryRoot is never kept, so a sheet cannot be refreshed in place and an account without a kit cannot be given one.** Both need the same thing: a new RecoveryRoot. `ReplaceRecoveryKey` makes it, and it is the only path for (a) an account made while its Home offered no recovery place, or before places existed, and (b) a sheet whose checkpoint is old. There is no separate "refresh" feature.
2. **Two steps, so that abandoning leaves the old key working.** `ReplaceRecoveryKey` (a controller device only; refused while any incoming recovery request is open) stages the new root's `RecoveryPolicy` (epoch one above the current one, or 0 when the account has none) and the kit for the Home it already uses (else the place the Home offers now), and shows the new key once, as a sheet with the head the device verified as its owner checkpoint, or as the key alone when there is no place. **Nothing is in force.** Only `ConfirmRecoveryKeyStored` for that operation signs a successor of the current head that commits the new policy (`recovery_commitment`), verifies it as a successor, appends it, swaps the kit, resets the record writer to a new lineage, writes the record under the new kit and tells contacts the new head. A newer ask replaces an unconfirmed one; a failure drops the staged key and leaves the account as it was.
3. **The old key stops working at the confirmation, not before.** After the transition the account commits to the new method key, so a request signed under the old root is not accepted by the controller (tested). The old record stays at the Home until its retention ends (the Home's `DELETE` is not called yet); it carries only what the old key could already read.
4. **User path.** Settings > Recovery > "Make a new safety code", then the same write-down and check as at first run. The explanation says the current code keeps working until the new one is confirmed.
5. **Staged key at rest.** Until confirmed, the staged key, policy and kit are held exactly as the first-run key is: in the profile's encrypted store, shown only through the display-once query and dropped at confirmation or when replaced. They survive a restart, so a person who was interrupted can finish.

## What stays open, and why (D-numbers in the relay-status report)

- **Kit distribution to other controller devices.** In this build every linked device is a messaging device, and a messaging device neither approves links nor signs control states, so no second controller exists to receive a kit. ADR-0199 section 5's in-session `RECOVERY_KIT` kind has no recipient until a device can be promoted to controller. The ceremony carries no kit today. When promotion exists, the kit rides the promotion approval as one more authenticated field (the same place the device-link approval carries its chain), and a device without a kit is not a writer, as ADR-0199 says.
- **Relationship and session restore after recovery.** The record's `RelationshipSnapshot` (ADR-0199 section 7) holds, per contact, the routes, the delivery targets and an optional relationship capability. A recovered device would use them to fetch each contact's key bundles, start fresh 1:1 sessions (ADR-0137 context per relationship), send the `RecoveryNotice` of ADR-0190 and take back the group memberships by a fresh MLS join, with history coming back only from a backup file (ADR-0184, read-only). That is a relationship-establishing state machine per contact and a group re-join path, each needing its own slice and its own tests with a contact online. It is not built; a recovered account today has its identity, control and devices, and its contacts must be re-invited.

## Where did authority go?

Nowhere new. A controller already may change the account's recovery policy by a control-state transition (ADR-0199 section 5); this gives it a user path. The confirmation is the person's saved key, so a lost or abandoned replacement cannot lock the person out.

## Consequences

An account that never had a kit becomes recoverable in two taps and a written code. A leaked old sheet becomes harmless once replaced (apart from the old record's retention). Residual: the person must keep the new sheet; the old one is dead.
