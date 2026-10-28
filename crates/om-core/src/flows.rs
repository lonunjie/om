// SPDX-License-Identifier: MPL-2.0
//! Core transitions: every function here runs inside one staged transaction (`Tx`), so a failed host commit
//! discards all of it and nothing is released. The persisted outbox carries the external effects; a transition
//! never performs I/O. First contact follows INVITES_INTROGATE_AND_SPAM.md as far as pass 1 goes (see
//! docs/reports/2026-10-28-core-kernel-m1-status.md "Spec questions" for what is deliberately not built).

use crate::api::*;
use crate::control;
use crate::crypto::binding::{self, Acs, PeerView, RelationshipContext};
use crate::crypto::cbor::{self, V};
use crate::crypto::delivery::{self, KeyMaterial};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{digest, ds_hash};
use crate::crypto::{self, PROFILE_V1, hex, hpke, unhex};
use crate::events::{self, Hlc};
use crate::host::{CoreError, NetCall, NetError, NetResult};
use crate::intro;
use crate::route::{RelayEntry, RelaySet, RouteOffer};
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
/// First contact (ADR-0181): the inviter's Accept (bootstrap response) and the requester's Bind.
const KIND_ACCEPT: u64 = 4;
const KIND_BIND: u64 = 5;
pub const HOUR_MS: u64 = 60 * 60 * 1000;
const DAY_MS: u64 = 24 * HOUR_MS;
/// Requests admitted to expensive work per rolling hour and day (Core-local, persisted; ADR-0181 section 5).
pub const FIRST_CONTACT_PER_HOUR: usize = 20;
pub const FIRST_CONTACT_PER_DAY: usize = 100;
/// An ACS chain from a peer that is not yet bound is refused above this many entries. Local resource policy, not a
/// protocol rule (the protocol cap is 64 links); a client may raise it (ADR-0181 section 6).
pub const FIRST_CONTACT_MAX_CHAIN: usize = 16;
/// A destructive-intent confirmation nonce lives this long and is spent by its first use (ADR-0181 section 8).
pub const CONFIRMATION_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_CONFIRMATIONS: usize = 16;

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

/// The deposit call for `route`. A legacy (pass 2) destination carries no `relay_uri`: its capability is the bare
/// mailbox id and the adapter derives the deposit capability itself, exactly as before routes existed.
fn deposit_call(route: &RelayEntry, bytes: Vec<u8>) -> NetCall {
    NetCall::Deposit {
        mailbox: route.capability.clone(),
        relay_uri: (!route.is_legacy()).then(|| route.relay_uri.clone()),
        bytes,
    }
}

pub struct Tx<'a, R: RngCore + CryptoRng> {
    pub st: &'a mut ProfileState,
    pub rng: &'a mut R,
    pub now_ms: u64,
    pub changed: Vec<OperationId>,
    /// Profiles currently open (SwitchIdentityProfile target check).
    pub open_profiles: &'a [String],
    /// Single-use route offers from the host's provider adapter (see `crate::route`).
    pub offers: &'a mut Vec<RouteOffer>,
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

    // ------------------------------------------------------------------------------------------ first contact

    /// The invite's intro identity, derived from the profile's `intro_root` (created on first use; ADR-0181 section 1).
    fn intro_signer(
        &mut self,
        invite_id: &[u8; 16],
    ) -> Result<crate::crypto::sig::DeviceSigner, CoreError> {
        if self.st.app.intro_root.is_none() {
            let mut root = arr32(self.rng);
            self.st.app.intro_root = Some(hex(&root).into());
            zeroize::Zeroize::zeroize(&mut root);
        }
        let root = zeroize::Zeroizing::new(h32(self
            .st
            .app
            .intro_root
            .as_deref()
            .ok_or(CoreError::CorruptState("intro root"))?)?);
        Ok(intro::derive_signer(&root, invite_id))
    }

    /// The Core-local first-contact limiter (ADR-0181 section 5). Runs after the cheap checks and before any
    /// decapsulation or signature verification. The clock is the wall clock guarded by a non-decreasing high-water
    /// mark, so a clock set backwards cannot reopen a window; a mark more than a day ahead of the wall clock is
    /// treated as a clock correction and reset.
    fn admit_first_contact(&mut self) -> bool {
        let wall = self.now_ms;
        let l = &mut self.st.app.first_contact;
        if l.high_water_ms > wall + DAY_MS {
            l.high_water_ms = wall;
            l.admitted.retain(|t| *t <= wall);
        }
        let now = wall.max(l.high_water_ms);
        l.high_water_ms = now;
        l.admitted.retain(|t| *t + DAY_MS > now);
        let hour = l.admitted.iter().filter(|t| **t + HOUR_MS > now).count();
        if hour >= FIRST_CONTACT_PER_HOUR || l.admitted.len() >= FIRST_CONTACT_PER_DAY {
            // Silent drop: no reply and no per-request state, only one aggregated counter for the notice.
            if l.last_drop_ms + HOUR_MS <= now {
                l.dropped = 0;
            }
            l.dropped = l.dropped.saturating_add(1);
            l.last_drop_ms = now;
            return false;
        }
        l.admitted.push(now);
        true
    }

    /// Drop first contacts that ran past their deadline, requests whose bootstrap key has expired, and expired
    /// confirmation nonces. Called on every inbound batch and intent, so no timer is needed.
    fn expire_stale(&mut self) {
        let (now_ms, now_s) = (self.now_ms, self.now_s());
        self.st
            .app
            .relationships
            .retain(|_, r| match (&r.pending_bind, r.state) {
                (Some(b), RelState::AwaitingAcceptance | RelState::AcceptedUnbound) => {
                    b.deadline_ms > now_ms
                }
                _ => true,
            });
        self.st
            .app
            .intros
            .retain(|_, i| i.reply_key.expires_at > now_s);
        self.st.app.confirmations.retain(|c| c.expires_at > now_ms);
    }

    // ------------------------------------------------------------------------------------------ confirmations (M2)

    fn issue_confirmation(&mut self, op: &OperationId, kind: &str, target: &str) -> OutcomeClass {
        if !DESTRUCTIVE_KINDS.contains(&kind)
            || target.is_empty()
            || target.chars().count() > 256
            || target.chars().any(char::is_control)
        {
            return OutcomeClass::Rejected {
                reason: RejectReason::Invalid,
            };
        }
        let nonce = hex(&self.rand16());
        let now = self.now_ms;
        let c = &mut self.st.app.confirmations;
        c.retain(|x| x.expires_at > now);
        c.push(ConfirmationRec {
            operation_id: op.clone(),
            nonce,
            intent_kind: kind.into(),
            target: target.into(),
            expires_at: now + CONFIRMATION_TTL_MS,
        });
        while c.len() > MAX_CONFIRMATIONS {
            c.remove(0);
        }
        OutcomeClass::Committed
    }

    /// UI-11: spend the nonce `token` names. It is single use even when it does not match, and it must name exactly
    /// this intent kind and target in this profile and be unexpired.
    fn consume_confirmation(
        &mut self,
        kind: &str,
        target: Option<String>,
        token: &serde_json::Value,
    ) -> bool {
        let now = self.now_ms;
        self.st.app.confirmations.retain(|c| c.expires_at > now);
        let Some(nonce) = token.get("nonce").and_then(|n| n.as_str()) else {
            return false;
        };
        let Some(pos) = self
            .st
            .app
            .confirmations
            .iter()
            .position(|c| c.nonce == nonce)
        else {
            return false;
        };
        let rec = self.st.app.confirmations.remove(pos);
        rec.intent_kind == kind && target.as_deref() == Some(rec.target.as_str())
    }

    // ------------------------------------------------------------------------------------------ sealed delivery

    /// Takes one route the provider adapter allocated: the mailbox id to poll and the entry peers deposit to. `None`
    /// when the host has supplied none (no provider is configured, or the adapter could not allocate), in which case
    /// nothing that needs a mailbox starts: there is no stand-in fallback.
    fn take_route(&mut self) -> Option<([u8; 16], RelayEntry)> {
        let at = self.offers.iter().position(|o| o.is_valid())?;
        let offer = self.offers.remove(at);
        let mailbox = <[u8; 16]>::try_from(offer.mailbox.as_slice()).ok()?;
        let entry = RelayEntry::new(
            &offer.relay_uri,
            &offer.capability,
            self.now_s() + DELIVERY_KEY_TTL_S,
        )?;
        Some((mailbox, entry))
    }

    /// A fresh receive key bound to `route` (the private seed stays in this profile's encrypted state).
    fn new_delivery_key(&mut self, route: &RelayEntry, ttl_s: u64) -> DeliveryKeyRec {
        let (seed, km) = delivery::new_key(route.binding(), self.now_s() + ttl_s, self.rng);
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
        let (Some(pd), Some(route)) = (&rel.peer_delivery, rel.peer_relays.primary()) else {
            return Err(RejectReason::Unavailable); // sealed_delivery_required / delivery_key_unknown_or_stale
        };
        let bound = hex(&route.binding()) == pd.public.route_binding;
        if pd.bootstrap || !bound || pd.public.expires_at <= self.now_s() {
            return Err(RejectReason::Unavailable);
        }
        Ok(pd.clone())
    }

    /// Wrap a complete inner envelope for the peer's route in the unsigned `SealedDeliveryEnvelope`.
    fn seal_to(
        &mut self,
        pd: &PeerDelivery,
        route: &RelayEntry,
        inner: &[u8],
    ) -> Result<Vec<u8>, ()> {
        let (kid, pk) = (
            h16(&pd.public.key_id).map_err(|_| ())?,
            hb(&pd.public.public_key).map_err(|_| ())?,
        );
        let expires = (self.now_s() + WRAPPER_TTL_S).min(pd.public.expires_at);
        delivery::seal(
            &pk,
            &kid,
            &route.relay_uri,
            &route.capability,
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
        self.expire_stale();
        // UI-11 (M2): a destructive intent runs only with a confirmation nonce that Core issued for exactly this
        // intent kind and target in this profile, unspent and unexpired. Anything else is `unauthorized`, and the
        // nonce is spent either way.
        if let Some(token) = intent.body.confirmation()
            && !self.consume_confirmation(kind, intent.body.confirmation_target(), token)
        {
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
            IntentBody::IssueConfirmation {
                intent_kind,
                target,
            } => self.issue_confirmation(op, intent_kind, target),
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
        if self.st.device.is_none() || self.acs_head().is_none() {
            return OutcomeClass::Rejected {
                reason: RejectReason::StaleState,
            };
        }
        let label = self.st.app.label.clone();
        let invite_id = self.rand16();
        let mailbox = self.rand16();
        let mut ikm = arr32(self.rng);
        let (seed, pk) = hpke::derive_key_pair(&ikm);
        let seed = zeroize::Zeroizing::new(seed);
        zeroize::Zeroize::zeroize(&mut ikm);
        let rk_digest = request_key_digest(&pk);
        let expires_at = self.now_ms + INVITE_TTL_MS;
        // The per-invite intro identity signs the descriptor (which carries the request key) and the ticket; the
        // link commits to its key digest. Neither object carries any account material (ADR-0181 sections 1, 2, 4).
        let Ok(signer) = self.intro_signer(&invite_id) else {
            return OutcomeClass::Rejected {
                reason: RejectReason::IntegrityFailure,
            };
        };
        let intro_pk = signer.public();
        let intro_digest = intro::key_digest(&intro_pk);
        let nonce = self.rand16();
        let descriptor = intro::sign_intro(
            &signer,
            intro::LBL_DESCRIPTOR_SIG,
            intro::descriptor_body(
                &invite_id,
                &label,
                &intro::service_uri(&mailbox),
                expires_at / 1000,
                Some(&pk),
            ),
            self.rng,
        );
        let ticket = intro::sign_intro(
            &signer,
            intro::LBL_TICKET_SIG,
            intro::ticket_body(&invite_id, &intro_digest, expires_at / 1000, &nonce, &pk),
            self.rng,
        );
        // One opaque record for the introduction service: the signed descriptor and ticket, and the intro public key
        // they verify under (the signature entry carries only the key's digest).
        let record = cbor::encode(&V::M(vec![
            (1, V::B(descriptor)),
            (2, V::B(ticket)),
            (3, V::B(intro_pk.clone())),
        ]));
        let mut token = invite_id.to_vec();
        token.extend_from_slice(&intro_digest);
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
                intro_pk: hex(&intro_pk),
                revoked: false,
                used: false,
            },
        );
        self.enqueue(
            NetCall::PutInvite {
                invite_id: invite_id.to_vec(),
                descriptor: record,
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
        // invite_id (16) || SHA-256(intro public key) (32)
        let Some(raw) = unb64url(token).filter(|r| r.len() == 48) else {
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
                intro_digest: hex(&raw[16..]),
            },
        );
        OutcomeClass::PendingNetwork
    }

    /// The inviter accepts a pending request: the relationship is `ACCEPTED_UNBOUND` (no account is known), and one
    /// Accept goes to the requester's reply route, sealed to its request-scoped bootstrap key (ADR-0181 section 3).
    fn accept_introduction(&mut self, op: &OperationId, request_id: &str) -> OutcomeClass {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(intro) = self.st.app.intros.get(request_id).cloned() else {
            return rejected(RejectReason::NotFound);
        };
        let Some(inv) = self.st.app.invites.get(&intro.invite_operation_id).cloned() else {
            return rejected(RejectReason::NotFound);
        };
        if intro.reply_key.expires_at <= self.now_s() {
            return rejected(RejectReason::Expired);
        }
        let (Some((head, chain, creds)), Some(my_key_dev)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| (d.account_id, d.device_id)),
        ) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let (Ok(rk_digest), Ok(my_intro_pk)) = (h32(&inv.request_digest), hb(&inv.intro_pk)) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        // Our own mailbox for this relationship comes from the provider adapter (no stand-in fallback).
        let Some((mb, my_route)) = self.take_route() else {
            return rejected(RejectReason::Unavailable);
        };
        let peer_route = intro.peer_route.clone();
        let conv = self.rand_id("conv");
        let my_key = self.new_delivery_key(&my_route, DELIVERY_KEY_TTL_S);
        let canon = arr32(self.rng);
        let Ok(record) = self.signed_key_record(&my_key) else {
            return rejected(RejectReason::IntegrityFailure);
        };
        let rnd = arr32(self.rng);
        let bind_body = intro::binding_body(
            &my_key_dev.0,
            &my_intro_pk,
            &rk_digest,
            head.generation,
            &head.hash(),
            self.now_s(),
        );
        let Some(binding) = self
            .st
            .device
            .as_ref()
            .map(|d| intro::sign_binding(&d.signer, &d.device_id, bind_body, rnd))
        else {
            return rejected(RejectReason::StaleState);
        };
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_ACCEPT)),
            (1, my_route.to_v()),
            (2, V::B(record)),
            (3, V::B(canon.to_vec())),
            (4, V::B(binding)),
            (5, V::A(chain.into_iter().map(V::B).collect())),
            (6, V::A(creds.into_iter().map(V::B).collect())),
        ]));
        let boot = PeerDelivery {
            public: intro.reply_key.clone(),
            bootstrap: true,
        };
        let Ok(wrapper) = self.seal_to(&boot, &peer_route, &payload) else {
            return rejected(RejectReason::PolicyDenied);
        };
        self.st.app.relationships.insert(
            conv.clone(),
            Relationship {
                id: conv.clone(),
                person_id: intro.person_id.clone(),
                display_name: intro.display_name.clone(),
                state: RelState::AcceptedUnbound,
                // No account is known until the requester's binding verifies.
                peer_account_id: String::new(),
                peer_device_id: String::new(),
                peer_ik: String::new(),
                peer_acs_body: String::new(),
                peer_credentials: vec![],
                my_mailbox: hex(&mb),
                my_relays: RelaySet::single(my_route),
                peer_relays: RelaySet::single(peer_route.clone()),
                blocked: false,
                integrity_failures: 0,
                created_at: self.now_ms,
                messages: vec![],
                seen_envelopes: vec![],
                my_delivery: vec![my_key],
                peer_delivery: Some(boot),
                bootstrap: None,
                replay: Default::default(),
                // The inviter names the conversation: random, provider-independent, carried in the Accept.
                canon_id: Some(hex(&canon)),
                hlc_ms: 0,
                hlc_logical: 0,
                equivocated: Default::default(),
                pending_bind: Some(PendingBind {
                    my_intro_pk: inv.intro_pk.clone(),
                    peer_intro_pk: intro.peer_intro_pk.clone(),
                    request_digest: inv.request_digest.clone(),
                    deadline_ms: self.now_ms + BOOTSTRAP_TTL_S * 1000,
                }),
            },
        );
        self.st.app.intros.remove(request_id);
        self.enqueue(
            deposit_call(&peer_route, wrapper),
            Purpose::DepositAccept {
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
        // Active: seal and send now. Not yet bound or still establishing (ADR-0181): queue the finished event and send
        // it, sealed, the moment the relationship is ACTIVE. Nothing is encrypted or deposited before then, and there
        // is never a clear fallback. The conversation id is fixed once the Accept exists, so a queued event is final.
        let (state, has_mailbox, blocked) =
            (rel.state, rel.peer_relays.primary().is_some(), rel.blocked);
        let queue = match (state, has_mailbox, blocked) {
            (RelState::Active, true, false) => false,
            (RelState::Establishing | RelState::AcceptedUnbound, true, false) => true,
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
        let (peer_ik, route) = (
            h32(&rel.peer_ik)?,
            rel.peer_relays
                .primary()
                .cloned()
                .ok_or(CoreError::Invalid("peer route"))?,
        );
        for m in rel.messages.iter().filter(|m| m.queued_event.is_some()) {
            let payload = hb(m.queued_event.as_deref().unwrap_or_default())?;
            let (env, _) = self
                .device()?
                .encrypt(&peer_ik, &payload)
                .map_err(|_| CoreError::Invalid("encrypt"))?;
            let wrapper = self
                .seal_to(&pd, &route, &env)
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
                deposit_call(&route, wrapper),
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
        let route = rel
            .peer_relays
            .primary()
            .cloned()
            .ok_or(CoreError::Invalid("mb"))?;
        let (env, _) = self
            .device()?
            .encrypt(&h32(&rel.peer_ik)?, payload)
            .map_err(|_| CoreError::Invalid("encrypt"))?;
        let wrapper = self
            .seal_to(&pd, &route, &env)
            .map_err(|_| CoreError::Invalid("seal"))?;
        self.enqueue(
            deposit_call(&route, wrapper),
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
                let (peer_route, env) = (
                    self.st
                        .app
                        .relationships
                        .get(&conv)
                        .and_then(|r| r.peer_relays.primary().cloned()),
                    hb(env)?,
                );
                let pd = self
                    .st
                    .app
                    .relationships
                    .get(&conv)
                    .and_then(|r| self.peer_key_for_send(r).ok());
                let (Some(route), Some(pd)) = (peer_route, pd) else {
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
                let Ok(wrapper) = self.seal_to(&pd, &route, &env) else {
                    return Err(CoreError::Invalid("seal"));
                };
                self.set_delivery(&conv, &msg.id, DeliveryState::PendingNetwork);
                self.enqueue(
                    deposit_call(&route, wrapper),
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
                | Purpose::DepositAccept {
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
                Purpose::PublishKeys
                | Purpose::Ack
                | Purpose::DepositKeys { .. }
                | Purpose::DepositBind { .. } => None,
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
                    if matches!(
                        entry.purpose,
                        Purpose::DepositRequest { .. } | Purpose::DepositAccept { .. }
                    ) && let Some(c) = conv
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
            Purpose::DepositRequest { operation_id, .. }
            | Purpose::DepositAccept { operation_id, .. } => {
                self.set_outcome(&operation_id, OutcomeClass::Committed)
            }
            Purpose::DepositHello {
                operation_id,
                conversation_id,
            } => {
                // Both bindings verified and the session's first message is out: the inviter's side is now ACTIVE and
                // anything queued while it was establishing goes out, sealed.
                self.set_outcome(&operation_id, OutcomeClass::Committed);
                if let Some(r) = self.st.app.relationships.get_mut(&conversation_id)
                    && r.state == RelState::Establishing
                {
                    r.state = RelState::Active;
                    r.pending_bind = None;
                    self.release_queued(&conversation_id)?;
                }
            }
            Purpose::DepositKeys { .. } | Purpose::DepositBind { .. } => {}
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
                intro_digest,
            } => {
                let out = self.on_invite_resolved(
                    &operation_id,
                    &invite_id,
                    &intro_digest,
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

    /// The requester resolves the invite: the request key is trusted only from a ContactDescriptor signed by the
    /// intro identity the link committed to, with a ticket signed by the same key (ADR-0181 section 2). The request
    /// is then sealed; it carries the requester's intro key and label, never an account.
    fn on_invite_resolved(
        &mut self,
        op: &OperationId,
        invite_id: &str,
        intro_digest: &str,
        record: Vec<u8>,
    ) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        let (Ok(invite16), Ok(link_digest)) = (h16(invite_id), h32(intro_digest)) else {
            return rej(IntegrityFailure);
        };
        let now_s = self.now_s();
        let parsed = (|| -> Result<_, crypto::Error> {
            if record.len() > 60_000 {
                return Err(crypto::Error::Decode("introduction record too large"));
            }
            let v = cbor::decode(&record)?;
            // The link commits to the digest of the intro key: check that before verifying any signature.
            let pk = v.get(3)?.b()?.to_vec();
            if intro::key_digest(&pk) != link_digest {
                return Err(crypto::Error::Binding("intro key is not the linked one"));
            }
            let d = intro::open_intro(v.get(1)?.b()?, intro::LBL_DESCRIPTOR_SIG, &pk)?;
            let t = intro::open_intro(v.get(2)?.b()?, intro::LBL_TICKET_SIG, &pk)?;
            Ok((pk, d, t))
        })();
        let Ok((intro_pk, d, t)) = parsed else {
            return rej(IntegrityFailure);
        };
        let (Ok(desc), Ok(ticket)) = (intro::read_descriptor(&d.body), intro::read_ticket(&t.body))
        else {
            return rej(IntegrityFailure);
        };
        let Some(pk) = desc.request_key.clone().filter(|k| k.len() == hpke::NPK) else {
            return rej(IntegrityFailure);
        };
        if desc.descriptor_id != invite16
            || ticket.ticket_id != invite16
            || ticket.service_binding != link_digest
            || ticket.request_key.as_deref() != Some(pk.as_slice())
            || registry()
                .check_body("ContactDescriptor", &d.body_bytes)
                .is_err()
            || registry()
                .check_body("IntroductionTicket", &t.body_bytes)
                .is_err()
        {
            return rej(IntegrityFailure);
        }
        if desc.expires_at_s <= now_s || ticket.expires_at_s <= now_s {
            return rej(Expired);
        }
        // The inviter's request mailbox is part of the signed descriptor, so the introduction service cannot redirect it.
        let Some(mailbox) = intro::parse_service_uri(&desc.intro_service) else {
            return rej(IntegrityFailure);
        };
        let mailbox = mailbox.to_vec();
        let rk = request_key_digest(&pk);
        let label = sanitize_label(&desc.label);
        let Ok(my_signer) = self.intro_signer(&invite16) else {
            return rej(IntegrityFailure);
        };
        let my_intro_pk = my_signer.public();
        // Our reply mailbox comes from the provider adapter; with none (no Home configured) nothing starts.
        let Some((my_mailbox, my_route)) = self.take_route() else {
            return rej(Unavailable);
        };
        let label_me = self.st.app.label.clone();
        // ADR-0144: a fresh request-scoped bootstrap key, bound to the reply route; never reused or promoted.
        let bootstrap_key = self.new_delivery_key(&my_route, BOOTSTRAP_TTL_S);
        let Ok(bootstrap_km) = key_material(&bootstrap_key.public) else {
            return rej(IntegrityFailure);
        };
        let route_expires = bootstrap_km.expires_at;
        // The requester's own signed descriptor: its intro key, the unverified label and nothing else.
        let sender_descriptor = intro::sign_intro(
            &my_signer,
            intro::LBL_DESCRIPTOR_SIG,
            intro::descriptor_body(
                &self.rand16(),
                &label_me,
                intro::INTRO_SERVICE_URI,
                route_expires,
                None,
            ),
            self.rng,
        );
        let payload = intro::sign_intro(
            &my_signer,
            intro::LBL_PAYLOAD_SIG,
            intro::payload_body(
                &invite16,
                &rk,
                &my_intro_pk,
                &sender_descriptor,
                &intro::RouteEntry {
                    relay_uri: &my_route.relay_uri,
                    capability: &my_route.capability,
                    expires_at_s: route_expires,
                },
                bootstrap_km.to_v(),
            ),
            self.rng,
        );
        if payload.len() as u64 > ticket.payload_limit {
            return rej(Quota);
        }
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
        let my_key = self.new_delivery_key(&my_route, DELIVERY_KEY_TTL_S);
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
                // The inviter's account is learned from its binding after acceptance, never before.
                peer_account_id: String::new(),
                peer_device_id: String::new(),
                peer_ik: String::new(),
                peer_acs_body: String::new(),
                peer_credentials: vec![],
                my_mailbox: hex(&my_mailbox),
                my_relays: RelaySet::single(my_route),
                peer_relays: RelaySet::default(),
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
                pending_bind: Some(PendingBind {
                    my_intro_pk: hex(&my_intro_pk),
                    peer_intro_pk: hex(&intro_pk),
                    request_digest: hex(&rk),
                    deadline_ms: route_expires * 1000,
                }),
            },
        );
        self.enqueue(
            NetCall::Deposit {
                mailbox,
                relay_uri: None, // the introduction service route is still the pass-2 stand-in (D10)
                bytes: request,
            },
            Purpose::DepositRequest {
                operation_id: op.clone(),
                conversation_id: conv,
            },
        );
        OutcomeClass::PendingNetwork
    }

    /// Inviter, after the requester's Bind verified: start the PQXDH session to the requester's device and send the
    /// Hello over the established route (ADR-0181 section 3, step 4).
    fn on_peer_keys(&mut self, op: &OperationId, conv: &str, fetch: Vec<Vec<u8>>) -> OutcomeClass {
        use RejectReason::*;
        let rej = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return rej(NotFound);
        };
        if rel.state != RelState::Establishing {
            return rej(StaleState);
        }
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
        let Ok(peer_ik) = h32(&rel.peer_ik) else {
            return rej(IntegrityFailure);
        };
        let Some(peer_route) = rel.peer_relays.primary().cloned() else {
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
        // The Hello rides the requester's established key (installed from its verified Bind), not the bootstrap key.
        let Ok(pd) = self.peer_key_for_send(&rel) else {
            return rej(Unavailable);
        };
        let Some(canon_id) = rel.canon_id.as_deref().and_then(|c| h32(c).ok()) else {
            return rej(IntegrityFailure);
        };
        let hello = cbor::encode(&V::M(vec![
            (0, V::U(KIND_HELLO)),
            (4, V::B(canon_id.to_vec())),
        ]));
        let env = match self.st.device.as_mut().map(|d| d.encrypt(&peer_ik, &hello)) {
            Some(Ok((env, _))) => env,
            _ => return rej(IntegrityFailure),
        };
        let Ok(wrapper) = self.seal_to(&pd, &peer_route, &env) else {
            return rej(IntegrityFailure);
        };
        self.enqueue(
            deposit_call(&peer_route, wrapper),
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
        self.expire_stale();
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

    /// A message in an invite mailbox. Order (ADR-0181 section 5): everything that is cheap and stateless first,
    /// then the persisted limiter, and only then the HPKE decapsulation and the signature checks.
    fn on_message_request(&mut self, invite_op: &str, bytes: &[u8]) {
        let Some(inv) = self.st.app.invites.get(invite_op).cloned() else {
            return;
        };
        // 1. Single-use, unexpired, unrevoked invites only; 2. a bounded message.
        if inv.revoked
            || inv.used
            || inv.expires_at <= self.now_ms
            || bytes.len() > intro::MAX_REQUEST_BYTES
        {
            return;
        }
        // 3. The W0 MessageRequest shape.
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
        // The ticket's payload limit plus the AEAD tag bounds the ciphertext, before any other work.
        if ct.len() as u64 > intro::PAYLOAD_LIMIT_BYTES + intro::AEAD_TAG_BYTES {
            return;
        }
        // 4. The ticket id and the request-key digest are exactly this invite's.
        if hex(&ticket) != inv.invite_id || hex(&rk) != inv.request_digest {
            return;
        }
        // 5. The limiter. A refused request is dropped silently (and counted for the one aggregated notice).
        if !self.admit_first_contact() {
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
        let now_s = self.now_s();
        let parsed = (|| -> Result<_, crypto::Error> {
            // The payload names the requester's intro key and is signed by it; its descriptor by the same key.
            let signer = intro::peek_body(&pt)?.get(4)?.b()?.to_vec();
            let p = intro::open_intro(&pt, intro::LBL_PAYLOAD_SIG, &signer)?;
            let b = &p.body;
            if b.get(0)?.u()? != intro::TYPE_PAYLOAD
                || b.get(1)?.u()? != 1
                || b.get(2)?.b()? != ticket16
                || b.get(3)?.get(1)?.b32()? != rk
            {
                return Err(crypto::Error::Binding("payload does not match the request"));
            }
            let d = intro::open_intro(b.get(5)?.b()?, intro::LBL_DESCRIPTOR_SIG, &signer)?;
            let desc = intro::read_descriptor(&d.body)?;
            let route = RelayEntry::from_v(b.get(8)?)
                .ok_or(crypto::Error::Binding("reply route is not a relay entry"))?;
            let reply_key = KeyMaterial::from_v(b.get(9)?)?;
            Ok((signer, desc, route, reply_key))
        })();
        let Ok((peer_intro_pk, desc, route, reply_key)) = parsed else {
            return;
        };
        let route_exp = route.expires_at;
        // The requester's descriptor must still be live, and must not be our own intro identity.
        if desc.expires_at_s <= now_s || hex(&peer_intro_pk) == inv.intro_pk {
            return;
        }
        // The bootstrap key must be bound to exactly the reply route in the same request, fresh, and no later than
        // the route (a bounded request deadline: min(route, key)).
        if reply_key.route_binding != route.binding()
            || reply_key.expires_at <= now_s
            || reply_key.expires_at > now_s + BOOTSTRAP_TTL_S + BOOTSTRAP_CLOCK_SKEW_S
            || reply_key.expires_at > route_exp
        {
            return;
        }
        if let Some(i) = self.st.app.invites.get_mut(invite_op) {
            i.used = true;
        }
        let label = sanitize_label(&desc.label);
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
                peer_intro_pk: hex(&peer_intro_pk),
                peer_route: route,
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
        let (Ok(seed), Some(route)) = (h32(&key.seed), rel.my_relays.primary().cloned()) else {
            return;
        };
        let seed = zeroize::Zeroizing::new(seed);
        // 4. route binding, key expiry, wrapper expiry no later than the key.
        if hex(&route.binding()) != key.public.route_binding
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
            &route.relay_uri,
            &route.capability,
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

    /// Process one complete inner payload. Returns whether it was definitively accepted (a duplicate of an
    /// already-processed payload is accepted idempotently with no effect).
    ///
    /// Before the relationship is bound the payload is not a session envelope: the requester's bootstrap key admits
    /// exactly one thing, the inviter's Accept, and an unbound inviter admits exactly the requester's Bind
    /// (ADR-0181 section 3). Both are plain CBOR inside the sealed delivery and authenticate by their own signatures.
    fn process_inner(&mut self, conv: &str, env: &[u8], via_bootstrap: bool) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let env_id = hex(&crypto::sha256(&[env]));
        if rel.seen_envelopes.contains(&env_id) {
            return true; // the same inner envelope under a fresh wrapper (sender retry): not an attack, no effect
        }
        let unbound_inviter =
            rel.state == RelState::AcceptedUnbound && rel.peer_account_id.is_empty();
        if via_bootstrap || unbound_inviter {
            let kind = if via_bootstrap {
                KIND_ACCEPT
            } else {
                KIND_BIND
            };
            let ok = cbor::decode(env)
                .ok()
                .filter(|p| p.get(0).and_then(|x| x.u()).ok() == Some(kind))
                .is_some_and(|p| {
                    if via_bootstrap {
                        self.on_accept(conv, &p)
                    } else {
                        self.on_bind(conv, &p)
                    }
                });
            if ok && let Some(r) = self.st.app.relationships.get_mut(conv) {
                r.seen_envelopes.push(env_id);
            }
            return ok;
        }
        let Ok(account) = h32(&rel.peer_account_id) else {
            return false;
        };
        let Some(dev) = self.st.device.as_mut() else {
            return false;
        };
        // A payload that is not accepted must leave the cryptographic state untouched on every path
        // (audit M6): decrypting advances the ratchet, so snapshot first and restore on rejection.
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
        let live = matches!(rel.state, RelState::Establishing | RelState::Active);
        let ok = match kind {
            // The Hello is the first session message after both bindings verified; it activates the requester.
            KIND_HELLO => self.on_hello(conv, &p),
            // Nothing but the Hello is accepted while the relationship is not yet bound.
            _ if !live => false,
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

    /// The `(head, credentials)` of a peer's chain sent with its binding, refused above the first-contact cap before
    /// any signature is checked (ADR-0181 section 6), then verified to an ACS head.
    fn first_contact_head(
        chain: Option<Vec<Vec<u8>>>,
        creds: Option<Vec<Vec<u8>>>,
    ) -> Option<(Acs, Vec<Vec<u8>>)> {
        let (chain, creds) = (chain?, creds?);
        if chain.is_empty() || chain.len() > FIRST_CONTACT_MAX_CHAIN {
            return None;
        }
        let head = control::verify_chain(&chain, &creds).ok()?;
        Some((head, creds))
    }

    /// Requester: the inviter's Accept arrived on the request-scoped bootstrap key (the winning commit, ADR-0145).
    /// Verify the inviter's `IntroIdentityBinding` for the intro key the link committed to, pin its account, install
    /// its established delivery key, and answer with our own Bind.
    fn on_accept(&mut self, conv: &str, p: &V) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let (RelState::AwaitingAcceptance, Some(pb)) = (rel.state, rel.pending_bind.clone()) else {
            return false;
        };
        // The inviter's route: a relay entry, or (a peer from before routes existed) its bare 16-byte mailbox.
        let route = match p.get(1) {
            Ok(V::B(mb)) if mb.len() == 16 => Some(RelayEntry::legacy(mb)),
            Ok(v) => RelayEntry::from_v(v),
            Err(_) => None,
        };
        let (Some(route), Ok(record), Ok(canon), Ok(binding)) = (
            route,
            p.get(2).and_then(|x| x.b()).map(<[u8]>::to_vec),
            p.get(3).and_then(|x| x.b32()),
            p.get(4).and_then(|x| x.b()).map(<[u8]>::to_vec),
        ) else {
            return false;
        };
        let Some((head, creds)) =
            Self::first_contact_head(bytes_list(p.get(5)), bytes_list(p.get(6)))
        else {
            return false;
        };
        let (Ok(peer_intro), Ok(rk), Ok(my_intro)) = (
            hb(&pb.peer_intro_pk),
            h32(&pb.request_digest),
            hb(&pb.my_intro_pk),
        ) else {
            return false;
        };
        let Ok(vb) = intro::verify_binding(
            &binding,
            &head,
            &creds,
            &intro::BindingExpect {
                intro_identity: &peer_intro,
                request_key_digest: &rk,
            },
        ) else {
            return false;
        };
        if self
            .st
            .device
            .as_ref()
            .is_none_or(|d| d.account_id == head.account_id)
        {
            return false;
        }
        let mut upd = rel.clone();
        self.bind_peer(&mut upd, &head, &vb, &creds);
        upd.peer_relays = RelaySet::single(route.clone());
        let Some(km) = self.verify_peer_record(&upd, &record, &route) else {
            return false;
        };
        let pd = PeerDelivery {
            public: pub_key(&km),
            bootstrap: false,
        };
        // Our Bind: the same kind of binding for the intro identity we used, sealed to the inviter's established key.
        let Some((bind, bind_route)) = self.build_bind(&upd, &pd, &my_intro, &rk) else {
            return false;
        };
        let Some(dev) = self.st.device.as_mut() else {
            return false;
        };
        if dev.trust_peer(head.body(), creds.clone()).is_err() {
            return false;
        }
        upd.state = RelState::AcceptedUnbound;
        upd.peer_delivery = Some(pd);
        upd.canon_id = Some(hex(&canon));
        upd.bootstrap = None; // the request-scoped key dies in the commit that installed the established route
        self.st.app.relationships.insert(conv.to_string(), upd);
        self.enqueue(
            deposit_call(&bind_route, bind),
            Purpose::DepositBind {
                conversation_id: conv.to_string(),
            },
        );
        true
    }

    /// Inviter: the requester's Bind arrived. Verify its binding for the intro key it signed the request with, pin its
    /// account, install its established key, and start the session (the Hello follows once its device bundle is fetched).
    fn on_bind(&mut self, conv: &str, p: &V) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let (RelState::AcceptedUnbound, Some(pb), Some(route)) = (
            rel.state,
            rel.pending_bind.clone(),
            rel.peer_relays.primary().cloned(),
        ) else {
            return false;
        };
        let (Ok(record), Ok(binding)) = (
            p.get(1).and_then(|x| x.b()).map(<[u8]>::to_vec),
            p.get(2).and_then(|x| x.b()).map(<[u8]>::to_vec),
        ) else {
            return false;
        };
        let Some((head, creds)) =
            Self::first_contact_head(bytes_list(p.get(3)), bytes_list(p.get(4)))
        else {
            return false;
        };
        let (Ok(peer_intro), Ok(rk)) = (hb(&pb.peer_intro_pk), h32(&pb.request_digest)) else {
            return false;
        };
        let Ok(vb) = intro::verify_binding(
            &binding,
            &head,
            &creds,
            &intro::BindingExpect {
                intro_identity: &peer_intro,
                request_key_digest: &rk,
            },
        ) else {
            return false;
        };
        if self
            .st
            .device
            .as_ref()
            .is_none_or(|d| d.account_id == head.account_id)
        {
            return false;
        }
        let mut upd = rel.clone();
        self.bind_peer(&mut upd, &head, &vb, &creds);
        let Some(km) = self.verify_peer_record(&upd, &record, &route) else {
            return false;
        };
        let Some(dev) = self.st.device.as_mut() else {
            return false;
        };
        if dev.trust_peer(head.body(), creds.clone()).is_err() {
            return false;
        }
        let device_id = vb.device_id.to_vec();
        upd.state = RelState::Establishing;
        upd.peer_delivery = Some(PeerDelivery {
            public: pub_key(&km),
            bootstrap: false,
        });
        self.st.app.relationships.insert(conv.to_string(), upd);
        self.enqueue(
            NetCall::FetchKeyMaterial { device_id },
            Purpose::FetchPeerKeys {
                operation_id: String::new(),
                conversation_id: conv.to_string(),
            },
        );
        true
    }

    /// Fill a relationship's peer fields from a verified ACS head and the device that signed the binding.
    fn bind_peer(
        &self,
        rel: &mut Relationship,
        head: &Acs,
        vb: &intro::VerifiedBinding,
        creds: &[Vec<u8>],
    ) {
        rel.peer_account_id = hex(&head.account_id);
        rel.peer_device_id = hex(&vb.device_id);
        rel.peer_ik = hex(&vb.credential.ik);
        rel.peer_acs_body = hex(&head.body());
        rel.peer_credentials = creds.iter().map(|c| hex(c)).collect();
    }

    /// The requester's Bind: our signed key record, our `IntroIdentityBinding`, our chain and credentials, sealed to
    /// the inviter's established key. Returns the wrapper and the inviter's route.
    fn build_bind(
        &mut self,
        rel: &Relationship,
        to: &PeerDelivery,
        my_intro: &[u8],
        rk: &[u8; 32],
    ) -> Option<(Vec<u8>, RelayEntry)> {
        let (head, chain, creds) = self.acs_head()?;
        let my_key = rel.my_delivery.first().cloned()?;
        let record = self.signed_key_record(&my_key).ok()?;
        let rnd = arr32(self.rng);
        let body = intro::binding_body(
            &self.st.device.as_ref()?.account_id,
            my_intro,
            rk,
            head.generation,
            &head.hash(),
            self.now_s(),
        );
        let binding = self
            .st
            .device
            .as_ref()
            .map(|d| intro::sign_binding(&d.signer, &d.device_id, body, rnd))?;
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_BIND)),
            (1, V::B(record)),
            (2, V::B(binding)),
            (3, V::A(chain.into_iter().map(V::B).collect())),
            (4, V::A(creds.into_iter().map(V::B).collect())),
        ]));
        let route = rel.peer_relays.primary().cloned()?;
        let wrapper = self.seal_to(to, &route, &payload).ok()?;
        Some((wrapper, route))
    }

    fn restore(&mut self, snapshot: Option<crate::crypto::store::Device>) {
        if let Some(d) = snapshot {
            self.st.device = Some(d);
        }
    }

    /// Verify a peer's signed `SealedDeliveryKeyRecord` against the relationship (profile section 4): signed by the
    /// pinned peer device, which must be active in the pinned ACS head, bound to the peer's own route, unexpired.
    fn verify_peer_record(
        &self,
        rel: &Relationship,
        record: &[u8],
        peer_route: &RelayEntry,
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
        (km.route_binding == peer_route.binding()
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
        // Messages queued while the relationship was establishing now go out, sealed.
        if self.release_queued(conv).is_err() {
            // A seal/encrypt failure here must not strand the key install: the messages stay queued.
        }
    }

    /// Requester: the inviter's Hello authenticated in the PQXDH session and names the conversation from the Accept.
    /// Both bindings are verified, so the relationship is ACTIVE (ADR-0181 section 3, step 4).
    fn on_hello(&mut self, conv: &str, p: &V) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return false;
        };
        let Ok(canon) = p.get(4).and_then(|x| x.b32()) else {
            return false;
        };
        if rel.state != RelState::AcceptedUnbound
            || rel.canon_id.as_deref() != Some(hex(&canon).as_str())
        {
            return false;
        }
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.state = RelState::Active;
            r.pending_bind = None;
        }
        // Anything composed while unbound goes out now, sealed (a failure leaves it queued).
        let _ = self.release_queued(conv);
        true
    }

    fn on_keys(&mut self, conv: &str, p: &V) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return false;
        };
        let (Ok(record), Some(route)) = (
            p.get(3).and_then(|x| x.b()).map(<[u8]>::to_vec),
            rel.peer_relays.primary().cloned(),
        ) else {
            return false;
        };
        let Some(km) = self.verify_peer_record(&rel, &record, &route) else {
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

fn bytes_list(v: Result<&V, crypto::Error>) -> Option<Vec<Vec<u8>>> {
    v.ok()?
        .a()
        .ok()?
        .iter()
        .map(|x| x.b().ok().map(<[u8]>::to_vec))
        .collect()
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
