//! The live controller side of recovery (work group E, slice 3): observing a `RecoveryRequest` and its vetoes, the
//! verifier-local interval, `VetoRecovery`, the controller co-sign ("That was me") and applying a verified completion.
//!
//! The delay is measured by THIS verifier only (ADR-0131/0147/0154). The record keeps the digest of the request, the
//! first valid observation, and `credited_ms`: monotonic time the host reported inside one boot session since that
//! first observation. Downtime between boot sessions is never credited, a reading that runs backwards credits
//! nothing, and a lost or rolled-back record starts a new interval at the next observation (it can only lengthen).
//! Home timestamps, wall clocks and request fields are never read as time. Transport is injected: nothing here
//! fetches a request; the host hands observations in.
//!
//! A completed recovery is recorded on the observation (`Completed`) and the verified successor is kept there. It is
//! NOT appended to `acs_chain`: `control::verify_chain` cannot replay a recovery link yet (it needs the policy and
//! request per link), so extending the chain would make the head unverifiable.

use super::*;
use crate::crypto::binding::Credential;
use crate::host::MonotonicSample;
use crate::recovery::{self, RecoveryPolicy, RecoveryRequest, RecoveryVeto, SuccessorBundle};

/// Requests tracked at once. Homes rate-limit; a verifier still bounds what it keeps.
pub const MAX_TRACKED: usize = 8;
const MAX_VETOES: usize = 16;
const MAX_CHILD_BYTES: usize = 16 * 1024;
const MAX_CRED_BYTES: usize = 8 * 1024;

/// Is `req` no longer applicable to `head`: another account, another epoch, another parent state, or its target
/// device was enrolled, quarantined or revoked meanwhile (V-002, ID-06)?
pub(crate) fn is_stale(head: &Acs, policy: &RecoveryPolicy, req: &RecoveryRequest) -> bool {
    let t = req.new_device_id;
    req.account_id != head.account_id
        || req.recovery_epoch != policy.epoch()
        || req.parent_state_hash != head.hash()
        || head.control.contains(&t)
        || head.messaging.contains(&t)
        || head.quarantined.contains(&t)
        || head.revoked.contains(&t)
        || head.credentials.iter().any(|(d, _)| *d == t)
}

/// Credit one host reading to a record (see the module doc).
fn credit(rec: &mut RecoveryObsRec, mono: &MonotonicSample) {
    let session = hex(&mono.boot_session);
    if session == rec.boot_session {
        if mono.elapsed_ms > rec.last_mono_ms {
            rec.credited_ms = rec
                .credited_ms
                .saturating_add(mono.elapsed_ms - rec.last_mono_ms);
            rec.last_mono_ms = mono.elapsed_ms;
        }
    } else {
        rec.boot_session = session;
        rec.last_mono_ms = mono.elapsed_ms;
    }
}

fn mono_ok(m: &MonotonicSample) -> bool {
    (1..=32).contains(&m.boot_session.len())
}

/// A live controller of `head`: in `control`, not quarantined.
fn live(head: &Acs, id: &[u8; 16]) -> bool {
    head.control.contains(id) && !head.quarantined.contains(id)
}

/// The signing key of controller `id`, but only the credential `head` commits to for it.
fn committed_key(head: &Acs, creds: &[Vec<u8>], id: &[u8; 16]) -> Option<Vec<u8>> {
    creds
        .iter()
        .filter_map(|c| Credential::from_body(c).ok())
        .find(|c| {
            c.device_id == *id
                && c.account_id == head.account_id
                && head
                    .credentials
                    .iter()
                    .any(|(d, h)| d == id && *h == c.hash())
        })
        .map(|c| c.signing_pk)
}

/// The view of every tracked request for `GetRecoveryStatus`. `ctx` is the verified head and the committed policy when
/// there are any: without them a waiting request cannot be re-checked and shows as stored.
pub(crate) fn request_views(
    app: &AppState,
    now_ms: u64,
    ctx: Option<(&Acs, &RecoveryPolicy)>,
    me: Option<[u8; 16]>,
) -> Vec<RecoveryRequestView> {
    let delay_ms = ctx.map_or(recovery::DEFAULT_DELAY_SECONDS, |(_, p)| p.delay_seconds) * 1000;
    let mut recs: Vec<&RecoveryObsRec> = app.recovery_requests.values().collect();
    recs.sort_by(|a, b| (a.first_seen_ms, &a.digest).cmp(&(b.first_seen_ms, &b.digest)));
    recs.into_iter()
        .map(|r| {
            let mut state = match r.state {
                RecoveryObsState::Waiting => RecoveryRequestStateCode::Waiting,
                RecoveryObsState::Vetoed => RecoveryRequestStateCode::Vetoed,
                RecoveryObsState::Stale => RecoveryRequestStateCode::Stale,
                RecoveryObsState::Completed => RecoveryRequestStateCode::Completed,
            };
            if let (RecoveryRequestStateCode::Waiting, Some((head, policy))) = (state, ctx) {
                let stale = unhex(&r.request)
                    .ok()
                    .and_then(|b| recovery::open_request(&b, policy.recovery_key()?).ok())
                    .is_none_or(|req| is_stale(head, policy, &req));
                if stale {
                    state = RecoveryRequestStateCode::Stale;
                }
            }
            let waiting = state == RecoveryRequestStateCode::Waiting;
            let remaining = delay_ms.saturating_sub(r.credited_ms);
            RecoveryRequestView {
                request_id: r.digest.clone(),
                operation_id: None,
                direction: RecoveryRequestDirection::Incoming,
                scope: RecoveryScope::Identity,
                custodian: RecoveryCustodianCode::RecoveryKey,
                started_at: r.first_seen_ms,
                activates_at: waiting.then(|| now_ms.saturating_add(remaining)),
                state,
                can_stop: waiting
                    && ctx.is_some_and(|(head, _)| me.is_some_and(|m| live(head, &m))),
                ready: waiting && remaining == 0,
            }
        })
        .collect()
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// The verified head, its credentials and the policy it commits to (on, with a recovery key); `None` for a profile
    /// without one (it ignores every observation).
    fn recovery_ctx(&self) -> Option<(Acs, Vec<Vec<u8>>, RecoveryPolicy)> {
        let (head, _, creds) = self.acs_head()?;
        let policy = recovery::RecoveryPolicy::from_body(
            &unhex(self.st.app.recovery_policy.as_deref()?).ok()?,
        )
        .ok()?;
        (head.recovery_commitment == Some(policy.hash())
            && policy.state == recovery::STATE_ON
            && policy.recovery_key().is_some())
        .then_some((head, creds, policy))
    }

    /// Observe a `signed-RecoveryRequest` the host fetched. A request that is invalid, signed by another key, for
    /// another account or epoch, bound to another parent state or aimed at a device that is no longer fresh is ignored
    /// with no state change; so is one already tracked (the interval is never reset).
    pub fn observe_recovery_request(
        &mut self,
        signed: &[u8],
        mono: &MonotonicSample,
    ) -> Result<(), CoreError> {
        if !mono_ok(mono) {
            return Err(CoreError::Invalid("monotonic sample"));
        }
        let Some((head, _, policy)) = self.recovery_ctx() else {
            return Ok(());
        };
        let Some(key) = policy.recovery_key() else {
            return Ok(());
        };
        let Ok(req) = recovery::open_request(signed, key) else {
            return Ok(());
        };
        if is_stale(&head, &policy, &req) {
            return Ok(());
        }
        let digest = hex(&req.digest());
        if self.st.app.recovery_requests.contains_key(&digest) {
            return Ok(());
        }
        if self.st.app.recovery_requests.len() >= MAX_TRACKED {
            // Room is made only by dropping a request that can no longer complete (oldest first), never a live one.
            let victim = self
                .st
                .app
                .recovery_requests
                .values()
                .filter(|r| r.state != RecoveryObsState::Waiting)
                .min_by_key(|r| r.first_seen_ms)
                .map(|r| r.digest.clone());
            match victim {
                Some(v) => {
                    self.st.app.recovery_requests.remove(&v);
                }
                None => return Ok(()),
            }
        }
        self.st.app.recovery_requests.insert(
            digest.clone(),
            RecoveryObsRec {
                digest,
                request: hex(signed),
                first_seen_ms: self.now_ms,
                boot_session: hex(&mono.boot_session),
                last_mono_ms: mono.elapsed_ms,
                credited_ms: 0,
                vetoes: vec![],
                own_veto: false,
                successor: None,
                state: RecoveryObsState::Waiting,
            },
        );
        Ok(())
    }

    /// Observe a `signed-RecoveryVeto`. Only a veto by a live (non-quarantined) controller of the head, valid under the
    /// credential the head commits to, naming a request this verifier tracks for this epoch, counts; anything else is
    /// ignored. A counted veto is final for that request here.
    pub fn observe_recovery_veto(
        &mut self,
        signed: &[u8],
        mono: &MonotonicSample,
    ) -> Result<(), CoreError> {
        if !mono_ok(mono) {
            return Err(CoreError::Invalid("monotonic sample"));
        }
        let Some((head, creds, policy)) = self.recovery_ctx() else {
            return Ok(());
        };
        let Ok(id) = recovery::veto_signer(signed) else {
            return Ok(());
        };
        let Some(key) = live(&head, &id)
            .then(|| committed_key(&head, &creds, &id))
            .flatten()
        else {
            return Ok(());
        };
        let Ok(veto) = recovery::open_veto(signed, &id, &key) else {
            return Ok(());
        };
        if veto.account_id != head.account_id || veto.recovery_epoch != policy.epoch() {
            return Ok(());
        }
        let Some(rec) = self
            .st
            .app
            .recovery_requests
            .get_mut(&hex(&veto.request_digest))
        else {
            return Ok(());
        };
        if rec.state != RecoveryObsState::Waiting || rec.vetoes.len() >= MAX_VETOES {
            return Ok(());
        }
        credit(rec, mono);
        let raw = hex(signed);
        if !rec.vetoes.contains(&raw) {
            rec.vetoes.push(raw);
        }
        rec.state = RecoveryObsState::Vetoed;
        Ok(())
    }

    /// The recovering device's recovery-signed successor for tracked request `digest` (hex). It is checked right away
    /// like a completion would be: a bad one is ignored; a good one that is already due is applied; otherwise it is
    /// kept so a controller can co-sign it or the interval can complete it.
    pub fn observe_recovery_successor(
        &mut self,
        digest: &str,
        bundle: &SuccessorBundle,
        mono: &MonotonicSample,
    ) -> Result<(), CoreError> {
        if !mono_ok(mono) {
            return Err(CoreError::Invalid("monotonic sample"));
        }
        if bundle.child.len() > MAX_CHILD_BYTES
            || bundle.credential.len() > MAX_CRED_BYTES
            || bundle.next_policy.len() > recovery::MAX_POLICY_BYTES
        {
            return Ok(());
        }
        let Some((head, creds, policy)) = self.recovery_ctx() else {
            return Ok(());
        };
        let Some(rec) = self.st.app.recovery_requests.get_mut(digest) else {
            return Ok(());
        };
        if rec.state != RecoveryObsState::Waiting || rec.successor.is_some() {
            return Ok(());
        }
        credit(rec, mono);
        let held = RecoverySuccessorRec {
            child: hex(&bundle.child),
            next_policy: hex(&bundle.next_policy),
            credential: hex(&bundle.credential),
        };
        if verify_held(&head, &creds, &policy, rec, &held).is_err() {
            return Ok(());
        }
        rec.successor = Some(held);
        self.try_complete(digest);
        Ok(())
    }

    /// Credit the host's monotonic reading to every waiting request, drop the ones the account has moved past, and
    /// apply any whose delay has run (with no veto) when the recovery-signed successor is held.
    pub fn recovery_tick(&mut self, mono: &MonotonicSample) -> Result<(), CoreError> {
        if !mono_ok(mono) {
            return Err(CoreError::Invalid("monotonic sample"));
        }
        let ctx = self.recovery_ctx();
        let digests: Vec<Hex> = self.st.app.recovery_requests.keys().cloned().collect();
        for d in digests {
            let Some(rec) = self.st.app.recovery_requests.get_mut(&d) else {
                continue;
            };
            if rec.state != RecoveryObsState::Waiting {
                continue;
            }
            credit(rec, mono);
            if let Some((head, _, policy)) = &ctx {
                let stale = unhex(&rec.request)
                    .ok()
                    .and_then(|b| recovery::open_request(&b, policy.recovery_key()?).ok())
                    .is_none_or(|req| is_stale(head, policy, &req));
                if stale {
                    rec.state = RecoveryObsState::Stale;
                    continue;
                }
            }
            self.try_complete(&d);
        }
        Ok(())
    }

    /// Apply the held successor of `digest` if it verifies as complete (delay run with no veto) without any further
    /// signature. A veto or a verification failure leaves the request waiting (a veto already made it `Vetoed`).
    fn try_complete(&mut self, digest: &str) {
        let Some((head, creds, policy)) = self.recovery_ctx() else {
            return;
        };
        let Some(rec) = self.st.app.recovery_requests.get(digest) else {
            return;
        };
        let Some(held) = rec.successor.clone() else {
            return;
        };
        if rec.state != RecoveryObsState::Waiting {
            return;
        }
        if matches!(
            verify_held(&head, &creds, &policy, rec, &held),
            Ok(Held::Complete)
        ) {
            self.finish_recovery(digest, held);
        }
    }

    /// Record a verified completion: the request is `Completed`, the signed successor is kept, every other waiting
    /// request is dropped (the parent head it was bound to is superseded).
    fn finish_recovery(&mut self, digest: &str, held: RecoverySuccessorRec) {
        for (d, r) in self.st.app.recovery_requests.iter_mut() {
            if d == digest {
                r.state = RecoveryObsState::Completed;
                r.successor = Some(held.clone());
            } else if r.state == RecoveryObsState::Waiting {
                r.state = RecoveryObsState::Stale;
            }
        }
    }

    /// `VetoRecovery`: this device, a live controller, signs a `RecoveryVeto` for the request and queues it for
    /// delivery (stub transport). The request is final-vetoed here at once.
    pub(super) fn veto_recovery(&mut self, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some((head, _, policy)) = self.recovery_ctx() else {
            return rejected(RejectReason::NotFound);
        };
        let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
            return rejected(RejectReason::StaleState);
        };
        if !live(&head, &me) {
            return rejected(RejectReason::Unauthorized);
        }
        let Some(rec) = self.st.app.recovery_requests.get(request_id) else {
            return rejected(RejectReason::NotFound);
        };
        if rec.state != RecoveryObsState::Waiting {
            return rejected(RejectReason::Conflict);
        }
        let Some(req) = unhex(&rec.request)
            .ok()
            .and_then(|b| recovery::open_request(&b, policy.recovery_key()?).ok())
        else {
            return rejected(RejectReason::IntegrityFailure);
        };
        if is_stale(&head, &policy, &req) {
            if let Some(r) = self.st.app.recovery_requests.get_mut(request_id) {
                r.state = RecoveryObsState::Stale;
            }
            return rejected(RejectReason::StaleState);
        }
        let veto = RecoveryVeto {
            account_id: head.account_id,
            recovery_epoch: policy.epoch(),
            request_digest: req.digest(),
        };
        let Some(dev) = self.st.device.as_ref() else {
            return rejected(RejectReason::StaleState);
        };
        let signed = recovery::sign_veto(&veto, &dev.device_id, &dev.signer, self.rng);
        if let Some(r) = self.st.app.recovery_requests.get_mut(request_id) {
            r.vetoes.push(hex(&signed));
            r.own_veto = true;
            r.state = RecoveryObsState::Vetoed;
        }
        self.enqueue(
            NetCall::RecoveryVetoFanout { bytes: signed },
            Purpose::RecoveryFanout,
        );
        OutcomeClass::Committed
    }

    /// `ConfirmRecovery` ("That was me"): this live controller adds its signature to the held recovery-signed
    /// successor, which must then verify as an IMMEDIATE completion (a controller co-sign needs no delay).
    pub(super) fn confirm_recovery(&mut self, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some((head, creds, policy)) = self.recovery_ctx() else {
            return rejected(RejectReason::NotFound);
        };
        let Some((me, signer)) = self
            .st
            .device
            .as_ref()
            .map(|d| (d.device_id, d.signer.clone()))
        else {
            return rejected(RejectReason::StaleState);
        };
        if !live(&head, &me) {
            return rejected(RejectReason::Unauthorized);
        }
        let Some(rec) = self.st.app.recovery_requests.get(request_id) else {
            return rejected(RejectReason::NotFound);
        };
        if rec.state != RecoveryObsState::Waiting {
            return rejected(RejectReason::Conflict);
        }
        // The recovering device's successor has not reached this device yet: an exact retry later may succeed.
        let Some(held) = rec.successor.clone() else {
            return rejected(RejectReason::Unavailable);
        };
        let Some(child) = unhex(&held.child)
            .ok()
            .and_then(|c| control::cosign_acs(&c, &me, &signer, self.rng).ok())
        else {
            return rejected(RejectReason::Conflict);
        };
        let cosigned = RecoverySuccessorRec {
            child: hex(&child),
            ..held
        };
        match verify_held(&head, &creds, &policy, rec, &cosigned) {
            Ok(Held::Complete) => {
                self.finish_recovery(request_id, cosigned);
                OutcomeClass::Committed
            }
            // A veto, or a successor the policy and parent majority do not accept with this one co-signer.
            _ => rejected(RejectReason::Conflict),
        }
    }
}

enum Held {
    Complete,
    Pending,
}

/// Run `verify_recovery_successor` over a held successor with this verifier's own observation.
fn verify_held(
    head: &Acs,
    creds: &[Vec<u8>],
    policy: &RecoveryPolicy,
    rec: &RecoveryObsRec,
    held: &RecoverySuccessorRec,
) -> Result<Held, ()> {
    let (Ok(child), Ok(next), Ok(cred), Ok(request)) = (
        unhex(&held.child),
        unhex(&held.next_policy),
        unhex(&held.credential),
        unhex(&rec.request),
    ) else {
        return Err(());
    };
    let next = RecoveryPolicy::from_body(&next).map_err(|_| ())?;
    let vetoes: Vec<Vec<u8>> = rec.vetoes.iter().filter_map(|v| unhex(v).ok()).collect();
    let mut all = creds.to_vec();
    if !all.contains(&cred) {
        all.push(cred);
    }
    match control::verify_recovery_successor(
        head,
        &child,
        &all,
        policy,
        &next,
        &request,
        &control::RecoveryObservation {
            elapsed_seconds: rec.credited_ms / 1000,
            vetoes: &vetoes,
        },
    ) {
        Ok(control::RecoveryVerdict::Complete { .. }) => Ok(Held::Complete),
        Ok(control::RecoveryVerdict::Pending { .. }) => Ok(Held::Pending),
        Err(_) => Err(()),
    }
}
