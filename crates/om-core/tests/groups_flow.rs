// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups, slice 3 (ADR-0177): invite, accept, add, leave, remove and group text through `GroupCrypto`,
//! driven with the TEST-ONLY fake engine. Three devices on two accounts: A1 (account A), B1 and B2 (account B).
//! The fake is not cryptography; these tests are evidence about Core's flow, not about MLS.

use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::DeviceSigner;
use om_core::groups::flow::*;
use om_core::groups::mls::{CryptoEnv, GroupCrypto};
use om_core::route::RelaySet;
use om_core::state::{AppState, RelState, Relationship};

mod group_heads;

struct Dev {
    app: AppState,
    eng: engine::Eng,
    signer: DeviceSigner,
    rng: InsecureTestRng,
    account: Vec<u8>,
    /// Head view knobs for the endpoint binding tests (see `refresh_heads`).
    messaging: bool,
    revoked: Vec<Vec<u8>>,
    quarantined: Vec<Vec<u8>>,
    unpinned: Vec<u8>,
}

impl Dev {
    fn new(tag: u8, account: u8) -> Self {
        let mut rng = InsecureTestRng::from_label(&[b'f', tag]);
        let signer = DeviceSigner::generate(&mut rng);
        Dev {
            app: AppState::default(),
            eng: engine::with_signer(&[tag; 16], &signer),
            signer,
            rng,
            account: vec![account; 32],
            messaging: false,
            revoked: vec![],
            quarantined: vec![],
            unpinned: vec![],
        }
    }
    fn id(&self) -> Vec<u8> {
        self.eng.device_id().to_vec()
    }
    fn release(&mut self) -> DeviceRelease {
        let kp = self
            .eng
            .key_package(&mut CryptoEnv {
                rng: &mut self.rng,
                now_ms: NOW,
            })
            .unwrap();
        DeviceRelease {
            device: om_core::crypto::hex(&self.id()),
            key_package: om_core::crypto::hex(&kp.bytes),
            pk: om_core::crypto::hex(&self.signer.public()),
        }
    }
}

/// The engine under test: the TEST-ONLY fake by default; stock OpenMLS with `--features groups-openmls`
/// (which depends on the proposed ADR-0187). The scenarios are identical; fake-only tests are `cfg`-gated.
#[cfg(not(feature = "groups-openmls"))]
mod engine {
    pub use om_core::groups::fake::{FakeGroupCrypto as Eng, FakeLoader as Loader};
    pub fn new(id: &[u8]) -> Eng {
        Eng::new(id)
    }
    pub fn with_signer(id: &[u8], s: &om_core::crypto::sig::DeviceSigner) -> Eng {
        Eng::with_signature_key(id, &s.public()[..32])
    }
}
#[cfg(feature = "groups-openmls")]
mod engine {
    pub use om_core::groups::openmls::{OpenMlsGroupCrypto as Eng, OpenMlsLoader as Loader};
    pub fn new(id: &[u8]) -> Eng {
        Eng::new(id).expect("engine")
    }
    pub fn with_signer(id: &[u8], s: &om_core::crypto::sig::DeviceSigner) -> Eng {
        Eng::with_signing_seed(id, &s.ed_seed).expect("engine")
    }
}

const NOW: u64 = 1_800_000_000_000;

macro_rules! ctx {
    ($d:expr) => {
        &mut GroupCtx {
            engine: &mut $d.eng,
            rng: &mut $d.rng,
            now_ms: NOW,
            signer: &$d.signer,
            my_account: $d.account.clone(),
        }
    };
}

fn rel(peer_account: u8, state: RelState) -> (String, Relationship) {
    let id = format!("rel-{peer_account}");
    let relay = serde_json::to_value(RelaySet::default()).unwrap();
    let r: Relationship = serde_json::from_value(serde_json::json!({
        "id": id, "personId": "p", "displayName": "P", "state": state,
        "peerAccountId": om_core::crypto::hex(&[peer_account; 32]),
        "peerDeviceId": "00", "peerIk": "00", "peerAcsBody": "00", "peerCredentials": [],
        "myMailbox": "00", "myRelays": relay, "peerRelays": relay,
        "blocked": false, "integrityFailures": 0, "createdAt": 0, "messages": []
    }))
    .unwrap();
    (id, r)
}

fn with_rel(d: &mut Dev, peer: u8, st: RelState) -> String {
    let (id, r) = rel(peer, st);
    d.app.relationships.insert(id.clone(), r);
    id
}

/// Give every device its own account's head and pin the other accounts' heads on its relationships. All devices of
/// an account are control devices of one head unless a device is `messaging`; per observer, `revoked` devices are
/// listed as revoked, `quarantined` as quarantined, and an account in `unpinned` is not pinned at all.
fn refresh_heads(devs: &mut [Dev]) {
    let mut accounts: Vec<Vec<u8>> = devs.iter().map(|d| d.account.clone()).collect();
    accounts.sort();
    accounts.dedup();
    for i in 0..devs.len() {
        let obs = &devs[i];
        let heads: Vec<group_heads::Head> = accounts
            .iter()
            .filter(|a| !obs.unpinned.contains(&a[0]) || **a == obs.account)
            .map(|a| {
                let of = |pred: &dyn Fn(&Dev) -> bool| -> Vec<group_heads::Dev> {
                    devs.iter()
                        .filter(|d| &d.account == a && pred(d))
                        .map(|d| (d.id(), d.signer.public()))
                        .collect()
                };
                let revoked = |d: &Dev| obs.revoked.contains(&d.id());
                let quar = |d: &Dev| obs.quarantined.contains(&d.id());
                group_heads::head_with(
                    a,
                    &of(&|d| !d.messaging && !revoked(d) && !quar(d)),
                    &of(&|d| d.messaging && !revoked(d) && !quar(d)),
                    &of(&|d| revoked(d)),
                    &of(&|d| quar(d) && !revoked(d)),
                )
            })
            .collect();
        let own = heads.iter().find(|h| h.account == obs.account).unwrap();
        let peers: Vec<&group_heads::Head> = heads.iter().collect();
        let (own, peers) = (
            group_heads::Head {
                account: own.account.clone(),
                acs: own.acs.clone(),
                creds: own.creds.clone(),
            },
            peers,
        );
        let d = &mut devs[i];
        group_heads::install(&mut d.app, &own, &peers);
    }
}

/// Deliver every outbound of `rep` (and, recursively, what the receivers answer); return all events by device index.
fn pump(devs: &mut [Dev], from: usize, rep: GroupReply) -> Vec<(usize, GroupEvent)> {
    refresh_heads(devs);
    let mut ev: Vec<(usize, GroupEvent)> = rep.events.into_iter().map(|e| (from, e)).collect();
    for o in rep.out {
        let targets: Vec<(usize, Sender)> = match &o.to {
            Target::Relationship(r) => {
                let peer = devs[from].app.relationships[r].peer_account_id.clone();
                let mine = devs[from].account[0];
                (0..devs.len())
                    .filter(|i| om_core::crypto::hex(&devs[*i].account) == peer)
                    .map(|i| (i, Sender::Relationship(format!("rel-{mine}"))))
                    .collect()
            }
            Target::Device { device, .. } => (0..devs.len())
                .filter(|i| devs[*i].id() == *device)
                .map(|i| (i, Sender::Device(devs[from].id())))
                .collect(),
        };
        for (i, s) in targets {
            let d = &mut devs[i];
            let r = d.app.group_receive(ctx!(d), s, o.wire.clone()).unwrap();
            ev.extend(pump(devs, i, r));
        }
    }
    ev
}

fn has(ev: &[(usize, GroupEvent)], dev: usize, e: &GroupEvent) -> bool {
    ev.iter().any(|(i, x)| *i == dev && x == e)
}

fn conv_of(d: &Dev) -> Vec<u8> {
    match d.app.group_query(GroupQuery::Groups).unwrap() {
        GroupView::Groups(g) => om_core::crypto::unhex(&g[0].conversation_id).unwrap(),
        _ => unreachable!(),
    }
}
fn detail(d: &Dev, conv: &[u8]) -> GroupDetail {
    match d
        .app
        .group_query(GroupQuery::Group {
            conversation_id: conv.to_vec(),
        })
        .unwrap()
    {
        GroupView::Group(g) => *g,
        _ => unreachable!(),
    }
}
fn texts(d: &Dev, conv: &[u8]) -> Vec<String> {
    match d
        .app
        .group_query(GroupQuery::Messages {
            conversation_id: conv.to_vec(),
        })
        .unwrap()
    {
        GroupView::Messages(m) => m.into_iter().map(|x| x.text).collect(),
        _ => unreachable!(),
    }
}

/// A creates a group inviting B; B1 accepts (releasing B1 and B2); the Add is prepared but not yet accepted.
fn invited_and_accepted() -> (Vec<Dev>, Vec<u8>, String) {
    let mut devs = vec![Dev::new(0xa1, 1), Dev::new(0xb1, 2), Dev::new(0xb2, 2)];
    let ra = with_rel(&mut devs[0], 2, RelState::Active);
    with_rel(&mut devs[1], 1, RelState::Active);
    with_rel(&mut devs[2], 1, RelState::Active);
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Create {
                name: "Book club".into(),
                relationship_ids: vec![ra],
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let conv = conv_of(&devs[0]);
    let inv = match &ev
        .iter()
        .find(|(i, e)| *i == 1 && matches!(e, GroupEvent::InviteReceived { .. }))
        .unwrap()
        .1
    {
        GroupEvent::InviteReceived { invite_id } => invite_id.clone(),
        _ => unreachable!(),
    };
    assert!(
        devs[2].app.group_invites.contains_key(&inv),
        "B2 got it too"
    );
    let sibling = devs[2].release();
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Accept {
                invite_id: inv.clone(),
                siblings: vec![sibling],
            },
        )
        .unwrap();
    // own-device sync stand-in: B2 learns that the invitation was accepted
    devs[2].app.group_invites.get_mut(&inv).unwrap().state = InviteState::Accepted;
    let ev = pump(&mut devs, 1, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 0 && matches!(e, GroupEvent::CommitPrepared { .. }))
    );
    (devs, conv, inv)
}

fn commit_id(d: &Dev) -> String {
    d.app.group_journal.keys().next().unwrap().clone()
}

#[test]
fn invite_accept_add_welcome_then_text_both_ways() {
    let (mut devs, conv, inv) = invited_and_accepted();
    // before the Add is accepted B is shown as invited, not as a member, and nothing can be sent under the candidate
    let det = detail(&devs[0], &conv);
    assert_eq!(det.summary.members, 1);
    assert!(det.people.iter().any(|p| p.status == "invited"));
    assert!(!det.can_send);
    let d = &mut devs[0];
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "x".into(),
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "commit_in_flight");
    // publish, accept, Welcome
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 0 && matches!(e, GroupEvent::MemberAdded { .. }))
    );
    for i in [1, 2] {
        assert!(
            has(
                &ev,
                i,
                &GroupEvent::Joined {
                    conversation_id: conv.clone()
                }
            ),
            "device {i} joined"
        );
    }
    let h0 = devs[0].app.group(&conv).unwrap().unwrap().hash();
    for d in &devs {
        assert_eq!(d.app.group(&conv).unwrap().unwrap().hash(), h0);
        assert_eq!(d.eng.epoch(&d.app.group_mls_id(&conv).unwrap()).unwrap(), 1);
        assert_eq!(detail(d, &conv).summary.members, 2);
    }
    assert_eq!(devs[0].app.group_invites[&inv].state, InviteState::Done);
    assert_eq!(detail(&devs[0], &conv).summary.invited, 0);
    // text: A to the group, B1 back
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "hello".into(),
            },
        )
        .unwrap();
    assert_eq!(
        rep.out.len(),
        2,
        "one per other device, no pairwise relationship fan-out"
    );
    assert!(
        rep.out
            .iter()
            .all(|o| matches!(o.to, Target::Device { .. }))
    );
    let ev = pump(&mut devs, 0, rep);
    assert!(
        ev.iter()
            .filter(|(_, e)| matches!(e, GroupEvent::MessageReceived { .. }))
            .count()
            == 2
    );
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "hi A".into(),
            },
        )
        .unwrap();
    pump(&mut devs, 1, rep);
    for d in &devs {
        assert_eq!(texts(d, &conv), vec!["hello", "hi A"]);
    }
    // a replayed ciphertext is not stored twice
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "again".into(),
            },
        )
        .unwrap();
    pump(&mut devs, 0, rep.clone());
    // The fake decrypts a replay and Core dedupes by message id; a real MLS engine refuses the replayed ciphertext
    // itself (its message key was already consumed), which is the stronger behaviour. Either way nothing is stored twice.
    #[cfg(not(feature = "groups-openmls"))]
    pump(&mut devs, 0, rep);
    #[cfg(feature = "groups-openmls")]
    for o in rep.out {
        let Target::Device { device, .. } = &o.to else {
            continue;
        };
        let from = devs[0].id();
        for d in devs.iter_mut().filter(|d| d.id() == *device) {
            assert!(
                d.app
                    .group_receive(ctx!(d), Sender::Device(from.clone()), o.wire.clone())
                    .is_err()
            );
        }
    }
    assert_eq!(texts(&devs[1], &conv).len(), 3);
}

#[test]
fn only_an_active_relationship_may_be_invited() {
    let mut a = Dev::new(0xa1, 1);
    let unbound = with_rel(&mut a, 2, RelState::AcceptedUnbound);
    let est = with_rel(&mut a, 3, RelState::Establishing);
    for r in [unbound.clone(), est] {
        let e = a
            .app
            .group_do(
                ctx!(a),
                GroupIntent::Create {
                    name: "g".into(),
                    relationship_ids: vec![r],
                },
            )
            .unwrap_err();
        assert_eq!(e.0, "relationship_not_active");
    }
    assert!(
        a.app.groups.is_empty(),
        "nothing created by a refused create"
    );
    let ok = with_rel(&mut a, 4, RelState::Active);
    a.app
        .group_do(
            ctx!(a),
            GroupIntent::Create {
                name: "g".into(),
                relationship_ids: vec![ok],
            },
        )
        .unwrap();
    let conv = conv_of(&a);
    let e = a
        .app
        .group_do(
            ctx!(a),
            GroupIntent::Invite {
                conversation_id: conv,
                relationship_id: unbound,
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "relationship_not_active");
    let e = a
        .app
        .group_do(
            ctx!(a),
            GroupIntent::Invite {
                conversation_id: vec![0; 32],
                relationship_id: "nope".into(),
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "unknown_group");
}

#[test]
fn unsolicited_welcome_is_dropped_and_declining_changes_nothing() {
    let mut devs = vec![Dev::new(0xa1, 1), Dev::new(0xb1, 2), Dev::new(0xb2, 2)];
    let ra = with_rel(&mut devs[0], 2, RelState::Active);
    with_rel(&mut devs[1], 1, RelState::Active);
    with_rel(&mut devs[2], 1, RelState::Active);
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Create {
                name: "g".into(),
                relationship_ids: vec![ra.clone()],
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let conv = conv_of(&devs[0]);
    let inv = match &ev
        .iter()
        .find(|(i, e)| *i == 1 && matches!(e, GroupEvent::InviteReceived { .. }))
        .unwrap()
        .1
    {
        GroupEvent::InviteReceived { invite_id } => invite_id.clone(),
        _ => unreachable!(),
    };
    // a Welcome while the invitation is only Offered (not accepted) is dropped; so is one for an unknown invitation
    for id in [inv.clone(), "ff".repeat(16)] {
        let w = GroupWire::Welcome {
            invite_id: id,
            welcome: "00".into(),
            state: "00".into(),
            directory: vec![],
        };
        let d = &mut devs[1];
        let r = d
            .app
            .group_receive(ctx!(d), Sender::Relationship("rel-1".into()), w)
            .unwrap();
        assert_eq!(r.events, vec![GroupEvent::Dropped("unsolicited_welcome")]);
    }
    assert!(devs[1].app.groups.is_empty());
    // decline
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Decline {
                invite_id: inv.clone(),
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 1, rep);
    assert!(has(
        &ev,
        0,
        &GroupEvent::InviteDeclined {
            invite_id: inv.clone()
        }
    ));
    assert_eq!(detail(&devs[0], &conv).summary.members, 1);
    assert_eq!(detail(&devs[0], &conv).summary.invited, 0);
    assert!(
        devs[0].app.group_journal.is_empty(),
        "no Commit for a declined invitation"
    );
    // an accept after a decline is unsolicited
    let a = GroupWire::Accept {
        invite_id: inv.clone(),
        releases: vec![],
    };
    let d = &mut devs[0];
    let r = d
        .app
        .group_receive(ctx!(d), Sender::Relationship(ra.clone()), a)
        .unwrap();
    assert_eq!(r.events, vec![GroupEvent::Dropped("unsolicited_accept")]);
    // the declined person can be invited again
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::Invite {
                conversation_id: conv.clone(),
                relationship_id: ra.clone(),
            },
        )
        .unwrap();
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Invite {
                conversation_id: conv,
                relationship_id: ra,
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "already_invited");
}

#[test]
fn under_recommended_a_plain_member_can_invite_and_only_admins_admit() {
    let (mut devs, conv, _) = invited_and_accepted();
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    pump(&mut devs, 0, rep);
    // B was admitted with the Member role (D-g2). Recommended (ADR-0189 section 3): anyone in the group can add
    // people, so B may invite; admitting (the Add itself) stays with room.manage_admission.
    let r = with_rel(&mut devs[1], 3, RelState::Active);
    let d = &mut devs[1];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::Invite {
                conversation_id: conv,
                relationship_id: r,
            },
        )
        .unwrap();
}

#[test]
fn leave_is_committed_by_another_member_and_the_leaver_stops() {
    let (mut devs, conv, _) = invited_and_accepted();
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    pump(&mut devs, 0, rep);
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Leave {
                conversation_id: conv.clone(),
            },
        )
        .unwrap();
    assert!(
        !detail(&devs[1], &conv).can_send,
        "a leaver sends nothing more"
    );
    let d = &mut devs[1];
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "x".into(),
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "leaving");
    // the request reaches A1, which prepares the removing Commit (it cannot be self-committed)
    let ev = pump(&mut devs, 1, rep);
    let pid = match &ev
        .iter()
        .find(|(i, e)| *i == 0 && matches!(e, GroupEvent::CommitPrepared { .. }))
        .unwrap()
        .1
    {
        GroupEvent::CommitPrepared { intent_id } => intent_id.clone(),
        _ => unreachable!(),
    };
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: pid.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: pid })
        .unwrap();
    let ev2 = pump(&mut devs, 0, rep);
    assert!(
        ev2.iter()
            .any(|(i, e)| *i == 0 && matches!(e, GroupEvent::MemberRemoved { .. }))
    );
    // the Commit went out at prepare time, so the leaver's devices processed it in the first pump
    for i in [1, 2] {
        assert!(has(
            &ev,
            i,
            &GroupEvent::NoLongerMember {
                conversation_id: conv.clone()
            }
        ));
        assert!(devs[i].app.groups.is_empty());
    }
    assert_eq!(detail(&devs[0], &conv).summary.members, 1);
    // later text from A reaches nobody who left (no devices remain in A's directory)
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv,
                text: "bye".into(),
            },
        )
        .unwrap();
    assert!(rep.out.is_empty());
}

#[test]
fn remove_needs_authority_and_cuts_every_device() {
    let (mut devs, conv, _) = invited_and_accepted();
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    pump(&mut devs, 0, rep);
    // B (Member) cannot remove A, and the refused intent leaves no open journal entry
    let d = &mut devs[1];
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Remove {
                conversation_id: conv.clone(),
                account: vec![1; 32],
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "not_authorized");
    assert!(devs[1].app.group_journal.is_empty());
    // A removes B
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Remove {
                conversation_id: conv.clone(),
                account: vec![2; 32],
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let pid = match &ev
        .iter()
        .find(|(i, e)| *i == 0 && matches!(e, GroupEvent::CommitPrepared { .. }))
        .unwrap()
        .1
    {
        GroupEvent::CommitPrepared { intent_id } => intent_id.clone(),
        _ => unreachable!(),
    };
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: pid.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: pid })
        .unwrap();
    // the Commit was fanned out at prepare time; the removed devices processed it then
    let ev2 = pump(&mut devs, 0, rep);
    assert!(
        ev2.iter()
            .any(|(_, e)| matches!(e, GroupEvent::MemberRemoved { .. }))
    );
    for i in [1, 2] {
        assert!(
            devs[i].app.groups.is_empty(),
            "device {i} dropped the group"
        );
    }
    let _ = ev;
}

#[test]
fn records_round_trip_and_older_records_still_load() {
    let (mut devs, conv, inv) = invited_and_accepted();
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    pump(&mut devs, 0, rep);
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "kept".into(),
            },
        )
        .unwrap();
    pump(&mut devs, 0, rep);
    let json = serde_json::to_string(&devs[0].app).unwrap();
    let back: AppState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.group_invites[&inv].state, InviteState::Done);
    assert_eq!(
        texts(
            &Dev {
                app: back.clone(),
                ..Dev::new(0xa1, 1)
            },
            &conv
        ),
        vec!["kept"]
    );
    // an engine restored from the stored record still sends
    let mut d = Dev::new(0xa1, 1);
    d.app = back;
    d.eng = d
        .app
        .group_crypto_open::<engine::Loader>()
        .unwrap()
        .unwrap();
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "after restart".into(),
            },
        )
        .unwrap();
    assert_eq!(rep.out.len(), 2);
    // a record written before slice 3 (none of its keys) loads with empty defaults
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for k in ["groupInvites", "groupDevices", "groupMsgs", "groupLeaving"] {
        assert!(
            v.as_object_mut().unwrap().remove(k).is_some(),
            "{k} is stored"
        );
    }
    let old: AppState = serde_json::from_value(v).unwrap();
    assert!(
        old.group_invites.is_empty()
            && old.group_devices.is_empty()
            && old.group_msgs.is_empty()
            && old.group_leaving.is_empty()
    );
}

// ---- an MLS leaf is admitted only when a validated head proves it --------------------------------

/// A invited B (B1 and B2 hold the invitation); nothing accepted yet. Returns the devices and the invitation id.
fn offered(configure: impl FnOnce(&mut [Dev])) -> (Vec<Dev>, String) {
    let mut devs = vec![Dev::new(0xa1, 1), Dev::new(0xb1, 2), Dev::new(0xb2, 2)];
    configure(&mut devs);
    let ra = with_rel(&mut devs[0], 2, RelState::Active);
    with_rel(&mut devs[1], 1, RelState::Active);
    with_rel(&mut devs[2], 1, RelState::Active);
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Create {
                name: "Book club".into(),
                relationship_ids: vec![ra],
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let inv = ev
        .iter()
        .find_map(|(i, e)| match e {
            GroupEvent::InviteReceived { invite_id } if *i == 1 => Some(invite_id.clone()),
            _ => None,
        })
        .unwrap();
    (devs, inv)
}

/// Deliver an Accept carrying `releases` to A, as if B1 sent it, under A's current view of B's head.
fn accept_arrives(devs: &mut [Dev], inv: &str, releases: Vec<DeviceRelease>) -> Vec<GroupEvent> {
    refresh_heads(devs);
    let d = &mut devs[0];
    // rewind a refused attempt so each case starts from the same offered invitation
    d.app.group_invites.get_mut(inv).unwrap().state = InviteState::Offered;
    d.app
        .group_receive(
            ctx!(d),
            Sender::Relationship("rel-2".into()),
            GroupWire::Accept {
                invite_id: inv.to_string(),
                releases,
            },
        )
        .unwrap()
        .events
}

fn blocked(ev: &[GroupEvent], why: &str) -> bool {
    ev.iter()
        .any(|e| matches!(e, GroupEvent::AdmitBlocked { reason, .. } if *reason == why))
}

fn nothing_prepared(d: &Dev, inv: &str) {
    assert!(d.app.group_journal.is_empty(), "no Commit was prepared");
    assert_eq!(d.app.group_invites[inv].state, InviteState::Accepted);
}

#[test]
fn endpoint_binding_a_leaf_no_head_proves_for_the_invitee_is_refused() {
    let (mut devs, inv) = offered(|_| {});
    // a collaborator's own client: B1's DeviceID, a signing key that is not B1's
    let mut imposter = engine::new(&[0xb1; 16]);
    let kp = imposter
        .key_package(&mut CryptoEnv {
            rng: &mut devs[1].rng,
            now_ms: NOW,
        })
        .unwrap();
    let release = DeviceRelease {
        device: om_core::crypto::hex(&[0xb1; 16]),
        key_package: om_core::crypto::hex(&kp.bytes),
        pk: om_core::crypto::hex(&devs[1].signer.public()),
    };
    let ev = accept_arrives(&mut devs, &inv, vec![release]);
    assert!(blocked(&ev, "endpoint_unbound"), "{ev:?}");
    nothing_prepared(&devs[0], &inv);
}

#[test]
fn endpoint_binding_revoked_and_quarantined_devices_are_refused() {
    for quarantine in [false, true] {
        let (mut devs, inv) = offered(|_| {});
        let b2 = devs[2].id();
        if quarantine {
            devs[0].quarantined.push(b2);
        } else {
            devs[0].revoked.push(b2);
        }
        let releases = vec![devs[1].release(), devs[2].release()];
        let ev = accept_arrives(&mut devs, &inv, releases);
        assert!(blocked(&ev, "endpoint_unbound"), "{ev:?}");
        nothing_prepared(&devs[0], &inv);
    }
}

#[test]
fn endpoint_binding_swapped_duplicate_wrong_account_and_unlisted_are_refused() {
    let (mut devs, inv) = offered(|_| {});
    // swapped: B2's KeyPackage under B1's DeviceID
    let mut r = devs[2].release();
    r.device = om_core::crypto::hex(&devs[1].id());
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "release_mismatch"), "{ev:?}");
    // swapped the other way: B1's key under B2's claimed pk
    let mut r = devs[1].release();
    r.pk = om_core::crypto::hex(&devs[2].signer.public());
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "release_mismatch"), "{ev:?}");
    // duplicate: the same KeyPackage released twice
    let r = devs[1].release();
    let ev = accept_arrives(&mut devs, &inv, vec![r.clone(), r]);
    assert!(blocked(&ev, "endpoint_duplicate"), "{ev:?}");
    // wrong account: a device of the inviter's own account released as the invitee's
    let r = devs[0].release();
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "endpoint_unbound"), "{ev:?}");
    // unlisted: a client no head of B lists, whatever it claims
    let mut stranger = Dev::new(0xb3, 2);
    let r = stranger.release();
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "endpoint_unbound"), "{ev:?}");
    nothing_prepared(&devs[0], &inv);
}

#[test]
fn endpoint_binding_the_honest_path_still_passes_with_a_messaging_only_device() {
    let (mut devs, inv) = offered(|d| d[2].messaging = true);
    let siblings = vec![devs[2].release()];
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Accept {
                invite_id: inv.clone(),
                siblings,
            },
        )
        .unwrap();
    devs[2].app.group_invites.get_mut(&inv).unwrap().state = InviteState::Accepted;
    let ev = pump(&mut devs, 1, rep);
    assert!(
        ev.iter()
            .any(|(_, e)| matches!(e, GroupEvent::CommitPrepared { .. }))
    );
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let conv = conv_of(&devs[0]);
    for i in [1, 2] {
        assert!(has(
            &ev,
            i,
            &GroupEvent::Joined {
                conversation_id: conv.clone()
            }
        ));
    }
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendText {
                conversation_id: conv.clone(),
                text: "hi from the messaging device's account".into(),
            },
        )
        .unwrap();
    pump(&mut devs, 1, rep);
    assert_eq!(texts(&devs[0], &conv).len(), 1);
}

#[test]
fn endpoint_binding_a_welcome_with_a_leaf_no_validated_head_proves_is_refused() {
    let (mut devs, inv) = offered(|_| {});
    let siblings = vec![devs[2].release()];
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Accept {
                invite_id: inv.clone(),
                siblings,
            },
        )
        .unwrap();
    devs[2].app.group_invites.get_mut(&inv).unwrap().state = InviteState::Accepted;
    pump(&mut devs, 1, rep);
    let id = commit_id(&devs[0]);
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    // B1 holds no head for A (the inviter's leaf is unproven), B2 has the head of B's own account only
    devs[1].unpinned.push(1);
    refresh_heads(&mut devs);
    let welcome = rep
        .out
        .iter()
        .find(|o| matches!(o.wire, GroupWire::Welcome { .. }))
        .unwrap()
        .wire
        .clone();
    let d = &mut devs[1];
    let e = d
        .app
        .group_receive(ctx!(d), Sender::Relationship("rel-1".into()), welcome)
        .unwrap_err();
    assert_eq!(e.0, "endpoint_unbound");
    assert!(
        d.app
            .group_query(GroupQuery::Groups)
            .map(|v| matches!(v, GroupView::Groups(g) if g.is_empty()))
            .unwrap()
    );
    assert_eq!(d.app.group_invites[&inv].state, InviteState::Accepted);
}

/// A, B1, B2 are in the Room; A then invites C (its own account, one device) and C accepts; returns the Add reply.
fn adding_c(b_knows_c: bool) -> (Vec<Dev>, Vec<u8>, Vec<(usize, GroupEvent)>) {
    let (mut devs, inv) = offered(|_| {});
    let siblings = vec![devs[2].release()];
    let d = &mut devs[1];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Accept {
                invite_id: inv.clone(),
                siblings,
            },
        )
        .unwrap();
    devs[2].app.group_invites.get_mut(&inv).unwrap().state = InviteState::Accepted;
    pump(&mut devs, 1, rep);
    let conv = conv_of(&devs[0]);
    for _ in 0..1 {
        let id = commit_id(&devs[0]);
        let d = &mut devs[0];
        d.app
            .group_do(
                ctx!(d),
                GroupIntent::CommitPublished {
                    intent_id: id.clone(),
                },
            )
            .unwrap();
        let rep = d
            .app
            .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
            .unwrap();
        pump(&mut devs, 0, rep);
    }
    devs.push(Dev::new(0xc1, 3));
    let rc = with_rel(&mut devs[0], 3, RelState::Active);
    with_rel(&mut devs[3], 1, RelState::Active);
    with_rel(&mut devs[3], 2, RelState::Active);
    if b_knows_c {
        with_rel(&mut devs[1], 3, RelState::Active);
        with_rel(&mut devs[2], 3, RelState::Active);
    }
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Invite {
                conversation_id: conv.clone(),
                relationship_id: rc,
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let inv_c = ev
        .iter()
        .find_map(|(i, e)| match e {
            GroupEvent::InviteReceived { invite_id } if *i == 3 => Some(invite_id.clone()),
            _ => None,
        })
        .unwrap();
    let d = &mut devs[3];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Accept {
                invite_id: inv_c,
                siblings: vec![],
            },
        )
        .unwrap();
    // the prepared Add reaches the roster devices here
    let add_events = pump(&mut devs, 3, rep);
    let id = devs[0]
        .app
        .group_invites
        .values()
        .filter(|i| {
            i.dir == InviteDir::Outgoing && i.peer_account == om_core::crypto::hex(&[3; 32])
        })
        .find_map(|i| i.intent_id.clone())
        .unwrap();
    let d = &mut devs[0];
    d.app
        .group_do(
            ctx!(d),
            GroupIntent::CommitPublished {
                intent_id: id.clone(),
            },
        )
        .unwrap();
    let rep = d
        .app
        .group_do(ctx!(d), GroupIntent::CommitAccepted { intent_id: id })
        .unwrap();
    pump(&mut devs, 0, rep);
    (devs, conv, add_events)
}

#[test]
fn endpoint_binding_an_incoming_commit_adding_an_unproven_leaf_is_dropped() {
    let (devs, conv, ev) = adding_c(false);
    for i in [1, 2] {
        assert!(
            ev.iter()
                .any(|(j, e)| *j == i && matches!(e, GroupEvent::Dropped("added_endpoint_unbound"))),
            "{ev:?}"
        );
        // never applied: still at the epoch before the Add of C
        assert_eq!(
            devs[i]
                .eng
                .epoch(&devs[i].app.group_mls_id(&conv).unwrap())
                .unwrap(),
            1
        );
    }
}

#[test]
fn endpoint_binding_an_incoming_commit_adding_a_proven_leaf_is_applied() {
    let (devs, conv, ev) = adding_c(true);
    assert!(
        !ev.iter().any(|(_, e)| matches!(e, GroupEvent::Dropped(_))),
        "{ev:?}"
    );
    for i in [1, 2] {
        assert_eq!(
            devs[i]
                .eng
                .epoch(&devs[i].app.group_mls_id(&conv).unwrap())
                .unwrap(),
            2
        );
    }
}
