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
        // The exact authenticated plaintext is what the record floor compares (ADR-0202 section 1).
        let opened = rr::open_envelope(&keys, &scope, &envelope)
            .and_then(|plain| rr::read_plaintext(&plain).map(|p| (plain, p)));
        match opened {
            Ok((plain, Plain::Record(r))) => match self.accept_record(&root, &r) {
                Some(_) => {
                    let lineage = r
                        .verify()
                        .map(|v| hex(&v.head.account_id))
                        .unwrap_or_default();
                    match self.apply_floor(&place.scope, &lineage, r.generation, &plain) {
                        Ok(()) => {
                            let evidence = self.owner_evidence(&j, &r);
                            if let Some(j) = self.st.app.recovery_join.as_mut() {
                                j.record = Some(hex(&plain));
                                j.evidence = evidence;
                                j.stage = JoinStage::Found;
                            }
                        }
                        Err(stage_err) => stage(self, stage_err),
                    }
                }
                None => stage(self, JoinStage::CantRead),
            },
            Ok((_, Plain::Redirect(red))) => {
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

    /// The durable record floor (ADR-0141, ADR-0202 section 1): per RecoveryStore scope the lineage (the account the
    /// record chain belongs to), the highest generation accepted and the digest of its exact authenticated plaintext. An
    /// older generation is `OutOfDate`; other bytes at the same generation, or another lineage under the same scope, is a
    /// `Conflict`. It never lowers, and it is kept apart from the account pin and from the join.
    pub(super) fn apply_floor(
        &mut self,
        scope: &str,
        lineage: &str,
        generation: u64,
        plaintext: &[u8],
    ) -> Result<(), JoinStage> {
        let digest = hex(&rr::plaintext_digest(plaintext));
        let new = RecordFloorRec {
            lineage: lineage.to_string(),
            generation,
            digest: digest.clone(),
        };
        let Some(f) = self.st.app.recovery_floors.get(scope).cloned() else {
            self.st.app.recovery_floors.insert(scope.to_string(), new);
            return Ok(());
        };
        if f.lineage != lineage {
            return Err(JoinStage::Conflict);
        }
        let mark = unhex(&f.digest)
            .ok()
            .and_then(|d| <[u8; 32]>::try_from(d).ok())
            .map(|d| (f.generation, d));
        match rr::freshness(mark, generation, plaintext) {
            rr::Freshness::Newer => {
                self.st.app.recovery_floors.insert(scope.to_string(), new);
                Ok(())
            }
            rr::Freshness::Same => Ok(()),
            rr::Freshness::Stale => Err(JoinStage::OutOfDate),
            rr::Freshness::Equivocation => Err(JoinStage::Conflict),
        }
    }

    /// What the record's evidence amounts to against what the owner independently holds (ADR-0202 section 1, ADR-0203
    /// section 1). Only a pin from the owner's own custody (the sheet's checkpoint) can make it `Pinned`; the record's own
    /// head, tail or genesis are advisory. The chain must start at genesis (Core replays from genesis), contain exactly
    /// the pinned state, and verify with no recovery link this device cannot check.
    pub(super) fn owner_evidence(&self, j: &RecoveryJoinRec, r: &RecoveryRecord) -> EvidenceStage {
        let Some(cp) = j.checkpoint.as_ref() else {
            return EvidenceStage::Unpinned;
        };
        let Ok(v) = r.verify() else {
            return EvidenceStage::Conflict;
        };
        let pin_hash = unhex(&cp.acs_hash).ok();
        if hex(&v.head.account_id) != cp.account_id || v.head.generation < cp.generation {
            return EvidenceStage::Conflict;
        }
        match v.states.iter().find(|s| s.generation == cp.generation) {
            Some(s) if Some(s.hash().to_vec()) != pin_hash => return EvidenceStage::Conflict,
            Some(_) => {}
            None => return EvidenceStage::NeedsEvidence,
        }
        // From genesis, or the evidence is not here.
        if v.states.first().is_none_or(|s| s.previous.is_some()) {
            return EvidenceStage::NeedsEvidence;
        }
        let chain: Vec<Vec<u8>> = r
            .evidence
            .tail
            .iter()
            .chain(std::iter::once(&r.evidence.head))
            .cloned()
            .collect();
        match control::verify_chain_with(&chain, &r.evidence.credentials, &[]) {
            Ok(h) if h.hash() == v.head.hash() => EvidenceStage::Pinned,
            _ => EvidenceStage::NeedsEvidence,
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
