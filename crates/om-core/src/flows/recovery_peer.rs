//! The contact side of recovery (ADR-0190): how a contact verifies a recovery-only successor of a peer's account.
//!
//! Evidence travels (a `RecoveryNotice`, or the dossier that rides with a chain), time never does. Every contact is a
//! verifier with its own durable record per relationship: its interval starts at ITS first valid observation, is
//! credited only from the host's monotonic readings (`recovery_watch::credit`, unchanged), and is never read from a
//! message, a Home, a custodian or a peer. Nothing here has a time field, and an unknown or time-like key is refused.
//!
//! Nothing is created until the request's signature has verified under the recovery key of a policy that the contact's
//! own pinned head commits to. The contact's pinned head is replaced only by a successor that verifies like a
//! completion with this verifier's own interval (`verify_recovery_successor`), and messages from the new device wait,
//! unprocessed, until then (ADR-0190 section 5).

use super::multi::{PeerPlan, chain_states};
use super::recovery_watch::{
    committed_key, credit, is_stale, live, verify_held, verify_successor_for,
};
use super::*;
use crate::crypto::binding::Credential;
use crate::host::MonotonicSample;
use crate::recovery::{self, RecoveryPolicy, SuccessorBundle};
use om_codec::Value;

/// In-session kind of a veto pushed by a live controller (ADR-0190 3.3): `{0: 12, 1: signed-RecoveryVeto}`.
pub(super) const KIND_RECOVERY_VETO: u64 = 12;
/// Largest `RecoveryNotice` (the registry bound; checked before any signature).
pub const MAX_NOTICE_BYTES: usize = 131_068;
/// Recoveries tracked per relationship (ADR-0190 section 4).
const MAX_PEER_RECOVERIES: usize = 2;
const MAX_VETOES: usize = 16;
/// A chain with more recovery links than this, or a dossier that does not match its links, is refused (section 8).
const MAX_DOSSIER_LINKS: usize = 4;

/// What a notice (or a dossier entry with its chain) hands the contact, before any of it is trusted.
pub(super) struct NoticeInput {
    policy: Vec<u8>,
    request: Vec<u8>,
    bridge: Vec<Vec<u8>>,
    creds: Vec<Vec<u8>>,
    /// `(child, next_policy, credential)` of the recovery-signed successor.
    successor: Option<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    vetoes: Vec<Vec<u8>>,
}

fn vget(v: &Value, k: u64) -> Option<&Value> {
    match v {
        Value::Map(m) => m
            .iter()
            .find(|(a, _)| matches!(a, Value::Uint(n) if *n == k))
            .map(|(_, b)| b),
        _ => None,
    }
}

fn vbytes(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Bytes(b) => Some(b.clone()),
        _ => None,
    }
}

fn blist(v: Option<&Value>) -> Option<Vec<Vec<u8>>> {
    match v {
        None => Some(vec![]),
        Some(Value::Array(a)) => a.iter().map(vbytes).collect(),
        Some(_) => None,
    }
}

/// A name that says or implies time. Core never reads time from an extension; a notice that names one is refused.
fn time_like(name: &str) -> bool {
    const SUB: [&str; 14] = [
        "time", "elapsed", "delay", "second", "millis", "clock", "expir", "since", "until",
        "deadline", "duration", "epoch", "interval", "remain",
    ];
    const TOK: [&str; 10] = [
        "ts", "ms", "sec", "secs", "at", "age", "date", "utc", "unix", "wait",
    ];
    let l = name.to_ascii_lowercase();
    SUB.iter().any(|s| l.contains(s))
        || l.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|t| TOK.contains(&t))
}

fn ext_has_time(v: &Value) -> bool {
    match v {
        Value::Map(m) => m
            .iter()
            .any(|(k, val)| matches!(k, Value::Text(t) if time_like(t)) || ext_has_time(val)),
        Value::Array(a) => a.iter().any(ext_has_time),
        _ => false,
    }
}

/// Parse a `RecoveryNotice` with every bound and the strict codec checked first. `None` for anything else.
pub(super) fn parse_notice(bytes: &[u8]) -> Option<NoticeInput> {
    if bytes.len() > MAX_NOTICE_BYTES {
        return None;
    }
    let v = registry().check("RecoveryNotice", bytes, false).ok()?;
    if vget(&v, 23).is_some_and(ext_has_time) {
        return None; // fail closed: no time, elapsed time or delay is ever taken from an extension
    }
    let successor = match vget(&v, 6) {
        None => None,
        Some(s) => Some((
            vbytes(vget(s, 0)?)?,
            vbytes(vget(s, 1)?)?,
            vbytes(vget(s, 2)?)?,
        )),
    };
    Some(NoticeInput {
        policy: vbytes(vget(&v, 2)?)?,
        request: vbytes(vget(&v, 3)?)?,
        bridge: blist(vget(&v, 4))?,
        creds: blist(vget(&v, 5))?,
        successor,
        vetoes: blist(vget(&v, 7))?,
    })
}

/// The `RecoveryNotice` bytes for a request (ADR-0190 section 2): the policy body it commits to, the request, the
/// bridge states ending at its parent, the credentials those need, and optionally the recovery-signed successor. The
/// bridge and credentials are dropped when the whole would exceed the bound.
pub fn build_notice(
    policy: &[u8],
    request: &[u8],
    bridge: &[Vec<u8>],
    creds: &[Vec<u8>],
    successor: Option<&SuccessorBundle>,
    vetoes: &[Vec<u8>],
) -> Option<Vec<u8>> {
    let make = |bridge: &[Vec<u8>], creds: &[Vec<u8>]| {
        let list = |l: &[Vec<u8>]| V::A(l.iter().cloned().map(V::B).collect());
        let mut f = vec![
            (0, V::U(34)),
            (1, V::U(1)),
            (2, V::B(policy.to_vec())),
            (3, V::B(request.to_vec())),
        ];
        if !bridge.is_empty() {
            f.push((4, list(bridge)));
        }
        if !creds.is_empty() {
            f.push((5, list(creds)));
        }
        if let Some(s) = successor {
            f.push((
                6,
                V::M(vec![
                    (0, V::B(s.child.clone())),
                    (1, V::B(s.next_policy.clone())),
                    (2, V::B(s.credential.clone())),
                ]),
            ));
        }
        if !vetoes.is_empty() {
            f.push((7, list(vetoes)));
        }
        cbor::encode(&V::M(f))
    };
    let take = bridge.len().min(8);
    let bridge = &bridge[bridge.len() - take..];
    let creds = &creds[..creds.len().min(16)];
    let full = make(bridge, creds);
    if full.len() <= MAX_NOTICE_BYTES {
        return Some(full);
    }
    let bare = make(&[], &[]);
    (bare.len() <= MAX_NOTICE_BYTES).then_some(bare)
}

/// The dossier of a first-contact or head-update payload (`key 5` / `key 8`): `None` when malformed or longer than
/// `MAX_DOSSIER_LINKS`, an empty list when absent.
pub(super) fn parse_dossier(v: Option<&V>) -> Option<Vec<control::RecoveryDossierEntry>> {
    let Some(v) = v else {
        return Some(vec![]);
    };
    let list = v.a().ok()?;
    if list.len() > MAX_DOSSIER_LINKS {
        return None;
    }
    list.iter()
        .map(|e| {
            let d = e.get(0).ok()?;
            if d.get(0).and_then(|a| a.u()).ok()? != 1 {
                return None;
            }
            Some(control::RecoveryDossierEntry {
                child_hash: d.get(1).and_then(|h| h.b32()).ok()?,
                policy: e.get(1).and_then(|b| b.b()).ok()?.to_vec(),
                next_policy: e.get(2).and_then(|b| b.b()).ok()?.to_vec(),
                request: e.get(3).and_then(|b| b.b()).ok()?.to_vec(),
            })
        })
        .collect()
}

/// First contact with an already-recovered account follows the normal first-contact rules (ADR-0190 ruling 1): there
/// is no earlier trust to protect, so the dossier's links replay with the policy's own delay as their interval. The
/// links still have to verify completely (policy commitment, request, shape); nothing is trusted that does not.
pub(super) fn first_contact_evidence(
    dossier: &[control::RecoveryDossierEntry],
) -> Option<Vec<control::RecoveryLinkEvidence>> {
    dossier
        .iter()
        .map(|d| {
            let p = RecoveryPolicy::from_body(&d.policy).ok()?;
            Some(control::RecoveryLinkEvidence {
                child_hash: d.child_hash,
                policy: d.policy.clone(),
                next_policy: d.next_policy.clone(),
                request: d.request.clone(),
                elapsed_seconds: p.delay_seconds,
            })
        })
        .collect()
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// The dossier of `chain` as wire values (never seconds).
    pub(super) fn dossier_value(&self, chain: &[Vec<u8>]) -> Vec<V> {
        control::dossier_of(chain, &self.st.app.link_evidence())
            .into_iter()
            .map(|d| {
                V::M(vec![
                    (0, digest(&d.child_hash)),
                    (1, V::B(d.policy)),
                    (2, V::B(d.next_policy)),
                    (3, V::B(d.request)),
                ])
            })
            .collect()
    }

    /// A known account keeps its contact-local verification across a fresh invitation.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_reconnect_binding(
        &mut self,
        fresh: &mut Relationship,
        head: &Acs,
        chain: &[Vec<u8>],
        creds: &[Vec<u8>],
        dossier: Option<&V>,
        wrapper: &[u8],
    ) -> bool {
        if fresh
            .reconnect_account
            .as_ref()
            .is_some_and(|account| *account != fresh.peer_account_id)
        {
            return false;
        }
        let Some(old) = self
            .st
            .app
            .relationships
            .values()
            .find(|r| {
                r.id != fresh.id
                    && r.state == RelState::Active
                    && r.peer_account_id == fresh.peer_account_id
            })
            .cloned()
        else {
            return true;
        };
        if old.blocked {
            return false;
        }
        let Some((pinned, _)) = self.peer_pinned(&old.id) else {
            return false;
        };
        let Some(states) = chain_states(chain) else {
            return false;
        };
        let Some(at) = states.iter().position(|s| s.hash() == pinned.hash()) else {
            return false; // stale or unrelated to the trusted head
        };
        let Some(plan) = Self::peer_plan(&old, &chain[at + 1..], creds) else {
            return false;
        };
        if plan.head.hash() != head.hash() {
            // Verify and observe the recovery against the original pin. The new invitation cannot credit its timer.
            let Some(dossier) = dossier else { return false };
            let _ = self.peer_commit(&old.id, plan);
            self.observe_dossier(&old.id, &V::M(vec![(5, dossier.clone())]), chain, creds);
            if self
                .st
                .app
                .relationships
                .get(&old.id)
                .is_some_and(|r| r.peer_recovery.iter().any(PeerRecoveryRec::is_live))
                && let Some(r) = self.st.app.relationships.get_mut(&fresh.id)
                && r.recovery_bind.is_none()
            {
                r.recovery_bind = Some(RecoveryBinding {
                    account_id: fresh.peer_account_id.clone(),
                    bytes: hex(wrapper),
                });
            }
            return false;
        }
        let accepted_target = old.peer_recovery.iter().rev().any(|r| {
            r.state == PeerRecoveryState::Accepted
                && r.obs
                    .successor
                    .as_ref()
                    .and_then(|s| unhex(&s.credential).ok())
                    .and_then(|b| Credential::from_body(&b).ok())
                    .is_some_and(|c| hex(&c.device_id) == fresh.peer_device_id)
        });
        if accepted_target && old.peer_device_id != fresh.peer_device_id {
            fresh.relink = Some(RelationshipRelink {
                conversation_id: old.id,
                previous_head: hex(&pinned.hash()),
            });
        }
        true
    }

    pub(super) fn restore_contact(&self, account: &str) -> bool {
        self.st
            .app
            .relationship_restore
            .as_ref()
            .is_some_and(|r| r.candidates.contains_key(account))
    }

    pub(super) fn relink_is_current(&self, fresh: &Relationship) -> bool {
        let Some(original) = &fresh.relink else {
            return true;
        };
        self.st.app.relationships.get(&original.conversation_id).is_some_and(|old| {
            !old.blocked && old.peer_account_id == fresh.peer_account_id
                && old.peer_device_id != fresh.peer_device_id
                && matches!(
                    (Acs::from_body(&hb(&old.peer_acs_body).unwrap_or_default()),
                    Acs::from_body(&hb(&fresh.peer_acs_body).unwrap_or_default())),
                    (Ok(old), Ok(new)) if hex(&old.hash()) == original.previous_head || old.hash() == new.hash()
                )
        })
    }

    /// After the normal handshake, use the original local thread with the new relationship's wire identity,
    /// mailbox and session. Old events stay outside the live reducer and old envelopes never follow the thread.
    pub(super) fn relink_ready(&mut self, conv: &str) {
        let Some(target) = self
            .st
            .app
            .relationships
            .get(conv)
            .filter(|r| r.state == RelState::Active)
            .and_then(|r| r.relink.as_ref().map(|l| l.conversation_id.clone()))
        else {
            return;
        };
        let Some(old) = self.st.app.relationships.remove(&target) else {
            return;
        };
        let Some(mut fresh) = self.st.app.relationships.remove(conv) else {
            self.st.app.relationships.insert(target, old);
            return;
        };
        fresh.id = target.clone();
        fresh.person_id = old.person_id;
        fresh.nickname = old.nickname;
        fresh.created_at = old.created_at;
        fresh.blocked = old.blocked;
        fresh.peer_links = old.peer_links;
        fresh.peer_recovery = old.peer_recovery;
        fresh.relink = None;
        fresh.relinked_from = Some(conv.into());
        self.st.app.outbox.retain_mut(|e| {
            if let Some(c) = e.purpose.conversation_mut() {
                if *c == target {
                    return false;
                }
                if *c == conv {
                    *c = target.clone();
                }
            }
            if let Some(f) = &mut e.failover
                && f.conversation_id == conv
            {
                f.conversation_id = target.clone();
            }
            true
        });
        for offer in self
            .st
            .app
            .direct_offers
            .values_mut()
            .filter(|o| o.conversation_id == target)
        {
            if offer.status.is_live() {
                offer.status = DirectStatus::Withdrawn;
            }
        }
        let mut history = Vec::new();
        for mut message in old.messages {
            if message.lane {
                continue;
            }
            message.envelope = None;
            message.queued_event = None;
            message.fan.clear();
            if matches!(
                message.delivery,
                DeliveryState::PendingNetwork
                    | DeliveryState::PendingReconciliation
                    | DeliveryState::WaitingForPeerRecovery
            ) {
                message.delivery = DeliveryState::Rejected {
                    reason: RejectReason::StaleState,
                };
                if let Some(op) = &message.operation_id {
                    self.set_outcome(
                        op,
                        OutcomeClass::Rejected {
                            reason: RejectReason::StaleState,
                        },
                    );
                }
            }
            history.push(message);
        }
        self.st
            .app
            .relationship_history
            .entry(target.clone())
            .or_default()
            .extend(history);
        self.st.app.relationships.insert(target, fresh);
    }

    // ------------------------------------------------------------------------------------------ sender (3.1)

    /// The recovering device pushes a `RecoveryNotice` to every active contact over the existing sealed 1:1 deposit
    /// (ADR-0190 3.1): the notice is the inner plaintext of an ordinary `SealedDeliveryEnvelope`, with a fresh wrapper
    /// each call. A contact it cannot seal to (no snapshot route, expired key) is skipped, never sent in the clear.
    /// Returns how many were queued. The resend schedule belongs to the caller; it only has to call again.
    pub fn send_recovery_notices(
        &mut self,
        request: &[u8],
        successor: Option<&SuccessorBundle>,
    ) -> usize {
        let Some((head, chain, creds)) = self.acs_head() else {
            return 0;
        };
        let Some(policy_hex) = self.st.app.recovery_policy.clone() else {
            return 0;
        };
        let Ok(policy) = unhex(&policy_hex) else {
            return 0;
        };
        // The bridge: our own chain up to the state the request names (the tail, at most 8; the contact skips
        // states it already holds).
        let parent = recovery::RecoveryRequest::from_body(
            &cbor::decode(request)
                .ok()
                .and_then(|v| Some(v.get(1).ok()?.b().ok()?.to_vec()))
                .unwrap_or_default(),
        )
        .map(|r| r.parent_state_hash)
        .unwrap_or(head.hash());
        let upto = chain_states(&chain)
            .and_then(|s| s.iter().position(|a| a.hash() == parent))
            .map_or(chain.len(), |i| i + 1);
        let Some(notice) = build_notice(&policy, request, &chain[..upto], &creds, successor, &[])
        else {
            return 0;
        };
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        let mut queued = 0;
        for conv in convs {
            let Some(rel) = self.st.app.relationships.get(&conv).cloned() else {
                continue;
            };
            let Ok((pd, route)) = self.peer_target(&rel) else {
                continue;
            };
            let Ok(wrapper) = self.seal_to(&pd, &route, &notice) else {
                continue;
            };
            self.enqueue_sealed(
                &conv,
                &route,
                wrapper,
                &notice,
                Purpose::DepositRecoveryNotice {
                    conversation_id: conv.clone(),
                },
            );
            queued += 1;
        }
        queued
    }

    /// A live controller that vetoes tells every active contact over the ordinary session (ADR-0190 3.3).
    pub(super) fn push_veto_to_contacts(&mut self, signed: &[u8]) {
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_RECOVERY_VETO)),
            (1, V::B(signed.to_vec())),
        ]));
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        for conv in convs {
            self.send_control(
                &conv,
                &payload,
                Purpose::DepositAcs {
                    conversation_id: conv.clone(),
                },
            );
        }
    }

    // ------------------------------------------------------------------------------------------ receiver (section 2)

    /// A sealed inner that is a `RecoveryNotice` (first map entry `0: 34`). Refusals leave nothing behind.
    pub(super) fn on_recovery_notice(&mut self, conv: &str, inner: &[u8]) -> Inner {
        match parse_notice(inner) {
            Some(n) => self.observe_notice(conv, n).into(),
            None => Inner::Rejected,
        }
    }

    /// An in-session veto (`KIND_RECOVERY_VETO`).
    pub(super) fn on_peer_veto(&mut self, conv: &str, p: &V) -> bool {
        let Some(raw) = p.get(1).ok().and_then(|b| b.b().ok()).map(<[u8]>::to_vec) else {
            return false;
        };
        let changed = self.peer_veto(conv, &raw);
        if changed {
            self.peer_eval(conv, None);
        }
        true // a veto for nothing we track is not an integrity failure
    }

    /// The dossier of a head update: for the recovery link right after the pinned head, hand the policy, request and
    /// successor to `observe_notice`. A dossier that does not match the chain is ignored.
    pub(super) fn observe_dossier(
        &mut self,
        conv: &str,
        p: &V,
        chain: &[Vec<u8>],
        creds: &[Vec<u8>],
    ) {
        let Some(dossier) = parse_dossier(p.opt(5)) else {
            return;
        };
        if dossier.is_empty() {
            return;
        }
        let Some(pinned) = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| hb(&r.peer_acs_body).ok())
            .and_then(|b| Acs::from_body(&b).ok())
        else {
            return;
        };
        let Some((child, entry)) = chain.iter().find_map(|s| {
            let a = Acs::from_body(cbor::decode(s).ok()?.get(1).ok()?.b().ok()?).ok()?;
            let e = dossier.iter().find(|e| e.child_hash == a.hash())?;
            (a.previous == Some(pinned.hash()) && control::has_recovery_signature(s))
                .then(|| (s.clone(), e.clone()))
        }) else {
            return;
        };
        let Some(cred) = recovery::RecoveryRequest::from_body(
            &cbor::decode(&entry.request)
                .ok()
                .and_then(|v| Some(v.get(1).ok()?.b().ok()?.to_vec()))
                .unwrap_or_default(),
        )
        .ok()
        .and_then(|r| {
            creds.iter().find(|c| {
                Credential::from_body(c).is_ok_and(|c| c.hash() == r.new_device_credential)
            })
        })
        .cloned() else {
            return;
        };
        self.observe_notice(
            conv,
            NoticeInput {
                policy: entry.policy,
                request: entry.request,
                bridge: vec![],
                creds: creds.to_vec(),
                successor: Some((child, entry.next_policy, cred)),
                vetoes: vec![],
            },
        );
    }

    /// Steps 1 to 5 of ADR-0190 section 2, then the record. `false` for evidence that does not verify (nothing was
    /// changed); `true` when it verified, including a valid request that is stale or that this contact already tracks.
    pub(super) fn observe_notice(&mut self, conv: &str, n: NoticeInput) -> bool {
        // 1. Active, not blocked, with a pinned head of the account.
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let Some(old) = hb(&rel.peer_acs_body)
            .ok()
            .and_then(|b| Acs::from_body(&b).ok())
        else {
            return false;
        };
        if rel.state != RelState::Active || rel.blocked {
            return false;
        }
        // 2. Bounds before any signature (the registry already bounded counts and sizes).
        if n.policy.len() > recovery::MAX_POLICY_BYTES
            || n.bridge.len() > 8
            || n.creds.len() > 16
            || n.vetoes.len() > MAX_VETOES
        {
            return false;
        }
        // 3. The bridge: ordinary signed states from the pinned head to the request's parent. Planned, not yet adopted.
        let Some(states) = chain_states(&n.bridge) else {
            return false;
        };
        if states.iter().any(|s| s.account_id != old.account_id) {
            return false;
        }
        let rest: &[Vec<u8>] = match states.iter().position(|s| s.hash() == old.hash()) {
            Some(k) => &n.bridge[k + 1..],
            None => &n.bridge[..],
        };
        let (head, plan): (Acs, Option<PeerPlan>) = if rest.is_empty() {
            (old.clone(), None)
        } else {
            let Some(plan) = Self::peer_plan(&rel, rest, &n.creds) else {
                return false;
            };
            // The whole bridge must be followed: a recovery link inside it stops the walk short (not followed).
            if states.last().map(Acs::hash) != Some(plan.head.hash()) {
                return false;
            }
            (plan.head.clone(), Some(plan))
        };
        // 4. The policy body is well formed, hashes to the head's commitment, is on and has a usable key.
        if registry().check_body("RecoveryPolicy", &n.policy).is_err() {
            return false;
        }
        let Ok(policy) = RecoveryPolicy::from_body(&n.policy) else {
            return false;
        };
        let Some(key) = policy.recovery_key() else {
            return false;
        };
        if head.recovery_commitment != Some(policy.hash()) || policy.state != recovery::STATE_ON {
            return false;
        }
        // 5. The request opens under that key, and names this account, epoch and parent with a fresh target.
        let Ok(req) = recovery::open_request(&n.request, key) else {
            return false;
        };
        if is_stale(&head, &policy, &req) {
            return false;
        }
        // Verified. Only now does anything change: the bridge is adopted, then the record is created or merged.
        let digest = hex(&req.digest());
        let tracked = rel.peer_recovery.iter().any(|r| r.obs.digest == digest);
        if !tracked
            && rel.peer_recovery.len() >= MAX_PEER_RECOVERIES
            && !rel.peer_recovery.iter().any(|r| !r.is_live())
        {
            return true; // two live recoveries already: a third is not tracked, never at the cost of a live one
        }
        let mut adopted = false;
        if let Some(plan) = plan {
            adopted = self.peer_commit(conv, plan).is_some();
            if !adopted {
                return false;
            }
        }
        if !tracked {
            let now = self.now_ms;
            let Some(r) = self.st.app.relationships.get_mut(conv) else {
                return false;
            };
            if r.peer_recovery.len() >= MAX_PEER_RECOVERIES
                && let Some(i) = r.peer_recovery.iter().position(|x| !x.is_live())
            {
                r.peer_recovery.remove(i);
            }
            r.peer_recovery.push(PeerRecoveryRec {
                obs: RecoveryObsRec {
                    digest: digest.clone(),
                    request: hex(&n.request),
                    first_seen_ms: now,
                    // No monotonic sample comes with a delivery: the baseline is set by the first tick, so the
                    // interval can only start later than the observation, never earlier.
                    boot_session: String::new(),
                    last_mono_ms: 0,
                    credited_ms: 0,
                    vetoes: vec![],
                    own_veto: false,
                    successor: None,
                    state: RecoveryObsState::Waiting,
                },
                policy: hex(&n.policy),
                state: PeerRecoveryState::Waiting,
                decision: None,
            });
        }
        for v in &n.vetoes {
            self.peer_veto(conv, v);
        }
        if let Some((child, next_policy, credential)) = n.successor {
            self.peer_successor(
                conv,
                &digest,
                RecoverySuccessorRec {
                    child: hex(&child),
                    next_policy: hex(&next_policy),
                    credential: hex(&credential),
                },
            );
        }
        self.peer_eval(conv, None);
        if adopted {
            self.retry_held(conv);
        }
        true
    }

    /// The pinned head and its credentials for the relationship.
    fn peer_pinned(&self, conv: &str) -> Option<(Acs, Vec<Vec<u8>>)> {
        let rel = self.st.app.relationships.get(conv)?;
        let head = Acs::from_body(&hb(&rel.peer_acs_body).ok()?).ok()?;
        let creds = rel
            .peer_credentials
            .iter()
            .filter_map(|c| unhex(c).ok())
            .collect();
        Some((head, creds))
    }

    /// A veto by a live controller of the PINNED head, valid under the credential that head commits to, naming a
    /// tracked request of this epoch. `true` when it ended a live recovery.
    fn peer_veto(&mut self, conv: &str, signed: &[u8]) -> bool {
        let Some((head, creds)) = self.peer_pinned(conv) else {
            return false;
        };
        let Ok(id) = recovery::veto_signer(signed) else {
            return false;
        };
        let Some(key) = live(&head, &id)
            .then(|| committed_key(&head, &creds, &id))
            .flatten()
        else {
            return false;
        };
        let Ok(veto) = recovery::open_veto(signed, &id, &key) else {
            return false;
        };
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return false;
        };
        let Some(rec) = rel
            .peer_recovery
            .iter_mut()
            .find(|r| r.obs.digest == hex(&veto.request_digest))
        else {
            return false;
        };
        let epoch_ok = unhex(&rec.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
            .is_some_and(|p| p.epoch() == veto.recovery_epoch);
        if veto.account_id != head.account_id || !epoch_ok || !rec.is_live() {
            return false;
        }
        let raw = hex(signed);
        if rec.obs.vetoes.len() < MAX_VETOES && !rec.obs.vetoes.contains(&raw) {
            rec.obs.vetoes.push(raw);
        }
        rec.state = PeerRecoveryState::Vetoed;
        rel.held.clear(); // dropped on vetoed (section 5)
        self.settle_peer_outgoing(conv, false);
        true
    }

    /// Keep a recovery-signed successor for a tracked request if it verifies as a (possibly pending) completion.
    fn peer_successor(&mut self, conv: &str, digest: &str, held: RecoverySuccessorRec) {
        let Some((head, creds)) = self.peer_pinned(conv) else {
            return;
        };
        let Some(rec) = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.peer_recovery.iter().find(|x| x.obs.digest == digest))
        else {
            return;
        };
        let Some(policy) = unhex(&rec.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
        else {
            return;
        };
        if !rec.is_live()
            || rec.obs.successor.is_some()
            || held.child.len() > 32 * 1024
            || held.credential.len() > 32 * 1024
            || held.next_policy.len() > recovery::MAX_POLICY_BYTES * 2
            || verify_held(&head, &creds, &policy, &rec.obs, &held).is_err()
        {
            return;
        }
        if let Some(r) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.peer_recovery.iter_mut().find(|x| x.obs.digest == digest))
        {
            r.obs.successor = Some(held);
        }
    }

    /// Whether this contact must confirm a recovery itself: every level but Recommended (fail closed for Custom).
    fn confirm_required(&self) -> bool {
        !matches!(
            self.st.app.privacy_level,
            None | Some(PrivacyPolicyLevel::Recommended)
        )
    }

    /// Re-evaluate every live recovery of `conv`: credit the host's reading (when there is one), end the ones the
    /// account moved past, and accept one whose own interval ran (with the held successor, no veto, and on Hardened
    /// and Maximum the contact's confirmation). Nothing here can shorten an interval.
    pub(super) fn peer_eval(&mut self, conv: &str, mono: Option<&MonotonicSample>) {
        let Some((head, creds)) = self.peer_pinned(conv) else {
            return;
        };
        let need_confirm = self.confirm_required();
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        let mut accept: Option<(usize, RecoverySuccessorRec)> = None;
        let mut ended = false;
        for (i, rec) in rel.peer_recovery.iter_mut().enumerate() {
            if !rec.is_live() {
                continue;
            }
            if let Some(m) = mono {
                credit(&mut rec.obs, m);
            }
            let Some(policy) = unhex(&rec.policy)
                .ok()
                .and_then(|b| RecoveryPolicy::from_body(&b).ok())
            else {
                continue;
            };
            let stale = policy
                .recovery_key()
                .and_then(|k| {
                    unhex(&rec.obs.request)
                        .ok()
                        .and_then(|b| recovery::open_request(&b, k).ok())
                })
                .is_none_or(|req| is_stale(&head, &policy, &req));
            if stale {
                rec.state = PeerRecoveryState::Stale;
                ended = true;
                continue;
            }
            let ran = rec.obs.credited_ms >= policy.delay_seconds.saturating_mul(1000);
            let verdict = rec.obs.successor.clone().map(|h| {
                (
                    verify_successor_for(&head, &creds, &policy, &rec.obs, &h),
                    h,
                )
            });
            rec.state = match verdict {
                Some((Ok(control::RecoveryVerdict::Complete { .. }), h)) => {
                    if need_confirm && rec.decision != Some(PeerDecision::Confirmed) {
                        PeerRecoveryState::Ready
                    } else {
                        accept = Some((i, h));
                        PeerRecoveryState::Ready
                    }
                }
                Some((Ok(control::RecoveryVerdict::Pending { .. }), _)) => {
                    PeerRecoveryState::Waiting
                }
                Some((Err(()), _)) => {
                    rec.obs.successor = None; // a successor that no longer verifies is dropped, the wait goes on
                    if ran {
                        PeerRecoveryState::Ready
                    } else {
                        PeerRecoveryState::Waiting
                    }
                }
                None => {
                    if ran {
                        PeerRecoveryState::Ready
                    } else {
                        PeerRecoveryState::Waiting
                    }
                }
            };
        }
        if ended && !rel.peer_recovery.iter().any(PeerRecoveryRec::is_live) {
            rel.held.clear(); // dropped on stale (section 5)
        }
        if let Some((i, held)) = accept {
            self.finish_peer_recovery(conv, i, held);
        } else if ended {
            self.settle_peer_outgoing(conv, false);
        }
    }

    /// The host's monotonic reading for every relationship with a live recovery (called with `recovery_tick`).
    pub(super) fn peer_tick(&mut self, mono: &MonotonicSample) {
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.peer_recovery.iter().any(PeerRecoveryRec::is_live))
            .map(|r| r.id.clone())
            .collect();
        for c in convs {
            self.peer_eval(&c, Some(mono));
        }
        // A text still waiting although no recovery is pending (a record that was lost or never restored): hand it back
        // rather than leave it waiting for something that can no longer happen.
        let stranded: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| {
                !r.peer_recovery.iter().any(PeerRecoveryRec::is_live)
                    && r.messages
                        .iter()
                        .any(|m| m.delivery == DeliveryState::WaitingForPeerRecovery)
            })
            .map(|r| r.id.clone())
            .collect();
        for c in stranded {
            self.settle_peer_outgoing(&c, false);
        }
    }

    /// Replace the pinned head with the verified successor, keep the evidence (with THIS verifier's seconds), end the
    /// other live recoveries and process what waited (section 4).
    fn finish_peer_recovery(&mut self, conv: &str, idx: usize, held: RecoverySuccessorRec) {
        let Some((head, creds)) = self.peer_pinned(conv) else {
            return;
        };
        let Some(rec) = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.peer_recovery.get(idx))
            .cloned()
        else {
            return;
        };
        let Some(policy) = unhex(&rec.policy)
            .ok()
            .and_then(|b| RecoveryPolicy::from_body(&b).ok())
        else {
            return;
        };
        let Ok(control::RecoveryVerdict::Complete { child, .. }) =
            verify_successor_for(&head, &creds, &policy, &rec.obs, &held)
        else {
            return;
        };
        let (Ok(cred), account) = (unhex(&held.credential), child.account_id) else {
            return;
        };
        let mut all = creds;
        if !all.contains(&cred) {
            all.push(cred);
        }
        let kept: Vec<Vec<u8>> = all
            .into_iter()
            .filter(|c| {
                Credential::from_body(c).is_ok_and(|cr| {
                    cr.account_id == account
                        && child
                            .credentials
                            .iter()
                            .any(|(d, h)| *d == cr.device_id && *h == cr.hash())
                })
            })
            .collect();
        let link = RecoveryLinkRec {
            child_hash: hex(&child.hash()),
            policy: rec.policy.clone(),
            next_policy: held.next_policy.clone(),
            request: rec.obs.request.clone(),
            elapsed_seconds: rec.obs.credited_ms / 1000,
        };
        if self
            .peer_commit(conv, PeerPlan { head: child, kept })
            .is_none()
        {
            return;
        }
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.peer_links.push(link);
            for (i, x) in r.peer_recovery.iter_mut().enumerate() {
                if i == idx {
                    x.state = PeerRecoveryState::Accepted;
                    x.obs.successor = Some(held.clone());
                } else if x.is_live() {
                    x.state = PeerRecoveryState::Stale;
                }
            }
        }
        self.retry_held(conv);
        self.settle_peer_outgoing(conv, true);
        let pending: Vec<_> = self
            .st
            .app
            .relationships
            .iter_mut()
            .filter(|(_, r)| {
                r.recovery_bind
                    .as_ref()
                    .is_some_and(|b| b.account_id == hex(&account))
            })
            .filter_map(|(id, r)| r.recovery_bind.take().map(|b| (id.clone(), b)))
            .collect();
        for (id, held) in pending {
            if let Ok(wrapper) = unhex(&held.bytes) {
                self.on_session_envelope(&id, &wrapper);
            }
        }
    }

    /// What happens to the texts written while a contact's recovery was pending, once no recovery of theirs is pending
    /// (ADR-0190 section 5, D131). Accepted: the head now lists their new device, so what waited is released in order, as
    /// ordinary sends. Vetoed, declined or stale: the condition the person was told about (their new device confirmed) did not
    /// happen, so nothing is sent on its own: each text is handed back as "not sent" with the usual retry. Does nothing while a
    /// recovery is still live, and nothing when no text waits.
    pub(super) fn settle_peer_outgoing(&mut self, conv: &str, accepted: bool) {
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return;
        };
        if rel.peer_recovery.iter().any(PeerRecoveryRec::is_live) {
            return;
        }
        let waiting: Vec<(String, Option<OperationId>)> = rel
            .messages
            .iter()
            .filter(|m| m.delivery == DeliveryState::WaitingForPeerRecovery)
            .map(|m| (m.id.clone(), m.operation_id.clone()))
            .collect();
        if waiting.is_empty() {
            return;
        }
        if accepted {
            for (id, _) in &waiting {
                self.set_delivery(conv, id, DeliveryState::PendingNetwork);
            }
            // A failure leaves them queued, never dropped: any later release (the next head refresh) retries it.
            let _ = self.release_queued(conv);
            return;
        }
        let unavailable = RejectReason::Unavailable;
        for (id, op) in waiting {
            self.set_delivery(
                conv,
                &id,
                DeliveryState::Rejected {
                    reason: unavailable,
                },
            );
            if let Some(op) = op {
                self.set_outcome(
                    &op,
                    OutcomeClass::Rejected {
                        reason: unavailable,
                    },
                );
            }
        }
    }

    /// TEST ONLY (`test-loopback`): run a `KIND_ACS_UPDATE` payload through the head-update path, including the
    /// dossier route of `observe_dossier`, as a verified session would have delivered it.
    #[cfg(feature = "test-loopback")]
    pub fn debug_acs_update(&mut self, conv: &str, payload: &[u8]) -> Result<bool, CoreError> {
        let p = cbor::decode(payload).map_err(|_| CoreError::Invalid("payload"))?;
        Ok(self.on_acs_update(conv, &p))
    }

    /// `DeclinePeerRecovery` ("This isn't them"): ends the request for this contact only. Not sent to anyone.
    pub(super) fn decline_peer_recovery(&mut self, conv: &str, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return rejected(RejectReason::NotFound);
        };
        let Some(rec) = rel
            .peer_recovery
            .iter_mut()
            .find(|r| r.obs.digest == request_id)
        else {
            return rejected(RejectReason::NotFound);
        };
        if !rec.is_live() {
            return rejected(RejectReason::Conflict);
        }
        rec.decision = Some(PeerDecision::Declined);
        rec.state = PeerRecoveryState::Declined;
        rel.held.clear(); // dropped on declined (section 5)
        self.settle_peer_outgoing(conv, false);
        OutcomeClass::Committed
    }

    /// `ConfirmPeerRecovery` ("Yes, it's them"): only after the wait (state `ready`). On Hardened and Maximum the
    /// contact's confirmation is what lets the accept happen; on Recommended it is accepted anyway and this is a no-op.
    pub(super) fn confirm_peer_recovery(&mut self, conv: &str, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return rejected(RejectReason::NotFound);
        };
        let Some(rec) = rel
            .peer_recovery
            .iter_mut()
            .find(|r| r.obs.digest == request_id)
        else {
            return rejected(RejectReason::NotFound);
        };
        if rec.state != PeerRecoveryState::Ready {
            return rejected(RejectReason::Conflict);
        }
        rec.decision = Some(PeerDecision::Confirmed);
        self.peer_eval(conv, None);
        OutcomeClass::Committed
    }
}

/// Is the first map entry of `inner` `0: 34`? (An ordinary session envelope is a CBOR array, so the two never
/// collide; a notice map has at most 9 entries, so its header is one byte.)
pub(super) fn is_notice_inner(inner: &[u8]) -> bool {
    inner.len() > 4 && inner[0] & 0xE0 == 0xA0 && inner[1..4] == [0x00, 0x18, 0x22]
}
