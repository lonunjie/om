// SPDX-License-Identifier: MPL-2.0
//! Everyday chat state that other events attach to a message (MESSENGER_BASELINE_V1, EVENT_MODEL): reactions today. Sending
//! queues a canonical `ConversationEvent` exactly like a message (same ratchet, fan-out and retry) as a `lane` entry that
//! never shows in the timeline; receiving applies the event to the message it names, in this conversation only.

use super::*;

/// A reaction key is a Unicode token of 1 to 64 bytes (`ReactionSet` in events.cddl).
pub const MAX_REACTION_KEY_BYTES: usize = 64;
/// Registers kept per person and message (adds and removals together). A bound on what a peer can make us store
/// (ADR-0229 section 5).
pub const MAX_REACTION_REGISTERS: usize = 64;
/// Present reactions shown per person and message: the present registers with the highest order (ADR-0229 section 5).
pub const MAX_REACTIONS_SHOWN: usize = 16;

/// The order of a register: `(sending epoch, EventClock physical, EventClock logical)`; a one-to-one chat has no epoch.
pub(crate) type RegOrder = (u64, u64, u32);

/// Admit one event into a bounded register set by order, never by arrival (ADR-0229 section 5 items 2 to 4). `in_scope`
/// selects the registers of this actor, `is_key` the register of the event's key. A register for the key is replaced only by
/// a higher order. Otherwise it is inserted when fewer than `MAX_REACTION_REGISTERS` are held, or, when full, evicts the
/// lowest-ordered register only if the event is ordered above it; an event at or below the lowest retained order is
/// refused, whether an add or a removal. The retained set is the registers with the highest order of their latest event,
/// a function of the set of events and not of their arrival order. Returns whether the set changed.
pub(crate) fn admit_register<T>(
    regs: &mut Vec<T>,
    in_scope: impl Fn(&T) -> bool,
    is_key: impl Fn(&T) -> bool,
    ord: impl Fn(&T) -> RegOrder,
    new: T,
) -> bool {
    let new_ord = ord(&new);
    if let Some(i) = regs.iter().position(|r| in_scope(r) && is_key(r)) {
        if new_ord > ord(&regs[i]) {
            regs[i] = new;
            return true;
        }
        return false;
    }
    let held = regs.iter().filter(|r| in_scope(r)).count();
    if held >= MAX_REACTION_REGISTERS {
        let Some(low) = regs
            .iter()
            .enumerate()
            .filter(|(_, r)| in_scope(r))
            .min_by_key(|(_, r)| ord(r))
            .map(|(i, _)| i)
        else {
            return false;
        };
        if new_ord <= ord(&regs[low]) {
            return false;
        }
        regs.remove(low);
    }
    regs.push(new);
    true
}

/// Mark the registers that are shown: of the present ones of each actor (`actor_of`), the `MAX_REACTIONS_SHOWN` with the
/// highest order. Returns one flag per register.
pub(crate) fn shown_flags<T>(
    regs: &[T],
    actor_of: impl Fn(&T) -> String,
    present: impl Fn(&T) -> bool,
    ord: impl Fn(&T) -> RegOrder,
) -> Vec<bool> {
    let mut flags = vec![false; regs.len()];
    let mut by_actor: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
    for (i, r) in regs.iter().enumerate() {
        if present(r) {
            by_actor.entry(actor_of(r)).or_default().push(i);
        }
    }
    for idx in by_actor.values_mut() {
        idx.sort_by_key(|&i| std::cmp::Reverse(ord(&regs[i])));
        for &i in idx.iter().take(MAX_REACTIONS_SHOWN) {
            flags[i] = true;
        }
    }
    flags
}

fn rec_order(r: &ReactionRec) -> RegOrder {
    (0, r.clock_ms, r.clock_logical)
}

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
    admit_register(
        recs,
        |r| r.mine == mine,
        |r| r.key == key,
        rec_order,
        ReactionRec {
            mine,
            key: key.to_string(),
            present,
            clock_ms: clock.physical_ms,
            clock_logical: clock.logical,
        },
    );
    true
}

/// Apply a received `message.edit` or `message.delete` to the message it names. `authored_by_us`: the author is one of our
/// own devices (so the target is a message we sent). The edit register takes the event only if ordered after what is held;
/// a delete is final. A target that is not here, or not the author's, is accepted with no effect; a malformed edit is
/// refused.
pub(super) fn apply_edit_delete(
    msgs: &mut [Msg],
    authored_by_us: bool,
    ev: &events::Event,
) -> bool {
    let Some(id) = ev
        .body
        .get(0)
        .ok()
        .and_then(|b| b.b().ok())
        .filter(|b| b.len() == 16)
        .map(|b| format!("msg-{}", hex(b)))
    else {
        return false;
    };
    let Some(m) = msgs
        .iter_mut()
        .find(|m| m.id == id && m.outgoing == authored_by_us && !m.lane)
    else {
        return true;
    };
    if m.deleted {
        return true;
    }
    if ev.kind == events::K_DELETE {
        m.deleted = true;
        m.text.clear();
        m.social.reactions.clear();
        return true;
    }
    if (ev.clock.physical_ms, ev.clock.logical) <= (m.clock_ms, m.clock_logical) {
        return true;
    }
    let Some(text) = ev
        .body
        .get(1)
        .ok()
        .and_then(events::plain_text)
        .filter(|t| t.chars().count() <= MAX_MESSAGE_CHARS)
    else {
        return false;
    };
    if text.trim().is_empty() && m.attachments.is_empty() {
        return true; // nothing left to show: no effect (the author deletes instead)
    }
    m.text = text;
    m.clock_ms = ev.clock.physical_ms;
    m.clock_logical = ev.clock.logical;
    m.social.edited = true;
    true
}

/// The reactions of a message as a person sees them: each key once, how many have it set, and whether one is ours.
pub(crate) fn reaction_views(m: &Msg) -> Vec<ReactionView> {
    let mut out: Vec<ReactionView> = vec![];
    let flags = shown_flags(
        &m.social.reactions,
        |r| r.mine.to_string(),
        |r| r.present,
        rec_order,
    );
    for (r, _) in m
        .social
        .reactions
        .iter()
        .zip(flags)
        .filter(|(_, shown)| *shown)
    {
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

impl AppState {
    pub(super) fn direct_mutation(
        &mut self,
        conv: &str,
        mine: bool,
        ev: &events::Event,
        bytes: &[u8],
        now_ms: u64,
    ) -> bool {
        let Some(target) = events::mutation_target(ev) else {
            return false;
        };
        let valid = match ev.kind {
            events::K_EDIT => ev
                .body
                .opt(1)
                .and_then(events::plain_text)
                .is_some_and(|text| text.chars().count() <= MAX_MESSAGE_CHARS),
            events::K_REACTION => read_reaction(ev).is_some(),
            _ => true,
        };
        if !valid {
            return false;
        }
        let Some(rel) = self.relationships.get_mut(conv) else {
            return false;
        };
        let eid = hex(&target);
        let id = format!("msg-{eid}");
        if rel.equivocated.contains(&eid) {
            return true;
        }
        if rel.messages.iter().any(|m| m.id == id) {
            return apply_direct_mutation(rel, ev, mine);
        }
        self.dangling_mutations.hold(
            conv,
            target,
            ev,
            events::MutationAuth::Direct { mine },
            bytes,
            now_ms,
        );
        true
    }

    pub(super) fn resolve_direct_mutations(&mut self, conv: &str, target: [u8; 16], now_ms: u64) {
        let Some(rel) = self.relationships.get_mut(conv) else {
            return;
        };
        for (event, auth) in self.dangling_mutations.take(conv, target, false, now_ms) {
            if let events::MutationAuth::Direct { mine } = auth
                && rel.canon_id.as_deref() == Some(&hex(&event.conversation_id))
                && (mine || rel.peer_account_id == hex(&event.author_account))
            {
                apply_direct_mutation(rel, &event, mine);
            }
        }
    }
}

fn apply_direct_mutation(rel: &mut Relationship, ev: &events::Event, mine: bool) -> bool {
    if ev.kind == events::K_REACTION {
        if let Some((target, key, present)) = read_reaction(ev) {
            apply_reaction(&mut rel.messages, &target, mine, &key, present, ev.clock);
        }
        true
    } else {
        apply_edit_delete(&mut rel.messages, mine, ev)
    }
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

    /// One of our own messages that an edit or delete may name (see [`Self::event_target`]).
    fn own_target(&self, conv: &str, message_id: &str) -> Option<[u8; 16]> {
        let rel = self.st.app.relationships.get(conv)?;
        rel.messages
            .iter()
            .any(|m| m.id == message_id && m.outgoing && !m.lane)
            .then(|| self.event_target(conv, message_id))?
    }

    pub(super) fn edit_message(
        &mut self,
        op: &OperationId,
        conv: &str,
        message_id: &str,
        text: &str,
    ) -> Result<(), CoreError> {
        let kind = "EditMessage";
        let reject = |s: &mut Self, reason| {
            s.record(op, kind, OutcomeClass::Rejected { reason }, None);
            Ok(())
        };
        if text.chars().count() > MAX_MESSAGE_CHARS {
            return reject(self, RejectReason::Invalid);
        }
        let Some(target) = self.own_target(conv, message_id) else {
            return reject(self, RejectReason::NotFound);
        };
        let has_attachments = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.messages.iter().find(|m| m.id == message_id))
            .is_some_and(|m| !m.attachments.is_empty());
        if text.trim().is_empty() && !has_attachments {
            return reject(self, RejectReason::Invalid);
        }
        let body = V::M(vec![
            (0, V::B(target.to_vec())),
            (1, events::rich_text(text)),
        ]);
        let Some((_, clock)) = self.queue_event(op, kind, conv, events::K_EDIT, body)? else {
            return Ok(());
        };
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == message_id))
        {
            m.text = text.to_string();
            m.clock_ms = clock.physical_ms;
            m.clock_logical = clock.logical;
            m.social.edited = true;
        }
        Ok(())
    }

    pub(super) fn delete_message(
        &mut self,
        op: &OperationId,
        conv: &str,
        message_id: &str,
    ) -> Result<(), CoreError> {
        let kind = "DeleteMessage";
        let Some(target) = self.own_target(conv, message_id) else {
            self.record(
                op,
                kind,
                OutcomeClass::Rejected {
                    reason: RejectReason::NotFound,
                },
                None,
            );
            return Ok(());
        };
        let body = V::M(vec![(0, V::B(target.to_vec()))]);
        if self
            .queue_event(op, kind, conv, events::K_DELETE, body)?
            .is_none()
        {
            return Ok(());
        }
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == message_id))
        {
            m.deleted = true;
            m.text.clear();
            m.social.reactions.clear();
        }
        Ok(())
    }

    /// Whether adding our present reaction `key` would make more than `MAX_REACTIONS_SHOWN` of ours present on the message.
    fn own_reaction_would_exceed(&self, conv: &str, message_id: &str, key: &str) -> bool {
        let Some(m) = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.messages.iter().find(|m| m.id == message_id))
        else {
            return false;
        };
        let mine = &m.social.reactions;
        !mine.iter().any(|r| r.mine && r.present && r.key == key)
            && mine.iter().filter(|r| r.mine && r.present).count() >= MAX_REACTIONS_SHOWN
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
        if present && self.own_reaction_would_exceed(conv, message_id, key) {
            return reject(self, RejectReason::Invalid); // ADR-0229 s5: no seventeenth present reaction
        }
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

#[cfg(test)]
mod register_tests {
    //! ADR-0229 section 5: the register set is the 64 keys with the highest order of their latest event, whatever the delivery
    //! order, for both reducers; at most 16 present reactions per member are shown.
    use super::*;
    use crate::groups::flow::GroupReactionRec;

    /// `(key index, present, order)`: 70 keys, each added and some later removed, plus a removal that churn evicts and an
    /// earlier add of the same key that arrives after it.
    fn events() -> Vec<(usize, bool, u64)> {
        let mut v = vec![];
        for k in 0..70usize {
            v.push((k, true, 10 + k as u64));
        }
        for k in (0..70usize).step_by(3) {
            v.push((k, false, 200 + k as u64));
        }
        // key 100: removed at 5 and added at 4 (the add is older than the removal and than the floor once churned)
        v.push((100, false, 5));
        v.push((100, true, 4));
        v
    }

    fn shuffled(mut v: Vec<(usize, bool, u64)>, seed: u64) -> Vec<(usize, bool, u64)> {
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for i in (1..v.len()).rev() {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            v.swap(i, (x >> 33) as usize % (i + 1));
        }
        v
    }

    fn run_1to1(order: &[(usize, bool, u64)]) -> Vec<(String, bool, u64)> {
        let mut regs: Vec<ReactionRec> = vec![];
        for &(k, present, o) in order {
            admit_register(
                &mut regs,
                |r| r.mine,
                |r| r.key == format!("k{k}"),
                rec_order,
                ReactionRec {
                    mine: true,
                    key: format!("k{k}"),
                    present,
                    clock_ms: o,
                    clock_logical: 0,
                },
            );
        }
        assert!(regs.len() <= MAX_REACTION_REGISTERS);
        let mut out: Vec<_> = regs.iter().map(|r| (r.key.clone(), r.present, r.clock_ms)).collect();
        out.sort();
        out
    }

    fn run_group(order: &[(usize, bool, u64)]) -> Vec<(String, bool, u64)> {
        let mut regs: Vec<GroupReactionRec> = vec![];
        for &(k, present, o) in order {
            admit_register(
                &mut regs,
                |r| r.account == "a",
                |r| r.key == format!("k{k}"),
                |r| (r.epoch, r.clock_ms, r.clock_logical),
                GroupReactionRec {
                    account: "a".into(),
                    key: format!("k{k}"),
                    present,
                    mine: false,
                    epoch: 0,
                    clock_ms: o,
                    clock_logical: 0,
                },
            );
        }
        assert!(regs.len() <= MAX_REACTION_REGISTERS);
        let mut out: Vec<_> = regs.iter().map(|r| (r.key.clone(), r.present, r.clock_ms)).collect();
        out.sort();
        out
    }

    #[test]
    fn same_events_in_any_order_keep_the_same_registers() {
        let base = events();
        let want1 = run_1to1(&base);
        let wantg = run_group(&base);
        assert_eq!(want1, wantg);
        assert_eq!(want1.len(), MAX_REACTION_REGISTERS);
        for seed in 0..200u64 {
            let p = shuffled(base.clone(), seed);
            assert_eq!(run_1to1(&p), want1, "1:1 seed {seed}");
            assert_eq!(run_group(&p), wantg, "group seed {seed}");
        }
        let mut rev = base.clone();
        rev.reverse();
        assert_eq!(run_1to1(&rev), want1);
    }

    #[test]
    fn an_older_event_than_the_lowest_retained_is_refused_add_or_removal() {
        let mut regs: Vec<ReactionRec> = vec![];
        let put = |regs: &mut Vec<ReactionRec>, k: usize, present: bool, o: u64| {
            admit_register(
                regs,
                |r| r.mine,
                |r| r.key == format!("k{k}"),
                rec_order,
                ReactionRec { mine: true, key: format!("k{k}"), present, clock_ms: o, clock_logical: 0 },
            )
        };
        for k in 0..64 {
            assert!(put(&mut regs, k, true, 100 + k as u64));
        }
        assert!(!put(&mut regs, 900, true, 50), "older add refused");
        assert!(!put(&mut regs, 901, false, 50), "older removal refused");
        assert!(put(&mut regs, 902, false, 500), "newer removal evicts the lowest");
        assert!(!regs.iter().any(|r| r.key == "k0"));
        assert!(!put(&mut regs, 0, true, 100), "the evicted key's old add stays refused");
    }

    #[test]
    fn only_the_sixteen_highest_present_registers_are_shown_per_actor() {
        let mut regs: Vec<ReactionRec> = vec![];
        for k in 0..40usize {
            admit_register(
                &mut regs,
                |r| r.mine,
                |r| r.key == format!("k{k}"),
                rec_order,
                ReactionRec { mine: true, key: format!("k{k}"), present: true, clock_ms: k as u64, clock_logical: 0 },
            );
        }
        let flags = shown_flags(&regs, |r| r.mine.to_string(), |r| r.present, rec_order);
        assert_eq!(flags.iter().filter(|f| **f).count(), MAX_REACTIONS_SHOWN);
        for (r, f) in regs.iter().zip(flags) {
            assert_eq!(f, r.clock_ms >= 24);
        }
    }
}
