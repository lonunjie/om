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
        super::attested(Eng::new(id), id, s)
    }
}
#[cfg(feature = "groups-openmls")]
mod engine {
    pub use om_core::groups::openmls::{OpenMlsGroupCrypto as Eng, OpenMlsLoader as Loader};
    pub fn new(id: &[u8]) -> Eng {
        Eng::new(id).expect("engine")
    }
    pub fn with_signer(id: &[u8], s: &om_core::crypto::sig::DeviceSigner) -> Eng {
        super::attested(new(id), id, s)
    }
}

const NOW: u64 = 1_800_000_000_000;

/// The engine with the attestation Core's own engine opening would give it: the device signs its dedicated leaf key.
fn attested(mut e: engine::Eng, id: &[u8], s: &DeviceSigner) -> engine::Eng {
    let did: [u8; 16] = id.try_into().expect("16-byte device id");
    let att = om_core::groups::endpoint::attest_leaf(s, &did, &e.signature_key(), [7; 32]);
    e.set_attestation(att).expect("attestation");
    e
}

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

// ---- the MLS leaf key is dedicated and attested by the device --------------------------------------

/// A client of B1's DeviceID whose leaf key is attested by `by` (a signer) over `key_of`'s leaf key.
fn attested_imposter(by: &DeviceSigner, key_of: Option<&engine::Eng>) -> engine::Eng {
    let mut e = engine::new(&[0xb1; 16]);
    let key = key_of.map_or_else(|| e.signature_key(), |k| k.signature_key());
    let att = om_core::groups::endpoint::attest_leaf(by, &[0xb1; 16], &key, [9; 32]);
    e.set_attestation(att).unwrap();
    e
}

fn released(devs: &mut [Dev], e: &mut engine::Eng) -> DeviceRelease {
    let kp = e
        .key_package(&mut CryptoEnv {
            rng: &mut devs[1].rng,
            now_ms: NOW,
        })
        .unwrap();
    DeviceRelease {
        device: om_core::crypto::hex(&[0xb1; 16]),
        key_package: om_core::crypto::hex(&kp.bytes),
        pk: om_core::crypto::hex(&devs[1].signer.public()),
    }
}

#[test]
fn leaf_attestation_wrong_or_absent_is_refused_and_the_honest_one_binds() {
    let (mut devs, inv) = offered(|_| {});
    // absent: B1's DeviceID with a leaf key nobody attested
    let mut bare = engine::new(&[0xb1; 16]);
    assert!(bare.attestation().is_empty());
    let r = released(&mut devs, &mut bare);
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "endpoint_unbound"), "absent: {ev:?}");
    // wrong signer: B2's device key attests B1's leaf
    let b2 = devs[2].signer.clone();
    let other = engine::new(&[0xb9; 16]);
    let mut e = attested_imposter(&b2, None);
    let r = released(&mut devs, &mut e);
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "endpoint_unbound"), "wrong signer: {ev:?}");
    // wrong key: B1's own attestation, but over some other leaf key
    let b1 = devs[1].signer.clone();
    let mut e = attested_imposter(&b1, Some(&other));
    let r = released(&mut devs, &mut e);
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(blocked(&ev, "endpoint_unbound"), "wrong key: {ev:?}");
    nothing_prepared(&devs[0], &inv);
    // the engine never holds the device key: its leaf key differs from the device's Ed25519 key
    assert_ne!(
        devs[1].eng.signature_key(),
        devs[1].signer.public()[..32].to_vec()
    );
    // the honest release binds
    let r = devs[1].release();
    let ev = accept_arrives(&mut devs, &inv, vec![r]);
    assert!(!blocked(&ev, "endpoint_unbound"), "honest: {ev:?}");
}

#[test]
fn a_commit_by_a_member_revoked_since_it_was_made_is_refused_at_apply_time() {
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
    // B1 leaves; A1 answers with the removing Commit, but B2 has learned A1 was revoked in the meantime
    let a1 = devs[0].id();
    devs[2].revoked.push(a1);
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
    let before = devs[2]
        .eng
        .epoch(&devs[2].app.group_mls_id(&conv).unwrap())
        .unwrap();
    let ev = pump(&mut devs, 1, rep);
    assert!(
        ev.iter()
            .any(|(i, e)| *i == 2 && matches!(e, GroupEvent::Dropped("committer_not_authorized"))),
        "{ev:?}"
    );
    assert_eq!(
        devs[2]
            .eng
            .epoch(&devs[2].app.group_mls_id(&conv).unwrap())
            .unwrap(),
        before,
        "refused: the epoch did not move"
    );
    assert!(!devs[2].app.groups.is_empty());
}

// ---- everyday chat in a Room: reply, reactions, edit and delete as typed events (D143) -------------------------------

use om_core::crypto::cbor::V;
use om_core::events;
use om_core::groups::GroupError;

/// A, B (B1, B2) and C in one Room, at the same epoch, A able to send.
fn room() -> (Vec<Dev>, Vec<u8>) {
    let (devs, conv, _) = adding_c(true);
    (devs, conv)
}

fn go(devs: &mut [Dev], i: usize, intent: GroupIntent) -> Vec<(usize, GroupEvent)> {
    let d = &mut devs[i];
    let rep = d.app.group_do(ctx!(d), intent).unwrap();
    pump(devs, i, rep)
}

fn msgs(d: &Dev, conv: &[u8]) -> Vec<GroupMsg> {
    match d
        .app
        .group_query(GroupQuery::Messages {
            conversation_id: conv.to_vec(),
        })
        .unwrap()
    {
        GroupView::Messages(m) => m,
        _ => unreachable!(),
    }
}

fn say(devs: &mut [Dev], i: usize, conv: &[u8], text: &str, reply_to: Option<&str>) -> String {
    let conversation_id = conv.to_vec();
    let text = text.to_string();
    let intent = match reply_to {
        None => GroupIntent::SendText {
            conversation_id,
            text,
        },
        Some(r) => GroupIntent::SendReply {
            conversation_id,
            text,
            reply_to: r.to_string(),
        },
    };
    go(devs, i, intent);
    msgs(&devs[i], conv).last().unwrap().id.clone()
}

fn react(devs: &mut [Dev], i: usize, conv: &[u8], id: &str, key: &str, present: bool) {
    go(
        devs,
        i,
        GroupIntent::SetReaction {
            conversation_id: conv.to_vec(),
            message_id: id.to_string(),
            key: key.to_string(),
            present,
        },
    );
}

fn counts(d: &Dev, conv: &[u8], id: &str) -> Vec<(String, usize, bool)> {
    msgs(d, conv)
        .iter()
        .find(|m| m.id == id)
        .map(GroupMsg::reaction_counts)
        .unwrap_or_default()
}

/// A typed event as device `from` of the Room would encrypt it (the test plays a peer that is not Core): `claimed` is the
/// account the event says it is from, `clock` its HLC. Returns the application wire.
fn craft(
    devs: &mut [Dev],
    from: usize,
    conv: &[u8],
    claimed: u8,
    clock: (u64, u32),
    kind: u64,
    body: V,
) -> GroupWire {
    let plain = events::Header {
        event_id: [0xee; 16],
        conversation_id: conv.try_into().unwrap(),
        author_account: [claimed; 32],
        author_device: devs[from].id().try_into().unwrap(),
        clock: events::Hlc {
            physical_ms: clock.0,
            logical: clock.1,
        },
        kind,
        body,
    }
    .encode();
    craft_plain(devs, from, conv, &plain)
}

fn craft_plain(devs: &mut [Dev], from: usize, conv: &[u8], plain: &[u8]) -> GroupWire {
    let d = &mut devs[from];
    let gid = d.app.group_mls_id(conv).unwrap();
    let ct = d
        .eng
        .encrypt_application(
            &mut CryptoEnv {
                rng: &mut d.rng,
                now_ms: NOW,
            },
            &gid,
            plain,
        )
        .unwrap();
    GroupWire::App {
        conversation_id: om_core::crypto::hex(conv),
        ciphertext: om_core::crypto::hex(&ct),
    }
}

/// Hand `wire` from device `from` to every other device; the result of each receive, by device index.
fn deliver(
    devs: &mut [Dev],
    from: usize,
    wire: &GroupWire,
) -> Vec<(usize, Result<GroupReply, GroupError>)> {
    refresh_heads(devs);
    let sender = devs[from].id();
    (0..devs.len())
        .filter(|i| *i != from)
        .map(|i| {
            let d = &mut devs[i];
            (
                i,
                d.app
                    .group_receive(ctx!(d), Sender::Device(sender.clone()), wire.clone()),
            )
        })
        .collect()
}

fn dropped(r: &Result<GroupReply, GroupError>, why: &str) -> bool {
    r.as_ref().is_ok_and(|r| {
        r.events
            .iter()
            .any(|e| matches!(e, GroupEvent::Dropped(w) if w == &why))
    })
}

fn target(id: &str) -> V {
    V::B(om_core::crypto::unhex(id).unwrap())
}

#[test]
fn reply_reaction_edit_and_delete_reach_every_member() {
    let (mut devs, conv) = room();
    let a = say(&mut devs, 0, &conv, "dinner at eight?", None);
    // B1 answers A's message: every device sees the link, and a reply to a message that is not here is refused before sending
    let d = &mut devs[1];
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SendReply {
                conversation_id: conv.clone(),
                text: "x".into(),
                reply_to: "00".repeat(16),
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "not_found");
    let r = say(&mut devs, 1, &conv, "yes please", Some(&a));
    for d in &devs {
        let m = msgs(d, &conv);
        assert_eq!(m.len(), 2, "no event shows as a message of its own");
        assert_eq!(m[1].reply_to.as_deref(), Some(a.as_str()));
        assert_eq!(m[0].reply_to, None);
    }
    // C and B1 react: a count per key, `mine` only where it is ours, and our other device (B2) sees B1's as ours
    react(&mut devs, 3, &conv, &a, "👍", true);
    react(&mut devs, 1, &conv, &a, "👍", true);
    react(&mut devs, 1, &conv, &a, "❤️", true);
    assert_eq!(
        counts(&devs[0], &conv, &a),
        vec![("👍".into(), 2, false), ("❤️".into(), 1, false)]
    );
    assert_eq!(
        counts(&devs[3], &conv, &a),
        vec![("👍".into(), 2, true), ("❤️".into(), 1, false)]
    );
    assert_eq!(
        counts(&devs[2], &conv, &a),
        vec![("👍".into(), 2, true), ("❤️".into(), 1, true)]
    );
    // a reaction is removed by its owner only: B1 removing leaves C's
    react(&mut devs, 1, &conv, &a, "👍", false);
    assert_eq!(
        counts(&devs[0], &conv, &a),
        vec![("👍".into(), 1, false), ("❤️".into(), 1, false)]
    );
    // A edits: every device shows the new text, marked edited; the reply link survives an edit of the reply
    go(
        &mut devs,
        0,
        GroupIntent::EditMessage {
            conversation_id: conv.clone(),
            message_id: a.clone(),
            text: "dinner at nine?".into(),
        },
    );
    go(
        &mut devs,
        1,
        GroupIntent::EditMessage {
            conversation_id: conv.clone(),
            message_id: r.clone(),
            text: "yes, please".into(),
        },
    );
    for d in &devs {
        let m = msgs(d, &conv);
        assert_eq!((m[0].text.as_str(), m[0].edited), ("dinner at nine?", true));
        assert_eq!(
            (m[1].text.as_str(), m[1].reply_to.as_deref()),
            ("yes, please", Some(a.as_str()))
        );
    }
    // A deletes for everyone: the message and its reactions are gone, the reply stays and now points at nothing
    go(
        &mut devs,
        0,
        GroupIntent::DeleteMessage {
            conversation_id: conv.clone(),
            message_id: a.clone(),
        },
    );
    for d in &devs {
        let m = msgs(d, &conv);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, r);
        assert!(!m.iter().any(|x| x.id == a));
        let stored = &d.app.group_msgs[&om_core::crypto::hex(&conv)];
        let gone = stored.iter().find(|x| x.id == a).unwrap();
        assert!(gone.deleted && gone.text.is_empty() && gone.reactions.is_empty());
    }
    // nothing can be done to a deleted message
    let d = &mut devs[1];
    let e = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::SetReaction {
                conversation_id: conv.clone(),
                message_id: a.clone(),
                key: "👍".into(),
                present: true,
            },
        )
        .unwrap_err();
    assert_eq!(e.0, "not_found");
}

#[test]
fn room_applies_an_early_delete_after_reopening_the_message_log() {
    let (mut devs, conv) = room();
    let target = [8; 16];
    let create = events::Header {
        event_id: target,
        conversation_id: conv.as_slice().try_into().unwrap(),
        author_account: devs[0].account.as_slice().try_into().unwrap(),
        author_device: devs[0].id().try_into().unwrap(),
        clock: events::Hlc {
            physical_ms: NOW,
            logical: 0,
        },
        kind: events::K_CREATE,
        body: events::message_create_body_with(Some("initial"), vec![]),
    }
    .encode();
    let create = craft_plain(&mut devs, 0, &conv, &create);
    let account = devs[0].account[0];
    let delete = craft(
        &mut devs,
        0,
        &conv,
        account,
        (NOW, 1),
        events::K_DELETE,
        V::M(vec![(0, V::B(target.to_vec()))]),
    );
    assert!(
        deliver(&mut devs, 0, &delete)
            .iter()
            .all(|(_, result)| result.is_ok())
    );
    for d in &mut devs[1..] {
        assert!(msgs(d, &conv).is_empty());
        d.app = serde_json::from_slice(&serde_json::to_vec(&d.app).unwrap()).unwrap();
    }
    assert!(
        deliver(&mut devs, 0, &create)
            .iter()
            .all(|(_, result)| result.is_ok())
    );
    for d in &devs[1..] {
        assert!(msgs(d, &conv).is_empty());
    }
}

#[test]
fn only_the_author_can_edit_or_delete() {
    let (mut devs, conv) = room();
    let a = say(&mut devs, 0, &conv, "mine", None);
    // our own intents refuse another member's message
    for intent in [
        GroupIntent::EditMessage {
            conversation_id: conv.clone(),
            message_id: a.clone(),
            text: "theirs".into(),
        },
        GroupIntent::DeleteMessage {
            conversation_id: conv.clone(),
            message_id: a.clone(),
        },
    ] {
        let d = &mut devs[1];
        assert_eq!(d.app.group_do(ctx!(d), intent).unwrap_err().0, "not_author");
    }
    // a member whose client sends one anyway (an authentic event of B) changes nothing anywhere
    let body = V::M(vec![(0, target(&a)), (1, events::rich_text("theirs"))]);
    let wire = craft(&mut devs, 1, &conv, 2, (NOW + 10, 0), events::K_EDIT, body);
    for (i, r) in deliver(&mut devs, 1, &wire) {
        assert!(dropped(&r, "not_author"), "device {i}: {r:?}");
    }
    let del = craft(
        &mut devs,
        1,
        &conv,
        2,
        (NOW + 11, 0),
        events::K_DELETE,
        V::M(vec![(0, target(&a))]),
    );
    for (i, r) in deliver(&mut devs, 1, &del) {
        assert!(dropped(&r, "not_author"), "device {i}: {r:?}");
    }
    // an event that names A as its author but arrives from B's device is refused: MLS says who sent it
    let spoof = craft(
        &mut devs,
        1,
        &conv,
        1,
        (NOW + 12, 0),
        events::K_DELETE,
        V::M(vec![(0, target(&a))]),
    );
    for (i, r) in deliver(&mut devs, 1, &spoof) {
        assert!(dropped(&r, "event_author_mismatch"), "device {i}: {r:?}");
    }
    for i in [0, 2, 3] {
        let m = msgs(&devs[i], &conv);
        assert_eq!(
            (m.len(), m[0].text.as_str(), m[0].edited),
            (1, "mine", false)
        );
    }
}

#[test]
fn registers_converge_under_reorder_and_repeat() {
    let (mut devs, conv) = room();
    let a = say(&mut devs, 0, &conv, "v0", None);
    let edit = |text: &str| V::M(vec![(0, target(&a)), (1, events::rich_text(text))]);
    // A's edits arrive newest first: the older one changes nothing
    let newer = craft(
        &mut devs,
        0,
        &conv,
        1,
        (NOW + 200, 0),
        events::K_EDIT,
        edit("v2"),
    );
    let older = craft(
        &mut devs,
        0,
        &conv,
        1,
        (NOW + 100, 0),
        events::K_EDIT,
        edit("v1"),
    );
    deliver(&mut devs, 0, &newer);
    deliver(&mut devs, 0, &older);
    assert_eq!(msgs(&devs[3], &conv)[0].text, "v2");
    // the same for a reaction: a late older add does not revive a later removal
    let off = craft(
        &mut devs,
        3,
        &conv,
        3,
        (NOW + 300, 0),
        events::K_REACTION,
        V::M(vec![
            (0, target(&a)),
            (1, V::T("👍".into())),
            (2, V::Bool(false)),
        ]),
    );
    let on = craft(
        &mut devs,
        3,
        &conv,
        3,
        (NOW + 250, 0),
        events::K_REACTION,
        V::M(vec![
            (0, target(&a)),
            (1, V::T("👍".into())),
            (2, V::Bool(true)),
        ]),
    );
    deliver(&mut devs, 3, &off);
    deliver(&mut devs, 3, &on);
    assert!(counts(&devs[0], &conv, &a).is_empty());
    // a repeated ciphertext is not applied twice (the fake decrypts it and Core is idempotent; real MLS refuses the replay)
    #[cfg(not(feature = "groups-openmls"))]
    {
        deliver(&mut devs, 3, &off);
        assert!(counts(&devs[0], &conv, &a).is_empty());
        assert_eq!(msgs(&devs[0], &conv).len(), 1);
    }
    #[cfg(feature = "groups-openmls")]
    for (_, r) in deliver(&mut devs, 3, &off) {
        assert!(r.is_err());
    }
    // a delete is final: a later edit does not bring the text back
    let del = craft(
        &mut devs,
        0,
        &conv,
        1,
        (NOW + 400, 0),
        events::K_DELETE,
        V::M(vec![(0, target(&a))]),
    );
    let late = craft(
        &mut devs,
        0,
        &conv,
        1,
        (NOW + 500, 0),
        events::K_EDIT,
        edit("back"),
    );
    deliver(&mut devs, 0, &del);
    deliver(&mut devs, 0, &late);
    assert!(msgs(&devs[2], &conv).is_empty());
    // ADR-0229 s5: a member shows at most 16 present reactions on one message, and our own client refuses a seventeenth
    let b = say(&mut devs, 0, &conv, "many", None);
    for n in 0..16u32 {
        react(&mut devs, 1, &conv, &b, &format!("k{n}"), true);
    }
    let d = &mut devs[1];
    assert!(
        d.app
            .group_do(
                ctx!(d),
                GroupIntent::SetReaction {
                    conversation_id: conv.clone(),
                    message_id: b.clone(),
                    key: "k16".into(),
                    present: true,
                },
            )
            .is_err(),
        "no seventeenth present reaction from our own client"
    );
    assert_eq!(counts(&devs[0], &conv, &b).len(), 16);
    // a peer that is not Core can still send more; every receiver holds at most 64 registers, shows the 16 newest present
    let c = say(&mut devs, 0, &conv, "flood", None);
    for n in 0..70u32 {
        let w = craft(
            &mut devs,
            1,
            &conv,
            2,
            (NOW + 1000 + u64::from(n), 0),
            events::K_REACTION,
            V::M(vec![
                (0, target(&c)),
                (1, V::T(format!("f{n}"))),
                (2, V::Bool(true)),
            ]),
        );
        deliver(&mut devs, 1, &w);
    }
    let shown = counts(&devs[0], &conv, &c);
    assert_eq!(shown.len(), 16);
    assert!(
        shown
            .iter()
            .all(|(k, ..)| { k[1..].parse::<u32>().is_ok_and(|n| n >= 54) })
    );
}

#[test]
fn a_removed_members_late_reaction_is_ignored() {
    let (mut devs, conv) = room();
    let a = say(&mut devs, 0, &conv, "hello", None);
    // C reacts, but the ciphertext is held back in the network
    let held = craft(
        &mut devs,
        3,
        &conv,
        3,
        (NOW + 10, 0),
        events::K_REACTION,
        V::M(vec![
            (0, target(&a)),
            (1, V::T("👍".into())),
            (2, V::Bool(true)),
        ]),
    );
    // A removes C and the Commit is accepted
    let d = &mut devs[0];
    let rep = d
        .app
        .group_do(
            ctx!(d),
            GroupIntent::Remove {
                conversation_id: conv.clone(),
                account: vec![3; 32],
            },
        )
        .unwrap();
    let ev = pump(&mut devs, 0, rep);
    let pid = ev
        .iter()
        .find_map(|(i, e)| match e {
            GroupEvent::CommitPrepared { intent_id } if *i == 0 => Some(intent_id.clone()),
            _ => None,
        })
        .unwrap();
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
    pump(&mut devs, 0, rep);
    assert!(devs[3].app.groups.is_empty(), "C is out");
    // the late reaction reaches the members who remain: it is refused or dropped, never applied
    for (i, r) in deliver(&mut devs, 3, &held)
        .into_iter()
        .filter(|(i, _)| *i != 3)
    {
        let ignored = match &r {
            Err(_) => true,
            Ok(rep) => rep
                .events
                .iter()
                .all(|e| matches!(e, GroupEvent::Dropped(_))),
        };
        assert!(ignored, "device {i}: {r:?}");
    }
    for (i, d) in devs.iter().enumerate().take(3) {
        assert!(counts(d, &conv, &a).is_empty(), "device {i}");
    }
    // and C can no longer send anything into the Room
    let d = &mut devs[3];
    assert!(
        d.app
            .group_do(
                ctx!(d),
                GroupIntent::SetReaction {
                    conversation_id: conv.clone(),
                    message_id: a,
                    key: "👍".into(),
                    present: true,
                },
            )
            .is_err()
    );
}

#[test]
fn unknown_kinds_and_malformed_payloads_are_ignored_and_older_text_still_renders() {
    let (mut devs, conv) = room();
    let a = say(&mut devs, 0, &conv, "hello", None);
    // a kind this build does not carry (receipt.update, kind 5): dropped, nothing changes, nothing fails
    let receipt = craft(
        &mut devs,
        1,
        &conv,
        2,
        (NOW + 10, 0),
        events::K_RECEIPT,
        V::M(vec![(0, V::U(1)), (1, V::A(vec![target(&a)]))]),
    );
    for (i, r) in deliver(&mut devs, 1, &receipt) {
        assert!(dropped(&r, "unsupported_kind"), "device {i}: {r:?}");
    }
    // a kind this build has never heard of, bytes that are not an event, and a reply to itself
    let unknown = craft(&mut devs, 1, &conv, 2, (NOW + 11, 0), 99, V::M(vec![]));
    let junk = craft_plain(&mut devs, 1, &conv, &[0xa0]);
    let text = |reply: &[u8]| {
        V::M(vec![
            (0, events::rich_text("x")),
            (1, V::A(vec![])),
            (2, V::B(reply.to_vec())),
        ])
    };
    let selfie = craft(
        &mut devs,
        1,
        &conv,
        2,
        (NOW + 12, 0),
        events::K_CREATE,
        text(&[0xee; 16]),
    );
    for w in [&unknown, &junk, &selfie] {
        for (i, r) in deliver(&mut devs, 1, w) {
            assert!(
                r.as_ref()
                    .is_ok_and(|r| r.events.iter().all(|e| matches!(e, GroupEvent::Dropped(_)))),
                "device {i}: {r:?}"
            );
        }
    }
    // a conversation id that is not this Room's is refused
    let other_plain = events::Header {
        event_id: [0xdd; 16],
        conversation_id: [9; 32],
        author_account: [2; 32],
        author_device: devs[1].id().try_into().unwrap(),
        clock: events::Hlc {
            physical_ms: NOW + 14,
            logical: 0,
        },
        kind: events::K_CREATE,
        body: events::message_create_body("elsewhere"),
    }
    .encode();
    let wrong_room = craft_plain(&mut devs, 1, &conv, &other_plain);
    for (i, r) in deliver(&mut devs, 1, &wrong_room) {
        assert!(dropped(&r, "event_author_mismatch"), "device {i}: {r:?}");
    }
    // the older JSON text payload from a member still renders
    let json =
        br#"{"k":"text","id":"00112233445566778899aabbccddeeff","text":"from an older build"}"#;
    let old = craft_plain(&mut devs, 1, &conv, json);
    for (i, r) in deliver(&mut devs, 1, &old) {
        assert!(r.is_ok(), "device {i}: {r:?}");
    }
    let m = msgs(&devs[3], &conv);
    assert_eq!(m.last().unwrap().text, "from an older build");
    // an older-build message can be replied to, reacted to and edited by its author (a legacy message sorts before any edit)
    let old_id = m.last().unwrap().id.clone();
    let r = say(&mut devs, 3, &conv, "ok", Some(&old_id));
    react(&mut devs, 0, &conv, &old_id, "👍", true);
    assert_eq!(
        counts(&devs[3], &conv, &old_id),
        vec![("👍".into(), 1, false)]
    );
    assert!(
        msgs(&devs[2], &conv)
            .iter()
            .any(|x| x.id == r && x.reply_to.as_deref() == Some(old_id.as_str()))
    );
    // a JSON payload that is not a text message is still an error (unchanged behaviour)
    let bad = craft_plain(&mut devs, 1, &conv, b"{");
    assert!(deliver(&mut devs, 1, &bad).iter().all(|(_, r)| r.is_err()));
    // every event the Core wrote is valid W0 ConversationEvent bytes: it passed the registry check on the way out and in
    assert!(msgs(&devs[0], &conv).iter().all(|m| !m.id.is_empty()));
}

#[test]
fn a_room_rejects_inline_type_four_from_an_authenticated_sender() {
    use std::io::Read as _;
    let (mut devs, conv) = room();
    let mut corpus = String::new();
    flate2::read::GzDecoder::new(
        &include_bytes!("../../../verification/vectors/expressive-wire-vectors.json.gz")[..],
    )
    .read_to_string(&mut corpus)
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&corpus).unwrap();
    let row = json["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "ConversationEvent/create-inline-custom-emoji")
        .unwrap();
    let bytes = om_core::crypto::unhex(row["hex"].as_str().unwrap()).unwrap();
    let body = om_core::crypto::cbor::decode(&bytes)
        .unwrap()
        .get(11)
        .unwrap()
        .clone();
    let plain = events::Header {
        event_id: [0xee; 16],
        conversation_id: conv.as_slice().try_into().unwrap(),
        author_account: [2; 32],
        author_device: devs[1].id().try_into().unwrap(),
        clock: events::Hlc {
            physical_ms: NOW + 10,
            logical: 0,
        },
        kind: events::K_CREATE,
        body,
    }
    .encode_full(
        None,
        Some(V::A(vec![V::T(om_core::expressive::INLINE.into())])),
    );
    assert!(om_core::expressive::decode_event(&plain, true).is_ok());
    let wire = craft_plain(&mut devs, 1, &conv, &plain);
    for (index, result) in deliver(&mut devs, 1, &wire) {
        assert!(
            dropped(&result, "unsupported_event"),
            "device {index}: {result:?}"
        );
        assert!(msgs(&devs[index], &conv).is_empty());
    }
}

// ---- mentions in a Room (ADR-0199) ------------------------------------------------------------------------------------

fn mention_intent(conv: &[u8], text: &str, mentions: Vec<(usize, usize, &str)>) -> GroupIntent {
    GroupIntent::SendMentions {
        conversation_id: conv.to_vec(),
        text: text.to_string(),
        reply_to: None,
        mentions: mentions
            .into_iter()
            .map(
                |(start, end, account)| om_core::groups::flow::GroupMention {
                    start,
                    end,
                    account: account.to_string(),
                },
            )
            .collect(),
    }
}

#[test]
fn a_mention_in_a_room_reaches_every_member_and_only_members_can_be_named() {
    let (mut devs, conv) = room();
    // A speaks first so B can learn A's account the way a person would: from the Room.
    let first = say(&mut devs, 0, &conv, "hello", None);
    let a_account = msgs(&devs[1], &conv)
        .iter()
        .find(|m| m.id == first)
        .unwrap()
        .sender_account
        .clone();
    go(
        &mut devs,
        1,
        mention_intent(&conv, "hi @A there", vec![(3, 5, &a_account)]),
    );
    for d in &devs {
        let m = msgs(d, &conv);
        let last = m.last().unwrap();
        assert_eq!(last.text, "hi @A there");
        assert_eq!(last.mentions.len(), 1, "every device sees the mention");
        assert_eq!(last.mentions[0].account, a_account);
        assert_eq!((last.mentions[0].start, last.mentions[0].end), (3, 5));
    }
    let before = msgs(&devs[1], &conv).len();
    // someone who is not in the Room, a range outside the text, an overlap: nothing is sent
    let stranger = "cd".repeat(32);
    for bad in [
        mention_intent(&conv, "hi @A", vec![(3, 5, &stranger)]),
        mention_intent(&conv, "hi @A", vec![(3, 50, &a_account)]),
        mention_intent(&conv, "hi @A", vec![(3, 5, &a_account), (4, 5, &a_account)]),
    ] {
        let d = &mut devs[1];
        assert!(d.app.group_do(ctx!(d), bad).is_err());
    }
    assert_eq!(msgs(&devs[1], &conv).len(), before);
}

// ---- polls in a Room (ADR-0223) ---------------------------------------------------------------------------------------

use om_core::groups::flow::{GroupPollRec, PollVoteRec};

fn poll_intent(conv: &[u8], multiple: bool, visibility: u8) -> GroupIntent {
    GroupIntent::CreatePoll {
        conversation_id: conv.to_vec(),
        spec: events::PollSpec {
            question: "Where do we eat?".into(),
            options: vec![
                (0, "Pizza".into()),
                (1, "Noodles".into()),
                (2, "Tacos".into()),
            ],
            multiple,
            max_selections: None,
            visibility,
        },
    }
}

fn poll_of(d: &Dev, conv: &[u8], id: &str) -> om_core::groups::flow::GroupPollView {
    let all = match d
        .app
        .group_query_as(
            &d.account,
            GroupQuery::Messages {
                conversation_id: conv.to_vec(),
            },
        )
        .unwrap()
    {
        GroupView::Messages(m) => m,
        _ => unreachable!(),
    };
    all.iter()
        .find(|m| m.id == id)
        .and_then(|m| m.poll_view.clone())
        .expect("a poll")
}

fn vote(
    devs: &mut [Dev],
    i: usize,
    conv: &[u8],
    id: &str,
    selected: Vec<u8>,
) -> Result<(), GroupError> {
    let d = &mut devs[i];
    let rep = d.app.group_do(
        ctx!(d),
        GroupIntent::VotePoll {
            conversation_id: conv.to_vec(),
            poll_id: id.to_string(),
            selected,
        },
    )?;
    pump(devs, i, rep);
    Ok(())
}

fn count_of(p: &om_core::groups::flow::GroupPollView, option: u8) -> Option<u32> {
    p.options.iter().find(|o| o.0 == option).and_then(|o| o.2)
}

#[test]
fn a_poll_in_a_room_is_created_voted_changed_retracted_and_closed_on_every_device() {
    let (mut devs, conv) = room();
    go(&mut devs, 1, poll_intent(&conv, false, 1));
    let id = msgs(&devs[1], &conv).last().unwrap().id.clone();
    for d in &devs {
        let m = msgs(d, &conv);
        let last = m.last().unwrap();
        assert_eq!(last.text, "Where do we eat?");
        assert_eq!(poll_of(d, &conv, &id).options.len(), 3);
    }
    vote(&mut devs, 0, &conv, &id, vec![0]).unwrap();
    vote(&mut devs, 3, &conv, &id, vec![1]).unwrap();
    for d in &devs {
        let p = poll_of(d, &conv, &id);
        assert_eq!(
            (count_of(&p, 0), count_of(&p, 1), count_of(&p, 2), p.voters),
            (Some(1), Some(1), Some(0), Some(2))
        );
    }
    assert_eq!(poll_of(&devs[0], &conv, &id).mine, vec![0]);
    // change, then take back
    vote(&mut devs, 0, &conv, &id, vec![2]).unwrap();
    assert_eq!(count_of(&poll_of(&devs[3], &conv, &id), 2), Some(1));
    vote(&mut devs, 0, &conv, &id, vec![]).unwrap();
    assert_eq!(poll_of(&devs[3], &conv, &id).voters, Some(1));
    // a single poll takes one option, an unknown option is refused, and nothing was sent for them
    assert!(vote(&mut devs, 0, &conv, &id, vec![0, 1]).is_err());
    assert!(vote(&mut devs, 0, &conv, &id, vec![9]).is_err());
    // only the author (or a moderator) closes: a plain member is refused, the author closes for everyone
    let c = &mut devs[3];
    assert!(
        c.app
            .group_do(
                ctx!(c),
                GroupIntent::ClosePoll {
                    conversation_id: conv.clone(),
                    poll_id: id.clone()
                }
            )
            .is_err()
    );
    go(
        &mut devs,
        1,
        GroupIntent::ClosePoll {
            conversation_id: conv.clone(),
            poll_id: id.clone(),
        },
    );
    for d in &devs {
        assert!(poll_of(d, &conv, &id).closed);
    }
    assert!(
        vote(&mut devs, 0, &conv, &id, vec![0]).is_err(),
        "no vote after the close"
    );
}

#[test]
fn the_poll_cap_counts_polls_kept_open_not_closed_ones() {
    // ADR-0223 section 6: at most 64 polls per Room are kept open for tallying. A closed poll is history, not an open poll.
    let (mut devs, conv) = room();
    for _ in 0..64 {
        go(&mut devs, 1, poll_intent(&conv, false, 1));
        let id = msgs(&devs[1], &conv).last().unwrap().id.clone();
        go(
            &mut devs,
            1,
            GroupIntent::ClosePoll {
                conversation_id: conv.clone(),
                poll_id: id,
            },
        );
    }
    // 64 closed polls, none open: the 65th is created and reaches every device, and the closed tallies are all still there
    go(&mut devs, 1, poll_intent(&conv, false, 1));
    for d in &devs {
        let polls: Vec<_> = msgs(d, &conv).into_iter().filter(|m| m.poll_view.is_some()).collect();
        assert_eq!(polls.len(), 65);
        assert_eq!(
            polls
                .iter()
                .filter(|m| m.poll_view.as_ref().is_some_and(|p| p.closed))
                .count(),
            64
        );
    }
    // 64 open (63 more) is the bound; the next one is refused, here and by the receivers' reducer
    for _ in 0..63 {
        go(&mut devs, 1, poll_intent(&conv, false, 1));
    }
    let d = &mut devs[1];
    let refused = d.app.group_do(ctx!(d), poll_intent(&conv, false, 1));
    assert_eq!(refused.unwrap_err().0, "too_many_polls");
}

#[test]
fn the_tally_follows_order_not_arrival_and_the_visibility_rule_holds_counts_back() {
    let me = "aa".repeat(32);
    let members: std::collections::BTreeSet<String> =
        ["aa", "bb", "cc"].iter().map(|x| x.repeat(32)).collect();
    let vote = |a: &str, sel: Vec<u8>, ms: u64| PollVoteRec {
        account: a.repeat(32),
        selected: sel,
        epoch: 1,
        clock_ms: ms,
        clock_logical: 0,
    };
    let mut p = GroupPollRec {
        options: vec![(0, "x".into()), (1, "y".into())],
        multiple: false,
        max_selections: None,
        visibility: 3,
        closed: None,
        // bb's vote is ordered after the close (ms 200) though it may have arrived first; cc's is before it
        votes: vec![vote("bb", vec![1], 300), vote("cc", vec![0], 100)],
    };
    // after_close: no counts while open, and not for a person who has not voted
    let open = p.view(&me, false, &members);
    assert!(open.options.iter().all(|o| o.2.is_none()) && open.voters.is_none());
    p.closed = Some((1, 200, 0));
    let closed = p.view(&me, true, &members);
    assert_eq!(
        (count_of(&closed, 0), count_of(&closed, 1), closed.voters),
        (Some(1), Some(0), Some(1))
    );
    assert!(!closed.can_close, "a closed poll offers no close");
    // after_vote: counts only once this person has a vote on record
    p.closed = None;
    p.visibility = 2;
    assert!(
        p.view(&me, false, &members)
            .options
            .iter()
            .all(|o| o.2.is_none())
    );
    p.votes.push(vote("aa", vec![0], 50));
    assert_eq!(count_of(&p.view(&me, false, &members), 0), Some(2));
    // a member who left is not counted; an earlier close wins over a later one is covered by the register itself
    let left: std::collections::BTreeSet<String> =
        ["aa", "cc"].iter().map(|x| x.repeat(32)).collect();
    assert_eq!(p.view(&me, false, &left).voters, Some(2));
}
