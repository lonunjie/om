//! The owner's recovery attempt (ADR-0202 sections 1 and 2, ADR-0190 sections 3.1 to 3.3): a fresh device that holds the
//! key sheet and has read the account's record asks for the account back. Everything here is persisted before use.
//!
//! * `RequestRecovery` stages a tentative device OUTSIDE `st.device`, builds the exact signed request and the
//!   recovery-signed successor, and delivers the request to every Home the record names (one outbox call per Home) and a
//!   `RecoveryNotice` to the contacts the record's snapshot can reach, resent at 1/3 and 2/3 of the interval.
//! * The interval is this device's OWN: monotonic time the host reports inside one boot session (`recovery_watch::credit`).
//!   Home timestamps, wall clocks and request fields are never time. Losing the record restarts it.
//! * Vetoes are read from every Home's board and verified against the pinned parent head; an observed valid veto is
//!   terminal and survives a Home that prunes it.
//! * Adoption re-reads every Home (the record and the board), re-verifies the evidence from genesis, refuses on any
//!   known newer, divergent or vetoed state, and installs the device, the extended chain, the evidence and the rotated
//!   policy in one transaction. Without an independently held pin (`EvidenceStage::Pinned`) nothing is ever adopted.

use super::*;
use crate::crypto::binding::Credential;
use crate::crypto::delivery::{self as dlv, KeyMaterial};
use crate::flows::recovery_watch as watch;
use crate::host::MonotonicSample;
use crate::recovery::{self, RecoveryPolicy, RecoveryRequest, RecoveryRoot};
use crate::recovery_record::{self as rr, Plain, RecordKeys};
use crate::route::RelayEntry;

/// What a read answers: its bytes, if any, and its list items.
pub(crate) type ReadAnswer = (Option<Vec<u8>>, Vec<Vec<u8>>);

/// Credit between two veto-board reads while waiting.
const POLL_CREDIT_MS: u64 = 10 * 60 * 1000;

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    fn attempt(&self) -> Option<&OwnerAttemptRec> {
        self.st
            .app
            .recovery_join
            .as_ref()
            .and_then(|j| j.attempt.as_ref())
    }

    fn attempt_mut(&mut self) -> Option<&mut OwnerAttemptRec> {
        self.st
            .app
            .recovery_join
            .as_mut()
            .and_then(|j| j.attempt.as_mut())
    }

    /// The parent head the attempt is bound to, re-verified from the stored evidence (from genesis, with no recovery
    /// link evidence: a chain that holds a recovery link this device cannot check does not verify here), its
    /// credentials and the policy the head commits to. `None` when any of it no longer verifies.
    fn attempt_ctx(&self) -> Option<(Acs, Vec<Vec<u8>>, RecoveryPolicy)> {
        let a = self.attempt()?;
        let chain: Vec<Vec<u8>> = a
            .chain
            .iter()
            .map(|c| unhex(c).ok())
            .collect::<Option<_>>()?;
        let creds: Vec<Vec<u8>> = a
            .creds
            .iter()
            .map(|c| unhex(c).ok())
            .collect::<Option<_>>()?;
        let head = control::verify_chain_with(&chain, &creds, &[]).ok()?;
        let policy = RecoveryPolicy::from_body(&unhex(&a.policy).ok()?).ok()?;
        (hex(&head.hash()) == a.parent && head.recovery_commitment == Some(policy.hash()))
            .then_some((head, creds, policy))
    }

    // --------------------------------------------------------------------------------------------- intents

    /// `RequestRecovery`: the person's explicit action (ADR-0202 section 1). It needs the record read and checked. A
    /// device with no independently held pin still prepares and delivers the request (so controllers and contacts can
    /// see it), and stays restricted: adoption needs the pin.
    pub(super) fn request_recovery(&mut self) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(j) = self.st.app.recovery_join.clone() else {
            return rejected(RejectReason::NotFound);
        };
        if self.st.device.is_some() || j.attempt.is_some() {
            return rejected(RejectReason::Conflict);
        }
        if j.stage != JoinStage::Found || j.evidence == EvidenceStage::Conflict {
            return rejected(RejectReason::StaleState);
        }
        let Some(plain) = j.record.as_deref().and_then(|r| unhex(r).ok()) else {
            return rejected(RejectReason::StaleState);
        };
        let Ok(Plain::Record(record)) = rr::read_plaintext(&plain) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let Ok(v) = record.verify() else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let Some(root) = unhex(&j.root)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .map(RecoveryRoot::from_bytes)
        else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let (head, policy) = (v.head.clone(), v.policy.clone());
        let epoch = policy.epoch();
        if policy.recovery_key() != Some(root.auth_public_key(epoch).as_slice())
            || policy.state != recovery::STATE_ON
        {
            return rejected(RejectReason::Unauthorized);
        }
        // The tentative device: its credential is built with the account the record names.
        let mut dev = crate::crypto::store::Device::new(self.rng);
        dev.account_id = head.account_id;
        dev.acs_generation = head.generation + 1;
        let cred = dev.credential();
        let req = RecoveryRequest {
            account_id: head.account_id,
            recovery_epoch: epoch,
            new_device_id: dev.device_id,
            new_device_credential: cred.hash(),
            parent_state_hash: head.hash(),
            request_nonce: self.rand16(),
        };
        let signed = recovery::sign_request(&req, &root.auth_signer(epoch), self.rng);
        // The successor: this device the sole controller, every earlier device revoked, the recovery policy rotated.
        let mut next = RecoveryPolicy::default_for(root.auth_public_key(epoch + 1), epoch + 1);
        next.delay_seconds = policy.delay_seconds;
        let mut child = head.clone();
        child.generation = head.generation + 1;
        child.previous = Some(head.hash());
        let mut revoked = head.revoked.clone();
        revoked.extend(head.control.iter().chain(head.messaging.iter()));
        child.control = std::collections::BTreeSet::from([dev.device_id]);
        child.quarantined.clear();
        child.messaging.clear();
        child.revoked = revoked;
        child.credentials = vec![(dev.device_id, cred.hash())];
        child.key_epochs.clear();
        child.recovery_commitment = Some(next.hash());
        let signed_child = control::sign_acs_recovery(
            &child.body(),
            &child.account_id,
            &root.auth_signer(epoch),
            &[],
            self.rng,
        );
        let bundle = recovery::SuccessorBundle {
            child: signed_child,
            next_policy: next.body(),
            credential: cred.body(),
        };
        // Where the request goes: the Home the record was read from, and every provider Home the record names.
        let mut homes: Vec<JoinPlace> = vec![];
        let mut add = |h: JoinPlace| {
            if !homes.iter().any(|x| x.host == h.host && x.port == h.port) {
                homes.push(h);
            }
        };
        if let Some(p) = j.place.clone() {
            add(p);
        }
        for p in &record.providers {
            add(JoinPlace {
                host: p.host.clone(),
                port: p.port,
                scope: p.scope.map(|s| hex(&s)).unwrap_or_default(),
            });
        }
        let chain: Vec<Hex> = record
            .evidence
            .tail
            .iter()
            .chain(std::iter::once(&record.evidence.head))
            .map(|s| hex(s))
            .collect();
        let digest = hex(&req.digest());
        let notice = crate::flows::recovery_peer::build_notice(
            &policy.body(),
            &signed,
            &chain
                .iter()
                .filter_map(|c| unhex(c).ok())
                .collect::<Vec<_>>(),
            &record.evidence.credentials,
            Some(&bundle),
            &[],
        );
        let attempt = OwnerAttemptRec {
            device: hex(&dev.to_bytes()).into(),
            obs: RecoveryObsRec {
                digest,
                request: hex(&signed),
                first_seen_ms: self.now_ms,
                boot_session: String::new(),
                last_mono_ms: 0,
                credited_ms: 0,
                vetoes: vec![],
                own_veto: false,
                successor: Some(RecoverySuccessorRec {
                    child: hex(&bundle.child),
                    next_policy: hex(&bundle.next_policy),
                    credential: hex(&bundle.credential),
                }),
                state: RecoveryObsState::Waiting,
            },
            state: AttemptState::Waiting,
            parent: hex(&head.hash()),
            parent_generation: head.generation,
            epoch,
            target: hex(&cred.hash()),
            chain,
            creds: record.evidence.credentials.iter().map(|c| hex(c)).collect(),
            policy: hex(&policy.body()),
            homes,
            notice: notice.as_deref().map(hex),
            resends: 0,
            polled_ms: 0,
            round: 0,
            pending_reads: 0,
            reconciling: false,
            round_conflict: false,
        };
        if let Some(j) = self.st.app.recovery_join.as_mut() {
            j.attempt = Some(attempt);
        }
        self.owner_post_all();
        OutcomeClass::Committed
    }

    /// `CancelRecovery`: closes the local attempt. The signed request stays valid at the Homes until a verifier moves on;
    /// nothing here claims otherwise.
    pub(super) fn cancel_recovery(&mut self) -> OutcomeClass {
        match self.attempt_mut() {
            Some(a) if a.state == AttemptState::Waiting => {
                a.state = AttemptState::Cancelled;
                a.reconciling = false;
                OutcomeClass::Committed
            }
            _ => OutcomeClass::Rejected {
                reason: RejectReason::NotFound,
            },
        }
    }

    /// `CheckRecoveryAgain`: ask again now. Before an attempt it re-reads the record (the floor decides what a newer,
    /// the same or an older answer means); during one it runs a veto and record round at every Home.
    pub(super) fn check_recovery_again(&mut self) -> OutcomeClass {
        let waiting = self
            .attempt()
            .is_some_and(|a| a.state == AttemptState::Waiting);
        if waiting {
            self.owner_round(false);
            return OutcomeClass::Committed;
        }
        if self.attempt().is_some() {
            return OutcomeClass::Rejected {
                reason: RejectReason::Conflict,
            };
        }
        if self.st.app.recovery_join.is_none() {
            return OutcomeClass::Rejected {
                reason: RejectReason::NotFound,
            };
        }
        self.recovery_find_start();
        OutcomeClass::Committed
    }

    // ------------------------------------------------------------------------------------------- delivery

    /// Deliver the exact signed request to every Home and the notice to the reachable contacts. A retry reuses the same
    /// bytes: nothing here resets the timer or makes a second attempt.
    fn owner_post_all(&mut self) {
        let Some(a) = self.attempt().cloned() else {
            return;
        };
        let Ok(req) = unhex(&a.obs.request) else {
            return;
        };
        for h in &a.homes {
            self.enqueue(
                NetCall::RecoveryRequestPost {
                    host: h.host.clone(),
                    port: h.port,
                    bytes: req.clone(),
                },
                Purpose::OwnerRecoveryPost,
            );
        }
        self.owner_send_notices();
    }

    /// The notice to each contact of the record's snapshot, over a route and key revalidated now (ADR-0199 section 7):
    /// a key that expired or is not bound to its route is skipped, never sent in the clear.
    fn owner_send_notices(&mut self) {
        let (Some(j), Some(a)) = (self.st.app.recovery_join.clone(), self.attempt().cloned())
        else {
            return;
        };
        let (Some(notice), Some(plain)) = (
            a.notice.as_deref().and_then(|n| unhex(n).ok()),
            j.record.as_deref().and_then(|r| unhex(r).ok()),
        ) else {
            return;
        };
        let Ok(Plain::Record(record)) = rr::read_plaintext(&plain) else {
            return;
        };
        let rr::Relationships::Plain(snapshot) = &record.relationships else {
            return;
        };
        let now_s = self.now_s();
        for e in &snapshot.entries {
            let routes: Vec<RelayEntry> = e.routes.iter().filter_map(RelayEntry::from_v).collect();
            let target = e.targets.iter().find_map(|(_, kv)| {
                let km = KeyMaterial::from_v(kv).ok()?;
                (km.expires_at > now_s).then_some(km)
            });
            let Some(km) = target else { continue };
            let Some(route) = routes.into_iter().find(|r| {
                (r.expires_at == 0 || r.expires_at > now_s) && r.binding() == km.route_binding
            }) else {
                continue;
            };
            let expires = (now_s + WRAPPER_TTL_S).min(km.expires_at);
            let Ok(wrapper) = dlv::seal(
                &km.public_key,
                &km.key_id,
                &route.relay_uri,
                &route.capability,
                expires,
                &notice,
                self.rng,
            ) else {
                continue;
            };
            self.enqueue(deposit_call(&route, wrapper), Purpose::Ack);
        }
    }

    /// Start a round of reads: the veto board at every Home and, when `with_record`, the record again at every Home that
    /// has a scope (the reconcile before adoption). Only the round that is current counts.
    fn owner_round(&mut self, with_record: bool) {
        let Some(j) = self.st.app.recovery_join.clone() else {
            return;
        };
        let Some(a) = j.attempt.clone() else { return };
        let Some(root) = unhex(&j.root)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
        else {
            return;
        };
        let Ok(digest) = unhex(&a.obs.digest) else {
            return;
        };
        let round = a.round + 1;
        let mut pending = 0u32;
        for h in &a.homes {
            self.enqueue(
                NetCall::RecoveryVetoRead {
                    host: h.host.clone(),
                    port: h.port,
                    request_digest: digest.clone(),
                },
                Purpose::OwnerRecoveryRead { kind: 1, round },
            );
            pending += 1;
            if with_record
                && let Some(scope) = unhex(&h.scope)
                    .ok()
                    .and_then(|b| <[u8; 32]>::try_from(b).ok())
            {
                let keys = RecordKeys::derive(&root, &scope);
                self.enqueue(
                    NetCall::RecoveryGet {
                        host: h.host.clone(),
                        port: h.port,
                        locator: rr::locator_of(&keys.read_cap).to_vec(),
                        read_cap: keys.read_cap.to_vec(),
                    },
                    Purpose::OwnerRecoveryRead { kind: 0, round },
                );
                pending += 1;
            }
        }
        if let Some(a) = self.attempt_mut() {
            a.round = round;
            a.pending_reads = pending;
            a.round_conflict = false;
        }
    }

    // ----------------------------------------------------------------------------------------------- reads

    /// One read of the current round came back (`None`: refused or unreachable for good; it supplies no evidence).
    pub(crate) fn owner_read_done(&mut self, kind: u8, round: u32, got: Option<ReadAnswer>) {
        let Some(a) = self.attempt() else { return };
        if a.round != round || a.pending_reads == 0 {
            return; // an answer to an earlier round counts for nothing
        }
        if let Some((_, items)) = got {
            if kind == 1 {
                for v in items {
                    self.owner_observe_veto(&v);
                }
            } else if let [_etag, envelope] = items.as_slice() {
                self.owner_observe_record(envelope);
            }
        }
        if let Some(a) = self.attempt_mut() {
            a.pending_reads = a.pending_reads.saturating_sub(1);
        }
        self.owner_evaluate();
    }

    /// A veto from a board. Only a veto by a live controller of the pinned parent head, under the credential that head
    /// commits to, for THIS request and epoch counts; everything else is ignored.
    fn owner_observe_veto(&mut self, signed: &[u8]) {
        let Some((head, creds, policy)) = self.attempt_ctx() else {
            return;
        };
        let Ok(id) = recovery::veto_signer(signed) else {
            return;
        };
        let Some(key) = watch::live(&head, &id)
            .then(|| watch::committed_key(&head, &creds, &id))
            .flatten()
        else {
            return;
        };
        let Ok(veto) = recovery::open_veto(signed, &id, &key) else {
            return;
        };
        let Some(a) = self.attempt_mut() else { return };
        if veto.account_id != head.account_id
            || veto.recovery_epoch != policy.epoch()
            || hex(&veto.request_digest) != a.obs.digest
            || a.obs.vetoes.len() >= 16
        {
            return;
        }
        let raw = hex(signed);
        if !a.obs.vetoes.contains(&raw) {
            a.obs.vetoes.push(raw);
        }
        a.obs.state = RecoveryObsState::Vetoed;
        if a.state == AttemptState::Waiting {
            a.state = AttemptState::Vetoed; // terminal, kept even if the Home prunes its copy
        }
    }

    /// The record read again before adoption. Same bytes: affirmative. An older generation: no evidence. A different
    /// lineage or other bytes at the same generation: a conflict. A newer head that verifies from the pinned parent
    /// supersedes the request (it ends); one that does not verify keeps the device restricted for this round.
    fn owner_observe_record(&mut self, envelope: &[u8]) {
        let Some(j) = self.st.app.recovery_join.clone() else {
            return;
        };
        let (Some(root), Some(place)) = (
            unhex(&j.root)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok()),
            j.place.clone(),
        ) else {
            return;
        };
        // The record at a Home other than the first one is keyed by that Home's own scope; this device only re-reads
        // the places it knows a scope for, and opens each with that scope.
        let scopes: Vec<[u8; 32]> = std::iter::once(place.scope.clone())
            .chain(
                j.attempt
                    .iter()
                    .flat_map(|a| a.homes.iter().map(|h| h.scope.clone())),
            )
            .filter_map(|s| unhex(&s).ok().and_then(|b| <[u8; 32]>::try_from(b).ok()))
            .collect();
        for scope in scopes {
            let keys = RecordKeys::derive(&root, &scope);
            let Ok(plain) = rr::open_envelope(&keys, &scope, envelope) else {
                continue;
            };
            let Ok(Plain::Record(r)) = rr::read_plaintext(&plain) else {
                continue;
            };
            let Ok(v) = r.verify() else { continue };
            let lineage = hex(&v.head.account_id);
            match self.apply_floor(&hex(&scope), &lineage, r.generation, &plain) {
                Ok(()) => {}
                Err(JoinStage::OutOfDate) => return, // an old copy: no evidence either way
                Err(_) => {
                    if let Some(a) = self.attempt_mut() {
                        a.round_conflict = true;
                    }
                    return;
                }
            }
            let Some(a) = self.attempt().cloned() else {
                return;
            };
            if hex(&v.head.hash()) == a.parent {
                return; // the same head the request names: unchanged
            }
            // A different head. It supersedes the request only when it verifies as an extension of the parent.
            let Some((parent, creds, _)) = self.attempt_ctx() else {
                return;
            };
            let later: Vec<Vec<u8>> = r
                .evidence
                .tail
                .iter()
                .chain(std::iter::once(&r.evidence.head))
                .filter(|s| {
                    rr::signed_body(s)
                        .ok()
                        .and_then(|b| Acs::from_body(&b).ok())
                        .is_some_and(|x| x.generation > a.parent_generation)
                })
                .cloned()
                .collect();
            let mut all = creds.clone();
            all.extend(r.evidence.credentials.iter().cloned());
            let extends = !later.is_empty()
                && control::verify_extension_partial(parent, &later, 1, &all, &[]).is_ok();
            if let Some(a) = self.attempt_mut() {
                if extends {
                    a.state = AttemptState::Stale;
                    a.obs.state = RecoveryObsState::Stale;
                } else {
                    a.round_conflict = true;
                }
            }
            return;
        }
    }

    // ------------------------------------------------------------------------------------------- the timer

    /// Credit the host's reading, resend at 1/3 and 2/3, poll the boards, and when the interval has run start the
    /// reconcile round that precedes adoption.
    pub(crate) fn owner_tick(&mut self, mono: &MonotonicSample) {
        if self
            .attempt()
            .is_none_or(|a| a.state != AttemptState::Waiting)
        {
            return;
        }
        let Some(a) = self.attempt_mut() else { return };
        watch::credit(&mut a.obs, mono);
        let (credited, resends, polled, pending, reconciling) = (
            a.obs.credited_ms,
            a.resends,
            a.polled_ms,
            a.pending_reads,
            a.reconciling,
        );
        let delay_ms = self
            .attempt_ctx()
            .map_or(recovery::DEFAULT_DELAY_SECONDS, |(_, _, p)| p.delay_seconds)
            * 1000;
        let due = |n: u64| credited >= delay_ms / 3 * n && delay_ms > 0;
        if (resends == 0 && due(1)) || (resends == 1 && due(2)) {
            if let Some(a) = self.attempt_mut() {
                a.resends = resends + 1;
            }
            self.owner_post_all();
        }
        if pending == 0 && !reconciling {
            if self.owner_complete_ready() {
                if let Some(a) = self.attempt_mut() {
                    a.reconciling = true;
                }
                self.owner_round(true);
            } else if credited.saturating_sub(polled) >= POLL_CREDIT_MS {
                if let Some(a) = self.attempt_mut() {
                    a.polled_ms = credited;
                }
                self.owner_round(false);
            }
        }
    }

    /// Would the held successor verify as complete right now: this device's own interval run, no veto, and the pin held.
    fn owner_complete_ready(&self) -> bool {
        let (Some(j), Some(a), Some((head, creds, policy))) = (
            self.st.app.recovery_join.as_ref(),
            self.attempt(),
            self.attempt_ctx(),
        ) else {
            return false;
        };
        let Some(held) = a.obs.successor.clone() else {
            return false;
        };
        j.evidence == EvidenceStage::Pinned
            && matches!(
                watch::verify_held(&head, &creds, &policy, &a.obs, &held),
                Ok(watch::Held::Complete)
            )
    }

    /// After a round: a veto or a supersession ends the attempt; otherwise, a finished reconcile round with no conflict
    /// adopts the successor.
    fn owner_evaluate(&mut self) {
        let Some(a) = self.attempt().cloned() else {
            return;
        };
        if a.state != AttemptState::Waiting || a.pending_reads > 0 {
            return;
        }
        if a.reconciling {
            if let Some(a) = self.attempt_mut() {
                a.reconciling = false;
            }
            if a.round_conflict || !self.owner_complete_ready() {
                return;
            }
            self.owner_adopt();
        }
    }

    // ---------------------------------------------------------------------------------------------- adoption

    /// Install the account: the tentative device becomes `st.device`, the chain gains the verified successor with the
    /// evidence for its recovery link, the recovery policy rotates and the old devices are revoked by the successor's
    /// own shape. All of it is checked first and applied in this one transaction; any failure changes nothing.
    fn owner_adopt(&mut self) -> bool {
        let (Some(j), Some(a), Some((head, creds, policy))) = (
            self.st.app.recovery_join.clone(),
            self.attempt().cloned(),
            self.attempt_ctx(),
        ) else {
            return false;
        };
        let Some(held) = a.obs.successor.clone() else {
            return false;
        };
        let (Ok(req_bytes), Ok(child), Ok(next_body), Ok(cred_body)) = (
            unhex(&a.obs.request),
            unhex(&held.child),
            unhex(&held.next_policy),
            unhex(&held.credential),
        ) else {
            return false;
        };
        // Re-validate the request against the parent and the target once more (V-002).
        let Ok(req) = recovery::open_request(&req_bytes, policy.recovery_key().unwrap_or_default())
        else {
            return false;
        };
        if watch::is_stale(&head, &policy, &req) || j.evidence != EvidenceStage::Pinned {
            return false;
        }
        if !matches!(
            watch::verify_held(&head, &creds, &policy, &a.obs, &held),
            Ok(watch::Held::Complete)
        ) {
            return false;
        }
        let Some(child_hash) = cbor::decode(&child)
            .ok()
            .and_then(|v| Acs::from_body(v.get(1).ok()?.b().ok()?).ok())
            .map(|c| c.hash())
        else {
            return false;
        };
        let link = RecoveryLinkRec {
            child_hash: hex(&child_hash),
            policy: a.policy.clone(),
            next_policy: held.next_policy.clone(),
            request: a.obs.request.clone(),
            elapsed_seconds: a.obs.credited_ms / 1000,
        };
        let mut chain = a.chain.clone();
        chain.push(held.child.clone());
        let mut all_creds = a.creds.clone();
        if !all_creds.contains(&held.credential) {
            all_creds.push(held.credential.clone());
        }
        let verify = |chain: &[Hex], creds: &[Hex], links: &[RecoveryLinkRec]| {
            let c: Option<Vec<Vec<u8>>> = chain.iter().map(|x| unhex(x).ok()).collect();
            let d: Option<Vec<Vec<u8>>> = creds.iter().map(|x| unhex(x).ok()).collect();
            let ev: Vec<control::RecoveryLinkEvidence> = links
                .iter()
                .filter_map(|l| {
                    Some(control::RecoveryLinkEvidence {
                        child_hash: unhex(&l.child_hash).ok()?.try_into().ok()?,
                        policy: unhex(&l.policy).ok()?,
                        next_policy: unhex(&l.next_policy).ok()?,
                        request: unhex(&l.request).ok()?,
                        elapsed_seconds: l.elapsed_seconds,
                    })
                })
                .collect();
            control::verify_chain_with(&c?, &d?, &ev).ok()
        };
        let links = vec![link];
        let Some(new_head) = verify(&chain, &all_creds, &links).filter(|h| h.hash() == child_hash)
        else {
            return false;
        };
        let Some(mut dev) = unhex(&a.device)
            .ok()
            .and_then(|b| crate::crypto::store::Device::from_bytes(&b).ok())
        else {
            return false;
        };
        // The device's credential must be exactly what the verified head commits to.
        let mine = Credential::from_body(&cred_body).ok();
        if mine.as_ref().map(Credential::hash) != Some(dev.credential().hash())
            || !new_head.control.contains(&dev.device_id)
            || hex(&dev.credential().hash()) != a.target
        {
            return false;
        }
        // Verified: apply.
        let now_s = self.now_s();
        dev.account_id = new_head.account_id;
        dev.acs_generation = new_head.generation;
        dev.bundle_expires_at = now_s + BUNDLE_TTL_S;
        dev.rotation_at = now_s;
        let up = dev.generate_one_time(ONE_TIME_KEYS, ONE_TIME_KEYS, self.rng);
        let call = NetCall::PublishKeyMaterial {
            device_id: dev.device_id.to_vec(),
            bundle: up.bundle,
            opks: up.opks,
            pqopks: up.pqopks,
        };
        self.st.app.device_meta.insert(
            hex(&dev.device_id),
            DeviceMeta {
                label: "This device".into(),
                enrolled_at: self.now_ms,
            },
        );
        self.st.app.acs_chain = chain;
        self.st.app.acs_credentials = all_creds;
        self.st.app.recovery_links = links;
        self.st.app.recovery_policy = Some(held.next_policy.clone());
        self.st.app.recovery_key_saved = true;
        self.st.app.created_at = self.now_ms;
        // The key sheet is the person's own: this device holds the root, so it can keep the record current again.
        if let (Some(place), Some(root)) = (
            j.place.as_ref(),
            unhex(&j.root)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok()),
        ) && let Some(scope) = unhex(&place.scope)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
        {
            let kit = rr::RecoveryKit::make(
                &root,
                scope,
                &place.host,
                place.port,
                RecoveryPolicy::from_body(&next_body)
                    .map(|p| p.epoch())
                    .unwrap_or(a.epoch + 1),
                arr32(self.rng),
                true,
            );
            self.st.app.recovery_kit = Some(SecretHex::from(hex(&kit.to_bytes())));
            self.st.app.recovery_writer.enabled = true;
            self.st.app.recovery_writer.authored = true;
        }
        self.st.device = Some(dev);
        self.st.app.recovery_join = None;
        self.enqueue(call, Purpose::PublishKeys);
        true
    }
}
