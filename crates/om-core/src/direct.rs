// SPDX-License-Identifier: MPL-2.0
//! Direct (non-spool) transfer of an encrypted attachment (ADR-0150 "Direct encrypted file transfer", ADR-0158 sections 6
//! and 8, STORAGE_AND_BLOB_ARCHITECTURE "Zero-payment personal bootstrap", AT-06).
//!
//! A file bigger than the Home spool bound is not sent through the spool. The sender keeps the sealed object (the same
//! ciphertext, BlobID and `ciphertext_digest` any BlobStore would hold) and serves it to the recipient over a byte channel
//! while the two are online or on the same network. This module is the transport-neutral, sans-IO state machine of that
//! exchange; it owns no socket, no clock and no randomness (the caller supplies transfer ids).
//!
//! * The frames mirror `LanFrameV1` (`spec/cddl/lan.cddl`): `fetch_blob`, `object_begin`, `object_chunk` (at most 16 KiB),
//!   `object_end`, `ack` and `error`. Their canonical CBOR codec and the WebRTC/rendezvous layer are the `local_delivery`
//!   scope (ADR-0158, V-024) and are **not** here; a host adapts any ordered reliable byte channel to [`DirectFrame`].
//! * The recipient asks by BlobID and the digest the message committed to, with the read capability from the reference.
//!   It checks the declared length against the committed `plaintext_size`, accepts only contiguous chunks, never
//!   allocates the declared total up front, verifies the transport SHA-256 and then `ciphertext_digest` before any
//!   plaintext exists ([`AttachmentRef::open`]).
//! * Resume: a transfer is transport-local state. After a drop the recipient asks again; the sender opens a fresh transfer
//!   id and the recipient's first `ack` after `object_begin` names the contiguous bytes it already holds, from which the
//!   sender continues. The final checks cover the whole object, so a wrong prefix cannot pass.
//! * A transport ack or a completed transfer is a *local receipt*, never storage durability ([`Durability`]).
//! * The peer's final `ack` is only a claim (ADR-0194): the sender reports `PeerClaimsReceived` and keeps the object
//!   until an authenticated `object_receipt` (a MAC under a key only a holder of the message's content key can derive,
//!   bound to the transfer id, the BlobID, the length, the digest and the whole object) arrives and gives `Confirmed`.
//!   Without the receipt there is no "delivered" state.

use crate::attach::{self, AttachError, AttachmentRef, HARD_MAX_PLAINTEXT, TAG_LEN};
use crate::crypto::direct_receipt::receipt_tag;
use crate::crypto::sha256;
use crate::host::BlobLimits;

/// Largest payload of one `object_chunk` (ADR-0158 section 1).
pub const MAX_CHUNK: usize = 16_384;
/// Sender backpressure high-water: stop queueing at or above this many buffered bytes.
pub const HIGH_WATER: usize = 262_144;
/// Resume queueing at or below this many buffered bytes.
pub const LOW_WATER: usize = 65_536;
pub const TRANSFER_ID_LEN: usize = 16;
/// `object_kind` of encrypted blob ciphertext on the wire (decision recorded in the 2026-11-13 report).
pub const OBJECT_KIND_BLOB: u8 = 8;

/// Bounded profile error codes (no free text from the peer). The frozen CDDL gives only the range 1..65535.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ErrorCode {
    /// Unknown object, wrong capability, wrong digest, or not offered any more: one code, so the answer is no oracle.
    Unavailable = 1,
    /// The peer broke the framing (offset gap, oversize chunk, wrong id, wrong total).
    Protocol = 2,
    /// The bytes did not match the declared or committed digest.
    Integrity = 3,
}

/// One transport record. Ids are 16 bytes; the read capability is 16..64 bytes (as in the reference).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectFrame {
    FetchBlob {
        request_id: [u8; TRANSFER_ID_LEN],
        read_cap: Vec<u8>,
        blob_id: [u8; 32],
        /// The `ciphertext_digest` the message committed to.
        digest: [u8; 32],
    },
    Begin {
        transfer_id: [u8; TRANSFER_ID_LEN],
        total_bytes: u64,
        /// Plain SHA-256 of the exact transported bytes (transport integrity; not the DS-HASH `ciphertext_digest`).
        sha256: [u8; 32],
    },
    Chunk {
        transfer_id: [u8; TRANSFER_ID_LEN],
        offset: u64,
        bytes: Vec<u8>,
    },
    End {
        transfer_id: [u8; TRANSFER_ID_LEN],
    },
    Ack {
        transfer_id: [u8; TRANSFER_ID_LEN],
        /// Highest contiguous byte offset the receiver holds.
        offset: u64,
    },
    Error {
        id: [u8; TRANSFER_ID_LEN],
        code: ErrorCode,
    },
    /// ADR-0194 (`object_receipt`, frame kind 8): the receiver's authenticated confirmation of the transfer `transfer_id`.
    Receipt {
        transfer_id: [u8; TRANSFER_ID_LEN],
        tag: [u8; 32],
    },
}

/// What a completed direct transfer proves. Never more than this (ADR-0150: do not imply another device or an offline
/// recipient can retrieve the file until normal replication or repair makes it true).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// Authenticated bytes are in this device's local encrypted blob cache only (the receiver's label).
    LocalReceiptOnly,
    /// Sender: the peer says it has the file; nothing was checked (ADR-0194). That device only.
    PeerClaim,
    /// Sender: a device holding the message's key proved it has every byte. That device only, nothing kept elsewhere.
    PeerLocalCopyConfirmed,
}

// ---------------------------------------------------------------------------------------------------------------------
// Sender policy: refuse before sending, say so, offer direct.

/// How a file of a given size may travel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SizePlan {
    /// Fits the advertised spool bound: the ordinary BlobStore path.
    Spool,
    /// Too big for the spool and so **not sent that way**; it can still go direct while the other person is online.
    /// `spool_max` is the largest plaintext the spool takes (0 when no store is advertised). The UI says so in plain words
    /// and offers "send when they are online"; Core sent nothing.
    OfferDirect { spool_max: u64 },
    /// Cannot travel at all (empty, or beyond what a V1 Core seals in one piece): `max` is that bound.
    Refused(AttachError),
}

/// Decide the route of a plaintext of `plaintext_len` bytes. Pure; sends nothing.
pub fn plan_size(limits: Option<BlobLimits>, plaintext_len: u64) -> SizePlan {
    if plaintext_len == 0 {
        return SizePlan::Refused(AttachError::Empty);
    }
    if plaintext_len > HARD_MAX_PLAINTEXT {
        return SizePlan::Refused(AttachError::TooLarge {
            max: HARD_MAX_PLAINTEXT,
        });
    }
    let spool_max = limits
        .map(|l| HARD_MAX_PLAINTEXT.min(l.max_blob_bytes.saturating_sub(TAG_LEN)))
        .unwrap_or(0);
    if plaintext_len <= spool_max {
        SizePlan::Spool
    } else {
        SizePlan::OfferDirect { spool_max }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Sender side (serves the sealed object).

/// Where the sender's offer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferState {
    /// Prepared and offered; nobody has asked yet (the recipient may be offline).
    Waiting,
    /// A transfer is open.
    Transferring,
    /// The peer acknowledged the last byte after `object_end`. A claim only: the object is kept until a valid receipt
    /// arrives, and nothing may call this delivered.
    PeerClaimsReceived,
    /// A valid receipt for the claimed transfer arrived (ADR-0194 section 2); the ciphertext is dropped.
    Confirmed,
    /// The person withdrew it, or it expired; the ciphertext is gone.
    Withdrawn,
}

#[derive(Debug)]
struct Xfer {
    id: [u8; TRANSFER_ID_LEN],
    /// None until the receiver's first ack after `object_begin` says where to start.
    next: Option<u64>,
    acked: u64,
    ended: bool,
    paused: bool,
}

/// Serves one sealed object to whoever presents its BlobID, digest and read capability.
pub struct DirectSender {
    blob_id: [u8; 32],
    digest: [u8; 32],
    read_cap: Vec<u8>,
    /// The content key of the reference: the receipt tag is checked under a key derived from it.
    key: [u8; 32],
    ciphertext: Vec<u8>,
    state: OfferState,
    xfer: Option<Xfer>,
    channel: Option<u64>,
    /// The transfer whose final ack made the claim; a receipt must name it.
    claim_id: Option<[u8; TRANSFER_ID_LEN]>,
    /// Invalid tags on the current transfer; a fresh transfer has a bounded recovery window.
    bad_receipts: u8,
    receipt_id: Option<[u8; TRANSFER_ID_LEN]>,
    /// Whole-object hashes across the offer, never reset by fetch. One cached tag serves one transfer's checks.
    receipt_hashes: u8,
    expected_receipt: Option<zeroize::Zeroizing<[u8; 32]>>,
    /// The hash of the whole object announced in `object_begin`, computed once.
    begin_sha: Option<[u8; 32]>,
}

impl Drop for DirectSender {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.key);
    }
}

/// Invalid tags checked per transfer; whole-object work is separately bounded for the entire offer.
pub const MAX_BAD_RECEIPTS: u8 = 8;
/// Maximum whole-object receipt hashes over one offer, including all fresh-transfer recovery attempts.
pub const MAX_RECEIPT_HASHES: u8 = 8;

impl DirectSender {
    /// `ciphertext` is the sealed object; `digest` must be its `ciphertext_digest` (checked), `read_cap` the capability the
    /// message reference carries (16..64 bytes), `key` its content key (it authenticates the peer's receipt).
    pub fn new(
        blob_id: [u8; 32],
        digest: [u8; 32],
        read_cap: Vec<u8>,
        ciphertext: Vec<u8>,
        key: [u8; 32],
    ) -> Result<Self, AttachError> {
        if ciphertext.len() as u64 <= TAG_LEN {
            return Err(AttachError::Empty);
        }
        if ciphertext.len() as u64 > HARD_MAX_PLAINTEXT + TAG_LEN {
            return Err(AttachError::TooLarge {
                max: HARD_MAX_PLAINTEXT,
            });
        }
        if !(16..=64).contains(&read_cap.len()) {
            return Err(AttachError::Invalid);
        }
        if attach::ciphertext_digest(&ciphertext) != digest {
            return Err(AttachError::IntegrityFailure);
        }
        Ok(DirectSender {
            blob_id,
            digest,
            read_cap,
            key,
            ciphertext,
            state: OfferState::Waiting,
            xfer: None,
            channel: None,
            claim_id: None,
            bad_receipts: 0,
            receipt_id: None,
            receipt_hashes: 0,
            expected_receipt: None,
            begin_sha: None,
        })
    }

    pub fn state(&self) -> OfferState {
        self.state
    }

    pub fn durability(&self) -> Durability {
        if self.state == OfferState::Confirmed {
            Durability::PeerLocalCopyConfirmed
        } else {
            Durability::PeerClaim
        }
    }

    /// Close the open transfer. An offer that only claimed receipt keeps the claim; one still moving goes back to waiting.
    fn close_xfer(&mut self) {
        self.xfer = None;
        if self.state == OfferState::Transferring {
            self.state = OfferState::Waiting;
        }
    }

    pub(crate) fn owns_channel(&self, channel: u64) -> bool {
        self.channel == Some(channel)
    }

    pub(crate) fn detach_channel(&mut self, channel: u64) {
        if self.owns_channel(channel) {
            self.close_xfer();
            self.channel = None;
            self.claim_id = None;
            self.expected_receipt = None;
        }
    }

    pub(crate) fn handle_on(
        &mut self,
        channel: u64,
        frame: &DirectFrame,
        fresh_id: [u8; TRANSFER_ID_LEN],
    ) -> Vec<DirectFrame> {
        let fetch = matches!(frame, DirectFrame::FetchBlob { .. });
        if !fetch && !self.owns_channel(channel) {
            return vec![];
        }
        let out = self.handle(frame, fresh_id);
        if fetch && matches!(out.first(), Some(DirectFrame::Begin { .. })) {
            self.channel = Some(channel);
        }
        out
    }

    /// Withdraw the offer and drop the ciphertext. Later fetches answer `unavailable`.
    pub fn withdraw(&mut self) {
        self.expected_receipt = None;
        self.state = OfferState::Withdrawn;
        self.xfer = None;
        self.claim_id = None;
        self.ciphertext = Vec::new();
    }

    /// Handle one frame from the recipient; `fresh_id` is a new random transfer id the caller supplies. Returns the
    /// frames to send back (after `ack` use [`pump`](Self::pump) for chunks).
    pub fn handle(
        &mut self,
        frame: &DirectFrame,
        fresh_id: [u8; TRANSFER_ID_LEN],
    ) -> Vec<DirectFrame> {
        match frame {
            DirectFrame::FetchBlob {
                request_id,
                read_cap,
                blob_id,
                digest,
            } => {
                let live = matches!(
                    self.state,
                    OfferState::Waiting | OfferState::Transferring | OfferState::PeerClaimsReceived
                );
                if !live
                    || !ct_eq(read_cap, &self.read_cap)
                    || !ct_eq(blob_id, &self.blob_id)
                    || !ct_eq(digest, &self.digest)
                {
                    return vec![DirectFrame::Error {
                        id: *request_id,
                        code: ErrorCode::Unavailable,
                    }];
                }
                if self.receipt_id != Some(fresh_id) {
                    self.receipt_id = Some(fresh_id);
                    self.bad_receipts = 0;
                    self.expected_receipt = None;
                }
                self.claim_id = None;
                // A new request supersedes an open transfer (the old channel is gone: resume). A standing claim stays
                // a claim: the exchange runs again so the receiver can confirm it.
                if self.state != OfferState::PeerClaimsReceived {
                    self.state = OfferState::Transferring;
                }
                self.xfer = Some(Xfer {
                    id: fresh_id,
                    next: None,
                    acked: 0,
                    ended: false,
                    paused: false,
                });
                let ciphertext = &self.ciphertext;
                let sha = *self.begin_sha.get_or_insert_with(|| sha256(&[ciphertext]));
                vec![DirectFrame::Begin {
                    transfer_id: fresh_id,
                    total_bytes: self.ciphertext.len() as u64,
                    sha256: sha,
                }]
            }
            DirectFrame::Ack {
                transfer_id,
                offset,
            } => {
                let total = self.ciphertext.len() as u64;
                let Some(x) = self.xfer.as_mut().filter(|x| &x.id == transfer_id) else {
                    return vec![];
                };
                if *offset > total || *offset < x.acked && x.next.is_some() {
                    // A receiver cannot hold more than exists, nor fewer than it already acknowledged.
                    self.close_xfer();
                    return vec![DirectFrame::Error {
                        id: *transfer_id,
                        code: ErrorCode::Protocol,
                    }];
                }
                x.acked = *offset;
                if x.next.is_none() {
                    x.next = Some(*offset);
                }
                if x.ended && *offset == total {
                    // Only a claim: the peer chose this number. The object stays until a receipt proves it.
                    self.state = OfferState::PeerClaimsReceived;
                    self.claim_id = Some(*transfer_id);
                    self.xfer = None;
                }
                vec![]
            }
            DirectFrame::Error { id, .. } => {
                // The receiver rejected the object or the framing: close the transfer, keep the offer.
                if self.xfer.as_ref().is_some_and(|x| &x.id == id) {
                    self.close_xfer();
                }
                vec![]
            }
            DirectFrame::Receipt { transfer_id, tag } => {
                // Anything that does not verify is ignored: the transfer is not failed and the claim stands.
                if self.state != OfferState::PeerClaimsReceived
                    || self.claim_id.as_ref() != Some(transfer_id)
                    || self.bad_receipts >= MAX_BAD_RECEIPTS
                {
                    return vec![];
                }
                if self.expected_receipt.is_none() {
                    if self.receipt_hashes >= MAX_RECEIPT_HASHES {
                        return vec![];
                    }
                    self.receipt_hashes += 1;
                    self.expected_receipt = Some(zeroize::Zeroizing::new(receipt_tag(
                        &self.key,
                        &self.digest,
                        &self.blob_id,
                        transfer_id,
                        &self.ciphertext,
                    )));
                }
                if self
                    .expected_receipt
                    .as_ref()
                    .is_some_and(|want| ct_eq(want.as_ref(), tag))
                {
                    self.expected_receipt = None;
                    self.state = OfferState::Confirmed;
                    self.xfer = None;
                    self.claim_id = None;
                    self.ciphertext = Vec::new();
                } else {
                    self.bad_receipts += 1;
                }
                vec![]
            }
            _ => vec![DirectFrame::Error {
                id: fresh_id,
                code: ErrorCode::Protocol,
            }],
        }
    }

    /// Next chunks (then `object_end`), given the channel's currently buffered bytes. Stops once the buffer reaches
    /// [`HIGH_WATER`] and does not start again until it is at or below [`LOW_WATER`] (ADR-0158 section 6).
    pub fn pump(&mut self, buffered: usize) -> Vec<DirectFrame> {
        let total = self.ciphertext.len() as u64;
        let Some(x) = self.xfer.as_mut() else {
            return vec![];
        };
        let Some(mut at) = x.next else {
            return vec![]; // waiting for the receiver's first ack
        };
        if x.paused {
            if buffered > LOW_WATER {
                return vec![];
            }
            x.paused = false;
        }
        let mut queued = buffered;
        let mut out = vec![];
        while at < total {
            if queued >= HIGH_WATER {
                x.paused = true;
                break;
            }
            let n = (total - at).min(MAX_CHUNK as u64) as usize;
            out.push(DirectFrame::Chunk {
                transfer_id: x.id,
                offset: at,
                bytes: self.ciphertext[at as usize..at as usize + n].to_vec(),
            });
            at += n as u64;
            queued += n;
        }
        x.next = Some(at);
        if at == total && !x.ended {
            x.ended = true;
            out.push(DirectFrame::End { transfer_id: x.id });
        }
        out
    }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

// ---------------------------------------------------------------------------------------------------------------------
// Recipient side (fetches by BlobID and digest).

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchState {
    Idle,
    Requested,
    Receiving,
    /// Every byte arrived and both digests held; call [`DirectFetch::open`]. Not yet durable: no receipt is sent.
    Complete,
    /// The host confirmed the ciphertext is durably in the local encrypted blob cache; the receipt may be sent.
    Committed,
    Failed(AttachError),
}

/// Pulls one attachment's ciphertext directly. Constructed from the reference the message carried.
pub struct DirectFetch {
    r: AttachmentRef,
    expected_len: u64,
    request_id: [u8; TRANSFER_ID_LEN],
    state: FetchState,
    transfer: Option<([u8; TRANSFER_ID_LEN], [u8; 32])>,
    buf: Vec<u8>,
    /// A verified object asked for again (the sender may not have a receipt yet).
    again: bool,
    /// The transfer whose `object_end` verified; the receipt names it.
    ended_id: Option<[u8; TRANSFER_ID_LEN]>,
    /// The transfer a receipt was last produced for.
    receipt_sent: Option<[u8; TRANSFER_ID_LEN]>,
    /// False once the sender answered a receipt with an error (an older build): stop sending them, keep the object.
    receipts_ok: bool,
}

impl DirectFetch {
    /// Fails closed when the reference has no read capability or declares more than a V1 Core opens.
    pub fn new(r: &AttachmentRef, request_id: [u8; TRANSFER_ID_LEN]) -> Result<Self, AttachError> {
        if r.read_cap.is_none() {
            return Err(AttachError::NotFound);
        }
        if r.plaintext_size == 0 {
            return Err(AttachError::Empty);
        }
        if r.plaintext_size > HARD_MAX_PLAINTEXT {
            return Err(AttachError::TooLarge {
                max: HARD_MAX_PLAINTEXT,
            });
        }
        Ok(DirectFetch {
            expected_len: r.plaintext_size + TAG_LEN,
            r: r.clone(),
            request_id,
            state: FetchState::Idle,
            transfer: None,
            buf: Vec::new(),
            again: false,
            ended_id: None,
            receipt_sent: None,
            receipts_ok: true,
        })
    }

    pub fn state(&self) -> FetchState {
        self.state.clone()
    }

    /// Contiguous bytes held (the resume point).
    pub fn held(&self) -> u64 {
        self.buf.len() as u64
    }

    pub fn durability(&self) -> Durability {
        Durability::LocalReceiptOnly
    }

    /// The request to send on a fresh channel (also after a drop: the held bytes are kept and resumed).
    pub fn request(&mut self, request_id: [u8; TRANSFER_ID_LEN]) -> Option<DirectFrame> {
        if matches!(self.state, FetchState::Failed(_)) {
            return None;
        }
        self.request_id = request_id;
        self.transfer = None;
        if self.verified() {
            // Already holds the whole verified object (for example after a lost receipt): ask again so the sender can
            // be told, and answer its transfer from the held bytes.
            self.again = true;
        } else {
            self.state = FetchState::Requested;
        }
        Some(DirectFrame::FetchBlob {
            request_id,
            read_cap: self.r.read_cap.clone()?,
            blob_id: self.r.blob_id,
            digest: self.r.ciphertext_digest,
        })
    }

    fn verified(&self) -> bool {
        matches!(self.state, FetchState::Complete | FetchState::Committed)
    }

    /// The host reports that the verified ciphertext is durably in the local encrypted blob cache. Only then may a
    /// receipt exist; a failed or unreported commit sends none. Returns the receipt for the transfer already ended, if any.
    pub fn committed(&mut self) -> Vec<DirectFrame> {
        if self.state == FetchState::Complete {
            self.state = FetchState::Committed;
        }
        match self.ended_id {
            Some(id) => self.take_receipt(&id).into_iter().collect(),
            None => vec![],
        }
    }

    /// The receipt for `transfer_id`: only after the commit, only for a transfer whose `object_end` verified, and not
    /// once the sender refused receipts.
    pub fn take_receipt(&mut self, transfer_id: &[u8; TRANSFER_ID_LEN]) -> Option<DirectFrame> {
        if self.state != FetchState::Committed
            || !self.receipts_ok
            || self.ended_id.as_ref() != Some(transfer_id)
        {
            return None;
        }
        self.receipt_sent = Some(*transfer_id);
        Some(DirectFrame::Receipt {
            transfer_id: *transfer_id,
            tag: receipt_tag(
                &self.r.key,
                &self.r.ciphertext_digest,
                &self.r.blob_id,
                transfer_id,
                &self.buf,
            ),
        })
    }

    fn fail(
        &mut self,
        e: AttachError,
        id: [u8; TRANSFER_ID_LEN],
        code: ErrorCode,
    ) -> Vec<DirectFrame> {
        self.buf = Vec::new();
        self.transfer = None;
        self.state = FetchState::Failed(e);
        vec![DirectFrame::Error { id, code }]
    }

    /// Handle one frame from the sender; returns the frames to answer with.
    pub fn handle(&mut self, frame: &DirectFrame) -> Vec<DirectFrame> {
        if matches!(self.state, FetchState::Failed(_)) {
            return vec![];
        }
        if self.verified() {
            return self.handle_verified(frame);
        }
        match frame {
            DirectFrame::Begin {
                transfer_id,
                total_bytes,
                sha256,
            } => {
                if self.state != FetchState::Requested {
                    return self.fail(
                        AttachError::IntegrityFailure,
                        *transfer_id,
                        ErrorCode::Protocol,
                    );
                }
                // Policy limit and the committed length are enforced before anything is accepted or allocated.
                if *total_bytes != self.expected_len {
                    return self.fail(
                        AttachError::IntegrityFailure,
                        *transfer_id,
                        ErrorCode::Protocol,
                    );
                }
                self.transfer = Some((*transfer_id, *sha256));
                self.state = FetchState::Receiving;
                vec![DirectFrame::Ack {
                    transfer_id: *transfer_id,
                    offset: self.held(),
                }]
            }
            DirectFrame::Chunk {
                transfer_id,
                offset,
                bytes,
            } => {
                let Some((id, _)) = self.transfer.filter(|(id, _)| id == transfer_id) else {
                    return self.fail(
                        AttachError::IntegrityFailure,
                        *transfer_id,
                        ErrorCode::Protocol,
                    );
                };
                if bytes.is_empty()
                    || bytes.len() > MAX_CHUNK
                    || *offset != self.held()
                    || self.held() + bytes.len() as u64 > self.expected_len
                {
                    return self.fail(AttachError::IntegrityFailure, id, ErrorCode::Protocol);
                }
                self.buf.extend_from_slice(bytes);
                vec![DirectFrame::Ack {
                    transfer_id: id,
                    offset: self.held(),
                }]
            }
            DirectFrame::End { transfer_id } => {
                let Some((id, wire_sha)) = self.transfer.filter(|(id, _)| id == transfer_id) else {
                    return self.fail(
                        AttachError::IntegrityFailure,
                        *transfer_id,
                        ErrorCode::Protocol,
                    );
                };
                if self.held() != self.expected_len {
                    return self.fail(AttachError::IntegrityFailure, id, ErrorCode::Protocol);
                }
                // Transport digest first, then the digest the message committed to; neither releases plaintext.
                if sha256(&[&self.buf]) != wire_sha
                    || attach::ciphertext_digest(&self.buf) != self.r.ciphertext_digest
                {
                    return self.fail(AttachError::IntegrityFailure, id, ErrorCode::Integrity);
                }
                self.state = FetchState::Complete;
                self.ended_id = Some(id);
                vec![DirectFrame::Ack {
                    transfer_id: id,
                    offset: self.held(),
                }]
            }
            DirectFrame::Error { id, .. } => {
                // The sender refused (or lost the offer). Keep what we hold; the person may try again later.
                let _ = id;
                self.transfer = None;
                self.state = FetchState::Idle;
                vec![]
            }
            _ => self.fail(
                AttachError::IntegrityFailure,
                self.request_id,
                ErrorCode::Protocol,
            ),
        }
    }

    /// A fetch that already holds the verified object. Never fails it and never discards it: a refused or odd frame is
    /// answered and ignored. A new transfer is answered with an ack of the whole length so the sender sends no bytes.
    fn handle_verified(&mut self, frame: &DirectFrame) -> Vec<DirectFrame> {
        let total = self.buf.len() as u64;
        match frame {
            DirectFrame::Begin {
                transfer_id,
                total_bytes,
                sha256: wire,
            } => {
                if !self.again || *total_bytes != total || sha256(&[&self.buf]) != *wire {
                    return vec![DirectFrame::Error {
                        id: *transfer_id,
                        code: ErrorCode::Protocol,
                    }];
                }
                self.transfer = Some((*transfer_id, *wire));
                vec![DirectFrame::Ack {
                    transfer_id: *transfer_id,
                    offset: total,
                }]
            }
            DirectFrame::End { transfer_id } => {
                if self.transfer.map(|(id, _)| id).as_ref() != Some(transfer_id) {
                    return vec![DirectFrame::Error {
                        id: *transfer_id,
                        code: ErrorCode::Protocol,
                    }];
                }
                self.ended_id = Some(*transfer_id);
                let mut out = vec![DirectFrame::Ack {
                    transfer_id: *transfer_id,
                    offset: total,
                }];
                out.extend(self.take_receipt(transfer_id));
                out
            }
            DirectFrame::Error { .. } => {
                // An error after a receipt went out is the sender saying it does not know receipts (an older build
                // answers an unknown frame with an error of its own id): stop sending them, keep the object.
                if self.receipt_sent.is_some() {
                    self.receipts_ok = false;
                }
                self.transfer = None;
                self.again = false;
                vec![]
            }
            DirectFrame::Chunk { transfer_id, .. } => vec![DirectFrame::Error {
                id: *transfer_id,
                code: ErrorCode::Protocol,
            }],
            _ => vec![DirectFrame::Error {
                id: self.request_id,
                code: ErrorCode::Protocol,
            }],
        }
    }

    /// The verified ciphertext (to cache locally under the BlobID). `None` until complete.
    pub fn ciphertext(&self) -> Option<&[u8]> {
        self.verified().then_some(self.buf.as_slice())
    }

    /// Decrypt a completed transfer. `AttachmentRef::open` verifies length and digest again before it decrypts.
    pub fn open(&self) -> Result<Vec<u8>, AttachError> {
        match &self.state {
            FetchState::Complete | FetchState::Committed => self.r.open(&self.buf),
            FetchState::Failed(e) => Err(e.clone()),
            _ => Err(AttachError::NotFound),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The `om-lan-v1` wire form (ADR-0158 section 6): every blob frame has exactly one `LanFrameV1` counterpart.

impl DirectFrame {
    /// The `LanFrameV1` of this frame. A transfer begins with the blob object kind.
    pub fn to_lan(&self) -> crate::lan_frame::Frame {
        use crate::lan_frame::Frame as L;
        match self {
            DirectFrame::FetchBlob {
                request_id,
                read_cap,
                blob_id,
                digest,
            } => L::FetchBlob {
                request_id: *request_id,
                capability: read_cap.clone(),
                blob_id: *blob_id,
                ciphertext_digest: *digest,
            },
            DirectFrame::Begin {
                transfer_id,
                total_bytes,
                sha256,
            } => L::Begin {
                transfer_id: *transfer_id,
                object_kind: u64::from(OBJECT_KIND_BLOB),
                total_bytes: *total_bytes,
                sha256: *sha256,
            },
            DirectFrame::Chunk {
                transfer_id,
                offset,
                bytes,
            } => L::Chunk {
                transfer_id: *transfer_id,
                offset: *offset,
                bytes: bytes.clone(),
            },
            DirectFrame::End { transfer_id } => L::End {
                transfer_id: *transfer_id,
            },
            DirectFrame::Ack {
                transfer_id,
                offset,
            } => L::Ack {
                transfer_id: *transfer_id,
                offset: *offset,
            },
            DirectFrame::Error { id, code } => L::Error {
                id: *id,
                code: *code as u64,
            },
            DirectFrame::Receipt { transfer_id, tag } => L::Receipt {
                transfer_id: *transfer_id,
                tag: *tag,
            },
        }
    }

    /// The blob frame for a `LanFrameV1`, or `None` for anything that is not part of a blob transfer: an async-key fetch, a
    /// transfer of another object kind, or an error code this profile does not define (unknown values are rejected, not
    /// guessed).
    pub fn from_lan(f: &crate::lan_frame::Frame) -> Option<Self> {
        use crate::lan_frame::Frame as L;
        Some(match f {
            L::FetchBlob {
                request_id,
                capability,
                blob_id,
                ciphertext_digest,
            } => DirectFrame::FetchBlob {
                request_id: *request_id,
                read_cap: capability.clone(),
                blob_id: *blob_id,
                digest: *ciphertext_digest,
            },
            L::Begin {
                transfer_id,
                object_kind,
                total_bytes,
                sha256,
            } if *object_kind == u64::from(OBJECT_KIND_BLOB) => DirectFrame::Begin {
                transfer_id: *transfer_id,
                total_bytes: *total_bytes,
                sha256: *sha256,
            },
            L::Chunk {
                transfer_id,
                offset,
                bytes,
            } => DirectFrame::Chunk {
                transfer_id: *transfer_id,
                offset: *offset,
                bytes: bytes.clone(),
            },
            L::End { transfer_id } => DirectFrame::End {
                transfer_id: *transfer_id,
            },
            L::Ack {
                transfer_id,
                offset,
            } => DirectFrame::Ack {
                transfer_id: *transfer_id,
                offset: *offset,
            },
            L::Error { id, code } => DirectFrame::Error {
                id: *id,
                code: match code {
                    1 => ErrorCode::Unavailable,
                    2 => ErrorCode::Protocol,
                    3 => ErrorCode::Integrity,
                    _ => return None,
                },
            },
            L::Receipt { transfer_id, tag } => DirectFrame::Receipt {
                transfer_id: *transfer_id,
                tag: *tag,
            },
            L::Begin { .. } | L::FetchAsyncKey { .. } => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_blob: u64) -> Option<BlobLimits> {
        Some(BlobLimits {
            max_blob_bytes: max_blob,
            max_spool_bytes: 0,
            max_blobs: 0,
        })
    }

    #[test]
    fn size_plan_refuses_spool_and_offers_direct() {
        let four_mib = 4 * 1024 * 1024;
        assert_eq!(plan_size(limits(four_mib), 1000), SizePlan::Spool);
        assert_eq!(
            plan_size(limits(four_mib), four_mib - TAG_LEN),
            SizePlan::Spool
        );
        assert_eq!(
            plan_size(limits(four_mib), four_mib - TAG_LEN + 1),
            SizePlan::OfferDirect {
                spool_max: four_mib - TAG_LEN
            }
        );
        assert_eq!(plan_size(None, 10), SizePlan::OfferDirect { spool_max: 0 });
        assert_eq!(
            plan_size(limits(four_mib), 0),
            SizePlan::Refused(AttachError::Empty)
        );
        assert_eq!(
            plan_size(limits(four_mib), HARD_MAX_PLAINTEXT + 1),
            SizePlan::Refused(AttachError::TooLarge {
                max: HARD_MAX_PLAINTEXT
            })
        );
    }
}

#[cfg(test)]
mod receipt_work_tests {
    use super::*;
    #[test]
    fn repeated_fetches_and_bad_tags_never_reset_the_whole_object_work_bound() {
        let ciphertext = vec![7; 65536];
        let digest = attach::ciphertext_digest(&ciphertext);
        let mut s = DirectSender::new([3; 32], digest, vec![4; 32], ciphertext, [5; 32]).unwrap();
        for round in 0..1000u64 {
            let mut id = [0; 16];
            id[..8].copy_from_slice(&round.to_be_bytes());
            s.handle(
                &DirectFrame::FetchBlob {
                    request_id: id,
                    read_cap: vec![4; 32],
                    blob_id: [3; 32],
                    digest,
                },
                id,
            );
            let ack = DirectFrame::Ack {
                transfer_id: id,
                offset: 65536,
            };
            s.handle(&ack, id);
            s.pump(0);
            s.handle(&ack, id);
            for _ in 0..100 {
                s.handle(
                    &DirectFrame::Receipt {
                        transfer_id: id,
                        tag: [0; 32],
                    },
                    id,
                );
            }
            assert!(s.receipt_hashes <= MAX_RECEIPT_HASHES);
            assert!(s.bad_receipts <= MAX_BAD_RECEIPTS);
            assert_eq!(s.state(), OfferState::PeerClaimsReceived);
        }
        assert_eq!(s.receipt_hashes, MAX_RECEIPT_HASHES);
    }

    #[test]
    fn every_blob_frame_round_trips_through_the_lan_wire_form() {
        let id = [7u8; TRANSFER_ID_LEN];
        let frames = [
            DirectFrame::FetchBlob {
                request_id: id,
                read_cap: vec![1; 32],
                blob_id: [2; 32],
                digest: [3; 32],
            },
            DirectFrame::Begin {
                transfer_id: id,
                total_bytes: 40_000,
                sha256: [4; 32],
            },
            DirectFrame::Chunk {
                transfer_id: id,
                offset: 16_384,
                bytes: vec![9; 100],
            },
            DirectFrame::End { transfer_id: id },
            DirectFrame::Ack {
                transfer_id: id,
                offset: 100,
            },
            DirectFrame::Error {
                id,
                code: ErrorCode::Integrity,
            },
            DirectFrame::Receipt {
                transfer_id: id,
                tag: [5; 32],
            },
        ];
        for f in frames {
            let bytes = crate::lan_frame::encode_frame(&f.to_lan()).expect("encodes");
            let back = crate::lan_frame::decode(&bytes).expect("decodes");
            assert_eq!(DirectFrame::from_lan(&back), Some(f));
        }
        // Not a blob transfer: another object kind, an async-key fetch, an unknown error code.
        use crate::lan_frame::Frame as L;
        assert_eq!(
            DirectFrame::from_lan(&L::Begin {
                transfer_id: id,
                object_kind: 1,
                total_bytes: 1,
                sha256: [0; 32]
            }),
            None
        );
        assert_eq!(
            DirectFrame::from_lan(&L::FetchAsyncKey {
                request_id: id,
                device_id: [1; 16],
                profile: 1,
                capability: vec![0; 32]
            }),
            None
        );
        assert_eq!(DirectFrame::from_lan(&L::Error { id, code: 99 }), None);
    }
}
