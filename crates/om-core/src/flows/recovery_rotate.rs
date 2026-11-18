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
        if self
            .st
            .app
            .recovery_requests
            .values()
            .any(|r| r.state == crate::state::RecoveryObsState::Waiting)
        {
            return rejected(RejectReason::Conflict);
        }
        let epoch = self
            .st
            .app
            .recovery_policy
            .as_deref()
            .and_then(|h| unhex(h).ok())
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
            .map_or(0, |p| p.epoch() + 1);
        let root = RecoveryRoot::generate(self.rng);
        let policy = RecoveryPolicy::default_for(root.auth_public_key(epoch), epoch);
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
            policy: hex(&policy.body()),
            kit,
        });
        self.st.app.recovery_key = Some(RecoveryKeyRec {
            key_operation_id: op.clone(),
            groups: recovery_groups(root_bytes),
            sheet,
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
        let Some(policy) = unhex(&pending.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
        else {
            return rejected(RejectReason::Invalid);
        };
        let mut child = head.clone();
        child.generation = head.generation + 1;
        child.previous = Some(head.hash());
        child.recovery_commitment = Some(policy.hash());
        let signed = control::sign_acs(&child.body(), &[(&dev.device_id, &dev.signer)], self.rng);
        // The signed child must verify as a successor of the head we hold (a policy change by the controllers).
        if control::verify_successor(&head, &signed, &creds).is_err() {
            return rejected(RejectReason::Conflict);
        }
        self.st.app.acs_chain.push(hex(&signed));
        self.st.app.recovery_policy = Some(pending.policy);
        self.st.app.recovery_key_saved = true;
        self.st.app.recovery_kit = pending.kit;
        // A new lineage at the Home: the writer starts over under the new kit (the old record is left to its retention).
        self.discard_recovery_puts();
        let w = &mut self.st.app.recovery_writer;
        *w = crate::state::RecoveryWriter::default();
        if self.st.app.recovery_kit.is_some() {
            w.enabled = true;
            w.authored = true;
        }
        self.broadcast_account_update();
        OutcomeClass::Committed
    }
}
