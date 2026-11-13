// SPDX-License-Identifier: MPL-2.0
//! The ADR-0163 group commit journal, persisted in `AppState.group_journal`.
//!
//! States (local implementation semantics, not wire): `INTENT_DURABLE -> CANDIDATE_PREPARED ->
//! PUBLISHED_UNCONFIRMED -> ACCEPTED`, with terminal alternatives `SUPERSEDED` and `ABANDONED`. For a UI they read
//! as [`Outcome`]: the first three are `Pending`, `ACCEPTED` is `Committed`, the other two are `Failed`.
//!
//! What the journal guarantees (GROUP_COMMIT_AND_STATE_CONVERGENCE section 5):
//!
//! 1. The authenticated intent and its parent `(state hash, epoch)` are durable before a Commit exists ([`begin`]).
//! 2. The exact Commit bytes, the signed binding, the Welcome and the engine's tentative state (inside its snapshot,
//!    saved in the same `AppState` write) are durable before anything is published ([`prepare`]).
//! 3. A relay acknowledgement is never acceptance: [`published`] only records transport progress.
//! 4. After a crash the recorded parent decides: still canonical, the exact bytes are replayed ([`replay`]);
//!    otherwise the candidate is superseded, never regenerated ([`recover`]).
//! 5. Acceptance promotes the engine, the group head and the idempotency marker in one `AppState` write
//!    ([`accept`]); a re-observed accepted commit does nothing ([`ReceiveOutcome::Duplicate`]).
//! 6. While a candidate is prepared or published the group is not sendable ([`AppState::group_may_send`]).
//! 7. A lost prepared state fails closed: the entry is abandoned and no equivalent bytes are rebuilt.
//!
//! Each `AppState` method here checks everything that can fail before it writes, so an error leaves the state as it
//! was. The engine is separate from `AppState`; where an engine call precedes a write, the engine call is undone
//! (`discard_pending`) if the write cannot happen.
//!
//! [`begin`]: crate::state::AppState::group_journal_begin
//! [`prepare`]: crate::state::AppState::group_journal_prepare
//! [`published`]: crate::state::AppState::group_journal_published
//! [`replay`]: crate::state::AppState::group_journal_replay
//! [`recover`]: crate::state::AppState::group_journal_recover
//! [`accept`]: crate::state::AppState::group_journal_accept

use super::binding::{
    BindingParams, KIND_COORDINATOR_SEQUENCED, KIND_RECOVERY, binding_body, mls_commit_hash,
    sign_binding, verify_binding,
};
use super::mls::{CryptoEnv, GroupCrypto};
use super::{Binding, GroupError, GroupState, Intent, Principal, R, apply, err, op};
use crate::crypto::sig::DeviceSigner;
use crate::crypto::{hex, unhex};
use serde::{Deserialize, Serialize};

/// Terminal entries kept after pruning (most recent by key order of insertion is not tracked: oldest ids go first).
const KEEP_TERMINAL: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    IntentDurable,
    CandidatePrepared,
    PublishedUnconfirmed,
    Accepted,
    Superseded,
    Abandoned,
}

/// The semantic class a UI sees (ADR-0163 section 5, reduced to what a commit can be).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Committed,
    Failed,
}

impl JournalState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Superseded | Self::Abandoned)
    }

    pub fn outcome(self) -> Outcome {
        match self {
            Self::Accepted => Outcome::Committed,
            Self::Superseded | Self::Abandoned => Outcome::Failed,
            _ => Outcome::Pending,
        }
    }
}

/// The exact material of a prepared candidate (all hex; never rebuilt).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub commit: String,
    pub welcome: Option<String>,
    /// The `signed-GroupCommitBinding`.
    pub signed_binding: String,
    pub commit_hash: String,
    pub resulting_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub intent_id: String,
    pub conversation_id: String,
    pub state: JournalState,
    /// The canonical `GroupControlIntent` body (hex).
    pub intent_body: String,
    /// The acting account (hex), the verified signer's account, evaluated at the parent.
    pub actor_account: String,
    pub parent_state_hash: String,
    pub parent_epoch: u64,
    pub candidate: Option<Candidate>,
    pub publish_attempts: u32,
    /// Why a terminal failure happened (a stable code).
    pub reason: Option<String>,
}

/// What Core must do to publish: the exact prepared bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    pub commit: Vec<u8>,
    pub welcome: Option<Vec<u8>>,
    pub signed_binding: Vec<u8>,
}

/// The MLS action an intent needs. It must agree with the intent's operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitPlan {
    /// `member.admit`: add the devices behind these KeyPackages.
    Add(Vec<Vec<u8>>),
    /// `member.remove` / `member.leave`: remove these device leaves.
    Remove(Vec<Vec<u8>>),
}

/// What restart reconciliation says to do with an open entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Intent is durable and its parent is canonical, no Commit exists yet: prepare one.
    Prepare(String),
    /// Retransmit exactly these prepared bytes ([`AppState::group_journal_replay`]).
    Retransmit(String),
    /// The parent is no longer canonical: the entry is superseded; rebase a still-applicable reducing intent, or
    /// revalidate a non-reducing one, as a fresh intent.
    Superseded(String),
    /// The prepared state was lost: failed closed, reconcile and build a fresh authorized candidate.
    Abandoned(String),
}

/// What observing a received Commit did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiveOutcome {
    /// Applied to the canonical head; `superseded` names this device's own losing candidate, if there was one.
    Applied {
        epoch: u64,
        still_member: bool,
        superseded: Option<String>,
    },
    /// The accepted Commit was seen before: nothing repeated.
    Duplicate,
    /// This is this device's own candidate coming back: call `group_journal_accept`.
    OwnCandidate(String),
    /// A same-parent candidate of this device is canonical by the ADR-0173 key; the received one is inert here.
    Loses,
}

fn h(x: &[u8]) -> String {
    hex(x)
}

fn un(x: &str) -> R<Vec<u8>> {
    unhex(x).map_err(|_| GroupError("record_corrupt"))
}

fn mls<T>(r: Result<T, super::mls::MlsError>) -> R<T> {
    r.map_err(|e| GroupError(e.0))
}

impl crate::state::AppState {
    /// Bind the MLS `group_id` of a conversation (once).
    pub fn group_bind_mls_id(&mut self, conversation_id: &[u8], group_id: &[u8]) -> R<()> {
        let key = h(conversation_id);
        match self.group_mls_ids.get(&key) {
            Some(x) if *x == h(group_id) => Ok(()),
            Some(_) => err("mls_id_rebound"),
            None => {
                self.group_mls_ids.insert(key, h(group_id));
                Ok(())
            }
        }
    }

    pub fn group_mls_id(&self, conversation_id: &[u8]) -> R<Vec<u8>> {
        un(self
            .group_mls_ids
            .get(&h(conversation_id))
            .ok_or(GroupError("unknown_group"))?)
    }

    /// Whether application messages may be sent: false while a candidate for this group is prepared or published
    /// (ADR-0163 rule 6, "no tentative-branch messaging").
    pub fn group_may_send(&self, conversation_id: &[u8]) -> bool {
        let c = h(conversation_id);
        !self.group_journal.values().any(|e| {
            e.conversation_id == c
                && matches!(
                    e.state,
                    JournalState::CandidatePrepared | JournalState::PublishedUnconfirmed
                )
        })
    }

    fn open_entry_for(&self, conversation_id: &str) -> Option<&JournalEntry> {
        self.group_journal
            .values()
            .find(|e| e.conversation_id == conversation_id && !e.state.is_terminal())
    }

    fn parent_is_canonical(&self, e: &JournalEntry) -> R<bool> {
        let head = self
            .group(&un(&e.conversation_id)?)?
            .ok_or(GroupError("unknown_group"))?;
        Ok(h(&head.hash()) == e.parent_state_hash && head.mls_epoch_ref == e.parent_epoch)
    }

    fn entry(&self, intent_id: &str) -> R<&JournalEntry> {
        self.group_journal
            .get(intent_id)
            .ok_or(GroupError("unknown_intent"))
    }

    fn prune_journal(&mut self) {
        let terminal: Vec<String> = self
            .group_journal
            .iter()
            .filter(|(_, e)| e.state.is_terminal())
            .map(|(k, _)| k.clone())
            .collect();
        if terminal.len() > KEEP_TERMINAL {
            for k in &terminal[..terminal.len() - KEEP_TERMINAL] {
                self.group_journal.remove(k);
            }
        }
    }

    /// Step 1, INTENT_DURABLE. Records the intent against the current head after proving it would apply there.
    /// Idempotent for the same intent id and body; refuses a second open entry for the group (`journal_busy`).
    pub fn group_journal_begin(
        &mut self,
        intent_body: &[u8],
        actor_account: &[u8],
    ) -> R<JournalEntry> {
        let intent = Intent::decode(intent_body)?;
        let id = h(&intent.intent_id);
        if let Some(e) = self.group_journal.get(&id) {
            return if e.intent_body == h(intent_body) {
                Ok(e.clone())
            } else {
                err("intent_id_reused")
            };
        }
        let head = self
            .group(&intent.conversation_id)?
            .ok_or(GroupError("unknown_group"))?;
        let actor = Principal::account(actor_account.to_vec());
        // Authorization and shape are evaluated at the parent now; a refused intent is never made durable.
        apply(&head, &intent, &actor, head.mls_epoch_ref + 1)?;
        let conv = h(&intent.conversation_id);
        if self.open_entry_for(&conv).is_some() {
            return err("journal_busy");
        }
        let e = JournalEntry {
            intent_id: id.clone(),
            conversation_id: conv,
            state: JournalState::IntentDurable,
            intent_body: h(intent_body),
            actor_account: h(actor_account),
            parent_state_hash: h(&head.hash()),
            parent_epoch: head.mls_epoch_ref,
            candidate: None,
            publish_attempts: 0,
            reason: None,
        };
        self.group_journal.insert(id, e.clone());
        Ok(e)
    }

    /// Step 2, CANDIDATE_PREPARED. Creates the pending Commit, builds and signs the binding, and stores the exact
    /// bytes together with the engine snapshot (which holds the tentative state) in this one write.
    pub fn group_journal_prepare(
        &mut self,
        intent_id: &str,
        plan: &CommitPlan,
        binding_kind: u64,
        engine: &mut dyn GroupCrypto,
        env: &mut CryptoEnv<'_>,
        signer: &DeviceSigner,
    ) -> R<Publication> {
        let e = self.entry(intent_id)?.clone();
        if e.state != JournalState::IntentDurable {
            return err("bad_journal_state");
        }
        if binding_kind == KIND_RECOVERY {
            return err("recovery_unsupported");
        }
        if !self.parent_is_canonical(&e)? {
            return err("stale_parent");
        }
        let conv = un(&e.conversation_id)?;
        let group_id = self.group_mls_id(&conv)?;
        let intent = Intent::decode(&un(&e.intent_body)?)?;
        let head = self.group(&conv)?.ok_or(GroupError("unknown_group"))?;
        let actor = Principal::account(un(&e.actor_account)?);
        let next = apply(&head, &intent, &actor, head.mls_epoch_ref + 1)?;
        match (intent.operation, plan) {
            (op::MEMBER_ADMIT, CommitPlan::Add(_))
            | (op::MEMBER_REMOVE | op::MEMBER_LEAVE, CommitPlan::Remove(_)) => {}
            _ => return err("plan_mismatch"),
        }
        if mls(engine.epoch(&group_id))? != e.parent_epoch {
            return err("epoch_mismatch");
        }
        let prepared = mls(match plan {
            CommitPlan::Add(kps) => engine.add_commit(env, &group_id, kps),
            CommitPlan::Remove(devs) => engine.remove_commit(env, &group_id, devs),
        })?;
        let built = (|| -> R<(Vec<u8>, Vec<u8>)> {
            if prepared.resulting_epoch != e.parent_epoch + 1 {
                return err("epoch_step");
            }
            let body = binding_body(&BindingParams {
                conversation_id: conv.clone(),
                binding_kind,
                reducing: intent.reducing,
                previous_state_hash: head.hash(),
                previous_state_version: head.state_version,
                previous_mls_epoch: e.parent_epoch,
                resulting_mls_epoch: prepared.resulting_epoch,
                intent_hash: Some(Intent::hash_of_body(&un(&e.intent_body)?)),
                mls_commit_hash: mls_commit_hash(&prepared.commit),
                resulting_state_hash: next.hash(),
                committer_device: engine.device_id().to_vec(),
                readded_devices: Vec::new(),
            })?;
            let rnd = crate::crypto::rng::arr32(env.rng);
            let signed = sign_binding(signer, engine.device_id(), &body, rnd);
            Ok((body, signed))
        })();
        let (_body, signed) = match built {
            Ok(x) => x,
            Err(x) => {
                let _ = engine.discard_pending(&group_id);
                return Err(x);
            }
        };
        let entry = self
            .group_journal
            .get_mut(intent_id)
            .expect("entry checked above");
        entry.candidate = Some(Candidate {
            commit: h(&prepared.commit),
            welcome: prepared.welcome.as_ref().map(|w| h(w)),
            signed_binding: h(&signed),
            commit_hash: h(&mls_commit_hash(&prepared.commit)),
            resulting_epoch: prepared.resulting_epoch,
        });
        entry.state = JournalState::CandidatePrepared;
        self.group_crypto_save(engine);
        Ok(Publication {
            commit: prepared.commit,
            welcome: prepared.welcome,
            signed_binding: signed,
        })
    }

    /// Record that the candidate was handed to transport (also each retry). Transport progress only.
    pub fn group_journal_published(&mut self, intent_id: &str) -> R<()> {
        let e = self.entry(intent_id)?;
        if !matches!(
            e.state,
            JournalState::CandidatePrepared | JournalState::PublishedUnconfirmed
        ) {
            return err("bad_journal_state");
        }
        let e = self.group_journal.get_mut(intent_id).expect("checked");
        e.state = JournalState::PublishedUnconfirmed;
        e.publish_attempts += 1;
        Ok(())
    }

    /// The exact prepared bytes for a retry, only while the recorded parent is canonical and the engine still holds
    /// the identical pending Commit. Anything else is an error and nothing is rebuilt.
    pub fn group_journal_replay(
        &self,
        intent_id: &str,
        engine: &dyn GroupCrypto,
    ) -> R<Publication> {
        let e = self.entry(intent_id)?;
        if !matches!(
            e.state,
            JournalState::CandidatePrepared | JournalState::PublishedUnconfirmed
        ) {
            return err("bad_journal_state");
        }
        if !self.parent_is_canonical(e)? {
            return err("stale_parent");
        }
        let c = e.candidate.as_ref().ok_or(GroupError("record_corrupt"))?;
        let commit = un(&c.commit)?;
        let group_id = self.group_mls_id(&un(&e.conversation_id)?)?;
        match mls(engine.pending(&group_id))? {
            Some(p) if p.commit == commit => Ok(Publication {
                commit,
                welcome: c.welcome.as_deref().map(un).transpose()?,
                signed_binding: un(&c.signed_binding)?,
            }),
            _ => err("candidate_unrecoverable"),
        }
    }

    fn finish(
        &mut self,
        intent_id: &str,
        engine: &mut dyn GroupCrypto,
        state: JournalState,
        reason: &str,
    ) -> R<()> {
        let e = self.entry(intent_id)?.clone();
        if e.state.is_terminal() {
            return err("bad_journal_state");
        }
        if e.candidate.is_some() {
            let gid = self.group_mls_id(&un(&e.conversation_id)?)?;
            if mls(engine.pending(&gid))?.is_some() {
                mls(engine.discard_pending(&gid))?;
            }
            self.group_crypto_save(engine);
        }
        let e = self.group_journal.get_mut(intent_id).expect("checked");
        e.state = state;
        e.reason = Some(reason.to_string());
        self.prune_journal();
        Ok(())
    }

    /// SUPERSEDED: a same-parent candidate lost, or its parent moved. Its tentative state is dropped, never merged.
    pub fn group_journal_supersede(
        &mut self,
        intent_id: &str,
        engine: &mut dyn GroupCrypto,
        reason: &str,
    ) -> R<()> {
        self.finish(intent_id, engine, JournalState::Superseded, reason)
    }

    /// ABANDONED: no longer wanted, or its prepared state is lost.
    pub fn group_journal_abandon(
        &mut self,
        intent_id: &str,
        engine: &mut dyn GroupCrypto,
        reason: &str,
    ) -> R<()> {
        self.finish(intent_id, engine, JournalState::Abandoned, reason)
    }

    /// ACCEPTED: the ordering profile accepted this candidate. Promotes the engine (merge), the canonical head and
    /// the idempotency marker together; every check runs before the first change.
    pub fn group_journal_accept(
        &mut self,
        intent_id: &str,
        engine: &mut dyn GroupCrypto,
    ) -> R<GroupState> {
        let e = self.entry(intent_id)?.clone();
        if !matches!(
            e.state,
            JournalState::CandidatePrepared | JournalState::PublishedUnconfirmed
        ) {
            return err("bad_journal_state");
        }
        if !self.parent_is_canonical(&e)? {
            return err("stale_parent");
        }
        let c = e.candidate.as_ref().ok_or(GroupError("record_corrupt"))?;
        let conv = un(&e.conversation_id)?;
        let gid = self.group_mls_id(&conv)?;
        match mls(engine.pending(&gid))? {
            Some(p) if h(&p.commit) == c.commit => {}
            _ => return err("candidate_unrecoverable"),
        }
        let head = self.group(&conv)?.ok_or(GroupError("unknown_group"))?;
        let intent = Intent::decode(&un(&e.intent_body)?)?;
        let actor = Principal::account(un(&e.actor_account)?);
        let next = apply(&head, &intent, &actor, e.parent_epoch + 1)?;
        let vb = verify_own_binding(c)?;
        if vb.resulting_state_hash != next.hash() {
            return err("resulting_state_mismatch");
        }
        let info = mls(engine.merge_pending(&gid))?;
        if info.epoch != c.resulting_epoch {
            // The engine has merged; this cannot be undone here, so it is a hard integrity failure.
            return err("epoch_integrity");
        }
        self.groups
            .insert(e.conversation_id.clone(), super::GroupRec::of(&next));
        self.group_accepted
            .insert(e.conversation_id.clone(), c.commit_hash.clone());
        self.group_crypto_save(engine);
        let entry = self.group_journal.get_mut(intent_id).expect("checked");
        entry.state = JournalState::Accepted;
        self.prune_journal();
        Ok(next)
    }

    /// Restart reconciliation of every open entry, in intent-id order. Mutates only entries it must supersede or
    /// abandon; returns what Core should do for each.
    pub fn group_journal_recover(&mut self, engine: &mut dyn GroupCrypto) -> R<Vec<Recovery>> {
        let open: Vec<JournalEntry> = self
            .group_journal
            .values()
            .filter(|e| !e.state.is_terminal())
            .cloned()
            .collect();
        let mut out = Vec::new();
        for e in open {
            let canonical = self.parent_is_canonical(&e)?;
            let id = e.intent_id.clone();
            if e.state == JournalState::IntentDurable {
                if canonical {
                    out.push(Recovery::Prepare(id));
                } else {
                    self.group_journal_supersede(&id, engine, "parent_moved")?;
                    out.push(Recovery::Superseded(id));
                }
                continue;
            }
            if !canonical {
                self.group_journal_supersede(&id, engine, "parent_moved")?;
                out.push(Recovery::Superseded(id));
            } else if self.group_journal_replay(&id, engine).is_ok() {
                out.push(Recovery::Retransmit(id));
            } else {
                self.group_journal_abandon(&id, engine, "prepared_state_lost")?;
                out.push(Recovery::Abandoned(id));
            }
        }
        Ok(out)
    }

    /// Observe a Commit made by another device. Verifies the signed binding against `committer_pk`, checks it against
    /// the canonical head and the intent, resolves a same-parent race by the ADR-0173 key, then applies the Commit
    /// and the state change together. Idempotent for an accepted Commit.
    #[allow(clippy::too_many_arguments)]
    pub fn group_receive_commit(
        &mut self,
        signed_binding: &[u8],
        committer_pk: &[u8],
        commit: &[u8],
        intent_body: &[u8],
        actor_account: &[u8],
        engine: &mut dyn GroupCrypto,
        env: &mut CryptoEnv<'_>,
    ) -> R<ReceiveOutcome> {
        let vb = verify_binding(signed_binding, committer_pk)?;
        let bd = &vb.binding;
        if bd.binding_kind == KIND_RECOVERY {
            return err("recovery_unsupported");
        }
        let chash = mls_commit_hash(commit);
        if bd.mls_commit_hash != chash {
            return err("commit_hash_mismatch");
        }
        let conv_hex = h(&bd.conversation_id);
        if self.group_accepted.get(&conv_hex) == Some(&h(&chash)) {
            return Ok(ReceiveOutcome::Duplicate);
        }
        let head = self
            .group(&bd.conversation_id)?
            .ok_or(GroupError("unknown_group"))?;
        if bd.previous_state_hash != head.hash()
            || bd.previous_state_version != head.state_version
            || bd.previous_mls_epoch != head.mls_epoch_ref
        {
            return err("stale_parent");
        }
        let intent = Intent::decode(intent_body)?;
        if Some(Intent::hash_of_body(intent_body).to_vec()) != bd.intent_hash {
            return err("intent_hash_mismatch");
        }
        if intent.reducing != bd.reducing {
            return err("reducing_mismatch");
        }
        let actor = Principal::account(actor_account.to_vec());
        let next = apply(&head, &intent, &actor, head.mls_epoch_ref + 1)?;
        if next.hash().as_slice() != bd.resulting_state_hash.as_slice() {
            return err("resulting_state_mismatch");
        }
        let gid = self.group_mls_id(&bd.conversation_id)?;
        let ci = mls(engine.inspect_commit(&gid, commit))?;
        if ci.from_epoch != bd.previous_mls_epoch
            || ci.resulting_epoch != bd.resulting_mls_epoch
            || ci.committer != bd.committer_device
        {
            return err("commit_binding_mismatch");
        }
        // Same-parent race against this device's own candidate.
        let mut superseded = None;
        let own = self
            .open_entry_for(&conv_hex)
            .filter(|e| e.candidate.is_some())
            .cloned();
        if let Some(own) = own {
            let oc = own.candidate.as_ref().expect("filtered");
            let ob = un(&oc.signed_binding)?;
            let obody = verify_own_body(&ob)?;
            let theirs = bd.ordering_key(&vb.body);
            let ours = Binding::decode(&obody)?.ordering_key(&obody);
            if theirs == ours {
                return Ok(ReceiveOutcome::OwnCandidate(own.intent_id));
            }
            if ours < theirs {
                return Ok(ReceiveOutcome::Loses);
            }
            let id = own.intent_id.clone();
            self.group_journal_supersede(&id, engine, "lost_same_parent")?;
            superseded = Some(id);
        }
        let info = mls(engine.process_commit(env, &gid, commit))?;
        self.groups
            .insert(conv_hex.clone(), super::GroupRec::of(&next));
        self.group_accepted.insert(conv_hex, h(&chash));
        self.group_crypto_save(engine);
        Ok(ReceiveOutcome::Applied {
            epoch: info.epoch,
            still_member: info.still_member,
            superseded,
        })
    }
}

/// The body of a binding this device itself signed and stored (the signature was made by us; the body is re-parsed
/// strictly, so a corrupted record is refused).
fn verify_own_body(signed: &[u8]) -> R<Vec<u8>> {
    let v = crate::crypto::cbor::decode(signed).map_err(|_| GroupError("record_corrupt"))?;
    let body = v
        .get(1)
        .and_then(|x| x.b())
        .map_err(|_| GroupError("record_corrupt"))?
        .to_vec();
    Binding::decode(&body)?;
    Ok(body)
}

fn verify_own_binding(c: &Candidate) -> R<Binding> {
    Binding::decode(&verify_own_body(&un(&c.signed_binding)?)?)
}

/// Used by tests and Core callers that want the coordinator-sequenced kind without importing the binding module.
pub const DEFAULT_BINDING_KIND: u64 = KIND_COORDINATOR_SEQUENCED;
