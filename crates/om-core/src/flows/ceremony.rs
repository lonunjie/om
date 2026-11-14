//! The device-link ceremony (deviation D24, `docs/reports/2026-10-28-relay-status.md`).
//!
//! Who does what: the EXISTING device (an active control device) offers a link; the NEW device opens it on an empty
//! profile, asks to join and shows a code; the existing device shows the same code and a person approves. Approval is
//! the single `link_messaging` transition of the AccountControlState, signed by the approving device and re-verified
//! as a successor of the head it came from, together with a `DeviceEnrollmentNotice` (ceremony `link_ceremony`) that
//! the new device REQUIRES in the approval and every other device of the account receives.
//!
//! Transport: two short-lived queues on the existing device's own Home, driven only through ordinary Home calls
//! (`Deposit`, drain/ack with an explicit capability, and `OpenQueue`, which is queue creation). The link carries the
//! Home, the request queue's deposit capability, the response queue's receive capability and a one-time ceremony key.
//! Every queue message is sealed under keys derived from that key and the two capabilities, so the Home sees only
//! sizes and times. Core stays sans-IO: all of it is `NetCall`s.
//!
//! The construction of the transcript MAC is not specified (the label `OM-DEVICE-LINK-CEREMONY-MAC-v1` is reserved
//! with no construction): the derivation below is provisional and recorded as a gap in D24.

use super::*;
use crate::crypto::binding::Credential;
use crate::crypto::kdf::{hkdf_sha256, hmac_sha256};
use crate::crypto::sig::{self, ROLE_CONTROLLER, ds_sig, signature_entry};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};

pub(super) const LINK_PREFIX: &str = "om:link/";
/// Reserved in `om-domain-separation.json` (status `reserved`, construction undefined): used provisionally.
const LBL_CEREMONY: &str = "OM-DEVICE-LINK-CEREMONY-MAC-v1";
const LBL_NOTICE_SIG: &str = "OM-DEVICE-ENROLLMENT-NOTICE-SIG-v1";
/// Sent from the approving device to every other device of the account: `{0: 11, 1: signed notice, 2: [states], 3: [creds]}`.
pub(super) const KIND_ENROLLED: u64 = 11;
/// A ceremony lives this long from the offer, whatever happens (the link carries it; Core enforces it).
pub const CEREMONY_TTL_MS: u64 = 5 * 60 * 1000;
/// One queue message (plaintext) stays well under the Home's deposit limit (133,000 bytes).
const MAX_MSG: usize = 100_000;
const MAX_ROSTER: usize = 64;
const MAX_NOTICES: usize = 32;
const MAX_CHAIN_TAIL: usize = 4;
const MAX_LINK_CHARS: usize = 256;
const DEVICE_CLASS_MESSAGING: u64 = 1;
const CEREMONY_LINK: u64 = 1;
const ROLE_REQ: u8 = 1;
const ROLE_RESP: u8 = 2;
/// New device -> existing: the request / the introductions of the joined conversations.
const M_REQUEST: u64 = 1;
const M_INTRO: u64 = 4;
/// Existing -> new: approval, refusal, one conversation handed over.
const M_APPROVE: u64 = 2;
const M_REJECT: u64 = 3;
const M_ROSTER: u64 = 5;
/// Existing -> new: one chunk of the `DeviceHistorySnapshot` (multi-device phase 5).
const M_HISTORY: u64 = 6;
/// The 32-byte random `codec_id` of this implementation's `DeviceHistorySnapshot` codec family, chosen once and never
/// reassigned to incompatible semantics (ADR-0156/0157, MULTI_DEVICE_SYNC "ADR-0156/0157 implementation namespacing").
const HISTORY_CODEC_ID: &str = "c3cb99a3a24d06c6178626672c4557d746938b453380526711411f7e6fb3794e";
const HISTORY_CODEC_MAJOR: u64 = 1;
/// Encoded body of one chunk stays under this, so a chunk fits the 64 KiB profile-1 bucket with room for framing.
const HISTORY_CHUNK_BODY: usize = 50_000;
/// A snapshot is bounded (the newest messages win): the ceremony window and the Home's queue are small.
pub const HISTORY_MAX_CHUNKS: usize = 32;

type Keys = ([u8; 32], [u8; 32], [u8; 32]);

/// `(request key, response key, code key)`, from the one-time key and the two capabilities in the link.
fn ceremony_keys(secret: &[u8], a_cap: &[u8], b_recv: &[u8]) -> Keys {
    let transcript = ds_hash(
        LBL_CEREMONY,
        &cbor::encode(&V::A(vec![V::B(a_cap.to_vec()), V::B(b_recv.to_vec())])),
    );
    let derive = |purpose: &[u8]| {
        let mut info = LBL_CEREMONY.as_bytes().to_vec();
        info.push(0);
        info.extend_from_slice(purpose);
        info.extend_from_slice(&transcript);
        let mut out = [0u8; 32];
        hkdf_sha256(LBL_CEREMONY.as_bytes(), secret, &info, &mut out);
        out
    };
    (derive(b"req"), derive(b"resp"), derive(b"code"))
}

fn aad(role: u8) -> Vec<u8> {
    let mut a = LBL_CEREMONY.as_bytes().to_vec();
    a.push(role);
    a
}

/// Profile-1 `SealedDeliveryEnvelope` ciphertext lengths (the Home's deposit check, D9): the buckets plus the AEAD tag.
const BUCKETS: [usize; 5] = [1040, 4112, 16400, 65552, 131088];
/// HPKE `enc` length of profile 1.
const ENC_LEN: usize = 1120;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// A ceremony message as the Home accepts a deposit: a structurally valid type-23, profile-1 envelope. The Home never
/// opens it; the other device does, with the ceremony key. `enc` and the ids are random filler; the ciphertext field is
/// the XChaCha20-Poly1305 nonce followed by the AEAD output over `len(4) || message || zeros`, sized so the whole field
/// is exactly one profile-1 bucket. The real tag covers the padding, so any change is caught on open.
fn wrap_envelope(
    nonce: &[u8; 24],
    aead_out: Vec<u8>,
    filler: [u8; 32],
    enc: Vec<u8>,
    expires_s: u64,
) -> Vec<u8> {
    let mut ct = nonce.to_vec();
    ct.extend_from_slice(&aead_out);
    cbor::encode(&V::M(vec![
        (0, V::U(23)),
        (1, V::U(1)),
        (2, V::U(1)),
        (3, V::B(filler[..16].to_vec())),
        (4, V::B(filler[16..].to_vec())),
        (5, V::B(enc)),
        (6, V::B(ct)),
        (7, V::U(expires_s)),
    ]))
}

fn open_msg(key: &[u8; 32], role: u8, bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > 140_000 {
        return None;
    }
    let v = cbor::decode(bytes).ok()?;
    if v.get(0).ok()?.u().ok()? != 23
        || v.get(1).ok()?.u().ok()? != 1
        || v.get(2).ok()?.u().ok()? != 1
        || v.get(5).ok()?.b().ok()?.len() != ENC_LEN
    {
        return None;
    }
    let ct = v.get(6).ok()?.b().ok()?;
    if !BUCKETS.contains(&ct.len()) {
        return None;
    }
    let nonce: [u8; 24] = ct[..NONCE_LEN].try_into().ok()?;
    let frame = XChaCha20Poly1305::new(key.into())
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &ct[NONCE_LEN..],
                aad: &aad(role),
            },
        )
        .ok()?;
    let n = u32::from_be_bytes(frame.get(..4)?.try_into().ok()?) as usize;
    let body = frame.get(4..4 + n)?;
    // Padding is zeros and nothing else: a longer or different tail is not a ceremony message.
    if frame[4 + n..].iter().any(|b| *b != 0) {
        return None;
    }
    Some(body.to_vec())
}

/// Six digits both devices derive from the request as the new device sent it.
fn verification_code(code_key: &[u8; 32], request: &[u8]) -> String {
    let m = hmac_sha256(code_key, request);
    let n = u32::from_be_bytes([m[0], m[1], m[2], m[3]]) % 1_000_000;
    format!("{n:06}")
}

fn request_id_of(device_id: &[u8]) -> String {
    hex(device_id)
}

/// The link payload, before base64url: version, expiry (POSIX s, u32), key, request deposit cap, response receive cap, Home.
fn encode_link(
    expiry_s: u32,
    secret: &[u8],
    a_cap: &[u8],
    b_recv: &[u8],
    uri: &str,
) -> Option<String> {
    let mut v = vec![1u8];
    v.extend_from_slice(&expiry_s.to_be_bytes());
    v.extend_from_slice(secret);
    v.push(u8::try_from(a_cap.len()).ok()?);
    v.extend_from_slice(a_cap);
    v.push(u8::try_from(b_recv.len()).ok()?);
    v.extend_from_slice(b_recv);
    v.extend_from_slice(uri.as_bytes());
    Some(format!("{LINK_PREFIX}{}", b64url(&v)))
}

struct LinkParts {
    expiry_s: u32,
    secret: Vec<u8>,
    a_cap: Vec<u8>,
    b_recv: Vec<u8>,
    uri: String,
}

fn decode_link(link: &str) -> Option<LinkParts> {
    if link.len() > MAX_LINK_CHARS {
        return None;
    }
    let token = link.strip_prefix(LINK_PREFIX)?;
    if !(16..=200).contains(&token.len())
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let raw = unb64url(token)?;
    let (&version, rest) = raw.split_first()?;
    if version != 1 || rest.len() < 4 + 16 + 1 {
        return None;
    }
    let expiry_s = u32::from_be_bytes(rest[..4].try_into().ok()?);
    let secret = rest[4..20].to_vec();
    let (a_len, rest) = rest[20..].split_first()?;
    let a_len = usize::from(*a_len);
    if rest.len() < a_len + 1 {
        return None;
    }
    let a_cap = rest[..a_len].to_vec();
    let (b_len, rest) = rest[a_len..].split_first()?;
    let b_len = usize::from(*b_len);
    if rest.len() <= b_len {
        return None;
    }
    let b_recv = rest[..b_len].to_vec();
    let uri = String::from_utf8(rest[b_len..].to_vec()).ok()?;
    // The same shape rules as a relay entry: capability lengths, an allowed URI.
    let entry = RelayEntry::new(&uri, &a_cap, 0)?;
    if entry.is_legacy() || !(16..=64).contains(&b_recv.len()) {
        return None;
    }
    Some(LinkParts {
        expiry_s,
        secret,
        a_cap,
        b_recv,
        uri,
    })
}

/// The id Core polls the new device's response queue under (the capability is what the Home checks).
fn recv_mailbox(b_recv: &[u8]) -> Vec<u8> {
    ds_hash(LBL_CEREMONY, b_recv)[..16].to_vec()
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    fn seal_msg(&mut self, key: &[u8; 32], role: u8, pt: &[u8], expires_s: u64) -> Option<Vec<u8>> {
        if pt.len() > MAX_MSG {
            return None;
        }
        let bucket = BUCKETS
            .iter()
            .copied()
            .find(|b| b - NONCE_LEN - TAG_LEN >= 4 + pt.len())?;
        let mut frame = vec![0u8; bucket - NONCE_LEN - TAG_LEN];
        frame[..4].copy_from_slice(&(pt.len() as u32).to_be_bytes());
        frame[4..4 + pt.len()].copy_from_slice(pt);
        let mut nonce = [0u8; 24];
        self.rng.fill_bytes(&mut nonce);
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &frame,
                    aad: &aad(role),
                },
            )
            .ok()?;
        let mut filler = [0u8; 32];
        self.rng.fill_bytes(&mut filler);
        let mut enc = vec![0u8; ENC_LEN];
        self.rng.fill_bytes(&mut enc);
        Some(wrap_envelope(&nonce, ct, filler, enc, expires_s))
    }

    fn deposit_ceremony(&mut self, op: &str, cap: &[u8], uri: &str, bytes: Vec<u8>) {
        self.enqueue(
            NetCall::Deposit {
                mailbox: cap.to_vec(),
                relay_uri: Some(uri.to_string()),
                bytes,
            },
            Purpose::DepositCeremony {
                operation_id: op.to_string(),
            },
        );
    }

    // ------------------------------------------------------------------------------------------ existing device: offer

    /// `OfferDeviceLink`: only an active control device offers (a messaging device never links another). Two queues
    /// at one of our Homes are allocated; the link exists once the response queue's receive capability is known.
    pub(super) fn offer_device_link(&mut self, op: &OperationId) -> OutcomeClass {
        use RejectReason::*;
        let rej = |r| OutcomeClass::Rejected { reason: r };
        if self
            .st
            .app
            .link_join
            .as_ref()
            .is_some_and(|j| j.state == LinkState::AwaitingApproval)
        {
            return rej(Conflict);
        }
        let (Some((head, ..)), Some(me)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return rej(StaleState);
        };
        if !head.control.contains(&me) || head.quarantined.contains(&me) {
            return rej(Unauthorized);
        }
        // Both queues at the same Home: the most preferred one.
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
        // A new offer replaces an earlier one that nobody approved.
        for o in self.st.app.link_offers.values_mut() {
            if matches!(
                o.state,
                LinkState::Opening | LinkState::WaitingForScan | LinkState::AwaitingApproval
            ) {
                o.state = LinkState::Expired;
            }
        }
        let secret = self.rand16();
        self.st.app.link_offers.insert(
            op.clone(),
            LinkOffer {
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
                request: None,
                introduced: vec![],
                history_sent: 0,
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

    /// The response queue is open: its receive capability completes the link.
    pub(super) fn on_open_queue(&mut self, op: &str, cap: Option<Vec<u8>>) {
        let now_s = self.now_s();
        let Some(o) = self.st.app.link_offers.get_mut(op) else {
            return;
        };
        let secret = unhex(&o.secret).unwrap_or_default();
        let a_cap = o.a_route.capability.clone();
        let link = cap
            .as_deref()
            .filter(|c| (16..=64).contains(&c.len()))
            .filter(|_| o.state == LinkState::Opening && o.expires_at / 1000 > now_s)
            .and_then(|c| {
                encode_link(
                    u32::try_from(o.expires_at / 1000).ok()?,
                    &secret,
                    &a_cap,
                    c,
                    &o.b_route.relay_uri,
                )
                .filter(|l| l.len() <= MAX_LINK_CHARS && l.len() - LINK_PREFIX.len() <= 200)
            });
        match (link, cap) {
            (Some(link), Some(cap)) => {
                o.b_recv = hex(&cap);
                o.link = Some(link);
                o.state = LinkState::WaitingForScan;
                self.set_outcome(&op.to_string(), OutcomeClass::Committed);
            }
            _ => {
                // The Home gave nothing usable, or the link would not fit the link grammar: no ceremony.
                o.state = LinkState::Expired;
                self.set_outcome(
                    &op.to_string(),
                    OutcomeClass::Rejected {
                        reason: RejectReason::Unavailable,
                    },
                );
            }
        }
    }

    /// A ceremony deposit or queue creation was refused for good: the ceremony ends, nothing is half adopted.
    pub(super) fn ceremony_failed(&mut self, op: &str) {
        if let Some(o) = self.st.app.link_offers.get_mut(op)
            && matches!(o.state, LinkState::Opening | LinkState::WaitingForScan)
        {
            o.state = LinkState::Expired;
        }
        if let Some(j) = self.st.app.link_join.as_mut()
            && j.operation_id == op
            && j.state == LinkState::AwaitingApproval
        {
            j.state = LinkState::Failed;
            j.secret = String::new().into();
        }
    }

    /// Run with `expire_stale`: a ceremony past its deadline is over (the secrets are wiped).
    pub(super) fn expire_ceremonies(&mut self) {
        let now = self.now_ms;
        for o in self.st.app.link_offers.values_mut() {
            if o.expires_at <= now {
                if matches!(
                    o.state,
                    LinkState::Opening | LinkState::WaitingForScan | LinkState::AwaitingApproval
                ) {
                    o.state = LinkState::Expired;
                }
                o.secret = String::new().into();
            }
        }
        if let Some(j) = self.st.app.link_join.as_mut()
            && j.expires_at <= now
        {
            if j.state == LinkState::AwaitingApproval {
                j.state = if j.approved {
                    LinkState::Failed
                } else {
                    LinkState::Expired
                };
            }
            mark_history_degraded(j);
            j.secret = String::new().into();
        }
    }

    // ------------------------------------------------------------------------------------------ new device: request

    /// `RequestDeviceLink`: on a profile with no device and no account. The device keys are made here; nothing is
    /// published and no account state exists until an approval verifies.
    pub(super) fn request_device_link(&mut self, op: &OperationId, link: &str) -> OutcomeClass {
        use RejectReason::*;
        let rej = |r| OutcomeClass::Rejected { reason: r };
        if self.st.device.is_some()
            || !self.st.app.acs_chain.is_empty()
            || self.st.app.link_join.is_some()
        {
            return rej(Conflict);
        }
        let now_s = self.now_s();
        let Some(parts) = decode_link(link) else {
            return rej(Invalid);
        };
        let expiry_s = u64::from(parts.expiry_s);
        if expiry_s <= now_s {
            return rej(Expired);
        }
        if expiry_s > now_s + CEREMONY_TTL_MS / 1000 + 60 {
            return rej(Invalid); // the offering device never makes a longer ceremony
        }
        let device = crate::crypto::store::Device::new(self.rng);
        let nonce = self.rand16();
        let label = "New device".to_string();
        let request = cbor::encode(&V::M(vec![
            (0, V::U(M_REQUEST)),
            (1, V::U(1)),
            (2, V::B(device.device_id.to_vec())),
            (3, V::B(device.signer.public())),
            (4, V::B(device.ik().to_vec())),
            (5, V::B(nonce.to_vec())),
            (6, V::T(label.clone())),
        ]));
        let (k_req, _, k_code) = ceremony_keys(&parts.secret, &parts.a_cap, &parts.b_recv);
        let Some(sealed) = self.seal_msg(&k_req, ROLE_REQ, &request, expiry_s) else {
            return rej(Invalid);
        };
        let request_id = request_id_of(&device.device_id);
        let code = verification_code(&k_code, &request);
        self.st.app.link_join = Some(LinkJoin {
            operation_id: op.clone(),
            secret: hex(&parts.secret).into(),
            relay_uri: parts.uri.clone(),
            a_cap: hex(&parts.a_cap),
            b_mailbox: hex(&recv_mailbox(&parts.b_recv)),
            b_recv: hex(&parts.b_recv),
            created_ms: self.now_ms,
            expires_at: expiry_s * 1000,
            label,
            nonce: hex(&nonce),
            request_id,
            code,
            state: LinkState::AwaitingApproval,
            approved: false,
            roster_expected: 0,
            roster_done: 0,
            approver: String::new(),
            history_expected: 0,
            history_done: 0,
            history_ignored: 0,
            history_messages: 0,
            history_seen: vec![],
            history_degraded: false,
        });
        self.st.device = Some(device);
        self.deposit_ceremony(op, &parts.a_cap, &parts.uri, sealed);
        OutcomeClass::PendingNetwork
    }

    // ------------------------------------------------------------------------------------------ dispatch

    /// A ceremony queue item. Returns `false` when the mailbox is not a ceremony queue of this profile.
    pub(super) fn on_ceremony_mailbox(&mut self, mailbox: &str, bytes: &[u8]) -> bool {
        let offer = self
            .st
            .app
            .link_offers
            .iter()
            .find(|(_, o)| o.a_mailbox == mailbox)
            .map(|(k, _)| k.clone());
        if let Some(op) = offer {
            self.on_link_inbound(&op, bytes);
            return true;
        }
        if self
            .st
            .app
            .link_join
            .as_ref()
            .is_some_and(|j| j.b_mailbox == mailbox)
        {
            self.on_join_inbound(bytes);
            return true;
        }
        false
    }

    // ------------------------------------------------------------------------------------------ existing device: inbound

    fn on_link_inbound(&mut self, op: &str, bytes: &[u8]) {
        let Some(offer) = self.st.app.link_offers.get(op).cloned() else {
            return;
        };
        if offer.expires_at <= self.now_ms {
            return;
        }
        let secret = unhex(&offer.secret).unwrap_or_default();
        let (k_req, _, k_code) = ceremony_keys(
            &secret,
            &offer.a_route.capability,
            &unhex(&offer.b_recv).unwrap_or_default(),
        );
        let Some(pt) = open_msg(&k_req, ROLE_REQ, bytes) else {
            return; // not sealed under this ceremony's key: dropped
        };
        let Ok(v) = cbor::decode(&pt) else { return };
        match v.get(0).and_then(|x| x.u()) {
            Ok(M_REQUEST) if offer.state == LinkState::WaitingForScan => {
                self.on_link_request(op, &v, &pt, &k_code);
            }
            Ok(M_INTRO) if offer.state == LinkState::Linked => self.on_link_intro(op, &v),
            _ => {}
        }
    }

    /// The first well-formed request of the ceremony. Later requests are ignored: one offer, one device.
    fn on_link_request(&mut self, op: &str, v: &V, pt: &[u8], k_code: &[u8; 32]) {
        let (Ok(1), Ok(id), Ok(pk), Ok(ik), Ok(nonce), Ok(label)) = (
            v.get(1).and_then(|x| x.u()),
            v.get(2).and_then(|x| x.b()),
            v.get(3).and_then(|x| x.b()),
            v.get(4).and_then(|x| x.b()),
            v.get(5).and_then(|x| x.b()),
            v.get(6).and_then(|x| x.t()),
        ) else {
            return;
        };
        if id.len() != 16
            || pk.len() != crate::crypto::sig::PK_LEN
            || ik.len() != 32
            || nonce.len() != 16
        {
            return;
        }
        let label = sanitize_label(label);
        let label = if label.is_empty() {
            "New device".to_string()
        } else {
            label
        };
        let rec = LinkRequestRec {
            request_id: request_id_of(id),
            device_id: hex(id),
            signing_pk: hex(pk),
            ik: hex(ik),
            label,
            nonce: hex(nonce),
            code: verification_code(k_code, pt),
            received_ms: self.now_ms,
        };
        if let Some(o) = self.st.app.link_offers.get_mut(op) {
            o.request = Some(rec);
            o.state = LinkState::AwaitingApproval;
        }
    }

    /// The new device introduces itself into a conversation it was handed (its routes and key records, signed by it).
    fn on_link_intro(&mut self, op: &str, v: &V) {
        let (Some(offer), Ok(conv), Ok(intro_bytes)) = (
            self.st.app.link_offers.get(op).cloned(),
            v.get(1).and_then(|x| x.t()).map(String::from),
            v.get(2).and_then(|x| x.b()).map(<[u8]>::to_vec),
        ) else {
            return;
        };
        let (Some(req), Ok(intro)) = (offer.request.as_ref(), cbor::decode(&intro_bytes)) else {
            return;
        };
        // Only the device that was approved introduces itself, and only into a conversation we are in.
        if intro.get(0).and_then(|x| x.b()).map(hex).ok().as_deref() != Some(req.device_id.as_str())
            || offer.introduced.contains(&conv)
            || self
                .st
                .app
                .relationships
                .get(&conv)
                .is_none_or(|r| r.state != RelState::Active || r.blocked)
        {
            return;
        }
        self.sync_own_head();
        if !self.adopt_intro(&conv, &intro, true) {
            return;
        }
        if let Some(o) = self.st.app.link_offers.get_mut(op) {
            o.introduced.push(conv.clone());
        }
        self.flush_waiting(&conv);
        self.start_missing_sessions(&conv);
        // The contact learns the new device's routes and where to fetch its keys, authenticated by our head.
        self.send_account_update(&conv, None, &[intro_bytes]);
    }

    // ------------------------------------------------------------------------------------------ existing device: decide

    /// `RejectDeviceLink`: tell the new device no, and end the ceremony.
    pub(super) fn reject_device_link(&mut self, request_id: &str) -> OutcomeClass {
        use RejectReason::*;
        let rej = |r| OutcomeClass::Rejected { reason: r };
        let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
            return rej(StaleState);
        };
        let Some((head, ..)) = self.acs_head() else {
            return rej(StaleState);
        };
        if !head.control.contains(&me) || head.quarantined.contains(&me) {
            return rej(Unauthorized);
        }
        let Some(offer) = self
            .st
            .app
            .link_offers
            .values()
            .find(|o| {
                o.state == LinkState::AwaitingApproval
                    && o.request
                        .as_ref()
                        .is_some_and(|r| r.request_id == request_id)
            })
            .cloned()
        else {
            return rej(NotFound);
        };
        let (Some(req), secret) = (
            offer.request.clone(),
            unhex(&offer.secret).unwrap_or_default(),
        ) else {
            return rej(NotFound);
        };
        let (_, k_resp, _) = ceremony_keys(
            &secret,
            &offer.a_route.capability,
            &unhex(&offer.b_recv).unwrap_or_default(),
        );
        let msg = cbor::encode(&V::M(vec![
            (0, V::U(M_REJECT)),
            (1, V::B(unhex(&req.nonce).unwrap_or_default())),
        ]));
        if let Some(sealed) = self.seal_msg(&k_resp, ROLE_RESP, &msg, offer.expires_at / 1000) {
            self.deposit_ceremony(
                &offer.operation_id,
                &offer.b_route.capability,
                &offer.b_route.relay_uri,
                sealed,
            );
        }
        if let Some(o) = self.st.app.link_offers.get_mut(&offer.operation_id) {
            o.state = LinkState::Rejected;
            o.secret = String::new().into();
        }
        OutcomeClass::Committed
    }

    /// `ApproveDeviceLink` (the confirmation nonce was spent by the caller). All or nothing: on any failure the
    /// profile is exactly as it was before.
    pub(super) fn approve_device_link(&mut self, request_id: &str) -> OutcomeClass {
        let snapshot = self.st.clone();
        match self.approve_inner(request_id) {
            Ok(()) => OutcomeClass::Committed,
            Err(reason) => {
                *self.st = snapshot;
                OutcomeClass::Rejected { reason }
            }
        }
    }

    fn approve_inner(&mut self, request_id: &str) -> Result<(), RejectReason> {
        use RejectReason::*;
        let (Some((head, chain, creds)), Some(me)) = (
            self.acs_head(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return Err(StaleState);
        };
        // Only an active control device approves (a messaging device never links another).
        if !head.control.contains(&me) || head.quarantined.contains(&me) {
            return Err(Unauthorized);
        }
        let now = self.now_ms;
        let Some(offer) = self
            .st
            .app
            .link_offers
            .values()
            .find(|o| {
                o.request
                    .as_ref()
                    .is_some_and(|r| r.request_id == request_id)
                    && matches!(o.state, LinkState::AwaitingApproval | LinkState::Expired)
            })
            .cloned()
        else {
            return Err(NotFound);
        };
        if offer.state != LinkState::AwaitingApproval || offer.expires_at <= now {
            return Err(Expired);
        }
        let req = offer.request.clone().ok_or(NotFound)?;
        let secret = unhex(&offer.secret).map_err(|_| Expired)?;
        let b_recv = unhex(&offer.b_recv).map_err(|_| Invalid)?;
        let (_, k_resp, _) = ceremony_keys(&secret, &offer.a_route.capability, &b_recv);

        // The new device's credential, rebuilt from what it asked with; the account is ours.
        let cred = Credential {
            device_id: h16(&req.device_id).map_err(|_| Invalid)?,
            account_id: head.account_id,
            signing_pk: hb(&req.signing_pk).map_err(|_| Invalid)?,
            ik: h32(&req.ik).map_err(|_| Invalid)?,
        };
        let known = head
            .control
            .iter()
            .chain(&head.messaging)
            .chain(&head.quarantined)
            .chain(&head.revoked)
            .chain(head.credentials.iter().map(|(d, _)| d))
            .any(|d| *d == cred.device_id);
        if known || head.messaging.len() >= 256 || head.credentials.len() >= 320 {
            return Err(Conflict); // a DeviceID is never reused (terminal revocation); limits are the protocol's
        }
        let mut child = head.clone();
        child.generation = head.generation + 1;
        child.previous = Some(head.hash());
        child.messaging.insert(cred.device_id);
        child.credentials.push((cred.device_id, cred.hash()));
        let mut creds_all = creds.clone();
        creds_all.push(cred.body());
        let signed = {
            let dev = self.st.device.as_ref().ok_or(StaleState)?;
            control::sign_acs(&child.body(), &[(&dev.device_id, &dev.signer)], self.rng)
        };
        // The approval is exactly one `link_messaging` transition authorized by the head it descends from.
        let child =
            control::verify_successor(&head, &signed, &creds_all).map_err(|_| Unauthorized)?;

        // The non-suppressible notice: signed over the new head, for the new device and for every other device.
        let notice = self.sign_notice(&child, &cred.device_id, me)?;
        let notice_hex = hex(&notice);
        // A controller-signed head of the new tip, for the account's messaging devices to cache (groups, D36).
        let head_sig = {
            let dev = self.st.device.as_ref().ok_or(StaleState)?;
            self.st
                .app
                .carry_sign_head_for_peers(&dev.signer, &dev.device_id, &child, self.rng)
        };
        let mut chain_all = chain.clone();
        chain_all.push(signed.clone());
        let tail_from = chain_all.len().saturating_sub(MAX_CHAIN_TAIL);
        // A sibling may have missed earlier enrollments: the payload carries the notice of every enrollment the tail
        // covers, in chain order (key 5), and the newest enrollment's notice in the usual slot (key 1).
        let prior: Vec<V> = chain_all[tail_from..chain_all.len() - 1]
            .iter()
            .filter_map(|s| chain_head(s))
            .filter_map(|st| {
                self.st
                    .app
                    .notices
                    .iter()
                    .filter_map(|n| unhex(&n.signed).ok())
                    .find(|n| notice_state(n) == Some(st.hash()))
            })
            .map(V::B)
            .collect();
        let mut enrolled_map = vec![
            (0, V::U(KIND_ENROLLED)),
            (1, V::B(notice.clone())),
            (
                2,
                V::A(chain_all[tail_from..].iter().cloned().map(V::B).collect()),
            ),
            (3, V::A(creds_all.iter().cloned().map(V::B).collect())),
        ];
        if !prior.is_empty() {
            enrolled_map.push((5, V::A(prior)));
        }
        let enrolled = cbor::encode(&V::M(enrolled_map));
        let enrolled = with_head(enrolled, 4, head_sig.as_deref());
        if enrolled.len() > MAX_MSG {
            return Err(Quota);
        }
        // Every other active device gets the notice, or nothing is approved.
        let others: Vec<String> = head
            .control
            .iter()
            .chain(&head.messaging)
            .filter(|d| **d != me && !head.quarantined.contains(*d))
            .map(|d| hex(d))
            .collect();
        for dev in &others {
            if !self.deliver_enrolled(dev, &enrolled) {
                return Err(Unavailable);
            }
        }

        // The approval message and the roster (conversations handed over). Built before anything is committed, so a
        // payload that does not fit refuses the approval instead of linking a device that cannot finish.
        let roster = self.roster_entries();
        let approval_notice = self.approval_notice(notice);
        let approval = cbor::encode(&V::M(vec![
            (0, V::U(M_APPROVE)),
            (1, V::B(head.account_id.to_vec())),
            (2, V::A(chain_all.iter().cloned().map(V::B).collect())),
            (3, V::A(creds_all.iter().cloned().map(V::B).collect())),
            (4, V::B(approval_notice)),
            (5, V::B(me.to_vec())),
            (6, V::B(unhex(&req.nonce).map_err(|_| Invalid)?)),
            (7, V::U(roster.len() as u64)),
            (8, V::T(self.st.app.label.clone())),
        ]));
        // Same-owner history for the new device: the approval is the explicit account authorization, and only a
        // device the new head lists as active is eligible (a revoked or quarantined one never gets a snapshot).
        let history = if history_eligible(&child, &cred.device_id) {
            self.history_chunks(&k_resp, offer.expires_at / 1000)
        } else {
            vec![]
        };
        let approval = {
            let mut v = cbor::decode(&approval).map_err(|_| Invalid)?;
            if let V::M(m) = &mut v {
                m.push((9, V::U(history.len() as u64)));
                if let Some(h) = &head_sig {
                    m.push((10, V::B(h.clone())));
                }
            }
            cbor::encode(&v)
        };
        let mut sealed = vec![
            self.seal_msg(&k_resp, ROLE_RESP, &approval, offer.expires_at / 1000)
                .ok_or(Quota)?,
        ];
        for entry in &roster {
            let msg = cbor::encode(&V::M(vec![(0, V::U(M_ROSTER)), (1, V::B(entry.clone()))]));
            sealed.push(
                self.seal_msg(&k_resp, ROLE_RESP, &msg, offer.expires_at / 1000)
                    .ok_or(Quota)?,
            );
        }

        sealed.extend(history.iter().cloned());
        self.st.app.acs_credentials.push(hex(&cred.body()));
        self.st.app.acs_chain.push(hex(&signed));
        self.st.app.device_meta.insert(
            req.device_id.clone(),
            DeviceMeta {
                label: req.label.clone(),
                enrolled_at: now,
            },
        );
        // The approving device tells its person too: a link is never silent anywhere.
        self.st.app.notices.push(NoticeRec {
            device_id: req.device_id.clone(),
            label: req.label.clone(),
            at: now,
            signed: notice_hex,
        });
        while self.st.app.notices.len() > MAX_NOTICES {
            self.st.app.notices.remove(0);
        }
        for s in sealed {
            self.deposit_ceremony(
                &offer.operation_id,
                &offer.b_route.capability,
                &offer.b_route.relay_uri,
                s,
            );
        }
        if let Some(o) = self.st.app.link_offers.get_mut(&offer.operation_id) {
            o.state = LinkState::Linked;
            o.history_sent = history.len() as u32;
        }
        // Contacts learn the new head (K7 on their side); introductions follow when the new device sends them.
        self.broadcast_account_update();
        Ok(())
    }

    /// TEST ONLY (`test-loopback`): leave the notice out of the approval, to show the new device refuses it.
    fn approval_notice(&self, notice: Vec<u8>) -> Vec<u8> {
        #[cfg(feature = "test-loopback")]
        if crate::runtime::TEST_OMIT_NOTICE.load(std::sync::atomic::Ordering::SeqCst) {
            return Vec::new();
        }
        notice
    }

    /// A `DeviceEnrollmentNotice` for `device` at `child`, signed by the approving device. The notice's signer role is
    /// not specified; this device signs as `controller`, as it does for the AccountControlState (recorded in D24).
    fn sign_notice(
        &mut self,
        child: &Acs,
        device: &[u8; 16],
        me: [u8; 16],
    ) -> Result<Vec<u8>, RejectReason> {
        let body = cbor::encode(&V::M(vec![
            (0, V::U(8)),
            (1, V::U(1)),
            (2, V::B(child.account_id.to_vec())),
            (3, V::B(device.to_vec())),
            (4, V::U(DEVICE_CLASS_MESSAGING)),
            (5, V::U(child.generation)),
            (6, digest(&child.hash())),
            (7, V::B(me.to_vec())),
            (8, V::U(CEREMONY_LINK)),
            (9, V::U(self.now_s())),
        ]));
        registry()
            .check_body("DeviceEnrollmentNotice", &body)
            .map_err(|_| RejectReason::IntegrityFailure)?;
        let rnd = arr32(self.rng);
        let dev = self.st.device.as_ref().ok_or(RejectReason::StaleState)?;
        let sg = dev.signer.sign_with(
            &ds_sig(LBL_NOTICE_SIG, PROFILE_V1, ROLE_CONTROLLER, &me, &body),
            rnd,
        );
        Ok(sig::envelope(
            body,
            vec![signature_entry(&me, PROFILE_V1, ROLE_CONTROLLER, sg)],
        ))
    }

    /// Put `payload` into the session with our other device `dev`, through any active conversation that reaches it.
    fn deliver_enrolled(&mut self, dev: &str, payload: &[u8]) -> bool {
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        for conv in convs {
            if !self.ensure_dev(&conv, dev, true) {
                continue;
            }
            let purpose = Purpose::DepositAcs {
                conversation_id: conv.clone(),
            };
            if self
                .with_dev(&conv, dev, |tx| tx.send_control(&conv, payload, purpose))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// The conversations handed to the new device: each one's relationship record without anything private to this
    /// device (our keys, routes, session replay state, history), and our current signed routes as an introduction.
    fn roster_entries(&mut self) -> Vec<Vec<u8>> {
        let mut convs: Vec<Relationship> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked && r.canon_id.is_some())
            .cloned()
            .collect();
        convs.sort_by(|a, b| a.id.cmp(&b.id));
        let me = self.st.device.as_ref().map(|d| d.device_id);
        let mut out = vec![];
        for rel in convs.into_iter().take(MAX_ROSTER) {
            let (Some(me), Some((signed, records))) = (me, self.current_set(&rel)) else {
                continue;
            };
            let intro = cbor::encode(&V::M(vec![
                (0, V::B(me.to_vec())),
                (1, V::B(signed)),
                (2, V::A(records.into_iter().map(V::B).collect())),
            ]));
            let mut stripped = rel.clone();
            stripped.messages.clear();
            stripped.seen_envelopes.clear();
            stripped.replay.clear();
            stripped.held.clear();
            stripped.my_boxes.clear();
            stripped.my_delivery.clear();
            stripped.my_relays = RelaySet::default();
            stripped.my_lineage = Lineage::default();
            stripped.my_mailbox = String::new();
            stripped.bootstrap = None;
            stripped.pending_bind = None;
            stripped.route_health.clear();
            stripped.head_asked_ms = 0;
            stripped.head_dirty = false;
            stripped.head_sent_ms = 0;
            stripped.hlc_ms = 0;
            stripped.hlc_logical = 0;
            stripped.equivocated.clear();
            stripped.multi.waiting = false;
            stripped.multi.in_view = false;
            for d in stripped
                .multi
                .extra
                .values_mut()
                .chain(stripped.multi.siblings.values_mut())
            {
                d.fetching = false;
                d.route_health.clear();
            }
            let Ok(json) = serde_json::to_vec(&stripped) else {
                continue;
            };
            let entry = cbor::encode(&V::M(vec![
                (1, V::T(rel.id.clone())),
                (2, V::B(json)),
                (3, V::B(intro)),
            ]));
            if entry.len() + 64 <= MAX_MSG {
                out.push(entry);
            }
        }
        out
    }

    /// Our current signed RelaySet (the effective node of our lineage, re-signed unchanged) and the signed key records
    /// of its routes. Nothing is added to the lineage: this is what the peers already hold, for a new sibling.
    fn current_set(&mut self, rel: &Relationship) -> Option<(Vec<u8>, Vec<Vec<u8>>)> {
        let (head, ..) = self.acs_head()?;
        let node = rel
            .my_lineage
            .effective(|s| Self::signer_ok(&head, s))?
            .clone();
        let previous = match &node.previous {
            Some(p) => Some(h32(p).ok()?),
            None => None,
        };
        let rec = Record::new(node.version, previous, node.entries.clone())?;
        if hex(&rec.hash()) != node.hash {
            return None;
        }
        let signed = self.sign_set(&rec)?;
        let mut records = vec![];
        for e in &rec.entries {
            let binding = hex(&e.binding());
            let key = rel
                .my_delivery
                .iter()
                .filter(|k| k.public.route_binding == binding)
                .max_by_key(|k| k.public.expires_at)?
                .clone();
            records.push(self.signed_key_record(&key).ok()?);
        }
        Some((signed, records))
    }

    // ------------------------------------------------------------------------------------------ new device: inbound

    fn on_join_inbound(&mut self, bytes: &[u8]) {
        let Some(join) = self.st.app.link_join.clone() else {
            return;
        };
        let history_pending =
            join.state == LinkState::Linked && join.history_done < join.history_expected;
        if join.expires_at <= self.now_ms
            || !(matches!(join.state, LinkState::AwaitingApproval) || history_pending)
        {
            return;
        }
        let (Ok(secret), Ok(a_cap), Ok(b_recv)) =
            (unhex(&join.secret), unhex(&join.a_cap), unhex(&join.b_recv))
        else {
            return;
        };
        let (_, k_resp, _) = ceremony_keys(&secret, &a_cap, &b_recv);
        let Some(pt) = open_msg(&k_resp, ROLE_RESP, bytes) else {
            return;
        };
        let Ok(v) = cbor::decode(&pt) else { return };
        let nonce = unhex(&join.nonce).unwrap_or_default();
        match v.get(0).and_then(|x| x.u()) {
            Ok(M_REJECT) if !join.approved => {
                if v.get(1).and_then(|x| x.b()).ok() == Some(nonce.as_slice()) {
                    self.finish_join(LinkState::Rejected);
                }
            }
            Ok(M_APPROVE) if !join.approved => {
                if !self.on_approval(&v, &nonce) {
                    // Anything short of a fully verified approval (including a missing notice) ends the ceremony.
                    self.finish_join(LinkState::Failed);
                }
            }
            Ok(M_HISTORY) if join.approved && join.history_done < join.history_expected => {
                self.on_history_chunk(&v);
            }
            Ok(M_ROSTER) if join.approved && join.roster_done < join.roster_expected => {
                if let Ok(entry) = v.get(1).and_then(|x| x.b()) {
                    let _ = self.adopt_roster(entry, &join);
                }
                if let Some(j) = self.st.app.link_join.as_mut() {
                    j.roster_done += 1;
                    if j.roster_done >= j.roster_expected {
                        self.finish_join(LinkState::Linked);
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_join(&mut self, state: LinkState) {
        if let Some(j) = self.st.app.link_join.as_mut() {
            j.state = state;
            // Chunks of a history snapshot may still follow a completed link; the key stays until they are in.
            if !(state == LinkState::Linked && j.history_done < j.history_expected) {
                j.secret = String::new().into();
            }
        }
    }

    /// Verify an approval completely before adopting any of it: the chain, our own entry in its head, the signed
    /// notice. A failure leaves the profile as it was (no account, no keys published).
    fn on_approval(&mut self, v: &V, nonce: &[u8]) -> bool {
        let (Ok(account), Some(chain), Some(creds), Ok(notice), Ok(approver), Ok(echo), Ok(count)) = (
            v.get(1).and_then(|x| x.b32()),
            bytes_list(v.get(2)),
            bytes_list(v.get(3)),
            v.get(4).and_then(|x| x.b()).map(<[u8]>::to_vec),
            v.get(5).and_then(|x| x.b()).map(<[u8]>::to_vec),
            v.get(6).and_then(|x| x.b()).map(<[u8]>::to_vec),
            v.get(7).and_then(|x| x.u()),
        ) else {
            return false;
        };
        let history_expected = v.opt(9).and_then(|x| x.u().ok()).unwrap_or(0);
        let label = v
            .opt(8)
            .and_then(|x| x.t().ok())
            .map(sanitize_label)
            .unwrap_or_default();
        if echo != nonce
            || chain.len() < 2
            || chain.len() > 65
            || creds.len() > 64
            || chain.iter().chain(creds.iter()).any(|x| x.len() > 65_536)
            || count > MAX_ROSTER as u64
            || notice.is_empty()
        {
            return false;
        }
        let Ok(approver) = <[u8; 16]>::try_from(approver.as_slice()) else {
            return false;
        };
        let Ok(head) = control::verify_chain(&chain, &creds) else {
            return false;
        };
        let (Some(dev), true) = (self.st.device.as_ref(), head.account_id == account) else {
            return false;
        };
        // Our own credential, rebuilt here with the account named by the chain, must be what the head commits to.
        let mine = Credential {
            device_id: dev.device_id,
            account_id: head.account_id,
            signing_pk: dev.signer.public(),
            ik: dev.ik(),
        };
        if !head.messaging.contains(&dev.device_id)
            || head.quarantined.contains(&dev.device_id)
            || !head
                .credentials
                .iter()
                .any(|(d, h)| *d == dev.device_id && *h == mine.hash())
            || !head.control.contains(&approver)
            || head.quarantined.contains(&approver)
        {
            return false;
        }
        // The notice: signed by the approving control device over this very head, naming this device.
        let Some(approver_cred) = creds
            .iter()
            .filter_map(|c| Credential::from_body(c).ok())
            .find(|c| {
                c.device_id == approver
                    && c.account_id == head.account_id
                    && head
                        .credentials
                        .iter()
                        .any(|(d, h)| *d == approver && *h == c.hash())
            })
        else {
            return false;
        };
        if !notice_ok(&notice, &approver_cred, &head, &dev.device_id) {
            return false;
        }
        // Verified: adopt the account state and make this a device of it.
        let now_s = self.now_s();
        let (acs_chain, acs_creds): (Vec<String>, Vec<String>) = (
            chain.iter().map(|c| hex(c)).collect(),
            creds.iter().map(|c| hex(c)).collect(),
        );
        let Some(dev) = self.st.device.as_mut() else {
            return false;
        };
        dev.account_id = head.account_id;
        dev.acs_generation = head.generation;
        dev.bundle_expires_at = now_s + BUNDLE_TTL_S;
        dev.rotation_at = now_s;
        let up = dev.generate_one_time(ONE_TIME_KEYS, ONE_TIME_KEYS, self.rng);
        let call = NetCall::PublishKeyMaterial {
            device_id: dev.device_id.to_vec(),
            bundle: up.bundle,
            opks: up.opks,
            pqopks: up.pqopks,
        };
        let my_id = hex(&dev.device_id);
        self.st.app.acs_chain = acs_chain;
        self.st.app.acs_credentials = acs_creds;
        self.st.app.label = label;
        self.st.app.created_at = self.now_ms;
        self.st.app.device_meta.insert(
            my_id,
            DeviceMeta {
                label: "This device".into(),
                enrolled_at: self.now_ms,
            },
        );
        for d in head.control.iter().chain(&head.messaging) {
            let id = hex(d);
            self.st.app.device_meta.entry(id).or_insert(DeviceMeta {
                label: "Linked device".into(),
                enrolled_at: self.now_ms,
            });
        }
        self.enqueue(call, Purpose::PublishKeys);
        self.sync_own_head();
        if let Some(h) = v.opt(10).and_then(|x| x.b().ok()) {
            self.st.app.carry_head_adopt(h); // the approver's signed head of this tip (groups, D36)
        }
        if let Some(j) = self.st.app.link_join.as_mut() {
            j.approved = true;
            j.roster_expected = count as u32;
            j.history_expected = history_expected.min(HISTORY_MAX_CHUNKS as u64) as u32;
            j.approver = hex(&approver);
            if count == 0 {
                self.finish_join(LinkState::Linked);
            }
        }
        true
    }

    /// One conversation handed over: set up our side of it as a device of the account and introduce ourselves to the
    /// approving device (which tells the contact). Fail closed per conversation: anything off skips it entirely.
    fn adopt_roster(&mut self, entry: &[u8], join: &LinkJoin) -> bool {
        let Ok(v) = cbor::decode(entry) else {
            return false;
        };
        let (Ok(conv), Ok(json), Ok(intro_bytes)) = (
            v.get(1).and_then(|x| x.t()).map(String::from),
            v.get(2).and_then(|x| x.b()),
            v.get(3).and_then(|x| x.b()),
        ) else {
            return false;
        };
        let (Ok(mut rel), Ok(intro)) = (
            serde_json::from_slice::<Relationship>(json),
            cbor::decode(intro_bytes),
        ) else {
            return false;
        };
        if conv.is_empty()
            || conv.len() > 64
            || rel.id != conv
            || rel.state != RelState::Active
            || rel.blocked
            || self.st.app.relationships.contains_key(&conv)
            || intro.get(0).and_then(|x| x.b()).map(hex).ok().as_deref()
                != Some(join.approver.as_str())
        {
            return false;
        }
        // The peer's head is pinned exactly like a first contact's: K7, verified against its own credentials.
        let (Ok(body), Some(creds)) = (
            hb(&rel.peer_acs_body),
            rel.peer_credentials
                .iter()
                .map(|c| unhex(c).ok())
                .collect::<Option<Vec<_>>>(),
        ) else {
            return false;
        };
        if self
            .st
            .device
            .as_mut()
            .is_none_or(|d| d.trust_peer(body, creds).is_err())
        {
            return false;
        }
        let Some((mailbox, route)) = self.take_route() else {
            return false;
        };
        let key = self.new_delivery_key(&route, DELIVERY_KEY_TTL_S);
        let Ok(record) = self.signed_key_record(&key) else {
            return false;
        };
        let Some(set) = Record::new(1, None, vec![route.clone()]) else {
            return false;
        };
        let Some(signed_set) = self.sign_set(&set) else {
            return false;
        };
        let Some(device_id) = self.st.device.as_ref().map(|d| d.device_id) else {
            return false;
        };
        rel.my_mailbox = hex(&mailbox);
        rel.my_relays = RelaySet::single(route.clone());
        rel.my_boxes = vec![MyBox {
            mailbox: hex(&mailbox),
            route: route.clone(),
            retired_at_ms: None,
            drain_until_ms: 0,
            retired_version: 0,
            acked: false,
            closed: false,
        }];
        rel.my_lineage = Lineage {
            nodes: vec![Node::from_record(&set, &device_id)],
            pending: vec![],
        };
        rel.my_delivery = vec![key];
        rel.created_at = self.now_ms;
        rel.messages = vec![];
        // The approving device is a sibling in this conversation; its introduction is verified against our head.
        rel.multi.siblings.remove(&join.approver);
        self.st.app.relationships.insert(conv.clone(), rel);
        self.sync_own_head();
        if !self.adopt_intro(&conv, &intro, true) {
            self.st.app.relationships.remove(&conv);
            return false;
        }
        // Where peers fetch our key bundle for this conversation, and the capability that lets them.
        let fetch_cap = arr32(self.rng);
        self.enqueue(
            NetCall::GrantKeyFetch {
                device_id: device_id.to_vec(),
                capability: fetch_cap.to_vec(),
            },
            Purpose::GrantKeyFetch {
                conversation_id: conv.clone(),
            },
        );
        let my_intro = cbor::encode(&V::M(vec![
            (0, V::B(device_id.to_vec())),
            (1, V::B(signed_set)),
            (2, V::A(vec![V::B(record)])),
            (3, V::T(route.relay_uri.clone())),
            (4, V::B(fetch_cap.to_vec())),
        ]));
        let msg = cbor::encode(&V::M(vec![
            (0, V::U(M_INTRO)),
            (1, V::T(conv)),
            (2, V::B(my_intro)),
        ]));
        let (Ok(secret), Ok(a_cap), Ok(b_recv)) =
            (unhex(&join.secret), unhex(&join.a_cap), unhex(&join.b_recv))
        else {
            return true;
        };
        let (k_req, ..) = ceremony_keys(&secret, &a_cap, &b_recv);
        if let Some(sealed) = self.seal_msg(&k_req, ROLE_REQ, &msg, join.expires_at / 1000) {
            self.deposit_ceremony(&join.operation_id, &a_cap, &join.relay_uri, sealed);
        }
        true
    }

    // ------------------------------------------------------------------------------------------ other devices: notice

    /// A sibling sent the enrollment of a new device with the chain that lists it. Our own head advances only along a
    /// verified descent, and the notice must verify against it before it is shown. `false` is a rejection.
    pub(super) fn on_enrolled(&mut self, p: &V) -> bool {
        let (Ok(notice), Some(chain), Some(creds)) = (
            p.get(1).and_then(|x| x.b()).map(<[u8]>::to_vec),
            bytes_list(p.get(2)),
            bytes_list(p.get(3)),
        ) else {
            return false;
        };
        if chain.is_empty()
            || chain.len() > MAX_CHAIN_TAIL
            || creds.len() > 64
            || chain.iter().chain(creds.iter()).any(|x| x.len() > 65_536)
        {
            return false;
        }
        let Some((head, have, have_creds)) = self.acs_head() else {
            return false;
        };
        // The ordered notices: earlier enrollments first (key 5), the newest in the usual slot (key 1).
        let mut notices = match p.opt(5) {
            None => vec![],
            Some(v) => match bytes_list(Ok(v)) {
                Some(l) if l.len() < MAX_CHAIN_TAIL => l,
                _ => return false,
            },
        };
        notices.push(notice);
        // States this device already held before this payload: a prior notice over one of them is verified but is no
        // news (for example this device's own enrollment).
        let held: Vec<[u8; 32]> = have
            .iter()
            .filter_map(|c| chain_head(c))
            .map(|a| a.hash())
            .collect();
        let Some(staged) = stage_enrollment(head, have, have_creds, &chain, &creds, &notices)
        else {
            return false;
        };
        if notices.iter().any(|n| notice_device(n).is_none()) {
            return false;
        }
        // Commit: head, credentials and notice together, only after the whole staged chain verified.
        if staged.extended {
            self.st.app.acs_chain = staged.chain.iter().map(|c| hex(c)).collect();
            self.st.app.acs_credentials = staged.creds.iter().map(|c| hex(c)).collect();
            self.sync_own_head();
        }
        if let Some(h) = p.opt(4).and_then(|x| x.b().ok()) {
            // a messaging device keeps the head a control device signed (ignored unless it is our tip's)
            self.st.app.carry_head_adopt(h);
        }
        for (i, notice) in notices.iter().enumerate() {
            let Some(device) = notice_device(notice) else {
                continue;
            };
            if i + 1 < notices.len() && notice_state(notice).is_some_and(|h| held.contains(&h)) {
                continue;
            }
            let key = hex(&device);
            if !self.st.app.notices.iter().any(|n| n.device_id == key) {
                let label = self
                    .st
                    .app
                    .device_meta
                    .get(&key)
                    .map(|m| m.label.clone())
                    .unwrap_or_else(|| "Linked device".into());
                self.st
                    .app
                    .device_meta
                    .entry(key.clone())
                    .or_insert(DeviceMeta {
                        label: label.clone(),
                        enrolled_at: self.now_ms,
                    });
                self.st.app.notices.push(NoticeRec {
                    device_id: key,
                    label,
                    at: self.now_ms,
                    signed: hex(notice),
                });
                while self.st.app.notices.len() > MAX_NOTICES {
                    self.st.app.notices.remove(0);
                }
            }
        }
        true
    }
}

/// The result of verifying an enrollment notice against a staged chain: the chain and credentials to commit, and
/// whether the chain grew.
pub(super) struct StagedEnrollment {
    pub chain: Vec<Vec<u8>>,
    pub creds: Vec<Vec<u8>>,
    pub extended: bool,
}

/// Verify `chain` (states that descend from `head`) and the ordered `notices` against a STAGED copy, touching no
/// durable state. Every device a previously unseen state newly lists is an enrollment that needs its own notice, and
/// one notice covers one enrollment: a sibling that missed several enrollments gets the whole ordered list, one notice
/// per unseen enrollment, and the whole list is verified before anything is applied. A missing notice, or any notice
/// that does not verify against its state of the staged chain, refuses the whole payload.
pub(super) fn stage_enrollment(
    mut head: Acs,
    mut have: Vec<Vec<u8>>,
    have_creds: Vec<Vec<u8>>,
    chain: &[Vec<u8>],
    creds: &[Vec<u8>],
    notices: &[Vec<u8>],
) -> Option<StagedEnrollment> {
    let mut all_creds = have_creds.clone();
    for c in creds {
        if !all_creds.contains(c) {
            all_creds.push(c.clone());
        }
    }
    // Extend by the states that descend from our head, each verified as its parent's successor. Every device a
    // previously unseen state newly lists is an enrollment that needs its own notice.
    let mut required: Vec<([u8; 32], [u8; 16])> = vec![];
    let mut extended = false;
    for s in chain {
        let state = chain_head(s)?;
        if state.generation <= head.generation {
            continue; // we hold it already (a different state at our height is a fork and fails the notice below)
        }
        let Ok(next) = control::verify_successor(&head, s, &all_creds) else {
            return None;
        };
        for d in next.messaging.iter().chain(&next.control) {
            let known = head.messaging.contains(d)
                || head.control.contains(d)
                || head.quarantined.contains(d)
                || head.revoked.contains(d);
            if !known && !required.contains(&(next.hash(), *d)) {
                required.push((next.hash(), *d));
            }
        }
        head = next;
        have.push(s.clone());
        extended = true;
    }
    // Every notice must be over a state of the staged chain and signed by a control device of that state.
    if notices.is_empty() || notices.len() > MAX_CHAIN_TAIL {
        return None;
    }
    let mut covered: Vec<([u8; 32], [u8; 16])> = vec![];
    for notice in notices {
        let Ok(body) = cbor::decode(notice).and_then(|v| Ok(v.get(1)?.b()?.to_vec())) else {
            return None;
        };
        let Ok(bv) = cbor::decode(&body) else {
            return None;
        };
        let (Ok(device), Ok(by), Ok(at_hash)) = (
            bv.get(3).and_then(|x| x.b()).map(<[u8]>::to_vec),
            bv.get(7).and_then(|x| x.b()).map(<[u8]>::to_vec),
            bv.get(6).and_then(|x| x.get(1)).and_then(|x| x.b32()),
        ) else {
            return None;
        };
        let (Ok(device), Ok(by)) = (
            <[u8; 16]>::try_from(device.as_slice()),
            <[u8; 16]>::try_from(by.as_slice()),
        ) else {
            return None;
        };
        let state = have
            .iter()
            .filter_map(|c| chain_head(c))
            .find(|s| s.hash() == at_hash)?;
        let by_cred = all_creds
            .iter()
            .filter_map(|c| Credential::from_body(c).ok())
            .find(|c| {
                c.device_id == by
                    && state
                        .credentials
                        .iter()
                        .any(|(d, h)| *d == by && *h == c.hash())
            })?;
        if !state.control.contains(&by) || !notice_ok(notice, &by_cred, &state, &device) {
            return None;
        }
        if covered.contains(&(at_hash, device)) {
            return None; // one notice per enrollment, never repeated
        }
        covered.push((at_hash, device));
    }
    // Each unseen enrollment must have its own verified notice.
    if required.iter().any(|r| !covered.contains(r)) {
        return None;
    }
    // Only credentials some state of the chain lists are kept.
    if extended {
        let listed: Vec<[u8; 32]> = have
            .iter()
            .filter_map(|c| chain_head(c))
            .flat_map(|s| s.credentials.into_iter().map(|(_, h)| h))
            .collect();
        all_creds.retain(|c| {
            have_creds.contains(c)
                || Credential::from_body(c).is_ok_and(|c| listed.contains(&c.hash()))
        });
    }
    Some(StagedEnrollment {
        chain: have,
        creds: all_creds,
        extended,
    })
}

/// The index a history chunk may be counted under: its own field 3, inside `0..expected`, not yet seen.
/// Anything else is a replay, a stray or a malformed frame and changes nothing.
fn history_index(v: &V, expected: u32, seen: &[u32]) -> Option<u32> {
    v.get(3)
        .and_then(|x| x.u())
        .ok()
        .and_then(|i| u32::try_from(i).ok())
        .filter(|i| *i < expected && !seen.contains(i))
}

/// The hash of the control state a (structurally parsed) notice is over.
fn notice_state(notice: &[u8]) -> Option<[u8; 32]> {
    let body = cbor::decode(notice).ok()?.get(1).ok()?.b().ok()?.to_vec();
    cbor::decode(&body)
        .ok()?
        .get(6)
        .ok()?
        .get(1)
        .ok()?
        .b32()
        .ok()
}

/// The device a (structurally parsed) notice announces.
fn notice_device(notice: &[u8]) -> Option<[u8; 16]> {
    let body = cbor::decode(notice).ok()?.get(1).ok()?.b().ok()?.to_vec();
    let bv = cbor::decode(&body).ok()?;
    <[u8; 16]>::try_from(bv.get(3).ok()?.b().ok()?).ok()
}

/// May `device` receive same-owner history at `head`? Only an active device of the account: listed as a messaging or
/// control device and neither quarantined nor revoked (MULTI_DEVICE_SYNC "Revocation": a revoked device cannot obtain
/// new `DeviceHistorySnapshot`s; ACCOUNT_CONTINUITY_AND_CONTROL: quarantine stops future secrets).
pub fn history_eligible(head: &Acs, device: &[u8; 16]) -> bool {
    (head.messaging.contains(device) || head.control.contains(device))
        && !head.quarantined.contains(device)
        && !head.revoked.contains(device)
}

/// A transfer whose time ran out with indices still missing ends degraded, once and for good (the caller runs this
/// when the ceremony is over). A complete or never-approved transfer is untouched.
pub(super) fn mark_history_degraded(j: &mut crate::state::LinkJoin) {
    if j.approved && j.history_expected > 0 && j.history_done < j.history_expected {
        j.history_degraded = true;
    }
}

/// The receiver's view of its transfer: counts and an explicit state, never content.
pub(super) fn receiver_view(j: &crate::state::LinkJoin) -> HistoryTransferView {
    HistoryTransferView {
        role: HistoryRole::Receiver,
        chunks_total: j.history_expected,
        chunks_done: j.history_done,
        chunks_ignored: j.history_ignored,
        messages_added: j.history_messages,
        chunks_missing: j.history_expected.saturating_sub(j.history_done),
        state: if j.history_degraded {
            HistoryTransferState::Degraded
        } else if j.history_done >= j.history_expected {
            HistoryTransferState::Complete
        } else {
            HistoryTransferState::InProgress
        },
    }
}

/// The progress of the latest history transfer, from whichever side this profile was on.
pub fn history_view(st: &ProfileState) -> Option<HistoryTransferView> {
    if let Some(j) = st.app.link_join.as_ref().filter(|j| j.history_expected > 0) {
        return Some(receiver_view(j));
    }
    st.app
        .link_offers
        .values()
        .filter(|o| o.history_sent > 0)
        .max_by_key(|o| o.created_ms)
        .map(|o| HistoryTransferView {
            role: HistoryRole::Sender,
            chunks_total: o.history_sent,
            chunks_done: o.history_sent,
            chunks_ignored: 0,
            messages_added: 0,
            chunks_missing: 0,
            state: HistoryTransferState::Complete,
        })
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// The snapshot as sealed queue messages: this device's history (what an ADR-0184 backup keeps: text and delivery
    /// facts, never a key, route, session or attachment secret), newest first, in chunks that each carry the codec
    /// namespace inside the sealed frame. Chunking is implementation-private. Over the bound the oldest are left out.
    fn history_chunks(&mut self, k_resp: &[u8; 32], expires_s: u64) -> Vec<Vec<u8>> {
        let Ok(archive) = crate::backup::collect(&self.st.app, self.now_ms) else {
            return vec![];
        };
        // Every message, newest first, with the chat it belongs to.
        // The position inside the chat breaks ties between messages of one millisecond, so order survives the trip.
        let mut keyed: Vec<(u64, usize, usize, &crate::backup::ArchivedMsg)> = archive
            .chats
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                c.messages
                    .iter()
                    .enumerate()
                    .map(move |(j, m)| (m.sent_at, j, i, m))
            })
            .collect();
        keyed.sort_by_key(|k| std::cmp::Reverse((k.0, k.1)));
        let all: Vec<(usize, &crate::backup::ArchivedMsg)> =
            keyed.into_iter().map(|(_, _, i, m)| (i, m)).collect();
        let mut groups: Vec<Vec<(usize, &crate::backup::ArchivedMsg)>> = vec![];
        let mut size = usize::MAX;
        for item in all {
            let cost = item.1.text.len() + item.1.id.len() + 160;
            if size == usize::MAX || size + cost > HISTORY_CHUNK_BODY {
                if groups.len() >= HISTORY_MAX_CHUNKS {
                    break;
                }
                groups.push(vec![]);
                size = 0;
            }
            size += cost;
            if let Some(g) = groups.last_mut() {
                g.push(item);
            }
        }
        let mut out = vec![];
        for (index, group) in groups.iter().enumerate() {
            let mut chats: Vec<crate::backup::ArchivedChat> = vec![];
            for (ci, m) in group.iter().rev() {
                let src = &archive.chats[*ci];
                match chats.iter_mut().find(|c| c.id == src.id) {
                    Some(c) => c.messages.push((*m).clone()),
                    None => chats.push(crate::backup::ArchivedChat {
                        messages: vec![(*m).clone()],
                        ..src.clone()
                    }),
                }
            }
            let body = crate::backup::encode_body(&crate::backup::HistoryArchive {
                exported_at_ms: archive.exported_at_ms,
                owner_label: archive.owner_label.clone(),
                chats,
            });
            #[allow(unused_mut)]
            let mut codec_id = unhex(HISTORY_CODEC_ID).unwrap_or_default();
            #[cfg(feature = "test-loopback")]
            if crate::runtime::TEST_FOREIGN_HISTORY_CODEC.load(std::sync::atomic::Ordering::SeqCst)
            {
                codec_id = vec![0x5a; 32];
            }
            let msg = cbor::encode(&V::M(vec![
                (0, V::U(M_HISTORY)),
                (1, V::B(codec_id)),
                (2, V::U(HISTORY_CODEC_MAJOR)),
                (3, V::U(index as u64)),
                (4, V::B(body)),
            ]));
            match self.seal_msg(k_resp, ROLE_RESP, &msg, expires_s) {
                Some(sealed) => out.push(sealed),
                None => return vec![], // all or nothing: a partial snapshot is not announced
            }
        }
        out
    }

    /// One chunk on the new device. Counted once per index, either way; a chunk whose `(codec_id, codec_major)` is not ours, or that is
    /// malformed, is ignored whole and none of it is interpreted. A device that is no longer active at its own head
    /// takes nothing.
    fn on_history_chunk(&mut self, v: &V) {
        let me = self.st.device.as_ref().map(|d| d.device_id);
        let active = match (self.acs_head(), me) {
            (Some((head, _, _)), Some(me)) => history_eligible(&head, &me),
            _ => false,
        };
        let known = v.get(1).and_then(|x| x.b()).ok() == unhex(HISTORY_CODEC_ID).ok().as_deref()
            && v.get(2).and_then(|x| x.u()).ok() == Some(HISTORY_CODEC_MAJOR);
        // The transfer is this ceremony (the frame opened under its key), the total is the count
        // the verified approval announced, and the index is the chunk's own field 3. A frame without an index, with
        // an index outside `0..total`, or whose index was already counted changes nothing.
        let (expected, seen) = match self.st.app.link_join.as_ref() {
            Some(j) => (j.history_expected, j.history_seen.clone()),
            None => return,
        };
        let Some(index) = history_index(v, expected, &seen) else {
            return;
        };
        let added = if active && known {
            v.get(4)
                .and_then(|x| x.b())
                .ok()
                .and_then(|b| crate::backup::decode_body(b).ok())
                .map(|a| self.merge_history(a))
        } else {
            None
        };
        if let Some(j) = self.st.app.link_join.as_mut() {
            j.history_seen.push(index);
            j.history_done = j.history_seen.len() as u32;
            match added {
                Some(n) => j.history_messages += n,
                None => j.history_ignored += 1,
            }
            // Complete only when every index is present.
            if j.history_done >= j.history_expected && j.state == LinkState::Linked {
                j.secret = String::new().into();
            }
        }
    }

    /// Union by EventID (the message id): a message already here is never replaced; chats this device holds live get
    /// the older messages in time order; the others become read-only restored history. Messages that never left the
    /// device they were written on (or were refused) stay there. Returns the messages added.
    fn merge_history(&mut self, archive: crate::backup::HistoryArchive) -> u32 {
        let mut added = 0u32;
        let mut rest = vec![];
        for chat in archive.chats {
            let Some(rel) = self.st.app.relationships.get_mut(&chat.id) else {
                rest.push(chat);
                continue;
            };
            let known: std::collections::BTreeSet<String> =
                rel.messages.iter().map(|m| m.id.clone()).collect();
            for m in chat.messages {
                if known.contains(&m.id) || !(m.delivery == 1 || m.delivery == 2) {
                    continue;
                }
                rel.messages.push(Msg {
                    id: m.id,
                    outgoing: m.outgoing,
                    operation_id: None,
                    text: m.text,
                    sent_at: m.sent_at,
                    delivery: if m.delivery == 2 {
                        DeliveryState::Delivered
                    } else {
                        DeliveryState::Sent
                    },
                    envelope: None,
                    digest: m.digest,
                    clock_ms: m.clock_ms,
                    clock_logical: m.clock_logical,
                    deleted: false,
                    queued_event: None,
                    fan: vec![],
                    attachments: vec![],
                    ext: None,
                    lane: false,
                });
                added += 1;
            }
            rel.messages
                .sort_by_key(|m| (m.sent_at, m.clock_ms, m.clock_logical));
        }
        if !rest.is_empty() {
            let probe = crate::backup::HistoryArchive {
                chats: rest,
                ..archive
            };
            let mut staged = self.st.app.clone();
            if let Ok(s) = crate::backup::apply_import(&mut staged, probe, self.now_ms) {
                added += s.messages_added;
                self.st.app = staged;
            }
        }
        added
    }
}

/// `payload` (a canonical map) with the controller-signed head added under `key` when there is one.
fn with_head(payload: Vec<u8>, key: u64, head: Option<&[u8]>) -> Vec<u8> {
    let (Some(h), Ok(V::M(mut m))) = (head, cbor::decode(&payload)) else {
        return payload;
    };
    m.push((key, V::B(h.to_vec())));
    cbor::encode(&V::M(m))
}

fn chain_head(signed: &[u8]) -> Option<Acs> {
    let v = cbor::decode(signed).ok()?;
    Acs::from_body(v.get(1).ok()?.b().ok()?).ok()
}

/// Is `notice` a `DeviceEnrollmentNotice` for `device`, ceremony `link_ceremony`, signed by `signer` over exactly `head`?
fn notice_ok(notice: &[u8], signer: &Credential, head: &Acs, device: &[u8; 16]) -> bool {
    let Ok(body) = sig::open_envelope(
        notice,
        LBL_NOTICE_SIG,
        PROFILE_V1,
        ROLE_CONTROLLER,
        &signer.device_id,
        &signer.signing_pk,
    ) else {
        return false;
    };
    if registry()
        .check_body("DeviceEnrollmentNotice", &body)
        .is_err()
    {
        return false;
    }
    let Ok(v) = cbor::decode(&body) else {
        return false;
    };
    v.get(2).and_then(|x| x.b32()).ok() == Some(head.account_id)
        && v.get(3).and_then(|x| x.b()).ok() == Some(device.as_slice())
        && v.get(4).and_then(|x| x.u()).ok() == Some(DEVICE_CLASS_MESSAGING)
        && v.get(5).and_then(|x| x.u()).ok() == Some(head.generation)
        && v.get(6).and_then(|x| x.get(1)).and_then(|x| x.b32()).ok() == Some(head.hash())
        && v.get(7).and_then(|x| x.b()).ok() == Some(signer.device_id.as_slice())
        && v.get(8).and_then(|x| x.u()).ok() == Some(CEREMONY_LINK)
        && head.messaging.contains(device)
}

// ---------------------------------------------------------------------------------------------- views and polling

/// What the host should poll for the ceremony: the existing device's request queue (its own capability) while the
/// ceremony runs, the new device's response queue (the capability from the link) until it ends.
pub fn poll_targets(st: &ProfileState, now_ms: u64) -> Vec<crate::host::PollTarget> {
    let mut out = vec![];
    for o in st.app.link_offers.values() {
        if o.expires_at > now_ms
            && matches!(
                o.state,
                LinkState::WaitingForScan | LinkState::AwaitingApproval | LinkState::Linked
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
    if let Some(j) = &st.app.link_join
        && j.expires_at > now_ms
        && (j.state == LinkState::AwaitingApproval
            || (j.state == LinkState::Linked && j.history_done < j.history_expected))
        && let (Ok(m), Ok(cap)) = (unhex(&j.b_mailbox), unhex(&j.b_recv))
    {
        out.push(crate::host::PollTarget {
            mailbox: m,
            after: st.app.cursors.get(&j.b_mailbox).copied().unwrap_or(0),
            kind: MailboxKind::Relay,
            relay_uri: Some(j.relay_uri.clone()),
            capability: cap,
        });
    }
    out
}

/// The Home and explicit capability for acknowledging a ceremony mailbox (`None`: not a ceremony mailbox).
pub(super) fn ack_route(st: &ProfileState, mailbox: &str) -> Option<(String, Vec<u8>)> {
    if let Some(o) = st.app.link_offers.values().find(|o| o.a_mailbox == mailbox) {
        return Some((o.a_route.relay_uri.clone(), vec![]));
    }
    st.app
        .link_join
        .as_ref()
        .filter(|j| j.b_mailbox == mailbox)
        .map(|j| (j.relay_uri.clone(), unhex(&j.b_recv).unwrap_or_default()))
}

fn offer_state(o: &LinkOffer, now_ms: u64) -> Option<LinkOfferState> {
    let live = o.expires_at > now_ms;
    Some(match o.state {
        LinkState::Opening => return None,
        LinkState::WaitingForScan if live => LinkOfferState::WaitingForScan,
        LinkState::AwaitingApproval if live => LinkOfferState::AwaitingApproval,
        LinkState::Linked => LinkOfferState::Linked,
        LinkState::Rejected => LinkOfferState::Rejected,
        _ => LinkOfferState::Expired,
    })
}

pub fn offer_view(st: &ProfileState, op: &str, now_ms: u64) -> Option<LinkOfferView> {
    let o = st.app.link_offers.get(op)?;
    Some(LinkOfferView {
        operation_id: o.operation_id.clone(),
        link: o.link.clone()?,
        expires_at: o.expires_at,
        verification_code: o.request.as_ref().map(|r| r.code.clone()),
        state: offer_state(o, now_ms)?,
    })
}

/// A request as the approving device holds it, or as the asking device made it.
pub fn request_view(st: &ProfileState, op: &str, now_ms: u64) -> Option<LinkRequestView> {
    if let Some(j) = st.app.link_join.as_ref().filter(|j| j.operation_id == op) {
        let state = match j.state {
            LinkState::Linked => LinkRequestState::Approved,
            LinkState::Rejected | LinkState::Failed => LinkRequestState::Rejected,
            LinkState::Expired => LinkRequestState::Expired,
            _ if j.approved => LinkRequestState::Approved,
            _ if j.expires_at <= now_ms => LinkRequestState::Expired,
            _ => LinkRequestState::AwaitingApproval,
        };
        return Some(LinkRequestView {
            request_id: j.request_id.clone(),
            operation_id: j.operation_id.clone(),
            device_label: j.label.clone(),
            verification_code: j.code.clone(),
            expires_at: j.expires_at,
            state,
        });
    }
    let o = st.app.link_offers.get(op)?;
    let r = o.request.as_ref()?;
    let state = match o.state {
        LinkState::Linked => LinkRequestState::Approved,
        LinkState::Rejected => LinkRequestState::Rejected,
        LinkState::AwaitingApproval if o.expires_at > now_ms => LinkRequestState::AwaitingApproval,
        _ => LinkRequestState::Expired,
    };
    Some(LinkRequestView {
        request_id: r.request_id.clone(),
        operation_id: o.operation_id.clone(),
        device_label: r.label.clone(),
        verification_code: r.code.clone(),
        expires_at: o.expires_at,
        state,
    })
}

/// Requests an approving device still has to decide.
pub fn pending_requests(st: &ProfileState, now_ms: u64) -> Vec<LinkRequestView> {
    st.app
        .link_offers
        .keys()
        .filter_map(|op| request_view(st, op, now_ms))
        .filter(|r| r.state == LinkRequestState::AwaitingApproval)
        .collect()
}

pub fn notice_views(st: &ProfileState) -> Vec<DeviceNoticeView> {
    st.app
        .notices
        .iter()
        .map(|n| DeviceNoticeView {
            code: DeviceNoticeCode::DeviceLinked,
            device_id: n.device_id.clone(),
            label: n.label.clone(),
            at: n.at,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head_with(control: &[u8], messaging: &[u8], quarantined: &[u8], revoked: &[u8]) -> Acs {
        let set = |ids: &[u8]| ids.iter().map(|b| [*b; 16]).collect();
        Acs {
            account_id: [9; 32],
            generation: 3,
            control: set(control),
            quarantined: set(quarantined),
            messaging: set(messaging),
            revoked: set(revoked),
            credentials: vec![],
            previous: None,
            constitutional: (1, None),
            recovery_commitment: None,
            min_profile: 1,
            key_epochs: vec![],
            transparency: None,
        }
    }

    #[test]
    fn only_an_active_device_may_receive_history() {
        let head = head_with(&[1], &[2, 3, 4], &[3], &[4]);
        assert!(history_eligible(&head, &[1; 16]), "a control device");
        assert!(
            history_eligible(&head, &[2; 16]),
            "an active messaging device"
        );
        assert!(
            !history_eligible(&head, &[3; 16]),
            "a quarantined device gets nothing new"
        );
        assert!(
            !history_eligible(&head, &[4; 16]),
            "a revoked device gets nothing new"
        );
        assert!(
            !history_eligible(&head, &[5; 16]),
            "an unknown device gets nothing"
        );
        // Revoked wins even if a malformed head also lists the device as active.
        let both = head_with(&[1], &[2], &[], &[2]);
        assert!(!history_eligible(&both, &[2; 16]));
    }

    fn envelope_with_frame(key: &[u8; 32], frame: &[u8], bucket_frame: usize) -> Vec<u8> {
        let mut padded = frame.to_vec();
        padded.resize(bucket_frame, 0);
        let nonce = [7u8; 24];
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &padded,
                    aad: &aad(ROLE_REQ),
                },
            )
            .unwrap();
        wrap_envelope(&nonce, ct, [9u8; 32], vec![1u8; ENC_LEN], 1_790_000_300)
    }

    #[test]
    fn padding_must_be_zeros_and_the_length_must_fit() {
        let key = [3u8; 32];
        let size = BUCKETS[0] - NONCE_LEN - TAG_LEN;
        let mut good = 5u32.to_be_bytes().to_vec();
        good.extend_from_slice(b"hello");
        assert_eq!(
            open_msg(&key, ROLE_REQ, &envelope_with_frame(&key, &good, size)),
            Some(b"hello".to_vec())
        );
        // A byte after the message that is not zero: the AEAD is genuine, the padding is not.
        let mut dirty = good.clone();
        dirty.resize(size, 0);
        dirty[size - 1] = 1;
        assert_eq!(
            open_msg(&key, ROLE_REQ, &envelope_with_frame(&key, &dirty, size)),
            None
        );
        // A length that runs past the frame.
        let mut long = 5000u32.to_be_bytes().to_vec();
        long.extend_from_slice(b"hello");
        assert_eq!(
            open_msg(&key, ROLE_REQ, &envelope_with_frame(&key, &long, size)),
            None
        );
        // The wrong role or key, and a ciphertext that is not a bucket length.
        assert_eq!(
            open_msg(&key, ROLE_RESP, &envelope_with_frame(&key, &good, size)),
            None
        );
        assert_eq!(
            open_msg(&key, ROLE_REQ, &envelope_with_frame(&key, &good, size - 8)),
            None
        );
    }

    // ---- enrollment notice and history chunk negative tests -------------------------------------------------------------

    use crate::crypto::cbor::{self, V};
    use crate::crypto::rng::insecure_test_rng::InsecureTestRng;
    use crate::crypto::store::Device;

    struct Fx {
        rng: InsecureTestRng,
        ctl: Device,
        states: Vec<Acs>,
        signed: Vec<Vec<u8>>,
        creds: Vec<Vec<u8>>,
        new_devs: Vec<Device>,
    }

    impl Fx {
        fn new() -> Fx {
            let mut rng = InsecureTestRng::from_label(b"f01");
            let ctl = Device::new(&mut rng);
            let g = ctl.acs();
            let signed = control::sign_acs(&g.body(), &[(&ctl.device_id, &ctl.signer)], &mut rng);
            Fx {
                creds: vec![ctl.credential().body()],
                signed: vec![signed],
                states: vec![g],
                ctl,
                rng,
                new_devs: vec![],
            }
        }
        /// Append a state enrolling `n` new messaging devices at once.
        fn enroll(&mut self, n: usize) {
            let head = self.states.last().unwrap().clone();
            let mut child = head.clone();
            child.generation += 1;
            child.previous = Some(head.hash());
            for _ in 0..n {
                let mut d = Device::new(&mut self.rng);
                d.account_id = head.account_id;
                child.messaging.insert(d.device_id);
                child.credentials.push((d.device_id, d.credential().hash()));
                self.creds.push(d.credential().body());
                self.new_devs.push(d);
            }
            let signed = control::sign_acs(
                &child.body(),
                &[(&self.ctl.device_id, &self.ctl.signer)],
                &mut self.rng,
            );
            self.signed.push(signed);
            self.states.push(child);
        }
        fn notice(&mut self, state: usize, device: [u8; 16], signer: &Device) -> Vec<u8> {
            let child = &self.states[state];
            let body = cbor::encode(&V::M(vec![
                (0, V::U(8)),
                (1, V::U(1)),
                (2, V::B(child.account_id.to_vec())),
                (3, V::B(device.to_vec())),
                (4, V::U(DEVICE_CLASS_MESSAGING)),
                (5, V::U(child.generation)),
                (6, digest(&child.hash())),
                (7, V::B(signer.device_id.to_vec())),
                (8, V::U(CEREMONY_LINK)),
                (9, V::U(1_790_000_000)),
            ]));
            let sg = signer.signer.sign_with(
                &ds_sig(
                    LBL_NOTICE_SIG,
                    PROFILE_V1,
                    ROLE_CONTROLLER,
                    &signer.device_id,
                    &body,
                ),
                [5; 32],
            );
            sig::envelope(
                body,
                vec![signature_entry(
                    &signer.device_id,
                    PROFILE_V1,
                    ROLE_CONTROLLER,
                    sg,
                )],
            )
        }
        fn stage(&self, have: usize, upto: usize, notice: &[u8]) -> Option<StagedEnrollment> {
            self.stage_all(have, upto, &[notice.to_vec()])
        }
        fn stage_all(
            &self,
            have: usize,
            upto: usize,
            notices: &[Vec<u8>],
        ) -> Option<StagedEnrollment> {
            stage_enrollment(
                self.states[have - 1].clone(),
                self.signed[..have].to_vec(),
                self.creds.clone(),
                &self.signed[have..upto],
                &self.creds,
                notices,
            )
        }
    }

    #[test]
    fn one_enrollment_with_its_notice_stages() {
        let mut f = Fx::new();
        f.enroll(1);
        let b = f.new_devs[0].device_id;
        let ctl = f.ctl.clone();
        let n = f.notice(1, b, &ctl);
        let staged = f.stage(1, 2, &n).expect("verified");
        assert!(staged.extended);
        assert_eq!(staged.chain.len(), 2);
    }

    #[test]
    fn a_bad_notice_stages_nothing() {
        let mut f = Fx::new();
        f.enroll(1);
        let b = f.new_devs[0].device_id;
        // signed by a device the state does not list as a controller
        let stranger = f.new_devs[0].clone();
        let n = f.notice(1, b, &stranger);
        assert!(f.stage(1, 2, &n).is_none());
        // a notice for a device the successor did not enroll
        let ctl = f.ctl.clone();
        let n = f.notice(1, [7; 16], &ctl);
        assert!(f.stage(1, 2, &n).is_none());
        // garbage
        assert!(f.stage(1, 2, &[1, 2, 3]).is_none());
    }

    #[test]
    fn a_missing_notice_for_an_unseen_enrollment_refuses() {
        let mut f = Fx::new();
        f.enroll(1);
        f.enroll(1);
        let ctl = f.ctl.clone();
        let (b, c) = (f.new_devs[0].device_id, f.new_devs[1].device_id);
        // two successors, one notice: refused whichever one it covers
        let n_c = f.notice(2, c, &ctl);
        assert!(f.stage(1, 3, &n_c).is_none(), "B has no notice");
        let n_b = f.notice(1, b, &ctl);
        assert!(f.stage(1, 3, &n_b).is_none(), "C has no notice");
        // a notice for an already held state does not excuse an unseen enrollment
        assert!(f.stage(2, 3, &n_b).is_none());
    }

    #[test]
    fn two_devices_in_one_successor_need_two_notices() {
        let mut f = Fx::new();
        f.enroll(2);
        let ctl = f.ctl.clone();
        let n = f.notice(1, f.new_devs[0].device_id, &ctl);
        assert!(f.stage(1, 2, &n).is_none());
    }

    #[test]
    fn a_replay_of_a_held_enrollment_is_accepted_but_changes_nothing() {
        let mut f = Fx::new();
        f.enroll(1);
        f.enroll(1);
        let ctl = f.ctl.clone();
        let c = f.new_devs[1].device_id;
        let n = f.notice(2, c, &ctl);
        let staged = f.stage(3, 3, &n).expect("held chain, valid notice");
        assert!(!staged.extended);
        assert_eq!(staged.chain, f.signed);
    }

    #[test]
    fn a_sibling_that_missed_several_enrollments_catches_up_with_the_whole_notice_list() {
        let mut f = Fx::new();
        f.enroll(1);
        f.enroll(1);
        f.enroll(1);
        let ctl = f.ctl.clone();
        let d: Vec<[u8; 16]> = f.new_devs.iter().map(|x| x.device_id).collect();
        let n: Vec<Vec<u8>> = (0..3).map(|i| f.notice(i + 1, d[i], &ctl)).collect();
        // the full ordered list stages the whole chain in one step
        let staged = f
            .stage_all(1, 4, &n)
            .expect("every enrollment has its notice");
        assert!(staged.extended);
        assert_eq!(staged.chain.len(), 4);
        // order does not matter, but each notice is used once
        let rev: Vec<Vec<u8>> = n.iter().rev().cloned().collect();
        assert!(f.stage_all(1, 4, &rev).is_some());
        let mut dup = n.clone();
        dup.push(n[0].clone());
        assert!(f.stage_all(1, 4, &dup).is_none(), "a repeated notice");
        // fail closed: one notice short, or one bad notice, refuses the whole list
        assert!(
            f.stage_all(1, 4, &n[..2]).is_none(),
            "newest enrollment has no notice"
        );
        assert!(
            f.stage_all(1, 4, &n[1..]).is_none(),
            "oldest enrollment has no notice"
        );
        let stranger = f.new_devs[0].clone();
        let bad = f.notice(2, d[1], &stranger);
        assert!(
            f.stage_all(1, 4, &[n[0].clone(), bad, n[2].clone()])
                .is_none()
        );
        assert!(
            f.stage_all(1, 4, &[n[0].clone(), vec![1, 2, 3], n[2].clone()])
                .is_none()
        );
        // a sibling that already holds the first two needs only the third
        assert!(f.stage_all(3, 4, &n[2..]).is_some());
    }

    #[test]
    fn a_transfer_that_ends_with_missing_indices_is_degraded_and_says_so() {
        let mut j = LinkJoin {
            operation_id: Default::default(),
            secret: String::new().into(),
            relay_uri: String::new(),
            a_cap: String::new(),
            b_mailbox: String::new(),
            b_recv: String::new(),
            created_ms: 0,
            expires_at: 0,
            label: String::new(),
            nonce: String::new(),
            request_id: String::new(),
            code: String::new(),
            state: LinkState::Linked,
            approved: true,
            roster_expected: 0,
            roster_done: 0,
            approver: String::new(),
            history_expected: 3,
            history_done: 2,
            history_ignored: 0,
            history_messages: 4,
            history_seen: vec![0, 2],
            history_degraded: false,
        };
        let live = receiver_view(&j);
        assert_eq!(live.state, HistoryTransferState::InProgress);
        assert_eq!(live.chunks_missing, 1);
        mark_history_degraded(&mut j);
        let end = receiver_view(&j);
        assert_eq!(end.state, HistoryTransferState::Degraded);
        assert_eq!(
            (end.chunks_done, end.chunks_total, end.chunks_missing),
            (2, 3, 1)
        );
        assert_eq!(end.messages_added, 4, "what arrived is kept");
        // a complete transfer is never marked, and an unapproved ceremony has no transfer to degrade
        let mut done = j.clone();
        done.history_degraded = false;
        done.history_seen.push(1);
        done.history_done = 3;
        mark_history_degraded(&mut done);
        assert_eq!(receiver_view(&done).state, HistoryTransferState::Complete);
        let mut unapproved = j.clone();
        unapproved.history_degraded = false;
        unapproved.approved = false;
        mark_history_degraded(&mut unapproved);
        assert!(!unapproved.history_degraded);
    }

    #[test]
    fn a_history_chunk_counts_only_its_own_new_index_inside_the_total() {
        let chunk = |i: Option<u64>| {
            let mut m = vec![(0, V::U(1))];
            if let Some(i) = i {
                m.push((3, V::U(i)));
            }
            V::M(m)
        };
        assert_eq!(history_index(&chunk(Some(0)), 3, &[]), Some(0));
        assert_eq!(
            history_index(&chunk(Some(2)), 3, &[0]),
            Some(2),
            "out of order"
        );
        assert_eq!(history_index(&chunk(Some(0)), 3, &[0]), None, "replayed");
        assert_eq!(
            history_index(&chunk(Some(3)), 3, &[]),
            None,
            "outside total"
        );
        assert_eq!(history_index(&chunk(Some(u64::MAX)), 3, &[]), None);
        assert_eq!(history_index(&chunk(None), 3, &[]), None, "no index");
        // n replays of one frame never reach the total (truncated transfer stays incomplete)
        let mut seen: Vec<u32> = vec![];
        for _ in 0..5 {
            if let Some(i) = history_index(&chunk(Some(1)), 5, &seen) {
                seen.push(i);
            }
        }
        assert_eq!(seen, vec![1]);
    }
}
