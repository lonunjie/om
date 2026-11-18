// SPDX-License-Identifier: MPL-2.0
//! ADR-0218 calls over existing established device sessions and sealed delivery.

use super::*;
use crate::calls::{
    Body, Call, CallDescription, CallNotice, CallReason, CallRoute, CallState, ContactDevice,
    DEPOSIT_LIMIT, Deposit, DepositKind, DescriptionKind, MediaState, NOTICE_LIMIT, REPLAY_MS,
    RING_MS, SEEN_LIMIT, SPAM_MS, SeenOffer, Signal, valid_sdp,
};
use zeroize::Zeroizing;

pub(super) enum LocalUpdate<'a> {
    Ice(&'a [u8]),
    Media(u8),
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    fn call_contact(&self, rel: &Relationship) -> bool {
        rel.state == RelState::Active
            && !rel.blocked
            && !rel.peer_account_id.is_empty()
            && !rel.peer_recovery.iter().any(PeerRecoveryRec::is_live)
            && self.relink_is_current(rel)
    }

    pub(super) fn set_contact_allow_calls(&mut self, conv: &str, allowed: bool) -> OutcomeClass {
        let Some(rel) = self
            .st
            .app
            .relationships
            .get(conv)
            .filter(|r| self.call_contact(r))
        else {
            return rejected(RejectReason::NotFound);
        };
        let account = rel.peer_account_id.clone();
        if allowed {
            self.st.app.calls_disabled.remove(&account);
        } else {
            self.st.app.calls_disabled.insert(account);
            self.maintain_calls();
        }
        OutcomeClass::Committed
    }

    pub(super) fn start_call(
        &mut self,
        target: &str,
        media: CallMedia,
        sdp: Option<&[u8]>,
    ) -> OutcomeClass {
        if !self.calls_enabled {
            return rejected(RejectReason::Unsupported);
        }
        let Some(sdp) = sdp.filter(|sdp| valid_sdp(sdp)) else {
            return rejected(RejectReason::Invalid);
        };
        if self.st.calls.current.as_ref().is_some_and(Call::is_live) {
            return rejected(RejectReason::Conflict);
        }
        let Some(rel) = self
            .st
            .app
            .relationships
            .values()
            .find(|r| r.person_id == target && self.call_contact(r))
            .cloned()
        else {
            return rejected(RejectReason::Unauthorized);
        };

        let id = self.rand16();
        let mut secret = Zeroizing::new([0; 32]);
        self.rng.fill_bytes(secret.as_mut());
        let deadline = self.now_ms.saturating_add(RING_MS);
        let offer = Signal {
            id,
            seq: 0,
            body: Body::Offer(media, sdp.to_vec(), secret.clone()),
        };
        let payload = offer.encode();
        let mut call = Call::new(id, &rel.id, media, true, deadline, secret);
        call.next_seq = 1;
        // No session acquisition, bootstrap or self-sync for an ephemeral offer.
        let devices = std::iter::once(rel.peer_device_id.clone()).chain(
            self.conv_devices(&rel)
                .into_iter()
                .filter_map(|(id, sibling)| (!sibling).then_some(id)),
        );
        for device in devices.filter_map(|id| h16(&id).ok()) {
            if self.queue_call(&rel.id, device, &id, &payload, deadline, DepositKind::Offer) {
                call.targets.push(ContactDevice {
                    id: device,
                    last_seq: None,
                    rejected: None,
                });
            }
        }
        if call.targets.is_empty() {
            call.finish(CallReason::Failed);
            self.st.calls.current = Some(call);
            return rejected(RejectReason::Unavailable);
        }
        self.st.calls.current = Some(call);
        OutcomeClass::Committed
    }

    fn queue_call(
        &mut self,
        conv: &str,
        device: [u8; 16],
        id: &[u8; 16],
        payload: &[u8],
        deadline_ms: u64,
        kind: DepositKind,
    ) -> bool {
        if self.st.calls.deposits.len() >= DEPOSIT_LIMIT || deadline_ms / 1000 <= self.now_s() {
            return false;
        }
        let target = hex(&device);
        if !self.ensure_dev(conv, &target, false) {
            return false;
        }
        self.with_dev(conv, &target, |tx| {
            let Some(rel) = tx.st.app.relationships.get(conv) else {
                return false;
            };
            if !tx.call_contact(rel) || !tx.cur_device_eligible(rel) {
                return false;
            }
            let (Ok((pd, route)), Ok(ik)) = (tx.peer_target(rel), h32(&rel.peer_ik)) else {
                return false;
            };
            let deadline_s = (deadline_ms / 1000).min(pd.public.expires_at);
            let deadline_s = if route.expires_at == 0 {
                deadline_s
            } else {
                deadline_s.min(route.expires_at)
            };
            if deadline_s <= tx.now_s() {
                return false;
            }
            let previous = tx.st.device.clone();
            let Some(Ok(inner)) = tx.st.device.as_mut().map(|d| d.encrypt_call(&ik, payload))
            else {
                return false;
            };
            let Ok(wrapper) = tx.seal_to_deadline(&pd, &route, &inner, deadline_s) else {
                tx.restore(previous);
                return false;
            };
            let net_id = tx.st.app.next_net_id;
            tx.st.app.next_net_id += 1;
            tx.st.calls.deposits.push(Deposit {
                id: net_id,
                call_id: *id,
                conversation: conv.into(),
                device,
                deadline_ms: deadline_s * 1000,
                kind,
                call: deposit_call(&route, wrapper),
            });
            true
        })
        .unwrap_or(false)
    }

    #[cfg(feature = "test-loopback")]
    pub(crate) fn debug_call_payload(
        &mut self,
        conv: &str,
        payload: &[u8],
    ) -> Result<(), CoreError> {
        let device = self
            .st
            .app
            .relationships
            .get(conv)
            .ok_or(CoreError::NotFound("conversation"))?;
        let device = h16(&device.peer_device_id)?;
        if self.queue_call(
            conv,
            device,
            &[0; 16],
            payload,
            self.now_ms + RING_MS,
            DepositKind::Other,
        ) {
            Ok(())
        } else {
            Err(CoreError::Invalid("call test deposit"))
        }
    }

    fn send_call(
        &mut self,
        call: &mut Call,
        body: Body,
        devices: &[[u8; 16]],
        kind: DepositKind,
    ) -> bool {
        let Some(next) = call.next_seq.checked_add(1) else {
            return false;
        };
        let payload = Signal {
            id: call.id,
            seq: call.next_seq,
            body,
        }
        .encode();
        let deadline = self.now_ms.saturating_add(RING_MS);
        let mut sent = false;
        for device in devices {
            sent |= self.queue_call(
                &call.view.conversation_id,
                *device,
                &call.id,
                &payload,
                deadline,
                kind,
            );
        }
        if sent {
            call.next_seq = next;
        }
        sent
    }

    fn take_current_call(&mut self, id: &str) -> Result<Call, RejectReason> {
        if !self.calls_enabled {
            return Err(RejectReason::Unsupported);
        }
        self.st
            .calls
            .current
            .take_if(|c| c.is_live() && c.view.call_session_id == id)
            .ok_or(RejectReason::NotFound)
    }

    pub(super) fn accept_call(&mut self, id: &str, sdp: &[u8], flags: u8) -> OutcomeClass {
        if !valid_sdp(sdp) || flags > 3 {
            return rejected(RejectReason::Invalid);
        }
        let mut call = match self.take_current_call(id) {
            Ok(call) => call,
            Err(reason) => return rejected(reason),
        };
        let result = if call.view.state != CallState::Incoming
            || (call.view.media == CallMedia::Audio && flags & 2 != 0)
        {
            rejected(RejectReason::Invalid)
        } else {
            let caller = call.targets[0].id;
            let me = self
                .st
                .device
                .as_ref()
                .expect("call has a local device")
                .device_id;
            if self.send_call(
                &mut call,
                Body::Answer(sdp.to_vec(), flags),
                &[caller],
                DepositKind::Answer,
            ) {
                if let Some(seen) = self
                    .st
                    .app
                    .call_offers_seen
                    .iter_mut()
                    .find(|offer| offer.id == id)
                {
                    seen.unanswered = false;
                }
                call.view.local_media_flags = flags;
                call.connect(&me, caller);
                OutcomeClass::Committed
            } else {
                call.finish(CallReason::Failed);
                rejected(RejectReason::Unavailable)
            }
        };
        self.st.calls.current = Some(call);
        result
    }

    pub(super) fn end_call(&mut self, id: &str, decline: bool) -> OutcomeClass {
        let mut call = match self.take_current_call(id) {
            Ok(call) => call,
            Err(reason) => return rejected(reason),
        };
        if decline && call.view.state != CallState::Incoming {
            self.st.calls.current = Some(call);
            return rejected(RejectReason::Invalid);
        }
        let reason = if decline {
            CallReason::Declined
        } else if call.selected.is_none() {
            CallReason::Canceled
        } else {
            CallReason::Hangup
        };
        self.terminate_call(&mut call, reason, true);
        self.st.calls.current = Some(call);
        OutcomeClass::Committed
    }

    pub(super) fn update_call(&mut self, id: &str, update: LocalUpdate<'_>) -> OutcomeClass {
        let mut call = match self.take_current_call(id) {
            Ok(call) => call,
            Err(reason) => return rejected(reason),
        };
        let result = match (&update, call.selected) {
            (LocalUpdate::Ice(sdp), Some(peer)) if valid_sdp(sdp) => {
                let sent = self.send_call(
                    &mut call,
                    Body::Ice(sdp.to_vec()),
                    &[peer],
                    DepositKind::Other,
                );
                if sent {
                    OutcomeClass::Committed
                } else {
                    rejected(RejectReason::Unavailable)
                }
            }
            (LocalUpdate::Media(flags), Some(peer)) if *flags <= 3 => {
                if self.send_call(&mut call, Body::Media(*flags), &[peer], DepositKind::Other) {
                    call.view.local_media_flags = *flags;
                    OutcomeClass::Committed
                } else {
                    rejected(RejectReason::Unavailable)
                }
            }
            _ => rejected(RejectReason::Invalid),
        };
        self.st.calls.current = Some(call);
        result
    }

    fn terminate_call(&mut self, call: &mut Call, reason: CallReason, tell_peer: bool) {
        self.st.calls.deposits.retain(|d| d.call_id != call.id);
        if tell_peer {
            let devices: Vec<_> = match call.selected {
                Some(device) => vec![device],
                None => call.targets.iter().map(|t| t.id).collect(),
            };
            let body = if reason == CallReason::Declined {
                Body::Reject(reason)
            } else {
                Body::End(reason)
            };
            self.send_call(call, body, &devices, DepositKind::Other);
        }
        call.finish(reason);
    }

    pub(super) fn receive_call(
        &mut self,
        conv: &str,
        sender: [u8; 16],
        plaintext: &[u8],
        expires_s: u64,
    ) {
        let Some(rel) = self
            .st
            .app
            .relationships
            .get(conv)
            .filter(|r| self.call_contact(r))
        else {
            return;
        };
        let account = rel.peer_account_id.clone();
        let Some(signal) = Signal::decode(plaintext) else {
            return;
        };
        if let Body::Offer(media, sdp, secret) = signal.body {
            if self.st.app.calls_disabled.contains(&account) {
                return;
            }
            self.receive_offer(
                conv, account, sender, signal.id, signal.seq, expires_s, media, sdp, secret,
            );
        } else if self.now_s() < expires_s {
            self.receive_call_reply(conv, sender, signal);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn receive_offer(
        &mut self,
        conv: &str,
        account: String,
        sender: [u8; 16],
        id: [u8; 16],
        seq: u32,
        expires_s: u64,
        media: CallMedia,
        sdp: Vec<u8>,
        secret: Zeroizing<[u8; 32]>,
    ) {
        let id_hex = hex(&id);
        if seq != 0
            || expires_s
                > self
                    .now_s()
                    .saturating_add(RING_MS / 1000 + delivery::CLOCK_SKEW)
            || self.st.app.call_offers_seen.len() >= SEEN_LIMIT
            || self
                .st
                .app
                .call_offers_seen
                .iter()
                .any(|offer| offer.id == id_hex && offer.account == account)
        {
            return;
        }

        let unanswered = self
            .st
            .app
            .call_offers_seen
            .iter()
            .filter(|offer| {
                offer.account == account
                    && offer.unanswered
                    && self.now_ms.saturating_sub(offer.at_ms) < SPAM_MS
            })
            .count();
        self.st.app.call_offers_seen.push(SeenOffer {
            id: id_hex,
            account,
            at_ms: self.now_ms,
            unanswered: false,
        });
        let mut incoming = Call::new(
            id,
            conv,
            media,
            false,
            self.now_ms.saturating_add(RING_MS),
            secret,
        );
        incoming.targets.push(ContactDevice {
            id: sender,
            last_seq: Some(seq),
            rejected: None,
        });
        if unanswered >= 3 {
            self.call_notice(&incoming, CallReason::Timeout, true);
            return;
        }
        if self.now_s() >= expires_s {
            self.call_notice(&incoming, CallReason::Timeout, false);
            // A delayed offer with a fresh sequence still counts toward the unanswered budget.
            self.st
                .app
                .call_offers_seen
                .last_mut()
                .expect("seen")
                .unanswered = true;
            return;
        }

        if let Some(mut current) = self.st.calls.current.take_if(|call| call.is_live()) {
            let glare = current.view.outgoing
                && current.selected.is_none()
                && current.view.conversation_id == conv;
            if glare && incoming.id < current.id {
                self.terminate_call(&mut current, CallReason::Canceled, true);
            } else {
                if !glare {
                    self.send_call(
                        &mut incoming,
                        Body::Reject(CallReason::Busy),
                        &[sender],
                        DepositKind::Other,
                    );
                    self.call_notice(&incoming, CallReason::Busy, false);
                }
                self.st.calls.current = Some(current);
                return;
            }
        }
        self.st
            .app
            .call_offers_seen
            .last_mut()
            .expect("seen")
            .unanswered = true;
        incoming.description = Some(CallDescription {
            kind: DescriptionKind::Offer,
            seq,
            sdp,
        });
        self.st.calls.current = Some(incoming);
    }

    fn receive_call_reply(&mut self, conv: &str, sender: [u8; 16], signal: Signal) {
        let Some(mut call) = self
            .st
            .calls
            .current
            .take_if(|c| c.is_live() && c.id == signal.id && c.view.conversation_id == conv)
        else {
            return;
        };
        self.apply_call_reply(&mut call, sender, signal);
        self.st.calls.current = Some(call);
    }

    fn apply_call_reply(&mut self, call: &mut Call, sender: [u8; 16], signal: Signal) {
        if call.selected.is_some_and(|s| s != sender) {
            return;
        }
        let Some(peer) = call.targets.iter_mut().find(|p| {
            p.id == sender
                && p.rejected.is_none()
                && p.last_seq.is_none_or(|prior| signal.seq > prior)
        }) else {
            return;
        };
        match signal.body {
            Body::Answer(sdp, flags)
                if call.view.outgoing
                    && call.selected.is_none()
                    && (call.view.media != CallMedia::Audio || flags & 2 == 0) =>
            {
                peer.last_seq = Some(signal.seq);
                let me = self
                    .st
                    .device
                    .as_ref()
                    .expect("call has a local device")
                    .device_id;
                call.connect(&me, sender);
                call.view.peer_media_flags = flags;
                call.description = Some(CallDescription {
                    kind: DescriptionKind::Answer,
                    seq: signal.seq,
                    sdp,
                });
                let others: Vec<_> = call
                    .targets
                    .iter()
                    .filter(|p| p.id != sender && p.rejected.is_none())
                    .map(|p| p.id)
                    .collect();
                self.st
                    .calls
                    .deposits
                    .retain(|d| d.call_id != call.id || d.kind != DepositKind::Offer);
                self.send_call(
                    call,
                    Body::End(CallReason::AnsweredElsewhere),
                    &others,
                    DepositKind::Other,
                );
            }
            Body::Reject(reason) if call.view.outgoing && call.selected.is_none() => {
                peer.rejected = Some(reason);
                self.finish_if_no_targets(call);
            }
            Body::End(reason)
                if call.view.outgoing
                    && call.selected.is_none()
                    && matches!(
                        reason,
                        CallReason::Timeout | CallReason::Failed | CallReason::Canceled
                    ) =>
            {
                peer.rejected = Some(reason);
                self.finish_if_no_targets(call);
            }
            Body::End(reason) if !call.view.outgoing || call.selected == Some(sender) => {
                if call.view.state == CallState::Incoming && reason != CallReason::AnsweredElsewhere
                {
                    self.call_notice(call, reason, false);
                }
                self.terminate_call(call, reason, false);
            }
            Body::Ice(sdp) if call.selected == Some(sender) => {
                peer.last_seq = Some(signal.seq);
                call.ice = Some(CallDescription {
                    kind: DescriptionKind::IceUpdate,
                    seq: signal.seq,
                    sdp,
                });
            }
            Body::Media(flags) if call.selected == Some(sender) => {
                peer.last_seq = Some(signal.seq);
                call.view.peer_media_flags = flags;
            }
            _ => {}
        }
    }

    fn finish_if_no_targets(&mut self, call: &mut Call) {
        if call.targets.iter().all(|t| t.rejected.is_some()) {
            let reason = [
                CallReason::Declined,
                CallReason::Busy,
                CallReason::Timeout,
                CallReason::Canceled,
            ]
            .into_iter()
            .find(|r| call.targets.iter().any(|t| t.rejected == Some(*r)))
            .unwrap_or(CallReason::Failed);
            self.terminate_call(call, reason, false);
        }
    }

    fn call_notice(&mut self, call: &Call, reason: CallReason, folded: bool) {
        let notices = &mut self.st.app.call_notices;
        if folded
            && notices.iter().any(|n| {
                n.folded
                    && n.conversation_id == call.view.conversation_id
                    && self.now_ms.saturating_sub(n.at_ms) < SPAM_MS
            })
        {
            return;
        }
        if notices.len() == NOTICE_LIMIT {
            notices.remove(0);
        }
        notices.push(CallNotice {
            call_session_id: call.view.call_session_id.clone(),
            conversation_id: call.view.conversation_id.clone(),
            reason,
            at_ms: self.now_ms,
            folded,
        });
    }

    pub(crate) fn maintain_calls(&mut self) {
        let clock_rolled_back = self
            .st
            .calls
            .last_clock_ms
            .replace(self.now_ms)
            .is_some_and(|last| last > self.now_ms);
        if clock_rolled_back {
            self.st.calls.deposits.clear();
        }
        self.st
            .app
            .call_offers_seen
            .retain(|offer| self.now_ms.saturating_sub(offer.at_ms) < REPLAY_MS);
        if let Some(mut call) = self.st.calls.current.take_if(|c| c.is_live()) {
            let rel = self.st.app.relationships.get(&call.view.conversation_id);
            let still_allowed = rel.is_some_and(|rel| {
                self.call_contact(rel)
                    && (call.view.state != CallState::Incoming
                        || !self.st.app.calls_disabled.contains(&rel.peer_account_id))
                    && call.selected.is_none_or(|id| {
                        eligible_call_device(self.st, &call.view.conversation_id, id)
                    })
                    && (call.view.outgoing
                        || eligible_call_device(
                            self.st,
                            &call.view.conversation_id,
                            call.targets[0].id,
                        ))
            });
            if !still_allowed {
                self.terminate_call(&mut call, CallReason::Failed, false);
            } else if call
                .view
                .ring_deadline_ms
                .is_some_and(|d| clock_rolled_back || d <= self.now_ms)
            {
                if !call.view.outgoing {
                    self.call_notice(&call, CallReason::Timeout, false);
                }
                self.terminate_call(&mut call, CallReason::Timeout, !clock_rolled_back);
            }
            self.st.calls.current = Some(call);
        }
        self.st
            .calls
            .deposits
            .retain(|d| d.deadline_ms > self.now_ms);
    }

    pub(crate) fn apply_media_state(&mut self, id: &str, state: MediaState) {
        let Ok(mut call) = self.take_current_call(id) else {
            return;
        };
        match state {
            MediaState::Active
                if call.keys.is_none()
                    && matches!(
                        call.view.state,
                        CallState::Connecting | CallState::Reconnecting
                    ) =>
            {
                call.view.state = CallState::Active;
            }
            MediaState::Interrupted if call.view.state == CallState::Active => {
                call.view.state = CallState::Reconnecting;
            }
            MediaState::Failed if call.selected.is_some() => {
                self.terminate_call(&mut call, CallReason::Failed, true)
            }
            _ => {}
        }
        self.st.calls.current = Some(call);
    }

    pub(crate) fn apply_call_route(&mut self, id: &str, route: CallRoute) {
        if let Some(call) = self
            .st
            .calls
            .current
            .as_mut()
            .filter(|c| c.is_live() && c.view.call_session_id == id && c.selected.is_some())
        {
            call.view.route = Some(route);
        }
    }

    pub(super) fn call_net_result(&mut self, id: u64, result: &NetResult) {
        let Some(pos) = self.st.calls.deposits.iter().position(|d| d.id == id) else {
            return;
        };
        let sent = self.st.calls.deposits.remove(pos);
        if !matches!(result, NetResult::Err { error } if *error != NetError::Timeout) {
            return;
        }
        let Some(mut call) = self
            .st
            .calls
            .current
            .take_if(|c| c.is_live() && c.id == sent.call_id)
        else {
            return;
        };
        match sent.kind {
            DepositKind::Offer if call.selected.is_none() => {
                if let Some(target) = call.targets.iter_mut().find(|t| t.id == sent.device) {
                    target.rejected = Some(CallReason::Failed);
                }
                self.finish_if_no_targets(&mut call);
            }
            DepositKind::Answer => self.terminate_call(&mut call, CallReason::Failed, false),
            _ => {}
        }
        self.st.calls.current = Some(call);
    }
}

pub(crate) fn eligible_call_device(
    st: &crate::state::ProfileState,
    conversation: &str,
    id: [u8; 16],
) -> bool {
    st.app.relationships.get(conversation).is_some_and(|rel| {
        rel.state == RelState::Active
            && !rel.blocked
            && !rel.peer_recovery.iter().any(PeerRecoveryRec::is_live)
            && h32(&rel.peer_account_id)
                .ok()
                .zip(st.device.as_ref())
                .is_some_and(|(account, dev)| {
                    dev.peer_devices
                        .iter()
                        .any(|p| p.1 == account && p.2 == id && p.3)
                })
    })
}

fn rejected(reason: RejectReason) -> OutcomeClass {
    OutcomeClass::Rejected { reason }
}
