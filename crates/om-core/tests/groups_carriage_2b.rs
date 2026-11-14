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
    let mut rt = Runtime::new(name, net);
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
            Err(e) => panic!("sealing {:?}: {e}", it.wire),
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

/// Alice invites Bob, then Carol, who has never met Bob. `heads`: what Alice's Welcome carries for the accounts it names
/// (0 their signed heads, 1 nothing, 2 a corrupted head). With 0 Carol joins a three-account Room; otherwise the
/// Welcome is held, never accepted unverified, bounded and expiring (D37, D44).
fn welcome_trio(tag: &str, heads: u8) -> Vec<P> {
    om_core::runtime::TEST_ROOM_HEADS.with(|m| m.set(heads));
    let net = LoopbackNetwork::default();
    let mut ps = vec![
        mk(&net, &format!("alice-{tag}"), "Alice"),
        mk(&net, &format!("bob-{tag}"), "Bob"),
        mk(&net, &format!("carol-{tag}"), "Carol"),
    ];
    connect(&mut ps, 0, 1);
    connect(&mut ps, 0, 2); // Carol and Bob have never met
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
}

/// A head that does not verify, or none at all, leaves the Welcome held until it expires.
#[test]
fn a_welcome_whose_head_does_not_verify_is_held_until_it_expires() {
    for heads in [1u8, 2] {
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

fn hex_dev(p: &P) -> Vec<u8> {
    om_codec::cbor::unhex(&p.device_hex())
}
