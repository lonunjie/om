// SPDX-License-Identifier: MPL-2.0
//! Purpose-bound, same-account controller exchange (ADR-0216). All state and exact queued bytes
//! are committed through the existing ceremony outbox.

use super::ceremony::{CEREMONY_TTL_MS, open_msg_ad};
use super::recovery_watch::{committed_key, live};
use super::*;
use crate::control::{Action, plan_device_change};
use crate::crypto::link_ceremony::CeremonyKeys;
use crate::crypto::recovery_approval as ra;
use crate::crypto::sig;
use crate::host::PollTarget;
use crate::recovery::RecoveryPolicy;
use crate::recovery_record::RecoveryKit;

const PREFIX: &str = "om:controller-change/";
const PURPOSE: &str = "om.controller_change.v1";

fn candidate(head: &Acs, origin: &[u8; 16], target: &[u8; 16], remove: bool) -> Option<Acs> {
    if origin == target || control::safety_frozen(head) {
        return None;
    }
    let action = if remove {
        Action::Revoke
    } else {
        Action::Promote
    };
    let (child, needs_signers) = plan_device_change(head, origin, action, target).ok()?;
    if remove {
        // In this exchange the selected device is the other eligible parent voter.
        (needs_signers && head.control.len() == 2 && live(head, target)).then_some(child)
    } else {
        (!needs_signers).then_some(child)
    }
}

fn keys(c: &ControllerChange, head: &Acs) -> Option<(CeremonyKeys, Vec<u8>)> {
    let target = h16(&c.target).ok()?;
    let origin = h16(&c.origin).ok()?;
    let exact_child = candidate(head, &origin, &target, c.remove)?;
    if c.parent != hex(&head.hash()) || c.child != hex(&exact_child.body()) {
        return None;
    }
    let (_, cred) = head.credentials.iter().find(|(id, _)| *id == target)?;
    let binding = cbor::encode(&V::A(vec![
        V::T(PURPOSE.to_string()),
        V::U(u64::from(c.remove)),
        V::B(head.account_id.to_vec()),
        V::B(head.hash().to_vec()),
        V::B(origin.to_vec()),
        V::B(target.to_vec()),
        V::B(cred.to_vec()),
        V::B(exact_child.hash().to_vec()),
    ]));
    transcript(c, binding)
}

fn transcript(c: &ControllerChange, binding: Vec<u8>) -> Option<(CeremonyKeys, Vec<u8>)> {
    let context = cbor::encode(&V::A(vec![
        V::B(binding.clone()),
        V::B(ra::context(
            c.expires_at / 1000,
            c.relay_uri.as_bytes(),
            &unhex(&c.a_cap).ok()?,
            &unhex(&c.b_recv).ok()?,
        )),
    ]));
    let keys = ra::keys_in_context(&context, &unhex(&c.secret).ok()?);
    let proof = cbor::encode(&V::A(vec![
        V::T(PURPOSE.to_string()),
        V::B(keys.id.to_vec()),
        V::B(binding),
    ]));
    Some((keys, proof))
}

fn link(c: &ControllerChange) -> Option<String> {
    let offer = ra::encode_offer(&ra::Offer {
        expiry_s: u32::try_from(c.expires_at / 1000).ok()?,
        secret: h16(&c.secret).ok()?,
        a_cap: unhex(&c.a_cap).ok()?,
        b_recv: unhex(&c.b_recv).ok()?,
        uri: c.relay_uri.clone(),
    })?;
    Some(format!(
        "{PREFIX}{}/{}/{}",
        u8::from(c.remove),
        c.origin,
        offer.strip_prefix(ra::OFFER_PREFIX)?
    ))
}

fn parse_link(text: &str) -> Option<(bool, [u8; 16], ra::Offer)> {
    let text = text.trim();
    if text.len() > PREFIX.len() + 35 + ra::MAX_OFFER_TOKEN {
        return None;
    }
    let (action, rest) = text.strip_prefix(PREFIX)?.split_once('/')?;
    let (origin, token) = rest.split_once('/')?;
    let remove = match action {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let id = h16(origin).ok()?;
    if hex(&id) != origin {
        return None;
    }
    Some((
        remove,
        id,
        ra::decode_offer(&format!("{}{token}", ra::OFFER_PREFIX)).ok()?,
    ))
}

/// Fuzz entry point (V-012): the controller-change link parser. Compiled only with `fuzz-entry`, which a `production` build refuses.
#[cfg(feature = "fuzz-entry")]
pub(super) fn fuzz_parse_link(text: &str) -> Result<(), &'static str> {
    let (_, origin, offer) = parse_link(text).ok_or("link_refused")?;
    assert_eq!(hex(&origin).len(), 32);
    assert!(ra::encode_offer(&offer).is_some(), "a parsed offer re-encodes");
    Ok(())
}

fn kit_for_head(head: &Acs, policy: &[u8], kit: &[u8]) -> bool {
    RecoveryPolicy::from_body(policy)
        .ok()
        .zip(RecoveryKit::from_bytes(kit).ok())
        .is_some_and(|(policy, kit)| {
            head.recovery_commitment == Some(policy.hash()) && policy.epoch() == kit.epoch
        })
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    pub(super) fn offer_controller_change(
        &mut self,
        op: &OperationId,
        target: &str,
        remove: bool,
    ) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        let (Some((head, ..)), Some(me), Ok(id)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
            h16(target),
        ) else {
            return rej(StaleState);
        };
        let Some(child) = candidate(&head, &me, &id, remove) else {
            return rej(Unauthorized);
        };
        if self.st.app.controller_change.is_some() {
            return rej(Conflict);
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
        let (Some((a, a_route)), Some((b, b_route))) = (
            self.take_route_excluding(&other),
            self.take_route_excluding(&other),
        ) else {
            return rej(Unavailable);
        };
        let secret = self.rand16();
        self.st.app.controller_change = Some(ControllerChange {
            operation_id: op.clone(),
            expires_at: (self.now_ms + CEREMONY_TTL_MS) / 1000 * 1000,
            secret: hex(&secret).into(),
            a_cap: hex(&a_route.capability).into(),
            b_recv: String::new().into(),
            mailbox: hex(&a),
            relay_uri: uri,
            origin: hex(&me),
            target: hex(&id),
            parent: hex(&head.hash()),
            child: hex(&child.body()),
            remove,
            response_cap: Some(hex(&b_route.capability).into()),
            confirmed: false,
            request: None,
            recipient_seed: None,
        });
        self.enqueue(
            NetCall::OpenQueue {
                mailbox: b.to_vec(),
                relay_uri: b_route.relay_uri,
            },
            Purpose::OpenCeremonyQueue {
                operation_id: op.clone(),
            },
        );
        OutcomeClass::PendingNetwork
    }

    pub(super) fn controller_open_queue(&mut self, op: &str, cap: Option<&[u8]>) -> bool {
        let Some(c) = self
            .st
            .app
            .controller_change
            .as_mut()
            .filter(|c| c.operation_id == op && c.response_cap.is_some())
        else {
            return false;
        };
        if let Some(cap) = cap.filter(|cap| (16..=64).contains(&cap.len())) {
            c.b_recv = hex(cap).into();
        }
        let outcome = if c.expires_at > self.now_ms && link(c).is_some() {
            OutcomeClass::Committed
        } else {
            self.st.app.controller_change = None;
            OutcomeClass::Rejected {
                reason: RejectReason::Unavailable,
            }
        };
        self.set_outcome(&op.to_string(), outcome);
        true
    }

    pub(super) fn join_controller_change(&mut self, op: &OperationId, text: &str) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        if self.st.app.controller_change.is_some() {
            return rej(Conflict);
        }
        let Some((remove, origin, offer)) = parse_link(text) else {
            return rej(Invalid);
        };
        let expiry = u64::from(offer.expiry_s) * 1000;
        if expiry <= self.now_ms || expiry > self.now_ms + CEREMONY_TTL_MS + 60_000 {
            return rej(Expired);
        }
        let (Some((head, ..)), Some(me)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return rej(StaleState);
        };
        let Some(child) = candidate(&head, &origin, &me, remove) else {
            return rej(Unauthorized);
        };
        self.st.app.controller_change = Some(ControllerChange {
            operation_id: op.clone(),
            expires_at: expiry,
            secret: hex(&offer.secret).into(),
            a_cap: hex(&offer.a_cap).into(),
            b_recv: hex(&offer.b_recv).into(),
            mailbox: hex(&ra::mailbox(&offer.b_recv)),
            relay_uri: offer.uri,
            origin: hex(&origin),
            target: hex(&me),
            parent: hex(&head.hash()),
            child: hex(&child.body()),
            remove,
            response_cap: None,
            confirmed: false,
            request: None,
            recipient_seed: None,
        });
        OutcomeClass::Committed
    }

    pub(super) fn confirm_controller_change(&mut self, request_id: &str) -> OutcomeClass {
        match self.confirm_controller_change_inner(request_id) {
            Ok(()) => OutcomeClass::Committed,
            Err(reason) => OutcomeClass::Rejected { reason },
        }
    }

    fn confirm_controller_change_inner(&mut self, request_id: &str) -> Result<(), RejectReason> {
        use RejectReason::*;
        let c = self.st.app.controller_change.clone().ok_or(NotFound)?;
        if c.expires_at <= self.now_ms || c.confirmed {
            return Err(Expired);
        }
        let (head, _, creds) = self.acs_head().ok_or(StaleState)?;
        let (keys, proof) = keys(&c, &head).ok_or(StaleState)?;
        if request_id != hex(&keys.id) {
            return Err(StaleState);
        }
        let dev = self.st.device.as_ref().ok_or(StaleState)?;
        let body = unhex(&c.child).map_err(|_| IntegrityFailure)?;
        if c.response_cap.is_none() {
            let seed = (!c.remove).then(|| zeroize::Zeroizing::new(arr32(self.rng)));
            let recipient = seed
                .as_ref()
                .map_or(V::Null, |seed| V::B(hpke::public_key(seed)));
            let signature = dev
                .signer
                .sign_with(&request_proof(&proof, &recipient), arr32(self.rng));
            let vote = if c.remove {
                V::B(control::sign_acs(
                    &body,
                    &[(&dev.device_id, &dev.signer)],
                    self.rng,
                ))
            } else {
                V::Null
            };
            let pt = cbor::encode(&V::A(vec![V::U(1), V::B(signature), vote, recipient]));
            let sealed = self
                .seal_msg_ad(&keys, ra::ROLE_REQ, &pt, c.expires_at / 1000, ra::aead_ad)
                .ok_or(TooLarge)?;
            let cap = unhex(&c.a_cap).map_err(|_| IntegrityFailure)?;
            if let Some(live) = self.st.app.controller_change.as_mut() {
                live.confirmed = true;
                live.recipient_seed = seed.as_ref().map(|seed| hex(&**seed).into());
                live.request = Some(hex(&pt));
            }
            self.deposit_ceremony(&c.operation_id, &cap, &c.relay_uri, sealed, false);
            return Ok(());
        }
        let raw = unhex(c.request.as_deref().ok_or(StaleState)?).map_err(|_| IntegrityFailure)?;
        let request = verify_request(&c, &head, &creds, &proof, &raw).ok_or(IntegrityFailure)?;
        let signed = match &request {
            VerifiedRequest::Vote(vote) => {
                control::cosign_acs(vote, &dev.device_id, &dev.signer, self.rng)
                    .map_err(|_| IntegrityFailure)?
            }
            VerifiedRequest::Promotion(_) => {
                control::sign_acs(&body, &[(&dev.device_id, &dev.signer)], self.rng)
            }
        };
        let child = control::verify_successor(&head, &signed, &creds).map_err(|_| Unauthorized)?;
        let kit_binding = reply_binding("kit", &keys.id, &raw, &child.hash());
        let mut transfer = V::Null;
        if let VerifiedRequest::Promotion(pk) = request
            && !self.st.app.recovery_writer.paused_for_exclusion
            && self.st.app.recovery_key_saved
            && let Some(policy) = self
                .st
                .app
                .recovery_policy
                .as_deref()
                .and_then(|p| unhex(p).ok())
            && let Some(kit) = self
                .st
                .app
                .recovery_kit
                .as_deref()
                .and_then(|k| unhex(k).ok())
            && kit_for_head(&child, &policy, &kit)
        {
            let kit = zeroize::Zeroizing::new(kit);
            let mut randomness = zeroize::Zeroizing::new([0u8; 64]);
            self.rng.fill_bytes(&mut randomness[..]);
            let (ss, enc) = hpke::encaps_derand(&pk, &randomness).map_err(|_| IntegrityFailure)?;
            let ct = hpke::key_schedule(&ss, &kit_binding).seal(&policy, &kit);
            transfer = V::A(vec![V::B(policy), V::B(enc), V::B(ct)]);
        }
        let reply = cbor::encode(&V::A(vec![V::U(2), V::B(signed.clone()), transfer]));
        let reply_proof = reply_binding("reply", &keys.id, &raw, &crypto::sha256(&[&reply]));
        let signature = dev.signer.sign_with(&reply_proof, arr32(self.rng));
        let pt = cbor::encode(&V::A(vec![V::B(reply), V::B(signature)]));
        let sealed = self
            .seal_msg_ad(&keys, ra::ROLE_RESP, &pt, c.expires_at / 1000, ra::aead_ad)
            .ok_or(TooLarge)?;
        let cap =
            unhex(c.response_cap.as_deref().ok_or(StaleState)?).map_err(|_| IntegrityFailure)?;
        self.st.app.acs_chain.push(hex(&signed));
        if head.control.len() == 1 && child.control.len() > 1 {
            self.cancel_inbox_acks();
        }
        self.st.app.recovery_writer.authored = true;
        self.deposit_ceremony(&c.operation_id, &cap, &c.relay_uri, sealed, true);
        self.st.app.controller_change = None;
        self.broadcast_account_update();
        Ok(())
    }

    pub(super) fn controller_mailbox(&mut self, mailbox: &str, bytes: &[u8]) -> bool {
        let Some(c) = self
            .st
            .app
            .controller_change
            .clone()
            .filter(|c| c.mailbox == mailbox)
        else {
            return false;
        };
        if c.expires_at <= self.now_ms || !self.st.app.acs_forks.is_empty() {
            return true;
        }
        let Some((head, chain, creds)) = self.acs_head() else {
            return true;
        };
        // A sibling head update can arrive before the private reply. In that case the exact
        // candidate must still be the current head and its verified parent must be retained.
        let parent = if c.parent == hex(&head.hash()) {
            head.clone()
        } else if c.response_cap.is_none() && c.child == hex(&head.body()) {
            let Some(parent) = chain
                .iter()
                .rev()
                .nth(1)
                .and_then(|s| super::ceremony::chain_head(s))
            else {
                return true;
            };
            parent
        } else {
            return true;
        };
        let Some((keys, proof)) = keys(&c, &parent) else {
            return true;
        };
        if c.response_cap.is_some() {
            if c.request.is_none()
                && let Some(pt) = open_msg_ad(&keys, ra::ROLE_REQ, bytes, ra::aead_ad)
                && verify_request(&c, &parent, &creds, &proof, &pt).is_some()
                && let Some(live) = self.st.app.controller_change.as_mut()
            {
                live.request = Some(hex(&pt));
            }
        } else if c.confirmed
            && let Some(pt) = open_msg_ad(&keys, ra::ROLE_RESP, bytes, ra::aead_ad)
        {
            self.apply_controller_reply(
                &c,
                &parent,
                &creds,
                &keys.id,
                &pt,
                head.hash() != parent.hash(),
            );
        }
        true
    }

    fn apply_controller_reply(
        &mut self,
        c: &ControllerChange,
        head: &Acs,
        creds: &[Vec<u8>],
        id: &[u8; 32],
        pt: &[u8],
        already_adopted: bool,
    ) {
        let Ok(V::A(fields)) = cbor::decode(pt) else {
            return;
        };
        let [V::B(reply), V::B(signature)] = fields.as_slice() else {
            return;
        };
        let Some(raw) = c.request.as_deref().and_then(|x| unhex(x).ok()) else {
            return;
        };
        let Ok(origin) = h16(&c.origin) else { return };
        let Some(pk) = committed_key(head, creds, &origin) else {
            return;
        };
        let proof = reply_binding("reply", id, &raw, &crypto::sha256(&[reply]));
        if sig::verify(&pk, &proof, signature).is_err() {
            return;
        }
        let Ok(V::A(fields)) = cbor::decode(reply) else {
            return;
        };
        let [V::U(2), V::B(signed), transfer] = fields.as_slice() else {
            return;
        };
        let Ok(child) = control::verify_successor(head, signed, creds) else {
            return;
        };
        if hex(&child.body()) != c.child {
            return;
        }
        let transfer = match transfer {
            V::Null => None,
            V::A(values) if !c.remove => {
                let [V::B(policy), V::B(enc), V::B(ct)] = values.as_slice() else {
                    return;
                };
                let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
                    return;
                };
                let Some(seed) = c
                    .recipient_seed
                    .as_deref()
                    .and_then(|x| h32(x).ok())
                    .map(zeroize::Zeroizing::new)
                else {
                    return;
                };
                if !live(&child, &me) {
                    return;
                }
                let Ok(ss) = hpke::decaps(&seed, enc) else {
                    return;
                };
                let info = reply_binding("kit", id, &raw, &child.hash());
                let Ok(kit) = hpke::key_schedule(&ss, &info).open(policy, ct) else {
                    return;
                };
                let kit = zeroize::Zeroizing::new(kit);
                if !kit_for_head(&child, policy, &kit) {
                    return;
                }
                Some((policy, kit))
            }
            _ => return,
        };
        if !already_adopted {
            self.st.app.acs_chain.push(hex(signed));
        }
        if let Some((policy, kit)) = transfer {
            self.st.app.recovery_policy = Some(hex(policy));
            self.st.app.recovery_kit = Some(hex(&kit).into());
            self.st.app.recovery_key_saved = true;
            self.st.app.recovery_writer.enabled = true;
        } else if c.remove {
            self.discard_recovery_puts();
            self.st.app.recovery_kit = None;
            self.st.app.recovery_writer = RecoveryWriter::default();
        }
        self.sync_own_head();
        self.st.app.controller_change = None;
    }

    pub(super) fn cancel_controller_change(&mut self, request_id: &str) -> OutcomeClass {
        let matches = self
            .acs_head()
            .and_then(|(head, ..)| view(self.st, &head, self.now_ms))
            .is_some_and(|v| v.request_id == request_id);
        if !matches
            || self
                .st
                .app
                .controller_change
                .as_ref()
                .is_some_and(|c| c.confirmed)
        {
            return OutcomeClass::Rejected {
                reason: RejectReason::StaleState,
            };
        }
        self.drop_controller_change();
        OutcomeClass::Committed
    }

    pub(crate) fn expire_controller_change(&mut self) {
        if let Some(c) = self.st.app.controller_change.as_ref() {
            let stale = self.acs_head().is_none_or(|(head, ..)| {
                c.parent != hex(&head.hash())
                    && (c.response_cap.is_some() || !c.confirmed || c.child != hex(&head.body()))
            });
            if c.expires_at <= self.now_ms || stale || !self.st.app.acs_forks.is_empty() {
                self.drop_controller_change();
            }
        }
    }

    fn drop_controller_change(&mut self) {
        let Some(c) = self.st.app.controller_change.take() else {
            return;
        };
        self.st.app.outbox.retain(|e| {
            !matches!(&e.purpose,
            Purpose::OpenCeremonyQueue { operation_id }
            | Purpose::DepositCeremony { operation_id, .. } if *operation_id == c.operation_id)
        });
    }

    pub(super) fn controller_failed(&mut self, op: &str, approval: bool) {
        if !approval
            && self
                .st
                .app
                .controller_change
                .as_ref()
                .is_some_and(|c| c.operation_id == op)
        {
            self.drop_controller_change();
        }
    }
}

fn request_proof(proof: &[u8], recipient: &V) -> Vec<u8> {
    cbor::encode(&V::A(vec![V::B(proof.to_vec()), recipient.clone()]))
}

fn reply_binding(role: &str, id: &[u8; 32], request: &[u8], value: &[u8; 32]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(PURPOSE.to_string()),
        V::T(role.to_string()),
        V::B(id.to_vec()),
        V::B(crypto::sha256(&[request]).to_vec()),
        V::B(value.to_vec()),
    ]))
}

enum VerifiedRequest {
    Promotion(Vec<u8>),
    Vote(Vec<u8>),
}

fn verify_request(
    c: &ControllerChange,
    head: &Acs,
    creds: &[Vec<u8>],
    proof: &[u8],
    pt: &[u8],
) -> Option<VerifiedRequest> {
    let target = h16(&c.target).ok()?;
    let V::A(fields) = cbor::decode(pt).ok()? else {
        return None;
    };
    let [V::U(1), V::B(signature), vote, recipient] = fields.as_slice() else {
        return None;
    };
    let pk = committed_key(head, creds, &target)?;
    sig::verify(&pk, &request_proof(proof, recipient), signature).ok()?;
    match (c.remove, vote, recipient) {
        (false, V::Null, V::B(pk)) if pk.len() == hpke::NPK => {
            Some(VerifiedRequest::Promotion(pk.clone()))
        }
        (true, V::B(signed), V::Null) => {
            let (_, body, signers) = control::open_signed(signed, creds, Some(head)).ok()?;
            (hex(&body) == c.child && signers.len() == 1 && signers.contains(&target))
                .then(|| VerifiedRequest::Vote(signed.clone()))
        }
        _ => None,
    }
}

pub(super) fn poll_target(st: &ProfileState, now_ms: u64) -> Option<PollTarget> {
    let c = st
        .app
        .controller_change
        .as_ref()
        .filter(|c| c.expires_at > now_ms && !c.b_recv.is_empty())?;
    Some(PollTarget {
        mailbox: unhex(&c.mailbox).ok()?,
        after: st.app.cursors.get(&c.mailbox).copied().unwrap_or(0),
        kind: MailboxKind::Relay,
        relay_uri: Some(c.relay_uri.clone()),
        capability: if c.response_cap.is_some() {
            vec![]
        } else {
            unhex(&c.b_recv).ok()?
        },
    })
}

pub(super) fn ack_route(st: &ProfileState, mailbox: &str) -> Option<(String, Vec<u8>)> {
    let c = st
        .app
        .controller_change
        .as_ref()
        .filter(|c| c.mailbox == mailbox)?;
    let cap = if c.response_cap.is_some() {
        vec![]
    } else {
        unhex(&c.b_recv).ok()?
    };
    Some((c.relay_uri.clone(), cap))
}

pub(crate) fn view(st: &ProfileState, head: &Acs, now_ms: u64) -> Option<ControllerChangeView> {
    if !st.app.acs_forks.is_empty() {
        return None;
    }
    let c = st
        .app
        .controller_change
        .as_ref()
        .filter(|c| c.expires_at > now_ms && !c.b_recv.is_empty())?;
    let (keys, proof) = keys(c, head)?;
    let host = c.response_cap.is_some();
    let waiting = if host {
        c.request.is_none()
    } else {
        c.confirmed
    };
    Some(ControllerChangeView {
        request_id: hex(&keys.id),
        device_id: c.target.clone(),
        remove: c.remove,
        host,
        waiting,
        verification_code: Some(ra::code_digits(&keys.code, &proof)),
        link: if host && waiting { link(c) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_ceremony_matches_the_independent_transcript_vector() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../verification/vectors/controller-change-v1.json"
        )))
        .unwrap();
        let expected = |entry: &str| vector[entry].as_str().unwrap();
        let offered = expected("link");
        let (remove, origin, offer) = parse_link(offered).unwrap();
        assert!(ra::decode_offer(offered).is_err());
        let token = offered.rsplit('/').next().unwrap();
        assert!(parse_link(&format!("{}{token}", ra::OFFER_PREFIX)).is_none());
        let old_version = format!("{}{token}", ra::OFFER_PREFIX);
        let mut raw = unb64url(old_version.strip_prefix(ra::OFFER_PREFIX).unwrap()).unwrap();
        raw[0] = 2;
        assert!(parse_link(&format!("{PREFIX}0/{}/{}", hex(&origin), b64url(&raw))).is_none());
        let c = ControllerChange {
            operation_id: "vector".into(),
            expires_at: u64::from(offer.expiry_s) * 1000,
            secret: hex(&offer.secret).into(),
            a_cap: hex(&offer.a_cap).into(),
            b_recv: hex(&offer.b_recv).into(),
            mailbox: String::new(),
            relay_uri: offer.uri,
            origin: hex(&origin),
            target: String::new(),
            parent: String::new(),
            child: String::new(),
            remove,
            response_cap: None,
            confirmed: false,
            request: None,
            recipient_seed: None,
        };
        assert_eq!(link(&c).as_deref(), Some(offered));
        let binding = cbor::encode(&V::A(vec![
            V::T(PURPOSE.into()),
            V::U(0),
            V::B(vec![0x11; 32]),
            V::B(vec![0x22; 32]),
            V::B(origin.to_vec()),
            V::B(vec![0x42; 16]),
            V::B(vec![0x53; 32]),
            V::B(vec![0x64; 32]),
        ]));
        assert_eq!(hex(&binding), expected("binding"));
        let (keys, proof) = transcript(&c, binding).unwrap();
        assert_eq!(hex(&keys.id), expected("ceremony_id"));
        assert_eq!(hex(&proof), expected("proof"));
        for (name, key) in [
            ("req", keys.req),
            ("resp", keys.resp),
            ("code", keys.code),
            ("bind", keys.bind),
        ] {
            assert_eq!(hex(&key), vector["keys"][name].as_str().unwrap());
        }
        assert_eq!(ra::code_digits(&keys.code, &proof), expected("code"));
    }
}
