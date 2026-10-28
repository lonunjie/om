// SPDX-License-Identifier: MPL-2.0
//! Core transitions: every function here runs inside one staged transaction (`Tx`), so a failed host commit
//! discards all of it and nothing is released. The persisted outbox carries the external effects; a transition
//! never performs I/O. First contact follows INVITES_INTROGATE_AND_SPAM.md as far as pass 1 goes (see
//! docs/reports/2026-10-28-core-kernel-m1-status.md "Spec questions" for what is deliberately not built).

use crate::api::*;
use crate::crypto::binding::{self, Acs, PeerView, RelationshipContext, TrustedPeer};
use crate::crypto::cbor::{self, V};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{digest, ds_hash};
use crate::crypto::{self, PROFILE_V1, hex, hpke, unhex};
use crate::host::{CoreError, NetCall, NetError, NetResult};
use crate::state::*;
use std::sync::OnceLock;

pub const INVITE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const BUNDLE_TTL_S: u64 = 90 * 24 * 60 * 60;
pub const MAX_MESSAGE_CHARS: usize = 8000;
pub const MAX_LABEL_CHARS: usize = 64;
const ONE_TIME_KEYS: usize = 10;
const KIND_TEXT: u64 = 1;
const KIND_HELLO: u64 = 2;
const LBL_REQUEST_KEY: &str = "OM-REQUEST-KEY-HASH-v1";

pub fn registry() -> &'static om_codec::Registry {
    static R: OnceLock<om_codec::Registry> = OnceLock::new();
    R.get_or_init(om_codec::Registry::load)
}

// ---------------------------------------------------------------------------------------------- small codecs

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64url(b: &[u8]) -> String {
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn unb64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in s.bytes() {
        let v = B64.iter().position(|&x| x == ch)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    // canonical: leftover bits must be zero and the length must be a valid unpadded length
    (s.len() % 4 != 1 && acc & ((1 << bits) - 1) == 0).then_some(out)
}

/// `om:invite/<token>`: the same strict grammar as `parseInviteLink` in the platform API.
pub fn parse_invite_token(input: &str) -> Option<&str> {
    let s = input.trim();
    if s.len() > 256 {
        return None;
    }
    let t = s.strip_prefix("om:invite/")?;
    ((16..=200).contains(&t.len())
        && t.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(t)
}

fn h16(s: &str) -> Result<[u8; 16], CoreError> {
    unhex(s)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(CoreError::CorruptState("bad id"))
}
fn h32(s: &str) -> Result<[u8; 32], CoreError> {
    unhex(s)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(CoreError::CorruptState("bad key"))
}
fn hb(s: &str) -> Result<Vec<u8>, CoreError> {
    unhex(s).map_err(|_| CoreError::CorruptState("bad hex"))
}

/// Display names come from a peer: bounded, no control characters.
pub fn sanitize_label(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Recovery key display: 32 random bytes plus a 3-byte check, RFC 4648 base32, 14 groups of 4 (transcription aid only).
fn recovery_groups(root: &[u8; 32]) -> Vec<String> {
    let chk = crypto::sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", root]);
    let mut raw = root.to_vec();
    raw.extend_from_slice(&chk[..3]);
    const A: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut s = String::new();
    for c in raw.chunks(5) {
        let mut n = 0u64;
        for i in 0..5 {
            n = n << 8 | *c.get(i).unwrap_or(&0) as u64;
        }
        for i in 0..8 {
            s.push(A[(n >> (35 - 5 * i) & 31) as usize] as char);
        }
    }
    s.truncate(56);
    s.as_bytes()
        .chunks(4)
        .map(|g| String::from_utf8_lossy(g).into_owned())
        .collect()
}

// ---------------------------------------------------------------------------------------------- transaction

pub struct Tx<'a, R: RngCore + CryptoRng> {
    pub st: &'a mut ProfileState,
    pub rng: &'a mut R,
    pub now_ms: u64,
    pub changed: Vec<OperationId>,
    /// Profiles currently open (SwitchIdentityProfile target check).
    pub open_profiles: &'a [String],
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    fn now_s(&self) -> u64 {
        self.now_ms / 1000
    }

    fn rand16(&mut self) -> [u8; 16] {
        let mut b = [0u8; 16];
        self.rng.fill_bytes(&mut b);
        b
    }

    fn rand_id(&mut self, prefix: &str) -> String {
        let mut b = [0u8; 8];
        self.rng.fill_bytes(&mut b);
        format!("{prefix}-{}", hex(&b))
    }

    fn touch(&mut self, op: &OperationId) {
        if !self.changed.contains(op) {
            self.changed.push(op.clone());
        }
    }

    fn record(
        &mut self,
        op: &OperationId,
        kind: &str,
        outcome: OutcomeClass,
        message_id: Option<String>,
    ) {
        self.st.app.ops.insert(
            op.clone(),
            OpRec {
                operation_id: op.clone(),
                kind: kind.into(),
                outcome,
                message_id,
            },
        );
        self.touch(op);
    }

    fn set_outcome(&mut self, op: &OperationId, outcome: OutcomeClass) {
        if let Some(r) = self.st.app.ops.get_mut(op)
            && r.outcome != outcome
        {
            r.outcome = outcome;
            self.touch(op);
        }
    }

    fn enqueue(&mut self, call: NetCall, purpose: Purpose) -> u64 {
        let id = self.st.app.next_net_id;
        self.st.app.next_net_id += 1;
        self.st.app.outbox.push(OutboxEntry { id, call, purpose });
        id
    }

    fn device(&mut self) -> Result<&mut crate::crypto::store::Device, CoreError> {
        self.st
            .device
            .as_mut()
            .ok_or(CoreError::Invalid("no identity"))
    }

    // ------------------------------------------------------------------------------------------ intents

    pub fn apply_intent(&mut self, intent: &Intent) -> Result<(), CoreError> {
        let op = &intent.operation_id;
        let kind = intent.body.kind();
        // UI-11: destructive intents never run on a bare dispatch.
        if intent.body.confirmation().is_some_and(|c| c.is_null()) {
            self.record(
                op,
                kind,
                OutcomeClass::Rejected {
                    reason: RejectReason::Unauthorized,
                },
                None,
            );
            return Ok(());
        }
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let unsupported = rejected(RejectReason::Unsupported);
        let outcome = match &intent.body {
            IntentBody::CreateIdentityProfile { label } => self.create_identity(op, label),
            IntentBody::ConfirmRecoveryKeyStored { key_operation_id } => {
                match &self.st.app.recovery_key {
                    Some(k) if &k.key_operation_id == key_operation_id => {
                        self.st.app.recovery_key = None;
                        self.st.app.recovery_key_saved = true;
                        OutcomeClass::Committed
                    }
                    _ => rejected(RejectReason::NotFound),
                }
            }
            IntentBody::SwitchIdentityProfile { target_profile_id } => {
                if self.open_profiles.contains(target_profile_id) {
                    OutcomeClass::Committed
                } else {
                    rejected(RejectReason::NotFound)
                }
            }
            IntentBody::CreateInvite => self.create_invite(op),
            IntentBody::RevokeInvite { invite_id } => self.revoke_invite(op, invite_id),
            IntentBody::AcceptInvitation { link } => self.accept_invitation(op, link),
            IntentBody::AcceptIntroduction { request_id } => {
                self.accept_introduction(op, request_id)
            }
            IntentBody::SendMessage {
                conversation_id,
                draft,
            } => return self.send_message(op, conversation_id, &draft.text),
            IntentBody::RetryDelivery { message_id } => return self.retry_delivery(op, message_id),
            IntentBody::BlockPerson {
                person_id, scope, ..
            } => match scope {
                BlockScope::Provider => unsupported,
                BlockScope::Local => match self
                    .st
                    .app
                    .relationships
                    .values_mut()
                    .find(|r| &r.person_id == person_id)
                {
                    Some(r) => {
                        r.blocked = true;
                        OutcomeClass::Committed
                    }
                    None => rejected(RejectReason::NotFound),
                },
            },
            // Presentation-only: Core holds no navigation state, so these are vacuously committed (ACTION_TO_CORE_INTENT_MAP.md).
            IntentBody::ClearAdaptiveNavigation
            | IntentBody::OpenOmnibarResult { .. }
            | IntentBody::PinNavigationTarget { .. } => OutcomeClass::Committed,
            // Not built in pass 1 (status report "What remains"); Rejected(unsupported) is the conservative answer.
            IntentBody::ShareContact { .. }
            | IntentBody::SuspendDevice { .. }
            | IntentBody::ChangePrivacyPolicy { .. }
            | IntentBody::ShareStaticLocation { .. }
            | IntentBody::StartCall { .. }
            | IntentBody::RestoreHistory { .. }
            | IntentBody::OfferDeviceLink
            | IntentBody::RequestDeviceLink { .. }
            | IntentBody::ApproveDeviceLink { .. }
            | IntentBody::RejectDeviceLink { .. }
            | IntentBody::RevokeDevice { .. }
            | IntentBody::StartRecovery { .. }
            | IntentBody::VetoRecovery { .. } => unsupported,
        };
        let msg = None;
        self.record(op, kind, outcome, msg);
        Ok(())
    }

    fn create_identity(&mut self, op: &OperationId, label: &str) -> OutcomeClass {
        let label = label.trim();
        if label.is_empty()
            || label.chars().count() > MAX_LABEL_CHARS
            || label.chars().any(char::is_control)
        {
            return OutcomeClass::Rejected {
                reason: RejectReason::Invalid,
            };
        }
        if self.st.device.is_some() {
            return OutcomeClass::Rejected {
                reason: RejectReason::Conflict,
            };
        }
        let now_s = self.now_s();
        let mut device = crate::crypto::store::Device::new(self.rng);
        device.bundle_expires_at = now_s + BUNDLE_TTL_S;
        let up = device.generate_one_time(ONE_TIME_KEYS, ONE_TIME_KEYS, self.rng);
        let call = NetCall::PublishKeyMaterial {
            device_id: device.device_id.to_vec(),
            bundle: up.bundle,
            opks: up.opks,
            pqopks: up.pqopks,
        };
        self.st.device = Some(device);
        let mut root = arr32(self.rng);
        let groups = recovery_groups(&root);
        zeroize::Zeroize::zeroize(&mut root);
        self.st.app.label = label.to_string();
        self.st.app.created_at = self.now_ms;
        self.st.app.recovery_key = Some(RecoveryKeyRec {
            key_operation_id: op.clone(),
            groups,
        });
        self.enqueue(call, Purpose::PublishKeys);
        OutcomeClass::Committed
    }

    fn create_invite(&mut self, op: &OperationId) -> OutcomeClass {
        let Some(device) = &self.st.device else {
            return OutcomeClass::Rejected {
                reason: RejectReason::StaleState,
            };
        };
        let acs = device.acs();
        let acs_body = acs.body();
        let cred_body = device.credential().body();
        let (acs_hash, label) = (acs.hash(), self.st.app.label.clone());
        let invite_id = self.rand16();
        let mailbox = self.rand16();
        let mut ikm = arr32(self.rng);
        let (seed, pk) = hpke::derive_key_pair(&ikm);
        zeroize::Zeroize::zeroize(&mut ikm);
        let rk_digest = request_key_digest(&pk);
        let expires_at = self.now_ms + INVITE_TTL_MS;
        let descriptor = cbor::encode(&V::M(vec![
            (1, V::B(acs_body)),
            (2, V::A(vec![V::B(cred_body)])),
            (3, V::B(pk)),
            (4, V::B(mailbox.to_vec())),
            (5, V::U(expires_at / 1000)),
            (6, V::T(label)),
        ]));
        // The link commits to the ACS head and the request key, so the (untrusted) introduction service cannot
        // substitute either (QR/link possession is a rendezvous capability, not relationship authority).
        let mut token = invite_id.to_vec();
        token.extend_from_slice(&acs_hash);
        token.extend_from_slice(&rk_digest);
        let link = format!("om:invite/{}", b64url(&token));
        self.st.app.invites.insert(
            op.clone(),
            InviteRec {
                operation_id: op.clone(),
                invite_id: hex(&invite_id),
                link,
                expires_at,
                mailbox: hex(&mailbox),
                request_seed: hex(&seed),
                request_digest: hex(&rk_digest),
                revoked: false,
                used: false,
            },
        );
        self.enqueue(
            NetCall::PutInvite {
                invite_id: invite_id.to_vec(),
                descriptor,
                expires_at,
            },
            Purpose::PutInvite {
                operation_id: op.clone(),
            },
        );
        OutcomeClass::PendingNetwork
    }

    fn revoke_invite(&mut self, op: &OperationId, invite_op: &str) -> OutcomeClass {
        let Some(inv) = self.st.app.invites.get_mut(invite_op) else {
            return OutcomeClass::Rejected {
                reason: RejectReason::NotFound,
            };
        };
        inv.revoked = true;
        let id = inv.invite_id.clone();
        match hb(&id) {
            Ok(id) => {
                self.enqueue(
                    NetCall::RevokeInvite { invite_id: id },
                    Purpose::RevokeInvite {
                        operation_id: op.clone(),
                    },
                );
                OutcomeClass::PendingNetwork
            }
            Err(_) => OutcomeClass::Rejected {
                reason: RejectReason::IntegrityFailure,
            },
        }
    }

    fn accept_invitation(&mut self, op: &OperationId, link: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        if self.st.device.is_none() {
            return rejected(RejectReason::StaleState);
        }
        let Some(token) = parse_invite_token(link) else {
            return rejected(RejectReason::Invalid);
        };
        let Some(raw) = unb64url(token).filter(|r| r.len() == 80) else {
            return rejected(RejectReason::Invalid);
        };
        if self.st.app.invites.values().any(|i| i.link == link.trim()) {
            return rejected(RejectReason::Invalid); // our own invite
        }
        let invite_id = hex(&raw[..16]);
        if self.st.app.accepted_invites.contains(&invite_id) {
            return rejected(RejectReason::Conflict);
        }
        self.enqueue(
            NetCall::GetInvite {
                invite_id: raw[..16].to_vec(),
            },
            Purpose::ResolveInvite {
                operation_id: op.clone(),
                invite_id,
                acs_hash: hex(&raw[16..48]),
                request_digest: hex(&raw[48..]),
            },
        );
        OutcomeClass::PendingNetwork
    }

    fn accept_introduction(&mut self, op: &OperationId, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(intro) = self.st.app.intros.get(request_id).cloned() else {
            return rejected(RejectReason::NotFound);
        };
        let built = (|| -> Result<(Relationship, String), CoreError> {
            let creds = intro
                .peer_credentials
                .iter()
                .map(|c| hb(c))
                .collect::<Result<Vec<_>, _>>()?;
            let acs_body = hb(&intro.peer_acs_body)?;
            let acs = Acs::from_body(&acs_body).map_err(|_| CoreError::Invalid("peer acs"))?;
            let peer = TrustedPeer {
                acs_body: acs_body.clone(),
                credential_bodies: creds,
            };
            let dev = binding::peer_devices(&peer, &acs.hash())
                .map_err(|_| CoreError::Invalid("peer devices"))?;
            let (ik, account, device_id, _) = dev
                .into_iter()
                .find(|d| d.3)
                .ok_or(CoreError::Invalid("no active peer device"))?;
            let rel = Relationship {
                id: String::new(),
                person_id: intro.person_id.clone(),
                display_name: intro.display_name.clone(),
                state: RelState::Establishing,
                peer_account_id: hex(&account),
                peer_device_id: hex(&device_id),
                peer_ik: hex(&ik),
                peer_acs_body: intro.peer_acs_body.clone(),
                peer_credentials: intro.peer_credentials.clone(),
                my_mailbox: String::new(),
                peer_mailbox: Some(intro.peer_mailbox.clone()),
                blocked: false,
                integrity_failures: 0,
                created_at: self.now_ms,
                messages: vec![],
                seen_envelopes: vec![],
            };
            Ok((rel, hex(&device_id)))
        })();
        let Ok((mut rel, device_hex)) = built else {
            return rejected(RejectReason::IntegrityFailure);
        };
        // Pin the peer's ACS head (K7 monotonic) before anything keyed to it happens (RelationshipContext is mandatory).
        let creds: Vec<Vec<u8>> = rel
            .peer_credentials
            .iter()
            .filter_map(|c| unhex(c).ok())
            .collect();
        let Ok(acs_body) = hb(&rel.peer_acs_body) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let Ok(device) = self.device() else {
            return rejected(RejectReason::StaleState);
        };
        if device.trust_peer(acs_body, creds).is_err() {
            return rejected(RejectReason::StaleState);
        }
        let mb = self.rand16();
        let conv = self.rand_id("conv");
        rel.id = conv.clone();
        rel.my_mailbox = hex(&mb);
        self.st.app.relationships.insert(conv.clone(), rel);
        self.st.app.intros.remove(request_id);
        let Ok(did) = hb(&device_hex) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        self.enqueue(
            NetCall::FetchKeyMaterial { device_id: did },
            Purpose::FetchPeerKeys {
                operation_id: op.clone(),
                conversation_id: conv,
            },
        );
        OutcomeClass::PendingNetwork
    }

    fn send_message(&mut self, op: &OperationId, conv: &str, text: &str) -> Result<(), CoreError> {
        let kind = "SendMessage";
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get(conv) else {
            self.record(op, kind, rejected(RejectReason::NotFound), None);
            return Ok(());
        };
        if text.trim().is_empty() || text.chars().count() > MAX_MESSAGE_CHARS {
            self.record(op, kind, rejected(RejectReason::Invalid), None);
            return Ok(());
        }
        let (RelState::Active, Some(peer_mailbox), false) =
            (rel.state, rel.peer_mailbox.clone(), rel.blocked)
        else {
            self.record(op, kind, rejected(RejectReason::Unavailable), None);
            return Ok(());
        };
        let peer_ik = h32(&rel.peer_ik)?;
        let mut mid = [0u8; 16];
        self.rng.fill_bytes(&mut mid);
        let message_id = format!("msg-{}", hex(&mid));
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_TEXT)),
            (1, V::B(mid.to_vec())),
            (2, V::T(text.to_string())),
            (3, V::U(self.now_ms)),
        ]));
        // The ratchet advance and the outbox entry are one staged transaction: persisted before the first external effect.
        let (env, _) = match self.device()?.encrypt(&peer_ik, &payload) {
            Ok(x) => x,
            Err(_) => {
                self.record(op, kind, rejected(RejectReason::Unavailable), None);
                return Ok(());
            }
        };
        let now = self.now_ms;
        let rel = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .ok_or(CoreError::NotFound("conversation"))?;
        rel.messages.push(Msg {
            id: message_id.clone(),
            outgoing: true,
            operation_id: Some(op.clone()),
            text: text.to_string(),
            sent_at: now,
            delivery: DeliveryState::PendingNetwork,
            envelope: Some(hex(&env)),
        });
        self.enqueue(
            NetCall::Deposit {
                mailbox: hb(&peer_mailbox)?,
                bytes: env,
            },
            Purpose::DepositMessage {
                operation_id: op.clone(),
                conversation_id: conv.to_string(),
                message_id: message_id.clone(),
            },
        );
        self.record(op, kind, OutcomeClass::PendingNetwork, Some(message_id));
        Ok(())
    }

    fn retry_delivery(&mut self, op: &OperationId, message_id: &str) -> Result<(), CoreError> {
        let kind = "RetryDelivery";
        let found = self.st.app.relationships.values().find_map(|r| {
            r.messages
                .iter()
                .find(|m| m.id == message_id && m.outgoing)
                .map(|m| (r.id.clone(), m.clone()))
        });
        let Some((conv, msg)) = found else {
            self.record(
                op,
                kind,
                OutcomeClass::Rejected {
                    reason: RejectReason::NotFound,
                },
                None,
            );
            return Ok(());
        };
        let orig = msg
            .operation_id
            .clone()
            .ok_or(CoreError::NotFound("message operation"))?;
        // Same logical operation: while it is not terminal there is nothing to retry; the outbox already holds it.
        if self
            .st
            .app
            .ops
            .get(&orig)
            .is_some_and(|r| !r.outcome.is_terminal())
        {
            self.record(op, kind, OutcomeClass::PendingNetwork, Some(msg.id));
            return Ok(());
        }
        match (&msg.delivery, &msg.envelope) {
            (DeliveryState::Rejected { reason }, Some(env)) if reason.retryable() => {
                // Resend the very same envelope: re-running the ratchet would create a second semantic send.
                let (peer_mailbox, env) = (
                    self.st
                        .app
                        .relationships
                        .get(&conv)
                        .and_then(|r| r.peer_mailbox.clone()),
                    hb(env)?,
                );
                let Some(peer_mailbox) = peer_mailbox else {
                    self.record(
                        op,
                        kind,
                        OutcomeClass::Rejected {
                            reason: RejectReason::Unavailable,
                        },
                        Some(msg.id),
                    );
                    return Ok(());
                };
                self.set_delivery(&conv, &msg.id, DeliveryState::PendingNetwork);
                self.enqueue(
                    NetCall::Deposit {
                        mailbox: hb(&peer_mailbox)?,
                        bytes: env,
                    },
                    Purpose::DepositMessage {
                        operation_id: op.clone(),
                        conversation_id: conv,
                        message_id: msg.id.clone(),
                    },
                );
                self.record(op, kind, OutcomeClass::PendingNetwork, Some(msg.id));
            }
            (DeliveryState::Rejected { .. }, _) => self.record(
                op,
                kind,
                OutcomeClass::Rejected {
                    reason: RejectReason::Conflict,
                },
                Some(msg.id),
            ),
            _ => self.record(op, kind, OutcomeClass::Committed, Some(msg.id)),
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------ network results

    /// Apply the result of outbox entry `net_id`. Unknown ids are ignored (a duplicate result after a restart).
    pub fn net_result(&mut self, net_id: u64, result: NetResult) -> Result<(), CoreError> {
        let Some(pos) = self.st.app.outbox.iter().position(|e| e.id == net_id) else {
            return Ok(());
        };
        let entry = self.st.app.outbox[pos].clone();
        // Classify: Ok/data, retry-in-place (definitely-not-sent or read-only), maybe-sent, or terminal.
        enum K {
            Done(Option<Vec<u8>>, Vec<Vec<u8>>),
            Retry,
            MaybeSent,
            Terminal(RejectReason),
        }
        let k = match result {
            NetResult::Ok => K::Done(None, vec![]),
            NetResult::Bytes { bytes } => K::Done(Some(bytes), vec![]),
            NetResult::Many { items } => K::Done(None, items),
            NetResult::Err {
                error: NetError::Unavailable,
            } => K::Retry,
            NetResult::Err {
                error: NetError::Timeout,
            } => match entry.call {
                NetCall::Deposit { .. }
                | NetCall::PutInvite { .. }
                | NetCall::RevokeInvite { .. } => K::MaybeSent,
                _ => K::Retry,
            },
            NetResult::Err {
                error: NetError::NotFound,
            } => match entry.call {
                NetCall::GetInvite { .. } => K::Terminal(RejectReason::NotFound),
                NetCall::FetchKeyMaterial { .. } => K::Retry, // the peer may not have published yet
                _ => K::Terminal(RejectReason::NotFound),
            },
            NetResult::Err {
                error: NetError::Rejected { reason },
            } => K::Terminal(reason),
        };
        let op_of = |p: &Purpose| -> Option<(OperationId, Option<String>, Option<String>)> {
            match p {
                Purpose::PutInvite { operation_id }
                | Purpose::RevokeInvite { operation_id }
                | Purpose::ResolveInvite { operation_id, .. } => {
                    Some((operation_id.clone(), None, None))
                }
                Purpose::DepositRequest {
                    operation_id,
                    conversation_id,
                }
                | Purpose::FetchPeerKeys {
                    operation_id,
                    conversation_id,
                }
                | Purpose::DepositHello {
                    operation_id,
                    conversation_id,
                } => Some((operation_id.clone(), Some(conversation_id.clone()), None)),
                Purpose::DepositMessage {
                    operation_id,
                    conversation_id,
                    message_id,
                } => Some((
                    operation_id.clone(),
                    Some(conversation_id.clone()),
                    Some(message_id.clone()),
                )),
                Purpose::PublishKeys | Purpose::Ack => None,
            }
        };
        let info = op_of(&entry.purpose);
        match k {
            K::Retry => {
                if let Some((op, ..)) = &info {
                    self.set_outcome(op, OutcomeClass::PendingNetwork);
                }
                if let Some((_, Some(c), Some(m))) = &info {
                    self.set_delivery(c, m, DeliveryState::PendingNetwork);
                }
                Ok(()) // stays in the outbox, identity unchanged
            }
            K::MaybeSent => {
                // A timeout after the bytes may have left is never a rejection (UI-12): reconcile by exact retry.
                if let Some((op, ..)) = &info {
                    self.set_outcome(op, OutcomeClass::PendingReconciliation);
                }
                if let Some((_, Some(c), Some(m))) = &info {
                    self.set_delivery(c, m, DeliveryState::PendingReconciliation);
                }
                Ok(())
            }
            K::Terminal(reason) => {
                self.st.app.outbox.remove(pos);
                if let Some((op, conv, msg)) = info {
                    self.set_outcome(&op, OutcomeClass::Rejected { reason });
                    if let (Some(c), Some(m)) = (&conv, &msg) {
                        self.set_delivery(c, m, DeliveryState::Rejected { reason });
                    }
                    if matches!(entry.purpose, Purpose::DepositRequest { .. })
                        && let Some(c) = conv
                    {
                        self.st.app.relationships.remove(&c);
                    }
                }
                Ok(())
            }
            K::Done(bytes, items) => {
                self.st.app.outbox.remove(pos);
                self.complete(entry, bytes, items)
            }
        }
    }

    fn set_delivery(&mut self, conv: &str, message_id: &str, d: DeliveryState) {
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == message_id))
        {
            m.delivery = d;
        }
    }

    fn complete(
        &mut self,
        entry: OutboxEntry,
        bytes: Option<Vec<u8>>,
        items: Vec<Vec<u8>>,
    ) -> Result<(), CoreError> {
        match entry.purpose {
            Purpose::PublishKeys => self.st.app.keys_published = true,
            Purpose::Ack => {}
            Purpose::PutInvite { operation_id } | Purpose::RevokeInvite { operation_id } => {
                self.set_outcome(&operation_id, OutcomeClass::Committed)
            }
            Purpose::DepositRequest { operation_id, .. } => {
                self.set_outcome(&operation_id, OutcomeClass::Committed)
            }
            Purpose::DepositHello {
                operation_id,
                conversation_id,
            } => {
                if let Some(r) = self.st.app.relationships.get_mut(&conversation_id) {
                    r.state = RelState::Active;
                }
                self.set_outcome(&operation_id, OutcomeClass::Committed);
            }
            Purpose::DepositMessage {
                operation_id,
                conversation_id,
                message_id,
            } => {
                self.set_delivery(&conversation_id, &message_id, DeliveryState::Sent);
                if let Some(m) = self
                    .st
                    .app
                    .relationships
                    .get_mut(&conversation_id)
                    .and_then(|r| r.messages.iter_mut().find(|m| m.id == message_id))
                {
                    m.envelope = None;
                }
                self.set_outcome(&operation_id, OutcomeClass::Committed);
            }
            Purpose::ResolveInvite {
                operation_id,
                invite_id,
                acs_hash,
                request_digest,
            } => {
                let out = self.on_invite_resolved(
                    &operation_id,
                    &invite_id,
                    &acs_hash,
                    &request_digest,
                    bytes.unwrap_or_default(),
                );
                self.set_outcome(&operation_id, out);
            }
            Purpose::FetchPeerKeys {
                operation_id,
                conversation_id,
            } => {
                let out = self.on_peer_keys(&operation_id, &conversation_id, items);
                self.set_outcome(&operation_id, out);
            }
        }
        Ok(())
    }

    fn on_invite_resolved(
        &mut self,
        op: &OperationId,
        invite_id: &str,
        acs_hash: &str,
        rk_digest: &str,
        descriptor: Vec<u8>,
    ) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        let parsed = (|| -> Result<_, CoreError> {
            let v = cbor::decode(&descriptor).map_err(|_| CoreError::Invalid("descriptor"))?;
            let acs_body = v
                .get(1)
                .and_then(|x| x.b())
                .map_err(|_| CoreError::Invalid("acs"))?
                .to_vec();
            let creds: Vec<Vec<u8>> = v
                .get(2)
                .and_then(|x| x.a())
                .map_err(|_| CoreError::Invalid("creds"))?
                .iter()
                .map(|c| c.b().map(<[u8]>::to_vec))
                .collect::<Result<_, _>>()
                .map_err(|_| CoreError::Invalid("creds"))?;
            let pk = v
                .get(3)
                .and_then(|x| x.b())
                .map_err(|_| CoreError::Invalid("pk"))?
                .to_vec();
            let mailbox: [u8; 16] = v
                .get(4)
                .and_then(|x| x.b())
                .ok()
                .and_then(|b| b.try_into().ok())
                .ok_or(CoreError::Invalid("mailbox"))?;
            let expires = v
                .get(5)
                .and_then(|x| x.u())
                .map_err(|_| CoreError::Invalid("expiry"))?;
            let label = v
                .get(6)
                .and_then(|x| x.t())
                .map(sanitize_label)
                .map_err(|_| CoreError::Invalid("label"))?;
            Ok((acs_body, creds, pk, mailbox, expires, label))
        })();
        let Ok((acs_body, creds, pk, mailbox, expires, label)) = parsed else {
            return rej(IntegrityFailure);
        };
        // Everything the introduction service returned is checked against what the link committed to.
        let Ok(acs) = Acs::from_body(&acs_body) else {
            return rej(IntegrityFailure);
        };
        if hex(&acs.hash()) != acs_hash || hex(&request_key_digest(&pk)) != rk_digest {
            return rej(IntegrityFailure);
        }
        if expires <= self.now_s() {
            return rej(Expired);
        }
        let peer = TrustedPeer {
            acs_body: acs_body.clone(),
            credential_bodies: creds.clone(),
        };
        let Ok(devs) = binding::peer_devices(&peer, &acs.hash()) else {
            return rej(IntegrityFailure);
        };
        let Some((ik, account, device_id, _)) = devs.into_iter().find(|d| d.3) else {
            return rej(IntegrityFailure);
        };
        let own = self.st.device.as_ref().map(|d| d.account_id);
        if own == Some(account) {
            return rej(Invalid);
        }
        // Pin the inviter's ACS head named by the link, then send the (HPKE-sealed) request.
        let Ok(dev) = self.device() else {
            return rej(StaleState);
        };
        if dev.trust_peer(acs_body.clone(), creds.clone()).is_err() {
            return rej(StaleState);
        }
        let my_acs = dev.acs().body();
        let my_cred = dev.credential().body();
        let (Ok(invite16), Ok(rk)) = (h16(invite_id), h32(rk_digest)) else {
            return rej(IntegrityFailure);
        };
        let my_mailbox = self.rand16();
        let label_me = self.st.app.label.clone();
        let payload = cbor::encode(&V::M(vec![
            (1, V::B(my_acs)),
            (2, V::A(vec![V::B(my_cred)])),
            (3, V::B(my_mailbox.to_vec())),
            (4, V::T(label_me)),
            (5, V::U(self.now_ms)),
        ]));
        let mut rnd = [0u8; 64];
        self.rng.fill_bytes(&mut rnd);
        let Ok((ss, enc)) = hpke::encaps_derand(&pk, &rnd) else {
            return rej(IntegrityFailure);
        };
        let info = hpke::message_request_info(PROFILE_V1, &invite16, &rk);
        let ct = hpke::key_schedule(&ss, &info).seal(b"", &payload);
        zeroize::Zeroize::zeroize(&mut rnd);
        // The W0 MessageRequest object (spec/cddl/discovery.cddl); it carries no sender identity in the clear.
        let request = cbor::encode(&V::M(vec![
            (0, V::U(19)),
            (1, V::U(1)),
            (2, V::B(invite16.to_vec())),
            (3, digest(&rk)),
            (4, V::U(PROFILE_V1)),
            (5, V::B(enc)),
            (6, V::B(ct)),
        ]));
        let conv = self.rand_id("conv");
        let person = self.rand_id("person");
        self.st.app.accepted_invites.insert(invite_id.to_string());
        self.st.app.relationships.insert(
            conv.clone(),
            Relationship {
                id: conv.clone(),
                person_id: person,
                display_name: if label.is_empty() {
                    "New contact".into()
                } else {
                    label
                },
                state: RelState::AwaitingAcceptance,
                peer_account_id: hex(&account),
                peer_device_id: hex(&device_id),
                peer_ik: hex(&ik),
                peer_acs_body: hex(&acs_body),
                peer_credentials: creds.iter().map(|c| hex(c)).collect(),
                my_mailbox: hex(&my_mailbox),
                peer_mailbox: None,
                blocked: false,
                integrity_failures: 0,
                created_at: self.now_ms,
                messages: vec![],
                seen_envelopes: vec![],
            },
        );
        self.enqueue(
            NetCall::Deposit {
                mailbox: mailbox.to_vec(),
                bytes: request,
            },
            Purpose::DepositRequest {
                operation_id: op.clone(),
                conversation_id: conv,
            },
        );
        OutcomeClass::PendingNetwork
    }

    fn on_peer_keys(&mut self, op: &OperationId, conv: &str, fetch: Vec<Vec<u8>>) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return rej(NotFound);
        };
        if registry()
            .check(
                "AsyncKeyBundle",
                fetch.first().map(Vec::as_slice).unwrap_or(&[]),
                false,
            )
            .is_err()
        {
            return rej(IntegrityFailure); // not even a W0-valid bundle: fail closed
        }
        let (Ok(acs_body), Some(cred)) = (
            hb(&rel.peer_acs_body),
            rel.peer_credentials
                .iter()
                .filter_map(|c| unhex(c).ok())
                .find(|c| {
                    crate::crypto::binding::Credential::from_body(c)
                        .is_ok_and(|c| hex(&c.device_id) == rel.peer_device_id)
                }),
        ) else {
            return rej(IntegrityFailure);
        };
        let trusted = ds_hash(binding::LBL_ACS_HASH, &acs_body);
        let now_s = self.now_s();
        let (peer_ik, label) = (h32(&rel.peer_ik), self.st.app.label.clone());
        let Ok(peer_ik) = peer_ik else {
            return rej(IntegrityFailure);
        };
        let Some(peer_mailbox) = rel.peer_mailbox.as_deref().and_then(|m| hb(m).ok()) else {
            return rej(IntegrityFailure);
        };
        let view = PeerView {
            acs_body: &acs_body,
            trusted_acs_hash: trusted,
            credential_body: &cred,
            now: now_s,
        };
        let Some(dev) = self.st.device.as_mut() else {
            return rej(StaleState);
        };
        if let Err(e) = dev.start_session(&fetch, &view, self.rng) {
            return rej(match e {
                crypto::Error::NoPrekeysPublished => Unavailable,
                _ => IntegrityFailure,
            });
        }
        let my_mailbox = h16(&rel.my_mailbox).unwrap_or_default();
        let hello = cbor::encode(&V::M(vec![
            (0, V::U(KIND_HELLO)),
            (1, V::B(my_mailbox.to_vec())),
            (2, V::T(label)),
        ]));
        let env = match self.st.device.as_mut().map(|d| d.encrypt(&peer_ik, &hello)) {
            Some(Ok((env, _))) => env,
            _ => return rej(IntegrityFailure),
        };
        self.enqueue(
            NetCall::Deposit {
                mailbox: peer_mailbox,
                bytes: env,
            },
            Purpose::DepositHello {
                operation_id: op.clone(),
                conversation_id: conv.to_string(),
            },
        );
        OutcomeClass::PendingNetwork
    }

    // ------------------------------------------------------------------------------------------ inbound

    /// Process relay items for `mailbox` that are newer than the committed cursor.
    pub fn inbound(&mut self, mailbox: &str, items: Vec<(u64, Vec<u8>)>) -> Result<(), CoreError> {
        let mut cursor = self.st.app.cursors.get(mailbox).copied().unwrap_or(0);
        let mut items = items;
        items.sort_by_key(|(s, _)| *s);
        let mut advanced = false;
        for (seq, bytes) in items {
            if seq <= cursor {
                continue;
            }
            self.inbound_one(mailbox, &bytes);
            cursor = seq;
            advanced = true;
        }
        if advanced {
            self.st.app.cursors.insert(mailbox.to_string(), cursor);
            // One pending Ack per mailbox, carrying the latest committed sequence.
            self.st.app.outbox.retain(
                |e| !matches!(&e.call, NetCall::Ack { mailbox: m, .. } if hex(m) == mailbox),
            );
            if let Ok(m) = hb(mailbox) {
                self.enqueue(
                    NetCall::Ack {
                        mailbox: m,
                        seq: cursor,
                    },
                    Purpose::Ack,
                );
            }
        }
        Ok(())
    }

    fn inbound_one(&mut self, mailbox: &str, bytes: &[u8]) {
        if let Some(op) = self
            .st
            .app
            .invites
            .iter()
            .find(|(_, i)| i.mailbox == mailbox)
            .map(|(k, _)| k.clone())
        {
            self.on_message_request(&op, bytes);
        } else if let Some(conv) = self
            .st
            .app
            .relationships
            .values()
            .find(|r| r.my_mailbox == mailbox)
            .map(|r| r.id.clone())
        {
            self.on_session_envelope(&conv, bytes);
        } // unknown mailbox: not ours, drop
    }

    fn on_message_request(&mut self, invite_op: &str, bytes: &[u8]) {
        let Some(inv) = self.st.app.invites.get(invite_op).cloned() else {
            return;
        };
        // Single-use, unexpired, unrevoked invites only; everything else is dropped before any expensive work.
        if inv.revoked || inv.used || inv.expires_at <= self.now_ms || bytes.len() > 70_000 {
            return;
        }
        if registry().check("MessageRequest", bytes, false).is_err() {
            return;
        }
        let Ok(v) = cbor::decode(bytes) else { return };
        let (Some(ticket), Some(rk), Some(enc), Some(ct)) = (
            v.get(2).and_then(|x| x.b()).ok().map(<[u8]>::to_vec),
            v.get(3).and_then(|d| d.get(1)).and_then(|x| x.b32()).ok(),
            v.get(5).and_then(|x| x.b()).ok().map(<[u8]>::to_vec),
            v.get(6).and_then(|x| x.b()).ok().map(<[u8]>::to_vec),
        ) else {
            return;
        };
        if hex(&ticket) != inv.invite_id || hex(&rk) != inv.request_digest {
            return;
        }
        let (Ok(seed), Ok(ticket16)) = (
            h32(&inv.request_seed),
            <[u8; 16]>::try_from(ticket.as_slice()),
        ) else {
            return;
        };
        let Ok(ss) = hpke::decaps(&seed, &enc) else {
            return;
        };
        let info = hpke::message_request_info(PROFILE_V1, &ticket16, &rk);
        let Ok(pt) = hpke::key_schedule(&ss, &info).open(b"", &ct) else {
            return;
        };
        let Ok(p) = cbor::decode(&pt) else { return };
        let parsed = (|| -> Result<_, crypto::Error> {
            let acs_body = p.get(1)?.b()?.to_vec();
            let creds: Vec<Vec<u8>> = p
                .get(2)?
                .a()?
                .iter()
                .map(|c| c.b().map(<[u8]>::to_vec))
                .collect::<Result<_, _>>()?;
            let mb = p.get(3)?.b()?.to_vec();
            let label = sanitize_label(p.get(4)?.t()?);
            Ok((acs_body, creds, mb, label))
        })();
        let Ok((acs_body, creds, mb, label)) = parsed else {
            return;
        };
        if mb.len() != 16 {
            return;
        }
        // Structural check only: the sender is NOT pinned until the user accepts (AcceptIntroduction).
        let Ok(acs) = Acs::from_body(&acs_body) else {
            return;
        };
        let peer = TrustedPeer {
            acs_body: acs_body.clone(),
            credential_bodies: creds.clone(),
        };
        let Ok(devs) = binding::peer_devices(&peer, &acs.hash()) else {
            return;
        };
        if !devs.iter().any(|d| d.3)
            || self
                .st
                .device
                .as_ref()
                .is_some_and(|d| d.account_id == acs.account_id)
        {
            return;
        }
        if let Some(i) = self.st.app.invites.get_mut(invite_op) {
            i.used = true;
        }
        let request_id = self.rand_id("req");
        let person_id = self.rand_id("person");
        self.st.app.intros.insert(
            request_id.clone(),
            IntroRec {
                request_id,
                invite_operation_id: invite_op.to_string(),
                person_id,
                display_name: if label.is_empty() {
                    "New contact".into()
                } else {
                    label
                },
                peer_acs_body: hex(&acs_body),
                peer_credentials: creds.iter().map(|c| hex(c)).collect(),
                peer_mailbox: hex(&mb),
            },
        );
    }

    fn on_session_envelope(&mut self, conv: &str, env: &[u8]) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        let Ok(account) = h32(&rel.peer_account_id) else {
            return;
        };
        let env_id = hex(&crypto::sha256(&[env]));
        if rel.seen_envelopes.contains(&env_id) {
            return; // redelivery of an envelope already processed (relay at-least-once): not an attack
        }
        let Some(dev) = self.st.device.as_mut() else {
            return;
        };
        // ADR-0137: the relationship context is mandatory and is checked before any session lookup or secret operation.
        let res = dev.decrypt(&RelationshipContext::new(account), env, self.rng);
        let Some(r) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        let pt = match res {
            Ok((_, pt, _)) => pt,
            Err(_) => {
                r.integrity_failures = r.integrity_failures.saturating_add(1);
                return;
            }
        };
        r.seen_envelopes.push(env_id);
        if r.seen_envelopes.len() > 256 {
            r.seen_envelopes.remove(0);
        }
        if r.blocked {
            return;
        }
        let Ok(p) = cbor::decode(&pt) else { return };
        let kind = p.get(0).and_then(|x| x.u()).unwrap_or(0);
        match kind {
            KIND_HELLO => {
                if let Ok(mb) = p.get(1).and_then(|x| x.b()).map(<[u8]>::to_vec)
                    && mb.len() == 16
                    && r.peer_mailbox.is_none()
                {
                    r.peer_mailbox = Some(hex(&mb));
                    r.state = RelState::Active;
                }
            }
            KIND_TEXT => {
                let (Ok(id), Ok(text), Ok(at)) = (
                    p.get(1).and_then(|x| x.b()),
                    p.get(2).and_then(|x| x.t()),
                    p.get(3).and_then(|x| x.u()),
                ) else {
                    return;
                };
                // A payload that decrypts on a session only the pinned peer can create also shows the peer accepted
                // (the Hello that carries its mailbox may be reordered by the relay), so state is not checked here.
                if id.len() != 16 || text.chars().count() > MAX_MESSAGE_CHARS {
                    return;
                }
                let id = format!("msg-{}", hex(id));
                if r.messages.iter().any(|m| m.id == id) {
                    return;
                }
                // The peer's clock is shown as given but never orders authority; arrival order is the timeline.
                r.messages.push(Msg {
                    id,
                    outgoing: false,
                    operation_id: None,
                    text: text.to_string(),
                    sent_at: at,
                    delivery: DeliveryState::Delivered,
                    envelope: None,
                });
            }
            _ => {} // unknown payload kinds from a newer client are ignored, not fatal
        }
    }
}

pub fn request_key_digest(pk: &[u8]) -> [u8; 32] {
    ds_hash(
        LBL_REQUEST_KEY,
        &cbor::encode(&V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(pk.to_vec()))])),
    )
}
