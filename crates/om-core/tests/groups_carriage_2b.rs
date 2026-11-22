// SPDX-License-Identifier: MPL-2.0
//! Group carriage, Package 2b (ADR-0189 sections 1.4, 2 and 3): three accounts joining one Room, a messaging-only
//! device joining, a Commit held until an account head arrives, G10 acceptance owned by Core, and the Room defaults
//! with their tightening and loosening announced. Real Cores over the loopback network; the engine is the test-only
//! fake by default and stock OpenMLS with `--features groups-openmls`. Deposit and polling are the host's (Package 3):
//! a `Sealed.wrapper` is handed to the destination Core directly, exactly the bytes the host would deposit.

mod common;

use om_core::api::*;
use om_core::crypto::hex;
use om_core::groups::GroupError;
use om_core::groups::carriage::{self as cg, *};
use om_core::groups::flow::*;
use om_core::groups::room::{self, Setting};
use om_core::host::Step;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};
use std::collections::BTreeMap;

struct P {
    rt: Runtime,
    ctx: ProfileExecutionContext,
    /// peer index -> the relationship id with that peer
    rels: BTreeMap<usize, String>,
}

fn mk(net: &LoopbackNetwork, name: &str, label: &str) -> P {
    mk_with_media(net, name, label, false)
}

fn mk_with_media(net: &LoopbackNetwork, name: &str, label: &str, expressive: bool) -> P {
    let mut rt = Runtime::new(name, net);
    if expressive {
        rt.core.enable_expressive_media();
    }
    let ctx = common::create_profile(&mut rt, &format!("p-{name}"), label);
    P {
        rt,
        ctx,
        rels: BTreeMap::new(),
    }
}

fn convs(p: &P) -> Vec<String> {
    let l: Vec<ConversationSummary> =
        p.rt.query(Query::GetConversationList { ctx: p.ctx.clone() });
    l.into_iter().map(|c| c.id).collect()
}

/// `a` invites `b` and the two become Active contacts (a fresh pair: the newest conversation of each).
fn connect(ps: &mut [P], ai: usize, bi: usize) {
    let (before_a, before_b) = (convs(&ps[ai]), convs(&ps[bi]));
    let op = format!("inv-{ai}-{bi}");
    ps[ai]
        .rt
        .dispatch(&ps[ai].ctx, &op, IntentBody::CreateInvite);
    let (c, _) = (ps[ai].ctx.clone(), 0);
    ps[ai].rt.flush_net(&c);
    let link = ps[ai]
        .rt
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: c,
            operation_id: op,
        })
        .unwrap()
        .link;
    let bctx = ps[bi].ctx.clone();
    ps[bi].rt.dispatch(
        &bctx,
        &format!("acc-{ai}-{bi}"),
        IntentBody::AcceptInvitation { link },
    );
    settle_pair(ps, ai, bi);
    let actx = ps[ai].ctx.clone();
    let intros: Vec<IntroductionRequest> = ps[ai]
        .rt
        .query(Query::GetIntroductions { ctx: actx.clone() });
    ps[ai].rt.dispatch(
        &actx,
        &format!("intro-{ai}-{bi}"),
        IntentBody::AcceptIntroduction {
            request_id: intros.last().unwrap().id.clone(),
        },
    );
    settle_pair(ps, ai, bi);
    let new = |p: &P, before: &[String]| {
        let n: Vec<String> = convs(p)
            .into_iter()
            .filter(|c| !before.contains(c))
            .collect();
        assert_eq!(n.len(), 1, "one new conversation");
        n[0].clone()
    };
    let (ra, rb) = (new(&ps[ai], &before_a), new(&ps[bi], &before_b));
    ps[ai].rels.insert(bi, ra);
    ps[bi].rels.insert(ai, rb);
}

fn settle_pair(ps: &mut [P], i: usize, j: usize) {
    let (lo, hi) = (i.min(j), i.max(j));
    let (left, right) = ps.split_at_mut(hi);
    let (a, b) = (&mut left[lo], &mut right[0]);
    settle_all(&mut [(&mut a.rt, &a.ctx), (&mut b.rt, &b.ctx)]);
}

fn settle_every(ps: &mut [P]) {
    let mut v: Vec<(&mut Runtime, &ProfileExecutionContext)> =
        ps.iter_mut().map(|p| (&mut p.rt, &p.ctx)).collect();
    // `settle_all` takes a slice of pairs
    let mut refs: Vec<(&mut Runtime, &ProfileExecutionContext)> = vec![];
    for (r, c) in v.drain(..) {
        refs.push((r, c));
    }
    settle_all(&mut refs);
}

fn trio() -> Vec<P> {
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, "alice-2b", "Alice"),
        mk(&net, "bob-2b", "Bob"),
        mk(&net, "carol-2b", "Carol"),
    ];
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2);
    connect(&mut ps, 1, 2);
    ps
}

impl P {
    fn dob(&mut self, intent: GroupIntent) -> Result<GroupReply, GroupError> {
        let (out, step): (_, Step) = self.rt.core.group_do(&self.ctx, intent).unwrap();
        assert!(self.rt.settle(&step));
        let mut rep = out?;
        for r in self.settle_commits() {
            rep.events.extend(r.events);
            rep.out.extend(r.out);
        }
        Ok(rep)
    }
    /// What the host driver does after every durable step (G10): no report of acceptance, no relay acknowledgement.
    fn settle_commits(&mut self) -> Vec<GroupReply> {
        let mut all = vec![];
        loop {
            let (r, step) = self.rt.core.group_settle(&self.ctx).unwrap();
            assert!(self.rt.settle(&step));
            let r = r.unwrap();
            if r.is_empty() {
                return all;
            }
            all.extend(r);
        }
    }
    fn view(&self, q: GroupQuery) -> GroupView {
        self.rt.core.group_query(&self.ctx, q).unwrap().unwrap()
    }
    fn groups(&self) -> usize {
        match self.view(GroupQuery::Groups) {
            GroupView::Groups(g) => g.len(),
            _ => unreachable!(),
        }
    }
    fn conv(&self) -> Vec<u8> {
        match self.view(GroupQuery::Groups) {
            GroupView::Groups(g) => om_codec::cbor::unhex(&g[0].conversation_id),
            _ => unreachable!(),
        }
    }
    fn detail(&self, conv: &[u8]) -> GroupDetail {
        match self.view(GroupQuery::Group {
            conversation_id: conv.to_vec(),
        }) {
            GroupView::Group(d) => *d,
            _ => unreachable!(),
        }
    }
    fn notices(&self, conv: &[u8]) -> Vec<room::Notice> {
        match self.view(GroupQuery::Notices {
            conversation_id: conv.to_vec(),
        }) {
            GroupView::Notices(n) => n,
            _ => unreachable!(),
        }
    }
    fn texts(&self, conv: &[u8]) -> Vec<String> {
        match self.view(GroupQuery::Messages {
            conversation_id: conv.to_vec(),
        }) {
            GroupView::Messages(m) => m.into_iter().map(|x| x.text).collect(),
            _ => unreachable!(),
        }
    }
    fn send_lane(&mut self, rep: &GroupReply) {
        for o in &rep.out {
            if let Target::Relationship(rel) = &o.to
                && matches!(
                    o.wire,
                    GroupWire::Invite { .. } | GroupWire::Accept { .. } | GroupWire::Decline { .. }
                )
            {
                self.rt.supply_routes(4);
                let (r, step) = self
                    .rt
                    .core
                    .group_lane_send(&self.ctx, rel, &o.wire)
                    .unwrap();
                r.unwrap();
                assert!(self.rt.settle(&step));
            }
        }
    }
    fn pump_lane(&mut self) -> Vec<GroupReply> {
        let (out, step) = self.rt.core.group_lane_pump(&self.ctx).unwrap();
        assert!(self.rt.settle(&step));
        let mut all: Vec<GroupReply> = out.into_iter().map(|(_, r)| r.unwrap()).collect();
        all.extend(self.settle_commits());
        all
    }
    fn seal(&mut self, o: &Outbound) -> Result<Vec<Sealed>, GroupError> {
        self.rt.supply_routes(4);
        let (r, step) = self.rt.core.group_frame_send(&self.ctx, o).unwrap();
        assert!(self.rt.settle(&step));
        r
    }
    fn open(&mut self, wrapper: &[u8]) -> Result<GroupReply, GroupError> {
        let (r, step) = self
            .rt
            .core
            .group_frame_receive(&self.ctx, wrapper)
            .unwrap();
        assert!(self.rt.settle(&step));
        let mut rep = r?;
        for x in self.settle_commits() {
            rep.events.extend(x.events);
            rep.out.extend(x.out);
        }
        Ok(rep)
    }
    fn device_hex(&self) -> String {
        let d: DevicesView = self.rt.query(Query::GetDevices {
            ctx: self.ctx.clone(),
        });
        d.devices.iter().find(|x| x.is_current).unwrap().id.clone()
    }
    fn outbox(&self) -> Vec<OutItem> {
        self.rt.core.group_outbox(&self.ctx).unwrap()
    }
    fn pending(&self) -> usize {
        self.rt.core.debug_app_state(&self.ctx).unwrap()["groupCarriage"]["pending"]
            .as_array()
            .unwrap()
            .len()
    }
    fn pending_retry(&mut self) -> Vec<GroupReply> {
        let (r, step) = self.rt.core.group_pending_retry(&self.ctx).unwrap();
        assert!(self.rt.settle(&step));
        r.unwrap()
    }
    fn pending_refresh(&mut self) {
        let step = self.rt.core.group_pending_refresh(&self.ctx).unwrap();
        assert!(self.rt.settle(&step));
    }
}

fn events(reps: &[GroupReply]) -> Vec<GroupEvent> {
    reps.iter().flat_map(|r| r.events.clone()).collect()
}

/// Take everything in `from`'s outbox in order, seal it, hand every wrapper to the destination device's Core, and
/// report the frames deposited. Returns what each destination answered: (destination index, result).
fn flush(ps: &mut [P], from: usize) -> Vec<(usize, Result<GroupReply, GroupError>)> {
    let mut got = vec![];
    let items = ps[from].outbox();
    let mut done = vec![];
    for it in items {
        let o = it.to_outbound().unwrap();
        let sealed = match ps[from].seal(&o) {
            Ok(s) => s,
            Err(e) => panic!("sealing {:?}: {e}", std::mem::discriminant(&it.wire)),
        };
        for s in sealed {
            let to = (0..ps.len())
                .find(|i| ps[*i].device_hex() == hex(&s.device))
                .expect("a destination Core");
            got.push((to, ps[to].open(&s.wrapper)));
        }
        done.push(it.id);
    }
    let step = ps[from]
        .rt
        .core
        .group_outbox_done(&ps[from].ctx, &done)
        .unwrap();
    assert!(ps[from].rt.settle(&step));
    got
}

/// `inviter` creates a Room inviting `invitee_idx` peers (relationships it holds); each accepts; Alice's Core adds
/// each one at a time and accepts every Commit itself; the frames are flushed in outbox order after each.
fn join_all(ps: &mut [P], creator: usize, invitees: &[usize], protection: u64) -> Vec<u8> {
    let rels: Vec<String> = invitees
        .iter()
        .map(|i| ps[creator].rels[i].clone())
        .collect();
    let rep = ps[creator]
        .dob(GroupIntent::CreateWithProtection {
            name: "Book club".into(),
            relationship_ids: rels,
            protection,
        })
        .unwrap();
    ps[creator].send_lane(&rep);
    settle_every(ps);
    let mut accepts = vec![];
    for i in invitees {
        let reps = ps[*i].pump_lane();
        let id = events(&reps)
            .into_iter()
            .find_map(|e| match e {
                GroupEvent::InviteReceived { invite_id } => Some(invite_id),
                _ => None,
            })
            .expect("invitation over the 1:1");
        accepts.push((*i, id));
    }
    for (i, id) in accepts {
        let rep = ps[i]
            .dob(GroupIntent::Accept {
                invite_id: id,
                siblings: vec![],
            })
            .unwrap();
        ps[i].send_lane(&rep);
    }
    settle_every(ps);
    // Alice's Core takes the Accepts; each Add is prepared, queued and accepted by Core alone
    let reps = ps[creator].pump_lane();
    assert!(
        events(&reps)
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. })),
        "{:?}",
        events(&reps)
    );
    // deliver Welcomes and Commits, one Add at a time, until Alice's outbox is empty and nobody waits
    for _ in 0..(2 * invitees.len() + 2) {
        for (to, r) in flush(ps, creator) {
            r.unwrap_or_else(|e| panic!("destination {to}: {e}"));
        }
        let more = ps[creator].settle_commits();
        let _ = more;
        if ps[creator].outbox().is_empty() {
            break;
        }
    }
    ps[creator].conv()
}

#[test]
fn three_accounts_join_one_room_and_talk() {
    let mut ps = trio();
    let conv = join_all(&mut ps, 0, &[1, 2], room::RECOMMENDED);
    for p in &ps {
        assert_eq!(p.groups(), 1);
        let d = p.detail(&conv);
        assert_eq!(d.summary.members, 3, "everyone sees all three people");
    }
    // text from each person reaches the other two
    for (i, text) in [(0usize, "from Alice"), (1, "from Bob"), (2, "from Carol")] {
        let rep = ps[i]
            .dob(GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: text.into(),
            })
            .unwrap();
        for o in &rep.out {
            for s in ps[i].seal(o).unwrap() {
                let to = (0..3)
                    .find(|j| ps[*j].device_hex() == hex(&s.device))
                    .unwrap();
                ps[to].open(&s.wrapper).unwrap();
            }
        }
    }
    for p in &ps {
        let mut t = p.texts(&conv);
        t.sort();
        assert_eq!(t, vec!["from Alice", "from Bob", "from Carol"]);
    }
}

/// Alice's outbox until empty, every frame sealed and handed to its destination Core.
fn drain(ps: &mut [P], from: usize) {
    for _ in 0..8 {
        if ps[from].outbox().is_empty() {
            return;
        }
        for (to, r) in flush(ps, from) {
            r.unwrap_or_else(|e| panic!("destination {to}: {e}"));
        }
        let _ = ps[from].settle_commits();
    }
    assert!(ps[from].outbox().is_empty(), "outbox did not drain");
}

#[test]
fn a_core_accepts_its_own_commit_with_no_report_and_the_host_cannot() {
    let mut ps = trio();
    let _conv = join_all(&mut ps, 0, &[1], room::RECOMMENDED);
    // nothing in Alice's outbox is left over, and every journal entry ended Accepted without a deposit report
    assert!(ps[0].outbox().is_empty());
    let conv = ps[0].conv();
    let rep = ps[0]
        .dob(GroupIntent::RoomSetting {
            conversation_id: conv.clone(),
            setting: Setting::Protection(2),
            confirmed: false,
        })
        .unwrap();
    assert!(
        events(std::slice::from_ref(&rep))
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. }))
    );
    assert!(
        !ps[0].outbox().is_empty(),
        "the Commit is queued by Core itself"
    );
    // the host may not report acceptance: Core owns it (G10)
    let id = match events(std::slice::from_ref(&rep))
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::CommitPrepared { intent_id, .. } => Some(intent_id),
            _ => None,
        }) {
        Some(i) => i,
        None => unreachable!(),
    };
    let e = ps[0]
        .dob(GroupIntent::CommitAccepted { intent_id: id })
        .unwrap_err();
    assert_eq!(e.0, "acceptance_is_core_owned");
    // a second settle is a no-op
    assert!(ps[0].settle_commits().is_empty());
}

#[test]
fn recommended_defaults_tighten_without_confirmation_and_are_announced() {
    let mut ps = trio();
    let conv = join_all(&mut ps, 0, &[1, 2], room::RECOMMENDED);
    let d = ps[1].detail(&conv);
    assert!(
        d.protection.members_may_invite,
        "Recommended: members can add"
    );
    assert_eq!(d.protection.disappearing_timer, None);
    // tightening needs no confirmation and tells everyone
    ps[0]
        .dob(GroupIntent::RoomSetting {
            conversation_id: conv.clone(),
            setting: Setting::Protection(2),
            confirmed: false,
        })
        .unwrap();
    drain(&mut ps, 0);
    for i in [0usize, 1, 2] {
        let d = ps[i].detail(&conv);
        assert!(!d.protection.members_may_invite, "member {i} sees Hardened");
        let n = ps[i].notices(&conv);
        assert!(!n.is_empty(), "member {i} was told");
        assert!(n.iter().all(|x| x.changes.iter().all(|c| !c.loosened)));
    }
}

#[test]
fn loosening_is_refused_unconfirmed_then_announced_as_loosened() {
    let mut ps = trio();
    let conv = join_all(&mut ps, 0, &[1, 2], room::HARDENED);
    assert!(!ps[1].detail(&conv).protection.members_may_invite);
    let e = ps[0]
        .dob(GroupIntent::RoomSetting {
            conversation_id: conv.clone(),
            setting: Setting::MembersMayInvite(true),
            confirmed: false,
        })
        .unwrap_err();
    assert_eq!(e.0, "loosening_needs_confirmation");
    assert!(
        ps[1].notices(&conv).is_empty(),
        "nothing was sent or recorded"
    );
    ps[0]
        .dob(GroupIntent::RoomSetting {
            conversation_id: conv.clone(),
            setting: Setting::MembersMayInvite(true),
            confirmed: true,
        })
        .unwrap();
    drain(&mut ps, 0);
    for i in [0usize, 1, 2] {
        assert!(ps[i].detail(&conv).protection.members_may_invite);
        let n = ps[i].notices(&conv);
        assert!(
            n.iter().any(|x| x
                .changes
                .iter()
                .any(|c| c.loosened && c.field == "members_may_invite")),
            "member {i} sees a loosening: {n:?}"
        );
    }
}

#[test]
fn under_hardened_a_member_cannot_invite_but_under_recommended_can() {
    for (profile, ok) in [(room::HARDENED, false), (room::RECOMMENDED, true)] {
        let mut ps = trio();
        let conv = join_all(&mut ps, 0, &[1], profile);
        let rel = ps[1].rels[&2].clone();
        let r = ps[1].dob(GroupIntent::Invite {
            conversation_id: conv.clone(),
            relationship_id: rel,
        });
        assert_eq!(r.is_ok(), ok, "profile {profile}: {:?}", r.err());
    }
}

/// D36: a messaging-only device (no controller key, so it cannot sign an AccountControlHead) joins a Room by its
/// account's validated head alone: no new authority, nothing signed by it that a controller would sign.
#[test]
fn a_messaging_only_device_joins_a_room() {
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, "carol-d36", "Carol"),
        mk(&net, "alice-d36", "Alice"),
    ];
    connect(&mut ps, 0, 1);
    let mut phone_rt = Runtime::new("phone-d36", &net);
    let phone_ctx = phone_rt.open("p-phone");
    // Alice links the phone (a Messaging device)
    let (ac, pc) = (ps[1].ctx.clone(), phone_ctx.clone());
    ps[1].rt.dispatch(&ac, "offer", IntentBody::OfferDeviceLink);
    ps[1].rt.flush_net(&ac);
    let link = ps[1]
        .rt
        .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
            ctx: ac.clone(),
            operation_id: "offer".into(),
        })
        .unwrap()
        .link;
    phone_rt.dispatch(&pc, "ask", IntentBody::RequestDeviceLink { link });
    let pc2 = pc.clone();
    let three = |ps: &mut Vec<P>, ph: &mut Runtime| {
        let (a, c) = (ps[0].ctx.clone(), ps[1].ctx.clone());
        let (l, r) = ps.split_at_mut(1);
        settle_all(&mut [(&mut l[0].rt, &a), (&mut r[0].rt, &c), (ph, &pc2)]);
    };
    three(&mut ps, &mut phone_rt);
    let req: DevicesView = ps[1].rt.query(Query::GetDevices { ctx: ac.clone() });
    let request_id = req.link_requests[0].request_id.clone();
    let confirmation = common::confirmation(&mut ps[1].rt, &ac, "ApproveDeviceLink", &request_id);
    ps[1].rt.dispatch(
        &ac,
        "ok",
        IntentBody::ApproveDeviceLink {
            request_id,
            confirmation,
        },
    );
    three(&mut ps, &mut phone_rt);
    three(&mut ps, &mut phone_rt);
    let ds: DevicesView = phone_rt.query(Query::GetDevices { ctx: pc.clone() });
    assert!(
        ds.devices
            .iter()
            .any(|d| d.is_current && d.role == DeviceRole::Messaging),
        "the phone is messaging-only"
    );
    // Advance the ACS without a new enrollment. The phone must get the controller's new head over the sibling path.
    ps[1]
        .rt
        .dispatch(&ac, "new-recovery", IntentBody::ReplaceRecoveryKey);
    ps[1].rt.dispatch(
        &ac,
        "saved-recovery",
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "new-recovery".into(),
        },
    );
    assert_eq!(
        ps[1].rt.outcome(&ac, "saved-recovery"),
        Some(OutcomeClass::Committed)
    );
    three(&mut ps, &mut phone_rt);
    let controller = ps[1].rt.core.debug_app_state(&ac).unwrap();
    let phone_state = phone_rt.core.debug_app_state(&pc).unwrap();
    assert!(
        phone_state["acsChain"] == controller["acsChain"],
        "phone advances to the controller's chain tip"
    );
    // the 1:1 with Carol on the phone
    let l: Vec<ConversationSummary> =
        phone_rt.query(Query::GetConversationList { ctx: pc.clone() });
    let mut phone = P {
        rt: phone_rt,
        ctx: pc,
        rels: BTreeMap::new(),
    };
    phone.rels.insert(0, l[0].id.clone());
    // Carol invites Alice's account; the phone (not the laptop) answers
    let rel = ps[0].rels[&1].clone();
    let rep = ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Phone room".into(),
            relationship_ids: vec![rel],
            protection: room::RECOMMENDED,
        })
        .unwrap();
    ps[0].send_lane(&rep);
    three(&mut ps, &mut phone.rt);
    let reps = phone.pump_lane();
    let id = events(&reps)
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .expect("the invitation reaches the phone over the 1:1");
    let rep = phone
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    phone.send_lane(&rep);
    three(&mut ps, &mut phone.rt);
    let reps = ps[0].pump_lane();
    assert!(
        events(&reps)
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. })),
        "Carol's Core accepts the messaging-only device's Accept: {:?}",
        events(&reps)
    );
    // Welcome (sealed to the phone's device) and Commit
    let items = ps[0].outbox();
    assert!(!items.is_empty());
    let phone_dev = phone.device_hex();
    let mut done = vec![];
    let mut welcomed = false;
    for it in items {
        let sealed = ps[0].seal(&it.to_outbound().unwrap()).unwrap();
        for s in sealed {
            assert_eq!(
                hex(&s.device),
                phone_dev,
                "only the phone is a member device"
            );
            phone.open(&s.wrapper).unwrap();
            welcomed = true;
        }
        done.push(it.id);
    }
    assert!(welcomed);
    let c0 = ps[0].ctx.clone();
    let step = ps[0].rt.core.group_outbox_done(&c0, &done).unwrap();
    assert!(ps[0].rt.settle(&step));
    assert_eq!(
        phone.groups(),
        1,
        "the messaging-only device is in the Room"
    );
    let conv = phone.conv();
    assert_eq!(phone.detail(&conv).summary.members, 2);
    assert_eq!(phone.pending(), 0);
}

/// Everything up to Alice's Core preparing the Add of Carol: nothing is delivered yet. `heads` is the test hook of
/// `TEST_ROOM_HEADS` (0 normal, 1 no heads, 2 corrupted heads, 3 and 4 the same for the Add Commit only, 5 the Commit also carries a stranger's head, 6 the Commit's heads are too large for one frame).
fn trio_before_delivery(tag: &str, heads: u8) -> Vec<P> {
    trio_before_delivery_with(tag, heads, false)
}

/// `stranger`: Alice also has a contact, Dave (index 3), who is not in the Room and never meets Bob or Carol.
fn trio_before_delivery_with(tag: &str, heads: u8, stranger: bool) -> Vec<P> {
    om_core::runtime::TEST_ROOM_HEADS.with(|m| m.set(heads));
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, &format!("alice-{tag}"), "Alice"),
        mk(&net, &format!("bob-{tag}"), "Bob"),
        mk(&net, &format!("carol-{tag}"), "Carol"),
    ];
    if stranger {
        ps.push(mk(&net, &format!("dave-{tag}"), "Dave"));
    }
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2); // Carol and Bob have never met
    if stranger {
        connect(&mut ps, 0, 3);
    }
    let _ = join_all(&mut ps, 0, &[1], room::RECOMMENDED);
    let conv = ps[0].conv();
    let rel = ps[0].rels[&2].clone();
    let rep = ps[0]
        .dob(GroupIntent::Invite {
            conversation_id: conv.clone(),
            relationship_id: rel,
        })
        .unwrap();
    ps[0].send_lane(&rep);
    settle_every(&mut ps);
    let reps = ps[2].pump_lane();
    let id = events(&reps)
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .unwrap();
    let rep = ps[2]
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    ps[2].send_lane(&rep);
    settle_every(&mut ps);
    ps[0].pump_lane();
    ps
}

/// Alice invites Bob, then Carol, who has never met Bob. `heads`: what Alice's Welcome carries for the accounts it names
/// (0 their signed heads, 1 nothing, 2 a corrupted head). With 0 Carol joins a three-account Room; otherwise the
/// Welcome is held, never accepted unverified, bounded and expiring (D37, D44).
fn welcome_trio(tag: &str, heads: u8) -> Vec<P> {
    let mut ps = trio_before_delivery(tag, heads);
    // deliver everything to Carol only: the Welcome must be held
    let mut held = false;
    let mut done = vec![];
    for it in ps[0].outbox() {
        for s in ps[0].seal(&it.to_outbound().unwrap()).unwrap() {
            if s.device == hex_dev(&ps[2]) {
                let r = ps[2].open(&s.wrapper).unwrap();
                held |= events(&[r])
                    .iter()
                    .any(|e| matches!(e, GroupEvent::Held(_)));
            }
        }
        done.push(it.id);
    }
    let c0 = ps[0].ctx.clone();
    let step = ps[0].rt.core.group_outbox_done(&c0, &done).unwrap();
    assert!(ps[0].rt.settle(&step));
    if heads == 0 {
        assert!(
            !held,
            "the Welcome names Bob with his signed head: nothing to hold"
        );
        assert_eq!(ps[2].groups(), 1, "Carol joined");
        assert_eq!(ps[2].pending(), 0);
        return ps;
    }
    assert!(held, "the Welcome is held, not accepted");
    assert_eq!(ps[2].groups(), 0, "nothing was accepted unverified");
    assert_eq!(ps[2].pending(), 1);
    ps
}

#[test]
fn a_three_account_room_joins_on_the_heads_the_welcome_carries() {
    let ps = welcome_trio("d44a", 0);
    let conv = ps[2].conv();
    assert_eq!(
        ps[2].detail(&conv).summary.members,
        3,
        "Alice, Bob and Carol"
    );
    assert_eq!(ps[2].pending(), 0);
    // Names come only from the device's own contact list: Carol knows Alice, but never met Bob, and the Room gives her no name
    // for him (ADR-0189: no new metadata). Her own entry is `you`, with no name.
    let named = |p: &P| -> Vec<(Option<String>, bool)> {
        let mut v: Vec<_> = p
            .detail(&conv)
            .people
            .into_iter()
            .map(|m| (m.display, m.you))
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        named(&ps[2]),
        vec![(None, false), (None, true), (Some("Alice".into()), false)]
    );
    assert_eq!(
        named(&ps[0]),
        vec![
            (None, true),
            (Some("Bob".into()), false),
            (Some("Carol".into()), false)
        ]
    );
}

/// A head that does not verify, or none at all, leaves the Welcome held until it expires.
#[test]
fn a_welcome_whose_head_does_not_verify_is_held_until_it_expires() {
    for heads in [1u8, 2, 7] {
        let mut ps = welcome_trio(&format!("d44b{heads}"), heads);
        ps[2].pending_retry();
        assert_eq!(ps[2].groups(), 0);
        assert_eq!(ps[2].pending(), 1);
        ps[2].rt.tick((cg::PENDING_TTL_S + 1) * 1000);
        ps[2].pending_retry();
        assert_eq!(ps[2].pending(), 0, "an expired frame is dropped");
        assert_eq!(ps[2].groups(), 0);
    }
}

#[test]
fn a_welcome_naming_an_unanchored_account_is_held_and_then_expires() {
    let mut ps = welcome_trio("d37a", 1);
    // a refresh asks the relationships it has; Bob is not one, so the Welcome stays held
    ps[2].pending_refresh();
    settle_every(&mut ps);
    ps[2].pending_retry();
    assert_eq!(ps[2].groups(), 0);
    assert_eq!(ps[2].pending(), 1);
    // it expires after the bound
    ps[2].rt.tick((cg::PENDING_TTL_S + 1) * 1000);
    ps[2].pending_retry();
    assert_eq!(ps[2].pending(), 0, "an expired frame is dropped");
    assert_eq!(ps[2].groups(), 0);
}

/// D37: the missing head arrives (Carol and Bob meet, so Carol can anchor Bob's account) and the held Welcome applies.
#[test]
fn a_held_welcome_applies_once_the_missing_head_arrives() {
    let mut ps = welcome_trio("d37b", 1);
    connect(&mut ps, 1, 2);
    ps[2].pending_refresh();
    settle_every(&mut ps);
    ps[2].pending_retry();
    settle_every(&mut ps);
    assert_eq!(ps[2].groups(), 1, "the held Welcome applied");
    assert_eq!(ps[2].pending(), 0);
}

/// D93. Alice invites Bob, then Carol, who has never met Bob. Alice's Add of Carol goes to both: the Welcome to Carol and
/// the Commit to Bob. Returns the people and what Bob's Core answered to the Commit.
fn add_carol(tag: &str, heads: u8) -> (Vec<P>, Vec<GroupEvent>) {
    add_carol_with(tag, heads, false)
}

fn add_carol_with(tag: &str, heads: u8, stranger: bool) -> (Vec<P>, Vec<GroupEvent>) {
    let mut ps = trio_before_delivery_with(tag, heads, stranger);
    let mut bob = vec![];
    let mut done = vec![];
    for it in ps[0].outbox() {
        for s in ps[0].seal(&it.to_outbound().unwrap()).unwrap() {
            if s.device == hex_dev(&ps[2]) {
                ps[2].open(&s.wrapper).unwrap();
            } else if s.device == hex_dev(&ps[1]) {
                bob.extend(events(&[ps[1].open(&s.wrapper).unwrap()]));
            }
        }
        done.push(it.id);
    }
    let c0 = ps[0].ctx.clone();
    let step = ps[0].rt.core.group_outbox_done(&c0, &done).unwrap();
    assert!(ps[0].rt.settle(&step));
    (ps, bob)
}

fn room_heads(p: &P) -> usize {
    p.rt.core.debug_app_state(&p.ctx).unwrap()["groupCarriage"]["roomHeads"]
        .as_object()
        .map_or(0, |m| m.len())
}

/// Everyone sends one text to the two others; each gets the other two (a real three-way Room).
fn all_talk(ps: &mut [P], conv: &[u8]) {
    for i in 0..ps.len() {
        ps[i]
            .dob(GroupIntent::SendText {
                conversation_id: conv.to_vec(),
                text: format!("from {i}"),
            })
            .unwrap();
        drain(ps, i);
    }
}

#[test]
fn a_room_of_strangers_survives_join_add_leave_and_restart() {
    for size in [3, 5] {
        let net = LoopbackNetwork::default();
        let mut ps: Vec<_> = (0..size)
            .map(|i| mk(&net, &format!("room-{size}-{i}"), &format!("Person {i}")))
            .collect();
        for i in 1..size {
            connect(&mut ps, 0, i);
        }
        let conv = join_all(&mut ps, 0, &[1], room::RECOMMENDED);
        let bob = &mut ps[1];
        bob.rt
            .dispatch(&bob.ctx, "rotate", IntentBody::ReplaceRecoveryKey);
        bob.rt.dispatch(
            &bob.ctx,
            "save",
            IntentBody::ConfirmRecoveryKeyStored {
                key_operation_id: "rotate".into(),
            },
        );
        assert_eq!(
            bob.rt.outcome(&bob.ctx, "save"),
            Some(OutcomeClass::Committed)
        );
        settle_every(&mut ps);
        for i in 2..size {
            let relationship_id = ps[0].rels[&i].clone();
            let invite = ps[0]
                .dob(GroupIntent::Invite {
                    conversation_id: conv.clone(),
                    relationship_id,
                })
                .unwrap();
            ps[0].send_lane(&invite);
            settle_every(&mut ps);
            accept_room_invite(&mut ps[i]);
            settle_every(&mut ps);
            ps[0].pump_lane();
            drain(&mut ps, 0);
        }
        ps[1].restart();
        for p in &ps {
            assert_eq!(p.detail(&conv).summary.members, size);
        }
        all_talk(&mut ps, &conv);
        let expected: Vec<_> = (0..size).map(|i| format!("from {i}")).collect();
        for p in &ps {
            let mut texts = p.texts(&conv);
            texts.sort();
            assert_eq!(texts, expected);
        }
        ps[size - 1]
            .dob(GroupIntent::Leave {
                conversation_id: conv.clone(),
            })
            .unwrap();
        drain(&mut ps, size - 1);
        drain(&mut ps, 0);
        assert_eq!(ps[size - 1].groups(), 0);
        for p in &ps[..size - 1] {
            assert_eq!(p.detail(&conv).summary.members, size - 1);
        }
    }
}

fn accept_room_invite(p: &mut P) {
    let id = events(&p.pump_lane())
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .expect("Room invitation");
    let reply = p
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    p.send_lane(&reply);
}

const ROOM_WEBP: &[u8] = b"RIFF\x12\0\0\0WEBPVP8L\x06\0\0\0\x2f\0\0\0\0\0";

fn prepare_room_media(
    net: &LoopbackNetwork,
    sender: &mut P,
    kind: om_core::expressive::Kind,
) -> String {
    use om_core::attach::{self, AttachmentRef};
    use om_core::crypto::cbor::{V, encode};
    use om_core::expressive::{AssetRef, BlobPolicy, CachePolicy, Install, Presentation, Release};

    let pack = [kind as u8; 32];
    let blob = [3; 32];
    let key = [4; 32];
    let mut animation = Vec::new();
    let (plaintext, duration_ms) = if kind == om_core::expressive::Kind::LoopAnimation {
        animation.extend_from_slice(
            b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x02\0\0\0\0\0\0\0\0\0ANIM\x06\0\0\0\0\0\0\0\0\0",
        );
        for _ in 0..2 {
            animation.extend_from_slice(b"ANMF\x1e\0\0\0");
            let mut frame = [0; 16];
            frame[12] = 34;
            animation.extend_from_slice(&frame);
            animation.extend_from_slice(&ROOM_WEBP[12..]);
        }
        let size = animation.len() as u32 - 8;
        animation[4..8].copy_from_slice(&size.to_le_bytes());
        (animation.as_slice(), Some(68))
    } else {
        (ROOM_WEBP, None)
    };
    let ciphertext = attach::seal(&key, &blob, plaintext);
    let asset = AssetRef {
        attachment: AttachmentRef {
            blob_id: blob,
            key,
            ciphertext_digest: attach::ciphertext_digest(&ciphertext),
            plaintext_size: plaintext.len() as u64,
            media_type: "image/webp".into(),
            locations: vec![],
            filename: None,
            read_cap: None,
            width: Some(1),
            height: Some(1),
            duration_ms,
        },
        presentation: Presentation::webp(kind, "A wave".into(), Some("Wave".into())),
    };
    let release = encode(&V::M(vec![
        (0, V::U(45)),
        (1, V::U(1)),
        (2, V::B(pack.to_vec())),
        (3, V::U(1)),
        (5, V::T("Greetings".into())),
        (7, V::A(vec![V::U(4)])),
        (
            8,
            V::A(vec![V::M(vec![
                (0, V::T("wave".into())),
                (1, V::U(kind as u64)),
                (2, asset.value()),
                (5, V::T("Wave".into())),
            ])]),
        ),
        (9, V::M(vec![])),
    ]));
    let id = Release::parse(&release).unwrap().id();
    let (result, commit) = sender
        .rt
        .core
        .install_content_pack(
            &sender.ctx,
            Install {
                release: &release,
                expected_release: id,
                confirmed_release: id,
                control: None,
                policy: CachePolicy::Lazy,
                favourites: &[],
                cached: &[],
            },
        )
        .unwrap();
    result.unwrap();
    assert!(sender.rt.settle(&Step {
        commit,
        handle: None
    }));
    let (prepared, step) = sender
        .rt
        .core
        .prepare_expressive(&sender.ctx, pack, "wave", &ciphertext, BlobPolicy::Fresh)
        .unwrap();
    assert!(sender.rt.settle(&step));
    let prepared = prepared.unwrap();
    net.stage_blob(
        &om_core::crypto::unhex(&prepared.blob_id).unwrap(),
        prepared.ciphertext,
    );
    prepared.blob_id
}

#[test]
fn room_attachment_uses_the_committed_membership_when_the_upload_finishes() {
    use om_core::expressive::{Kind, SendFrom, Source, Target as ExpressiveTarget};

    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk_with_media(&net, "room-alice", "Alice", true),
        mk_with_media(&net, "room-bob", "Bob", true),
        mk_with_media(&net, "room-carol", "Carol", false),
    ];
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2);
    let conv = join_all(&mut ps, 0, &[1], room::RECOMMENDED);
    let start_send = |sender: &mut P, op, kind, target| {
        let blob_id = prepare_room_media(&net, sender, kind);
        sender.rt.dispatch(
            &sender.ctx,
            op,
            IntentBody::SendExpressive {
                conversation_id: hex(&conv),
                target,
                asset: SendFrom::Prepared { blob_id },
            },
        );
    };
    let check_last = |p: &mut P, expressive: bool, kind| {
        let GroupView::Messages(messages) = p.view(GroupQuery::Messages {
            conversation_id: conv.clone(),
        }) else {
            panic!("Room messages");
        };
        let message = messages.last().unwrap();
        let (attachment_id, attachment) = message.attachment_view.as_ref().unwrap();
        assert_eq!(message.text, "Wave");
        assert_eq!(attachment.media_type, "image/webp");
        assert_eq!(message.expressive_fallback, message.outgoing && !expressive);
        assert_eq!(
            p.rt.core
                .expressive_presentation(&p.ctx, attachment_id, Source::Attachment(0))
                .map(|asset| asset.kind),
            expressive.then_some(kind)
        );
        let opened = p.rt.fetch_attachment(&p.ctx, attachment_id, 0).unwrap();
        if kind == Kind::LoopAnimation {
            assert_eq!(opened.duration_ms, Some(68));
        } else {
            assert_eq!(opened.plaintext, ROOM_WEBP);
        }
    };
    let finish_send = |parties: &mut [P], op, recipients: &[usize]| {
        let mut expected: Vec<_> = recipients
            .iter()
            .map(|i| parties[*i].device_hex())
            .collect();
        expected.sort();
        let sender = &mut parties[0];
        assert_eq!(
            sender.rt.outcome(&sender.ctx, op),
            Some(OutcomeClass::PendingNetwork)
        );
        assert!(
            !sender
                .outbox()
                .iter()
                .any(|i| matches!(i.wire, GroupWire::App { .. }))
        );
        sender.rt.flush_net(&sender.ctx);
        assert_eq!(
            sender.rt.outcome(&sender.ctx, op),
            Some(OutcomeClass::Committed)
        );
        let mut devices: Vec<_> = sender
            .outbox()
            .iter()
            .filter(|item| matches!(item.wire, GroupWire::App { .. }))
            .map(|item| match item.to_outbound().unwrap().to {
                Target::Device { device, .. } => hex(&device),
                other => panic!("Unexpected Room destination {other:?}"),
            })
            .collect();
        devices.sort();
        assert_eq!(devices, expected);
        drain(parties, 0);
    };

    start_send(
        &mut ps[0],
        "both-capable",
        Kind::Sticker,
        ExpressiveTarget::Attachment,
    );
    finish_send(&mut ps, "both-capable", &[1]);
    check_last(&mut ps[0], true, Kind::Sticker);
    check_last(&mut ps[1], true, Kind::Sticker);

    let relationship_id = ps[0].rels[&2].clone();
    let invite = ps[0]
        .dob(GroupIntent::Invite {
            conversation_id: conv.clone(),
            relationship_id,
        })
        .unwrap();
    ps[0].send_lane(&invite);
    settle_every(&mut ps);
    accept_room_invite(&mut ps[2]);
    settle_every(&mut ps);
    start_send(
        &mut ps[0],
        "join-during-upload",
        Kind::Sticker,
        ExpressiveTarget::Attachment,
    );
    ps[0].pump_lane();
    drain(&mut ps, 0);
    assert_eq!(ps[2].groups(), 1);
    finish_send(&mut ps, "join-during-upload", &[1, 2]);
    check_last(&mut ps[0], false, Kind::Sticker);
    check_last(&mut ps[1], false, Kind::Sticker);
    ps[2].rt.core.enable_expressive_media();
    check_last(&mut ps[2], false, Kind::Sticker);

    start_send(
        &mut ps[0],
        "leave-during-upload",
        Kind::LoopAnimation,
        ExpressiveTarget::Attachment,
    );
    ps[0].restart();
    ps[0].rt.core.enable_expressive_media();
    ps[2]
        .dob(GroupIntent::Leave {
            conversation_id: conv.clone(),
        })
        .unwrap();
    drain(&mut ps, 2);
    drain(&mut ps, 0);
    assert_eq!(ps[2].groups(), 0);
    finish_send(&mut ps, "leave-during-upload", &[1]);
    check_last(&mut ps[1], true, Kind::LoopAnimation);

    start_send(
        &mut ps[0],
        "room-inline",
        Kind::CustomEmoji,
        ExpressiveTarget::Inline,
    );
    assert_eq!(
        ps[0].rt.outcome(&ps[0].ctx, "room-inline"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unsupported,
        })
    );
    assert_eq!(net.blob_put_count(), 3);

    let sender = &mut ps[0];
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../verification/vectors/pack-file-vectors.json"
    ))
    .unwrap();
    let pack_bytes =
        om_core::crypto::unhex(vectors["vectors"][0]["hex"].as_str().unwrap()).unwrap();
    let prepared = sender
        .rt
        .prepare_attachment(
            &sender.ctx,
            &pack_bytes,
            om_core::attach::Meta {
                media_type: om_core::expressive::PACK_MEDIA_TYPE.into(),
                filename: Some("greetings.ompack".into()),
                width: None,
                height: None,
                duration_ms: None,
            },
        )
        .unwrap();
    sender.rt.dispatch(
        &sender.ctx,
        "room-pack",
        IntentBody::SendAttachments {
            conversation_id: hex(&conv),
            draft: Draft {
                text: String::new(),
                reply_to: None,
            },
            attachments: vec![AttachmentInput {
                blob_id: prepared.blob_id,
                voice_note: None,
            }],
        },
    );
    finish_send(&mut ps, "room-pack", &[1]);
    let receiver = &mut ps[1];
    let GroupView::Messages(messages) = receiver.view(GroupQuery::Messages {
        conversation_id: conv,
    }) else {
        panic!("Room messages");
    };
    let (attachment_id, _) = messages.last().unwrap().attachment_view.as_ref().unwrap();
    let pack = receiver
        .rt
        .fetch_attachment(&receiver.ctx, attachment_id, 0)
        .unwrap();
    assert_eq!(pack.media_type, om_core::expressive::PACK_MEDIA_TYPE);
    assert_eq!(pack.plaintext, pack_bytes);
    assert!(
        receiver
            .rt
            .core
            .review_pack_file(&receiver.ctx, &pack.plaintext)
            .unwrap()
            .is_ok()
    );
}

#[test]
fn a_commit_before_welcome_waits_across_restart() {
    let mut ps = trio_before_delivery("d41", 0);
    let conv = ps[0].conv();
    let carol = hex_dev(&ps[2]);
    let welcome = ps[0]
        .outbox()
        .into_iter()
        .find(|it| matches!(it.wire, GroupWire::Welcome { .. }))
        .unwrap();
    let welcome = ps[0]
        .seal(&welcome.to_outbound().unwrap())
        .unwrap()
        .remove(0);
    assert_eq!(welcome.device, carol);
    ps[0]
        .dob(GroupIntent::RoomSetting {
            conversation_id: conv.clone(),
            setting: Setting::MembersMayInvite(false),
            confirmed: false,
        })
        .unwrap();
    ps[0].settle_commits();
    let mut commit = ps[0]
        .outbox()
        .into_iter()
        .filter(|it| matches!(it.wire, GroupWire::Commit { .. }))
        .filter_map(|it| it.to_outbound())
        .find(|o| matches!(&o.to, Target::Device { device, .. } if *device == carol))
        .unwrap();
    let genuine = ps[0].seal(&commit).unwrap().remove(0);
    let GroupWire::Commit { signed_binding, .. } = &mut commit.wire else {
        unreachable!()
    };
    let last = signed_binding.pop().unwrap();
    signed_binding.push(if last == '0' { '1' } else { '0' });
    let invalid = ps[0].seal(&commit).unwrap().remove(0);
    assert!(ps[2].open(&invalid.wrapper).is_err());
    assert_eq!(ps[2].pending(), 0);
    let commit = genuine;
    let reply = ps[2].open(&commit.wrapper).unwrap();
    assert!(
        reply
            .events
            .iter()
            .any(|e| matches!(e, GroupEvent::Held(_)))
    );
    ps[2].open(&commit.wrapper).unwrap();
    assert_eq!(ps[2].pending(), 1, "an exact duplicate occupies one slot");
    assert_eq!(ps[2].groups(), 0);
    ps[2].restart();
    ps[2].pending_retry();
    assert_eq!(ps[2].pending(), 1);
    ps[2].open(&welcome.wrapper).unwrap();
    ps[2].pending_retry();
    assert_eq!(ps[2].pending(), 0);
    assert!(!ps[2].detail(&conv).protection.members_may_invite);
}

#[test]
fn an_accept_newer_than_the_pin_waits_for_verified_refresh_across_restart() {
    let net = LoopbackNetwork::default();
    let mut ps = vec![mk(&net, "alice-d40", "Alice"), mk(&net, "bob-d40", "Bob")];
    connect(&mut ps, 0, 1);
    let relationship_id = ps[0].rels[&1].clone();
    let invite = ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Head refresh".into(),
            relationship_ids: vec![relationship_id.clone()],
            protection: room::RECOMMENDED,
        })
        .unwrap();
    ps[0].send_lane(&invite);
    settle_every(&mut ps);
    let bob = &mut ps[1];
    let (_, step) = bob.rt.core.debug_link_messaging_device(&bob.ctx).unwrap();
    assert!(bob.rt.settle(&step));
    let updates = bob.rt.core.net_ops(&bob.ctx).unwrap();
    assert!(!updates.is_empty());
    accept_room_invite(bob);
    // Deliver the Accept before the ACS update, without changing either payload.
    for op in bob.rt.core.net_ops(&bob.ctx).unwrap() {
        if !updates.iter().any(|old| old.id == op.id) {
            let step = bob
                .rt
                .core
                .net_result(&bob.ctx, op.id, net.perform(&op.call))
                .unwrap();
            assert!(bob.rt.settle(&step));
        }
    }
    let alice = &mut ps[0];
    assert!(alice.rt.poll(&alice.ctx) > 0);
    assert!(alice.pump_lane().is_empty());
    assert!(
        alice
            .rt
            .core
            .net_ops(&alice.ctx)
            .unwrap()
            .iter()
            .all(|op| { !matches!(op.call, om_core::host::NetCall::Ack { .. }) }),
        "the relay acknowledgement waits with the reserved Accept"
    );
    let state = alice.rt.core.debug_app_state(&alice.ctx).unwrap();
    assert_eq!(
        state["relationships"][&relationship_id]["held"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    alice.restart();
    settle_every(&mut ps);
    assert!(
        events(&ps[0].pump_lane())
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. }))
    );
    drain(&mut ps, 0);
    assert_eq!(ps[1].groups(), 1);
}

/// D93: Bob never met Carol; the Add Commit carries Carol's signed head, so Bob applies it at once and the three talk.
#[test]
fn a_member_who_never_met_the_new_person_applies_the_add_on_the_head_the_commit_carries() {
    let (mut ps, bob) = add_carol("d93a", 0);
    assert!(
        !bob.iter().any(|e| matches!(e, GroupEvent::Held(_))),
        "nothing held: {bob:?}"
    );
    assert_eq!(ps[1].pending(), 0);
    let conv = ps[0].conv();
    for p in &ps {
        assert_eq!(p.detail(&conv).summary.members, 3, "all see three people");
    }
    // the head is kept only as the inviter's word, for Carol's account, and only because the Commit used it
    assert_eq!(room_heads(&ps[1]), 1, "Bob holds Carol's vouched head");
    assert_eq!(
        room_heads(&ps[0]),
        0,
        "Alice has a relationship: no vouched head"
    );
    all_talk(&mut ps, &conv);
    for p in &ps {
        let mut t = p.texts(&conv);
        t.sort();
        assert_eq!(t, vec!["from 0", "from 1", "from 2"]);
    }
}

/// D93, fail closed: a Commit with no head, or a head that does not verify, is held, never applied, and it expires.
#[test]
fn an_add_commit_without_a_verifiable_head_is_held_until_it_expires() {
    for heads in [3u8, 4] {
        let (mut ps, bob) = add_carol(&format!("d93b{heads}"), heads);
        assert!(
            bob.iter().any(|e| matches!(e, GroupEvent::Held(_))),
            "held: {bob:?}"
        );
        let conv = ps[0].conv();
        assert_eq!(ps[1].pending(), 1);
        assert_eq!(
            ps[1].detail(&conv).summary.members,
            2,
            "Bob did not apply it"
        );
        assert_eq!(room_heads(&ps[1]), 0, "an unverified head is not kept");
        ps[1].pending_retry();
        assert_eq!(ps[1].pending(), 1);
        ps[1].rt.tick((cg::PENDING_TTL_S + 1) * 1000);
        ps[1].pending_retry();
        assert_eq!(ps[1].pending(), 0, "an expired frame is dropped");
        assert_eq!(ps[1].detail(&conv).summary.members, 2);
    }
}

/// D93: a head the Commit carries for an account no added device belongs to is not kept, even when it is genuine and the
/// receiver holds no head for that account (here: Dave, Alice's contact, whom Bob has never met). The receiver ignores it
/// on arrival and in any case keeps a head only if the applied Commit registered a device of that account, so this
/// pins the outcome (Bob's store), not which of the two rules removed it.
#[test]
fn a_head_for_an_account_no_added_device_belongs_to_is_not_kept() {
    let (ps, bob) = add_carol_with("d93e", 5, true);
    assert!(
        !bob.iter().any(|e| matches!(e, GroupEvent::Held(_))),
        "Carol's own head was enough: {bob:?}"
    );
    let conv = ps[0].conv();
    assert_eq!(
        ps[1].detail(&conv).summary.members,
        3,
        "Bob applied the Add"
    );
    assert_eq!(
        room_heads(&ps[1]),
        1,
        "only Carol's head is kept; Dave's was carried but names nobody who was added"
    );
}

/// D95: a Commit whose heads would not fit one frame is sent without them, never truncated: the receiver then holds it
/// for the missing head (fail closed) and applies it once the head arrives by the normal path.
#[test]
fn an_add_commit_too_large_with_heads_goes_without_them_and_is_held() {
    let (mut ps, bob) = add_carol("d95a", 6);
    assert!(
        bob.iter().any(|e| matches!(e, GroupEvent::Held(_))),
        "sent without heads, so held: {bob:?}"
    );
    let conv = ps[0].conv();
    assert_eq!(ps[1].pending(), 1);
    assert_eq!(ps[1].detail(&conv).summary.members, 2);
    assert_eq!(room_heads(&ps[1]), 0);
    assert_eq!(
        ps[2].groups(),
        1,
        "Carol's Welcome (its own heads) was not affected"
    );
    connect(&mut ps, 1, 2);
    ps[1].pending_refresh();
    settle_every(&mut ps);
    ps[1].pending_retry();
    settle_every(&mut ps);
    assert_eq!(ps[1].pending(), 0);
    assert_eq!(ps[1].detail(&conv).summary.members, 3);
}

/// D93: a held Commit applies once the missing head arrives by the normal path (Bob and Carol meet).
#[test]
fn a_held_add_commit_applies_once_the_head_arrives_by_the_normal_path() {
    let (mut ps, _) = add_carol("d93c", 3);
    connect(&mut ps, 1, 2);
    ps[1].pending_refresh();
    settle_every(&mut ps);
    ps[1].pending_retry();
    settle_every(&mut ps);
    let conv = ps[0].conv();
    assert_eq!(ps[1].pending(), 0);
    assert_eq!(ps[1].detail(&conv).summary.members, 3);
}

/// D93: a head the Commit carries never replaces a head Bob already holds (here: his own relationship with Carol).
#[test]
fn a_head_in_the_commit_never_replaces_a_head_the_receiver_holds() {
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, "alice-d93d", "Alice"),
        mk(&net, "bob-d93d", "Bob"),
        mk(&net, "carol-d93d", "Carol"),
    ];
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2);
    connect(&mut ps, 1, 2); // Bob knows Carol
    let _ = join_all(&mut ps, 0, &[1, 2], room::RECOMMENDED);
    let conv = ps[0].conv();
    for p in &ps {
        assert_eq!(p.detail(&conv).summary.members, 3);
    }
    assert_eq!(
        room_heads(&ps[1]),
        0,
        "Bob used his own head, not the vouched one"
    );
    assert_eq!(room_heads(&ps[2]), 0);
}

fn hex_dev(p: &P) -> Vec<u8> {
    om_codec::cbor::unhex(&p.device_hex())
}

// ---------------------------------------------------------------------------------------------------------------------
macro_rules! core {
    ($p:expr, $m:ident $(, $a:expr)*) => {{
        let p = &mut $p;
        p.rt.core.$m(&p.ctx $(, $a)*)
    }};
}

// Core owns the relationship-lane items and the route updates until the host has moved them (ADR-0189 1.1 and 1.3,
// ADR-0120 K1; reports D57 and D58). Each person is on a different relay.

/// Three accounts, each on its own relay (home 1, 2, 3), all pairwise connected.
fn trio_apart() -> Vec<P> {
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, "alice-out", "Alice"),
        mk(&net, "bob-out", "Bob"),
        mk(&net, "carol-out", "Carol"),
    ];
    for (i, p) in ps.iter_mut().enumerate() {
        p.rt.homes = vec![i as u8 + 1];
    }
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2);
    connect(&mut ps, 1, 2);
    ps
}

impl P {
    fn status(&self) -> om_core::engine::GroupCarriageStatus {
        self.rt.core.group_carriage_status(&self.ctx).unwrap()
    }
    /// What the driver does: take every kept lane item into the 1:1 queue.
    fn flush_lane(&mut self) -> usize {
        let mut n = 0;
        loop {
            self.rt.supply_routes(4);
            let (r, step) = self.rt.core.group_lane_flush(&self.ctx).unwrap();
            assert!(self.rt.settle(&step));
            match r {
                Ok(true) => n += 1,
                Ok(false) => return n,
                Err(e) => panic!("lane flush: {e}"),
            }
        }
    }
    fn restart(&mut self) {
        let ctxs = self.rt.restart();
        self.ctx = ctxs.into_iter().next().unwrap();
    }
    fn carriage(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()["groupCarriage"].clone()
    }
    fn account_hex(&self, conv: &[u8]) -> Vec<u8> {
        let d = self.detail(conv);
        om_codec::cbor::unhex(&d.people.iter().find(|m| m.you).unwrap().account)
    }
}

#[test]
fn invitations_are_kept_by_core_across_a_restart_and_leave_exactly_once() {
    let mut ps = trio_apart();
    let rels = vec![ps[0].rels[&1].clone(), ps[0].rels[&2].clone()];
    ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Book club".into(),
            relationship_ids: rels,
            protection: room::RECOMMENDED,
        })
        .unwrap();
    // no driver keeper exists: the two invitations are in Core's own state, in the step's commit
    assert_eq!(ps[0].status().lane_out, 2);
    ps[0].restart();
    assert_eq!(
        ps[0].status().lane_out,
        2,
        "the kept items survive a restart"
    );
    assert_eq!(ps[0].flush_lane(), 2);
    assert_eq!(ps[0].status().lane_out, 0);
    assert_eq!(ps[0].flush_lane(), 0, "nothing is sent twice");
    settle_every(&mut ps);
    for i in [1usize, 2] {
        let reps = ps[i].pump_lane();
        let invites = events(&reps)
            .into_iter()
            .filter(|e| matches!(e, GroupEvent::InviteReceived { .. }))
            .count();
        assert_eq!(invites, 1, "person {i} got exactly one invitation");
    }
}

#[test]
fn a_caller_that_still_sends_the_lane_itself_does_not_duplicate_it() {
    let mut ps = trio_apart();
    let rel = ps[0].rels[&1].clone();
    let rep = ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Book club".into(),
            relationship_ids: vec![rel],
            protection: room::RECOMMENDED,
        })
        .unwrap();
    ps[0].send_lane(&rep);
    assert_eq!(ps[0].status().lane_out, 0, "taken in the same transaction");
    assert_eq!(ps[0].flush_lane(), 0);
    settle_every(&mut ps);
    let reps = ps[1].pump_lane();
    assert_eq!(
        events(&reps)
            .iter()
            .filter(|e| matches!(e, GroupEvent::InviteReceived { .. }))
            .count(),
        1
    );
}

#[test]
fn the_lane_keeper_is_bounded_and_a_full_one_refuses_the_step() {
    let mut ps = trio_apart();
    let rel = ps[0].rels[&1].clone();
    let mut refused = None;
    for i in 0..(MAX_LANE_OUT + 1) {
        match ps[0].dob(GroupIntent::CreateWithProtection {
            name: format!("Room {i}"),
            relationship_ids: vec![rel.clone()],
            protection: room::RECOMMENDED,
        }) {
            Ok(_) => assert!(i < MAX_LANE_OUT),
            Err(e) => {
                refused = Some((i, e.0));
                break;
            }
        }
    }
    assert_eq!(refused, Some((MAX_LANE_OUT, "lane_out_full")));
    assert_eq!(ps[0].status().lane_out, MAX_LANE_OUT);
}

#[test]
fn a_lane_item_that_needs_a_route_offer_waits_and_goes_once() {
    let mut ps = trio_apart();
    let rel = ps[0].rels[&1].clone();
    let rep = ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Book club".into(),
            relationship_ids: vec![rel],
            protection: room::RECOMMENDED,
        })
        .unwrap();
    ps[0].send_lane(&rep);
    settle_every(&mut ps);
    let id = events(&ps[1].pump_lane())
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .unwrap();
    ps[1]
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    assert_eq!(
        ps[1].status().lane_out,
        1,
        "the Accept is Core's until the lane takes it"
    );
    // a restart empties the in-memory route offers; the Accept releases a route minted for this Room, so it waits
    ps[1].restart();
    assert_eq!(ps[1].status().lane_out, 1);
    let (r, step) = core!(ps[1], group_lane_flush).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Err(GroupError("no_route_offer")));
    assert_eq!(ps[1].status().lane_out, 1, "kept, never invented");
    assert_eq!(ps[1].status().lane_lost, 0);
    // once offers exist it goes, exactly once
    assert_eq!(ps[1].flush_lane(), 1);
    assert_eq!(ps[1].status().lane_out, 0);
    assert_eq!(ps[1].flush_lane(), 0);
}

#[test]
fn a_lane_item_older_than_the_horizon_is_dropped_and_counted() {
    let mut ps = trio_apart();
    let rel = ps[0].rels[&1].clone();
    let rep = ps[0]
        .dob(GroupIntent::CreateWithProtection {
            name: "Book club".into(),
            relationship_ids: vec![rel],
            protection: room::RECOMMENDED,
        })
        .unwrap();
    ps[0].send_lane(&rep);
    settle_every(&mut ps);
    let id = events(&ps[1].pump_lane())
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .unwrap();
    ps[1]
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    ps[1].restart();
    ps[1].rt.tick((LANE_OUT_TTL_S + 1) * 1000);
    let (r, step) = core!(ps[1], group_lane_flush).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Ok(true));
    assert_eq!(ps[1].status().lane_out, 0);
    assert_eq!(
        ps[1].status().lane_lost,
        1,
        "counted, so the person is told"
    );
}

/// Alice removes Carol from a three-person Room (three relays): returns the Room id.
fn remove_carol() -> (Vec<P>, Vec<u8>) {
    let mut ps = trio_apart();
    let conv = join_all(&mut ps, 0, &[1, 2], room::RECOMMENDED);
    let carol = ps[2].account_hex(&conv);
    let rep = ps[0]
        .dob(GroupIntent::Remove {
            conversation_id: conv.clone(),
            account: carol,
        })
        .unwrap();
    assert!(
        events(std::slice::from_ref(&rep))
            .iter()
            .any(|e| matches!(e, GroupEvent::CommitPrepared { .. }))
    );
    // the Commit reaches Bob; Carol's copy may or may not apply, she is out either way
    for it in ps[0].outbox() {
        let o = it.to_outbound().unwrap();
        for s in ps[0].seal(&o).unwrap() {
            let to = (0..3)
                .find(|i| ps[*i].device_hex() == hex(&s.device))
                .unwrap();
            let _ = ps[to].open(&s.wrapper);
        }
        let step = core!(ps[0], group_outbox_done, &[it.id]).unwrap();
        assert!(ps[0].rt.settle(&step));
    }
    (ps, conv)
}

#[test]
fn a_route_update_is_queued_in_cores_outbox_survives_a_restart_and_is_retried_until_deposited() {
    let (mut ps, conv) = remove_carol();
    let hexconv = hex(&conv);
    for i in [0usize, 1] {
        assert!(
            ps[i].status().route_owed.contains(&hexconv),
            "member {i} owes a route"
        );
    }
    // Bob mints and queues in one transaction: the owed mark and the frame move together
    ps[1].rt.tick(2000); // a route must outlive the one it replaces (route_not_newer)
    ps[1].rt.supply_routes(4);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(
        r,
        Ok(1),
        "one frame, for Alice: Carol is no longer in the roster"
    );
    assert!(ps[1].status().route_owed.is_empty());
    let queued: Vec<OutItem> = ps[1]
        .outbox()
        .into_iter()
        .filter(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .collect();
    assert_eq!(queued.len(), 1);
    // a crash before the host touched it: nothing is lost
    ps[1].restart();
    let queued2: Vec<OutItem> = ps[1]
        .outbox()
        .into_iter()
        .filter(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .collect();
    assert_eq!(queued2, queued);
    // the deposit failed (the host never reports done), so the item is sealed again on the next pass and still reaches Alice
    let o = queued2[0].to_outbound().unwrap();
    let first = ps[1].seal(&o).unwrap();
    let second = ps[1].seal(&o).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(
        ps[1]
            .outbox()
            .iter()
            .filter(|x| x.id == queued2[0].id)
            .count(),
        1,
        "still owned until reported deposited"
    );
    let before = ps[0].carriage()["peers"].clone();
    ps[0].open(&second[0].wrapper).unwrap();
    let after = ps[0].carriage()["peers"].clone();
    assert_ne!(before, after, "Alice learned Bob's fresh route");
    // the same wrapper again (a replay, or a double deposit) changes nothing
    let snap = ps[0].carriage();
    let _ = ps[0].open(&second[0].wrapper);
    assert_eq!(ps[0].carriage()["peers"], snap["peers"]);
    // reported deposited: gone
    let step = core!(ps[1], group_outbox_done, &[queued2[0].id]).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(
        ps[1]
            .outbox()
            .iter()
            .all(|x| !matches!(x.wire, GroupWire::RouteUpdate { .. }))
    );
}

#[test]
fn a_newer_route_update_replaces_the_queued_one_and_a_stale_one_expires() {
    let (mut ps, conv) = remove_carol();
    ps[1].rt.supply_routes(8);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Ok(1));
    let first: Vec<u64> = ps[1].outbox().iter().map(|o| o.id).collect();
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Ok(1));
    let updates: Vec<OutItem> = ps[1]
        .outbox()
        .into_iter()
        .filter(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .collect();
    assert_eq!(updates.len(), 1, "only the latest route is worth sending");
    assert!(!first.contains(&updates[0].id));
    // After the overlap the old capability is revoked; a fresh frame still advertises the current route.
    ps[1].rt.tick((ROUTE_OVERLAP_S + 1) * 1000);
    let (_revoke, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(
        ps[1]
            .outbox()
            .iter()
            .filter(|x| matches!(x.wire, GroupWire::RouteUpdate { .. }))
            .count(),
        1
    );
}

#[test]
fn a_route_update_that_cannot_be_minted_leaves_the_room_owed() {
    let (mut ps, conv) = remove_carol();
    let hexconv = hex(&conv);
    // no offers left to mint a route with: nothing changes and the call can be repeated
    ps[1].rt.core.supply_routes(vec![]);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    if let Err(e) = r {
        assert_eq!(e.0, "no_route_offer");
        assert!(ps[1].status().route_owed.contains(&hexconv));
        assert!(
            ps[1]
                .outbox()
                .iter()
                .all(|x| !matches!(x.wire, GroupWire::RouteUpdate { .. }))
        );
    }
    ps[1].rt.supply_routes(4);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(r.is_ok());
}

#[test]
fn route_distribution_survives_offline_expiry_and_restart_with_two_survivors() {
    let (mut ps, conv) = remove_carol();
    ps[1].rt.tick(2000);
    ps[1].rt.supply_routes(4);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Ok(1));
    let queued = ps[1]
        .outbox()
        .into_iter()
        .find(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .unwrap();
    ps[1].rt.tick((ROUTE_OVERLAP_S + 1) * 1000);
    ps[1].restart();
    let (revoke, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(!revoke.is_empty());
    ps[1].restart();
    let (recovered, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(
        recovered, revoke,
        "retirement must survive a crash before DELETE"
    );
    let old_cap = [vec![0xc4; 16], om_codec::cbor::unhex(&recovered[0].mailbox)].concat();
    let net = ps[1].rt.net.clone();
    let old_deposit = om_core::host::NetCall::Deposit {
        mailbox: old_cap,
        relay_uri: Some(recovered[0].relay_uri.clone()),
        bytes: vec![1, 2, 3],
    };
    assert_eq!(net.perform(&old_deposit), om_core::host::NetResult::Ok);
    for grant in recovered {
        net.revoke_deposit(&grant);
        net.revoke_deposit(&grant); // exact DELETE retry
        let step = core!(ps[1], group_route_revoked, &grant).unwrap();
        assert!(ps[1].rt.settle(&step));
    }
    assert_eq!(
        net.perform(&old_deposit),
        om_core::host::NetResult::Err {
            error: om_core::host::NetError::Rejected {
                reason: RejectReason::Revoked
            },
        }
    );
    let (cleared, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(cleared.is_empty());
    let retry = ps[1]
        .outbox()
        .into_iter()
        .find(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .expect("current route still owed");
    assert_ne!(queued.id, retry.id);
    assert_eq!(queued.wire, retry.wire);
    let sealed = ps[1].seal(&retry.to_outbound().unwrap()).unwrap();
    ps[0].open(&sealed[0].wrapper).unwrap();
    let step = core!(ps[1], group_outbox_done, &[queued.id]).unwrap();
    assert!(ps[1].rt.settle(&step)); // a late completion of the expired item cannot clear the debt
    assert_eq!(ps[1].carriage()["routeDebt"].as_array().unwrap().len(), 1);
    let step = core!(ps[1], group_outbox_done, &[retry.id]).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(ps[1].carriage()["routeDebt"].as_array().unwrap().is_empty());
    // The other surviving device can now advertise its replacement through the route it just learned.
    ps[0].rt.tick((ROUTE_OVERLAP_S + 3) * 1000);
    ps[0].rt.supply_routes(4);
    let (r, step) = core!(ps[0], group_route_update, &conv).unwrap();
    assert!(ps[0].rt.settle(&step));
    assert_eq!(r, Ok(1));
    let update = ps[0]
        .outbox()
        .into_iter()
        .find(|o| matches!(o.wire, GroupWire::RouteUpdate { .. }))
        .unwrap();
    let sealed = ps[0].seal(&update.to_outbound().unwrap()).unwrap();
    ps[1].open(&sealed[0].wrapper).unwrap();
}

// ADR-0229 section 3 item 2: a dedicated queue is retired at its Home after the ADR-0214 overlap and every grant revocation on
// it are over; the debt is durable, retried with a backoff and the same request id, and cleared only on an acknowledgement.

fn replaced_route_after_overlap() -> Vec<P> {
    let (mut ps, conv) = remove_carol();
    ps[1].rt.supply_routes(8);
    let (r, step) = core!(ps[1], group_route_update, &conv).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(r, Ok(1));
    ps
}

#[test]
fn a_replaced_queue_is_retired_only_after_the_overlap_and_its_grant_debt() {
    let mut ps = replaced_route_after_overlap();
    let (due, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(due.is_empty(), "the overlap has not ended");
    ps[1].rt.tick((ROUTE_OVERLAP_S + 1) * 1000);
    let (revoke, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(!revoke.is_empty());
    let (due, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(
        due.is_empty(),
        "a grant revocation on the queue is outstanding"
    );
    for grant in &revoke {
        let step = core!(ps[1], group_route_revoked, grant).unwrap();
        assert!(ps[1].rt.settle(&step));
    }
    let (due, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(due.len(), revoke.len());
    for (q, g) in due.iter().zip(&revoke) {
        assert_eq!(
            q.relay_uri, g.relay_uri,
            "only the Home that holds the queue"
        );
        assert_eq!(q.mailbox, g.mailbox);
        assert_eq!(q.request_id.len(), 32);
        assert!(q.request_id.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

#[test]
fn the_retire_debt_survives_restart_retries_with_backoff_and_the_same_request_id() {
    let mut ps = replaced_route_after_overlap();
    ps[1].rt.tick((ROUTE_OVERLAP_S + 1) * 1000);
    let (revoke, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    for grant in &revoke {
        let step = core!(ps[1], group_route_revoked, grant).unwrap();
        assert!(ps[1].rt.settle(&step));
    }
    let (first, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(!first.is_empty());
    // the reply is lost and the device restarts: nothing is repeated before the backoff, and nothing is forgotten
    ps[1].restart();
    let (early, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(early.is_empty());
    assert_eq!(
        ps[1].carriage()["queueRetire"].as_array().unwrap().len(),
        first.len()
    );
    ps[1].rt.tick((cg::QUEUE_RETIRE_BACKOFF_S + 1) * 1000);
    let (again, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(again, first, "an exact retry: the same request id");
    // the backoff doubles
    ps[1].rt.tick((cg::QUEUE_RETIRE_BACKOFF_S + 1) * 1000);
    let (early, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(early.is_empty());
    ps[1].rt.tick((cg::QUEUE_RETIRE_BACKOFF_S + 1) * 1000);
    let (third, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(third, first);
    // only an acknowledgement clears it
    for q in &third {
        let step = core!(ps[1], group_queue_retired, q).unwrap();
        assert!(ps[1].rt.settle(&step));
    }
    assert!(
        ps[1].carriage()["queueRetire"]
            .as_array()
            .is_none_or(|a| a.is_empty())
    );
    ps[1].restart();
    ps[1].rt.tick(cg::QUEUE_RETIRE_BACKOFF_MAX_S * 2 * 1000);
    let (none, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert!(none.is_empty());
}

#[test]
fn an_acknowledgement_for_another_request_id_does_not_clear_the_debt() {
    let mut ps = replaced_route_after_overlap();
    ps[1].rt.tick((ROUTE_OVERLAP_S + 1) * 1000);
    let (revoke, step) = core!(ps[1], group_routes_expire).unwrap();
    assert!(ps[1].rt.settle(&step));
    for grant in &revoke {
        let step = core!(ps[1], group_route_revoked, grant).unwrap();
        assert!(ps[1].rt.settle(&step));
    }
    let (due, step) = core!(ps[1], group_queue_retires).unwrap();
    assert!(ps[1].rt.settle(&step));
    let mut wrong = due[0].clone();
    wrong.request_id = "00".repeat(16);
    let step = core!(ps[1], group_queue_retired, &wrong).unwrap();
    assert!(ps[1].rt.settle(&step));
    assert_eq!(
        ps[1].carriage()["queueRetire"].as_array().unwrap().len(),
        due.len()
    );
}
