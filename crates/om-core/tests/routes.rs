// SPDX-License-Identifier: MPL-2.0
//! Audit M1 / deviation D1: delivery keys bind to the route the host's provider adapter allocated (a `RelayEntry`), not
//! to the pass-2 stand-in `om-core:mailbox/v0`; state from before routes existed migrates to explicit legacy routes and
//! keeps working.

mod common;

use common::{FakeRequester, app_state, create_profile, invite};
use om_core::api::*;
use om_core::crypto::delivery::{self, CORE_ROUTE_URI};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::{hex, unhex};
use om_core::host::NetCall;
use om_core::route::{RelayEntry, RelaySet, RouteOffer};
use om_core::runtime::{LOOPBACK_RELAY_URI, LoopbackNetwork, Runtime, settle_all};
use serde_json::{Value, json};

struct Duo {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
}

/// Alice and Bob in an ACTIVE relationship, one message each way delivered.
fn active_duo() -> Duo {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
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
    let req = alice
        .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })
        .remove(0);
    alice.dispatch(
        &a,
        "ok",
        IntentBody::AcceptIntroduction { request_id: req.id },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let mut d = Duo {
        net,
        alice,
        bob,
        a,
        b,
    };
    say(&mut d, true, "hello from alice");
    say(&mut d, false, "hello from bob");
    d
}

fn say(d: &mut Duo, from_alice: bool, text: &str) {
    let (rt, ctx) = if from_alice {
        (&mut d.alice, &d.a)
    } else {
        (&mut d.bob, &d.b)
    };
    let conv = rt
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: ctx.clone() })[0]
        .id
        .clone();
    let op = format!("m-{}", hex(text.as_bytes()));
    rt.dispatch(
        ctx,
        &op,
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft { text: text.into() },
        },
    );
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
}

fn texts(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<String> {
    let app = app_state(rt, ctx);
    let rel = app["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    rel["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["text"].as_str().map(str::to_string))
        .collect()
}

fn the_relationship(rt: &Runtime, ctx: &ProfileExecutionContext) -> Value {
    app_state(rt, ctx)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

fn binding_of(entry: &Value) -> String {
    hex(&delivery::route_binding(
        entry["relayUri"].as_str().unwrap(),
        &unhex(entry["capability"].as_str().unwrap()).unwrap(),
    ))
}

#[test]
fn delivery_keys_bind_to_the_route_the_adapter_allocated_and_never_to_the_stand_in() {
    let d = active_duo();
    for (rt, ctx, peer_rt, peer_ctx) in [
        (&d.alice, &d.a, &d.bob, &d.b),
        (&d.bob, &d.b, &d.alice, &d.a),
    ] {
        let rel = the_relationship(rt, ctx);
        let mine = &rel["myRelays"]["entries"];
        assert_eq!(mine.as_array().unwrap().len(), 1);
        assert_eq!(mine[0]["relayUri"], LOOPBACK_RELAY_URI);
        // The capability peers deposit to is not the mailbox id this install polls: Core keeps them apart.
        let cap = mine[0]["capability"].as_str().unwrap();
        assert_ne!(cap, rel["myMailbox"].as_str().unwrap());
        assert!(cap.ends_with(rel["myMailbox"].as_str().unwrap()));
        // Every key we receive on commits to exactly that entry...
        for k in rel["myDelivery"].as_array().unwrap() {
            assert_eq!(k["public"]["routeBinding"], binding_of(&mine[0]));
        }
        // ...and the key the peer seals to for us is the one the peer pinned for this route.
        let peer_rel = the_relationship(peer_rt, peer_ctx);
        // (the expiry each side stamps on its copy differs: the peer learned it from the signed request or Accept)
        for f in ["relayUri", "capability", "priority"] {
            assert_eq!(peer_rel["peerRelays"]["entries"][0][f], mine[0][f], "{f}");
        }
        assert_eq!(
            peer_rel["peerDelivery"]["public"]["routeBinding"],
            binding_of(&mine[0])
        );
        // The stand-in appears nowhere in a profile created with routes.
        let all = serde_json::to_string(&app_state(rt, ctx)).unwrap();
        assert!(!all.contains(CORE_ROUTE_URI), "no stand-in route in state");
    }
    assert_eq!(texts(&d.alice, &d.a).len(), 2);
    assert_eq!(texts(&d.bob, &d.b).len(), 2);
}

#[test]
fn a_deposit_names_the_relay_and_carries_the_capability_not_the_mailbox() {
    let mut d = active_duo();
    let conv = d
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.a.clone() })[0]
        .id
        .clone();
    d.alice.dispatch(
        &d.a,
        "one-more",
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft {
                text: "route check".into(),
            },
        },
    );
    let peer = the_relationship(&d.alice, &d.a)["peerRelays"]["entries"][0].clone();
    let calls: Vec<NetCall> = d
        .alice
        .core
        .net_ops(&d.a)
        .unwrap()
        .into_iter()
        .map(|o| o.call)
        .collect();
    let deposit = calls.iter().find_map(|c| match c {
        NetCall::Deposit {
            mailbox, relay_uri, ..
        } => Some((mailbox.clone(), relay_uri.clone())),
        _ => None,
    });
    let (cap, uri) = deposit.expect("a deposit is owed");
    assert_eq!(uri.as_deref(), Some(LOOPBACK_RELAY_URI));
    assert_eq!(hex(&cap), peer["capability"].as_str().unwrap());
    assert_eq!(cap.len(), 32);
}

#[test]
fn nothing_that_needs_a_mailbox_starts_without_a_route_but_an_invite_still_can() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let _b = create_profile(&mut bob, "p-bob", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    // Bob's host supplies no routes (no Home configured): a restart drops any left over from setup.
    let b = bob.restart().remove(0);
    assert_eq!(bob.core.route_offers(), 0);
    let step = bob
        .core
        .dispatch(&Intent {
            ctx: b.clone(),
            operation_id: "acc".into(),
            body: IntentBody::AcceptInvitation { link },
        })
        .unwrap();
    bob.settle(&step);
    // Resolving the invite needs no mailbox of ours; the request itself does.
    for _ in 0..3 {
        bob.flush_net(&b);
    }
    assert_eq!(
        bob.outcome(&b, "acc"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unavailable
        })
    );
    assert_eq!(
        app_state(&bob, &b)["relationships"]
            .as_object()
            .unwrap()
            .len(),
        0,
        "no half-made relationship"
    );
    assert_eq!(net.deposit_count(), 0);
}

#[test]
fn offers_are_single_use_validated_and_bounded() {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("x", &net);
    let good = |n: u8| RouteOffer {
        mailbox: vec![n; 16],
        relay_uri: "https://home.test/".into(),
        capability: vec![n; 32],
    };
    assert_eq!(
        rt.core.supply_routes(vec![good(1), good(1)]),
        1,
        "a duplicate mailbox is dropped"
    );
    let bad = [
        RouteOffer {
            mailbox: vec![2; 15],
            ..good(2)
        },
        RouteOffer {
            capability: vec![2; 15],
            ..good(2)
        },
        RouteOffer {
            capability: vec![2; 65],
            ..good(2)
        },
        RouteOffer {
            relay_uri: String::new(),
            ..good(2)
        },
        RouteOffer {
            relay_uri: "https://has space.test/".into(),
            ..good(2)
        },
        RouteOffer {
            relay_uri: CORE_ROUTE_URI.into(),
            ..good(2)
        },
    ];
    assert_eq!(
        rt.core.supply_routes(bad.to_vec()),
        1,
        "invalid offers and the legacy stand-in are refused"
    );
    let many: Vec<RouteOffer> = (10..60).map(good).collect();
    assert_eq!(rt.core.supply_routes(many), om_core::route::MAX_OFFERS);
}

#[test]
fn a_reply_route_whose_key_is_bound_elsewhere_is_dropped() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"routes");
    let now_ms = 1_790_000_000_000u64;
    // The request claiming another relay than its key is bound to is dropped; the honest one is held for the user.
    for (n, (claimed, expect)) in [(Some("https://elsewhere.test/"), 0), (None, 1)]
        .into_iter()
        .enumerate()
    {
        let (link, record) = invite(&mut alice, &a, &format!("inv-{n}"));
        let mut r = FakeRequester::new(&mut rng);
        r.set_mailbox([30 + n as u8; 16]);
        r.claimed_uri = claimed.map(str::to_string);
        r.request(&net, &mut rng, &link, &record, now_ms, 3600, "Eve");
        alice.poll(&a);
        let seen = alice
            .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })
            .len();
        assert_eq!(seen, expect, "case {n}");
    }
}

// ----------------------------------------------------------------------------------------------- migration

/// Rewrite the stored profile into the shape and bindings of app state version 3: the stand-in route and the bare
/// mailbox everywhere, `peerMailbox` instead of relay sets. (The keys are unchanged; the route only enters the
/// HPKE info and the signed binding, so a rewritten binding is exactly what pass-2 Core had produced.)
fn into_v3_world(rt: &Runtime, profile: &str) {
    let rec = rt
        .store
        .records(profile)
        .into_iter()
        .find(|r| r.key == "state")
        .unwrap();
    let v = om_core::crypto::cbor::decode(&rec.value).unwrap();
    let mut app: Value = serde_json::from_slice(v.get(1).unwrap().b().unwrap()).unwrap();
    assert!(
        app["outbox"].as_array().unwrap().is_empty(),
        "settled before rewriting"
    );
    app["version"] = json!(3);
    let legacy_binding = |mailbox: &str| {
        json!(hex(&delivery::mailbox_route_binding(
            &unhex(mailbox).unwrap()
        )))
    };
    for rel in app["relationships"].as_object_mut().unwrap().values_mut() {
        let mine = rel["myMailbox"].as_str().unwrap().to_string();
        let peer_cap = rel["peerRelays"]["entries"][0]["capability"]
            .as_str()
            .unwrap()
            .to_string();
        let peer_mailbox = peer_cap[peer_cap.len() - 32..].to_string(); // loopback capability = prefix || mailbox id
        for k in rel["myDelivery"].as_array_mut().unwrap() {
            k["public"]["routeBinding"] = legacy_binding(&mine);
        }
        rel["peerDelivery"]["public"]["routeBinding"] = legacy_binding(&peer_mailbox);
        rel["peerMailbox"] = json!(peer_mailbox);
        let o = rel.as_object_mut().unwrap();
        o.remove("myRelays");
        o.remove("peerRelays");
    }
    let old = om_core::crypto::cbor::encode(&om_core::crypto::cbor::V::M(vec![
        (0, v.get(0).unwrap().clone()),
        (
            1,
            om_core::crypto::cbor::V::B(serde_json::to_vec(&app).unwrap()),
        ),
    ]));
    rt.store.replace_record(profile, "state", old);
}

#[test]
fn an_active_version_3_relationship_migrates_to_explicit_legacy_routes_and_keeps_working() {
    let mut d = active_duo();
    into_v3_world(&d.alice, "p-alice");
    into_v3_world(&d.bob, "p-bob");
    d.a = d.alice.restart().remove(0);
    d.b = d.bob.restart().remove(0);

    for (rt, ctx) in [(&d.alice, &d.a), (&d.bob, &d.b)] {
        let rel = the_relationship(rt, ctx);
        assert_eq!(app_state(rt, ctx)["version"], 4);
        assert!(
            rel.get("peerMailbox").is_none(),
            "the bare peer mailbox is gone"
        );
        for side in ["myRelays", "peerRelays"] {
            let e = &rel[side]["entries"];
            assert_eq!(e.as_array().unwrap().len(), 1, "{side}");
            assert_eq!(
                e[0]["relayUri"], CORE_ROUTE_URI,
                "{side}: explicit legacy entry"
            );
            assert_eq!(
                e[0]["capability"].as_str().unwrap().len(),
                32,
                "{side}: the bare 16-byte mailbox"
            );
        }
        assert_eq!(
            rel["myRelays"]["entries"][0]["capability"],
            rel["myMailbox"]
        );
    }

    // The relationship keeps working in both directions, and a legacy destination is deposited to as before:
    // the bare mailbox and no relay name (the adapter derives the capability itself).
    let conv = d
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.a.clone() })[0]
        .id
        .clone();
    d.alice.dispatch(
        &d.a,
        "after-migration",
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft {
                text: "still here".into(),
            },
        },
    );
    let owed = d.alice.core.net_ops(&d.a).unwrap();
    assert!(owed.iter().any(|o| matches!(&o.call, NetCall::Deposit { mailbox, relay_uri: None, .. } if mailbox.len() == 16)));
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    say(&mut d, false, "and bob answers");
    assert!(texts(&d.bob, &d.b).contains(&"still here".to_string()));
    assert!(texts(&d.alice, &d.a).contains(&"and bob answers".to_string()));
    assert_eq!(the_relationship(&d.bob, &d.b)["integrityFailures"], 0);

    // New relationships use real routes: a third party reaches Alice through a normal invite.
    let mut carol = Runtime::new("carol", &d.net);
    let c = create_profile(&mut carol, "p-carol", "Carol");
    d.alice.dispatch(&d.a, "inv2", IntentBody::CreateInvite);
    d.alice.flush_net(&d.a);
    let link = d
        .alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: d.a.clone(),
            operation_id: "inv2".into(),
        })
        .unwrap()
        .link;
    carol.dispatch(&c, "acc", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut d.alice, &d.a), (&mut carol, &c)]);
    let intros = d
        .alice
        .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: d.a.clone() });
    assert_eq!(intros.len(), 1);
    d.alice.dispatch(
        &d.a,
        "ok2",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut d.alice, &d.a), (&mut carol, &c)]);
    let rels = app_state(&d.alice, &d.a)["relationships"]
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(rels.len(), 2);
    let legacy: Vec<_> = rels
        .values()
        .filter(|r| r["myRelays"]["entries"][0]["relayUri"] == CORE_ROUTE_URI)
        .collect();
    assert_eq!(
        legacy.len(),
        1,
        "only the migrated relationship stays on the legacy route"
    );
}

#[test]
fn a_relay_entry_round_trips_and_rejects_what_the_spec_forbids() {
    let e = RelayEntry::new("https://home.test/", &[9u8; 32], 2_000_000_000).unwrap();
    assert_eq!(RelayEntry::from_v(&e.to_v()), Some(e.clone()));
    assert_eq!(RelaySet::single(e.clone()).primary(), Some(&e));
    assert!(RelaySet::default().primary().is_none());
    let two = RelaySet {
        entries: vec![e.clone(), e.clone()],
    };
    assert!(
        two.primary().is_none(),
        "several entries per mailbox are not used yet, and not half-used"
    );
    assert!(RelayEntry::new("https://home.test/", &[9u8; 15], 1).is_none());
    assert!(RelayEntry::new("", &[9u8; 16], 1).is_none());
    assert_eq!(
        e.binding(),
        delivery::route_binding("https://home.test/", &[9u8; 32])
    );
    assert_ne!(e.binding(), RelayEntry::legacy(&[9u8; 16]).binding());
    // An unknown field is not silently accepted.
    let mut v = e.to_v();
    if let om_core::crypto::cbor::V::M(m) = &mut v {
        m.push((9, om_core::crypto::cbor::V::U(1)));
    }
    assert!(RelayEntry::from_v(&v).is_none());
}
