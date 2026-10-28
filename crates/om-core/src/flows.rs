// SPDX-License-Identifier: MPL-2.0
//! Core transitions: every function here runs inside one staged transaction (`Tx`), so a failed host commit
//! discards all of it and nothing is released. The persisted outbox carries the external effects; a transition
//! never performs I/O. First contact follows INVITES_INTROGATE_AND_SPAM.md as far as pass 1 goes (see
//! docs/reports/2026-10-28-core-kernel-m1-status.md "Spec questions" for what is deliberately not built).

use crate::api::*;
use crate::control;
use crate::crypto::binding::{self, Acs, PeerView, RelationshipContext, TrustedPeer};
use crate::crypto::cbor::{self, V};
use crate::crypto::delivery::{self, KeyMaterial};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{digest, ds_hash};
use crate::crypto::{self, PROFILE_V1, hex, hpke, unhex};
use crate::events::{self, Hlc};
use crate::host::{CoreError, NetCall, NetError, NetResult};
use crate::state::*;
use std::sync::OnceLock;

pub const INVITE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const BUNDLE_TTL_S: u64 = 90 * 24 * 60 * 60;
pub const MAX_MESSAGE_CHARS: usize = 8000;
pub const MAX_LABEL_CHARS: usize = 64;
const ONE_TIME_KEYS: usize = 10;
/// Publish a fresh batch of one-time prekeys when fewer than this many of either kind remain locally.
const REPLENISH_BELOW: usize = 4;
const KIND_HELLO: u64 = 2;
/// Post-acceptance control message carrying the sender's signed `SealedDeliveryKeyRecord` (ADR-0142 section 9).
const KIND_KEYS: u64 = 3;
/// Delivery keys live as long as a signed prekey bundle; a sealed wrapper lives a week (never past its key).
pub const DELIVERY_KEY_TTL_S: u64 = 90 * 24 * 60 * 60;
pub const WRAPPER_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Request-scoped bootstrap key lifetime (the bootstrap acceptance deadline is the earlier of route and key expiry).
pub const BOOTSTRAP_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Clock class: wall clock on both sides (display and expiry hints only, never authority). A requester whose clock
/// runs ahead of ours by up to this much is still accepted; anything longer than `BOOTSTRAP_TTL_S` plus this is not
/// (SEALED_SENDER_DELIVERY_PROFILE.md section 7: the request deadline is min(route, key)).
pub const BOOTSTRAP_CLOCK_SKEW_S: u64 = 60 * 60;
/// ADR-0146: at most this many distinct bootstrap wrappers are ever admitted for one request.
pub const MAX_BOOTSTRAP_CANDIDATES: usize = 8;
const LBL_REQUEST_KEY: &str = "OM-REQUEST-KEY-HASH-v1";

/// The head state, the signed chain that leads to it and every credential the account has committed to.
type AcsHead = (Acs, Vec<Vec<u8>>, Vec<Vec<u8>>);

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

    // ------------------------------------------------------------------------------------------ sealed delivery

    /// A fresh receive key for `mailbox` (the private seed stays in this profile's encrypted state).
    fn new_delivery_key(&mut self, mailbox: &[u8], ttl_s: u64) -> DeliveryKeyRec {
        let (seed, km) = delivery::new_key(mailbox, self.now_s() + ttl_s, self.rng);
        let seed = zeroize::Zeroizing::new(seed);
        DeliveryKeyRec {
            public: pub_key(&km),
            seed: hex(&*seed).into(),
        }
    }

    /// The signed `SealedDeliveryKeyRecord` for one of our keys, signed by this device.
    fn signed_key_record(&mut self, k: &DeliveryKeyRec) -> Result<Vec<u8>, CoreError> {
        let km = key_material(&k.public)?;
        let rnd = arr32(self.rng);
        let dev = self.device()?;
        Ok(delivery::sign_key_record(
            &dev.signer,
            &dev.device_id,
            &km,
            rnd,
        ))
    }

    /// One-time prekey replenishment. A fetch pops one item of each kind at the provider, so the local private
    /// inventory (which only shrinks when a handshake consumes an item) tracks it closely; when it runs low a new
    /// batch is generated and queued for the key-material provider. The signed bundle and its epoch are unchanged.
    /// Signed-prekey rotation and bundle expiry renewal change the epoch and the provider's pool and are not built.
    fn maintain_prekeys(&mut self) {
        if !self.st.app.keys_published
            || self
                .st
                .app
                .outbox
                .iter()
                .any(|e| matches!(e.purpose, Purpose::PublishKeys))
        {
            return;
        }
        let Some(dev) = self.st.device.as_mut() else {
            return;
        };
        if dev.opks.len() >= REPLENISH_BELOW && dev.pqopks.len() >= REPLENISH_BELOW {
            return;
        }
        let (n_ec, n_pq) = (
            ONE_TIME_KEYS.saturating_sub(dev.opks.len()),
            ONE_TIME_KEYS.saturating_sub(dev.pqopks.len()),
        );
        let up = dev.generate_one_time(n_ec, n_pq, self.rng);
        let call = NetCall::PublishKeyMaterial {
            device_id: dev.device_id.to_vec(),
            bundle: up.bundle,
            opks: up.opks,
            pqopks: up.pqopks,
        };
        self.enqueue(call, Purpose::PublishKeys);
    }

    /// The account's signed AccountControlState chain and its head, re-verified from stored bytes.
    fn acs_head(&self) -> Option<AcsHead> {
        let chain: Vec<Vec<u8>> = self
            .st
            .app
            .acs_chain
            .iter()
            .map(|c| unhex(c).ok())
            .collect::<Option<_>>()?;
        let creds: Vec<Vec<u8>> = self
            .st
            .app
            .acs_credentials
            .iter()
            .map(|c| unhex(c).ok())
            .collect::<Option<_>>()?;
        let head = control::verify_chain(&chain, &creds).ok()?;
        Some((head, chain, creds))
    }

    /// Can we seal to this peer right now? There is never a clear-sender fallback (profile section 12).
    fn peer_key_for_send(&self, rel: &Relationship) -> Result<PeerDelivery, RejectReason> {
        let (Some(pd), Some(mb)) = (&rel.peer_delivery, &rel.peer_mailbox) else {
            return Err(RejectReason::Unavailable); // sealed_delivery_required / delivery_key_unknown_or_stale
        };
        let bound = hb(mb)
            .map(|m| hex(&delivery::mailbox_route_binding(&m)) == pd.public.route_binding)
            .unwrap_or(false);
        if pd.bootstrap || !bound || pd.public.expires_at <= self.now_s() {
            return Err(RejectReason::Unavailable);
        }
        Ok(pd.clone())
    }

    /// Wrap a complete inner envelope for `peer`'s mailbox in the unsigned `SealedDeliveryEnvelope`.
    fn seal_to(&mut self, pd: &PeerDelivery, mailbox: &[u8], inner: &[u8]) -> Result<Vec<u8>, ()> {
        let (kid, pk) = (
            h16(&pd.public.key_id).map_err(|_| ())?,
            hb(&pd.public.public_key).map_err(|_| ())?,
        );
        let expires = (self.now_s() + WRAPPER_TTL_S).min(pd.public.expires_at);
        delivery::seal(
            &pk,
            &kid,
            delivery::CORE_ROUTE_URI,
            mailbox,
            expires,
            inner,
            self.rng,
        )
        .map_err(|_| ())
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
            IntentBody::SuspendDevice { device_id, .. } => {
                self.device_transition(device_id, control::Action::Suspend)
            }
            IntentBody::RevokeDevice { device_id, .. } => {
                self.device_transition(device_id, control::Action::Revoke)
            }
            // Not built (status report "What remains"); Rejected(unsupported) is the conservative answer.
            IntentBody::ShareContact { .. }
            | IntentBody::ChangePrivacyPolicy { .. }
            | IntentBody::ShareStaticLocation { .. }
            | IntentBody::StartCall { .. }
            | IntentBody::RestoreHistory { .. }
            | IntentBody::OfferDeviceLink
            | IntentBody::RequestDeviceLink { .. }
            | IntentBody::ApproveDeviceLink { .. }
            | IntentBody::RejectDeviceLink { .. }
            | IntentBody::StartRecovery { .. }
            | IntentBody::VetoRecovery { .. } => unsupported,
        };
        let msg = None;
        self.record(op, kind, outcome, msg);
        Ok(())
    }

    /// `SuspendDevice` / `RevokeDevice`: a real AccountControlState transition, authorized against the parent head
    /// with this device as the only signer, signed, re-verified and appended to the chain in this transaction.
    fn device_transition(&mut self, device_id: &str, action: control::Action) -> OutcomeClass {
        use control::PlanError::*;
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let (Some((head, chain, creds)), Some(me)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return rejected(RejectReason::StaleState);
        };
        let Some(target) = unhex(device_id)
            .ok()
            .and_then(|v| <[u8; 16]>::try_from(v).ok())
        else {
            return rejected(RejectReason::Invalid);
        };
        let child = match control::plan(&head, &me, action, &target) {
            Ok(c) => c,
            Err(NotFound) => return rejected(RejectReason::NotFound),
            Err(NotController) => return rejected(RejectReason::Unauthorized),
            Err(Conflict | WouldFreeze) => return rejected(RejectReason::Conflict),
            // Authorized only with other controllers' signatures: Core has no signature-collection transport yet.
            Err(NeedsOtherSigners) => return rejected(RejectReason::Unsupported),
            Err(Forbidden) => return rejected(RejectReason::Unauthorized),
        };
        let Some(dev) = self.st.device.as_ref() else {
            return rejected(RejectReason::StaleState);
        };
        let signed = control::sign_acs(&child.body(), &[(&dev.device_id, &dev.signer)], self.rng);
        // Defense in depth: the signed child must verify as a successor of the head we hold.
        if control::verify_successor(&head, &signed, &creds).is_err() {
            return rejected(RejectReason::IntegrityFailure);
        }
        let _ = chain;
        self.st.app.acs_chain.push(hex(&signed));
        OutcomeClass::Committed
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
        // The genesis AccountControlState is signed by the genesis control device (role `controller`).
        let genesis = control::sign_acs(
            &device.acs().body(),
            &[(&device.device_id, &device.signer)],
            self.rng,
        );
        self.st.app.acs_chain = vec![hex(&genesis)];
        self.st.app.acs_credentials = vec![hex(&device.credential().body())];
        self.st.app.device_meta.insert(
            hex(&device.device_id),
            DeviceMeta {
                label: "This device".into(),
                enrolled_at: self.now_ms,
            },
        );
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
        let _ = device;
        let Some((acs, chain, creds)) = self.acs_head() else {
            return OutcomeClass::Rejected {
                reason: RejectReason::IntegrityFailure,
            };
        };
        let acs_body = acs.body();
        let (acs_hash, label) = (acs.hash(), self.st.app.label.clone());
        let invite_id = self.rand16();
        let mailbox = self.rand16();
        let mut ikm = arr32(self.rng);
        let (seed, pk) = hpke::derive_key_pair(&ikm);
        let seed = zeroize::Zeroizing::new(seed);
        zeroize::Zeroize::zeroize(&mut ikm);
        let rk_digest = request_key_digest(&pk);
        let expires_at = self.now_ms + INVITE_TTL_MS;
        let descriptor = cbor::encode(&V::M(vec![
            (1, V::B(acs_body)),
            (2, V::A(creds.into_iter().map(V::B).collect())),
            (3, V::B(pk)),
            (4, V::B(mailbox.to_vec())),
            (5, V::U(expires_at / 1000)),
            (6, V::T(label)),
            (7, V::A(chain.into_iter().map(V::B).collect())),
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
                request_seed: hex(&*seed).into(),
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
                my_delivery: vec![],
                // Until the invitee's signed record arrives, only the request-scoped bootstrap key is usable, and
                // only for the Hello (send_message refuses a bootstrap key).
                peer_delivery: Some(PeerDelivery {
                    public: intro.reply_key.clone(),
                    bootstrap: true,
                }),
                bootstrap: None,
                replay: Default::default(),
                canon_id: None,
                hlc_ms: 0,
                hlc_logical: 0,
                equivocated: Default::default(),
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
        rel.my_delivery = vec![self.new_delivery_key(&mb, DELIVERY_KEY_TTL_S)];
        // The inviter names the conversation: random, provider-independent, bound to this relationship by the Hello.
        rel.canon_id = Some(hex(&arr32(self.rng)));
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
        // Active: seal and send now. Establishing (we accepted; the peer's signed delivery key has not arrived yet):
        // queue the finished event and send it, sealed, the moment the key arrives. There is never a clear fallback.
        let (state, has_mailbox, blocked) = (rel.state, rel.peer_mailbox.is_some(), rel.blocked);
        let queue = match (state, has_mailbox, blocked) {
            (RelState::Active, true, false) => false,
            (RelState::Establishing, true, false) => true,
            _ => {
                self.record(op, kind, rejected(RejectReason::Unavailable), None);
                return Ok(());
            }
        };
        let (Some(canon), Some(account), Some(device_id)) = (
            rel.canon_id.as_deref().and_then(|c| h32(c).ok()),
            self.st.device.as_ref().map(|d| d.account_id),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            self.record(op, kind, rejected(RejectReason::Unavailable), None);
            return Ok(());
        };
        if !queue && self.peer_key_for_send(rel).is_err() {
            self.record(op, kind, rejected(RejectReason::Unavailable), None);
            return Ok(());
        }
        let clock = Hlc::next(
            Hlc {
                physical_ms: rel.hlc_ms,
                logical: rel.hlc_logical,
            },
            self.now_ms,
        );
        let mid = self.rand16();
        let message_id = format!("msg-{}", hex(&mid));
        // A canonical W0 ConversationEvent (message.create); the 1:1 session AEAD authenticates it. Its EventID,
        // clock and bytes are fixed here, so queueing never changes what is eventually sent.
        let payload = events::Header {
            event_id: mid,
            conversation_id: canon,
            author_account: account,
            author_device: device_id,
            clock,
            kind: events::K_CREATE,
            body: events::message_create_body(text),
        }
        .encode();
        if registry()
            .check("ConversationEvent", &payload, false)
            .is_err()
        {
            self.record(op, kind, rejected(RejectReason::Invalid), None);
            return Ok(());
        }
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
            envelope: None,
            digest: hex(&crypto::sha256(&[&payload])),
            clock_ms: clock.physical_ms,
            clock_logical: clock.logical,
            deleted: false,
            queued_event: Some(hex(&payload)),
        });
        rel.hlc_ms = clock.physical_ms;
        rel.hlc_logical = clock.logical;
        if !queue {
            // The ratchet advance and the outbox entry are one staged transaction: persisted before the first external effect.
            self.release_queued(conv)?;
        }
        self.record(op, kind, OutcomeClass::PendingNetwork, Some(message_id));
        Ok(())
    }

    /// Encrypt, seal and enqueue every queued outgoing event of `conv`, in order. Any failure aborts the whole staged
    /// transaction, so a ratchet advance is never persisted without its outbox entry.
    fn release_queued(&mut self, conv: &str) -> Result<(), CoreError> {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return Ok(());
        };
        let Ok(pd) = self.peer_key_for_send(&rel) else {
            return Ok(()); // still no usable key: stay queued
        };
        let (peer_ik, mailbox) = (
            h32(&rel.peer_ik)?,
            hb(rel
                .peer_mailbox
                .as_deref()
                .ok_or(CoreError::Invalid("peer mailbox"))?)?,
        );
        for m in rel.messages.iter().filter(|m| m.queued_event.is_some()) {
            let payload = hb(m.queued_event.as_deref().unwrap_or_default())?;
            let (env, _) = self
                .device()?
                .encrypt(&peer_ik, &payload)
                .map_err(|_| CoreError::Invalid("encrypt"))?;
            let wrapper = self
                .seal_to(&pd, &mailbox, &env)
                .map_err(|_| CoreError::Invalid("seal"))?;
            if let Some(slot) = self
                .st
                .app
                .relationships
                .get_mut(conv)
                .and_then(|r| r.messages.iter_mut().find(|x| x.id == m.id))
            {
                slot.envelope = Some(hex(&env));
                slot.queued_event = None;
            }
            self.enqueue(
                NetCall::Deposit {
                    mailbox: mailbox.clone(),
                    bytes: wrapper,
                },
                Purpose::DepositMessage {
                    operation_id: m.operation_id.clone().unwrap_or_default(),
                    conversation_id: conv.to_string(),
                    message_id: m.id.clone(),
                },
            );
        }
        Ok(())
    }

    /// TEST ONLY (`test-loopback`): encrypt and seal an arbitrary inner payload as this device so scenario tests can
    /// play a misbehaving authenticated peer (equivocation, wrong conversation, edits). Not reachable in production.
    #[cfg(feature = "test-loopback")]
    pub fn debug_inject(&mut self, conv: &str, payload: &[u8]) -> Result<(), CoreError> {
        let rel = self
            .st
            .app
            .relationships
            .get(conv)
            .cloned()
            .ok_or(CoreError::NotFound("conversation"))?;
        let pd = self
            .peer_key_for_send(&rel)
            .map_err(|_| CoreError::Invalid("no key"))?;
        let mailbox = hb(rel
            .peer_mailbox
            .as_deref()
            .ok_or(CoreError::Invalid("mb"))?)?;
        let (env, _) = self
            .device()?
            .encrypt(&h32(&rel.peer_ik)?, payload)
            .map_err(|_| CoreError::Invalid("encrypt"))?;
        let wrapper = self
            .seal_to(&pd, &mailbox, &env)
            .map_err(|_| CoreError::Invalid("seal"))?;
        self.enqueue(
            NetCall::Deposit {
                mailbox,
                bytes: wrapper,
            },
            Purpose::DepositKeys {
                conversation_id: conv.to_string(),
            },
        );
        Ok(())
    }

    /// TEST ONLY (`test-loopback`): enroll a second device as a MESSAGING device by a real, signed `link_messaging`
    /// transition (the device-link ceremony transport is not built, so tests need a way to get a multi-device ACS).
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_messaging_device(&mut self) -> Result<String, CoreError> {
        let (head, _, _) = self.acs_head().ok_or(CoreError::Invalid("no acs"))?;
        let (me_id, account) = {
            let d = self.device()?;
            (d.device_id, d.account_id)
        };
        let mut other = crate::crypto::store::Device::new(self.rng);
        other.account_id = account;
        let cred = other.credential();
        let mut child = head.clone();
        child.generation = head.generation + 1;
        child.previous = Some(head.hash());
        child.messaging.insert(other.device_id);
        child.credentials.push((other.device_id, cred.hash()));
        let signer = self.device()?.signer.clone();
        let signed = control::sign_acs(&child.body(), &[(&me_id, &signer)], self.rng);
        self.st.app.acs_credentials.push(hex(&cred.body()));
        self.st.app.acs_chain.push(hex(&signed));
        self.st.app.device_meta.insert(
            hex(&other.device_id),
            DeviceMeta {
                label: "Test phone".into(),
                enrolled_at: self.now_ms,
            },
        );
        Ok(hex(&other.device_id))
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
                // Resend the very same inner envelope: re-running the ratchet would create a second semantic send.
                // It is re-sealed under a fresh delivery_id; the recipient dedupes the inner envelope.
                let (peer_mailbox, env) = (
                    self.st
                        .app
                        .relationships
                        .get(&conv)
                        .and_then(|r| r.peer_mailbox.clone()),
                    hb(env)?,
                );
                let pd = self
                    .st
                    .app
                    .relationships
                    .get(&conv)
                    .and_then(|r| self.peer_key_for_send(r).ok());
                let (Some(peer_mailbox), Some(pd)) = (peer_mailbox, pd) else {
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
                let mailbox = hb(&peer_mailbox)?;
                let Ok(wrapper) = self.seal_to(&pd, &mailbox, &env) else {
                    return Err(CoreError::Invalid("seal"));
                };
                self.set_delivery(&conv, &msg.id, DeliveryState::PendingNetwork);
                self.enqueue(
                    NetCall::Deposit {
                        mailbox,
                        bytes: wrapper,
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
                Purpose::PublishKeys | Purpose::Ack | Purpose::DepositKeys { .. } => None,
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
            Purpose::DepositHello { operation_id, .. } => {
                // The relationship becomes Active only when the peer's signed delivery-key record arrives:
                // until then there is no key we may seal an ordinary message to.
                self.set_outcome(&operation_id, OutcomeClass::Committed);
            }
            Purpose::DepositKeys { .. } => {}
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
            let chain: Vec<Vec<u8>> = v
                .get(7)
                .and_then(|x| x.a())
                .map_err(|_| CoreError::Invalid("acs chain"))?
                .iter()
                .map(|c| c.b().map(<[u8]>::to_vec))
                .collect::<Result<_, _>>()
                .map_err(|_| CoreError::Invalid("acs chain"))?;
            Ok((acs_body, creds, pk, mailbox, expires, label, chain))
        })();
        let Ok((acs_body, creds, pk, mailbox, expires, label, chain)) = parsed else {
            return rej(IntegrityFailure);
        };
        // The head the link committed to must be the head of a verified, signed AccountControlState chain.
        if !control::verify_chain(&chain, &creds).is_ok_and(|h| h.body() == acs_body) {
            return rej(IntegrityFailure);
        }
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
        let Some((my_head, my_chain, my_creds)) = self.acs_head() else {
            return rej(IntegrityFailure);
        };
        let my_acs = my_head.body();
        let (Ok(invite16), Ok(rk)) = (h16(invite_id), h32(rk_digest)) else {
            return rej(IntegrityFailure);
        };
        let my_mailbox = self.rand16();
        let label_me = self.st.app.label.clone();
        // ADR-0144: a fresh request-scoped bootstrap key, bound to the reply route; never reused or promoted.
        let bootstrap_key = self.new_delivery_key(&my_mailbox, BOOTSTRAP_TTL_S);
        let Ok(bootstrap_km) = key_material(&bootstrap_key.public) else {
            return rej(IntegrityFailure);
        };
        let payload = cbor::encode(&V::M(vec![
            (1, V::B(my_acs)),
            (2, V::A(my_creds.into_iter().map(V::B).collect())),
            (3, V::B(my_mailbox.to_vec())),
            (4, V::T(label_me)),
            (5, V::U(self.now_ms)),
            (6, bootstrap_km.to_v()),
            (7, V::A(my_chain.into_iter().map(V::B).collect())),
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
        let my_key = self.new_delivery_key(&my_mailbox, DELIVERY_KEY_TTL_S);
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
                my_delivery: vec![my_key],
                peer_delivery: None,
                bootstrap: Some(BootstrapRec {
                    key: bootstrap_key,
                    candidates: vec![],
                }),
                replay: Default::default(),
                canon_id: None,
                hlc_ms: 0,
                hlc_logical: 0,
                equivocated: Default::default(),
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
        // The bootstrap key is request-scoped and valid for this one Hello only; its expiry is the request deadline.
        let Some(boot) = rel.peer_delivery.clone().filter(|p| p.bootstrap) else {
            return rej(IntegrityFailure);
        };
        if boot.public.expires_at <= self.now_s() {
            return rej(Expired);
        }
        let Some(my_key) = rel.my_delivery.first().cloned() else {
            return rej(IntegrityFailure);
        };
        let Some(canon_id) = rel.canon_id.as_deref().and_then(|c| h32(c).ok()) else {
            return rej(IntegrityFailure);
        };
        let Ok(record) = self.signed_key_record(&my_key) else {
            return rej(IntegrityFailure);
        };
        let hello = cbor::encode(&V::M(vec![
            (0, V::U(KIND_HELLO)),
            (1, V::B(my_mailbox.to_vec())),
            (2, V::T(label)),
            (3, V::B(record)),
            (4, V::B(canon_id.to_vec())),
        ]));
        let env = match self.st.device.as_mut().map(|d| d.encrypt(&peer_ik, &hello)) {
            Some(Ok((env, _))) => env,
            _ => return rej(IntegrityFailure),
        };
        let Ok(wrapper) = self.seal_to(&boot, &peer_mailbox, &env) else {
            return rej(IntegrityFailure);
        };
        self.enqueue(
            NetCall::Deposit {
                mailbox: peer_mailbox,
                bytes: wrapper,
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
        self.maintain_prekeys();
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
        let seed = zeroize::Zeroizing::new(seed);
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
            let reply_key = KeyMaterial::from_v(p.get(6)?)?;
            let chain: Vec<Vec<u8>> = p
                .get(7)?
                .a()?
                .iter()
                .map(|c| c.b().map(<[u8]>::to_vec))
                .collect::<Result<_, _>>()?;
            Ok((acs_body, creds, mb, label, reply_key, chain))
        })();
        let Ok((acs_body, creds, mb, label, reply_key, chain)) = parsed else {
            return;
        };
        // The requester's state must be a verified, signed AccountControlState (nothing is pinned until accepted).
        if !control::verify_chain(&chain, &creds).is_ok_and(|h| h.body() == acs_body) {
            return;
        }
        // The bootstrap key must be bound to exactly the reply route in the same request, and fresh.
        let now_s = self.now_s();
        if mb.len() != 16
            || reply_key.route_binding != delivery::mailbox_route_binding(&mb)
            || reply_key.expires_at <= now_s
            || reply_key.expires_at > now_s + BOOTSTRAP_TTL_S + BOOTSTRAP_CLOCK_SKEW_S
        {
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
                reply_key: pub_key(&reply_key),
            },
        );
    }

    fn count_failure(&mut self, conv: &str) {
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.integrity_failures = r.integrity_failures.saturating_add(1);
        }
    }

    /// Sealed-delivery acceptance (SEALED_SENDER_DELIVERY_PROFILE.md section 8, IMPLEMENTATION_ALGORITHMS section 3).
    /// Everything below runs in one staged transaction, so the replay record, the inner session acceptance and any
    /// application effect become durable together or not at all (a refused host commit releases nothing, and the
    /// exact wrapper simply arrives again). Outer HPKE success never authenticates the sender.
    fn on_session_envelope(&mut self, conv: &str, wrapper: &[u8]) {
        let now_s = self.now_s();
        // 1. bounded canonical parse. A clear-sender envelope, a stripped wrapper or junk is `sealed_delivery_required`.
        if wrapper.len() > 140_000
            || registry()
                .check("SealedDeliveryEnvelope", wrapper, false)
                .is_err()
        {
            return self.count_failure(conv);
        }
        let Ok(v) = cbor::decode(wrapper) else {
            return self.count_failure(conv);
        };
        let (Ok(Ok(key_id)), Ok(Ok(delivery_id)), Ok(wrapper_exp)) = (
            v.get(3).and_then(|x| x.b()).map(<[u8; 16]>::try_from),
            v.get(4).and_then(|x| x.b()).map(<[u8; 16]>::try_from),
            v.get(7).and_then(|x| x.u()),
        ) else {
            return self.count_failure(conv);
        };
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return;
        };
        let fp = hex(&crypto::sha256(&[wrapper]));
        let rkey = format!("{}:{}", hex(&key_id), hex(&delivery_id));
        // 2. durable replay state before live key material is needed: an exact COMPLETE/REJECTED retry is idempotent.
        if let Some(old) = rel.replay.get(&rkey) {
            if old.fingerprint != fp {
                return self.count_failure(conv); // same (key, id), different wrapper: collision, never a new branch
            }
            return;
        }
        // 3. relationship-local delivery key by opaque id (ordinary keys, or the request-scoped bootstrap key).
        let found = rel
            .my_delivery
            .iter()
            .find(|k| hb(&k.public.key_id).is_ok_and(|b| b == key_id))
            .map(|k| (k.clone(), false))
            .or_else(|| {
                rel.bootstrap
                    .as_ref()
                    .filter(|b| hb(&b.key.public.key_id).is_ok_and(|x| x == key_id))
                    .map(|b| (b.key.clone(), true))
            });
        let Some((key, is_bootstrap)) = found else {
            return self.count_failure(conv); // delivery_key_unknown_or_stale
        };
        let (Ok(seed), Ok(mailbox)) = (h32(&key.seed), hb(&rel.my_mailbox)) else {
            return;
        };
        let seed = zeroize::Zeroizing::new(seed);
        // 4. route binding, key expiry, wrapper expiry no later than the key.
        if hex(&delivery::mailbox_route_binding(&mailbox)) != key.public.route_binding
            || now_s > key.public.expires_at.saturating_add(delivery::CLOCK_SKEW)
            || wrapper_exp > key.public.expires_at
        {
            return self.count_failure(conv);
        }
        // 5. outer HPKE open + bucket validation.
        let Ok(opened) = delivery::open(
            wrapper,
            &seed,
            &key_id,
            delivery::CORE_ROUTE_URI,
            &mailbox,
            now_s,
        ) else {
            return self.count_failure(conv); // delivery_open_failed / delivery_expired
        };
        // 6. bootstrap candidate budget, checked before a replay record is allocated.
        if is_bootstrap
            && rel.bootstrap.as_ref().is_some_and(|b| {
                !b.candidates.contains(&fp) && b.candidates.len() >= MAX_BOOTSTRAP_CANDIDATES
            })
        {
            return self.count_failure(conv);
        }
        // 7. RESERVED (transaction-local), then the unchanged inner envelope through the normal inbound API.
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        if is_bootstrap && let Some(b) = rel.bootstrap.as_mut() {
            b.candidates.push(fp.clone());
        }
        rel.replay.insert(
            rkey.clone(),
            ReplayRec {
                fingerprint: fp,
                state: ReplayState::Reserved,
                expires_at: wrapper_exp,
            },
        );
        let accepted = self.process_inner(conv, &opened.inner_envelope, is_bootstrap);
        // 8. COMPLETE or REJECTED in the same transaction as the inner acceptance.
        if let Some(rel) = self.st.app.relationships.get_mut(conv) {
            if let Some(r) = rel.replay.get_mut(&rkey) {
                r.state = if accepted {
                    ReplayState::Complete
                } else {
                    ReplayState::Rejected
                };
            }
            if !accepted {
                rel.integrity_failures = rel.integrity_failures.saturating_add(1);
            }
            // Tombstones live only through the authenticated window plus skew.
            rel.replay
                .retain(|_, r| r.expires_at.saturating_add(delivery::CLOCK_SKEW) >= now_s);
        }
    }

    /// Process one complete inner envelope. Returns whether it was definitively accepted (a duplicate of an
    /// already-processed inner envelope is accepted idempotently with no effect).
    fn process_inner(&mut self, conv: &str, env: &[u8], via_bootstrap: bool) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let Ok(account) = h32(&rel.peer_account_id) else {
            return false;
        };
        let env_id = hex(&crypto::sha256(&[env]));
        if rel.seen_envelopes.contains(&env_id) {
            return true; // the same inner envelope under a fresh wrapper (sender retry): not an attack, no effect
        }
        let Some(dev) = self.st.device.as_mut() else {
            return false;
        };
        // A payload that is not accepted must leave the cryptographic state untouched on every path, bootstrap or
        // established (audit M6): decrypting advances the ratchet, so snapshot first and restore on rejection.
        let snapshot = Some(dev.clone());
        // ADR-0137: the relationship context is mandatory and is checked before any session lookup or secret operation.
        let res = dev.decrypt(&RelationshipContext::new(account), env, self.rng);
        let pt = match res {
            Ok((_, pt, _)) => pt,
            Err(_) => return false,
        };
        let Ok(p) = cbor::decode(&pt) else {
            self.restore(snapshot);
            return false;
        };
        let kind = p.get(0).and_then(|x| x.u()).unwrap_or(0);
        // The bootstrap key admits exactly one thing: the peer's Hello with a valid signed key record.
        if via_bootstrap && kind != KIND_HELLO {
            self.restore(snapshot);
            return false;
        }
        let ok = match kind {
            KIND_HELLO => self.on_hello(conv, &p, via_bootstrap),
            KIND_KEYS => self.on_keys(conv, &p),
            events::OBJECT_TYPE => self.on_event(conv, &pt),
            _ => true, // unknown payload kinds from a newer client are ignored, not fatal
        };
        if !ok {
            self.restore(snapshot);
            return false;
        }
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.seen_envelopes.push(env_id);
            if r.seen_envelopes.len() > 256 {
                r.seen_envelopes.remove(0);
            }
        }
        true
    }

    fn restore(&mut self, snapshot: Option<crate::crypto::store::Device>) {
        if let Some(d) = snapshot {
            self.st.device = Some(d);
        }
    }

    /// Verify a peer's signed `SealedDeliveryKeyRecord` against the relationship (profile section 4): signed by the
    /// pinned peer device, which must be active in the pinned ACS head, bound to the peer's own mailbox, unexpired.
    fn verify_peer_record(
        &self,
        rel: &Relationship,
        record: &[u8],
        peer_mailbox: &[u8],
    ) -> Option<KeyMaterial> {
        let cred = rel
            .peer_credentials
            .iter()
            .filter_map(|c| unhex(c).ok())
            .filter_map(|c| binding::Credential::from_body(&c).ok())
            .find(|c| hex(&c.device_id) == rel.peer_device_id)?;
        let acs = Acs::from_body(&hb(&rel.peer_acs_body).ok()?).ok()?;
        if !acs.device_active(&cred.device_id) {
            return None;
        }
        let km = delivery::open_key_record(record, &cred.device_id, &cred.signing_pk).ok()?;
        (km.route_binding == delivery::mailbox_route_binding(peer_mailbox)
            && km.expires_at > self.now_s()
            && km.expires_at <= self.now_s() + DELIVERY_KEY_TTL_S)
            .then_some(km)
    }

    /// Install an established peer key from an authenticated inner control message. A newer record replaces the
    /// old one (delivery-key rotation); an older or identical one changes nothing.
    fn install_peer_key(&mut self, conv: &str, km: &KeyMaterial) {
        let Some(r) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        let replace = match &r.peer_delivery {
            Some(pd) if !pd.bootstrap => pd.public.expires_at < km.expires_at,
            _ => true,
        };
        if replace {
            r.peer_delivery = Some(PeerDelivery {
                public: pub_key(km),
                bootstrap: false,
            });
        }
        if r.state != RelState::Active && r.peer_mailbox.is_some() {
            r.state = RelState::Active;
        }
        // Messages queued while the relationship was establishing now go out, sealed.
        if self.release_queued(conv).is_err() {
            // A seal/encrypt failure here must not strand the key install: the messages stay queued.
        }
    }

    fn on_hello(&mut self, conv: &str, p: &V, via_bootstrap: bool) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let (Ok(mb), Ok(record)) = (
            p.get(1).and_then(|x| x.b()).map(<[u8]>::to_vec),
            p.get(3).and_then(|x| x.b()).map(<[u8]>::to_vec),
        ) else {
            return false;
        };
        let Ok(canon) = p.get(4).and_then(|x| x.b32()) else {
            return false;
        };
        if mb.len() != 16 || rel.canon_id.as_deref().is_some_and(|c| c != hex(&canon)) {
            return false;
        }
        if rel.peer_mailbox.is_some() && rel.peer_mailbox.as_deref() != Some(hex(&mb).as_str()) {
            return false; // a second, different Hello is not another bootstrap branch
        }
        let Some(km) = self.verify_peer_record(&rel, &record, &mb) else {
            return false;
        };
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.peer_mailbox = Some(hex(&mb));
            r.canon_id = Some(hex(&canon));
        }
        self.install_peer_key(conv, &km);
        if via_bootstrap {
            // The winning bootstrap commit: the request-scoped key dies in the same transaction that installed
            // the established route and key (ADR-0145). Later candidates have no key to open with.
            if let Some(r) = self.st.app.relationships.get_mut(conv) {
                r.bootstrap = None;
            }
            self.send_keys(conv);
        }
        true
    }

    /// Reply to the peer's Hello with our own signed delivery-key record, sealed to the key just installed.
    fn send_keys(&mut self, conv: &str) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        let (Some(my_key), Ok(peer_ik), Ok(pd), Some(Ok(mailbox))) = (
            rel.my_delivery.first().cloned(),
            h32(&rel.peer_ik),
            self.peer_key_for_send(&rel).ok().ok_or(()),
            rel.peer_mailbox.as_deref().map(hb),
        ) else {
            return;
        };
        let Ok(record) = self.signed_key_record(&my_key) else {
            return;
        };
        let payload = cbor::encode(&V::M(vec![(0, V::U(KIND_KEYS)), (3, V::B(record))]));
        let Some(Ok((env, _))) = self
            .st
            .device
            .as_mut()
            .map(|d| d.encrypt(&peer_ik, &payload))
        else {
            return;
        };
        let Ok(wrapper) = self.seal_to(&pd, &mailbox, &env) else {
            return;
        };
        self.enqueue(
            NetCall::Deposit {
                mailbox,
                bytes: wrapper,
            },
            Purpose::DepositKeys {
                conversation_id: conv.to_string(),
            },
        );
    }

    fn on_keys(&mut self, conv: &str, p: &V) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let (Ok(record), Some(Ok(mb))) = (
            p.get(3).and_then(|x| x.b()).map(<[u8]>::to_vec),
            rel.peer_mailbox.as_deref().map(hb),
        ) else {
            return false;
        };
        let Some(km) = self.verify_peer_record(&rel, &record, &mb) else {
            return false;
        };
        self.install_peer_key(conv, &km);
        true
    }

    /// An authenticated inner payload that is a `ConversationEvent`: W0-valid, in this conversation, authored by the
    /// pinned peer endpoint, deduplicated by EventID, and applied as a state transition.
    fn on_event(&mut self, conv: &str, bytes: &[u8]) -> bool {
        if registry().check("ConversationEvent", bytes, false).is_err() {
            return false;
        }
        let Some(ev) = events::parse(bytes) else {
            return false;
        };
        let Some(r) = self.st.app.relationships.get_mut(conv) else {
            return false;
        };
        let (Some(canon), Ok(peer_account), Ok(peer_device)) = (
            r.canon_id.as_deref(),
            h32(&r.peer_account_id),
            h16(&r.peer_device_id),
        ) else {
            return false;
        };
        // The event must name this conversation and be authored by the authenticated endpoint that carried it.
        if hex(&ev.conversation_id) != canon
            || ev.author_account != peer_account
            || ev.author_device != peer_device
        {
            return false;
        }
        let eid = hex(&ev.event_id);
        if r.equivocated.contains(&eid) {
            return false;
        }
        if r.blocked {
            return true;
        }
        let target = |body: &V| -> Option<String> {
            body.get(0)
                .ok()?
                .b()
                .ok()
                .filter(|b| b.len() == 16)
                .map(|b| format!("msg-{}", hex(b)))
        };
        match ev.kind {
            events::K_CREATE => {
                let Some(text) = ev
                    .body
                    .get(0)
                    .ok()
                    .and_then(events::plain_text)
                    .filter(|t| t.chars().count() <= MAX_MESSAGE_CHARS)
                else {
                    return false;
                };
                let id = format!("msg-{eid}");
                if let Some(old) = r.messages.iter().position(|m| m.id == id) {
                    if r.messages[old].digest == hex(&ev.digest) {
                        return true; // logical duplicate (the EventID is the canonical dedupe key)
                    }
                    // One EventID, two payloads: reject every version.
                    r.messages.remove(old);
                    r.equivocated.insert(eid);
                    return false;
                }
                // The peer's clock is shown as given but never orders authority; arrival order is the timeline.
                r.messages.push(Msg {
                    id,
                    outgoing: false,
                    operation_id: None,
                    text,
                    sent_at: ev.clock.physical_ms,
                    delivery: DeliveryState::Delivered,
                    envelope: None,
                    digest: hex(&ev.digest),
                    clock_ms: ev.clock.physical_ms,
                    clock_logical: ev.clock.logical,
                    deleted: false,
                    queued_event: None,
                });
                true
            }
            events::K_EDIT | events::K_DELETE => {
                let Some(id) = target(&ev.body) else {
                    return false;
                };
                // Only the author may edit or delete, so the target must be one of the peer's own messages.
                let Some(m) = r.messages.iter_mut().find(|m| m.id == id && !m.outgoing) else {
                    return true; // dangling or not the author's: held-and-never-displayed is unbuilt, so no effect
                };
                if m.deleted {
                    return true;
                }
                let newer =
                    (ev.clock.physical_ms, ev.clock.logical) > (m.clock_ms, m.clock_logical);
                if ev.kind == events::K_DELETE {
                    m.deleted = true;
                    m.text.clear();
                } else if newer {
                    let Some(text) = ev
                        .body
                        .get(1)
                        .ok()
                        .and_then(events::plain_text)
                        .filter(|t| t.chars().count() <= MAX_MESSAGE_CHARS)
                    else {
                        return false;
                    };
                    m.text = text;
                    m.clock_ms = ev.clock.physical_ms;
                    m.clock_logical = ev.clock.logical;
                }
                true
            }
            // Reactions have no UI surface in Core's view types yet: validated and accepted, no state kept.
            events::K_REACTION => true,
            events::K_RECEIPT => {
                // `delivered` and `read` both settle an outgoing message at Delivered (the view has no read state).
                if let Ok(ids) = ev.body.get(1).and_then(|x| x.a()) {
                    for id in ids {
                        if let Ok(b) = id.b() {
                            let id = format!("msg-{}", hex(b));
                            if let Some(m) = r.messages.iter_mut().find(|m| {
                                m.id == id && m.outgoing && m.delivery == DeliveryState::Sent
                            }) {
                                m.delivery = DeliveryState::Delivered;
                            }
                        }
                    }
                }
                true
            }
            _ => false, // later-wave kinds: refused, not silently dropped
        }
    }
}

pub fn request_key_digest(pk: &[u8]) -> [u8; 32] {
    ds_hash(
        LBL_REQUEST_KEY,
        &cbor::encode(&V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(pk.to_vec()))])),
    )
}

pub fn pub_key(km: &KeyMaterial) -> PubKey {
    PubKey {
        key_id: hex(&km.key_id),
        public_key: hex(&km.public_key),
        route_binding: hex(&km.route_binding),
        expires_at: km.expires_at,
    }
}

pub fn key_material(p: &PubKey) -> Result<KeyMaterial, CoreError> {
    Ok(KeyMaterial {
        key_id: h16(&p.key_id)?,
        public_key: hb(&p.public_key)?,
        route_binding: h32(&p.route_binding)?,
        expires_at: p.expires_at,
    })
}
