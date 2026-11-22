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

impl AppState {
    pub(crate) fn verify_own_chain(&self, chain: &[Vec<u8>], creds: &[Vec<u8>]) -> Option<Acs> {
        verify_owner_chain(
            chain,
            creds,
            &self.link_evidence(),
            self.acs_checkpoint.as_ref(),
        )
    }
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Ask the Home at the entered place for the record. With no scope yet the entry waits for a place string.
    pub(super) fn recovery_find_start(&mut self) {
        let Some(j) = self.st.app.recovery_join.as_mut() else {
            return;
        };
        if j.terminal.is_some() {
            return;
        }
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
                scope: place.scope,
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
                RejectReason::StaleState => JoinStage::OutOfDate,
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
            Ok((plain, Plain::Record(r))) => {
                match self.accept_record(&root, &r, j.checkpoint.as_ref()) {
                    Some(verified) => {
                        let evidence = self.owner_evidence(&j, &r, &verified);
                        if evidence == EvidenceStage::Conflict {
                            if let Some(j) = self.st.app.recovery_join.as_mut() {
                                j.record = None;
                                j.evidence = evidence;
                                j.stage = JoinStage::Conflict;
                            }
                            return;
                        }
                        let account = hex(&verified.head.account_id);
                        match self.apply_floor(
                            &place.scope,
                            &keys.locator,
                            &account,
                            r.generation,
                            &plain,
                        ) {
                            Ok(()) => {
                                if let Some(j) = self.st.app.recovery_join.as_mut() {
                                    j.record = Some(hex(&plain).into());
                                    j.evidence = evidence;
                                    j.stage = JoinStage::Found;
                                }
                            }
                            Err(stage_err) => stage(self, stage_err),
                        }
                    }
                    None => stage(self, JoinStage::CantRead),
                }
            }
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

    /// The durable record floor is scoped to the store and opaque locator (ADR-0225 §3).
    pub(super) fn floor_key(scope: &str, locator: &[u8; 32]) -> String {
        format!("{scope}:{}", hex(locator))
    }

    pub(super) fn apply_floor(
        &mut self,
        scope: &str,
        locator: &[u8; 32],
        account: &str,
        generation: u64,
        plaintext: &[u8],
    ) -> Result<(), JoinStage> {
        let key = Self::floor_key(scope, locator);
        let floors = &mut self.st.app.recovery_floors;
        let existing = floors.get(&key);
        if existing.is_some_and(|f| f.lineage != account) {
            return Err(JoinStage::Conflict);
        }
        let legacy = floors.get(scope).filter(|f| f.lineage == account);
        if existing.is_none() && legacy.is_none() && floors.len() >= 16 {
            return Err(JoinStage::FloorTableFull);
        }
        if let (Some(current), Some(old)) = (existing, legacy)
            && current.generation == old.generation
            && current.digest != old.digest
        {
            return Err(JoinStage::Conflict);
        }
        let new = RecordFloorRec {
            lineage: account.to_string(),
            generation,
            digest: hex(&rr::plaintext_digest(plaintext)),
        };
        let held = existing
            .into_iter()
            .chain(legacy)
            .max_by_key(|f| f.generation);
        let (keep, result) = match held {
            Some(f) if f.generation > generation => (f.clone(), Err(JoinStage::OutOfDate)),
            Some(f) if f.generation == generation && f.digest != new.digest => {
                return Err(JoinStage::Conflict);
            }
            _ => (new, Ok(())),
        };
        if legacy.is_some() {
            floors.remove(scope);
        }
        floors.insert(key, keep);
        result
    }

    /// The saved checkpoint supplies the trusted starting state (ADR-0225 §1).
    fn owner_evidence(
        &self,
        j: &RecoveryJoinRec,
        r: &RecoveryRecord,
        v: &rr::Verified,
    ) -> EvidenceStage {
        let Some(cp) = j.checkpoint.as_ref() else {
            return EvidenceStage::Unpinned;
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
        let chain: Vec<Vec<u8>> = r
            .evidence
            .tail
            .iter()
            .chain(std::iter::once(&r.evidence.head))
            .cloned()
            .collect();
        match verify_owner_chain(&chain, &r.evidence.credentials, &[], Some(cp)) {
            Some(h) if h.hash() == v.head.hash() => EvidenceStage::Pinned,
            _ => EvidenceStage::NeedsEvidence,
        }
    }

    /// The checks of a record read on a fresh device: the cross-field rules (ADR-0199 section 4), the policy is the one
    /// this key derives (a different key cannot have produced it), and the carried states verify as a chain wherever
    /// they can (a recovery link inside the window has no evidence here and is held as the record's own claim, which every
    /// verifier re-checks against its own state).
    pub(super) fn accept_record(
        &self,
        root: &[u8; 32],
        r: &RecoveryRecord,
        checkpoint: Option<&CheckpointRec>,
    ) -> Option<rr::Verified> {
        let v = r.verify().ok()?;
        let epoch = v.policy.epoch();
        if v.policy.recovery_key()? != RecoveryRoot::from_bytes(*root).auth_public_key(epoch) {
            return None;
        }
        let states: Vec<&Vec<u8>> = r
            .evidence
            .tail
            .iter()
            .chain(std::iter::once(&r.evidence.head))
            .collect();
        let start = checkpoint
            .and_then(|pin| {
                v.states
                    .iter()
                    .position(|state| matches_checkpoint(state, pin))
            })
            .unwrap_or(0);
        let rest: Vec<Vec<u8>> = states.into_iter().skip(start + 1).cloned().collect();
        if !rest.is_empty() {
            // Ok(_, pending) either way: a pending recovery link is not an error here.
            control::verify_extension_partial(
                v.states[start].clone(),
                &rest,
                start + 1,
                &r.evidence.credentials,
                &[],
            )
            .ok()?;
        }
        Some(v)
    }
}

pub(super) fn checkpoint_index(chain: &[Vec<u8>], pin: &CheckpointRec) -> Option<usize> {
    chain.iter().position(|signed| {
        super::ceremony::chain_head(signed).is_some_and(|state| matches_checkpoint(&state, pin))
    })
}

fn matches_checkpoint(state: &Acs, pin: &CheckpointRec) -> bool {
    state.generation == pin.generation
        && hex(&state.account_id) == pin.account_id
        && hex(&state.hash()) == pin.acs_hash
}

/// The checkpoint is independent local evidence, whether or not the carrier starts at genesis.
pub(super) fn verify_owner_chain(
    chain: &[Vec<u8>],
    credentials: &[Vec<u8>],
    links: &[control::RecoveryLinkEvidence],
    pin: Option<&CheckpointRec>,
) -> Option<Acs> {
    let Some(pin) = pin else {
        return control::verify_chain_with(chain, credentials, links).ok();
    };
    let index = checkpoint_index(chain, pin)?;
    let anchor = super::ceremony::chain_head(&chain[index])?;
    let (head, pending) = control::verify_extension_partial(
        anchor,
        &chain[index + 1..],
        index + 1,
        credentials,
        links,
    )
    .ok()?;
    pending.is_none().then_some(head)
}
