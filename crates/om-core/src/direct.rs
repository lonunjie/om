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

use crate::attach::{self, AttachError, AttachmentRef, HARD_MAX_PLAINTEXT, TAG_LEN};
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
}

/// What a completed direct transfer proves. Never more than this (ADR-0150: do not imply another device or an offline
/// recipient can retrieve the file until normal replication or repair makes it true).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// Authenticated bytes are in this device's local encrypted blob cache only.
    LocalReceiptOnly,
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
    /// The recipient acknowledged the last byte after `object_end`: local receipt only.
    Delivered,
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
    ciphertext: Vec<u8>,
    state: OfferState,
    xfer: Option<Xfer>,
}

impl DirectSender {
    /// `ciphertext` is the sealed object; `digest` must be its `ciphertext_digest` (checked), `read_cap` the capability the
    /// message reference carries (16..64 bytes).
    pub fn new(
        blob_id: [u8; 32],
        digest: [u8; 32],
        read_cap: Vec<u8>,
        ciphertext: Vec<u8>,
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
            ciphertext,
            state: OfferState::Waiting,
            xfer: None,
        })
    }

    pub fn state(&self) -> OfferState {
        self.state
    }

    pub fn durability(&self) -> Durability {
        Durability::LocalReceiptOnly
    }

    /// Withdraw the offer and drop the ciphertext. Later fetches answer `unavailable`.
    pub fn withdraw(&mut self) {
        self.state = OfferState::Withdrawn;
        self.xfer = None;
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
                let live = matches!(self.state, OfferState::Waiting | OfferState::Transferring);
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
                // A new request supersedes an open transfer (the old channel is gone: resume).
                self.state = OfferState::Transferring;
                self.xfer = Some(Xfer {
                    id: fresh_id,
                    next: None,
                    acked: 0,
                    ended: false,
                    paused: false,
                });
                vec![DirectFrame::Begin {
                    transfer_id: fresh_id,
                    total_bytes: self.ciphertext.len() as u64,
                    sha256: sha256(&[&self.ciphertext]),
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
                    self.xfer = None;
                    self.state = OfferState::Waiting;
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
                    self.state = OfferState::Delivered;
                    self.xfer = None;
                    self.ciphertext = Vec::new();
                }
                vec![]
            }
            DirectFrame::Error { id, .. } => {
                // The receiver rejected the object or the framing: close the transfer, keep the offer.
                if self.xfer.as_ref().is_some_and(|x| &x.id == id) {
                    self.xfer = None;
                    self.state = OfferState::Waiting;
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
    /// Every byte arrived and both digests held; call [`DirectFetch::open`].
    Complete,
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
        if matches!(self.state, FetchState::Complete | FetchState::Failed(_)) {
            return None;
        }
        self.request_id = request_id;
        self.transfer = None;
        self.state = FetchState::Requested;
        Some(DirectFrame::FetchBlob {
            request_id,
            read_cap: self.r.read_cap.clone()?,
            blob_id: self.r.blob_id,
            digest: self.r.ciphertext_digest,
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
        if matches!(self.state, FetchState::Complete | FetchState::Failed(_)) {
            return vec![];
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

    /// The verified ciphertext (to cache locally under the BlobID). `None` until complete.
    pub fn ciphertext(&self) -> Option<&[u8]> {
        (self.state == FetchState::Complete).then_some(self.buf.as_slice())
    }

    /// Decrypt a completed transfer. `AttachmentRef::open` verifies length and digest again before it decrypts.
    pub fn open(&self) -> Result<Vec<u8>, AttachError> {
        match &self.state {
            FetchState::Complete => self.r.open(&self.buf),
            FetchState::Failed(e) => Err(e.clone()),
            _ => Err(AttachError::NotFound),
        }
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
