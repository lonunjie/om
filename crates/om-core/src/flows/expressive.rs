// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::crypto::binding::Credential;
use crate::expressive::{self as media, AssetRef, Pending, SendFrom, Target, WEBP};
use crate::groups::{
    GroupError,
    flow::{GroupCtx, MAX_TEXT_BYTES},
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoomUpload {
    pub room: String,
    pub text: String,
    pub attachment: AttachRec,
    presentation: Option<media::Presentation>,
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Covers the whole authenticated recipient set, even if a device has no route yet.
    pub(super) fn expressive_recipients(&self, rel: &Relationship, slot: Option<&str>) -> bool {
        if !self.expressive_enabled {
            return false;
        }
        let (Some(device), Ok(peer)) = (self.st.device.as_ref(), h32(&rel.peer_account_id)) else {
            return false;
        };
        let supports = |account, sibling: bool| -> Option<bool> {
            let Some((_, trusted)) = device.peers.get(&account) else {
                if !sibling {
                    return None;
                }
                // The own head can be absent from the session store on older installs.
                // Read the already accepted local head and prove there are no other recipients.
                let signed = cbor::decode(&unhex(self.st.app.acs_chain.last()?).ok()?).ok()?;
                let head = Acs::from_body(signed.get(1).ok()?.b().ok()?).ok()?;
                let has_sibling = head
                    .control
                    .union(&head.messaging)
                    .any(|id| id != &device.device_id && head.device_active(id));
                return (head.account_id == account && !has_sibling).then_some(false);
            };
            let head = Acs::from_body(&trusted.acs_body).ok()?;
            let mut any = false;
            for id in head.control.union(&head.messaging) {
                if !head.device_active(id) || (sibling && id == &device.device_id) {
                    continue;
                }
                any = true;
                let expected = head.credentials.iter().find(|(d, _)| d == id)?.1;
                let cred = trusted
                    .credential_bodies
                    .iter()
                    .filter_map(|b| Credential::from_body(b).ok())
                    .find(|c| {
                        c.device_id == *id && c.account_id == account && c.hash() == expected
                    })?;
                let includes = |cap: &str| cred.capabilities.iter().any(|s| s == cap);
                if !includes(WEBP) || !slot.is_none_or(includes) {
                    return None;
                }
            }
            Some(any)
        };
        supports(peer, false) == Some(true) && supports(device.account_id, true).is_some()
    }

    pub(super) fn send_expressive(
        &mut self,
        op: &OperationId,
        conv: &str,
        target: &Target,
        source: &SendFrom,
    ) -> Result<(), CoreError> {
        const KIND: &str = "SendExpressive";
        let refuse = |s: &mut Self, reason| {
            s.record(op, KIND, OutcomeClass::Rejected { reason }, None);
            Ok(())
        };
        if !self.expressive_enabled {
            return refuse(self, RejectReason::Unsupported);
        }
        let room = self.st.app.groups.contains_key(conv);
        if room {
            if !matches!(target, Target::Attachment) {
                return refuse(self, RejectReason::Unsupported);
            }
        } else {
            let Some(rel) = self.st.app.relationships.get(conv) else {
                return refuse(self, RejectReason::NotFound);
            };
            if !matches!(rel.state, RelState::Active) || rel.blocked {
                return refuse(self, RejectReason::Unavailable);
            }
        }
        let (rec, presentation, prepared) = match source {
            SendFrom::Prepared { blob_id } => {
                let Some(p) = self.st.app.prepared.get(blob_id) else {
                    return refuse(self, RejectReason::NotFound);
                };
                let Some(presentation) = p.expressive.clone() else {
                    return refuse(self, RejectReason::Invalid);
                };
                (p.rec.clone(), presentation, Some(blob_id))
            }
            SendFrom::Shared {
                conversation_id,
                message_id,
            } => {
                let Some(m) = self
                    .st
                    .app
                    .relationships
                    .get(conversation_id)
                    .filter(|r| !r.blocked)
                    .and_then(|r| {
                        r.messages
                            .iter()
                            .find(|m| m.id == *message_id && m.outgoing && !m.deleted)
                    })
                else {
                    return refuse(self, RejectReason::NotFound);
                };
                let Some(presentation) = m
                    .social
                    .expressive_send
                    .as_ref()
                    .map(|send| &send.presentation)
                    .filter(|p| p.shared_blob)
                else {
                    return refuse(self, RejectReason::Invalid);
                };
                let Some(mut rec) = m
                    .attachments
                    .first()
                    .cloned()
                    .filter(|r| r.stored && r.expires_ms > self.now_ms && !r.direct)
                else {
                    return refuse(self, RejectReason::Unavailable);
                };
                // The source operation owns deletion and repair. This send reuses only its read ref.
                rec.owner_cap = None;
                rec.request_id = None;
                (rec, presentation.clone(), None)
            }
        };
        let Some(attachment) = blobs::rec_ref(&rec) else {
            return refuse(self, RejectReason::Invalid);
        };
        if AssetRef::new(attachment, presentation.clone()).is_err()
            || presentation.media_profile_id != WEBP
        {
            return refuse(self, RejectReason::Invalid);
        }
        let valid_target = match target {
            Target::Attachment => matches!(
                presentation.kind,
                media::Kind::Sticker | media::Kind::LoopAnimation
            ),
            Target::Inline => presentation.kind == media::Kind::CustomEmoji,
            Target::Edit { message_id } => {
                if self.own_target(conv, message_id).is_none() {
                    return refuse(self, RejectReason::NotFound);
                }
                presentation.kind == media::Kind::CustomEmoji
            }
            Target::Reaction { message_id, key } => {
                if self.event_target(conv, message_id).is_none() {
                    return refuse(self, RejectReason::NotFound);
                }
                presentation.reaction_key() == Some(key.as_str())
                    && !self.own_reaction_would_exceed(conv, message_id, key)
            }
        };
        if !valid_target {
            return refuse(self, RejectReason::Invalid);
        }
        if room {
            let text = presentation.fallback().to_owned();
            return self.start_room_upload(op, KIND, conv, text, rec, Some(presentation));
        }
        let inline_fallback_fits =
            target.slot() != Some(media::INLINE) || presentation.fallback().chars().count() <= 128;
        let capable = inline_fallback_fits
            && self.expressive_recipients(&self.st.app.relationships[conv], target.slot());
        if target.slot().is_some() && !capable {
            let text = presentation.fallback();
            if let Target::Inline = target {
                self.compose(op, KIND, conv, Some(text), vec![], None, None, &[])?;
            } else {
                let pending = Pending {
                    target: target.clone(),
                    presentation,
                };
                let (kind, body) = pending
                    .body(None)
                    .ok_or(CoreError::Invalid("expressive fallback"))?;
                if let Some((_, clock)) = self.queue_event(op, KIND, conv, kind, body.clone())? {
                    self.apply_expressive_mutation(conv, kind, body, clock);
                }
            }
        } else {
            if !rec.stored {
                let size = rec.ciphertext_len.saturating_sub(crate::attach::TAG_LEN);
                if let Err(e) = self.check_blob_size(self.blob_limits, size) {
                    return refuse(self, e.reject_reason());
                }
                if let Some(limits) = self.blob_limits
                    && !self.blob_budget_ok(&limits, rec.ciphertext_len, 1)
                {
                    return refuse(self, RejectReason::Quota);
                }
            }
            let stored = rec.stored;
            self.compose(
                op,
                KIND,
                conv,
                Some(presentation.fallback()),
                vec![rec],
                None,
                None,
                &[],
            )?;
            let Some(message) = self
                .st
                .app
                .ops
                .get(op)
                .and_then(|o| o.message_id.clone())
                .filter(|_| {
                    self.st
                        .app
                        .ops
                        .get(op)
                        .is_some_and(|o| !matches!(o.outcome, OutcomeClass::Rejected { .. }))
                })
            else {
                return Ok(());
            };
            let Some(m) = self
                .st
                .app
                .relationships
                .get_mut(conv)
                .and_then(|r| r.messages.iter_mut().find(|m| m.id == message))
            else {
                return Err(CoreError::CorruptState("expressive send message"));
            };
            m.lane = matches!(target, Target::Edit { .. } | Target::Reaction { .. });
            m.social.expressive_send = Some(Pending {
                target: target.clone(),
                presentation,
            });
            if stored {
                self.finalize_attachment_message(conv, &message)?;
            }
        }
        if let Some(id) = prepared
            && self
                .st
                .app
                .ops
                .get(op)
                .is_some_and(|o| !matches!(o.outcome, OutcomeClass::Rejected { .. }))
        {
            self.st.app.prepared.remove(id);
        }
        Ok(())
    }

    pub(super) fn start_room_upload(
        &mut self,
        op: &OperationId,
        kind: &str,
        room: &str,
        text: String,
        attachment: AttachRec,
        presentation: Option<media::Presentation>,
    ) -> Result<(), CoreError> {
        let refuse = |s: &mut Self, reason| {
            s.record(op, kind, OutcomeClass::Rejected { reason }, None);
            Ok(())
        };
        let Some(device) = &self.st.device else {
            return refuse(self, RejectReason::Unavailable);
        };
        let conv = h32(room).map_err(|_| CoreError::Invalid("room id"))?;
        if self
            .st
            .app
            .group_upload_check(&conv, &device.account_id)
            .is_err()
        {
            return refuse(self, RejectReason::Unavailable);
        }
        if !self.expressive_enabled || attachment.direct {
            return refuse(self, RejectReason::Unsupported);
        }
        if text.len() > MAX_TEXT_BYTES {
            return refuse(self, RejectReason::Invalid);
        }
        if !attachment.stored {
            let size = attachment
                .ciphertext_len
                .saturating_sub(crate::attach::TAG_LEN);
            if let Err(err) = self.check_blob_size(self.blob_limits, size) {
                return refuse(self, err.reject_reason());
            }
            if self
                .blob_limits
                .is_some_and(|limits| !self.blob_budget_ok(&limits, attachment.ciphertext_len, 1))
            {
                return refuse(self, RejectReason::Quota);
            }
            if !self.enqueue_blob_put(op, room, &format!("room-upload-{op}"), &attachment) {
                return refuse(self, RejectReason::Invalid);
            }
        }
        let stored = attachment.stored;
        self.st.app.prepared.remove(&attachment.blob_id);
        self.st.app.room_uploads.insert(
            op.clone(),
            RoomUpload {
                room: room.to_owned(),
                text,
                attachment,
                presentation,
            },
        );
        self.record(op, kind, OutcomeClass::PendingNetwork, None);
        if stored {
            self.finish_room_upload(op)?;
        }
        Ok(())
    }

    pub(super) fn finish_room_upload(&mut self, op: &OperationId) -> Result<(), CoreError> {
        let upload = self
            .st
            .app
            .room_uploads
            .remove(op)
            .ok_or(CoreError::CorruptState("room attachment upload"))?;
        let conv = h32(&upload.room).map_err(|_| CoreError::CorruptState("room id"))?;
        let device = self
            .st
            .device
            .as_ref()
            .ok_or(CoreError::CorruptState("room device"))?;
        // The upload spans transactions. Open the committed MLS snapshot and use the Room head that is
        // canonical now, then persist the MLS advance, message and fan-out in the receipt transaction.
        let result = (|| {
            if !self.expressive_enabled || upload.attachment.expires_ms <= self.now_ms {
                return Err(GroupError("attachment_unavailable"));
            }
            let mut app = self.st.app.clone();
            let snapshot = app
                .group_crypto
                .as_ref()
                .ok_or(GroupError("unknown_group"))?;
            let mut engine = crate::engine::group_engine_open(&device.device_id, Some(snapshot))?;
            let mut ctx = GroupCtx {
                engine: &mut *engine,
                rng: &mut *self.rng,
                now_ms: self.now_ms,
                signer: &device.signer,
                my_account: device.account_id.to_vec(),
            };
            let id = app.group_send_attachment(
                &mut ctx,
                &conv,
                &upload.text,
                upload.attachment.clone(),
                upload.presentation.as_ref(),
            )?;
            self.st.app = app;
            Ok(id)
        })();
        if let Ok(id) = result {
            if let Some(record) = self.st.app.ops.get_mut(op) {
                record.message_id = Some(id);
            }
            self.set_outcome(op, OutcomeClass::Committed);
        } else {
            self.delete_stored_blob(&upload.attachment);
            self.set_outcome(
                op,
                OutcomeClass::Rejected {
                    reason: RejectReason::Unavailable,
                },
            );
        }
        Ok(())
    }

    pub(super) fn apply_expressive_mutation(&mut self, conv: &str, kind: u64, body: V, clock: Hlc) {
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        let ev = events::Event {
            event_id: [0; 16],
            conversation_id: [0; 32],
            author_account: [0; 32],
            author_device: [0; 16],
            clock,
            kind,
            body,
            digest: [0; 32],
            extensions: None,
        };
        social::apply_direct_mutation(rel, &ev, true);
    }
}

impl Pending {
    pub(super) fn body(&self, asset: Option<&AssetRef>) -> Option<(u64, V)> {
        let token = self.presentation.fallback();
        let rich = || {
            let Some(asset) = asset else {
                return events::rich_text(token);
            };
            let inline = V::M(vec![
                (0, V::U(4)),
                (1, V::T(token.into())),
                (6, asset.value()),
            ]);
            V::M(vec![
                (0, V::U(1)),
                (
                    1,
                    V::A(vec![V::M(vec![(0, V::U(1)), (1, V::A(vec![inline]))])]),
                ),
            ])
        };
        Some(match &self.target {
            Target::Attachment => return None,
            Target::Inline => (events::K_CREATE, V::M(vec![(0, rich()), (1, V::A(vec![]))])),
            Target::Edit { message_id } => (
                events::K_EDIT,
                V::M(vec![
                    (
                        0,
                        V::B(h16(message_id.strip_prefix("msg-")?).ok()?.to_vec()),
                    ),
                    (1, rich()),
                ]),
            ),
            Target::Reaction { message_id, key } => {
                let mut fields = vec![
                    (
                        0,
                        V::B(h16(message_id.strip_prefix("msg-")?).ok()?.to_vec()),
                    ),
                    (1, V::T(key.clone())),
                    (2, V::Bool(true)),
                ];
                if let Some(asset) = asset {
                    fields.push((3, asset.value()));
                }
                (events::K_REACTION, V::M(fields))
            }
        })
    }
}
