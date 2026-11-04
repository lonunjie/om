// SPDX-License-Identifier: MPL-2.0
//! Per-device sessions (ONE_TO_ONE_SESSION_MANAGEMENT.md K4-K6, MULTI_DEVICE_SYNC.md self-sync, V-003): a message goes
//! to every eligible device of the recipient and to our own other devices, a device that cannot be sealed to yet gets
//! its copy when it can, a revoked device gets nothing, a crossed initiation leaves exactly one session, and a logical
//! message is applied once however many copies arrive.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::crypto::binding::{Acs, RelationshipContext};
use om_core::crypto::cbor::{self, V};
use om_core::crypto::delivery::{self, KeyMaterial};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::{Error, sha256, unhex};
use om_core::events;
use om_core::host::{NetCall, NetResult};
use om_core::relayset::{self, Record};
use om_core::route::RelayEntry;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

const KIND_RELAYSET: u64 = 6;
const KIND_ACS_UPDATE: u64 = 8;
const NOW_S: u64 = 1_790_000_000;

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn active_pair() -> Pair {
    active_pair_seeded("alice")
}

fn active_pair_seeded(alice_label: &str) -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new(alice_label, &net);
    let mut bob = Runtime::new("bob", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let b = common::create_profile(&mut bob, "p-b", "Bob");
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

fn texts(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<String> {
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

fn send(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str, conv: &str, t: &str) {
    rt.dispatch(
        ctx,
        op,
        IntentBody::SendMessage {
            conversation_id: conv.into(),
            draft: Draft { text: t.into() },
        },
    );
}

fn delivery_of(
    rt: &Runtime,
    ctx: &ProfileExecutionContext,
    conv: &str,
    text: &str,
) -> DeliveryState {
    let v: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    });
    v.timeline
        .into_iter()
        .find_map(|t| match t {
            TimelineItem::Outgoing {
                text: x, delivery, ..
            } if x == text => Some(delivery),
            _ => None,
        })
        .expect("message")
}

fn fan_states(
    rt: &Runtime,
    ctx: &ProfileExecutionContext,
    text_idx: usize,
) -> Vec<(String, bool, String)> {
    rel(rt, ctx)["messages"][text_idx]["fan"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                s(&f["deviceId"]),
                f["sibling"].as_bool().unwrap(),
                s(&f["state"]),
            )
        })
        .collect()
}

/// The head and credentials of a profile's own account, as a contact would pin them.
fn head_of(rt: &Runtime, ctx: &ProfileExecutionContext) -> (Vec<u8>, Vec<Vec<u8>>) {
    let st = rt.core.debug_app_state(ctx).unwrap();
    let last = unhex(
        st["acsChain"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let body = cbor::decode(&last)
        .unwrap()
        .get(1)
        .unwrap()
        .b()
        .unwrap()
        .to_vec();
    let creds = st["acsCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| unhex(c.as_str().unwrap()).unwrap())
        .collect();
    (body, creds)
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

/// A device the test controls, with a mailbox of its own on the loopback relay.
struct Hand {
    dev: Device,
    rng: InsecureTestRng,
    seed: [u8; 32],
    km: KeyMaterial,
    route: RelayEntry,
    cap: Vec<u8>,
    read: usize,
}

impl Hand {
    fn new(label: &str, account: [u8; 32], tag: u8) -> Self {
        let mut rng = InsecureTestRng::from_label(label.as_bytes());
        let mut dev = Device::new(&mut rng);
        dev.account_id = account;
        dev.bundle_expires_at = NOW_S + 30 * 24 * 3600;
        let route = common::route_of(&[tag; 16]);
        let (seed, km) = delivery::new_key(route.binding(), NOW_S + 60 * 24 * 3600, &mut rng);
        Hand {
            dev,
            rng,
            seed,
            km,
            route,
            cap: vec![tag; 32],
            read: 0,
        }
    }

    /// Publish this device's key material and let anyone holding `cap` fetch it.
    fn publish(&mut self, net: &LoopbackNetwork) {
        let up = self.dev.generate_one_time(10, 10, &mut self.rng);
        net.perform(&NetCall::PublishKeyMaterial {
            device_id: self.dev.device_id.to_vec(),
            bundle: up.bundle,
            opks: up.opks,
            pqopks: up.pqopks,
        });
        net.perform(&NetCall::GrantKeyFetch {
            device_id: self.dev.device_id.to_vec(),
            capability: self.cap.clone(),
        });
    }

    /// The introduction a contact adopts: signed RelaySet, signed key record, key service and fetch capability.
    fn intro(&mut self) -> V {
        let rec = Record::new(1, None, vec![self.route.clone()]).unwrap();
        let signed = relayset::sign(&self.dev.signer, &self.dev.device_id, &rec, [3; 32]);
        let key =
            delivery::sign_key_record(&self.dev.signer, &self.dev.device_id, &self.km, [4; 32]);
        V::M(vec![
            (0, V::B(self.dev.device_id.to_vec())),
            (1, V::B(signed)),
            (2, V::A(vec![V::B(key)])),
            (3, V::T("https://loopback.test/".into())),
            (4, V::B(self.cap.clone())),
        ])
    }

    fn trust(&mut self, head: &(Vec<u8>, Vec<Vec<u8>>)) {
        self.dev.trust_peer(head.0.clone(), head.1.clone()).unwrap();
    }

    /// Wrappers deposited for this device since the last call, opened to inner session envelopes.
    fn inbox(&mut self, net: &LoopbackNetwork) -> Vec<Vec<u8>> {
        let all = net.mailbox_bytes(&self.route.capability[16..]);
        let new: Vec<Vec<u8>> = all[self.read..]
            .iter()
            .map(|w| {
                delivery::open(
                    w,
                    &self.seed,
                    &self.km.key_id,
                    &self.route.relay_uri,
                    &self.route.capability,
                    NOW_S,
                )
                .expect("opens with the device's delivery key")
                .inner_envelope
            })
            .collect();
        self.read = all.len();
        new
    }

    /// Decrypt everything new, as the session context of `peer_account`. Returns `(kind, event text)` per payload,
    /// and the errors of those that did not decrypt.
    fn read(
        &mut self,
        net: &LoopbackNetwork,
        peer_account: [u8; 32],
    ) -> (Vec<(u64, Option<String>)>, Vec<Error>) {
        let (mut ok, mut errs) = (vec![], vec![]);
        for env in self.inbox(net) {
            match self
                .dev
                .decrypt(&RelationshipContext::new(peer_account), &env, &mut self.rng)
            {
                Ok((_, pt, _)) => {
                    let p = cbor::decode(&pt).unwrap();
                    let kind = p.get(0).unwrap().u().unwrap();
                    let text = (kind == events::OBJECT_TYPE)
                        .then(|| events::parse(&pt))
                        .flatten()
                        .and_then(|e| e.body.get(0).ok().and_then(events::plain_text));
                    ok.push((kind, text));
                }
                Err(e) => errs.push(e),
            }
        }
        (ok, errs)
    }

    /// A message event of `author_account` in the conversation `canon`, encrypted to `peer_ik` and sealed to the route.
    fn event_to(
        &mut self,
        canon: [u8; 32],
        peer_ik: &[u8; 32],
        text: &str,
        route: &RelayEntry,
        key: &serde_json::Value,
    ) -> Vec<u8> {
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&sha256(&[text.as_bytes()])[..4]);
        id[4] = 0xEE;
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
        let (env, _) = self.dev.encrypt(peer_ik, &payload).expect("encrypt");
        delivery::seal(
            &unhex(&s(&key["publicKey"])).unwrap(),
            &unhex(&s(&key["keyId"])).unwrap().try_into().unwrap(),
            &route.relay_uri,
            &route.capability,
            NOW_S + 3600,
            &env,
            &mut self.rng,
        )
        .unwrap()
    }
}

fn account_of(rt: &Runtime, ctx: &ProfileExecutionContext, peer: bool) -> [u8; 32] {
    let st = rt.core.debug_app_state(ctx).unwrap();
    if peer {
        unhex(&s(&rel(rt, ctx)["peerAccountId"]))
            .unwrap()
            .try_into()
            .unwrap()
    } else {
        // Our own account: the genesis state names it.
        let first = unhex(st["acsChain"][0].as_str().unwrap()).unwrap();
        let body = cbor::decode(&first)
            .unwrap()
            .get(1)
            .unwrap()
            .b()
            .unwrap()
            .to_vec();
        Acs::from_body(&body).unwrap().account_id
    }
}

/// Bob's Core sends `payload` to Alice inside the relationship.
fn bob_sends(p: &mut Pair, payload: &[u8]) {
    let step = p
        .bob
        .core
        .debug_inject_payload(&p.b, &p.b_conv, payload)
        .unwrap();
    p.bob.settle(&step);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

fn acs_update(chain: Vec<Vec<u8>>, creds: Vec<Vec<u8>>, intros: Vec<V>) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(KIND_ACS_UPDATE)),
        (1, V::A(chain.into_iter().map(V::B).collect())),
        (2, V::A(creds.into_iter().map(V::B).collect())),
        (4, V::A(intros)),
    ]))
}

/// Bob links `hand` as a second device, publishes its keys, and Alice learns the head.
fn link_bob2(p: &mut Pair, tag: u8) -> Hand {
    let mut bob2 = Hand::new("bob-two", account_of(&p.alice, &p.a, true), tag);
    bob2.publish(&p.net);
    let (_, step) = p
        .bob
        .core
        .debug_link_credential(&p.b, &bob2.dev.credential().body())
        .unwrap();
    p.bob.settle(&step);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    bob2.trust(&head_of(&p.alice, &p.a));
    bob2
}

#[test]
fn a_message_reaches_every_eligible_device_and_one_that_cannot_be_sealed_to_yet_gets_it_later() {
    let mut p = active_pair();
    let mut bob2 = link_bob2(&mut p, 0xB2);
    let alice_acct = account_of(&p.alice, &p.a, false);
    // Alice knows Bob2 from the head but has no route or key for it: the copy waits, Bob's own copy goes out.
    send(
        &mut p.alice,
        &p.a,
        "m1",
        &p.a_conv,
        "before the introduction",
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv),
        vec!["before the introduction".to_string()]
    );
    let f = fan_states(&p.alice, &p.a, 0);
    assert_eq!(f.len(), 2, "{f:?}");
    assert!(
        f.iter()
            .any(|(d, sib, st)| *d == hex(&bob2.dev.device_id) && !sib && st == "waiting")
    );
    assert!(
        bob2.inbox(&p.net).is_empty(),
        "nothing is sent to a device without its keys"
    );
    // Bob introduces his new device inside the relationship.
    let (chain, creds) = chain_of(&p.bob, &p.b);
    let intro = bob2.intro();
    bob_sends(&mut p, &acs_update(chain, creds, vec![intro]));
    // Alice fetched Bob2's bundle, opened a session with her RelaySet, and the waiting copy followed.
    let (got, errs) = bob2.read(&p.net, alice_acct);
    assert!(errs.is_empty(), "{errs:?}");
    let kinds: Vec<u64> = got.iter().map(|(k, _)| *k).collect();
    assert!(kinds.contains(&KIND_RELAYSET), "{kinds:?}");
    assert_eq!(
        got.iter()
            .filter_map(|(_, t)| t.clone())
            .collect::<Vec<_>>(),
        vec!["before the introduction".to_string()]
    );
    assert_eq!(
        delivery_of(&p.alice, &p.a, &p.a_conv, "before the introduction"),
        DeliveryState::Sent
    );
    // From now on both devices get every message, once.
    send(&mut p.alice, &p.a, "m2", &p.a_conv, "to both");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let (got, errs) = bob2.read(&p.net, alice_acct);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(
        got.iter()
            .filter_map(|(_, t)| t.clone())
            .collect::<Vec<_>>(),
        vec!["to both".to_string()]
    );
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv),
        vec!["before the introduction".to_string(), "to both".to_string()]
    );
    assert_eq!(
        delivery_of(&p.alice, &p.a, &p.a_conv, "to both"),
        DeliveryState::Sent
    );
}

#[test]
fn a_revoked_device_gets_no_further_copy() {
    let mut p = active_pair();
    let mut bob2 = link_bob2(&mut p, 0xB3);
    let alice_acct = account_of(&p.alice, &p.a, false);
    let (chain, creds) = chain_of(&p.bob, &p.b);
    let intro = bob2.intro();
    bob_sends(&mut p, &acs_update(chain, creds, vec![intro]));
    send(&mut p.alice, &p.a, "m1", &p.a_conv, "while linked");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let (got, _) = bob2.read(&p.net, alice_acct);
    assert!(
        got.iter()
            .any(|(_, t)| t.as_deref() == Some("while linked"))
    );
    // Bob revokes the device; Alice learns it.
    let id = hex(&bob2.dev.device_id);
    let confirmation = nonce(&mut p.bob, &p.b, "RevokeDevice", &id);
    p.bob.dispatch(
        &p.b,
        "rev",
        IntentBody::RevokeDevice {
            device_id: id,
            confirmation,
        },
    );
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    send(&mut p.alice, &p.a, "m2", &p.a_conv, "after the revocation");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert!(
        bob2.inbox(&p.net).is_empty(),
        "K5: nothing is encrypted to a revoked device"
    );
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv),
        vec![
            "while linked".to_string(),
            "after the revocation".to_string()
        ]
    );
}

#[test]
fn our_other_device_gets_a_copy_and_what_it_sends_appears_once_as_ours() {
    let mut p = active_pair();
    let alice_acct = account_of(&p.alice, &p.a, false);
    let mut alice2 = Hand::new("alice-two", alice_acct, 0xA2);
    alice2.publish(&p.net);
    let (_, step) = p
        .alice
        .core
        .debug_link_credential(&p.a, &alice2.dev.credential().body())
        .unwrap();
    p.alice.settle(&step);
    // The ceremony would hand Alice this introduction; the test hands it over directly.
    let intro = cbor::encode(&alice2.intro());
    let step = p
        .alice
        .core
        .debug_adopt_sibling_intro(&p.a, &p.a_conv, &intro)
        .unwrap();
    p.alice.settle(&step);
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    alice2.trust(&head_of(&p.alice, &p.a));
    send(&mut p.alice, &p.a, "m1", &p.a_conv, "note to self and Bob");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let (got, errs) = alice2.read(&p.net, alice_acct);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(
        got.iter()
            .filter_map(|(_, t)| t.clone())
            .collect::<Vec<_>>(),
        vec!["note to self and Bob".to_string()]
    );
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv),
        vec!["note to self and Bob".to_string()]
    );
    // The copy to our own device never decides the message's delivery state.
    assert_eq!(
        delivery_of(&p.alice, &p.a, &p.a_conv, "note to self and Bob"),
        DeliveryState::Sent
    );
    // Alice2 sends a message in the conversation: Alice's timeline shows it as hers, once, and Bob is not involved.
    let ar = rel(&p.alice, &p.a);
    let canon: [u8; 32] = unhex(&s(&ar["canonId"])).unwrap().try_into().unwrap();
    let alice1_ik: [u8; 32] = {
        let (_, creds) = head_of(&p.alice, &p.a);
        let c = om_core::crypto::binding::Credential::from_body(&creds[0]).unwrap();
        c.ik
    };
    let w = alice2.event_to(
        canon,
        &alice1_ik,
        "from the phone",
        &common::route_of(
            &unhex(&s(&ar["myBoxes"][0]["mailbox"]))
                .unwrap()
                .try_into()
                .unwrap(),
        ),
        &ar["myDelivery"][0]["public"],
    );
    for _ in 0..2 {
        // The relay delivers it twice.
        let route = &ar["myBoxes"][0]["route"];
        p.net.perform(&NetCall::Deposit {
            mailbox: unhex(&s(&route["capability"])).unwrap(),
            relay_uri: Some(s(&route["relayUri"])),
            bytes: w.clone(),
        });
        p.alice.poll(&p.a);
    }
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    assert_eq!(
        texts(&p.alice, &p.a, &p.a_conv),
        vec![
            "note to self and Bob".to_string(),
            "from the phone".to_string()
        ]
    );
    assert_eq!(
        rel(&p.alice, &p.a)["integrityFailures"].as_u64().unwrap(),
        0
    );
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv).len(),
        1,
        "Bob never sees the self-sync copy"
    );
}

fn hex(b: &[u8]) -> String {
    om_core::crypto::hex(b)
}

/// Both devices initiate at once. Exactly one session survives (the lower EK_A wins on both), nothing is counted as
/// an integrity failure, and every message arrives once.
fn crossed(seed: u8) -> bool {
    let mut p = active_pair_seeded(&format!("alice-{seed}"));
    let mut bob2 = link_bob2(&mut p, 0xC0 + seed % 8);
    bob2.rng = InsecureTestRng::from_label(format!("crossed-{seed}").as_bytes());
    let alice_acct = account_of(&p.alice, &p.a, false);
    let alice_ik: [u8; 32] = unhex(&s(&rel(&p.bob, &p.b)["peerIk"]))
        .unwrap()
        .try_into()
        .unwrap();
    send(&mut p.alice, &p.a, "m1", &p.a_conv, "m1");
    // Alice learns the introduction and fetches Bob2's bundle: her initiation is in Bob2's mailbox.
    let (chain, creds) = chain_of(&p.bob, &p.b);
    let intro = bob2.intro();
    bob_sends(&mut p, &acs_update(chain, creds, vec![intro]));
    // Before reading it, Bob2 initiates toward Alice from her published bundle.
    let alice_dev = unhex(&s(&rel(&p.bob, &p.b)["peerDeviceId"])).unwrap();
    let cap = vec![0x77; 32];
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
    let (acs, cred) = {
        let br = rel(&p.bob, &p.b);
        (
            unhex(&s(&br["peerAcsBody"])).unwrap(),
            unhex(&s(&br["peerCredentials"][0])).unwrap(),
        )
    };
    let view = om_core::crypto::binding::PeerView {
        trusted_acs_hash: om_core::crypto::sig::ds_hash(
            om_core::crypto::binding::LBL_ACS_HASH,
            &acs,
        ),
        acs_body: &acs,
        credential_body: &cred,
        now: NOW_S + 100,
    };
    let mut rng = InsecureTestRng::from_label(format!("crossed-start-{seed}").as_bytes());
    bob2.dev.start_session(&items, &view, &mut rng).unwrap();
    let ar = rel(&p.alice, &p.a);
    let canon: [u8; 32] = unhex(&s(&ar["canonId"])).unwrap().try_into().unwrap();
    let my_mailbox: [u8; 16] = unhex(&s(&ar["myBoxes"][0]["mailbox"]))
        .unwrap()
        .try_into()
        .unwrap();
    let alice_route = common::route_of(&my_mailbox);
    let w = bob2.event_to(
        canon,
        &alice_ik,
        "first from bob2",
        &alice_route,
        &ar["myDelivery"][0]["public"],
    );
    let route = &ar["myBoxes"][0]["route"];
    p.net.perform(&NetCall::Deposit {
        mailbox: unhex(&s(&route["capability"])).unwrap(),
        relay_uri: Some(s(&route["relayUri"])),
        bytes: w,
    });
    p.alice.poll(&p.a);
    // Now Bob2 reads what Alice sent.
    let (got, errs) = bob2.read(&p.net, alice_acct);
    let alice_won = errs.is_empty(); // Bob2 accepted Alice's lower initiation
    assert!(
        errs.iter().all(|e| matches!(e, Error::CrossedInitiation)),
        "a crossed initiation is never anything but resolved or ignored: {errs:?}"
    );
    if !alice_won {
        // Bob2 won: Alice dropped her session and re-sent m1 on Bob2's; Bob2's first message was accepted.
        settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
        let (again, errs) = bob2.read(&p.net, alice_acct);
        assert!(errs.is_empty(), "{errs:?}");
        let all: Vec<String> = again.iter().filter_map(|(_, t)| t.clone()).collect();
        assert_eq!(
            all,
            vec!["m1".to_string()],
            "m1 is re-sent on the winner, once"
        );
        assert!(texts(&p.alice, &p.a, &p.a_conv).contains(&"first from bob2".to_string()));
    } else {
        // Alice won: Bob2 dropped its initiation; its first message (ignored by Alice) goes again on her session.
        let got_m1 = got
            .iter()
            .filter_map(|(_, t)| t.clone())
            .collect::<Vec<_>>();
        assert_eq!(got_m1, vec!["m1".to_string()]);
        let w = bob2.event_to(
            canon,
            &alice_ik,
            "first from bob2",
            &alice_route,
            &ar["myDelivery"][0]["public"],
        );
        p.net.perform(&NetCall::Deposit {
            mailbox: unhex(&s(&route["capability"])).unwrap(),
            relay_uri: Some(s(&route["relayUri"])),
            bytes: w,
        });
        p.alice.poll(&p.a);
        assert!(texts(&p.alice, &p.a, &p.a_conv).contains(&"first from bob2".to_string()));
    }
    // The conversation keeps working in both directions on the one surviving session.
    send(&mut p.alice, &p.a, "m2", &p.a_conv, "m2");
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
    let (more, errs) = bob2.read(&p.net, alice_acct);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(more.iter().any(|(_, t)| t.as_deref() == Some("m2")));
    let w = bob2.event_to(
        canon,
        &alice_ik,
        "second from bob2",
        &alice_route,
        &ar["myDelivery"][0]["public"],
    );
    p.net.perform(&NetCall::Deposit {
        mailbox: unhex(&s(&route["capability"])).unwrap(),
        relay_uri: Some(s(&route["relayUri"])),
        bytes: w,
    });
    p.alice.poll(&p.a);
    let t = texts(&p.alice, &p.a, &p.a_conv);
    for want in ["first from bob2", "second from bob2"] {
        assert_eq!(t.iter().filter(|x| *x == want).count(), 1, "{t:?}");
    }
    assert_eq!(
        rel(&p.alice, &p.a)["integrityFailures"].as_u64().unwrap(),
        0
    );
    assert_eq!(
        texts(&p.bob, &p.b, &p.b_conv).len(),
        2,
        "Bob's own copies are unaffected"
    );
    alice_won
}

#[test]
fn a_crossed_initiation_leaves_one_session_whichever_side_wins() {
    let (mut alice_won, mut bob2_won) = (false, false);
    for seed in 0..24u8 {
        if crossed(seed) {
            alice_won = true;
        } else {
            bob2_won = true;
        }
        if alice_won && bob2_won {
            return;
        }
    }
    panic!("never saw both outcomes: alice_won={alice_won} bob2_won={bob2_won}");
}
