// SPDX-License-Identifier: MPL-2.0
//! Everyday chat state that other events attach to a message (MESSENGER_BASELINE_V1, EVENT_MODEL): reactions today. Sending
//! queues a canonical `ConversationEvent` exactly like a message (same ratchet, fan-out and retry) as a `lane` entry that
//! never shows in the timeline; receiving applies the event to the message it names, in this conversation only.

use super::*;

/// A reaction key is a Unicode token of 1 to 64 bytes (`ReactionSet` in events.cddl).
pub const MAX_REACTION_KEY_BYTES: usize = 64;
/// Registers kept per person and message (adds and tombstones together). A bound on what a peer can make us store.
pub const MAX_REACTIONS_PER_ACTOR: usize = 16;

pub(crate) fn valid_reaction_key(k: &str) -> bool {
    !k.trim().is_empty() && k.len() <= MAX_REACTION_KEY_BYTES && !k.chars().any(|c| c.is_control())
}

/// Apply one `reaction.set` to the message it names. The register for (actor, key) takes the event only if it is ordered
/// after what is held, so reordered or repeated events converge. `false`: the target is not a shown message here.
pub(super) fn apply_reaction(
    msgs: &mut [Msg],
    target: &str,
    mine: bool,
    key: &str,
    present: bool,
    clock: Hlc,
) -> bool {
    let Some(m) = msgs
        .iter_mut()
        .find(|m| m.id == target && !m.lane && !m.deleted)
    else {
        return false;
    };
    let recs = &mut m.social.reactions;
    if let Some(r) = recs.iter_mut().find(|r| r.mine == mine && r.key == key) {
        if (clock.physical_ms, clock.logical) > (r.clock_ms, r.clock_logical) {
            r.present = present;
            r.clock_ms = clock.physical_ms;
            r.clock_logical = clock.logical;
        }
        return true;
    }
    if recs.iter().filter(|r| r.mine == mine).count() >= MAX_REACTIONS_PER_ACTOR {
        // Full: a removed one makes room, otherwise the new one is not kept.
        match recs.iter().position(|r| r.mine == mine && !r.present) {
            Some(i) => {
                recs.remove(i);
            }
            None => return true,
        }
    }
    recs.push(ReactionRec {
        mine,
        key: key.to_string(),
        present,
        clock_ms: clock.physical_ms,
        clock_logical: clock.logical,
    });
    true
}

/// The reactions of a message as a person sees them: each key once, how many have it set, and whether one is ours.
pub(crate) fn reaction_views(m: &Msg) -> Vec<ReactionView> {
    let mut out: Vec<ReactionView> = vec![];
    for r in m.social.reactions.iter().filter(|r| r.present) {
        match out.iter_mut().find(|v| v.key == r.key) {
            Some(v) => {
                v.count += 1;
                v.mine |= r.mine;
            }
            None => out.push(ReactionView {
                key: r.key.clone(),
                count: 1,
                mine: r.mine,
            }),
        }
    }
    out
}

/// Parse a received `reaction.set` body: `(target message id, key, present)`.
pub(super) fn read_reaction(ev: &events::Event) -> Option<(String, String, bool)> {
    let target = ev.body.get(0).ok()?.b().ok().filter(|b| b.len() == 16)?;
    let key = ev.body.get(1).ok()?.t().ok()?;
    let present = match ev.body.get(2).ok()? {
        V::Bool(b) => *b,
        _ => return None,
    };
    valid_reaction_key(key).then(|| (format!("msg-{}", hex(target)), key.to_string(), present))
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Build a canonical event of `ekind` with `body` and queue it on the relationship like a message, hidden from the
    /// timeline. Records the refusal and returns `None` when the conversation cannot send now; otherwise returns the id
    /// of the queued entry and the event's clock (the caller applies the same change locally).
    pub(super) fn queue_event(
        &mut self,
        op: &OperationId,
        kind: &str,
        conv: &str,
        ekind: u64,
        body: V,
    ) -> Result<Option<(String, Hlc)>, CoreError> {
        let rejected = |reason| OutcomeClass::Rejected { reason };
        let Some(rel) = self.st.app.relationships.get(conv) else {
            self.record(op, kind, rejected(RejectReason::NotFound), None);
            return Ok(None);
        };
        if rel.peer_recovery.iter().any(PeerRecoveryRec::is_live) {
            self.record(op, kind, rejected(RejectReason::PeerRecovering), None);
            return Ok(None);
        }
        if !(rel.state == RelState::Active && rel.peer_relays.primary().is_some() && !rel.blocked)
            || self.peer_target(rel).is_err()
        {
            self.record(op, kind, rejected(RejectReason::Unavailable), None);
            return Ok(None);
        }
        let (Some(canon), Some(account), Some(device_id)) = (
            rel.canon_id.as_deref().and_then(|c| h32(c).ok()),
            self.st.device.as_ref().map(|d| d.account_id),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            self.record(op, kind, rejected(RejectReason::Unavailable), None);
            return Ok(None);
        };
        let clock = Hlc::next(
            Hlc {
                physical_ms: rel.hlc_ms,
                logical: rel.hlc_logical,
            },
            self.now_ms,
        );
        let mid = self.rand16();
        let payload = events::Header {
            event_id: mid,
            conversation_id: canon,
            author_account: account,
            author_device: device_id,
            clock,
            kind: ekind,
            body,
        }
        .encode();
        if registry()
            .check("ConversationEvent", &payload, false)
            .is_err()
        {
            self.record(op, kind, rejected(RejectReason::Invalid), None);
            return Ok(None);
        }
        let now = self.now_ms;
        let id = format!("msg-{}", hex(&mid));
        let rel = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .ok_or(CoreError::NotFound("conversation"))?;
        rel.messages.push(Msg {
            id: id.clone(),
            outgoing: true,
            operation_id: Some(op.clone()),
            text: String::new(),
            sent_at: now,
            delivery: DeliveryState::PendingNetwork,
            envelope: None,
            digest: hex(&crypto::sha256(&[&payload])),
            clock_ms: clock.physical_ms,
            clock_logical: clock.logical,
            deleted: false,
            queued_event: Some(hex(&payload)),
            fan: vec![],
            attachments: vec![],
            ext: None,
            lane: true,
            social: Social::default(),
        });
        rel.hlc_ms = clock.physical_ms;
        rel.hlc_logical = clock.logical;
        self.release_queued(conv)?;
        self.record(op, kind, OutcomeClass::PendingNetwork, Some(id.clone()));
        Ok(Some((id, clock)))
    }

    /// The shown message `message_id` of `conv` that an event of ours may name: not a lane entry, not deleted, and, if it is
    /// our own, already accepted by the delivery service (so the other side cannot get the event before the message).
    pub(super) fn event_target(&self, conv: &str, message_id: &str) -> Option<[u8; 16]> {
        let m = self
            .st
            .app
            .relationships
            .get(conv)?
            .messages
            .iter()
            .find(|m| m.id == message_id && !m.lane && !m.deleted)?;
        if m.outgoing && !matches!(m.delivery, DeliveryState::Sent | DeliveryState::Delivered) {
            return None;
        }
        h16(m.id.strip_prefix("msg-")?).ok()
    }

    pub(super) fn set_reaction(
        &mut self,
        op: &OperationId,
        conv: &str,
        message_id: &str,
        key: &str,
        present: bool,
    ) -> Result<(), CoreError> {
        let kind = "SetReaction";
        let reject = |s: &mut Self, reason| {
            s.record(op, kind, OutcomeClass::Rejected { reason }, None);
            Ok(())
        };
        if !valid_reaction_key(key) {
            return reject(self, RejectReason::Invalid);
        }
        let Some(target) = self.event_target(conv, message_id) else {
            return reject(self, RejectReason::NotFound);
        };
        let body = V::M(vec![
            (0, V::B(target.to_vec())),
            (1, V::T(key.to_string())),
            (2, V::Bool(present)),
        ]);
        let Some((_, clock)) = self.queue_event(op, kind, conv, events::K_REACTION, body)? else {
            return Ok(());
        };
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            apply_reaction(&mut r.messages, message_id, true, key, present, clock);
        }
        Ok(())
    }
}
