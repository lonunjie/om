//! The owner side of lost-all-devices recovery (ADR-0199 section 4, ADR-0200 sections 6 and 8): a fresh device that holds
//! the key sheet asks the Home for the account's RecoveryRecord, opens it with keys derived from the RecoveryRoot, checks it
//! and keeps what a request and successor are built from. Core is pure here: the read is an outbox call the host performs
//! (`RecoveryGet`), and nothing the record says is an authority. The head it carries is advisory; every verifier checks the
//! request against its own pinned state (ADR-0199 section 1).

use super::*;
use crate::control;
use crate::recovery::RecoveryRoot;
use crate::recovery_record::{self as rr, Plain, RecordKeys, RecoveryRecord};
use crate::recovery_sheet as sheet;

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Ask the Home at the entered place for the record. With no scope yet the entry waits for a place string.
    pub(super) fn recovery_find_start(&mut self) {
        let Some(j) = self.st.app.recovery_join.as_mut() else {
            return;
        };
        let Some(place) = j.place.clone() else {
            j.stage = JoinStage::NeedsPlace;
            return;
        };
        let (Some(root), Some(scope)) = (
            unhex(&j.root)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok()),
            unhex(&place.scope)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok()),
        ) else {
            j.stage = JoinStage::CantRead;
            return;
        };
        j.stage = JoinStage::Checking;
        if !j
            .visited
            .iter()
            .any(|(h, s)| *h == place.host && *s == place.scope)
        {
            j.visited.push((place.host.clone(), place.scope.clone()));
        }
        let keys = RecordKeys::derive(&root, &scope);
        self.enqueue(
            NetCall::RecoveryGet {
                host: place.host,
                port: place.port,
                locator: rr::locator_of(&keys.read_cap).to_vec(),
                read_cap: keys.read_cap.to_vec(),
            },
            Purpose::RecoveryFind,
        );
    }

    /// The Home refused or does not know the record. A wrong key and an unknown record are the same answer.
    pub(crate) fn recovery_find_refused(&mut self, reason: RejectReason) {
        if let Some(j) = self.st.app.recovery_join.as_mut() {
            j.stage = match reason {
                RejectReason::NotFound => JoinStage::NotFound,
                _ => JoinStage::CantRead,
            };
        }
    }

    /// The envelope came back (`None`: an answer that is not an envelope).
    pub(crate) fn recovery_found(&mut self, envelope: Option<Vec<u8>>) {
        let Some(j) = self.st.app.recovery_join.clone() else {
            return;
        };
        let stage = |tx: &mut Self, s: JoinStage| {
            if let Some(j) = tx.st.app.recovery_join.as_mut() {
                j.stage = s;
            }
        };
        let (Some(place), Some(root)) = (
            j.place.clone(),
            unhex(&j.root)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok()),
        ) else {
            return stage(self, JoinStage::CantRead);
        };
        let Some(scope) = unhex(&place.scope)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
        else {
            return stage(self, JoinStage::CantRead);
        };
        let Some(envelope) = envelope else {
            return stage(self, JoinStage::CantRead);
        };
        let keys = RecordKeys::derive(&root, &scope);
        match rr::open(&keys, &scope, &envelope) {
            Ok(Plain::Record(r)) => match self.accept_record(&root, &r) {
                Some(plain) => {
                    if let Some(j) = self.st.app.recovery_join.as_mut() {
                        j.record = Some(hex(&plain));
                        j.stage = JoinStage::Found;
                    }
                }
                None => stage(self, JoinStage::CantRead),
            },
            Ok(Plain::Redirect(red)) => {
                // A moved Home (ADR-0200 section 6): follow it only under the rules that need no network, then look again.
                let visited: Vec<(String, [u8; 32])> = j
                    .visited
                    .iter()
                    .filter_map(|(h, s)| Some((h.clone(), unhex(s).ok()?.try_into().ok()?)))
                    .collect();
                let new = red.new_place.clone();
                match sheet::check_redirect(&new, &visited, visited.len().saturating_sub(1)) {
                    Ok(()) => {
                        if let (Some(j), Some(scope)) =
                            (self.st.app.recovery_join.as_mut(), new.scope)
                        {
                            j.place = Some(JoinPlace {
                                host: new.host.clone(),
                                port: new.port,
                                scope: hex(&scope),
                            });
                        }
                        self.recovery_find_start();
                    }
                    Err(_) => stage(self, JoinStage::CantRead),
                }
            }
            Err(_) => stage(self, JoinStage::CantRead),
        }
    }

    /// The checks of a record read on a fresh device: the cross-field rules (ADR-0199 section 4), the policy is the one
    /// this key derives (a different key cannot have produced it), and the carried states verify as a chain wherever
    /// they can (a recovery link inside the window has no evidence here and is held as the record's own claim, which every
    /// verifier re-checks against its own state). Returns the padded plaintext to keep.
    fn accept_record(&mut self, root: &[u8; 32], r: &RecoveryRecord) -> Option<Vec<u8>> {
        let v = r.verify().ok()?;
        let epoch = v.policy.epoch();
        if v.policy.recovery_key()? != RecoveryRoot::from_bytes(*root).auth_public_key(epoch) {
            return None;
        }
        let creds: Vec<Vec<u8>> = r.evidence.credentials.clone();
        let states: Vec<&Vec<u8>> = r
            .evidence
            .tail
            .iter()
            .chain(std::iter::once(&r.evidence.head))
            .collect();
        if let Some((first, rest)) = states.split_first()
            && !rest.is_empty()
        {
            let first_acs = Acs::from_body(&rr::signed_body(first).ok()?).ok()?;
            let rest: Vec<Vec<u8>> = rest.iter().map(|s| (*s).clone()).collect();
            // Ok(_, pending) either way: a pending recovery link is not an error here.
            control::verify_extension_partial(first_acs, &rest, 1, &creds, &[]).ok()?;
        }
        r.to_padded_bytes().ok()
    }
}
