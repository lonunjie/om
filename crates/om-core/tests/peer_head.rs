// SPDX-License-Identifier: MPL-2.0
//! Peer ACS-head refresh (ONE_TO_ONE_SESSION_MANAGEMENT.md K7, K9, K12; ADR-0144): a contact learns that the person
//! has a new device, never accepts an older head, holds what arrives from a device its head does not list yet instead
//! of rejecting it, and rejects a revoked device for good.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::crypto::binding::Acs;
use om_core::crypto::binding::PeerView;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::{delivery, hex, sha256, unhex};
use om_core::events;
use om_core::host::{NetCall, NetResult};
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

const KIND_ACS_UPDATE: u64 = 8;

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn mk(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    common::create_profile(rt, id, label)
}

fn active_pair() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = mk(&mut alice, "p-a", "Alice");
    let b = mk(&mut bob, "p-b", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let conv = |rt: &Runtime, c: &ProfileExecutionContext| {
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: c.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        l[0].id.clone()
    };
    let (a_conv, b_conv) = (conv(&alice, &a), conv(&bob, &b));
    Pair {
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

fn rel(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
    rt.core.debug_app_state(ctx).unwrap()["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

fn s(v: &serde_json::Value) -> String {
    v.as_str().unwrap().to_string()
}

fn timeline(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<String> {
    let v: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    });
    v.timeline
        .into_iter()
        .map(|t| match t {
            TimelineItem::Incoming { text, .. } | TimelineItem::Outgoing { text, .. } => text,
        })
        .collect()
}

fn peer_generation(r: &serde_json::Value) -> u64 {
    Acs::from_body(&unhex(&s(&r["peerAcsBody"])).unwrap())
        .unwrap()
        .generation
}

/// Bob's second device: a hand-built `Device` of Bob's account that the test controls.
struct Bob2 {
    dev: Device,
    rng: InsecureTestRng,
}

impl Bob2 {
    fn new(p: &Pair) -> Self {
        let mut rng = InsecureTestRng::from_label(b"bob-two");
        let mut dev = Device::new(&mut rng);
        dev.account_id = unhex(&s(&rel(&p.alice, &p.a)["peerAccountId"]))
            .unwrap()
            .try_into()
            .unwrap();
        Bob2 { dev, rng }
    }

    /// Bob's Core links this device by a real signed `link_messaging` transition.
    fn link(&self, p: &mut Pair) -> String {
        let (id, step) = p
            .bob
            .core
            .debug_link_credential(&p.b, &self.dev.credential().body())
            .unwrap();
        p.bob.settle(&step);
        id
    }

    /// Start a session to Alice's device from Alice's published key material and return a sealed wrapper
    /// carrying one message, addressed to Alice's route.
    fn message(&mut self, p: &Pair, first: bool, text: &str) -> Vec<u8> {
        let (br, ar) = (rel(&p.bob, &p.b), rel(&p.alice, &p.a));
        let alice_dev = unhex(&s(&br["peerDeviceId"])).unwrap();
        let alice_ik: [u8; 32] = unhex(&s(&br["peerIk"])).unwrap().try_into().unwrap();
        if first {
            let cap = vec![9u8; 32];
            p.net.perform(&NetCall::GrantKeyFetch {
                device_id: alice_dev.clone(),
                capability: cap.clone(),
            });
            let NetResult::Many { items } = p.net.perform(&NetCall::FetchKeyMaterial {
                device_id: alice_dev,
                service: None,
                capability: cap,
            }) else {
                panic!("alice's key material")
            };
            let acs = unhex(&s(&br["peerAcsBody"])).unwrap();
            let cred = unhex(&s(&br["peerCredentials"][0])).unwrap();
            let view = PeerView {
                trusted_acs_hash: om_core::crypto::sig::ds_hash(
                    om_core::crypto::binding::LBL_ACS_HASH,
                    &acs,
                ),
                acs_body: &acs,
                credential_body: &cred,
                now: 1_790_000_100,
            };
            self.dev
                .start_session(&items, &view, &mut self.rng)
                .expect("session to alice");
        }
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&sha256(&[text.as_bytes()])[..4]);
        let canon: [u8; 32] = unhex(&s(&ar["canonId"])).unwrap().try_into().unwrap();
        let payload = events::Header {
            event_id: id,
            conversation_id: canon,
            author_account: self.dev.account_id,
            author_device: self.dev.device_id,
            clock: events::Hlc {
                physical_ms: 1_790_000_000_500,
                logical: 0,
            },
            kind: events::K_CREATE,
            body: events::message_create_body(text),
        }
        .encode();
        let (env, _) = self.dev.encrypt(&alice_ik, &payload).expect("encrypt");
        seal_to_alice(p, &env)
    }
}

/// Seal `inner` to Alice's current delivery key and first route, as any sender would.
fn seal_to_alice(p: &Pair, inner: &[u8]) -> Vec<u8> {
    let ar = rel(&p.alice, &p.a);
    let key = &ar["myDelivery"][0]["public"];
    let route = &ar["myBoxes"][0]["route"];
    let mut rng = InsecureTestRng::from_label(b"sealer");
    delivery::seal(
        &unhex(&s(&key["publicKey"])).unwrap(),
        &unhex(&s(&key["keyId"])).unwrap().try_into().unwrap(),
        &s(&route["relayUri"]),
        &unhex(&s(&route["capability"])).unwrap(),
        key["expiresAt"].as_u64().unwrap().min(1_790_000_000 + 3600),
        inner,
        &mut rng,
    )
    .expect("seal")
}

/// Deposit `wrapper` at Alice's first route, as any sender would, and let Alice poll it.
fn deliver(p: &mut Pair, wrapper: &[u8]) {
    let ar = rel(&p.alice, &p.a);
    let route = &ar["myBoxes"][0]["route"];
    let r = p.net.perform(&NetCall::Deposit {
        mailbox: unhex(&s(&route["capability"])).unwrap(),
        relay_uri: Some(s(&route["relayUri"])),
        bytes: wrapper.to_vec(),
    });
    assert!(matches!(r, NetResult::Ok), "{r:?}");
    p.alice.poll(&p.a);
}

fn cursor(p: &Pair) -> u64 {
    let ar = rel(&p.alice, &p.a);
    let mailbox = unhex(&s(&ar["myBoxes"][0]["mailbox"])).unwrap();
    p.alice
        .core
        .poll_targets(&p.a)
        .unwrap()
        .into_iter()
        .find(|t| t.mailbox == mailbox)
        .unwrap()
        .after
}

fn held(p: &Pair) -> usize {
    rel(&p.alice, &p.a)["held"].as_array().unwrap().len()
}

fn failures(p: &Pair) -> u64 {
    rel(&p.alice, &p.a)["integrityFailures"].as_u64().unwrap()
}

#[test]
fn a_contact_learns_a_new_device_and_accepts_its_message() {
    let mut p = active_pair();
    let mut b2 = Bob2::new(&p);
    assert_eq!(peer_generation(&rel(&p.alice, &p.a)), 1);
    b2.link(&mut p);
    // Bob's Core announces its new head to Alice inside the relationship.
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let r = rel(&p.alice, &p.a);
    assert_eq!(peer_generation(&r), 2, "Alice pinned the new head");
    assert_eq!(r["peerCredentials"].as_array().unwrap().len(), 2);
    assert_eq!(failures(&p), 0);
    // The new device's first message is accepted, authored by that device.
    let w = b2.message(&p, true, "hello from the new phone");
    deliver(&mut p, &w);
    assert_eq!(
        timeline(&p.alice, &p.a, &p.a_conv),
        vec!["hello from the new phone".to_string()]
    );
    assert_eq!(failures(&p), 0);
    assert_eq!(held(&p), 0);
}

#[test]
fn a_message_that_beats_the_head_update_is_held_then_accepted_exactly_once() {
    let mut p = active_pair();
    let mut b2 = Bob2::new(&p);
    b2.link(&mut p);
    // Bob's update is still in his outbox: Alice sees the new device's message first.
    let w = b2.message(&p, true, "first");
    let before = cursor(&p);
    deliver(&mut p, &w);
    assert!(cursor(&p) > before, "the cursor advanced: nothing is stuck");
    assert_eq!(held(&p), 1, "indeterminate, so held");
    assert_eq!(failures(&p), 0, "not an integrity failure");
    assert!(timeline(&p.alice, &p.a, &p.a_conv).is_empty());
    assert!(
        rel(&p.alice, &p.a)["replay"]
            .as_object()
            .unwrap()
            .values()
            .all(|r| r["state"] == "complete"),
        "no replay verdict was written for the held wrapper"
    );
    // Alice asked for the head; Bob's update and his answer both arrive.
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(peer_generation(&rel(&p.alice, &p.a)), 2);
    assert_eq!(held(&p), 0);
    assert_eq!(
        timeline(&p.alice, &p.a, &p.a_conv),
        vec!["first".to_string()]
    );
    assert_eq!(failures(&p), 0);
    // A relay redelivery of the same wrapper is idempotent.
    deliver(&mut p, &w);
    assert_eq!(timeline(&p.alice, &p.a, &p.a_conv).len(), 1);
    assert_eq!(failures(&p), 0);
}

#[test]
fn the_held_list_is_bounded_and_overflow_is_a_counted_failure() {
    let mut p = active_pair();
    let mut b2 = Bob2::new(&p);
    b2.link(&mut p);
    for i in 0..9 {
        let w = b2.message(&p, i == 0, &format!("m{i}"));
        deliver(&mut p, &w);
    }
    assert_eq!(held(&p), 8);
    assert_eq!(failures(&p), 1, "the ninth is rejected and counted");
}

#[test]
fn a_revoked_device_is_rejected_for_good_once_the_revocation_is_known() {
    let mut p = active_pair();
    let mut b2 = Bob2::new(&p);
    let id = b2.link(&mut p);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let w = b2.message(&p, true, "while linked");
    deliver(&mut p, &w);
    assert_eq!(
        timeline(&p.alice, &p.a, &p.a_conv),
        vec!["while linked".to_string()],
        "the head Alice holds lists the device as active"
    );
    let confirmation = nonce(&mut p.bob, &p.b, "RevokeDevice", &id);
    p.bob.dispatch(
        &p.b,
        "rev",
        IntentBody::RevokeDevice {
            device_id: id,
            confirmation,
        },
    );
    assert_eq!(p.bob.outcome(&p.b, "rev"), Some(OutcomeClass::Committed));
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(peer_generation(&rel(&p.alice, &p.a)), 3);
    // K9: from here on the device is a definitive rejection (counted), never a hold and never a message.
    let w2 = b2.message(&p, false, "after the revocation");
    deliver(&mut p, &w2);
    assert_eq!(held(&p), 0);
    assert_eq!(
        timeline(&p.alice, &p.a, &p.a_conv),
        vec!["while linked".to_string()]
    );
    assert_eq!(failures(&p), 1);
}

#[test]
fn a_wrapper_held_for_a_head_that_never_lists_its_device_is_dropped_after_the_ttl() {
    let mut p = active_pair();
    let mut b2 = Bob2::new(&p);
    // Never linked: the device is simply unknown to every head Bob will ever publish.
    let w = b2.message(&p, true, "stranger");
    deliver(&mut p, &w);
    assert_eq!(held(&p), 1);
    p.alice.tick(8 * 24 * 3600 * 1000);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(held(&p), 0, "the hold expired");
    assert!(timeline(&p.alice, &p.a, &p.a_conv).is_empty());
}

fn update_payload(chain: Vec<Vec<u8>>, creds: Vec<Vec<u8>>) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(KIND_ACS_UPDATE)),
        (1, V::A(chain.into_iter().map(V::B).collect())),
        (2, V::A(creds.into_iter().map(V::B).collect())),
    ]))
}

fn chain_of(rt: &Runtime, ctx: &ProfileExecutionContext) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let st = rt.core.debug_app_state(ctx).unwrap();
    let list = |k: &str| -> Vec<Vec<u8>> {
        st[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| unhex(x.as_str().unwrap()).unwrap())
            .collect()
    };
    (list("acsChain"), list("acsCredentials"))
}

/// Bob's Core sends `payload` to Alice inside the relationship, as a real session message.
fn bob_sends(p: &mut Pair, payload: &[u8]) {
    let step = p
        .bob
        .core
        .debug_inject_payload(&p.b, &p.b_conv, payload)
        .unwrap();
    p.bob.settle(&step);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

#[test]
fn an_older_head_is_ignored_and_a_forged_or_foreign_chain_is_refused() {
    let mut p = active_pair();
    let b2 = Bob2::new(&p);
    let (chain1, creds1) = chain_of(&p.bob, &p.b);
    b2.link(&mut p);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let head2 = s(&rel(&p.alice, &p.a)["peerAcsBody"]);
    assert_eq!(peer_generation(&rel(&p.alice, &p.a)), 2);
    let (chain2, creds2) = chain_of(&p.bob, &p.b);

    // K7: the generation-1 chain again is an older head. Ignored, not an attack, nothing moves.
    bob_sends(&mut p, &update_payload(chain1.clone(), creds1));
    assert_eq!(s(&rel(&p.alice, &p.a)["peerAcsBody"]), head2);
    assert_eq!(failures(&p), 0);

    // The same head again: a no-op.
    bob_sends(&mut p, &update_payload(chain2.clone(), creds2.clone()));
    assert_eq!(s(&rel(&p.alice, &p.a)["peerAcsBody"]), head2);
    assert_eq!(failures(&p), 0);

    // A suffix that ends at the pinned head is that head.
    bob_sends(
        &mut p,
        &update_payload(chain2[1..].to_vec(), creds2.clone()),
    );
    assert_eq!(failures(&p), 0, "generation 2 is the pinned one: same head");

    // A chain of another account is never a refresh.
    let net = &p.net;
    let mut carol = Runtime::new("carol", net);
    let c = mk(&mut carol, "p-c", "Carol");
    let (cchain, ccreds) = chain_of(&carol, &c);
    bob_sends(&mut p, &update_payload(cchain, ccreds));
    assert_eq!(failures(&p), 1);
    assert_eq!(s(&rel(&p.alice, &p.a)["peerAcsBody"]), head2);
}

#[test]
fn a_state_that_forks_the_stored_head_at_the_same_generation_is_refused_by_the_device_store() {
    // Two different heads of one account at one height are a fork, never a refresh (K7 at the store layer).
    let mut rng = InsecureTestRng::from_label(b"fork");
    let mut me = Device::new(&mut rng);
    let peer = Device::new(&mut rng);
    me.trust_peer(peer.acs().body(), vec![peer.credential().body()])
        .unwrap();
    let mut forked = peer.acs();
    forked.generation = 1;
    forked.messaging.insert([7; 16]);
    assert_ne!(forked.hash(), peer.acs().hash());
    assert!(
        me.trust_peer(forked.body(), vec![peer.credential().body()])
            .is_err()
    );
    // And the same head again is fine.
    me.trust_peer(peer.acs().body(), vec![peer.credential().body()])
        .unwrap();
    let _ = hex(&[0]);
}
