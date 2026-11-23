// SPDX-License-Identifier: MPL-2.0
//! TEST ONLY: an in-memory loopback provider (key material, introduction service, relay mailboxes), an in-memory
//! encrypted-store stand-in and a synchronous driver for the sans-IO [`Core`]. Compiled only with the
//! `test-loopback` feature, which cannot be combined with `production` (R-072). Real hosts implement the same
//! `NetCall` / `Commit` contract over the PlatformAdapter.

use crate::api::*;
use crate::crypto::rng::insecure_test_rng::InsecureTestRng;
use crate::host::*;
use crate::route::{DepositRevocation, RoomRouteOffer, RouteOffer};
use crate::{Core, crypto::hex};
use rand_core::RngCore as _;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The loopback's relay name and capability shape. Deliberately unlike the mailbox id (a fixed prefix plus the id), so a
/// Core that confused "mailbox I poll" with "capability peers deposit to" would not work against it.
pub const LOOPBACK_RELAY_URI: &str = "https://loopback.test/";
const LOOPBACK_CAP_PREFIX: [u8; 16] = [0xC4; 16];

/// What a provider adapter would allocate for one new mailbox on the loopback relay.
pub fn loopback_offer(mailbox: [u8; 16]) -> RouteOffer {
    let mut capability = LOOPBACK_CAP_PREFIX.to_vec();
    capability.extend_from_slice(&mailbox);
    RouteOffer {
        mailbox: mailbox.to_vec(),
        relay_uri: LOOPBACK_RELAY_URI.to_string(),
        capability,
        priority: 0,
    }
}

/// The loopback can stand in for several Homes: `https://loopback.test/` is Home 0, `https://loopback.test/h<n>/` is
/// Home n. A person with several Homes gets offers spread over them, the first preferred (`priority` = its rank).
pub fn loopback_home_uri(home: u8) -> String {
    if home == 0 {
        LOOPBACK_RELAY_URI.to_string()
    } else {
        format!("{LOOPBACK_RELAY_URI}h{home}/")
    }
}

/// What a LAN receiver would allocate (`local_delivery`, ADR-0158 section 5): an `om+lan://v1/<token>` route whose capability
/// the loopback maps to the mailbox the same way it does for a Home route. Test shape only; the real table is `lan_route`.
pub fn lan_offer(mailbox: [u8; 16]) -> RouteOffer {
    let mut o = loopback_offer(mailbox);
    o.relay_uri = format!(
        "{}{}",
        crate::lan_route::URI_PREFIX,
        crate::flows::b64url(&mailbox)
    );
    o
}

/// What a provider adapter would allocate for one new mailbox at loopback Home `home`.
pub fn loopback_offer_at(mailbox: [u8; 16], home: u8) -> RouteOffer {
    let mut o = loopback_offer(mailbox);
    o.relay_uri = loopback_home_uri(home);
    o.priority = home;
    o
}

/// Which mailbox a deposit capability addresses: a real-route capability ends in the mailbox id, a legacy one is it.
fn mailbox_of_deposit(mailbox: &[u8], relay_uri: Option<&str>) -> Vec<u8> {
    match relay_uri {
        Some(u)
            if (u.starts_with(LOOPBACK_RELAY_URI)
                || u.starts_with(crate::lan_route::URI_PREFIX))
                && mailbox.len() == 32
                && mailbox[..16] == LOOPBACK_CAP_PREFIX =>
        {
            mailbox[16..].to_vec()
        }
        Some(_) => vec![], // not this relay's capability: lands nowhere
        None => mailbox.to_vec(),
    }
}

// TEST ONLY (`test-loopback`): what a Welcome's sender does with the account heads it would carry, per test thread:
// 0 normal, 1 leave them out, 2 corrupt each signed head; 3 and 4 the same but only for an Add Commit's heads (D93), a
// Welcome keeping its own. Per-thread, so tests that set it do not disturb others.
#[cfg(feature = "test-loopback")]
thread_local! {
    pub static TEST_ROOM_HEADS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// TEST ONLY: while set, an approving Core leaves the enrollment notice out of the approval (the new device must refuse).
/// Process-global, so a test that sets it lives in a test binary of its own.
pub static TEST_OMIT_NOTICE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// TEST ONLY: while set, an approving Core corrupts the approval binding (the new device must adopt nothing).
/// Process-global: its test lives in a test binary of its own.
/// Test switch of the recovery approval reply (a controller that misbehaves): 1 changed target, 2 other request digest,
/// 3 last chunk missing, 4 a conflicting duplicate of chunk 0 first, 5 bad reply tag. Process-global: a test binary of its own.
pub static TEST_APPROVAL_FAULT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub static TEST_BAD_BINDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// TEST ONLY: while set, an approving Core labels its history snapshot with a codec this implementation does not know
/// (the new device must ignore every chunk whole). Process-global: its test lives in a binary of its own.
pub static TEST_FOREIGN_HISTORY_CODEC: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The loopback's receive capability for a queue it opened (48 bytes, as a real Home's).
fn queue_receive_cap(mailbox: &[u8]) -> Vec<u8> {
    let mut cap = vec![0xD5; 16];
    cap.extend_from_slice(mailbox);
    cap.extend_from_slice(&[0x5D; 16]);
    cap
}

fn receive_cap_queue(cap: &[u8]) -> Option<Vec<u8>> {
    (cap.len() == 48 && cap[..16] == [0xD5; 16] && cap[32..] == [0x5D; 16])
        .then(|| cap[16..32].to_vec())
}

pub const TEST_ONLY_MARKER: &str = "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION";

#[derive(Default)]
struct Published {
    bundle: Vec<u8>,
    /// The signed-prekey generation of the stored bundle (0 when the bundle is opaque).
    generation: u64,
    /// One-time items with the generation of the bundle that was current when they were published.
    opks: Vec<(u64, Vec<u8>)>,
    pqopks: Vec<(u64, Vec<u8>)>,
}

use crate::crypto::binding::bundle_generation_unverified as bundle_generation;

#[derive(Default)]
struct Mailbox {
    next_seq: u64,
    items: Vec<(u64, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Reject the call before doing anything (definitely not performed).
    Unavailable,
    /// Perform the call but report a timeout (the bytes were delivered; outcome unknown to the caller).
    TimeoutAfterEffect,
    /// Report a timeout without performing it.
    TimeoutBeforeEffect,
    /// Refuse the call for good (the Home will not take it, however often it is retried).
    Refused,
}

/// One object in the loopback BlobStore.
struct StoredBlob {
    owner_cap: Vec<u8>,
    ciphertext: Vec<u8>,
    digest: Vec<u8>,
}

/// What the loopback BlobStore enforces (the Home's defaults: 4 MiB per object, 32 MiB, 64 objects).
#[derive(Clone, Copy)]
pub struct StoreLimits {
    pub max_blob_bytes: u64,
    pub max_spool_bytes: u64,
    pub max_blobs: usize,
}

impl Default for StoreLimits {
    fn default() -> Self {
        StoreLimits {
            max_blob_bytes: 4 * 1024 * 1024,
            max_spool_bytes: 32 * 1024 * 1024,
            max_blobs: 64,
        }
    }
}

/// Posix seconds the loopback store reports as every blob's expiry (the test clock starts at 1_790_000_000 s).
pub const LOOPBACK_BLOB_EXPIRY_S: u64 = 1_790_000_000 + 7 * 24 * 3600;
const LOOPBACK_BLOB_BASE: &str = "https://blobs.loopback.test/v1/blobs/";

fn blob_read_cap(owner_cap: &[u8]) -> Vec<u8> {
    let mut c = owner_cap[..16].to_vec();
    c.extend_from_slice(&crate::crypto::sha256(&[b"loopback-read-cap", owner_cap])[..16]);
    c
}

#[derive(Default)]
struct LoopInbox {
    cap_hash: [u8; 32],
    items: Vec<([u8; 16], u8, Vec<u8>)>,
    next: u64,
}

/// One record of the loopback RecoveryStore. The Home sees ciphertext only: it binds `SHA-256(write_cap)` at create,
/// answers a put by `If-Match` (compare-and-swap on the `ETag` it computes over the bytes) and replays a repeated
/// `request_id` with the same bytes without storing twice.
struct RecoverySlot {
    write_hash: [u8; 32],
    bytes: Vec<u8>,
    etag: String,
    last_request: Option<(Vec<u8>, Vec<u8>, String)>,
}

fn recovery_etag(bytes: &[u8]) -> String {
    crate::crypto::hex(&crate::crypto::sha256(&[bytes]))
}

#[derive(Default)]
struct NetState {
    /// Ciphertext the hosts staged (what a real host keeps on its own disk); consumed by `PutBlob`.
    staged: BTreeMap<Vec<u8>, Vec<u8>>,
    blobs: BTreeMap<Vec<u8>, StoredBlob>,
    store_limits: StoreLimits,
    /// Every `PutBlob` the store received, by object (the BlobStore-visible stream: ciphertext only).
    blob_puts: u64,
    keys: BTreeMap<Vec<u8>, Published>,
    /// Fetch capabilities each device owner granted (relationship fetch capabilities).
    fetch_grants: BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
    invites: BTreeMap<Vec<u8>, Vec<u8>>,
    /// The loopback RecoveryStore (ADR-0201 semantics a Core test needs): locator -> stored record.
    recovery: BTreeMap<Vec<u8>, RecoverySlot>,
    /// Every accepted or replayed put, for assertions: (locator, request id, bytes).
    recovery_log: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    /// The loopback Home's recovery traffic (ADR-0201 section 8.2): requests posted (deduplicated by bytes) and the veto
    /// board by request digest. A Home verifies no signature, so neither does this.
    recovery_requests: Vec<Vec<u8>>,
    recovery_vetoes: BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
    /// Recovery inboxes by account id: the SHA-256 of the capability that reads them and the items in arrival order
    /// (id, kind, bytes). Fan-out appends only to an inbox that exists, and answers the same either way.
    recovery_inboxes: BTreeMap<Vec<u8>, LoopInbox>,
    mailboxes: BTreeMap<Vec<u8>, Mailbox>,
    faults: Vec<Fault>,
    /// Loopback Homes that are down: deposits to them are `unavailable`, and nothing can be drained from them.
    blocked: Vec<String>,
    revoked_room_grants: std::collections::BTreeSet<(String, String)>,
    deposits: u64,
    /// Every deposit a relay ever received, in order (the relay-visible byte stream; mailboxes are drained by acks).
    log: Vec<(Vec<u8>, Vec<u8>)>,
}

/// One loopback provider shared by every client in a test (cheap to clone).
#[derive(Clone, Default)]
pub struct LoopbackNetwork(Rc<RefCell<NetState>>);

impl LoopbackNetwork {
    /// Queue a fault for the next network call of any kind.
    pub fn inject(&self, f: Fault) {
        self.0.borrow_mut().faults.push(f);
    }
    /// Stage the ciphertext of a prepared attachment, as a real host does on its own disk.
    pub fn stage_blob(&self, blob_id: &[u8], ciphertext: Vec<u8>) {
        self.0
            .borrow_mut()
            .staged
            .insert(blob_id.to_vec(), ciphertext);
    }
    pub fn set_store_limits(&self, l: StoreLimits) {
        self.0.borrow_mut().store_limits = l;
    }
    pub fn blob_count(&self) -> usize {
        self.0.borrow().blobs.len()
    }
    pub fn blob_put_count(&self) -> u64 {
        self.0.borrow().blob_puts
    }
    /// Everything the BlobStore holds, as it sees it: ciphertext only.
    pub fn blob_ciphertexts(&self) -> Vec<Vec<u8>> {
        self.0
            .borrow()
            .blobs
            .values()
            .map(|b| b.ciphertext.clone())
            .collect()
    }
    /// Flip one bit of every stored object (a store that serves altered bytes).
    pub fn corrupt_blobs(&self) {
        for b in self.0.borrow_mut().blobs.values_mut() {
            if let Some(x) = b.ciphertext.first_mut() {
                *x ^= 1;
            }
        }
    }
    /// Forget every stored object (the store's retention ran out).
    pub fn expire_blobs(&self) {
        self.0.borrow_mut().blobs.clear();
    }
    /// Take a loopback Home down (a vendor block, an outage): deposits to its routes fail, its mailboxes cannot be polled.
    pub fn block_home(&self, relay_uri: &str) {
        self.0.borrow_mut().blocked.push(relay_uri.to_string());
    }
    pub fn unblock_home(&self, relay_uri: &str) {
        self.0.borrow_mut().blocked.retain(|u| u != relay_uri);
    }
    pub fn deposit_count(&self) -> u64 {
        self.0.borrow().deposits
    }
    /// Everything a relay could see for a mailbox (for ciphertext-only assertions).
    pub fn mailbox_bytes(&self, mailbox: &[u8]) -> Vec<Vec<u8>> {
        self.0
            .borrow()
            .mailboxes
            .get(mailbox)
            .map(|m| m.items.iter().map(|(_, b)| b.clone()).collect())
            .unwrap_or_default()
    }
    /// Every byte string any relay mailbox ever received, in deposit order (acks free relay storage, not this log).
    pub fn all_deposited_bytes(&self) -> Vec<Vec<u8>> {
        self.0.borrow().log.iter().map(|(_, b)| b.clone()).collect()
    }
    /// Test only: the Home loses one queued item (a deposit that was accepted and never reached the reader). Returns whether
    /// the exact bytes were still queued in `mailbox`.
    pub fn lose_queued(&self, mailbox: &[u8], bytes: &[u8]) -> bool {
        let mut s = self.0.borrow_mut();
        let Some(m) = s.mailboxes.get_mut(mailbox) else {
            return false;
        };
        let n = m.items.len();
        m.items.retain(|(_, b)| b != bytes);
        m.items.len() != n
    }
    /// The deposit log with its destination mailbox.
    pub fn deposit_log(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.0.borrow().log.clone()
    }
    /// One-time prekeys the key-material provider still holds, per published device: (device id, EC items, PQ items).
    pub fn key_inventory(&self) -> Vec<(Vec<u8>, usize, usize)> {
        self.0
            .borrow()
            .keys
            .iter()
            .map(|(d, p)| (d.clone(), p.opks.len(), p.pqopks.len()))
            .collect()
    }
    /// The signed-prekey generation of the bundle the key-material provider holds, per published device.
    pub fn key_generations(&self) -> Vec<(Vec<u8>, u64)> {
        self.0
            .borrow()
            .keys
            .iter()
            .map(|(d, p)| (d.clone(), p.generation))
            .collect()
    }
    /// Hostile introduction service: exchange the descriptors stored under two invite ids.
    pub fn swap_invites(&self, a: &[u8], b: &[u8]) {
        let mut s = self.0.borrow_mut();
        if let (Some(x), Some(y)) = (s.invites.get(a).cloned(), s.invites.get(b).cloned()) {
            s.invites.insert(a.to_vec(), y);
            s.invites.insert(b.to_vec(), x);
        }
    }
    /// The envelope the loopback RecoveryStore holds at `locator`, with its ETag.
    pub fn recovery_record(&self, locator: &[u8]) -> Option<(String, Vec<u8>)> {
        self.0
            .borrow()
            .recovery
            .get(locator)
            .map(|s| (s.etag.clone(), s.bytes.clone()))
    }
    /// The signed requests posted to the loopback Home, in order.
    pub fn recovery_requests(&self) -> Vec<Vec<u8>> {
        self.0.borrow().recovery_requests.clone()
    }

    /// Post a signed request as any client could (a Home checks shape only, so a forgery is stored like any request).
    pub fn recovery_post_request(&self, signed: &[u8]) {
        let mut s = self.0.borrow_mut();
        if let Some(account) = crate::recovery::request_inbox_id(signed)
            && let Some(inbox) = s.recovery_inboxes.get_mut(account.as_slice())
        {
            inbox.next += 1;
            let mut id = [0u8; 16];
            id[8..].copy_from_slice(&inbox.next.to_be_bytes());
            inbox.items.push((id, 1, signed.to_vec()));
        }
        s.recovery_requests.push(signed.to_vec());
    }

    /// The Home forgets every veto it held (a prune): nothing a verifier already observed depends on it.
    pub fn recovery_clear_vetoes(&self) {
        self.0.borrow_mut().recovery_vetoes.clear();
    }

    /// Post a signed veto as a controller's host would (the board is public and unverified).
    pub fn recovery_post_veto(&self, signed: &[u8]) {
        if let Some(d) = crate::recovery::veto_digest(signed) {
            self.0
                .borrow_mut()
                .recovery_vetoes
                .entry(d.to_vec())
                .or_default()
                .push(signed.to_vec());
        }
    }

    /// Every put the loopback RecoveryStore stored (locator, request id, bytes), in order.
    pub fn recovery_puts(&self) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        self.0.borrow().recovery_log.clone()
    }
    /// Another writer replaces the record behind Core's back (the compare-and-swap tests).
    pub fn recovery_replace(&self, locator: &[u8], bytes: Vec<u8>) {
        if let Some(s) = self.0.borrow_mut().recovery.get_mut(locator) {
            s.etag = recovery_etag(&bytes);
            s.bytes = bytes;
            s.last_request = None;
        }
    }
    pub fn revoke_all_invites(&self) {
        self.0.borrow_mut().invites.clear();
    }

    pub fn revoke_deposit(&self, grant: &DepositRevocation) {
        if grant.id == grant.mailbox {
            self.0
                .borrow_mut()
                .revoked_room_grants
                .insert((grant.relay_uri.clone(), grant.id.clone()));
        }
    }

    pub fn perform(&self, call: &NetCall) -> NetResult {
        let mut s = self.0.borrow_mut();
        let fault = if s.faults.is_empty() {
            None
        } else {
            Some(s.faults.remove(0))
        };
        match fault {
            Some(Fault::Unavailable) => {
                return NetResult::Err {
                    error: NetError::Unavailable,
                };
            }
            Some(Fault::TimeoutBeforeEffect) => {
                return NetResult::Err {
                    error: NetError::Timeout,
                };
            }
            Some(Fault::Refused) => {
                return NetResult::Err {
                    error: NetError::Rejected {
                        reason: crate::api::RejectReason::PolicyDenied,
                    },
                };
            }
            _ => {}
        }
        let r = match call {
            NetCall::PublishKeyMaterial {
                device_id,
                bundle,
                opks,
                pqopks,
            } => {
                let p = s.keys.entry(device_id.clone()).or_default();
                let generation = bundle_generation(bundle);
                // Deviation D28: one-time items of a lower generation can never verify under a higher one, so a
                // newly published higher generation drops them.
                if generation > p.generation {
                    p.opks.retain(|(g, _)| *g >= generation);
                    p.pqopks.retain(|(g, _)| *g >= generation);
                }
                p.generation = p.generation.max(generation);
                p.bundle = bundle.clone();
                p.opks.extend(opks.iter().map(|o| (generation, o.clone())));
                p.pqopks
                    .extend(pqopks.iter().map(|o| (generation, o.clone())));
                NetResult::Ok
            }
            NetCall::RevokeKeyGeneration {
                device_id,
                generation,
            } => {
                if let Some(p) = s.keys.get_mut(device_id)
                    && p.generation == *generation
                {
                    *p = Published::default();
                }
                NetResult::Ok
            }
            NetCall::GrantKeyFetch {
                device_id,
                capability,
            } => {
                s.fetch_grants
                    .entry(device_id.clone())
                    .or_default()
                    .push(capability.clone());
                NetResult::Ok
            }
            // A fetch needs a capability the device owner granted (ASYNC_KEY_MATERIAL 2.1); anonymous fetch gets nothing.
            NetCall::FetchKeyMaterial {
                device_id,
                capability,
                ..
            } if !s
                .fetch_grants
                .get(device_id)
                .is_some_and(|g| g.contains(capability)) =>
            {
                NetResult::Err {
                    error: NetError::Rejected {
                        reason: RejectReason::Unauthorized,
                    },
                }
            }
            NetCall::FetchKeyMaterial { device_id, .. } => match s.keys.get_mut(device_id) {
                None => NetResult::Err {
                    error: NetError::NotFound,
                },
                Some(p) => {
                    let mut out = vec![p.bundle.clone()];
                    if !p.opks.is_empty() {
                        out.push(p.opks.remove(0).1);
                    }
                    if !p.pqopks.is_empty() {
                        out.push(p.pqopks.remove(0).1);
                    }
                    NetResult::Many { items: out }
                }
            },
            NetCall::PutInvite {
                invite_id,
                descriptor,
                ..
            } => {
                s.invites.insert(invite_id.clone(), descriptor.clone());
                NetResult::Ok
            }
            NetCall::GetInvite { invite_id, .. } => match s.invites.get(invite_id) {
                Some(d) => NetResult::Bytes { bytes: d.clone() },
                None => NetResult::Err {
                    error: NetError::NotFound,
                },
            },
            NetCall::RevokeInvite { invite_id } => {
                s.invites.remove(invite_id);
                NetResult::Ok
            }
            NetCall::Deposit {
                relay_uri: Some(u), ..
            } if s.blocked.contains(u) => NetResult::Err {
                error: NetError::Unavailable,
            },
            NetCall::Deposit {
                mailbox,
                relay_uri,
                bytes,
            } => {
                let mailbox = mailbox_of_deposit(mailbox, relay_uri.as_deref());
                if let Some(uri) = relay_uri
                    && s.revoked_room_grants
                        .contains(&(uri.clone(), hex(&mailbox)))
                {
                    return NetResult::Err {
                        error: NetError::Rejected {
                            reason: RejectReason::Revoked,
                        },
                    };
                }
                s.deposits += 1;
                s.log.push((mailbox.clone(), bytes.clone()));
                let m = s.mailboxes.entry(mailbox).or_default();
                m.next_seq += 1;
                let seq = m.next_seq;
                m.items.push((seq, bytes.clone()));
                NetResult::Ok
            }
            // The introduction service holds each invite's requests in a queue named by the ticket id (the invite id).
            NetCall::SubmitIntro { bytes, .. } => {
                let ticket = crate::crypto::cbor::decode(bytes)
                    .ok()
                    .and_then(|v| v.get(2).and_then(|t| t.b().map(<[u8]>::to_vec)).ok())
                    .unwrap_or_default();
                s.deposits += 1;
                s.log.push((ticket.clone(), bytes.clone()));
                let m = s.mailboxes.entry(ticket).or_default();
                m.next_seq += 1;
                let seq = m.next_seq;
                m.items.push((seq, bytes.clone()));
                NetResult::Ok
            }
            // A ceremony queue: the receive capability is derived from the mailbox, like a real Home's, and checked on poll.
            NetCall::OpenQueue { mailbox, .. } => {
                s.mailboxes.entry(mailbox.clone()).or_default();
                NetResult::Bytes {
                    bytes: queue_receive_cap(mailbox),
                }
            }
            NetCall::PutBlob {
                blob_id,
                owner_cap,
                digest,
                length,
                ..
            } => 'put: {
                let Some(ct) = s.staged.get(blob_id).cloned() else {
                    break 'put NetResult::Err {
                        error: NetError::Rejected {
                            reason: crate::api::RejectReason::Invalid,
                        },
                    };
                };
                let rej = |reason| NetResult::Err {
                    error: NetError::Rejected { reason },
                };
                if owner_cap.len() < 32 || ct.len() as u64 != *length {
                    break 'put rej(crate::api::RejectReason::Invalid);
                }
                let loc = owner_cap[..16].to_vec();
                let receipt = |loc: &[u8]| NetResult::Bytes {
                    bytes: crate::host::BlobReceipt {
                        read_cap: blob_read_cap(owner_cap),
                        locations: vec![format!("{LOOPBACK_BLOB_BASE}{}", crate::crypto::hex(loc))],
                        digest: digest.as_slice().try_into().unwrap_or([0; 32]),
                        expires_at: Some(LOOPBACK_BLOB_EXPIRY_S),
                    }
                    .to_bytes(),
                };
                if let Some(old) = s.blobs.get(&loc) {
                    // Same owner capability and the same bytes: an exact retry, answered as before.
                    break 'put if old.owner_cap == *owner_cap && old.digest == *digest {
                        receipt(&loc)
                    } else {
                        rej(crate::api::RejectReason::Invalid)
                    };
                }
                let limits = s.store_limits;
                if ct.len() as u64 > limits.max_blob_bytes {
                    break 'put rej(crate::api::RejectReason::TooLarge);
                }
                if crate::attach::ciphertext_digest(&ct).as_slice() != digest.as_slice() {
                    break 'put rej(crate::api::RejectReason::Invalid);
                }
                let used: u64 = s.blobs.values().map(|b| b.ciphertext.len() as u64).sum();
                if s.blobs.len() >= limits.max_blobs
                    || used + ct.len() as u64 > limits.max_spool_bytes
                {
                    break 'put rej(crate::api::RejectReason::Quota);
                }
                s.blob_puts += 1;
                s.blobs.insert(
                    loc.clone(),
                    StoredBlob {
                        owner_cap: owner_cap.clone(),
                        ciphertext: ct,
                        digest: digest.clone(),
                    },
                );
                receipt(&loc)
            }
            NetCall::GetBlob {
                locations,
                read_cap,
                ..
            } => {
                let found = locations
                    .iter()
                    .filter_map(|l| l.strip_prefix(LOOPBACK_BLOB_BASE))
                    .filter_map(|h| crate::crypto::unhex(h).ok())
                    .find_map(|loc| s.blobs.get(&loc));
                match found {
                    Some(b) if blob_read_cap(&b.owner_cap) == *read_cap => NetResult::Bytes {
                        bytes: b.ciphertext.clone(),
                    },
                    _ => NetResult::Err {
                        error: NetError::NotFound,
                    },
                }
            }
            NetCall::DeleteBlob { owner_cap, .. } => {
                let loc = owner_cap.get(..16).unwrap_or_default().to_vec();
                match s.blobs.get(&loc) {
                    None => NetResult::Ok,
                    Some(b) if b.owner_cap == *owner_cap => {
                        s.blobs.remove(&loc);
                        NetResult::Ok
                    }
                    Some(_) => NetResult::Err {
                        error: NetError::Rejected {
                            reason: crate::api::RejectReason::Unauthorized,
                        },
                    },
                }
            }
            NetCall::RecoveryPut {
                locator,
                write_cap,
                request_id,
                if_match,
                bytes,
                ..
            } => {
                let wh = crate::crypto::sha256(&[write_cap]);
                let reject = |reason| NetResult::Err {
                    error: NetError::Rejected { reason },
                };
                match s.recovery.get_mut(locator) {
                    None => {
                        if if_match.is_some() {
                            reject(RejectReason::StaleState)
                        } else {
                            let etag = recovery_etag(bytes);
                            s.recovery.insert(
                                locator.clone(),
                                RecoverySlot {
                                    write_hash: wh,
                                    bytes: bytes.clone(),
                                    etag: etag.clone(),
                                    last_request: Some((
                                        request_id.clone(),
                                        bytes.clone(),
                                        etag.clone(),
                                    )),
                                },
                            );
                            s.recovery_log.push((
                                locator.clone(),
                                request_id.clone(),
                                bytes.clone(),
                            ));
                            NetResult::Bytes {
                                bytes: etag.into_bytes(),
                            }
                        }
                    }
                    Some(slot) if slot.write_hash != wh => reject(RejectReason::NotFound),
                    Some(slot) => match &slot.last_request {
                        Some((id, b, etag)) if id == request_id => {
                            if b == bytes {
                                // the same request id with the same bytes: the determined outcome, nothing stored twice
                                NetResult::Bytes {
                                    bytes: etag.clone().into_bytes(),
                                }
                            } else {
                                reject(RejectReason::Conflict)
                            }
                        }
                        _ if if_match.as_deref() != Some(slot.etag.as_str()) => {
                            reject(RejectReason::StaleState)
                        }
                        _ => {
                            let etag = recovery_etag(bytes);
                            slot.bytes = bytes.clone();
                            slot.etag = etag.clone();
                            slot.last_request =
                                Some((request_id.clone(), bytes.clone(), etag.clone()));
                            s.recovery_log.push((
                                locator.clone(),
                                request_id.clone(),
                                bytes.clone(),
                            ));
                            NetResult::Bytes {
                                bytes: etag.into_bytes(),
                            }
                        }
                    },
                }
            }
            NetCall::RecoveryGet {
                locator, read_cap, ..
            } => match s.recovery.get(locator) {
                Some(slot)
                    if crate::recovery_record::locator_of(
                        &read_cap.as_slice().try_into().unwrap_or([0; 32]),
                    )
                    .as_slice()
                        == locator.as_slice() =>
                {
                    NetResult::Many {
                        items: vec![slot.etag.clone().into_bytes(), slot.bytes.clone()],
                    }
                }
                _ => NetResult::Err {
                    error: NetError::NotFound,
                },
            },
            NetCall::RecoveryDelete {
                locator, write_cap, ..
            } => {
                let key = crate::crypto::unhex(locator).unwrap_or_default();
                let hash = crate::crypto::unhex(write_cap)
                    .map(|cap| crate::crypto::sha256(&[&cap]))
                    .ok();
                if s.recovery
                    .get(&key)
                    .is_some_and(|r| Some(r.write_hash) == hash)
                {
                    s.recovery.remove(&key);
                    NetResult::Ok
                } else {
                    NetResult::Err {
                        error: NetError::NotFound,
                    }
                }
            }
            NetCall::RecoveryRequestPost { bytes, .. } => {
                if !s.recovery_requests.contains(bytes) {
                    s.recovery_requests.push(bytes.clone());
                    if let Some(account) = crate::recovery::request_inbox_id(bytes)
                        && let Some(inbox) = s.recovery_inboxes.get_mut(account.as_slice())
                    {
                        inbox.next += 1;
                        let mut id = [0u8; 16];
                        id[8..].copy_from_slice(&inbox.next.to_be_bytes());
                        inbox.items.push((id, 1, bytes.clone()));
                    }
                }
                NetResult::Ok
            }
            NetCall::RecoveryInboxRegister {
                account_id,
                recovery_epoch,
                cap,
                ..
            } => {
                let inbox_id = crate::recovery::inbox_id(
                    &account_id.as_slice().try_into().unwrap_or_default(),
                    *recovery_epoch,
                );
                let h = crate::crypto::sha256(&[cap]);
                match s.recovery_inboxes.get(inbox_id.as_slice()) {
                    Some(i) if i.cap_hash != h => NetResult::Err {
                        error: NetError::Rejected {
                            reason: RejectReason::Unauthorized,
                        },
                    },
                    Some(_) => NetResult::Ok,
                    None => {
                        s.recovery_inboxes.insert(
                            inbox_id.to_vec(),
                            LoopInbox {
                                cap_hash: h,
                                ..LoopInbox::default()
                            },
                        );
                        NetResult::Ok
                    }
                }
            }
            NetCall::RecoveryInboxDrain {
                account_id,
                recovery_epoch,
                cap,
                ..
            } => {
                let inbox_id = crate::recovery::inbox_id(
                    &account_id.as_slice().try_into().unwrap_or_default(),
                    *recovery_epoch,
                );
                let h = crate::crypto::sha256(&[cap]);
                match s.recovery_inboxes.get(inbox_id.as_slice()) {
                    Some(i) if i.cap_hash == h => NetResult::Many {
                        items: i
                            .items
                            .iter()
                            .take(16)
                            .map(|(id, kind, b)| {
                                crate::crypto::cbor::encode(&crate::crypto::cbor::V::M(vec![
                                    (0, crate::crypto::cbor::V::B(id.to_vec())),
                                    (1, crate::crypto::cbor::V::U(u64::from(*kind))),
                                    (2, crate::crypto::cbor::V::B(b.clone())),
                                ]))
                            })
                            .collect(),
                    },
                    // a wrong capability and an absent inbox are answered alike
                    _ => NetResult::Err {
                        error: NetError::Rejected {
                            reason: RejectReason::Unauthorized,
                        },
                    },
                }
            }
            NetCall::RecoveryInboxAck {
                account_id,
                recovery_epoch,
                cap,
                item_ids,
                ..
            } => {
                let inbox_id = crate::recovery::inbox_id(
                    &account_id.as_slice().try_into().unwrap_or_default(),
                    *recovery_epoch,
                );
                let h = crate::crypto::sha256(&[cap]);
                match s.recovery_inboxes.get_mut(inbox_id.as_slice()) {
                    Some(i) if i.cap_hash == h => {
                        i.items
                            .retain(|(id, _, _)| !item_ids.contains(&crate::crypto::hex(id)));
                        NetResult::Ok
                    }
                    _ => NetResult::Err {
                        error: NetError::Rejected {
                            reason: RejectReason::Unauthorized,
                        },
                    },
                }
            }
            NetCall::RecoveryVetoPost { bytes, .. } => {
                if let Some(account) = crate::recovery::veto_inbox_id(bytes)
                    && let Some(inbox) = s.recovery_inboxes.get_mut(account.as_slice())
                    && inbox.items.len() < 40
                {
                    inbox.next += 1;
                    let mut id = [0u8; 16];
                    id[8..].copy_from_slice(&inbox.next.to_be_bytes());
                    inbox.items.push((id, 2, bytes.clone()));
                }
                let digest = crate::recovery::veto_digest(bytes);
                if let Some(d) = digest {
                    let board = s.recovery_vetoes.entry(d.to_vec()).or_default();
                    if !board.contains(bytes) && board.len() < 16 {
                        board.push(bytes.clone());
                    }
                }
                NetResult::Ok
            }
            NetCall::RecoveryVetoRead { request_digest, .. } => NetResult::Many {
                items: s
                    .recovery_vetoes
                    .get(request_digest)
                    .cloned()
                    .unwrap_or_default(),
            },
            // No Home serves recovery fan-out yet (stub transport): refused, and Core drops the outbox entry.
            NetCall::RecoveryRequestFanout { .. } | NetCall::RecoveryVetoFanout { .. } => {
                NetResult::Err {
                    error: NetError::Rejected {
                        reason: RejectReason::Unsupported,
                    },
                }
            }
            NetCall::Ack {
                mailbox,
                seq,
                capability,
                ..
            } => {
                let queue = if capability.is_empty() {
                    Some(mailbox.clone())
                } else {
                    receive_cap_queue(capability)
                };
                if let Some(m) = queue.and_then(|q| s.mailboxes.get_mut(&q)) {
                    m.items.retain(|(q, _)| q > seq);
                }
                NetResult::Ok
            }
        };
        if fault == Some(Fault::TimeoutAfterEffect) {
            return NetResult::Err {
                error: NetError::Timeout,
            };
        }
        r
    }

    /// Poll a mailbox at the Home its route names (nothing comes back from a blocked Home).
    pub fn poll_at(&self, relay_uri: Option<&str>, mailbox: &[u8], after: u64) -> Vec<Inbound> {
        if relay_uri.is_some_and(|u| self.0.borrow().blocked.iter().any(|b| b == u)) {
            return vec![];
        }
        self.poll(mailbox, after)
    }

    pub fn poll(&self, mailbox: &[u8], after: u64) -> Vec<Inbound> {
        self.0
            .borrow()
            .mailboxes
            .get(mailbox)
            .map(|m| {
                m.items
                    .iter()
                    .filter(|(q, _)| *q > after)
                    .map(|(q, b)| Inbound {
                        seq: *q,
                        bytes: b.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Per-profile durable records, standing in for the host's EncryptedKv. Tests can make the next commit fail.
#[derive(Clone, Default)]
pub struct MemStore(Rc<RefCell<MemStoreInner>>);

#[derive(Default)]
struct MemStoreInner {
    data: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    fail_commits: u32,
}

impl MemStore {
    pub fn fail_next_commit(&self) {
        self.0.borrow_mut().fail_commits += 1;
    }
    /// Replace one stored record (tests that rewrite a profile into an older shape and reopen it).
    pub fn replace_record(&self, profile: &str, key: &str, value: Vec<u8>) {
        self.0
            .borrow_mut()
            .data
            .entry(profile.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }
    pub fn records(&self, profile: &str) -> Vec<Record> {
        self.0
            .borrow()
            .data
            .get(profile)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| Record {
                        key: k.clone(),
                        value: v.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// All stored bytes of a profile (for "no plaintext/secret at rest outside the sealed record" style checks).
    pub fn raw(&self, profile: &str) -> Vec<Vec<u8>> {
        self.0
            .borrow()
            .data
            .get(profile)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
    fn commit(&self, c: &Commit) -> bool {
        let mut s = self.0.borrow_mut();
        if s.fail_commits > 0 {
            s.fail_commits -= 1;
            return false;
        }
        let m = s.data.entry(c.profile_id.clone()).or_default();
        for w in &c.writes {
            match &w.value {
                Some(v) => m.insert(w.key.clone(), v.clone()),
                None => m.remove(&w.key),
            };
        }
        true
    }
}

pub fn home() -> HomeSummary {
    HomeSummary {
        tier: "T1".into(),
        observers: vec![
            HomeObserverCode::HostSeesMetadata,
            HomeObserverCode::HostSeesCiphertext,
        ],
        migration_available: true,
        client_origin_choice: vec![ClientOriginChoiceCode::DefaultOrigin],
    }
}

/// A synchronous host around [`Core`]: commits through the store, performs the outbox against the loopback
/// provider and polls mailboxes. `restart` throws the Core away and rebuilds it from the store alone.
pub struct Runtime {
    pub core: Core<InsecureTestRng>,
    pub store: MemStore,
    pub net: LoopbackNetwork,
    pub clock: Rc<Cell<u64>>,
    label: String,
    boots: u32,
    open: Vec<String>,
    offer_rng: InsecureTestRng,
    /// The loopback Homes this person has, most preferred first (offers rotate over them); `[0]` is the ordinary case.
    pub homes: Vec<u8>,
    /// TEST ONLY: hand Core `om+lan://v1` route offers instead of Home offers (local_delivery loopback).
    pub lan_routes: bool,
    /// What the host tells Core the BlobStore advertised (re-supplied after a restart, as a real host does at start).
    pub blob_limits: Option<crate::host::BlobLimits>,
    /// Where this Home keeps recovery records (re-supplied after a restart). `None` unless a test opts in.
    pub recovery_place: Option<crate::recovery_record::RecoveryPlace>,
}

impl Runtime {
    pub fn new(label: &str, net: &LoopbackNetwork) -> Self {
        let clock = Rc::new(Cell::new(1_790_000_000_000u64));
        let c2 = clock.clone();
        Runtime {
            core: Core::new(
                InsecureTestRng::from_label(format!("{label}/0").as_bytes()),
                move || c2.get(),
                home(),
            ),
            store: MemStore::default(),
            net: net.clone(),
            clock,
            label: label.to_string(),
            boots: 0,
            open: vec![],
            offer_rng: InsecureTestRng::from_label(format!("{label}/offers").as_bytes()),
            homes: vec![0],
            lan_routes: false,
            blob_limits: Some(crate::host::BlobLimits {
                max_blob_bytes: 4 * 1024 * 1024,
                max_spool_bytes: 32 * 1024 * 1024,
                max_blobs: 64,
            }),
            recovery_place: None,
        }
        .with_limits()
    }

    fn with_limits(mut self) -> Self {
        self.core.set_blob_limits(self.blob_limits);
        self
    }

    /// Tell Core where this Home keeps recovery records (`None`: it offers none).
    pub fn set_recovery_place(&mut self, p: Option<crate::recovery_record::RecoveryPlace>) {
        self.recovery_place = p.clone();
        self.core.set_recovery_place(p);
    }

    /// Tell Core what the store advertises (`None`: it offers no blobs).
    pub fn set_blob_limits(&mut self, l: Option<crate::host::BlobLimits>) {
        self.blob_limits = l;
        self.core.set_blob_limits(l);
    }

    /// Seal an attachment through Core, commit it, and stage the ciphertext as a host would.
    pub fn prepare_attachment(
        &mut self,
        ctx: &ProfileExecutionContext,
        plaintext: &[u8],
        meta: crate::attach::Meta,
    ) -> Result<PreparedAttachment, crate::attach::AttachError> {
        let (out, step) = self
            .core
            .prepare_attachment(ctx, plaintext, meta)
            .expect("prepare");
        assert!(self.settle(&step), "commit");
        let p = out?;
        self.net.stage_blob(
            &crate::crypto::unhex(&p.blob_id).unwrap(),
            p.ciphertext.clone(),
        );
        Ok(p)
    }

    /// What a host does when the person opens an attachment: take Core's fetch plan, perform it, hand the bytes back.
    pub fn fetch_attachment(
        &mut self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<OpenedAttachment, crate::attach::AttachError> {
        let plan = self.core.attachment_fetch(ctx, message_id, index)?;
        match self.net.perform(&plan) {
            NetResult::Bytes { bytes } => self.core.open_attachment(ctx, message_id, index, &bytes),
            _ => Err(crate::attach::AttachError::NotFound),
        }
    }

    /// Stands in for the driver topping Core's route offers up from the provider adapter (done before every dispatch).
    pub fn supply_routes(&mut self, n: usize) {
        let homes = self.homes.clone();
        let offers = (0..n)
            .map(|i| {
                let mut mb = [0u8; 16];
                self.offer_rng.fill_bytes(&mut mb);
                let at = i % homes.len();
                let mut o = if self.lan_routes {
                    lan_offer(mb)
                } else {
                    loopback_offer_at(mb, homes[at])
                };
                o.priority = at as u8;
                o
            })
            .collect();
        self.core.supply_routes(offers);
        for profile in &self.open {
            let ctx = self.core.context_for(profile).unwrap();
            let expiry = self.clock.get() / 1000 + crate::groups::carriage::ROUTE_TTL_S + 300;
            let offers = (0..n)
                .map(|i| {
                    let mut mailbox = [0u8; 16];
                    self.offer_rng.fill_bytes(&mut mailbox);
                    RoomRouteOffer {
                        route: loopback_offer_at(mailbox, homes[i % homes.len()]),
                        id: mailbox.to_vec(),
                        expires_at: expiry,
                    }
                })
                .collect();
            self.core.supply_room_routes(&ctx, offers).unwrap();
        }
    }

    /// Recovery observations as a host would hand them in (see `Core::recovery_observe_*`): `session` names the host's
    /// boot of its monotonic clock and `mono_ms` is that clock's reading.
    pub fn recovery_observe_request(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed: &[u8],
        mono: &MonotonicSample,
    ) {
        let step = self
            .core
            .recovery_observe_request(ctx, signed, mono)
            .expect("observe request");
        self.settle(&step);
    }

    pub fn recovery_observe_veto(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed: &[u8],
        mono: &MonotonicSample,
    ) {
        let step = self
            .core
            .recovery_observe_veto(ctx, signed, mono)
            .expect("observe veto");
        self.settle(&step);
    }

    pub fn recovery_observe_successor(
        &mut self,
        ctx: &ProfileExecutionContext,
        request_id: &str,
        bundle: &crate::recovery::SuccessorBundle,
        mono: &MonotonicSample,
    ) {
        let step = self
            .core
            .recovery_observe_successor(ctx, request_id, bundle, mono)
            .expect("observe successor");
        self.settle(&step);
    }

    /// The recovering device's `RecoveryNotice` push (see `Core::recovery_send_notices`); returns how many were queued.
    pub fn recovery_send_notices(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed_request: &[u8],
        successor: Option<&crate::recovery::SuccessorBundle>,
    ) -> usize {
        let (n, step) = self
            .core
            .recovery_send_notices(ctx, signed_request, successor)
            .expect("send recovery notices");
        self.settle(&step);
        n
    }

    pub fn recovery_tick(&mut self, ctx: &ProfileExecutionContext, mono: &MonotonicSample) {
        let step = self.core.recovery_tick(ctx, mono).expect("recovery tick");
        self.settle(&step);
    }

    pub fn tick(&self, ms: u64) {
        self.clock.set(self.clock.get() + ms);
    }

    pub fn open(&mut self, profile: &str) -> ProfileExecutionContext {
        let recs = self.store.records(profile);
        let ctx = self
            .core
            .open_profile(profile, &recs)
            .expect("open profile");
        if !self.open.iter().any(|p| p == profile) {
            self.open.push(profile.to_string());
        }
        ctx
    }

    /// Process restart: a new Core (new RNG stream) over the same durable records.
    pub fn restart(&mut self) -> Vec<ProfileExecutionContext> {
        self.boots += 1;
        let c2 = self.clock.clone();
        self.core = Core::new(
            InsecureTestRng::from_label(format!("{}/{}", self.label, self.boots).as_bytes()),
            move || c2.get(),
            home(),
        );
        self.core.set_blob_limits(self.blob_limits);
        self.core.set_recovery_place(self.recovery_place.clone());
        let open = self.open.clone();
        open.iter().map(|p| self.open(p)).collect()
    }

    /// Commit a step through the store. Returns false when the host could not persist it (nothing released).
    pub fn settle(&mut self, step: &Step) -> bool {
        match &step.commit {
            None => true,
            Some(c) => {
                let ok = self.store.commit(c);
                self.core.finish_commit(c.id, ok).expect("finish commit");
                ok
            }
        }
    }

    pub fn dispatch(
        &mut self,
        ctx: &ProfileExecutionContext,
        op: &str,
        body: IntentBody,
    ) -> OperationHandle {
        self.supply_routes(4usize.saturating_sub(self.core.route_offers()));
        let step = self
            .core
            .dispatch(&Intent {
                ctx: ctx.clone(),
                operation_id: op.to_string(),
                body,
            })
            .expect("dispatch");
        self.settle(&step);
        step.handle.expect("handle")
    }

    pub fn outcome(&self, ctx: &ProfileExecutionContext, op: &str) -> Option<OutcomeClass> {
        self.core.outcome(ctx, op).expect("outcome")
    }

    pub fn query<T: serde::de::DeserializeOwned>(&self, q: Query) -> T {
        serde_json::from_value(self.core.query(&q).expect("query")).expect("query result shape")
    }

    /// Perform every owed network call once (in outbox order) and feed the results back.
    pub fn flush_net(&mut self, ctx: &ProfileExecutionContext) -> usize {
        let ops = self.core.net_ops(ctx).expect("net ops");
        let n = ops.len();
        for op in ops {
            let result = self.net.perform(&op.call);
            let step = self
                .core
                .net_result(ctx, op.id, result)
                .expect("net result");
            self.settle(&step);
        }
        n
    }

    /// Poll every mailbox once.
    pub fn poll(&mut self, ctx: &ProfileExecutionContext) -> usize {
        let mut n = 0;
        for t in self.core.poll_targets(ctx).expect("poll targets") {
            // A queue opened for someone else is drained only with the capability its owner handed out, which names it.
            let queue = if t.capability.is_empty() {
                t.mailbox.clone()
            } else {
                match receive_cap_queue(&t.capability) {
                    Some(q) => q,
                    None => continue,
                }
            };
            let items = self.net.poll_at(t.relay_uri.as_deref(), &queue, t.after);
            if items.is_empty() {
                continue;
            }
            n += items.len();
            let step = self.core.inbound(ctx, &t.mailbox, &items).expect("inbound");
            self.settle(&step);
        }
        n
    }

    /// One full cycle: owed network work, then polling.
    pub fn pump(&mut self, ctx: &ProfileExecutionContext) -> usize {
        let tick = self.core.notice_tick(ctx).expect("notice tick");
        self.settle(&tick);
        self.flush_net(ctx) + self.poll(ctx)
    }
}

/// Alternate `pump` over several (runtime, context) pairs until nothing moves.
pub fn settle_all(parties: &mut [(&mut Runtime, &ProfileExecutionContext)]) {
    for _ in 0..40 {
        let mut moved = 0;
        for (rt, ctx) in parties.iter_mut() {
            moved += rt.pump(ctx);
        }
        if moved == 0 {
            return;
        }
    }
    panic!("network did not settle");
}

pub fn mailbox_hex(b: &[u8]) -> String {
    hex(b)
}
