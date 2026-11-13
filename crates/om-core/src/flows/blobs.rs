// SPDX-License-Identifier: MPL-2.0
//! Attachment send and receive transitions (STORAGE_AND_BLOB_ARCHITECTURE, ADR-0150, MESSENGER_BASELINE_V1).
//!
//! Send: the host asks Core to *prepare* an attachment (seal it on the device, persist its BlobID, content key and
//! capabilities), then sends a message that names prepared attachments. Each blob is uploaded by a persisted `PutBlob`
//! outbox entry; the `message.create` is built only after every receipt is in, because the event carries the receipt's
//! locations and read capability. The BlobID, the owner capability and the put's request id are fixed at preparation,
//! before the first external effect, so an exact retry (after a timeout, a restart or a `RetryDelivery`) repeats the
//! same put. A timeout after the bytes may have left is `PendingReconciliation`, never a rejection.
//!
//! Receive: the references are stored with the message. Nothing is fetched by Core on its own; the host asks for a fetch
//! plan, performs it, and `open_attachment` verifies the digest over the exact bytes before it decrypts anything.

use super::*;
use crate::attach::{self, AttachError, AttachmentRef, Meta};
use crate::direct;
use crate::host::{BlobLimits, BlobReceipt};
use crate::present::{self, Extensions, Location, Voice};
use std::collections::BTreeMap;

/// A prepared attachment nobody sent is forgotten after this long. Nothing external happened to it.
const PREPARED_TTL_MS: u64 = DAY_MS;
/// Prepared attachments held at once (bounded).
const MAX_PREPARED: usize = 2 * attach::MAX_ATTACHMENTS;
/// A direct offer lapses after this long (the spool's default retention, so both routes read the same).
const DIRECT_OFFER_TTL_MS: u64 = DEFAULT_STORE_MS;
/// Live direct offers at once (bounded).
const MAX_DIRECT_OFFERS: usize = 16;
/// A delete that makes no progress this many times is given up on; the blob expires on its own.
const DELETE_GIVE_UP: u32 = 10;
/// Used for a receipt that names no expiry: the spool's default retention (a display hint, not authority).
const DEFAULT_STORE_MS: u64 = 7 * DAY_MS;

/// What a received `message.create` carries, validated.
pub(super) struct CreateParts {
    pub text: String,
    pub attachments: Vec<AttachRec>,
    pub ext: Option<Hex>,
}

/// Read the content, attachment references and extensions of a `message.create`. `None` refuses the event: a content
/// that is not RichText, an attachment reference Core cannot use, a repeated BlobID, an empty message.
pub(super) fn read_create(ev: &events::Event) -> Option<CreateParts> {
    let content = match ev.body.opt(0) {
        None => None,
        Some(rt) => {
            Some(events::plain_text(rt).filter(|t| t.chars().count() <= MAX_MESSAGE_CHARS)?)
        }
    };
    let mut attachments: Vec<AttachRec> = vec![];
    for a in ev.body.get(1).ok()?.a().ok()? {
        let r = AttachmentRef::from_v(a)?;
        let id = hex(&r.blob_id);
        if attachments.iter().any(|x| x.blob_id == id) {
            return None;
        }
        attachments.push(AttachRec {
            blob_id: id,
            reference: hex(&cbor::encode(&r.to_v())).into(),
            owner_cap: None,
            request_id: None,
            stored: false,
            location: None,
            expires_ms: 0,
            ciphertext_len: 0,
            direct: false,
        });
    }
    if content.is_none() && attachments.is_empty() {
        return None;
    }
    Some(CreateParts {
        text: content.unwrap_or_default(),
        attachments,
        ext: ev.extensions.as_ref().map(|x| hex(&cbor::encode(x))),
    })
}

/// The decoded reference of a stored attachment.
pub(crate) fn rec_ref(rec: &AttachRec) -> Option<AttachmentRef> {
    AttachmentRef::from_v(&cbor::decode(&unhex(&rec.reference).ok()?).ok()?)
}

/// The presentation extensions of a message as they currently stand: schema-valid (re-read from the authenticated
/// bytes), bound to the immutable attachments, and for a location card to the message's current text (ADR-0155).
pub(crate) fn presentation(m: &Msg) -> Option<PresentationView> {
    let ext = unhex(m.ext.as_deref()?).ok()?;
    let parsed = present::parse_extensions(cbor::decode(&ext).ok().as_ref())?;
    let refs: Vec<AttachmentRef> = m.attachments.iter().filter_map(rec_ref).collect();
    let mut out = PresentationView::default();
    if let Some(v) = parsed.voice
        && refs.len() == m.attachments.len()
        && present::voice_binds(&v, &refs)
    {
        out.voice_note = Some(VoiceNoteView {
            attachment_index: v.attachment_index as u32,
            waveform: v.waveform,
        });
    }
    if let Some(l) = parsed.location
        && !m.deleted
        && present::fallback_names_exactly(&m.text, l.lat_e7, l.lon_e7)
    {
        out.location_card = Some(LocationCardView {
            lat_e7: l.lat_e7,
            lon_e7: l.lon_e7,
            geo: present::geo_token(l.lat_e7, l.lon_e7),
            accuracy_m: l.accuracy_m,
            label: l.label,
        });
    }
    (out != PresentationView::default()).then_some(out)
}

/// The attachments of a message as the UI sees them.
pub(crate) fn attachment_views(
    m: &Msg,
    offers: &BTreeMap<Hex, DirectOfferRec>,
) -> Vec<AttachmentView> {
    m.attachments
        .iter()
        .enumerate()
        .filter_map(|(i, rec)| {
            let r = rec_ref(rec)?;
            let outgoing = rec.owner_cap.is_some();
            Some(AttachmentView {
                index: i as u32,
                blob_id: rec.blob_id.clone(),
                media_type: r.media_type,
                size: r.plaintext_size,
                filename: r.filename,
                width: r.width,
                height: r.height,
                duration_ms: r.duration_ms,
                state: if rec.direct {
                    match offers.get(&rec.blob_id).map(|o| o.status) {
                        Some(DirectStatus::Waiting) => AttachmentState::WaitingForRecipient,
                        Some(DirectStatus::Delivered) => AttachmentState::DeliveredLocally,
                        _ => AttachmentState::NotOffered,
                    }
                } else {
                    match (outgoing, rec.stored) {
                        (true, true) => AttachmentState::StoredForLater,
                        (true, false) => AttachmentState::Uploading,
                        (false, _) if r.locations.is_empty() && r.read_cap.is_some() => {
                            AttachmentState::RemoteDirect
                        }
                        (false, _) => AttachmentState::Remote,
                    }
                },
                stored_until: (outgoing && rec.stored).then_some(rec.expires_ms),
            })
        })
        .collect()
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    // ------------------------------------------------------------------------------------------ preparation

    /// Does the store have room for `extra` more ciphertext bytes (and one more object) beyond what we already put there?
    fn blob_budget_ok(&self, limits: &BlobLimits, extra_bytes: u64, extra_count: u32) -> bool {
        let now = self.now_ms;
        let (mut bytes, mut count) = (0u64, 0u32);
        for r in self.st.app.relationships.values() {
            for m in &r.messages {
                let rejected = matches!(m.delivery, DeliveryState::Rejected { .. });
                for a in m.attachments.iter().filter(|a| a.owner_cap.is_some()) {
                    let live = if a.stored {
                        a.expires_ms > now
                    } else {
                        !rejected
                    };
                    if live {
                        bytes += a.ciphertext_len;
                        count += 1;
                    }
                }
            }
        }
        (limits.max_spool_bytes == 0 || bytes + extra_bytes <= limits.max_spool_bytes)
            && (limits.max_blobs == 0 || count + extra_count <= limits.max_blobs)
    }

    /// The typed refusal for an object of `plaintext_len` bytes, before anything is sealed or sent.
    fn check_blob_size(
        &self,
        limits: Option<BlobLimits>,
        plaintext_len: u64,
    ) -> Result<(), AttachError> {
        let Some(limits) = limits else {
            return Err(AttachError::NoBlobStore);
        };
        if plaintext_len == 0 {
            return Err(AttachError::Empty);
        }
        let max =
            attach::HARD_MAX_PLAINTEXT.min(limits.max_blob_bytes.saturating_sub(attach::TAG_LEN));
        if plaintext_len > max {
            return Err(AttachError::TooLarge { max });
        }
        Ok(())
    }

    /// Seal an attachment on this device and persist what an exact retry needs, before any external effect.
    pub fn prepare_attachment(
        &mut self,
        limits: Option<BlobLimits>,
        plaintext: &[u8],
        meta: Meta,
    ) -> Result<PreparedAttachment, AttachError> {
        self.check_blob_size(limits, plaintext.len() as u64)?;
        let limits = limits.ok_or(AttachError::NoBlobStore)?;
        if !attach::valid_media_type(&meta.media_type)
            || [meta.width, meta.height]
                .iter()
                .flatten()
                .any(|d| !(1..=65535).contains(d))
            || meta.duration_ms == Some(0) && meta.media_type.starts_with("audio/")
        {
            return Err(AttachError::Invalid);
        }
        let ct_len = plaintext.len() as u64 + attach::TAG_LEN;
        if self.st.app.prepared.len() >= MAX_PREPARED || !self.blob_budget_ok(&limits, ct_len, 1) {
            return Err(AttachError::StorageFull);
        }
        let (key, blob_id, owner_cap) = (arr32(self.rng), arr32(self.rng), arr32(self.rng));
        let request_id = self.rand16();
        let ct = attach::seal(&key, &blob_id, plaintext);
        let digest = attach::ciphertext_digest(&ct);
        let r = AttachmentRef {
            blob_id,
            media_type: meta.media_type,
            plaintext_size: plaintext.len() as u64,
            ciphertext_digest: digest,
            key,
            locations: vec![],
            filename: meta.filename.as_deref().and_then(attach::clean_filename),
            width: meta.width,
            height: meta.height,
            duration_ms: meta.duration_ms,
            read_cap: None,
        };
        let id = hex(&blob_id);
        self.st.app.prepared.insert(
            id.clone(),
            PreparedBlob {
                rec: AttachRec {
                    blob_id: id.clone(),
                    reference: hex(&cbor::encode(&r.to_v())).into(),
                    owner_cap: Some(hex(&owner_cap).into()),
                    request_id: Some(hex(&request_id)),
                    stored: false,
                    location: None,
                    expires_ms: 0,
                    ciphertext_len: ct_len,
                    direct: false,
                },
                created_ms: self.now_ms,
            },
        );
        Ok(PreparedAttachment {
            blob_id: id,
            ciphertext_digest: hex(&digest),
            ciphertext_len: ct_len,
            ciphertext: ct,
        })
    }

    /// Seal a file that is too big for the delivery service so it can be offered directly. Core has already refused the
    /// spool path for it (`plan_size`, ruled behaviour: refuse before sending, say so, offer); a file that fits the
    /// spool is `Invalid` here, it goes the ordinary way. Nothing leaves the device. The read capability is a random
    /// 32-byte value Core mints now; it travels only inside the sealed message. The sealed object goes to the returned
    /// sender (memory only; `Core::arm_direct_offer` rebuilds it from the file after a restart).
    pub fn prepare_direct_attachment(
        &mut self,
        limits: Option<BlobLimits>,
        plaintext: &[u8],
        meta: Meta,
    ) -> Result<(PreparedAttachment, direct::DirectSender), AttachError> {
        match direct::plan_size(limits, plaintext.len() as u64) {
            direct::SizePlan::Refused(e) => return Err(e),
            direct::SizePlan::Spool => return Err(AttachError::Invalid),
            direct::SizePlan::OfferDirect { .. } => {}
        }
        if !attach::valid_media_type(&meta.media_type)
            || [meta.width, meta.height]
                .iter()
                .flatten()
                .any(|d| !(1..=65535).contains(d))
            || meta.duration_ms == Some(0) && meta.media_type.starts_with("audio/")
        {
            return Err(AttachError::Invalid);
        }
        let live = self
            .st
            .app
            .direct_offers
            .values()
            .filter(|o| o.status.is_live())
            .count();
        if self.st.app.prepared.len() >= MAX_PREPARED || live >= MAX_DIRECT_OFFERS {
            return Err(AttachError::StorageFull);
        }
        let (key, blob_id, read_cap) = (arr32(self.rng), arr32(self.rng), arr32(self.rng));
        let ct = attach::seal(&key, &blob_id, plaintext);
        let digest = attach::ciphertext_digest(&ct);
        let ct_len = ct.len() as u64;
        let r = AttachmentRef {
            blob_id,
            media_type: meta.media_type,
            plaintext_size: plaintext.len() as u64,
            ciphertext_digest: digest,
            key,
            locations: vec![],
            filename: meta.filename.as_deref().and_then(attach::clean_filename),
            width: meta.width,
            height: meta.height,
            duration_ms: meta.duration_ms,
            read_cap: Some(read_cap.to_vec()),
        };
        let id = hex(&blob_id);
        self.st.app.prepared.insert(
            id.clone(),
            PreparedBlob {
                rec: AttachRec {
                    blob_id: id.clone(),
                    reference: hex(&cbor::encode(&r.to_v())).into(),
                    owner_cap: None,
                    request_id: None,
                    stored: false,
                    location: None,
                    expires_ms: 0,
                    ciphertext_len: ct_len,
                    direct: true,
                },
                created_ms: self.now_ms,
            },
        );
        let sender = direct::DirectSender::new(blob_id, digest, read_cap.to_vec(), ct)?;
        Ok((
            PreparedAttachment {
                blob_id: id,
                ciphertext_digest: hex(&digest),
                ciphertext_len: ct_len,
                ciphertext: vec![],
            },
            sender,
        ))
    }

    // ------------------------------------------------------------------------------------------ intents

    pub(super) fn send_attachments_direct(
        &mut self,
        op: &OperationId,
        conv: &str,
        text: &str,
        inputs: &[AttachmentInput],
    ) -> Result<(), CoreError> {
        let kind = "SendAttachmentsDirect";
        let reject =
            |s: &mut Self, reason| s.record(op, kind, OutcomeClass::Rejected { reason }, None);
        if inputs.is_empty() || inputs.len() > attach::MAX_ATTACHMENTS {
            reject(self, RejectReason::Invalid);
            return Ok(());
        }
        let mut recs: Vec<AttachRec> = vec![];
        for input in inputs {
            let Some(p) = self.st.app.prepared.get(&input.blob_id) else {
                reject(self, RejectReason::NotFound);
                return Ok(());
            };
            // Direct files only, no voice notes (their presentation is bound to the ordinary path), no repeats.
            if !p.rec.direct
                || input.voice_note.is_some()
                || recs.iter().any(|r| r.blob_id == input.blob_id)
            {
                reject(self, RejectReason::Invalid);
                return Ok(());
            }
            recs.push(p.rec.clone());
        }
        let text = text.trim();
        let ids: Vec<Hex> = recs.iter().map(|r| r.blob_id.clone()).collect();
        self.compose(
            op,
            kind,
            conv,
            (!text.is_empty()).then_some(text),
            recs,
            None,
        )?;
        let Some(mid) = self
            .st
            .app
            .ops
            .get(op)
            .filter(|o| !matches!(o.outcome, OutcomeClass::Rejected { .. }))
            .and_then(|o| o.message_id.clone())
        else {
            return Ok(());
        };
        let now = self.now_ms;
        for id in ids {
            self.st.app.prepared.remove(&id);
            self.st.app.direct_offers.insert(
                id,
                DirectOfferRec {
                    conversation_id: conv.to_string(),
                    message_id: mid.clone(),
                    created_ms: now,
                    expires_ms: now + DIRECT_OFFER_TTL_MS,
                    status: DirectStatus::Waiting,
                },
            );
        }
        Ok(())
    }

    pub(super) fn withdraw_direct_offer(
        &mut self,
        op: &OperationId,
        message_id: &str,
    ) -> Result<(), CoreError> {
        let kind = "WithdrawDirectOffer";
        let mut found = false;
        for o in self
            .st
            .app
            .direct_offers
            .values_mut()
            .filter(|o| o.message_id == message_id)
        {
            found = true;
            if o.status.is_live() {
                o.status = DirectStatus::Withdrawn;
            }
        }
        let outcome = if found {
            OutcomeClass::Committed
        } else {
            OutcomeClass::Rejected {
                reason: RejectReason::NotFound,
            }
        };
        self.record(op, kind, outcome, Some(message_id.to_string()));
        Ok(())
    }

    /// An offer nobody fetched in time lapses; a finished one is forgotten after a month.
    pub(super) fn expire_direct_offers(&mut self) {
        let now = self.now_ms;
        for o in self.st.app.direct_offers.values_mut() {
            if o.status.is_live() && o.expires_ms <= now {
                o.status = DirectStatus::Expired;
            }
        }
        self.st
            .app
            .direct_offers
            .retain(|_, o| o.status.is_live() || o.expires_ms + 30 * DAY_MS > now);
    }

    /// A fetch that finished with the exact bytes: the offer is delivered (local receipt only).
    pub(crate) fn direct_delivered(&mut self, blob_id: &str) {
        if let Some(o) = self.st.app.direct_offers.get_mut(blob_id)
            && o.status.is_live()
        {
            o.status = DirectStatus::Delivered;
        }
    }

    pub(super) fn send_attachments(
        &mut self,
        op: &OperationId,
        conv: &str,
        text: &str,
        inputs: &[AttachmentInput],
    ) -> Result<(), CoreError> {
        let kind = "SendAttachments";
        let reject =
            |s: &mut Self, reason| s.record(op, kind, OutcomeClass::Rejected { reason }, None);
        if inputs.is_empty() || inputs.len() > attach::MAX_ATTACHMENTS {
            reject(self, RejectReason::Invalid);
            return Ok(());
        }
        let mut recs: Vec<AttachRec> = vec![];
        let mut voice: Option<Voice> = None;
        for (i, input) in inputs.iter().enumerate() {
            let Some(p) = self.st.app.prepared.get(&input.blob_id) else {
                reject(self, RejectReason::NotFound);
                return Ok(());
            };
            if recs.iter().any(|r| r.blob_id == input.blob_id) {
                reject(self, RejectReason::Invalid);
                return Ok(());
            }
            if let Some(vn) = &input.voice_note {
                let waveform = match vn.waveform.as_deref().map(unhex) {
                    None => None,
                    Some(Ok(w))
                        if (present::WAVEFORM_MIN..=present::WAVEFORM_MAX).contains(&w.len()) =>
                    {
                        Some(w)
                    }
                    Some(_) => {
                        reject(self, RejectReason::Invalid);
                        return Ok(());
                    }
                };
                let v = Voice {
                    attachment_index: i as u8,
                    waveform,
                };
                let bound = rec_ref(&p.rec)
                    .is_some_and(|r| r.media_type.starts_with("audio/") && r.duration_ms.is_some());
                if voice.is_some() || !bound {
                    reject(self, RejectReason::Invalid);
                    return Ok(());
                }
                voice = Some(v);
            }
            recs.push(p.rec.clone());
        }
        // The size check is repeated here: the limits may have changed since preparation, and a refusal must come
        // before anything is sent (typed, in plain words), never as a late failure.
        let limits = self.blob_limits;
        let mut extra = (0u64, 0u32);
        for r in &recs {
            let plain = r.ciphertext_len.saturating_sub(attach::TAG_LEN);
            if let Err(e) = self.check_blob_size(limits, plain) {
                reject(self, e.reject_reason());
                return Ok(());
            }
            extra = (extra.0 + r.ciphertext_len, extra.1 + 1);
        }
        if let Some(l) = limits
            && !self.blob_budget_ok(&l, extra.0, extra.1)
        {
            reject(self, RejectReason::Quota);
            return Ok(());
        }
        let text = text.trim();
        let ids: Vec<Hex> = recs.iter().map(|r| r.blob_id.clone()).collect();
        let ext = Extensions {
            voice,
            location: None,
        };
        self.compose(
            op,
            kind,
            conv,
            (!text.is_empty()).then_some(text),
            recs,
            Some(ext),
        )?;
        if self
            .st
            .app
            .ops
            .get(op)
            .is_some_and(|o| !matches!(o.outcome, OutcomeClass::Rejected { .. }))
        {
            for id in ids {
                self.st.app.prepared.remove(&id);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn send_location(
        &mut self,
        op: &OperationId,
        conv: &str,
        lat_e7: i32,
        lon_e7: i32,
        accuracy_m: Option<u32>,
        label: Option<&str>,
        note: Option<&str>,
    ) -> Result<(), CoreError> {
        let kind = "SendLocation";
        let invalid = |s: &mut Self| {
            s.record(
                op,
                kind,
                OutcomeClass::Rejected {
                    reason: RejectReason::Invalid,
                },
                None,
            )
        };
        let label = label
            .map(sanitize_label)
            .filter(|l| !l.is_empty() && l.len() <= present::LABEL_MAX_BYTES);
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        if lat_e7.unsigned_abs() > 900_000_000
            || lon_e7.unsigned_abs() > 1_800_000_000
            || accuracy_m.is_some_and(|a| !(1..=present::ACCURACY_MAX_M).contains(&a))
        {
            invalid(self);
            return Ok(());
        }
        // The fallback carries exactly this one canonical point, so old clients show the same place the card does.
        let token = present::geo_token(lat_e7, lon_e7);
        let text = match note {
            Some(n) => format!("{n}\n{token}"),
            None => token,
        };
        if !present::fallback_names_exactly(&text, lat_e7, lon_e7) {
            invalid(self);
            return Ok(());
        }
        let ext = Extensions {
            voice: None,
            location: Some(Location {
                lat_e7,
                lon_e7,
                accuracy_m,
                label,
            }),
        };
        self.compose(op, kind, conv, Some(&text), vec![], Some(ext))
    }

    // ------------------------------------------------------------------------------------------ uploads

    /// Queue (or re-queue) the uploads of a message's blobs that are not stored. The same BlobID, owner capability and
    /// request id every time: an upload repeated after an uncertain outcome is the same operation.
    pub(super) fn enqueue_puts(&mut self, op: &OperationId, conv: &str, mid: &str) {
        let now = self.now_ms;
        let Some(recs) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == mid))
            .map(|m| {
                m.attachments
                    .iter_mut()
                    .filter(|a| a.owner_cap.is_some())
                    .filter_map(|a| {
                        if a.stored && a.expires_ms <= now {
                            a.stored = false; // the store's copy has expired: put it again
                        }
                        (!a.stored).then(|| a.clone())
                    })
                    .collect::<Vec<_>>()
            })
        else {
            return;
        };
        for a in recs {
            let queued = self.st.app.outbox.iter().any(|e| {
                matches!(&e.purpose, Purpose::PutBlob { blob_id, message_id, .. }
                    if *blob_id == a.blob_id && message_id == mid)
            });
            let (Some(cap), Some(rid), Some(r)) = (&a.owner_cap, &a.request_id, rec_ref(&a)) else {
                continue;
            };
            if queued {
                continue;
            }
            let (Ok(blob_id), Ok(owner_cap), Ok(request_id)) =
                (unhex(&a.blob_id), unhex(cap), unhex(rid))
            else {
                continue;
            };
            self.enqueue(
                NetCall::PutBlob {
                    blob_id,
                    owner_cap,
                    request_id,
                    digest: r.ciphertext_digest.to_vec(),
                    length: a.ciphertext_len,
                },
                Purpose::PutBlob {
                    operation_id: op.clone(),
                    conversation_id: conv.to_string(),
                    message_id: mid.to_string(),
                    blob_id: a.blob_id.clone(),
                },
            );
        }
    }

    /// A message that cannot go on (a refused upload): drop its other pending uploads, and, when it can never be retried,
    /// remove what already reached the store. Best effort; the store expires everything anyway.
    pub(super) fn abort_puts(&mut self, conv: &str, mid: &str, delete_stored: bool) {
        self.st.app.outbox.retain(
            |e| !matches!(&e.purpose, Purpose::PutBlob { message_id, .. } if message_id == mid),
        );
        if !delete_stored {
            return;
        }
        let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == mid))
        else {
            return;
        };
        let mut dels = vec![];
        for a in m.attachments.iter_mut().filter(|a| a.stored) {
            if let (Some(cap), Some(loc)) = (&a.owner_cap, &a.location) {
                dels.push((a.blob_id.clone(), cap.to_string(), loc.clone()));
            }
            a.stored = false;
        }
        for (id, cap, loc) in dels {
            if let (Ok(blob_id), Ok(owner_cap)) = (unhex(&id), unhex(&cap)) {
                self.enqueue(
                    NetCall::DeleteBlob {
                        blob_id,
                        owner_cap,
                        location: loc,
                    },
                    Purpose::DeleteBlob { blob_id: id },
                );
            }
        }
    }

    /// `RetryDelivery` of an attachment message whose uploads were refused for a reason that can clear: upload again with
    /// the same identities. `true` when this handled the message.
    pub(super) fn retry_uploads(&mut self, op: &OperationId, conv: &str, msg: &Msg) -> bool {
        let retryable =
            matches!(&msg.delivery, DeliveryState::Rejected { reason } if reason.retryable());
        if !retryable
            || msg.attachments.iter().all(|a| a.owner_cap.is_none())
            || msg.queued_event.is_some()
            || !msg.fan.is_empty()
        {
            return false;
        }
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == msg.id))
        {
            // The retry is now the message's current operation (the earlier one stays rejected, as it was).
            m.operation_id = Some(op.clone());
        }
        self.set_delivery(conv, &msg.id, DeliveryState::PendingNetwork);
        self.enqueue_puts(op, conv, &msg.id);
        self.record(
            op,
            "RetryDelivery",
            OutcomeClass::PendingNetwork,
            Some(msg.id.clone()),
        );
        let _ = self.finalize_attachment_message(conv, &msg.id);
        true
    }

    /// The store's receipt for one blob. Anything that does not match what we sealed fails closed.
    pub(super) fn on_blob_put(
        &mut self,
        op: &OperationId,
        conv: &str,
        mid: &str,
        blob_id: &str,
        receipt: Option<Vec<u8>>,
    ) -> Result<(), CoreError> {
        let now = self.now_ms;
        let rec = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.messages.iter().find(|m| m.id == mid))
            .and_then(|m| m.attachments.iter().find(|a| a.blob_id == blob_id))
            .cloned();
        let Some(rec) = rec else {
            return Ok(());
        };
        let updated = (|| {
            let receipt = BlobReceipt::from_bytes(&receipt?)?;
            let mut r = rec_ref(&rec)?;
            if receipt.digest != r.ciphertext_digest {
                return None; // the store holds other bytes than we sealed
            }
            r.locations = receipt
                .locations
                .into_iter()
                .filter(|l| attach::valid_location(l))
                .take(8)
                .collect();
            if r.locations.is_empty() || !(16..=64).contains(&receipt.read_cap.len()) {
                return None;
            }
            r.read_cap = Some(receipt.read_cap);
            Some((r, receipt.expires_at))
        })();
        let Some((r, expires)) = updated else {
            // Wrong digest or an unusable receipt: the object is not what we sealed (or cannot be addressed). Not retried
            // blindly; the message is refused with an integrity failure and what reached the store is removed.
            self.set_outcome(
                op,
                OutcomeClass::Rejected {
                    reason: RejectReason::IntegrityFailure,
                },
            );
            self.set_delivery(
                conv,
                mid,
                DeliveryState::Rejected {
                    reason: RejectReason::IntegrityFailure,
                },
            );
            self.abort_puts(conv, mid, true);
            return Ok(());
        };
        if let Some(a) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == mid))
            .and_then(|m| m.attachments.iter_mut().find(|a| a.blob_id == blob_id))
        {
            a.location = r.locations.first().cloned();
            a.reference = hex(&cbor::encode(&r.to_v())).into();
            a.stored = true;
            a.expires_ms = expires
                .map(|e| e.saturating_mul(1000))
                .unwrap_or(now + DEFAULT_STORE_MS);
        }
        self.finalize_attachment_message(conv, mid)
    }

    /// Once every blob of an outgoing message is stored, build its `message.create` (EventID and clock were fixed when
    /// the message was composed) and release it exactly like a text message.
    pub(super) fn finalize_attachment_message(
        &mut self,
        conv: &str,
        mid: &str,
    ) -> Result<(), CoreError> {
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return Ok(());
        };
        let Some(m) = rel.messages.iter().find(|m| m.id == mid).cloned() else {
            return Ok(());
        };
        if m.queued_event.is_some()
            || !m.fan.is_empty()
            || m.envelope.is_some()
            || matches!(m.delivery, DeliveryState::Rejected { .. })
            || m.attachments
                .iter()
                .any(|a| a.owner_cap.is_some() && !a.stored)
        {
            return Ok(());
        }
        let state = rel.state;
        let (Some(canon), Some(account), Some(device_id)) = (
            rel.canon_id.as_deref().and_then(|c| h32(c).ok()),
            self.st.device.as_ref().map(|d| d.account_id),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return Ok(());
        };
        let event_id = unhex(mid.trim_start_matches("msg-"))
            .ok()
            .and_then(|b| <[u8; 16]>::try_from(b).ok())
            .ok_or(CoreError::CorruptState("message id"))?;
        let refs: Option<Vec<V>> = m
            .attachments
            .iter()
            .map(|a| rec_ref(a).map(|r| r.to_v()))
            .collect();
        let ext = m
            .ext
            .as_deref()
            .and_then(|e| unhex(e).ok())
            .and_then(|e| cbor::decode(&e).ok());
        let payload = refs.map(|refs| {
            events::Header {
                event_id,
                conversation_id: canon,
                author_account: account,
                author_device: device_id,
                clock: Hlc {
                    physical_ms: m.clock_ms,
                    logical: m.clock_logical,
                },
                kind: events::K_CREATE,
                body: events::message_create_body_with(
                    (!m.text.is_empty()).then_some(m.text.as_str()),
                    refs,
                ),
            }
            .encode_with(ext)
        });
        let Some(payload) =
            payload.filter(|p| registry().check("ConversationEvent", p, false).is_ok())
        else {
            let op = m.operation_id.clone().unwrap_or_default();
            self.set_outcome(
                &op,
                OutcomeClass::Rejected {
                    reason: RejectReason::Invalid,
                },
            );
            self.set_delivery(
                conv,
                mid,
                DeliveryState::Rejected {
                    reason: RejectReason::Invalid,
                },
            );
            self.abort_puts(conv, mid, true);
            return Ok(());
        };
        if let Some(slot) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|x| x.id == mid))
        {
            slot.digest = hex(&crypto::sha256(&[&payload]));
            slot.queued_event = Some(hex(&payload));
        }
        if state == RelState::Active {
            self.release_queued(conv)?;
        }
        Ok(())
    }

    /// Prepared attachments nobody used are forgotten.
    pub(super) fn expire_prepared(&mut self) {
        let now = self.now_ms;
        self.st
            .app
            .prepared
            .retain(|_, p| p.created_ms + PREPARED_TTL_MS > now);
    }

    /// A delete that cannot get through is given up on after a while (the blob expires by itself).
    pub(super) fn delete_gave_up(&self, pos: usize) -> bool {
        self.st.app.outbox.get(pos).is_some_and(|e| {
            matches!(e.purpose, Purpose::DeleteBlob { .. }) && e.fails > DELETE_GIVE_UP
        })
    }
}
