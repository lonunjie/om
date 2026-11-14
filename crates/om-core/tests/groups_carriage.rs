// SPDX-License-Identifier: MPL-2.0
//! Group carriage, Package 2a (ADR-0189 sections 1.1 to 1.3): the relationship lane (kinds 15 to 17), per-Room routes,
//! `GroupTransportFrame`s over sealed delivery and the semantic negatives, with two real Cores. The engine is the
//! test-only fake by default and stock OpenMLS with `--features groups-openmls`.
//!
//! The host's deposit and polling of a per-Room mailbox is Package 3: here a `Sealed.wrapper` is handed to the other
//! Core directly, exactly the bytes the host would deposit.

use flate2::read::GzDecoder;
use om_codec::cbor::unhex;
use om_core::api::*;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::delivery::KeyMaterial;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::{
    DeviceSigner, ROLE_CONTROLLER, ROLE_DEVICE, ds_sig, envelope, signature_entry,
};
use om_core::crypto::{PROFILE_V1, hex};
use om_core::events::{self, Hlc};
use om_core::groups::GroupError;
use om_core::groups::carriage::*;
use om_core::groups::flow::*;
use om_core::host::Step;
use om_core::route::RelayEntry;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};
use std::io::Read;

// ------------------------------------------------------------------------------------------------ pure pieces

fn vectors() -> Vec<serde_json::Value> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/group-transport-vectors.json.gz");
    let mut s = String::new();
    GzDecoder::new(&std::fs::read(p).unwrap()[..])
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&s).unwrap()["vectors"]
        .as_array()
        .unwrap()
        .clone()
}

fn vec_bytes(v: &serde_json::Value) -> Vec<u8> {
    use serde_json::Value as J;
    match &v["hex"] {
        J::String(s) => unhex(s),
        J::Array(parts) => parts
            .iter()
            .flat_map(|p| match p {
                J::String(s) => unhex(s),
                J::Object(o) => {
                    vec![unhex(o["fill"].as_str().unwrap())[0]; o["len"].as_u64().unwrap() as usize]
                }
                _ => panic!(),
            })
            .collect(),
        _ => panic!(),
    }
}

fn frame_vector(id: &str) -> Vec<u8> {
    vec_bytes(
        vectors()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap_or_else(|| panic!("{id}")),
    )
}

/// Rewrite a frame: `f` gets (kind, inner body bytes) and returns the new pair.
fn reframe(frame: &[u8], f: impl FnOnce(u64, Vec<u8>) -> (u64, Vec<u8>)) -> Vec<u8> {
    let v = cbor::decode(frame).unwrap();
    let (k, b) = f(
        v.get(2).unwrap().u().unwrap(),
        v.get(3).unwrap().b().unwrap().to_vec(),
    );
    cbor::encode(&V::M(vec![
        (0, V::U(33)),
        (1, V::U(1)),
        (2, V::U(k)),
        (3, V::B(b)),
    ]))
}

#[test]
fn every_wire_valid_frame_fixture_decodes_and_every_wire_negative_is_dropped() {
    let mut seen = 0;
    for v in vectors() {
        let id = v["id"].as_str().unwrap();
        if !id.starts_with("GroupTransportFrame/") {
            continue;
        }
        let bytes = vec_bytes(&v);
        let got = decode_frame(&bytes);
        if v["expect"] == "accept" {
            let body = got.unwrap_or_else(|e| panic!("{id}: {e}"));
            // a frame the codec accepted re-encodes to the same frame (the extension fixture carries a key-23 map
            // that Core does not keep)
            if !id.contains("extension") {
                assert_eq!(
                    encode_frame(&body).as_deref().ok(),
                    Some(&bytes[..]),
                    "{id}"
                );
            }
        } else {
            assert!(got.is_err(), "{id} must be dropped");
        }
        seen += 1;
    }
    assert!(seen >= 38, "{seen}");
}

#[test]
fn the_inner_body_is_decoded_strictly_and_must_match_the_frame_kind() {
    let welcome = frame_vector("GroupTransportFrame/welcome");
    // kind mismatch: a Welcome body in a commit frame (and in every other kind)
    for k in [
        FK_COMMIT,
        FK_LEAVE_REQUEST,
        FK_ADMIT_REQUEST,
        FK_ROUTE_UPDATE,
    ] {
        let f = reframe(&welcome, |_, b| (k, b));
        assert_eq!(
            decode_frame(&f).unwrap_err().0,
            "frame_body_invalid",
            "kind {k}"
        );
    }
    // an unknown field inside the body
    let f = reframe(&welcome, |k, b| {
        let mut m = match cbor::decode(&b).unwrap() {
            V::M(m) => m,
            _ => unreachable!(),
        };
        m.push((9, V::U(1)));
        (k, cbor::encode(&V::M(m)))
    });
    assert_eq!(decode_frame(&f).unwrap_err().0, "frame_body_invalid");
    // a body that is not canonical (trailing byte), and one that is not CBOR at all
    let f = reframe(&welcome, |k, mut b| {
        b.push(0);
        (k, b)
    });
    assert_eq!(decode_frame(&f).unwrap_err().0, "frame_body_invalid");
    let f = reframe(&welcome, |k, _| (k, vec![0xff, 0x01]));
    assert_eq!(decode_frame(&f).unwrap_err().0, "frame_body_invalid");
    // an empty MLS part is wire-valid and never accepted (Q4)
    let f = reframe(&frame_vector("GroupTransportFrame/commit"), |k, b| {
        let mut m = match cbor::decode(&b).unwrap() {
            V::M(m) => m,
            _ => unreachable!(),
        };
        for e in m.iter_mut() {
            if e.0 == 2 {
                e.1 = V::B(vec![]);
            }
        }
        (k, cbor::encode(&V::M(m)))
    });
    assert_eq!(decode_frame(&f).unwrap_err().0, "empty_mls_object");
    // an empty application ciphertext
    let f = reframe(&frame_vector("GroupTransportFrame/app"), |k, _| (k, vec![]));
    assert!(decode_frame(&f).is_err());
}

struct Dev {
    signer: DeviceSigner,
    id: [u8; 16],
    rng: InsecureTestRng,
}
fn dev(tag: u8) -> Dev {
    let mut rng = InsecureTestRng::from_label(&[b'c', tag]);
    Dev {
        signer: DeviceSigner::generate(&mut rng),
        id: [tag; 16],
        rng,
    }
}
const NOW: u64 = 1_800_000_000;

fn record(d: &mut Dev, expires: u64) -> RouteRecord {
    let entry = RelayEntry::new("https://home.example/relay", &[7u8; 32], expires).unwrap();
    let (_seed, km) = om_core::crypto::delivery::new_key(entry.binding(), expires, &mut d.rng);
    RouteRecord {
        device_id: d.id,
        delivery: km,
        relays: vec![entry],
    }
}

#[test]
fn a_route_record_verifies_only_with_role_device_for_its_own_device_and_while_fresh() {
    let mut a = dev(1);
    let r = record(&mut a, NOW + 1000);
    let signed = r.sign(&a.signer, [1; 32]).unwrap();
    let pk = a.signer.public();
    assert_eq!(RouteRecord::open(&signed, &a.id, &pk, NOW).unwrap(), r);
    // role controller (same key, same body): refused
    let body = r.body();
    let s = a.signer.sign_with(
        &ds_sig(LBL_ROUTE_SIG, PROFILE_V1, ROLE_CONTROLLER, &a.id, &body),
        [2; 32],
    );
    let bad = envelope(
        body.clone(),
        vec![signature_entry(&a.id, PROFILE_V1, ROLE_CONTROLLER, s)],
    );
    assert!(RouteRecord::open(&bad, &a.id, &pk, NOW).is_err());
    // another device's key
    let b = dev(2);
    assert_eq!(
        RouteRecord::open(&signed, &a.id, &b.signer.public(), NOW)
            .unwrap_err()
            .0,
        "route_bad_signature"
    );
    // a record that names another device than the one it was signed for and released as
    let other = RouteRecord {
        device_id: [9; 16],
        ..r.clone()
    };
    let body = other.body();
    let s = a.signer.sign_with(
        &ds_sig(LBL_ROUTE_SIG, PROFILE_V1, ROLE_DEVICE, &a.id, &body),
        [3; 32],
    );
    let forged = envelope(
        body,
        vec![signature_entry(&a.id, PROFILE_V1, ROLE_DEVICE, s)],
    );
    assert_eq!(
        RouteRecord::open(&forged, &a.id, &pk, NOW).unwrap_err().0,
        "route_for_another_device"
    );
    assert!(RouteRecord::open(&signed, &[9; 16], &pk, NOW).is_err());
    // stale
    assert_eq!(
        RouteRecord::open(&signed, &a.id, &pk, NOW + 1000 + 301)
            .unwrap_err()
            .0,
        "route_stale"
    );
    // a delivery key bound to another relay entry
    let mut wrong = r.clone();
    wrong.relays[0].capability = vec![8; 32];
    let signed = wrong.sign(&a.signer, [4; 32]).unwrap();
    assert_eq!(
        RouteRecord::open(&signed, &a.id, &pk, NOW).unwrap_err().0,
        "route_binding_mismatch"
    );
}

#[test]
fn a_frame_seals_to_a_route_opens_only_there_and_a_stale_route_is_never_sealed_to() {
    let mut a = dev(1);
    let mut s = dev(3);
    let entry = RelayEntry::new("https://home.example/relay", &[7u8; 32], NOW + 5000).unwrap();
    let (seed, km) = om_core::crypto::delivery::new_key(entry.binding(), NOW + 5000, &mut a.rng);
    let rec = RouteRecord {
        device_id: a.id,
        delivery: km.clone(),
        relays: vec![entry.clone()],
    };
    let frame = encode_frame(&FrameBody::App(vec![1, 2, 3])).unwrap();
    let (wrapper, used) = seal_frame(&frame, &rec, NOW, &mut s.rng).unwrap();
    assert_eq!(used, entry);
    assert_eq!(wrapper_key_id(&wrapper), Some(km.key_id));
    let opened = om_core::crypto::delivery::open(
        &wrapper,
        &seed,
        &km.key_id,
        &entry.relay_uri,
        &entry.capability,
        NOW,
    )
    .unwrap();
    assert_eq!(
        decode_frame(&opened.inner_envelope).unwrap(),
        FrameBody::App(vec![1, 2, 3])
    );
    // another route's key cannot open it
    let (seed2, km2) = om_core::crypto::delivery::new_key(entry.binding(), NOW + 5000, &mut a.rng);
    assert!(
        om_core::crypto::delivery::open(
            &wrapper,
            &seed2,
            &km2.key_id,
            &entry.relay_uri,
            &entry.capability,
            NOW
        )
        .is_err()
    );
    // a stale route is refused at seal time: the sender queues and refreshes, it never falls back
    assert_eq!(
        seal_frame(&frame, &rec, NOW + 6000, &mut s.rng)
            .unwrap_err()
            .0,
        "route_stale"
    );
    // over the bucket: refused with room_too_large before sealing
    assert_eq!(
        encode_frame(&FrameBody::App(vec![1; 131_001]))
            .unwrap_err()
            .0,
        "room_too_large"
    );
}

#[test]
fn lane_events_need_the_group_capability_and_an_accept_is_bounded() {
    let id = [5u8; 16];
    let ev = lane_event(
        [1; 16],
        [2; 32],
        [3; 32],
        [4; 16],
        Hlc {
            physical_ms: 1,
            logical: 0,
        },
        &LaneBody::Invite {
            invite_id: id,
            room_name: "Book club".into(),
            member_count: Some(2),
        },
    )
    .unwrap();
    assert!(parse_lane_event(&ev).is_ok());
    // the same event without key 10 is rejected (Q3) and is invisible to a W0 decoder
    let h = events::Header {
        event_id: [1; 16],
        conversation_id: [2; 32],
        author_account: [3; 32],
        author_device: [4; 16],
        clock: Hlc {
            physical_ms: 1,
            logical: 0,
        },
        kind: KIND_INVITE,
        body: V::M(vec![(0, V::B(id.to_vec())), (1, V::T("x".into()))]),
    };
    assert_eq!(
        parse_lane_event(&h.encode()).unwrap_err().0,
        "capability_missing"
    );
    assert!(
        om_codec::Registry::load()
            .check("ConversationEvent", &ev, false)
            .is_err()
    );
    // another capability set
    let other = h.encode_full(None, Some(V::A(vec![V::T("om.other.v1".into())])));
    assert!(parse_lane_event(&other).is_err());
    // the Accept limit: 16 releases of 6 KiB exceed 65,536 bytes
    let rel = |n: u8| Release {
        device: [n; 16],
        key_package: vec![1; 5000],
        credential: vec![0; 10],
        route: vec![0; 10],
    };
    let big = LaneBody::Accept {
        invite_id: id,
        releases: (0..16).map(rel).collect(),
        head: vec![0; 10],
    };
    assert!(
        lane_event(
            [1; 16],
            [2; 32],
            [3; 32],
            [4; 16],
            Hlc {
                physical_ms: 1,
                logical: 0
            },
            &big
        )
        .is_err()
    );
}

#[test]
fn route_replacement_retires_older_routes_and_returns_the_capabilities_to_revoke() {
    let mut rec = GroupCarriageRec::default();
    let mk = |k: u8, at: u64| MyRoute {
        key_id: hex(&[k; 16]),
        seed: hex(&[k; 32]).into(),
        mailbox: hex(&[k; 16]),
        entries: vec![RelayEntry::new("https://h.example", &[k; 32], at + 9999).unwrap()],
        record: String::new(),
        expires_at: at + 9999,
        minted_at: at,
        retired_at: None,
    };
    rec.add_mine("room", mk(1, 10));
    rec.replace_owed.push("room".into());
    rec.add_mine("room", mk(2, 20));
    rec.retire_older("room", 20);
    assert!(rec.replace_owed.is_empty());
    assert_eq!(rec.mine["room"][0].retired_at, Some(20));
    assert_eq!(rec.mine["room"][1].retired_at, None);
    // inside the overlap the old route still receives; afterwards it is dropped and its capability is returned
    assert!(rec.expire_retired(20 + ROUTE_OVERLAP_S - 1).is_empty());
    let revoke = rec.expire_retired(20 + ROUTE_OVERLAP_S);
    assert_eq!(revoke.len(), 1);
    assert_eq!(revoke[0].capability, vec![1; 32]);
    assert_eq!(rec.mine["room"].len(), 1);
}

// ------------------------------------------------------------------------------------------------ two Cores

struct Party {
    rt: Runtime,
    ctx: ProfileExecutionContext,
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

fn connected() -> [Party; 2] {
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
    bob.dispatch(
        &b,
        "acc-1",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
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
    [
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
    ]
}

impl Party {
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
    /// What the host driver does after every durable step: let Core accept its own Commits (G10, ADR-0189 section 2).
    /// There is no report of acceptance and no relay acknowledgement in this loop.
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
    fn state(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()
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
    /// Send the relationship-lane outbounds of `rep` (Invite, Accept, Decline) as kind 15 to 17 events.
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
        r
    }
}

fn deliver(p: &mut [Party; 2]) {
    let [a, b] = p;
    settle_all(&mut [(&mut a.rt, &a.ctx), (&mut b.rt, &b.ctx)]);
}

fn events(reps: &[GroupReply]) -> Vec<GroupEvent> {
    reps.iter().flat_map(|r| r.events.clone()).collect()
}

/// Alice creates and invites Bob; the Invite travels as a kind 15 event.
fn invited() -> ([Party; 2], String) {
    let mut p = connected();
    let rel = p[0].rel.clone();
    let rep = p[0]
        .dob(GroupIntent::Create {
            name: "Book club".into(),
            relationship_ids: vec![rel],
        })
        .unwrap();
    p[0].send_lane(&rep);
    deliver(&mut p);
    let reps = p[1].pump_lane();
    let id = events(&reps)
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::InviteReceived { invite_id } => Some(invite_id),
            _ => None,
        })
        .expect("Bob got the invitation over the 1:1");
    (p, id)
}

/// Bob accepts: the Accept (kind 16) carries his release, credential, per-Room route and his head; Alice prepares the Add.
fn accepted() -> ([Party; 2], String, String) {
    let (mut p, id) = invited();
    let rep = p[1]
        .dob(GroupIntent::Accept {
            invite_id: id.clone(),
            siblings: vec![],
        })
        .unwrap();
    p[1].send_lane(&rep);
    deliver(&mut p);
    let reps = p[0].pump_lane();
    let intent = events(&reps)
        .into_iter()
        .find_map(|e| match e {
            GroupEvent::CommitPrepared { intent_id } => Some(intent_id),
            _ => None,
        })
        .expect("Alice prepared the Add from the Accept");
    (p, id, intent)
}

/// Alice's Core accepted its own Add by itself (G10): its Welcome is in the Core outbox, after the Commit.
fn welcomed() -> ([Party; 2], String, Vec<Sealed>) {
    let (mut p, id, _intent) = accepted();
    let w = p[0]
        .rt
        .core
        .group_outbox(&p[0].ctx)
        .unwrap()
        .into_iter()
        .find(|o| matches!(o.wire, GroupWire::Welcome { .. }))
        .expect("a Welcome")
        .to_outbound()
        .unwrap();
    assert!(matches!(w.to, Target::Relationship(_)));
    let sealed = p[0].seal(&w).unwrap();
    (p, id, sealed)
}

fn conv_of(p: &Party) -> Vec<u8> {
    match p.view(GroupQuery::Groups) {
        GroupView::Groups(g) => unhex(&g[0].conversation_id),
        _ => unreachable!(),
    }
}

/// The parts of a profile's state that the group layer owns (what a drop must leave untouched).
fn group_part(p: &Party) -> serde_json::Value {
    let st = p.state();
    serde_json::json!([
        st["groups"],
        st["groupInvites"],
        st["groupDevices"],
        st["groupCarriage"],
        st["groupJournal"],
        st["groupMsgs"],
        st["groupMlsIds"],
        st["groupCrypto"]
    ])
}

#[test]
fn easy_path_invite_accept_welcome_over_the_lane_and_a_per_room_route() {
    let ([mut a, mut b], _id, sealed) = welcomed();
    // the invitation never showed up as a chat message, on either side
    for x in [&a, &b] {
        let tl: ConversationView = x.rt.query(Query::GetConversationView {
            ctx: x.ctx.clone(),
            conversation_id: x.rel.clone(),
        });
        assert!(tl.timeline.is_empty(), "{:?}", tl.timeline);
    }
    // exactly one frame, to Bob's one device, and it is an opaque sealed envelope (type 23)
    assert_eq!(sealed.len(), 1);
    assert_eq!(
        cbor::decode(&sealed[0].wrapper)
            .unwrap()
            .get(0)
            .unwrap()
            .u()
            .unwrap(),
        23
    );
    assert_eq!(b.groups(), 0);
    let rep = b.open(&sealed[0].wrapper).unwrap();
    assert!(
        rep.events
            .iter()
            .any(|e| matches!(e, GroupEvent::Joined { .. })),
        "{:?}",
        rep.events
    );
    assert_eq!(b.groups(), 1);
    // Bob's route for the invite is now the Room's, and he holds Alice's
    let st = b.state();
    let c = &st["groupCarriage"];
    let mine = c["mine"].as_object().unwrap();
    assert_eq!(mine.len(), 1);
    assert!(!mine.keys().next().unwrap().starts_with("invite:"));
    assert_eq!(
        c["peers"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
    // an application message travels as a frame to Bob's per-Room route and decrypts
    let conv = conv_of(&a);
    let rep = a
        .dob(GroupIntent::SendText {
            conversation_id: conv.clone(),
            text: "hello".into(),
        })
        .unwrap();
    let app = rep
        .out
        .iter()
        .find(|o| matches!(o.wire, GroupWire::App { .. }))
        .unwrap()
        .clone();
    let sealed = a.seal(&app).unwrap();
    b.open(&sealed[0].wrapper).unwrap();
    match b.view(GroupQuery::Messages {
        conversation_id: conv,
    }) {
        GroupView::Messages(m) => assert_eq!(
            m.iter().map(|x| x.text.clone()).collect::<Vec<_>>(),
            vec!["hello"]
        ),
        _ => unreachable!(),
    }
}

#[test]
fn a_decline_reaches_the_inviter_and_mints_no_route() {
    let (mut p, id) = invited();
    let rep = p[1].dob(GroupIntent::Decline { invite_id: id }).unwrap();
    p[1].send_lane(&rep);
    deliver(&mut p);
    let reps = p[0].pump_lane();
    assert!(
        events(&reps)
            .iter()
            .any(|e| matches!(e, GroupEvent::InviteDeclined { .. }))
    );
    assert!(
        p[1].state()["groupCarriage"]["mine"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

fn route_from_state(st: &serde_json::Value, room: &str) -> RouteRecord {
    let rec = unhex(
        st["groupCarriage"]["mine"][room][0]["record"]
            .as_str()
            .unwrap(),
    );
    let v = cbor::decode(&envelope_body(&rec).unwrap()).unwrap();
    RouteRecord {
        device_id: v.get(2).unwrap().b().unwrap().try_into().unwrap(),
        delivery: KeyMaterial::from_v(v.get(3).unwrap()).unwrap(),
        relays: v
            .get(4)
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .map(|e| RelayEntry::from_v(e).unwrap())
            .collect(),
    }
}

/// Seal `frame` to the first per-Room route a profile holds, as a stranger who learned the route would.
fn seal_to_room(p: &Party, frame: &[u8], room: Option<&str>) -> Vec<u8> {
    let st = p.state();
    let room = room.map(str::to_string).unwrap_or_else(|| {
        st["groupCarriage"]["mine"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone()
    });
    let rec = route_from_state(&st, &room);
    let now = rec.delivery.expires_at - ROUTE_TTL_S;
    seal_frame(
        frame,
        &rec,
        now,
        &mut InsecureTestRng::from_label(b"stranger"),
    )
    .unwrap()
    .0
}

#[test]
fn a_welcome_without_an_accepted_invite_is_dropped_with_no_state_change() {
    // replay after joining: the route is now the Room's and the invite is Done
    let ([_a, mut b], _id, sealed) = welcomed();
    b.open(&sealed[0].wrapper).unwrap();
    let before = group_part(&b);
    assert!(b.open(&sealed[0].wrapper).is_err());
    assert_eq!(group_part(&b), before);

    // a (wire-valid) Welcome for another invite id arrives on the route minted for this invite
    let (mut p, _id, _intent) = accepted();
    let foreign = frame_vector("GroupTransportFrame/semantic-welcome-without-accepted-invite");
    let w = seal_to_room(&p[1], &foreign, None);
    let before = group_part(&p[1]);
    assert_eq!(p[1].open(&w).unwrap_err().0, "route_for_another_room");
    assert_eq!(group_part(&p[1]), before);
    assert_eq!(p[1].groups(), 0);
}

#[test]
fn an_accept_for_an_unknown_invite_id_is_dropped_with_no_state_change() {
    let (mut p, id) = invited();
    // Bob's Core builds a real Accept (his own release, route and head) but for an invite id Alice never issued
    let rep = p[1]
        .dob(GroupIntent::Accept {
            invite_id: id,
            siblings: vec![],
        })
        .unwrap();
    let releases = rep
        .out
        .iter()
        .find_map(|o| match &o.wire {
            GroupWire::Accept { releases, .. } => Some(releases.clone()),
            _ => None,
        })
        .unwrap();
    let wire = GroupWire::Accept {
        invite_id: hex(&[0x66; 16]),
        releases,
    };
    p[1].rt.supply_routes(4);
    let rel = p[1].rel.clone();
    let (r, step) = p[1]
        .rt
        .core
        .group_lane_send(&p[1].ctx, &rel, &wire)
        .unwrap();
    r.unwrap();
    assert!(p[1].rt.settle(&step));
    let before = group_part(&p[0]);
    deliver(&mut p);
    assert!(
        p[0].pump_lane().is_empty(),
        "nothing was queued for the flow"
    );
    assert_eq!(group_part(&p[0]), before);
}

#[test]
fn a_lane_event_without_the_capability_is_rejected_by_the_receiver() {
    let (mut p, _id) = invited();
    let st = p[0].state();
    let rel = &st["relationships"][&p[0].rel];
    let canon: [u8; 32] = unhex(rel["canonId"].as_str().unwrap()).try_into().unwrap();
    // Alice (an authenticated device of the conversation) sends Bob an invite with no `required_capabilities`
    let acct: [u8; 32] = unhex(
        st["relationships"][&p[0].rel]["peerAccountId"]
            .as_str()
            .unwrap(),
    )
    .try_into()
    .unwrap();
    let _ = acct;
    let bst = p[1].state();
    let my_acct: [u8; 32] = unhex(
        bst["relationships"][&p[1].rel]["peerAccountId"]
            .as_str()
            .unwrap(),
    )
    .try_into()
    .unwrap();
    let my_dev: [u8; 16] = unhex(
        bst["relationships"][&p[1].rel]["peerDeviceId"]
            .as_str()
            .unwrap(),
    )
    .try_into()
    .unwrap();
    let ev = events::Header {
        event_id: [0x21; 16],
        conversation_id: canon,
        author_account: my_acct,
        author_device: my_dev,
        clock: Hlc {
            physical_ms: 1,
            logical: 0,
        },
        kind: KIND_INVITE,
        body: V::M(vec![(0, V::B(vec![0x44; 16])), (1, V::T("No caps".into()))]),
    }
    .encode();
    #[cfg(feature = "test-loopback")]
    {
        let before = group_part(&p[1]);
        let step = p[0]
            .rt
            .core
            .debug_inject_payload(&p[0].ctx, &p[0].rel, &ev)
            .unwrap();
        assert!(p[0].rt.settle(&step));
        deliver(&mut p);
        assert!(p[1].pump_lane().is_empty());
        assert_eq!(group_part(&p[1]), before);
    }
    let _ = ev;
}

#[test]
fn a_commit_frame_from_a_non_roster_device_is_dropped_before_any_mls_state() {
    let ([mut a, mut b], _id, sealed) = welcomed();
    b.open(&sealed[0].wrapper).unwrap();
    // a wire-valid Commit frame whose binding and intent are signed by a device that is in nobody's roster
    let fx = frame_vector("GroupTransportFrame/semantic-commit-frame-non-roster-sender");
    let conv = hex(&conv_of(&a));
    let w = seal_to_room(&a, &fx, Some(&conv));
    let before = group_part(&a);
    let e = a.open(&w).unwrap_err();
    assert!(
        ["non_roster_sender", "added_evidence_unverifiable"].contains(&e.0),
        "{e:?}"
    );
    assert_eq!(group_part(&a), before);
    let _ = &mut b;
}
