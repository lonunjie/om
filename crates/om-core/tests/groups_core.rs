// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups through the real `Core` (ADR-0177, slice 3 surface): `Core::group_do`, `group_receive` and
//! `group_query` as a host drives them, with two real profiles that are `Active` in a 1:1 relationship (so the
//! device keys, account ids and the engine cache are Core's own, not a test fixture). The engine is the test-only
//! fake by default and stock OpenMLS with `--features groups-openmls` (ADR-0187, proposed).

use om_core::api::*;
use om_core::crypto::{hex, unhex};
use om_core::groups::GroupError;
use om_core::groups::flow::*;
use om_core::host::Step;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Party {
    rt: Runtime,
    ctx: ProfileExecutionContext,
    /// This person's relationship id with the other person.
    rel: String,
}

fn create_profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "create-1",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

fn connected() -> (Party, Party) {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    alice.dispatch(&a, "inv-1", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let invite: Option<InviteView> = alice.query(Query::GetInvite {
        ctx: a.clone(),
        operation_id: "inv-1".into(),
    });
    let link = invite.expect("invite view").link;
    bob.dispatch(&b, "acc-1", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "intro-1",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let al: Vec<ConversationSummary> = alice.query(Query::GetConversationList { ctx: a.clone() });
    let bl: Vec<ConversationSummary> = bob.query(Query::GetConversationList { ctx: b.clone() });
    (
        Party {
            rt: alice,
            ctx: a,
            rel: al[0].id.clone(),
        },
        Party {
            rt: bob,
            ctx: b,
            rel: bl[0].id.clone(),
        },
    )
}

impl Party {
    /// One typed step, settled through the store; the commit must be durable before anything is sent.
    fn dob(&mut self, intent: GroupIntent) -> Result<GroupReply, GroupError> {
        let (out, step): (_, Step) = self.rt.core.group_do(&self.ctx, intent).unwrap();
        assert!(self.rt.settle(&step));
        out
    }
    fn recv(&mut self, from: Sender, wire: GroupWire) -> Result<GroupReply, GroupError> {
        let (out, step) = self.rt.core.group_receive(&self.ctx, from, wire).unwrap();
        assert!(self.rt.settle(&step));
        out
    }
    fn view(&self, q: GroupQuery) -> GroupView {
        self.rt.core.group_query(&self.ctx, q).unwrap().unwrap()
    }
    fn texts(&self, conv: &[u8]) -> Vec<String> {
        match self.view(GroupQuery::Messages {
            conversation_id: conv.to_vec(),
        }) {
            GroupView::Messages(m) => m.into_iter().map(|x| x.text).collect(),
            _ => unreachable!(),
        }
    }
    fn conv(&self) -> Vec<u8> {
        match self.view(GroupQuery::Groups) {
            GroupView::Groups(g) => unhex(&g[0].conversation_id).unwrap(),
            _ => unreachable!(),
        }
    }
    /// This party's own device id, from the group's device directory (the one that is not `other`).
    fn device_in(&self, conv: &[u8], other: &[u8]) -> Vec<u8> {
        let st = self.rt.core.debug_app_state(&self.ctx).unwrap();
        st["groupDevices"][hex(conv)]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| unhex(k).unwrap())
            .find(|d| d != other)
            .unwrap()
    }
}

/// Deliver every outbound of `rep` from party `from` to the other party; return all events as `(party, event)`.
fn pump(p: &mut [Party; 2], from: usize, rep: GroupReply) -> Vec<(usize, GroupEvent)> {
    let mut ev: Vec<(usize, GroupEvent)> = rep.events.into_iter().map(|e| (from, e)).collect();
    let to = 1 - from;
    for o in rep.out {
        let sender = match &o.to {
            Target::Relationship(_) => Sender::Relationship(p[to].rel.clone()),
            Target::Device { device, .. } => {
                // the receiving device is `device`; the sender is the other device of the 2-device directory
                let conv = unhex(&match &o.wire {
                    GroupWire::Commit {
                        conversation_id, ..
                    }
                    | GroupWire::App {
                        conversation_id, ..
                    }
                    | GroupWire::LeaveRequest {
                        conversation_id, ..
                    } => conversation_id.clone(),
                    _ => unreachable!("device-targeted wires carry the conversation id"),
                })
                .unwrap();
                Sender::Device(p[from].device_in(&conv, device))
            }
        };
        let r = p[to].recv(sender, o.wire).unwrap();
        ev.extend(pump(p, to, r));
    }
    ev
}

#[test]
fn group_flow_runs_through_core_with_real_devices_and_survives_a_restart() {
    let (a, b) = connected();
    let mut p = [a, b];
    // Alice creates a group inviting Bob over their Active relationship
    let rel = p[0].rel.clone();
    let rep = p[0]
        .dob(GroupIntent::Create {
            name: "Book club".into(),
            relationship_ids: vec![rel],
        })
        .unwrap();
    let ev = pump(&mut p, 0, rep);
    let conv = p[0].conv();
    let inv = ev
        .iter()
        .find_map(|(i, e)| match (i, e) {
            (1, GroupEvent::InviteReceived { invite_id }) => Some(invite_id.clone()),
            _ => None,
        })
        .expect("Bob got the invitation");
    // Bob sees it and accepts; Alice prepares the Add
    match p[1].view(GroupQuery::Invites) {
        GroupView::Invites(i) => assert_eq!(i.len(), 1),
        _ => unreachable!(),
    }
    let rep = p[1]
        .dob(GroupIntent::Accept {
            invite_id: inv,
            siblings: vec![],
        })
        .unwrap();
    let ev = pump(&mut p, 1, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 0 && matches!(e, GroupEvent::CommitPrepared { .. })),
        "{ev:?}"
    );
    // nothing can be sent under the unmerged candidate
    assert_eq!(
        p[0].dob(GroupIntent::SendText {
            conversation_id: conv.clone(),
            text: "x".into()
        })
        .unwrap_err()
        .0,
        "commit_in_flight"
    );
    // publish, accept, Welcome; then the engine must survive a restart of Alice's Core (rebuilt from the store)
    let id = ev
        .iter()
        .find_map(|(_, e)| match e {
            GroupEvent::CommitPrepared { intent_id } => Some(intent_id.clone()),
            _ => None,
        })
        .unwrap();
    p[0].dob(GroupIntent::CommitPublished {
        intent_id: id.clone(),
    })
    .unwrap();
    p[0].ctx = p[0].rt.restart().remove(0);
    // Core accepts its own Commit (G10): durable in the journal and the outbox, parent canonical, nothing better
    // received. The restart above lost nothing, and no host report is involved.
    let (reps, step) = p[0].rt.core.group_settle(&p[0].ctx).unwrap();
    assert!(p[0].rt.settle(&step));
    let mut rep = GroupReply::default();
    for r in reps.unwrap() {
        rep.events.extend(r.events);
        rep.out.extend(r.out);
    }
    assert_eq!(
        p[0].rt
            .core
            .group_settle(&p[0].ctx)
            .unwrap()
            .0
            .unwrap()
            .len(),
        0,
        "settling twice does nothing more"
    );
    let _ = id;
    let ev = pump(&mut p, 0, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 1 && matches!(e, GroupEvent::Joined { .. }))
    );
    // text both ways
    let rep = p[0]
        .dob(GroupIntent::SendText {
            conversation_id: conv.clone(),
            text: "hello".into(),
        })
        .unwrap();
    assert!(
        rep.out
            .iter()
            .all(|o| matches!(o.to, Target::Device { .. }))
    );
    pump(&mut p, 0, rep);
    p[1].ctx = p[1].rt.restart().remove(0);
    let rep = p[1]
        .dob(GroupIntent::SendText {
            conversation_id: conv.clone(),
            text: "hi A".into(),
        })
        .unwrap();
    pump(&mut p, 1, rep);
    for party in &p {
        assert_eq!(party.texts(&conv), vec!["hello", "hi A"]);
    }
    // Bob leaves: Alice's Core prepares the removal
    let rep = p[1]
        .dob(GroupIntent::Leave {
            conversation_id: conv.clone(),
        })
        .unwrap();
    let ev = pump(&mut p, 1, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 0 && matches!(e, GroupEvent::CommitPrepared { .. }))
    );
}

#[test]
fn a_refused_step_changes_nothing_and_a_failed_commit_reloads_the_engine() {
    let (a, b) = connected();
    let mut p = [a, b];
    // unknown relationship: a typed refusal, no commit
    let (out, step) = p[0]
        .rt
        .core
        .group_do(
            &p[0].ctx,
            GroupIntent::Create {
                name: "x".into(),
                relationship_ids: vec!["nope".into()],
            },
        )
        .unwrap();
    assert!(out.is_err());
    assert!(step.commit.is_none());
    match p[0].view(GroupQuery::Groups) {
        GroupView::Groups(g) => assert!(g.is_empty()),
        _ => unreachable!(),
    }
    // the host cannot make the commit durable: nothing is released, and the next attempt works
    let rel = p[0].rel.clone();
    let (out, step) = p[0]
        .rt
        .core
        .group_do(
            &p[0].ctx,
            GroupIntent::Create {
                name: "Club".into(),
                relationship_ids: vec![rel.clone()],
            },
        )
        .unwrap();
    assert!(out.is_ok());
    let c = step.commit.unwrap();
    p[0].rt.core.finish_commit(c.id, false).unwrap();
    match p[0].view(GroupQuery::Groups) {
        GroupView::Groups(g) => assert!(g.is_empty()),
        _ => unreachable!(),
    }
    let rep = p[0]
        .dob(GroupIntent::Create {
            name: "Club".into(),
            relationship_ids: vec![rel],
        })
        .unwrap();
    assert_eq!(rep.out.len(), 1);
    // a stale context is refused like every other call
    let stale = ProfileExecutionContext {
        generation: "old".into(),
        ..p[0].ctx.clone()
    };
    assert!(
        p[0].rt
            .core
            .group_query(&stale, GroupQuery::Groups)
            .is_err()
    );
}
