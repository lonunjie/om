//! Replacing the recovery key (ADR-0205): the way an account without a recovery kit, or one that wants a sheet with a newer
//! checkpoint, gets a new key. The RecoveryRoot is never kept, so a sheet cannot be refreshed in place; a new root and a new
//! recovery policy take its place, committed in the account's control state by a controller device (ADR-0199 section 5).
//!
//! Two steps, so that abandoning leaves the old key working: `ReplaceRecoveryKey` stages a new root's policy and kit and shows
//! the new key once; only `ConfirmRecoveryKeyStored` for that operation signs the transition, swaps the kit and starts the
//! record under it.

use super::*;
use crate::recovery::{RecoveryPolicy, RecoveryRoot};
use crate::recovery_sheet::{OwnerCheckpoint, Sheet};
use crate::state::PendingRecovery;

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    fn replacement_blocked(&self, head: &Acs) -> bool {
        control::safety_frozen(head)
            || !self.st.app.acs_forks.is_empty()
            || self
                .st
                .app
                .recovery_requests
                .values()
                .any(|r| r.state == RecoveryObsState::Waiting)
    }

    fn committed_recovery_epoch(&self, head: &Acs) -> Option<Option<u64>> {
        let Some(commitment) = head.recovery_commitment else {
            return Some(None);
        };
        let raw = unhex(self.st.app.recovery_policy.as_deref()?).ok()?;
        let policy = RecoveryPolicy::from_body(&raw).ok()?;
        (policy.hash() == commitment).then_some(Some(policy.epoch()))
    }

    fn replacement_candidate(head: &Acs, policy: &RecoveryPolicy) -> Option<Acs> {
        let mut child = head.clone();
        child.generation = head.generation.checked_add(1)?;
        child.previous = Some(head.hash());
        child.recovery_commitment = Some(policy.hash());
        Some(child)
    }

    /// Stage a new recovery key: nothing is in force until it is confirmed saved.
    pub(super) fn replace_recovery_key(&mut self, op: &OperationId) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let (Some((head, _, _)), Some(me)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return rejected(RejectReason::StaleState);
        };
        // Only an active controller commits a policy to the account; an incoming recovery in flight is never mixed with a change
        // of who may recover.
        if !head.control.contains(&me) || head.quarantined.contains(&me) {
            return rejected(RejectReason::Unauthorized);
        }
        if self.replacement_blocked(&head) {
            return rejected(RejectReason::Conflict);
        }
        let required_votes = control::majority(head.control.len());
        if required_votes > 1 {
            return OutcomeClass::NeedsUserAction { action: UserActionCode::AuthorizeWithDevice };
        }
        let Some(parent_epoch) = self.committed_recovery_epoch(&head) else {
            return rejected(RejectReason::StaleState);
        };
        let Some(epoch) = parent_epoch.map_or(Some(0), |epoch| epoch.checked_add(1)) else {
            return rejected(RejectReason::Conflict);
        };
        let root = RecoveryRoot::generate(self.rng);
        let policy = RecoveryPolicy::default_for(root.auth_public_key(epoch), epoch);
        let Some(child) = Self::replacement_candidate(&head, &policy) else {
            return rejected(RejectReason::Conflict);
        };
        let root_arr = zeroize::Zeroizing::new(*root.expose());
        let root_bytes: &[u8; 32] = &root_arr;
        drop(root);
        // The place is the one this account already keeps its record at, else the one the Home offers now.
        let place = match self.recovery_kit_value() {
            Some(k) => Some((k.scope, k.host.clone(), k.port)),
            None => self
                .recovery_place
                .clone()
                .map(|p| (p.scope, p.host.clone(), p.port)),
        };
        let (sheet, kit) = match place {
            Some((scope, host, port)) => {
                let checkpoint = OwnerCheckpoint {
                    account_id: head.account_id,
                    generation: head.generation,
                    acs_hash: head.hash(),
                };
                let sheet = Sheet::full_v2(*root_bytes, scope, port, &host, checkpoint)
                    .ok()
                    .map(|s| s.to_sheet_form());
                let kit = crate::recovery_record::RecoveryKit::make(
                    root_bytes,
                    scope,
                    &host,
                    port,
                    policy.epoch(),
                    arr32(self.rng),
                    true,
                );
                (sheet, Some(SecretHex::from(hex(&kit.to_bytes()))))
            }
            None => (None, None),
        };
        self.st.app.pending_recovery = Some(PendingRecovery {
            key_operation_id: op.clone(),
            parent: hex(&head.hash()),
            parent_epoch,
            controllers: head.control.iter().map(|id| hex(id)).collect(),
            required_votes,
            child_hash: hex(&child.hash()),
            policy: hex(&policy.body()),
            kit,
        });
        self.st.app.recovery_key = Some(RecoveryKeyRec {
            key_operation_id: op.clone(),
            groups: recovery_groups(root_bytes),
            sheet: sheet.map(SecretHex::from),
        });
        OutcomeClass::Committed
    }

    /// The new key is confirmed saved: commit the new policy in the control state, keep the new kit and start the record under it.
    /// Any failure drops the staged key and leaves the account exactly as it was.
    pub(super) fn commit_recovery_replacement(&mut self, op: &OperationId) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(pending) = self.st.app.pending_recovery.take() else {
            return rejected(RejectReason::NotFound);
        };
        self.st.app.recovery_key = None;
        if &pending.key_operation_id != op {
            return rejected(RejectReason::NotFound);
        }
        let (Some((head, _, creds)), Some(dev)) = (self.acs_head(), self.st.device.as_ref()) else {
            return rejected(RejectReason::StaleState);
        };
        if !head.control.contains(&dev.device_id) || head.quarantined.contains(&dev.device_id) {
            return rejected(RejectReason::Unauthorized);
        }
        if pending.parent != hex(&head.hash())
            || self.committed_recovery_epoch(&head) != Some(pending.parent_epoch)
            || pending.controllers != head.control.iter().map(|id| hex(id)).collect::<Vec<_>>()
            || pending.required_votes != control::majority(head.control.len())
        {
            return rejected(RejectReason::StaleState);
        }
        if self.replacement_blocked(&head) {
            return rejected(RejectReason::Conflict);
        }
        let Some(policy) = unhex(&pending.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
        else {
            return rejected(RejectReason::Invalid);
        };
        let Some(child) = Self::replacement_candidate(&head, &policy)
            .filter(|child| hex(&child.hash()) == pending.child_hash)
        else {
            return rejected(RejectReason::StaleState);
        };
        let signed = control::sign_acs(&child.body(), &[(&dev.device_id, &dev.signer)], self.rng);
        // The signed child must verify as a successor of the head we hold (a policy change by the controllers).
        if control::verify_successor(&head, &signed, &creds).is_err() {
            return rejected(RejectReason::Conflict);
        }
        if pending.kit.is_some()
            && let Some(old) = self.recovery_kit_value()
        {
            self.st.app.retired_recovery_records.push(RetiredRecoveryRecord {
                place: JoinPlace { host: old.host.clone(), port: old.port, scope: hex(&old.scope) },
                locator: hex(&old.locator()),
                write_cap: hex(&old.write_cap).into(),
                replacement_generation: child.generation,
                expires_ms: self.now_ms.saturating_add(400 * 86_400_000),
            });
        }
        self.retire_recovery_inbox();
        self.st.app.acs_chain.push(hex(&signed));
        self.st.app.recovery_policy = Some(pending.policy);
        self.st.app.recovery_key_saved = true;
        self.st.app.recovery_kit = pending.kit;
        // The writer starts over under the new kit. The acknowledged replacement unlocks retirement.
        self.discard_recovery_puts();
        let w = &mut self.st.app.recovery_writer;
        *w = crate::state::RecoveryWriter::default();
        w.sheet_anchor = Some(CheckpointRec {
            account_id: hex(&head.account_id),
            generation: head.generation,
            acs_hash: hex(&head.hash()),
        });
        if self.st.app.recovery_kit.is_some() {
            w.enabled = true;
            w.authored = true;
        }
        self.broadcast_account_update();
        OutcomeClass::Committed
    }
}
