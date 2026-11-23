// SPDX-License-Identifier: MPL-2.0
//! Encrypted blob transfer over the LAN channel (`local_delivery` slice 6; ADR-0158, LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md).
//! Sans-IO sender and receiver state. The blob is already encrypted; the transfer carries the same BlobID and
//! `ciphertext_digest`, so a later WAN backfill rewrites nothing.
//!
//! * Chunks are at most 16 KiB. The sender stops queueing at 256 KiB in flight and resumes below 64 KiB.
//! * A transport ACK only moves the in-flight window; it is not authority. The transfer is confirmed solely by an
//!   authenticated receipt that names the same BlobID and digest and the full length.
//! * Resume starts from the offset the receiver durably acknowledged, never from the sender's guess, and never past it.

pub const MAX_CHUNK: usize = 16 * 1024;
pub const HIGH_WATER: usize = 256 * 1024;
pub const LOW_WATER: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    /// ACK beyond what was sent, or backwards.
    BadAck,
    /// The receipt names another blob or digest, or a different length.
    ReceiptMismatch,
    /// The receipt was not authenticated.
    ReceiptNotAuthenticated,
    /// Chunk at an offset the receiver did not expect, or oversize.
    BadChunk,
    /// More data than the declared length.
    Overrun,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    pub offset: u64,
    pub len: usize,
}

pub struct Sender {
    blob_id: [u8; 32],
    digest: [u8; 32],
    total: u64,
    /// Next byte not yet handed to the channel.
    sent: u64,
    /// Highest byte the transport reported delivered (non-authoritative).
    acked: u64,
    confirmed: bool,
}

impl Sender {
    pub fn new(blob_id: [u8; 32], digest: [u8; 32], total: u64) -> Self {
        Self {
            blob_id,
            digest,
            total,
            sent: 0,
            acked: 0,
            confirmed: false,
        }
    }

    /// Resumes from the receiver's durably acknowledged offset. It can only restart at or before what was sent.
    pub fn resume_from(&mut self, receiver_offset: u64) -> Result<(), TransferError> {
        if receiver_offset > self.total {
            return Err(TransferError::BadAck);
        }
        self.sent = receiver_offset;
        self.acked = receiver_offset;
        Ok(())
    }

    pub fn in_flight(&self) -> usize {
        (self.sent - self.acked) as usize
    }

    /// Next chunk to send, or `None` while backpressured or finished. Pass `paused` as the previous result of the
    /// hysteresis: stop at the high mark, resume under the low mark.
    pub fn next_chunk(&mut self, paused: &mut bool) -> Option<Chunk> {
        if self.sent >= self.total || self.confirmed {
            return None;
        }
        let f = self.in_flight();
        if *paused {
            if f >= LOW_WATER {
                return None;
            }
            *paused = false;
        }
        if f >= HIGH_WATER {
            *paused = true;
            return None;
        }
        let len = MAX_CHUNK.min((self.total - self.sent) as usize);
        let c = Chunk {
            offset: self.sent,
            len,
        };
        self.sent += len as u64;
        Some(c)
    }

    /// A transport ACK. Moves the window only; never confirms.
    pub fn transport_ack(&mut self, up_to: u64) -> Result<(), TransferError> {
        if up_to > self.sent || up_to < self.acked {
            return Err(TransferError::BadAck);
        }
        self.acked = up_to;
        Ok(())
    }

    /// The authenticated receipt: the only thing that confirms delivery.
    pub fn receipt(
        &mut self,
        authenticated: bool,
        blob_id: &[u8; 32],
        digest: &[u8; 32],
        length: u64,
    ) -> Result<(), TransferError> {
        if !authenticated {
            return Err(TransferError::ReceiptNotAuthenticated);
        }
        if *blob_id != self.blob_id || *digest != self.digest || length != self.total {
            return Err(TransferError::ReceiptMismatch);
        }
        self.confirmed = true;
        Ok(())
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }
}

pub struct Receiver {
    total: u64,
    /// Durably stored contiguous prefix.
    offset: u64,
}

impl Receiver {
    pub fn new(total: u64) -> Self {
        Self { total, offset: 0 }
    }
    /// Restores a persisted offset after a restart.
    pub fn restore(total: u64, offset: u64) -> Self {
        Self {
            total,
            offset: offset.min(total),
        }
    }
    /// The offset to tell a resuming sender.
    pub fn acknowledged(&self) -> u64 {
        self.offset
    }
    /// Accepts the next contiguous chunk only.
    pub fn accept(&mut self, c: &Chunk) -> Result<(), TransferError> {
        if c.len == 0 || c.len > MAX_CHUNK || c.offset != self.offset {
            return Err(TransferError::BadChunk);
        }
        if self.offset + c.len as u64 > self.total {
            return Err(TransferError::Overrun);
        }
        self.offset += c.len as u64;
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.offset == self.total
    }
}
