//! The recovery approval ceremony (ADR-0202 section 3, ADR-0203 sections 2 and 3): a restricted owner device and a
//! surviving controller exchange the exact signed recovery request and the controller's bound approval over two
//! short-lived queues on the controller's own Home, with the owner as the courier of an offer link.
//!
//! The controller (`OfferRecoveryApproval`) opens two queues and shows the link. The restricted device
//! (`JoinRecoveryApproval`) sends the exact proposal. The controller verifies it against its own reconciled head,
//! registers it as an observed request and shows the six digits; the person's `ConfirmRecovery` is the explicit
//! confirmation. It co-signs the same child body and replies with a bound descriptor and the evidence in 50,000-byte
//! chunks. The restricted device checks the binding, assembles, runs the full account verifier and only then holds the
//! controller's chain as its pin; the existing reconcile round and atomic adoption (`recovery_attempt.rs`) finish it.
//!
//! The exchange shares the ADR-0192 mechanism and nothing of its meaning: own link prefix and version, key schedule,
//! associated data, message types and queues. An enrollment link is never read as an approval link or the reverse. It
//! never carries the RecoveryRoot, a session or history key, or any elapsed time, and a ceremony that ends for any
//! reason leaves the signed request and its verifier-local attempt exactly as they were.

use super::ceremony::{CEREMONY_TTL_MS, open_msg_ad};
use super::*;
use crate::crypto::binding::Credential;
use crate::crypto::link_ceremony::{self as lc, CeremonyKeys};
use crate::crypto::recovery_approval as ra;
use crate::flows::recovery_watch as watch;
use crate::recovery::{self, RecoveryPolicy, SuccessorBundle};
use crate::state::{ApprovalJoin, ApprovalOffer};

/// At most this many approval offers wait at once; a newer one replaces an unanswered older one.
const MAX_ACTIVE: usize = 2;
/// Ended offers are summaries only and go after this long.
const KEEP_MS: u64 = 24 * 60 * 60 * 1000;

fn offer_keys(o: &ApprovalOffer) -> Option<CeremonyKeys> {
    let secret = unhex(&o.secret).ok().filter(|s| s.len() >= 16)?;
    let b_recv = unhex(&o.b_recv).ok().filter(|b| !b.is_empty())?;
    Some(ra::keys(
        o.expires_at / 1000,
        o.b_route.relay_uri.as_bytes(),
        &secret,
        &o.a_route.capability,
        &b_recv,
    ))
}

fn join_keys(j: &ApprovalJoin) -> Option<CeremonyKeys> {
    let secret = unhex(&j.secret).ok().filter(|s| s.len() >= 16)?;
    let (a_cap, b_recv) = (unhex(&j.a_cap).ok()?, unhex(&j.b_recv).ok()?);
    if a_cap.is_empty() || b_recv.is_empty() {
        return None;
    }
    Some(ra::keys(
        j.expires_at / 1000,
        j.relay_uri.as_bytes(),
        &secret,
        &a_cap,
        &b_recv,
    ))
}

/// The child state a signed successor carries (`None` when it is not one).
fn child_of(signed: &[u8]) -> Option<Acs> {
    Acs::from_body(&crate::recovery_record::signed_body(signed).ok()?).ok()
}

fn h32(b: &[u8]) -> Option<[u8; 32]> {
    b.try_into().ok()
}

/// The controller's chain as it stood before the recovery completed, captured for the reply.
pub(super) struct ApprovalPre {
    op: OperationId,
    states: Vec<Vec<u8>>,
    creds: Vec<Vec<u8>>,
    dossiers: Vec<ra::Dossier>,
    controller: [u8; 16],
    /// The request digest, the parent head and the target credential the proposal named.
    bound: ([u8; 32], [u8; 32], [u8; 32]),
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    // ----------------------------------------------------------------------------------------------- controller: offer

    /// `OfferRecoveryApproval`: only a live control device of an account with recovery on offers one.
    pub(super) fn offer_recovery_approval(&mut self, op: &OperationId) -> OutcomeClass {
        use RejectReason::*;
        let rej = |r| OutcomeClass::Rejected { reason: r };
        let (Some((head, ..)), Some(me)) = (
            self.recovery_ctx(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return rej(StaleState);
        };
        if !watch::live(&head, &me) {
            return rej(Unauthorized);
        }
        let Some(uri) = self
            .offers
            .iter()
            .filter(|o| o.is_valid())
            .min_by_key(|o| o.priority)
            .map(|o| o.relay_uri.clone())
        else {
            return rej(Unavailable);
        };
        let other: Vec<String> = self
            .offers
            .iter()
            .filter(|o| o.relay_uri != uri)
            .map(|o| o.relay_uri.clone())
            .collect();
        let (Some((a_mailbox, a_route)), Some((b_mailbox, b_route))) = (
            self.take_route_excluding(&other),
            self.take_route_excluding(&other),
        ) else {
            return rej(Unavailable);
        };
        for o in self.st.app.approval_offers.values_mut() {
            if !o.state.is_terminal() {
                o.state = LinkState::Expired;
            }
        }
        self.scrub_approvals();
        let active = self
            .st
            .app
            .approval_offers
            .values()
            .filter(|o| !o.state.is_terminal())
            .count();
        if active >= MAX_ACTIVE {
            return rej(RateLimited);
        }
        let secret = self.rand16();
        self.st.app.approval_offers.insert(
            op.clone(),
            ApprovalOffer {
                operation_id: op.clone(),
                secret: hex(&secret).into(),
                a_mailbox: hex(&a_mailbox),
                a_route,
                b_mailbox: hex(&b_mailbox),
                b_route: b_route.clone(),
                b_recv: String::new(),
                link: None,
                created_ms: self.now_ms,
                expires_at: self.now_ms + CEREMONY_TTL_MS,
                state: LinkState::Opening,
                request_digest: None,
                proposal: None,
                code: None,
                reply_undelivered: false,
            },
        );
        self.enqueue(
            NetCall::OpenQueue {
                mailbox: b_mailbox.to_vec(),
                relay_uri: b_route.relay_uri,
            },
            Purpose::OpenCeremonyQueue {
                operation_id: op.clone(),
            },
        );
        OutcomeClass::PendingNetwork
    }

    /// The response queue is open: its receive capability completes the link. Returns `false` when `op` is not an
    /// approval offer (the caller then treats it as an enrollment offer).
    pub(super) fn approval_open_queue(&mut self, op: &str, cap: Option<Vec<u8>>) -> bool {
        let now_s = self.now_s();
        let Some(o) = self.st.app.approval_offers.get_mut(op) else {
            return false;
        };
        let secret = unhex(&o.secret)
            .ok()
            .and_then(|s| <[u8; 16]>::try_from(s).ok());
        let link = match (&cap, secret) {
            (Some(c), Some(secret))
                if o.state == LinkState::Opening
                    && o.expires_at / 1000 > now_s
                    && (16..=64).contains(&c.len()) =>
            {
                u32::try_from(o.expires_at / 1000).ok().and_then(|e| {
                    ra::encode_offer(&ra::Offer {
                        expiry_s: e,
                        secret,
                        a_cap: o.a_route.capability.clone(),
                        b_recv: c.clone(),
                        uri: o.b_route.relay_uri.clone(),
                    })
                })
            }
            _ => None,
        };
        match (link, cap) {
            (Some(link), Some(cap)) => {
                o.b_recv = hex(&cap);
                o.link = Some(link.into());
                o.state = LinkState::WaitingForScan;
                self.set_outcome(&op.to_string(), OutcomeClass::Committed);
            }
            _ => {
                o.state = LinkState::Expired;
                self.set_outcome(
                    &op.to_string(),
                    OutcomeClass::Rejected {
                        reason: RejectReason::Unavailable,
                    },
                );
            }
        }
        self.scrub_approvals();
        true
    }

    /// A ceremony deposit or queue creation was refused for good: the ceremony ends and nothing is half done.
    pub(super) fn approval_failed(&mut self, op: &str, reply: bool) {
        if let Some(o) = self.st.app.approval_offers.get_mut(op) {
            if !o.state.is_terminal() {
                o.state = LinkState::Failed;
            }
            o.reply_undelivered |= reply;
        }
        if let Some(a) = self.attempt_approval_mut()
            && a.operation_id == op
            && !a.state.is_terminal()
        {
            a.state = LinkState::Failed;
        }
        self.scrub_approvals();
    }

    /// A ceremony past its deadline ends; the signed request and the attempt are untouched.
    pub(super) fn expire_approvals(&mut self) {
        let now = self.now_ms;
        for o in self.st.app.approval_offers.values_mut() {
            if o.expires_at <= now && !o.state.is_terminal() {
                o.state = LinkState::Expired;
            }
        }
        if let Some(a) = self.attempt_approval_mut()
            && a.expires_at <= now
            && !a.state.is_terminal()
        {
            a.state = LinkState::Expired;
        }
        self.scrub_approvals();
        self.st
            .app
            .approval_offers
            .retain(|_, o| !o.state.is_terminal() || o.expires_at + KEEP_MS > now);
    }

    fn scrub_approvals(&mut self) {
        use zeroize::Zeroize;
        for o in self
            .st
            .app
            .approval_offers
            .values_mut()
            .filter(|o| o.state.is_terminal())
        {
            o.secret.zeroize();
            o.link = None;
            o.a_route.capability.zeroize();
            o.b_route.capability.zeroize();
            o.b_recv.zeroize();
            o.proposal = None;
        }
        if let Some(a) = self.attempt_approval_mut()
            && a.state.is_terminal()
        {
            a.secret.zeroize();
            a.a_cap.zeroize();
            a.b_recv.zeroize();
            a.chunks.clear();
        }
    }

    fn attempt_approval_mut(&mut self) -> Option<&mut ApprovalJoin> {
        self.st
            .app
            .recovery_join
            .as_mut()
            .and_then(|j| j.attempt.as_mut())
            .and_then(|a| a.approval.as_mut())
    }

    // ------------------------------------------------------------------------------------------- controller: inbound

    /// A message on a request queue of ours. Returns `false` when `mailbox` is not an approval queue.
    pub(super) fn approval_mailbox(&mut self, mailbox: &str, bytes: &[u8]) -> bool {
        if let Some(op) = self
            .st
            .app
            .approval_offers
            .iter()
            .find(|(_, o)| o.a_mailbox == mailbox)
            .map(|(k, _)| k.clone())
        {
            self.approval_inbound(&op, bytes);
            return true;
        }
        let mine = self
            .st
            .app
            .recovery_join
            .as_ref()
            .and_then(|j| j.attempt.as_ref())
            .and_then(|a| a.approval.as_ref())
            .is_some_and(|a| a.b_mailbox == mailbox);
        if mine {
            self.approval_reply_inbound(bytes);
            return true;
        }
        false
    }

    fn approval_inbound(&mut self, op: &str, bytes: &[u8]) {
        let Some(offer) = self.st.app.approval_offers.get(op).cloned() else {
            return;
        };
        if offer.expires_at <= self.now_ms || offer.state != LinkState::WaitingForScan {
            return; // one offer, one proposal
        }
        let Some(keys) = offer_keys(&offer) else {
            return;
        };
        let Some(pt) = open_msg_ad(&keys, ra::ROLE_REQ, bytes, ra::aead_ad) else {
            return; // not sealed under this ceremony: dropped
        };
        self.approval_proposal(op, &offer, &keys, &pt);
    }

    /// The proposal: checked against this controller's own reconciled head before the person is asked anything.
    fn approval_proposal(
        &mut self,
        op: &str,
        offer: &ApprovalOffer,
        keys: &CeremonyKeys,
        pt: &[u8],
    ) {
        let Some(prop) = ra::Proposal::from_bytes(pt) else {
            return; // a proposal of another purpose or a malformed one: refused whole, the offer stays open
        };
        let fail = |tx: &mut Self| {
            if let Some(o) = tx.st.app.approval_offers.get_mut(op) {
                o.state = LinkState::Failed;
            }
            tx.scrub_approvals();
        };
        let (Some((head, creds, policy)), Some(me)) = (
            self.recovery_ctx(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return fail(self);
        };
        let Some(key) = policy.recovery_key() else {
            return fail(self);
        };
        let (Ok(req), Some(child)) = (
            recovery::open_request(&prop.request, key),
            child_of(&prop.bundle.child),
        ) else {
            return fail(self);
        };
        if !watch::live(&head, &me) {
            return fail(self);
        }
        let digest = hex(&req.digest());
        let child_hash = child.hash();
        let known_veto = self
            .st
            .app
            .recovery_requests
            .get(&digest)
            .is_some_and(|r| r.state == RecoveryObsState::Vetoed);
        let stale = watch::is_stale(&head, &policy, &req);
        if stale || known_veto {
            // A newer state or a veto goes back as evidence, with no co-signature: the device reconciles it.
            let vetoes = self
                .st
                .app
                .recovery_requests
                .get(&digest)
                .map(|r| r.vetoes.iter().filter_map(|v| unhex(v).ok()).collect())
                .unwrap_or_default();
            let pre = self.approval_pre(
                op,
                me,
                (
                    req.digest(),
                    req.parent_state_hash,
                    req.new_device_credential,
                ),
            );
            let Some(pre) = pre else { return fail(self) };
            let Some(ev) = approval_evidence(&pre, None, vetoes) else {
                return fail(self);
            };
            self.approval_send(
                op,
                offer,
                keys,
                pt,
                req.digest(),
                req.parent_state_hash,
                req.new_device_credential,
                child_hash,
                me,
                ev,
            );
            if let Some(o) = self.st.app.approval_offers.get_mut(op) {
                o.state = LinkState::Rejected;
            }
            self.scrub_approvals();
            return;
        }
        let held = RecoverySuccessorRec {
            child: hex(&prop.bundle.child),
            next_policy: hex(&prop.bundle.next_policy),
            credential: hex(&prop.bundle.credential),
        };
        // Shape, target and policy: the same verifier every controller uses, with this device's own observation.
        let mut obs = self
            .st
            .app
            .recovery_requests
            .get(&digest)
            .cloned()
            .unwrap_or_else(|| RecoveryObsRec {
                digest: digest.clone(),
                request: hex(&prop.request),
                first_seen_ms: self.now_ms,
                boot_session: String::new(),
                last_mono_ms: 0,
                credited_ms: 0,
                vetoes: vec![],
                own_veto: false,
                veto_homes: vec![],
                successor: None,
                state: RecoveryObsState::Waiting,
            });
        if obs.state != RecoveryObsState::Waiting
            || obs.request != hex(&prop.request)
            || obs
                .successor
                .as_ref()
                .is_some_and(|s| s.child != held.child)
        {
            return fail(self);
        }
        obs.successor = Some(held.clone());
        if watch::verify_held(&head, &creds, &policy, &obs, &held).is_err() {
            return fail(self);
        }
        if !self.st.app.recovery_requests.contains_key(&digest)
            && self.st.app.recovery_requests.len() >= watch::MAX_TRACKED
        {
            return fail(self);
        }
        self.st.app.recovery_requests.insert(digest.clone(), obs);
        if let Some(o) = self.st.app.approval_offers.get_mut(op) {
            o.state = LinkState::AwaitingApproval;
            o.request_digest = Some(digest);
            o.code = Some(ra::code_digits(&keys.code, pt));
            o.proposal = Some(hex(pt));
        }
    }

    /// The controller's chain before a recovery completes, bounded as the wire allows (`None`: it does not fit, so no
    /// reply can be made and the ceremony fails safe).
    fn approval_pre(
        &self,
        op: &str,
        me: [u8; 16],
        bound: ([u8; 32], [u8; 32], [u8; 32]),
    ) -> Option<ApprovalPre> {
        let states: Vec<Vec<u8>> = self
            .st
            .app
            .acs_chain
            .iter()
            .map(|s| unhex(s).ok())
            .collect::<Option<_>>()?;
        let creds: Vec<Vec<u8>> = self
            .st
            .app
            .acs_credentials
            .iter()
            .map(|s| unhex(s).ok())
            .collect::<Option<_>>()?;
        let dossiers = control::dossier_of(&states, &self.st.app.link_evidence())
            .into_iter()
            .map(|d| ra::Dossier {
                child_hash: d.child_hash,
                policy: d.policy,
                next_policy: d.next_policy,
                request: d.request,
            })
            .collect();
        Some(ApprovalPre {
            op: op.to_string(),
            states,
            creds,
            dossiers,
            controller: me,
            bound,
        })
    }

    /// Before `ConfirmRecovery` co-signs: when `digest` is the request an open approval offer was shown, capture the
    /// chain as it stands (the evidence is the PARENT chain, taken before the successor is applied).
    pub(super) fn approval_before_confirm(&self, digest: &str) -> Option<ApprovalPre> {
        let (op, offer) = self.st.app.approval_offers.iter().find(|(_, o)| {
            o.state == LinkState::AwaitingApproval
                && o.expires_at > self.now_ms
                && o.request_digest.as_deref() == Some(digest)
        })?;
        let me = self.st.device.as_ref()?.device_id;
        let (_, _, policy) = self.recovery_ctx()?;
        let prop = ra::Proposal::from_bytes(&unhex(offer.proposal.as_deref()?).ok()?)?;
        let req = recovery::open_request(&prop.request, policy.recovery_key()?).ok()?;
        self.approval_pre(
            op,
            me,
            (
                req.digest(),
                req.parent_state_hash,
                req.new_device_credential,
            ),
        )
    }

    /// After a confirmed recovery completed here: send the bound reply and the evidence. The co-signed successor is
    /// what the controller's own verifier accepted as complete.
    pub(super) fn approval_after_confirm(
        &mut self,
        pre: ApprovalPre,
        cosigned: &RecoverySuccessorRec,
    ) {
        let Some(offer) = self.st.app.approval_offers.get(&pre.op).cloned() else {
            return;
        };
        let (Some(keys), Some(proposal)) = (
            offer_keys(&offer),
            offer.proposal.as_deref().and_then(|p| unhex(p).ok()),
        ) else {
            return;
        };
        let (Ok(child), Ok(next_policy), Ok(credential)) = (
            unhex(&cosigned.child),
            unhex(&cosigned.next_policy),
            unhex(&cosigned.credential),
        ) else {
            return;
        };
        let bundle = SuccessorBundle {
            child,
            next_policy,
            credential,
        };
        let Some(c) = child_of(&bundle.child) else {
            return;
        };
        let (digest, parent, target_cred) = pre.bound;
        let controller = pre.controller;
        let Some(ev) = approval_evidence(&pre, Some(bundle), vec![]) else {
            if let Some(o) = self.st.app.approval_offers.get_mut(&pre.op) {
                o.state = LinkState::Failed;
            }
            self.scrub_approvals();
            return;
        };
        self.approval_send(
            &pre.op,
            &offer,
            &keys,
            &proposal,
            digest,
            parent,
            target_cred,
            c.hash(),
            controller,
            ev,
        );
        if let Some(o) = self.st.app.approval_offers.get_mut(&pre.op) {
            o.state = LinkState::Linked;
        }
        self.scrub_approvals();
    }

    /// Seal the reply and every evidence chunk for the response queue.
    #[allow(clippy::too_many_arguments)]
    fn approval_send(
        &mut self,
        op: &str,
        offer: &ApprovalOffer,
        keys: &CeremonyKeys,
        proposal: &[u8],
        request_digest: [u8; 32],
        parent: [u8; 32],
        target: [u8; 32],
        child: [u8; 32],
        controller: [u8; 16],
        ev: ra::Evidence,
    ) {
        #[cfg(feature = "test-loopback")]
        let fault = crate::runtime::TEST_APPROVAL_FAULT.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(feature = "test-loopback"))]
        let fault = 0u8;
        let (mut request_digest, mut target) = (request_digest, target);
        match fault {
            1 => target[0] ^= 1,
            2 => request_digest[0] ^= 1,
            _ => {}
        }
        let bytes = ev.to_bytes();
        let chunks = ra::split(&bytes, ra::CHUNK_BYTES);
        let evidence_digest = ra::evidence_digest(&bytes);
        let descriptor = ra::Descriptor {
            request_digest,
            parent,
            target,
            controller,
            child,
            evidence_digest,
            evidence_len: bytes.len() as u64,
            chunks: chunks.len() as u64,
        };
        let raw = descriptor.to_bytes();
        let mut tag = ra::reply_tag(&keys.bind, &keys.id, proposal, &raw);
        if fault == 5 {
            tag[0] ^= 1;
        }
        let reply = ra::Reply { descriptor, tag };
        let expires_s = offer.expires_at / 1000;
        let mut out = vec![reply.to_bytes()];
        let mut chunks = chunks;
        if fault == 3 {
            chunks.pop();
        }
        if fault == 4
            && let Some(first) = chunks.first()
        {
            let mut other = first.clone();
            other[0] ^= 1;
            out.push(
                ra::Chunk {
                    digest: evidence_digest,
                    index: 0,
                    data: other,
                }
                .to_bytes(),
            );
        }
        for (i, data) in chunks.into_iter().enumerate() {
            out.push(
                ra::Chunk {
                    digest: evidence_digest,
                    index: i as u64,
                    data,
                }
                .to_bytes(),
            );
        }
        let mut sealed = vec![];
        for m in &out {
            match self.seal_msg_ad(keys, ra::ROLE_RESP, m, expires_s, ra::aead_ad) {
                Some(s) => sealed.push(s),
                None => {
                    if let Some(o) = self.st.app.approval_offers.get_mut(op) {
                        o.state = LinkState::Failed;
                    }
                    return;
                }
            }
        }
        let (cap, uri) = (
            offer.b_route.capability.clone(),
            offer.b_route.relay_uri.clone(),
        );
        // ADR-0226 section 5: the reply (manifest) and every chunk are repeated whole by the sender; nothing is buffered
        // by the receiver before the manifest.
        self.schedule_evidence_repeat(op, &offer.b_route, &sealed, offer.expires_at);
        for s in sealed {
            self.deposit_ceremony(op, &cap, &uri, s, true);
        }
    }

    // ------------------------------------------------------------------------------------------ restricted device

    /// `JoinRecoveryApproval`: this restricted device, with an attempt waiting, sends its exact proposal to the
    /// controller whose link it was given. An enrollment link, an unsupported version or an expired link is refused
    /// whole and changes nothing.
    pub(super) fn join_recovery_approval(&mut self, op: &OperationId, link: &str) -> OutcomeClass {
        use RejectReason::*;
        let rej = |r| OutcomeClass::Rejected { reason: r };
        let Some(j) = self.st.app.recovery_join.clone() else {
            return rej(NotFound);
        };
        let Some(a) = j
            .attempt
            .as_ref()
            .filter(|a| a.state == AttemptState::Waiting)
        else {
            return rej(StaleState);
        };
        if self.st.device.is_some() || a.approval.as_ref().is_some_and(|x| !x.state.is_terminal()) {
            return rej(Conflict);
        }
        let offer = match ra::decode_offer(link) {
            Ok(o) => o,
            Err(ra::OfferError::Version) => return rej(Unsupported),
            Err(_) => return rej(Invalid),
        };
        let now_s = self.now_s();
        let expiry_s = u64::from(offer.expiry_s);
        if expiry_s <= now_s {
            return rej(Expired);
        }
        if expiry_s > now_s + CEREMONY_TTL_MS / 1000 + 60 {
            return rej(Invalid);
        }
        let Some(held) = a.obs.successor.as_ref() else {
            return rej(StaleState);
        };
        let (Ok(request), Ok(child), Ok(next_policy), Ok(credential)) = (
            unhex(&a.obs.request),
            unhex(&held.child),
            unhex(&held.next_policy),
            unhex(&held.credential),
        ) else {
            return rej(IntegrityFailure);
        };
        let proposal = ra::Proposal {
            request,
            bundle: SuccessorBundle {
                child,
                next_policy,
                credential,
            },
        }
        .to_bytes();
        let keys = ra::keys(
            expiry_s,
            offer.uri.as_bytes(),
            &offer.secret,
            &offer.a_cap,
            &offer.b_recv,
        );
        let Some(sealed) = self.seal_msg_ad(&keys, ra::ROLE_REQ, &proposal, expiry_s, ra::aead_ad)
        else {
            return rej(TooLarge);
        };
        let rec = ApprovalJoin {
            operation_id: op.clone(),
            secret: hex(&offer.secret).into(),
            relay_uri: offer.uri.clone(),
            a_cap: hex(&offer.a_cap),
            b_mailbox: hex(&ra::mailbox(&offer.b_recv)),
            b_recv: hex(&offer.b_recv),
            expires_at: expiry_s * 1000,
            state: LinkState::AwaitingApproval,
            code: ra::code_digits(&keys.code, &proposal),
            proposal: hex(&proposal),
            descriptor: None,
            chunks: vec![],
        };
        if let Some(a) = self
            .st
            .app
            .recovery_join
            .as_mut()
            .and_then(|j| j.attempt.as_mut())
        {
            a.approval = Some(rec);
        }
        self.deposit_ceremony(op, &offer.a_cap, &offer.uri, sealed, false);
        OutcomeClass::PendingNetwork
    }

    /// A message on the restricted device's response queue: the reply first, then the evidence chunks.
    fn approval_reply_inbound(&mut self, bytes: &[u8]) {
        let Some(a) = self
            .st
            .app
            .recovery_join
            .as_ref()
            .and_then(|j| j.attempt.as_ref())
            .filter(|a| a.state == AttemptState::Waiting)
            .cloned()
        else {
            return;
        };
        let Some(join) = a.approval.clone() else {
            return;
        };
        if join.expires_at <= self.now_ms || join.state.is_terminal() {
            return;
        }
        let Some(keys) = join_keys(&join) else { return };
        let Some(pt) = open_msg_ad(&keys, ra::ROLE_RESP, bytes, ra::aead_ad) else {
            return; // another ceremony, another purpose or another direction: dropped
        };
        match (
            join.state,
            cbor::decode(&pt).ok().and_then(|v| v.get(0).ok()?.u().ok()),
        ) {
            (LinkState::AwaitingApproval, Some(ra::TYPE_REPLY)) => {
                self.approval_on_reply(&a, &join, &keys, &pt)
            }
            (LinkState::Transferring, Some(ra::TYPE_CHUNK)) => {
                self.approval_on_chunk(&a, &join, &pt)
            }
            // An identical repeat of the reply is a no-op; a different one ends the exchange (ADR-0226 section 5).
            (LinkState::Transferring, Some(ra::TYPE_REPLY)) => {
                if let Some((_, raw)) = ra::Reply::from_bytes(&pt)
                    && join.descriptor.as_deref() != Some(hex(&raw).as_str())
                {
                    self.approval_end(LinkState::Failed);
                }
            }
            // An early chunk may be dropped; anything else is not part of this exchange.
            _ => {}
        }
    }

    fn approval_end(&mut self, state: LinkState) {
        if let Some(x) = self.attempt_approval_mut() {
            x.state = state;
        }
        self.scrub_approvals();
    }

    fn approval_on_reply(
        &mut self,
        a: &OwnerAttemptRec,
        join: &ApprovalJoin,
        keys: &CeremonyKeys,
        pt: &[u8],
    ) {
        let Some((reply, raw)) = ra::Reply::from_bytes(pt) else {
            return self.approval_end(LinkState::Failed);
        };
        let proposal = unhex(&join.proposal).unwrap_or_default();
        let expect = ra::reply_tag(&keys.bind, &keys.id, &proposal, &raw);
        let d = &reply.descriptor;
        let held_child = a
            .obs
            .successor
            .as_ref()
            .and_then(|s| unhex(&s.child).ok())
            .and_then(|c| child_of(&c));
        // The exact proposal, request, parent, target and proposed child are what it names; anything else is refused.
        let bound = lc::tag_eq(&reply.tag, &expect)
            && unhex(&a.obs.digest).ok().and_then(|x| h32(&x)) == Some(d.request_digest)
            && unhex(&a.parent).ok().and_then(|x| h32(&x)) == Some(d.parent)
            && unhex(&a.target).ok().and_then(|x| h32(&x)) == Some(d.target)
            && held_child.is_some_and(|c| c.hash() == d.child);
        if !bound {
            return self.approval_end(LinkState::Failed);
        }
        let n = d.chunks as usize;
        if let Some(x) = self.attempt_approval_mut() {
            x.descriptor = Some(hex(&raw));
            x.chunks = vec![None; n];
            x.state = LinkState::Transferring;
        }
    }

    fn approval_on_chunk(&mut self, _a: &OwnerAttemptRec, join: &ApprovalJoin, pt: &[u8]) {
        let (Some(chunk), Some(raw)) = (
            ra::Chunk::from_bytes(pt),
            join.descriptor.as_deref().and_then(|d| unhex(d).ok()),
        ) else {
            return; // malformed: not part of this exchange
        };
        let Some(descriptor) = ra::Descriptor::from_bytes(&raw) else {
            return self.approval_end(LinkState::Failed);
        };
        let mut asm = ra::Assembler::new(descriptor);
        for (i, c) in join.chunks.iter().enumerate() {
            if let Some(data) = c.as_deref().and_then(|c| unhex(c).ok()) {
                let _ = asm.add(&ra::Chunk {
                    digest: asm.descriptor.evidence_digest,
                    index: i as u64,
                    data,
                });
            }
        }
        match asm.add(&chunk) {
            Ok(()) => {}
            Err(ra::ChunkError::Conflict) => return self.approval_end(LinkState::Failed),
            Err(ra::ChunkError::Rejected) => return, // a chunk of no pending manifest: dropped
        }
        let idx = chunk.index as usize;
        if let Some(x) = self.attempt_approval_mut()
            && let Some(slot) = x.chunks.get_mut(idx)
        {
            *slot = Some(hex(&chunk.data));
        }
        if let Some(whole) = asm.assemble() {
            self.approval_apply(&whole);
        }
    }

    /// The evidence is whole. Verify everything before anything is held: the chain from genesis with its dossier, the
    /// head against the reply's parent, the policy it commits to, the controller's standing and the co-signed
    /// successor. Then the controller's chain becomes this device's pin and the reconcile round that precedes
    /// adoption starts. Any failure leaves the attempt exactly as it was.
    fn approval_apply(&mut self, whole: &[u8]) {
        let (Some(j), Some(a)) = (
            self.st.app.recovery_join.clone(),
            self.st
                .app
                .recovery_join
                .as_ref()
                .and_then(|j| j.attempt.clone()),
        ) else {
            return;
        };
        let Some(raw) = a
            .approval
            .as_ref()
            .and_then(|x| x.descriptor.as_deref())
            .and_then(|d| unhex(d).ok())
        else {
            return;
        };
        let (Some(d), Some(ev)) = (
            ra::Descriptor::from_bytes(&raw),
            ra::Evidence::from_bytes(whole),
        ) else {
            return self.approval_end(LinkState::Failed);
        };
        let start = match &j.checkpoint {
            Some(pin) => match super::recovery_owner::checkpoint_index(&ev.states, pin) {
                Some(i) => i,
                None => return self.approval_end(LinkState::Failed),
            },
            None => 0,
        };
        let states: Vec<_> = ev.states[start..]
            .iter()
            .filter_map(|s| child_of_state(s))
            .collect();
        let links: Option<Vec<_>> = ev
            .dossiers
            .iter()
            .map(|x| {
                Some(control::RecoveryLinkEvidence {
                    child_hash: x.child_hash,
                    policy: x.policy.clone(),
                    next_policy: x.next_policy.clone(),
                    request: x.request.clone(),
                    elapsed_seconds: self.owner_link_credit(&x.request)?,
                })
            })
            .collect();
        let Some(links) = links else {
            return self.approval_end(LinkState::Failed);
        };
        let Some(head) = super::recovery_owner::verify_owner_chain(
            &ev.states,
            &ev.creds,
            &links,
            j.checkpoint.as_ref(),
        ) else {
            return self.approval_end(LinkState::Failed);
        };
        if a.competing_heads_full
            || a.competing_heads.iter().any(|observation| {
                !states.iter().any(|s| {
                    s.generation == observation.head.generation
                        && hex(&s.account_id) == observation.head.account_id
                        && hex(&s.hash()) == observation.head.acs_hash
                })
            })
        {
            return self.approval_end(LinkState::Failed);
        }
        let Some(policy) = unhex(&a.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
        else {
            return self.approval_end(LinkState::Failed);
        };
        let latest_links: Vec<_> = links
            .iter()
            .map(|l| RecoveryLinkRec {
                child_hash: hex(&l.child_hash),
                policy: hex(&l.policy),
                next_policy: hex(&l.next_policy),
                request: hex(&l.request),
                elapsed_seconds: l.elapsed_seconds,
            })
            .collect();
        let parent = h32(&unhex(&a.parent).unwrap_or_default());
        if head.hash() != d.parent || Some(head.hash()) != parent {
            // Learn a verified descendant of the pinned head. Ending this request cannot
            // authorize another request: the person must ask again against the new parent.
            let extends = ev.successor.is_none()
                && ev.states[start..]
                    .iter()
                    .any(|s| child_of_state(s).is_some_and(|x| Some(x.hash()) == parent))
                && head.generation > a.parent_generation;
            if extends
                && let Some(at) = self
                    .st
                    .app
                    .recovery_join
                    .as_mut()
                    .and_then(|j| j.attempt.as_mut())
            {
                at.state = AttemptState::Stale;
                at.obs.state = RecoveryObsState::Stale;
                if j.evidence == EvidenceStage::Pinned
                    && head.recovery_commitment == Some(policy.hash())
                    && watch::live(&head, &d.controller)
                    && ev.vetoes.is_empty()
                    && a.snapshot_conflict_generation.is_none()
                {
                    at.chain = ev.states.iter().map(|s| hex(s)).collect();
                    at.creds = ev.creds.iter().map(|c| hex(c)).collect();
                    at.links = latest_links;
                    at.competing_heads.clear();
                    at.retry_from_verified_head = true;
                }
            }
            if extends
                && !self
                    .attempt()
                    .is_some_and(OwnerAttemptRec::can_retry_from_verified_head)
            {
                self.st.app.close_owner_attempt(AttemptState::Stale);
            }
            return self.approval_end(LinkState::Failed);
        }
        if head.recovery_commitment != Some(policy.hash()) || !watch::live(&head, &d.controller) {
            return self.approval_end(LinkState::Failed);
        }
        // The pin and the evidence, held on a copy until the successor verifies too.
        let mut next = a.clone();
        next.chain = ev.states.iter().map(|s| hex(s)).collect();
        next.creds = ev.creds.iter().map(|c| hex(c)).collect();
        next.links = latest_links;
        if let Some(b) = &ev.successor {
            let (Some(original), Some(cosigned)) = (a.obs.successor.clone(), child_of(&b.child))
            else {
                return self.approval_end(LinkState::Failed);
            };
            let same_body = unhex(&original.child)
                .ok()
                .and_then(|c| crate::recovery_record::signed_body(&c).ok())
                == crate::recovery_record::signed_body(&b.child).ok();
            if !same_body
                || cosigned.hash() != d.child
                || hex(&b.next_policy) != original.next_policy
                || hex(&b.credential) != original.credential
            {
                return self.approval_end(LinkState::Failed);
            }
            let held = RecoverySuccessorRec {
                child: hex(&b.child),
                ..original
            };
            let creds: Vec<Vec<u8>> = ev.creds.clone();
            if !matches!(
                watch::verify_held(&head, &creds, &policy, &a.obs, &held),
                Ok(watch::Held::Complete)
            ) {
                return self.approval_end(LinkState::Failed);
            }
            next.obs.successor = Some(held);
            next.controller_approved = true;
        }
        let mut j = j;
        j.evidence = EvidenceStage::Pinned;
        j.attempt = Some(next);
        self.st.app.recovery_join = Some(j);
        // A veto the controller knew is observed like any other, and ends the attempt for good.
        for v in &ev.vetoes {
            self.owner_observe_veto(v);
        }
        self.approval_end(LinkState::Linked);
        let go = self
            .st
            .app
            .recovery_join
            .as_ref()
            .and_then(|j| j.attempt.as_ref())
            .is_some_and(|a| a.state == AttemptState::Waiting && ev.successor.is_some());
        if go {
            self.owner_reconcile_now();
        }
    }
}

fn child_of_state(signed: &[u8]) -> Option<Acs> {
    child_of(signed)
}

/// The evidence a controller carries: its verified chain through the parent, the credential bodies, the dossier, the
/// vetoes it knows and, when it approves, the co-signed successor. `None` when any bound would be exceeded: nothing is
/// truncated to fit.
fn approval_evidence(
    pre: &ApprovalPre,
    successor: Option<SuccessorBundle>,
    vetoes: Vec<Vec<u8>>,
) -> Option<ra::Evidence> {
    if pre.states.is_empty()
        || pre.states.len() > ra::MAX_STATES
        || pre.creds.is_empty()
        || pre.creds.len() > ra::MAX_CREDS
        || pre.dossiers.len() > ra::MAX_DOSSIERS
        || vetoes.len() > ra::MAX_VETOES
    {
        return None;
    }
    let ev = ra::Evidence {
        states: pre.states.clone(),
        creds: pre.creds.clone(),
        dossiers: pre.dossiers.clone(),
        vetoes,
        successor,
    };
    let bytes = ev.to_bytes();
    (bytes.len() <= ra::MAX_EVIDENCE_BYTES && ra::Evidence::from_bytes(&bytes).is_some())
        .then_some(ev)
}

// ---------------------------------------------------------------------------------------------- polling and views

/// What the host polls for approvals: the controller's request queue and the restricted device's response queue.
pub(super) fn approval_poll_targets(
    st: &ProfileState,
    now_ms: u64,
    out: &mut Vec<crate::host::PollTarget>,
) {
    for o in st.app.approval_offers.values() {
        if o.expires_at > now_ms
            && matches!(
                o.state,
                LinkState::WaitingForScan | LinkState::AwaitingApproval
            )
            && let Ok(m) = unhex(&o.a_mailbox)
        {
            out.push(crate::host::PollTarget {
                mailbox: m,
                after: st.app.cursors.get(&o.a_mailbox).copied().unwrap_or(0),
                kind: MailboxKind::Relay,
                relay_uri: Some(o.a_route.relay_uri.clone()),
                capability: vec![],
            });
        }
    }
    if let Some(a) = st
        .app
        .recovery_join
        .as_ref()
        .and_then(|j| j.attempt.as_ref())
        .and_then(|a| a.approval.as_ref())
        && a.expires_at > now_ms
        && matches!(
            a.state,
            LinkState::AwaitingApproval | LinkState::Transferring
        )
        && let (Ok(m), Ok(cap)) = (unhex(&a.b_mailbox), unhex(&a.b_recv))
    {
        out.push(crate::host::PollTarget {
            mailbox: m,
            after: st.app.cursors.get(&a.b_mailbox).copied().unwrap_or(0),
            kind: MailboxKind::Relay,
            relay_uri: Some(a.relay_uri.clone()),
            capability: cap,
        });
    }
}

/// The Home and capability to acknowledge an approval mailbox with (`None`: not one).
pub(super) fn approval_ack_route(st: &ProfileState, mailbox: &str) -> Option<(String, Vec<u8>)> {
    if let Some(o) = st
        .app
        .approval_offers
        .values()
        .find(|o| o.a_mailbox == mailbox)
    {
        return Some((o.a_route.relay_uri.clone(), vec![]));
    }
    st.app
        .recovery_join
        .as_ref()
        .and_then(|j| j.attempt.as_ref())
        .and_then(|a| a.approval.as_ref())
        .filter(|a| a.b_mailbox == mailbox)
        .map(|a| (a.relay_uri.clone(), unhex(&a.b_recv).unwrap_or_default()))
}

/// What the controller shows while it waits for the restricted device and the person.
pub fn approval_offer_view(
    st: &ProfileState,
    op: &str,
    now_ms: u64,
) -> Option<crate::api::RecoveryApprovalOfferView> {
    use crate::api::ApprovalOfferCode as C;
    let o = st.app.approval_offers.get(op)?;
    let live = o.expires_at > now_ms;
    let state = match o.state {
        LinkState::Opening => return None,
        LinkState::WaitingForScan if live => C::WaitingForDevice,
        LinkState::AwaitingApproval if live => C::AwaitingConfirmation,
        LinkState::Linked | LinkState::Transferring => C::Approved,
        LinkState::Rejected => C::Declined,
        LinkState::Failed => C::Failed,
        _ => C::Expired,
    };
    Some(crate::api::RecoveryApprovalOfferView {
        operation_id: o.operation_id.clone(),
        link: o
            .link
            .as_deref()
            .filter(|_| live && !o.state.is_terminal())
            .map(str::to_owned),
        expires_at: o.expires_at,
        verification_code: o.code.clone().filter(|_| state == C::AwaitingConfirmation),
        request_id: o
            .request_digest
            .clone()
            .filter(|_| state == C::AwaitingConfirmation),
        state,
    })
}

/// What the restricted device shows during the exchange.
pub fn approval_join_view(
    a: &crate::state::OwnerAttemptRec,
    now_ms: u64,
) -> Option<crate::api::RecoveryApprovalView> {
    use crate::api::ApprovalCode as C;
    let x = a.approval.as_ref()?;
    let live = x.expires_at > now_ms;
    let state = match x.state {
        LinkState::AwaitingApproval if live => C::WaitingForController,
        LinkState::Transferring if live => C::Receiving,
        LinkState::Linked => C::Done,
        LinkState::Failed | LinkState::Rejected => C::Failed,
        _ => C::Expired,
    };
    Some(crate::api::RecoveryApprovalView {
        state,
        verification_code: (state == C::WaitingForController || state == C::Receiving)
            .then(|| x.code.clone()),
        expires_at: x.expires_at,
    })
}

/// Did the account verify this credential body as `target` (used by tests of the binding).
#[allow(dead_code)]
fn cred_hash(body: &[u8]) -> Option<[u8; 32]> {
    Credential::from_body(body).ok().map(|c| c.hash())
}
