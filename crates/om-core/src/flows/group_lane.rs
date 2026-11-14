//! The group relationship lane (ADR-0189 section 1.1): Invite, Accept and Decline as 1:1 `ConversationEvent`s of kinds
//! 15 to 17 inside the ordinary sealed 1:1 envelope. They are queued and delivered exactly like a message (same ratchet,
//! same fan-out, same retry) but are flagged `lane`, so they never appear in the timeline.

use super::*;
use crate::groups::GroupError;
use crate::groups::carriage::{self, LaneBody};

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Build the lane event for `body` and queue it on the relationship. The relationship must be `Active` with a
    /// usable delivery key, as for any message; otherwise `relationship_unavailable` and nothing was queued.
    pub fn group_lane_queue(
        &mut self,
        conv: &str,
        body: &LaneBody,
    ) -> Result<Result<(), GroupError>, CoreError> {
        let no = |why| Ok(Err(GroupError(why)));
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return no("unknown_relationship");
        };
        if !(rel.state == RelState::Active && rel.peer_relays.primary().is_some() && !rel.blocked)
            || self.peer_target(rel).is_err()
        {
            return no("relationship_unavailable");
        }
        let (Some(canon), Some(account), Some(device_id)) = (
            rel.canon_id.as_deref().and_then(|c| h32(c).ok()),
            self.st.device.as_ref().map(|d| d.account_id),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return no("relationship_unavailable");
        };
        let clock = Hlc::next(
            Hlc {
                physical_ms: rel.hlc_ms,
                logical: rel.hlc_logical,
            },
            self.now_ms,
        );
        let mid = self.rand16();
        let payload = match carriage::lane_event(mid, canon, account, device_id, clock, body) {
            Ok(p) => p,
            Err(e) => return Ok(Err(e)),
        };
        let now = self.now_ms;
        let rel = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .ok_or(CoreError::NotFound("conversation"))?;
        rel.messages.push(Msg {
            id: format!("msg-{}", hex(&mid)),
            outgoing: true,
            operation_id: None,
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
        });
        rel.hlc_ms = clock.physical_ms;
        rel.hlc_logical = clock.logical;
        self.release_queued(conv)?;
        Ok(Ok(()))
    }

    /// An authenticated inner payload of kind 15 to 17. Structural failure (not a valid lane event, wrong
    /// conversation or author, equivocation) is a rejection like any event. A well-formed event that the group layer
    /// refuses on its merits (an Accept for an unknown invite, a stale or foreign route, a credential the validated head
    /// does not commit to) is dropped with no state change and the payload is accepted, so it is not replayed.
    pub(super) fn on_group_lane(
        &mut self,
        conv: &str,
        bytes: &[u8],
        sender: [u8; 16],
        sibling: bool,
    ) -> bool {
        let Ok((ev, body)) = carriage::parse_lane_event(bytes) else {
            return false;
        };
        if sibling {
            return true; // own-device sync of lane events is not specified: ignored, never half-applied
        }
        let now_s = self.now_s();
        let Some(r) = self.st.app.relationships.get(conv) else {
            return false;
        };
        let (Some(canon), Ok(peer_account)) = (r.canon_id.as_deref(), h32(&r.peer_account_id))
        else {
            return false;
        };
        if hex(&ev.conversation_id) != canon
            || ev.author_account != peer_account
            || ev.author_device != sender
            || r.equivocated.contains(&hex(&ev.event_id))
        {
            return false;
        }
        if r.blocked {
            return true;
        }
        let before = self.st.app.group_carriage.clone();
        if self
            .st
            .app
            .carry_lane_in(conv, &ev.event_id, body, now_s)
            .is_err()
        {
            self.st.app.group_carriage = before;
        }
        true
    }
}
