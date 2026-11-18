// SPDX-License-Identifier: MPL-2.0
//! ADR-0207: an authenticated contact can request an ordinary invitation back to a Room.

use super::carriage::{LANE_OUT_TTL_S, MAX_LANE_IN, MAX_LANE_OUT, envelope_body};
use super::endpoint::active_in;
use super::flow::{
    DeviceRelease, GroupCtx, GroupEvent, GroupInviteRec, GroupReply, GroupWire, InviteState,
    Outbound, Target, device_intent_body,
};
use super::journal::CommitPlan;
use super::{GroupError, GroupState, Intent, Principal, R, b, err, op, req, target_of, u};
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::{hex, sha256, unhex};
use crate::recovery::RecoveryRequest;
use crate::state::{AppState, RecoveryLinkRec, RelState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejoinState {
    pub requests: Vec<PendingRejoin>,
    pub reviews: Vec<RejoinReview>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    closed: Vec<ClosedRejoin>,
}

/// One decision per current Room membership and verified recovery successor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ClosedRejoin {
    id: String,
    room: String,
    relationship: String,
    device: String,
}

impl From<RejoinReview> for ClosedRejoin {
    fn from(r: RejoinReview) -> Self {
        Self {
            id: r.id,
            room: r.room,
            relationship: r.relationship,
            device: r.device,
        }
    }
}

fn review_id(
    room: &[u8],
    account: &[u8],
    device: &[u8],
    head: &[u8],
    incarnation: &[u8],
) -> String {
    hex(&sha256(&[room, account, device, head, incarnation]))
}

fn invite_lapsed(invite: &GroupInviteRec, now_ms: u64) -> bool {
    matches!(invite.state, InviteState::Offered | InviteState::Accepted)
        && now_ms >= invite.created_ms.saturating_add(LANE_OUT_TTL_S * 1000)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRejoin {
    pub relationship_id: String,
    pub room_hint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejoinReview {
    id: String,
    room: String,
    incarnation: Vec<u8>,
    relationship: String,
    account: String,
    device: String,
    head: String,
    removal: Option<String>,
    invite: Option<String>,
    declined: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejoinReviewView {
    pub id: String,
    pub conversation_id: String,
    pub relationship_id: String,
    pub working: bool,
    pub can_invite: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejoinView {
    pub contacts: Vec<String>,
    pub pending: Vec<PendingRejoin>,
    pub reviews: Vec<RejoinReviewView>,
}

fn bytes(s: &str) -> R<Vec<u8>> {
    unhex(s).map_err(|_| GroupError("malformed"))
}

fn recovered_device(link: &RecoveryLinkRec, head: &Acs, device: &[u8]) -> bool {
    link.child_hash == hex(&head.hash())
        && bytes(&link.request)
            .and_then(|wire| envelope_body(&wire))
            .ok()
            .and_then(|body| RecoveryRequest::from_body(&body).ok())
            .is_some_and(|r| r.account_id == head.account_id && r.new_device_id == device)
        && !crate::control::safety_frozen(head)
}

impl AppState {
    pub(crate) fn rejoin_own_head(&self, account: &[u8], device: &[u8]) -> Option<String> {
        let head = self.carry_own_tip().ok()?;
        (head.account_id == account
            && self
                .recovery_links
                .last()
                .is_some_and(|l| recovered_device(l, &head, device)))
        .then(|| hex(&head.hash()))
    }

    pub(crate) fn rejoin_peer_head(&self, rel: &str, account: &[u8], device: &[u8]) -> R<Acs> {
        let r = self
            .relationships
            .get(rel)
            .filter(|r| {
                r.state == RelState::Active && !r.blocked && r.peer_account_id == hex(account)
            })
            .ok_or(GroupError("relationship_not_active"))?;
        let head =
            Acs::from_body(&bytes(&r.peer_acs_body)?).map_err(|_| GroupError("stale_rejoin"))?;
        if !r
            .peer_links
            .last()
            .is_some_and(|l| recovered_device(l, &head, device))
            || !self
                .group_heads()
                .iter()
                .any(|(best, _)| best.hash() == head.hash())
            || !r.peer_credentials.iter().any(|s| {
                bytes(s)
                    .ok()
                    .and_then(|b| Credential::from_body(&b).ok())
                    .is_some_and(|c| c.device_id == device && active_in(&head, &c))
            })
        {
            return err("stale_rejoin");
        }
        Ok(head)
    }

    pub(super) fn group_ask_to_rejoin(
        &mut self,
        ctx: &GroupCtx<'_>,
        relationship: &str,
        room_hint: String,
        rep: &mut GroupReply,
    ) -> R<()> {
        let device = ctx.engine.device_id();
        let head = self
            .rejoin_own_head(&ctx.my_account, device)
            .ok_or(GroupError("recovery_required"))?;
        if !self.rejoin_contact_usable(relationship) {
            return err("relationship_not_active");
        }
        if room_hint.trim().is_empty() || room_hint.len() > 256 {
            return err("room_name_invalid");
        }
        let request = PendingRejoin {
            relationship_id: relationship.into(),
            room_hint,
        };
        let pending = &mut self.group_carriage.rejoin.requests;
        if pending.contains(&request) {
            return Ok(());
        }
        if pending.len() >= MAX_LANE_OUT {
            return err("lane_out_full");
        }
        rep.out.push(Outbound {
            to: Target::Relationship(relationship.into()),
            wire: GroupWire::RejoinRequest {
                account: hex(&ctx.my_account),
                room_hint: request.room_hint.clone(),
                device: hex(device),
                head,
            },
        });
        pending.push(request);
        rep.events.push(GroupEvent::RejoinChanged);
        Ok(())
    }

    fn rejoin_contact_usable(&self, rel: &str) -> bool {
        self.relationships.get(rel).is_some_and(|r| {
            r.state == RelState::Active
                && !r.blocked
                && r.peer_delivery.is_some()
                && r.peer_relays.primary().is_some()
        })
    }

    pub(super) fn group_record_rejoin(
        &mut self,
        ctx: &GroupCtx<'_>,
        relationship: String,
        account: String,
        device: String,
        head: String,
        hint: &str,
        rep: &mut GroupReply,
    ) -> R<()> {
        let (who, did) = (bytes(&account)?, bytes(&device)?);
        let pinned = self.rejoin_peer_head(&relationship, &who, &did)?;
        if head != hex(&pinned.hash()) {
            return err("stale_rejoin");
        }
        let member = Principal::account(who);
        let me = Principal::account(ctx.my_account.clone());
        let my_device = hex(ctx.engine.device_id());
        let mut matches = self
            .groups
            .values()
            .filter_map(|r| r.open().ok())
            .filter(|g| {
                g.profile.name == hint
                    && g.member(&member).is_some()
                    && g.member(&me).is_some()
                    && self
                        .group_devices
                        .get(&hex(&g.conversation_id))
                        .is_some_and(|d| d.contains_key(&my_device) && !d.contains_key(&device))
            });
        let Some(room) = matches.next() else {
            return Ok(());
        };
        if matches.next().is_some() {
            return Ok(());
        }
        let incarnation = room.member(&member).expect("matched").incarnation.clone();
        let id = review_id(
            &room.conversation_id,
            &member.id,
            &did,
            &pinned.hash(),
            &incarnation,
        );
        let mut prior = std::mem::take(&mut self.group_carriage.rejoin.reviews);
        prior.retain(|review| {
            bytes(&review.room)
                .ok()
                .and_then(|conv| self.group_state(&conv).ok())
                .is_some_and(|room| self.rejoin_review_current(review, &room).is_ok())
        });
        self.group_carriage.rejoin.reviews = prior;
        let state = &mut self.group_carriage.rejoin;
        if state.reviews.iter().any(|r| r.id == id) || state.closed.iter().any(|r| r.id == id) {
            return Ok(());
        }
        let reviews = &mut state.reviews;
        reviews.retain(|r| r.account != account || r.head == head);
        if reviews.iter().filter(|r| r.invite.is_none()).count() >= MAX_LANE_IN {
            return err("lane_full");
        }
        reviews.push(RejoinReview {
            id,
            room: hex(&room.conversation_id),
            incarnation,
            relationship,
            account,
            device,
            head,
            removal: None,
            invite: None,
            declined: false,
        });
        rep.events.push(GroupEvent::RejoinChanged);
        Ok(())
    }

    fn closed_rejoin_current(&self, r: &ClosedRejoin) -> Option<bool> {
        let conv = bytes(&r.room).ok()?;
        let device = bytes(&r.device).ok()?;
        let rel = self.relationships.get(&r.relationship)?;
        let account = bytes(&rel.peer_account_id).ok()?;
        let room = self.group_state(&conv).ok()?;
        let member = room.member(&Principal::account(account.clone()))?;
        let head = Acs::from_body(&bytes(&rel.peer_acs_body).ok()?).ok()?;
        Some(r.id == review_id(&conv, &account, &device, &head.hash(), &member.incarnation))
    }

    pub(crate) fn maintain_rejoin_invites(&mut self, now_ms: u64) {
        let state = &mut self.group_carriage.rejoin;
        if state.reviews.is_empty() && state.closed.is_empty() {
            return;
        }
        let mut reviews = std::mem::take(&mut state.reviews);
        let mut closed = std::mem::take(&mut state.closed);
        closed.retain(|r| self.closed_rejoin_current(r) == Some(true));
        reviews.retain_mut(|r| {
            if r.declined
                || r.invite
                    .as_ref()
                    .and_then(|id| self.group_invites.get(id))
                    .is_some_and(|i| i.state == InviteState::Done)
            {
                let tombstone = ClosedRejoin::from(r.clone());
                if self.closed_rejoin_current(&tombstone) == Some(true) {
                    closed.push(tombstone);
                }
                return false;
            }
            if let Some(id) = &r.invite
                && let Some(invite) = self.group_invites.get_mut(id)
                && (invite.state == InviteState::Declined || invite_lapsed(invite, now_ms))
            {
                invite.state = InviteState::Declined;
                r.invite = None;
            }
            true
        });
        self.group_carriage.rejoin.reviews = reviews;
        self.group_carriage.rejoin.closed = closed;
    }

    fn rejoin_review_current(&self, r: &RejoinReview, room: &GroupState) -> R<Acs> {
        let who = Principal::account(bytes(&r.account)?);
        let head = self.rejoin_peer_head(&r.relationship, &who.id, &bytes(&r.device)?)?;
        if r.head != hex(&head.hash())
            || room.member(&who).map(|m| &m.incarnation) != Some(&r.incarnation)
        {
            return err("stale_rejoin");
        }
        Ok(head)
    }

    pub(super) fn group_resolve_rejoin(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        id: &str,
        agree: bool,
        rep: &mut GroupReply,
    ) -> R<()> {
        let i = self
            .group_carriage
            .rejoin
            .reviews
            .iter()
            .position(|r| r.id == id && !r.declined && r.invite.is_none())
            .ok_or(GroupError("unknown_rejoin"))?;
        let review = &self.group_carriage.rejoin.reviews[i];
        if self.rejoin_removal_open(review) {
            return err("journal_busy");
        }
        if !agree {
            let state = &mut self.group_carriage.rejoin;
            state.closed.push(state.reviews.remove(i).into());
            rep.events.push(GroupEvent::RejoinChanged);
            return Ok(());
        }
        self.group_advance_rejoin(ctx, i, rep)
    }

    fn rejoin_removal_open(&self, r: &RejoinReview) -> bool {
        r.removal
            .as_ref()
            .and_then(|id| self.group_journal.get(id))
            .is_some_and(|e| !e.state.is_terminal())
    }

    fn group_advance_rejoin(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        i: usize,
        rep: &mut GroupReply,
    ) -> R<()> {
        let r = self.group_carriage.rejoin.reviews[i].clone();
        let conv = bytes(&r.room)?;
        let room = self.group_state(&conv)?;
        let acs = self.rejoin_review_current(&r, &room)?;
        let me = Principal::account(ctx.my_account.clone());
        check_inviter(&room, &me)?;
        self.rejoin_coordinator(&room, ctx.engine.device_id(), &me)?;
        let who = Principal::account(bytes(&r.account)?);
        if self.group_dir(&conv).contains_key(&r.device) {
            return err("already_member");
        }
        if let Some(old) = self
            .group_devices_of(&conv, &who.id)
            .into_iter()
            .find(|d| device_in(&acs.revoked, d) || device_in(&acs.quarantined, d))
        {
            let body = device_intent_body(
                &room,
                ctx.rng,
                op::DEVICE_REMOVE_REVOKED,
                &who,
                &r.incarnation,
                &old,
                &acs,
            );
            let my_dev = ctx.engine.device_id().to_vec();
            let actor = ctx.my_account.clone();
            let id = self.group_prepare_intent(
                ctx,
                &body,
                &CommitPlan::Remove(vec![old]),
                &actor,
                None,
            )?;
            rep.out
                .extend(self.group_commit_wires(&conv, &my_dev, &id, &my_dev, vec![])?);
            rep.events.push(GroupEvent::CommitPrepared {
                intent_id: id.clone(),
            });
            self.group_carriage.rejoin.reviews[i].removal = Some(id);
        } else {
            let invite_id = self.group_invite(ctx, &conv, &r.relationship, Some(&r.id), rep)?;
            let r = &mut self.group_carriage.rejoin.reviews[i];
            r.invite = Some(invite_id);
            r.removal = None;
        }
        rep.events.push(GroupEvent::RejoinChanged);
        Ok(())
    }

    /// Only the accepted removal releases the ordinary invitation (or prepares the next revoked leaf).
    pub(super) fn group_rejoin_after_removal(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        removal: &str,
        rep: &mut GroupReply,
    ) -> bool {
        let Some(i) = self
            .group_carriage
            .rejoin
            .reviews
            .iter()
            .position(|r| r.removal.as_deref() == Some(removal))
        else {
            return false;
        };
        self.group_carriage.rejoin.reviews[i].removal = None;
        if self.group_advance_rejoin(ctx, i, rep).is_err() {
            rep.events.push(GroupEvent::RejoinChanged);
        }
        rep.events
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. }))
    }

    pub(super) fn group_rejoin_add(
        &self,
        ctx: &mut GroupCtx<'_>,
        room: &GroupState,
        inv: &GroupInviteRec,
        releases: &[DeviceRelease],
    ) -> R<Vec<u8>> {
        let r = self
            .group_carriage
            .rejoin
            .reviews
            .iter()
            .find(|r| {
                Some(&r.id) == inv.rejoin.as_ref()
                    && r.invite.as_ref() == Some(&inv.id)
                    && r.room == inv.conversation_id
                    && r.account == inv.peer_account
                    && r.relationship == inv.relationship_id
            })
            .ok_or(GroupError("unknown_rejoin"))?;
        let acs = self.rejoin_review_current(r, room)?;
        let me = Principal::account(ctx.my_account.clone());
        self.rejoin_coordinator(room, ctx.engine.device_id(), &me)?;
        check_inviter(room, &me)?;
        if releases.len() != 1 || releases[0].device != r.device {
            return err("release_mismatch");
        }
        if self.group_has_revoked_leaf(&room.conversation_id) {
            return err("revoked_leaf_pending");
        }
        let who = Principal::account(bytes(&r.account)?);
        Ok(device_intent_body(
            room,
            ctx.rng,
            op::DEVICE_ADD,
            &who,
            &r.incarnation,
            &bytes(&r.device)?,
            &acs,
        ))
    }

    pub(super) fn rejoin_coordinator(
        &self,
        room: &GroupState,
        device: &[u8],
        account: &Principal,
    ) -> R<()> {
        let record = self
            .group_dir(&room.conversation_id)
            .remove(&hex(device))
            .ok_or(GroupError("not_authorized"))?;
        if room.coordinator_succession.first().map(Vec::as_slice) != Some(device) {
            return err("waiting_for_coordinator");
        }
        if record.account != hex(&account.id)
            || !self.group_device_authorized(&account.id, device, &bytes(&record.pk)?)
        {
            return err("not_authorized");
        }
        Ok(())
    }

    pub(super) fn group_has_revoked_leaf(&self, conv: &[u8]) -> bool {
        let Some(directory) = self.group_devices.get(&hex(conv)) else {
            return false;
        };
        let heads = self.group_heads();
        let has_ineligible_leaf = |head: &Acs, include_quarantine: bool| {
            let account = hex(&head.account_id);
            directory.iter().any(|(d, r)| {
                r.account == account
                    && bytes(d).is_ok_and(|d| {
                        device_in(&head.revoked, &d)
                            || (include_quarantine && device_in(&head.quarantined, &d))
                    })
            })
        };
        heads.iter().any(|(head, _)| has_ineligible_leaf(head, true))
            // A local chat block changes delivery and consent, never what we know about a Room leaf.
            || self.relationships.values().filter(|r| r.state == RelState::Active).any(|r| {
                bytes(&r.peer_acs_body).ok().and_then(|b| Acs::from_body(&b).ok()).is_some_and(|head| {
                    hex(&head.account_id) == r.peer_account_id && has_ineligible_leaf(&head, !heads.iter().any(|(best, _)| {
                        best.account_id == head.account_id && best.generation > head.generation
                    }))
                })
            })
    }

    pub(super) fn group_apply_at(
        &self,
        parent: &GroupState,
        intent: &Intent,
        actor: &Principal,
        new_epoch: u64,
    ) -> R<GroupState> {
        let device_op = matches!(intent.operation, op::DEVICE_ADD | op::DEVICE_REMOVE_REVOKED);
        if device_op {
            let (target, _) = target_of(&intent.args)?;
            let device = b(req(&intent.args, 2)?)?;
            let generation = u(req(&intent.args, 3)?)?;
            let (head, creds) = self
                .group_heads()
                .into_iter()
                .find(|(head, _)| {
                    target.id == head.account_id
                        && head.generation == generation
                        && intent.acs_head_hash.as_deref() == Some(head.hash().as_slice())
                })
                .ok_or(GroupError("stale_account_head"))?;
            let old = self
                .group_dir(&parent.conversation_id)
                .remove(&hex(&device));
            if intent.operation == op::DEVICE_REMOVE_REVOKED {
                if !old.is_some_and(|r| r.account == hex(&target.id))
                    || !(device_in(&head.revoked, &device) || device_in(&head.quarantined, &device))
                {
                    return err("not_authorized");
                }
            } else {
                check_inviter(parent, actor)?;
                // Recovery Add must identify the recovery device at the current locally validated head.
                let recovery = self.relationships.values().any(|r| {
                    r.peer_account_id == hex(&target.id)
                        && r.state == RelState::Active
                        && !r.blocked
                        && r.peer_links
                            .last()
                            .is_some_and(|l| recovered_device(l, &head, &device))
                });
                if intent.authorization_kind != 1
                    || old.is_some()
                    || !recovery
                    || self.group_has_revoked_leaf(&parent.conversation_id)
                    || !creds
                        .iter()
                        .any(|c| device == c.device_id && active_in(&head, c))
                {
                    return err("not_authorized");
                }
            }
        }
        super::apply_with_device_evidence(parent, intent, actor, new_epoch, device_op)
    }

    pub(super) fn group_rejoin_joined(&mut self, invite: &GroupInviteRec, name: &str) {
        if invite.group_name != name {
            return;
        }
        let requests = &mut self.group_carriage.rejoin.requests;
        if requests.iter().any(|r| {
            r.relationship_id == invite.relationship_id && r.room_hint == invite.group_name
        }) {
            requests.retain(|r| r.room_hint != invite.group_name);
        }
    }

    pub(crate) fn group_rejoin_view(
        &self,
        account: &[u8],
        device: &[u8],
        now_ms: u64,
    ) -> RejoinView {
        let state = &self.group_carriage.rejoin;
        let contacts = if self.rejoin_own_head(account, device).is_some() {
            self.relationships
                .keys()
                .filter(|id| self.rejoin_contact_usable(id))
                .cloned()
                .collect()
        } else {
            vec![]
        };
        let me = Principal::account(account.to_vec());
        let reviews = state
            .reviews
            .iter()
            .filter_map(|r| {
                if r.declined
                    || r.invite
                        .as_ref()
                        .and_then(|id| self.group_invites.get(id))
                        .is_some_and(|invite| {
                            invite.state != InviteState::Declined && !invite_lapsed(invite, now_ms)
                        })
                {
                    return None;
                }
                let room = self.group_state(&bytes(&r.room).ok()?).ok()?;
                room.member(&me)?;
                self.rejoin_review_current(r, &room).ok()?;
                Some(RejoinReviewView {
                    id: r.id.clone(),
                    conversation_id: r.room.clone(),
                    relationship_id: r.relationship.clone(),
                    working: self.rejoin_removal_open(r),
                    can_invite: self.rejoin_coordinator(&room, device, &me).is_ok()
                        && check_inviter(&room, &me).is_ok(),
                })
            })
            .collect();
        RejoinView {
            contacts,
            pending: state.requests.clone(),
            reviews,
        }
    }
}

fn device_in(set: &std::collections::BTreeSet<[u8; 16]>, device: &[u8]) -> bool {
    <&[u8; 16]>::try_from(device).is_ok_and(|d| set.contains(d))
}

fn check_inviter(room: &GroupState, who: &Principal) -> R<()> {
    let caps = room.capabilities_of(who);
    if !caps.contains("room.invite") || !caps.contains("room.manage_admission") {
        return err("not_authorized");
    }
    let governance = &room.governance;
    if governance
        .protected_operations
        .as_ref()
        .is_some_and(|ops| ops.contains(&op::DEVICE_ADD) || ops.contains(&op::MEMBER_ADMIT))
        && (governance.approval_threshold != 1 || !governance.principals.contains(who))
    {
        return err("not_authorized");
    }
    Ok(())
}
