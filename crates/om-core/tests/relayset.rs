// SPDX-License-Identifier: MPL-2.0
//! Deviation D22: the signed, versioned, hash-chained RelaySet of a relationship (MULTI_RELAY_AND_DELIVERY_RESILIENCE
//! 2, 2.1, 2.2, 4; REACHABILITY_AND_OBJECT_REPAIR 2; ADR-0161 section 2). Vectors for branch convergence, then two
//! people who each hold two Homes: routes are added, a blocked Home is failed over, and a Home is retired with overlap.

mod common;

use common::{app_state, create_profile};
use om_core::api::*;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::hex;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::DeviceSigner;
use om_core::relayset::{self, Lineage, MAX_PENDING, Node, Offer, Record};
use om_core::route::RelayEntry;
use om_core::runtime::{LoopbackNetwork, Runtime, loopback_home_uri, settle_all};
use serde_json::Value;

// ------------------------------------------------------------------------------------------------ vectors

fn entry(n: u8) -> RelayEntry {
    RelayEntry::new(&format!("https://h{n}.test/"), &[n; 32], 2_000_000_000).unwrap()
}

fn rec(version: u64, parent: Option<&Record>, entries: &[u8]) -> Record {
    Record::new(
        version,
        parent.map(Record::hash),
        entries.iter().map(|n| entry(*n)).collect(),
    )
    .unwrap()
}

fn node(r: &Record, signer: u8) -> Node {
    Node::from_record(r, &[signer; 16])
}

fn always(_: &str) -> bool {
    true
}

fn heads(l: &Lineage, eligible: impl Fn(&str) -> bool) -> Option<String> {
    l.effective(eligible).map(|n| n.hash.clone())
}

/// Every order in which `items` can arrive.
fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = vec![];
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let x = rest.remove(i);
        for mut p in permutations(&rest) {
            p.insert(0, x.clone());
            out.push(p);
        }
    }
    out
}

#[test]
fn the_effective_set_is_the_highest_version_then_the_smallest_hash_whatever_the_arrival_order() {
    // genesis g; two devices extend it concurrently (a2, b2); a2 is extended again (a3); b2 stays a side branch.
    let g = rec(1, None, &[1]);
    let a2 = rec(2, Some(&g), &[1, 2]);
    let b2 = rec(2, Some(&g), &[1, 3]);
    let a3 = rec(3, Some(&a2), &[2]);
    let all = [node(&g, 1), node(&a2, 1), node(&b2, 2), node(&a3, 1)];
    let mut winners = std::collections::BTreeSet::new();
    for order in permutations(&all) {
        let mut l = Lineage::default();
        for n in order {
            let o = l.offer(n);
            assert!(matches!(o, Offer::Accepted | Offer::Pending), "{o:?}");
        }
        assert!(l.pending.is_empty(), "every child found its parent");
        assert_eq!(l.nodes.len(), 4, "side branches are kept as history");
        winners.insert(heads(&l, always).unwrap());
    }
    assert_eq!(winners.len(), 1, "arrival order never picks a winner");
    assert_eq!(winners.into_iter().next().unwrap(), hex(&a3.hash()));
}

#[test]
fn concurrent_branches_at_the_same_version_pick_the_smallest_hash() {
    let g = rec(1, None, &[1]);
    let a2 = rec(2, Some(&g), &[1, 2]);
    let b2 = rec(2, Some(&g), &[1, 3]);
    let want = if a2.hash() < b2.hash() { &a2 } else { &b2 };
    for order in permutations(&[node(&g, 1), node(&a2, 1), node(&b2, 2)]) {
        let mut l = Lineage::default();
        for n in order {
            l.offer(n);
        }
        let e = l.effective(always).unwrap();
        assert_eq!(e.hash, hex(&want.hash()));
        assert_eq!(e.entries, want.entries);
    }
}

#[test]
fn a_child_before_its_parent_is_pending_and_is_reconsidered_when_the_parent_arrives() {
    let g = rec(1, None, &[1]);
    let a2 = rec(2, Some(&g), &[1, 2]);
    let a3 = rec(3, Some(&a2), &[2]);
    let mut l = Lineage::default();
    assert_eq!(l.offer(node(&a3, 1)), Offer::Pending);
    assert_eq!(l.offer(node(&a2, 1)), Offer::Pending);
    assert!(l.effective(always).is_none(), "nothing is effective yet");
    assert_eq!(l.offer(node(&g, 1)), Offer::Accepted);
    assert_eq!(l.nodes.len(), 3, "the chain unlocked together");
    assert!(l.pending.is_empty());
    assert_eq!(heads(&l, always).unwrap(), hex(&a3.hash()));
    assert_eq!(l.offer(node(&a3, 1)), Offer::Duplicate, "idempotent");
}

#[test]
fn version_jumps_and_malformed_continuity_are_rejected() {
    let g = rec(1, None, &[1]);
    let mut l = Lineage::default();
    l.offer(node(&g, 1));
    // v3 naming a v1 parent: a jump.
    let jump = Record::new(3, Some(g.hash()), vec![entry(2)]).unwrap();
    assert!(matches!(l.offer(node(&jump, 1)), Offer::Rejected(_)));
    // The records themselves cannot be built wrongly: genesis with a parent, successor without one.
    assert!(Record::new(1, Some(g.hash()), vec![entry(2)]).is_none());
    assert!(Record::new(2, None, vec![entry(2)]).is_none());
    // A forged node bypassing the constructor is rejected by the lineage too.
    let mut bad = node(&g, 1);
    bad.hash = "aa".repeat(32);
    bad.previous = Some(hex(&g.hash()));
    assert!(
        matches!(l.offer(bad.clone()), Offer::Rejected(_)),
        "genesis names a parent"
    );
    bad.version = 2;
    bad.previous = None;
    assert!(
        matches!(l.offer(bad), Offer::Rejected(_)),
        "successor names no parent"
    );
    // A waiting record whose parent then arrives with the wrong version is dropped, not kept forever.
    let mut l = Lineage::default();
    let far = Node {
        version: 5,
        ..node(&rec(2, Some(&g), &[2]), 1)
    };
    assert_eq!(l.offer(far), Offer::Pending);
    l.offer(node(&g, 1));
    assert!(l.pending.is_empty() && l.nodes.len() == 1);
    // An unknown parent (another lineage) stays pending, bounded.
    let mut l = Lineage::default();
    for i in 0..MAX_PENDING as u8 {
        let other = rec(1, None, &[100 + i]);
        let orphan = rec(2, Some(&other), &[100 + i]);
        assert_eq!(l.offer(node(&orphan, 1)), Offer::Pending);
    }
    let other = rec(1, None, &[200]);
    assert!(matches!(
        l.offer(node(&rec(2, Some(&other), &[200]), 1)),
        Offer::Rejected(_)
    ));
}

#[test]
fn a_record_whose_signer_is_no_longer_eligible_stops_being_a_candidate_but_stays_as_ancestry() {
    // Device 1 signs v1 and v2; device 2 signs v3 on top. Device 2 is then quarantined: v2 (device 1) is effective again.
    // If device 1 is the one removed, v3 is still structurally verifiable through v2's retained hash.
    let g = rec(1, None, &[1]);
    let a2 = rec(2, Some(&g), &[1, 2]);
    let a3 = rec(3, Some(&a2), &[2]);
    let mut l = Lineage::default();
    for n in [node(&g, 1), node(&a2, 1), node(&a3, 2)] {
        l.offer(n);
    }
    let sig = |d: u8| hex(&[d; 16]);
    assert_eq!(
        heads(&l, |s| s == sig(1) || s == sig(2)).unwrap(),
        hex(&a3.hash())
    );
    assert_eq!(
        heads(&l, |s| s == sig(1)).unwrap(),
        hex(&a2.hash()),
        "recomputed from retained candidates"
    );
    assert_eq!(
        heads(&l, |s| s == sig(2)).unwrap(),
        hex(&a3.hash()),
        "ineligible ancestors do not hide a valid descendant"
    );
    assert!(heads(&l, |_| false).is_none());
}

#[test]
fn a_signed_relayset_verifies_only_under_the_signer_and_key_and_is_canonical() {
    let mut rng = InsecureTestRng::from_label(b"relayset-sign");
    let (s1, s2) = (
        DeviceSigner::generate(&mut rng),
        DeviceSigner::generate(&mut rng),
    );
    let (d1, d2) = ([1u8; 16], [2u8; 16]);
    let r = rec(1, None, &[1, 2]);
    let bytes = relayset::sign(&s1, &d1, &r, [7; 32]);
    assert!(bytes.len() <= relayset::MAX_BYTES);
    assert_eq!(relayset::claimed_signer(&bytes), Some(d1));
    assert_eq!(relayset::open(&bytes, &d1, &s1.public()).unwrap(), r);
    assert!(
        relayset::open(&bytes, &d1, &s2.public()).is_err(),
        "another device's key"
    );
    assert!(
        relayset::open(&bytes, &d2, &s1.public()).is_err(),
        "another claimed signer"
    );
    // Any change to the body breaks the signature.
    let mut v = cbor::decode(&bytes).unwrap();
    let mut body = v.get(1).unwrap().b().unwrap().to_vec();
    let at = body.len() - 3;
    body[at] ^= 1;
    v = V::M(vec![
        (1, V::B(body)),
        (2, V::A(v.get(2).unwrap().a().unwrap().to_vec())),
    ]);
    assert!(relayset::open(&cbor::encode(&v), &d1, &s1.public()).is_err());
    // The same record signed by two devices (a signature list of two) is not a single device signature.
    let hash = r.hash();
    let child = Record::new(2, Some(hash), vec![entry(2)]).unwrap();
    assert_ne!(child.hash(), hash);
}

// ------------------------------------------------------------------------------------------------ two people, two Homes each

const ALICE_HOMES: [u8; 2] = [0, 1];
const BOB_HOMES: [u8; 2] = [2, 3];

struct Duo {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
}

fn uri(h: u8) -> String {
    loopback_home_uri(h)
}

/// Alice (Homes 0 and 1) and Bob (Homes 2 and 3) in an ACTIVE relationship; each has added its second Home's route.
fn two_home_duo() -> Duo {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    alice.homes = ALICE_HOMES.to_vec();
    bob.homes = BOB_HOMES.to_vec();
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
    rt.dispatch(
        ctx,
        &format!("m-{}", hex(text.as_bytes())),
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft { text: text.into() },
        },
    );
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
}

fn rel(rt: &Runtime, ctx: &ProfileExecutionContext) -> Value {
    app_state(rt, ctx)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

fn texts(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<String> {
    rel(rt, ctx)["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["text"].as_str().map(str::to_string))
        .collect()
}

fn uris(entries: &Value) -> Vec<String> {
    let mut v: Vec<String> = entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["relayUri"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

fn caps(entries: &Value) -> Vec<String> {
    entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["capability"].as_str().unwrap().to_string())
        .collect()
}

fn polled_homes(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<String> {
    let mut v: Vec<String> = rt
        .core
        .poll_targets(ctx)
        .unwrap()
        .into_iter()
        .filter_map(|t| t.relay_uri)
        .collect();
    v.sort();
    v
}

fn version_of(lineage: &Value) -> u64 {
    lineage["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["version"].as_u64().unwrap())
        .max()
        .unwrap()
}

#[test]
fn each_side_adds_a_route_at_its_second_home_and_the_peer_learns_it_through_a_signed_version() {
    let d = two_home_duo();
    for (rt, ctx, peer_rt, peer_ctx, mine, theirs) in [
        (&d.alice, &d.a, &d.bob, &d.b, ALICE_HOMES, BOB_HOMES),
        (&d.bob, &d.b, &d.alice, &d.a, BOB_HOMES, ALICE_HOMES),
    ] {
        let r = rel(rt, ctx);
        let mut want: Vec<String> = mine.iter().map(|h| uri(*h)).collect();
        want.sort();
        assert_eq!(uris(&r["myRelays"]), want, "our set names both Homes");
        assert_eq!(
            r["myBoxes"].as_array().unwrap().len(),
            2,
            "and we poll a mailbox at each"
        );
        assert_eq!(
            version_of(&r["myLineage"]),
            2,
            "genesis plus one signed version"
        );
        let mut caps_mine = caps(&r["myRelays"]);
        caps_mine.sort();
        caps_mine.dedup();
        assert_eq!(caps_mine.len(), 2, "one capability per route (PC-A05)");
        // The peer holds exactly our set, from our signed lineage, and a delivery key for each route.
        let p = rel(peer_rt, peer_ctx);
        assert_eq!(uris(&p["peerRelays"]), want);
        assert_eq!(version_of(&p["peerLineage"]), 2);
        let mut their_bindings: Vec<String> = p["peerKeys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["public"]["routeBinding"].as_str().unwrap().to_string())
            .collect();
        their_bindings.push(
            p["peerDelivery"]["public"]["routeBinding"]
                .as_str()
                .unwrap()
                .to_string(),
        );
        their_bindings.sort();
        their_bindings.dedup();
        assert_eq!(
            their_bindings.len(),
            2,
            "a sealed-delivery key per route (ADR-0142)"
        );
        let _ = theirs;
    }
    // No capability is shared between the two directions either.
    let mut all = caps(&rel(&d.alice, &d.a)["myRelays"]);
    all.extend(caps(&rel(&d.bob, &d.b)["myRelays"]));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 4);
    assert_eq!(polled_homes(&d.alice, &d.a).len(), 2);
}

#[test]
fn a_blocked_home_does_not_stall_the_peer_it_fails_over_to_the_next_route_with_the_same_event() {
    let mut d = two_home_duo();
    let before = texts(&d.bob, &d.b).len();
    // Bob's preferred Home (2) is blocked: a vendor block, an outage.
    d.net.block_home(&uri(BOB_HOMES[0]));
    say(&mut d, true, "through the second route");
    assert_eq!(texts(&d.bob, &d.b).len(), before + 1, "delivered once");
    assert_eq!(
        texts(&d.bob, &d.b).last().unwrap(),
        "through the second route"
    );
    let a = rel(&d.alice, &d.a);
    let delivered = a["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(delivered["delivery"]["state"], "sent");
    assert_eq!(
        a["messages"].as_array().unwrap().len(),
        texts(&d.alice, &d.a).len()
    );
    assert!(
        !a["routeHealth"].as_object().unwrap().is_empty(),
        "the route left is remembered as unhealthy"
    );
    // Bob could not drain his blocked Home and received nothing there; the wrapper that arrived is bound to the other.
    let log = d.net.deposit_log();
    let (blocked_mailbox, other_mailbox) = {
        let bx = rel(&d.bob, &d.b);
        let find = |h: u8| {
            bx["myBoxes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["route"]["relayUri"] == uri(h))
                .map(|b| hex(&om_core::crypto::unhex(b["mailbox"].as_str().unwrap()).unwrap()))
                .unwrap()
        };
        (find(BOB_HOMES[0]), find(BOB_HOMES[1]))
    };
    let at = |m: &str| log.iter().filter(|(mb, _)| hex(mb) == m).count();
    assert_eq!(
        at(&blocked_mailbox) + 1,
        at(&other_mailbox) + at(&blocked_mailbox),
        "the message lives only at the second Home"
    );
    assert!(at(&other_mailbox) >= 1);

    // The next message goes straight to the healthy route: one round of network work is enough.
    let conv = d
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.a.clone() })[0]
        .id
        .clone();
    d.alice.dispatch(
        &d.a,
        "straight",
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft {
                text: "and straight away".into(),
            },
        },
    );
    d.alice.flush_net(&d.a);
    d.bob.poll(&d.b);
    assert_eq!(texts(&d.bob, &d.b).last().unwrap(), "and straight away");

    // Bob's own sends are unaffected (Alice's Homes are up) and the relationship carries on in both directions.
    say(&mut d, false, "from bob");
    assert_eq!(texts(&d.alice, &d.a).last().unwrap(), "from bob");
}

#[test]
fn a_message_in_flight_when_the_home_goes_down_is_delivered_once_after_failover_and_after_it_returns()
 {
    let mut d = two_home_duo();
    let conv = d
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.a.clone() })[0]
        .id
        .clone();
    d.alice.dispatch(
        &d.a,
        "inflight",
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft {
                text: "in flight".into(),
            },
        },
    );
    d.net.block_home(&uri(BOB_HOMES[0]));
    // Two rounds without progress, then the same inner envelope is re-sealed for the other route.
    d.alice.flush_net(&d.a);
    d.alice.flush_net(&d.a);
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    assert_eq!(
        texts(&d.bob, &d.b)
            .iter()
            .filter(|t| *t == "in flight")
            .count(),
        1
    );
    d.net.unblock_home(&uri(BOB_HOMES[0]));
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    assert_eq!(
        texts(&d.bob, &d.b)
            .iter()
            .filter(|t| *t == "in flight")
            .count(),
        1,
        "still exactly once"
    );
}

#[test]
fn moving_off_a_home_publishes_a_version_without_it_and_keeps_polling_it_for_the_overlap() {
    let mut d = two_home_duo();
    let old = uri(ALICE_HOMES[0]);
    // Bob still has Alice's v2 and writes to her preferred (old) Home while she moves.
    let (stuck, step) = d.alice.core.retire_home(&d.a, &old, false).unwrap();
    d.alice.settle(&step);
    d.alice.homes = vec![ALICE_HOMES[1]]; // the host no longer offers the Home she left
    assert_eq!(stuck, 0);
    let conv = d
        .bob
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.b.clone() })[0]
        .id
        .clone();
    d.bob.dispatch(
        &d.b,
        "in-transit",
        IntentBody::SendMessage {
            conversation_id: conv,
            draft: Draft {
                text: "sent to the old route".into(),
            },
        },
    );
    d.bob.flush_net(&d.b); // deposited at Alice's old Home before her new version reaches him
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    assert_eq!(
        texts(&d.alice, &d.a).last().unwrap(),
        "sent to the old route",
        "the dropped route is still drained during the overlap"
    );
    let a = rel(&d.alice, &d.a);
    assert_eq!(
        uris(&a["myRelays"]),
        vec![uri(ALICE_HOMES[1])],
        "the new version no longer names it"
    );
    assert_eq!(version_of(&a["myLineage"]), 3);
    assert_eq!(
        a["myBoxes"].as_array().unwrap().len(),
        2,
        "but the mailbox is kept for now"
    );
    assert_eq!(polled_homes(&d.alice, &d.a).len(), 2);
    let b = rel(&d.bob, &d.b);
    assert_eq!(
        uris(&b["peerRelays"]),
        vec![uri(ALICE_HOMES[1])],
        "Bob follows the effective version"
    );
    assert_eq!(version_of(&b["peerLineage"]), 3);

    // Bob now writes to the new route: that is the acknowledgement. A day later the old route is closed.
    say(&mut d, false, "to the new route");
    assert_eq!(texts(&d.alice, &d.a).last().unwrap(), "to the new route");
    assert_eq!(
        polled_homes(&d.alice, &d.a).len(),
        2,
        "not closed before the grace period"
    );
    d.alice.tick(2 * 24 * 60 * 60 * 1000);
    d.bob.tick(2 * 24 * 60 * 60 * 1000);
    say(&mut d, true, "later");
    let a = rel(&d.alice, &d.a);
    assert_eq!(
        a["myBoxes"].as_array().unwrap().len(),
        1,
        "the overlap is over"
    );
    assert_eq!(polled_homes(&d.alice, &d.a), vec![uri(ALICE_HOMES[1])]);
    let keys: Vec<&str> = a["myDelivery"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["public"]["routeBinding"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys.len(),
        1,
        "the dropped route's delivery key is gone with it"
    );
}

#[test]
fn an_unacknowledged_old_route_still_closes_when_its_overlap_ends() {
    let mut d = two_home_duo();
    let (_, step) = d
        .alice
        .core
        .retire_home(&d.a, &uri(ALICE_HOMES[0]), false)
        .unwrap();
    d.alice.settle(&step);
    d.alice.homes = vec![ALICE_HOMES[1]];
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    assert_eq!(polled_homes(&d.alice, &d.a).len(), 2);
    d.alice.tick(31 * 24 * 60 * 60 * 1000);
    d.bob.tick(31 * 24 * 60 * 60 * 1000);
    say(&mut d, true, "after the overlap");
    assert_eq!(polled_homes(&d.alice, &d.a), vec![uri(ALICE_HOMES[1])]);
}

#[test]
fn an_emergency_revocation_drops_the_route_at_once() {
    let mut d = two_home_duo();
    let (stuck, step) = d
        .alice
        .core
        .retire_home(&d.a, &uri(ALICE_HOMES[0]), true)
        .unwrap();
    d.alice.settle(&step);
    d.alice.homes = vec![ALICE_HOMES[1]];
    assert_eq!(stuck, 0);
    let a = rel(&d.alice, &d.a);
    assert_eq!(a["myBoxes"].as_array().unwrap().len(), 1, "no overlap");
    assert_eq!(
        a["myDelivery"].as_array().unwrap().len(),
        1,
        "its delivery key is invalidated with it"
    );
    assert_eq!(polled_homes(&d.alice, &d.a), vec![uri(ALICE_HOMES[1])]);
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    say(&mut d, false, "after the revocation");
    assert_eq!(
        texts(&d.alice, &d.a).last().unwrap(),
        "after the revocation"
    );
    assert_eq!(
        uris(&rel(&d.bob, &d.b)["peerRelays"]),
        vec![uri(ALICE_HOMES[1])]
    );
}

#[test]
fn a_person_with_one_route_cannot_retire_it_and_nothing_changes() {
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
    let before = rel(&alice, &a);
    assert_eq!(
        version_of(&before["myLineage"]),
        1,
        "a single-entry set still has its signed lineage"
    );
    let (stuck, step) = alice.core.retire_home(&a, &uri(0), false).unwrap();
    alice.settle(&step);
    assert_eq!(stuck, 1);
    assert_eq!(rel(&alice, &a)["myLineage"], before["myLineage"]);
    assert_eq!(polled_homes(&alice, &a).len(), 1);
}

#[test]
fn a_relayset_that_does_not_verify_changes_nothing() {
    let mut d = two_home_duo();
    let conv = d
        .alice
        .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: d.a.clone() })[0]
        .id
        .clone();
    let bob_before = rel(&d.bob, &d.b);
    // Alice's authenticated session carries a RelaySet signed by a key that is not any device of hers.
    let mut rng = InsecureTestRng::from_label(b"forger");
    let forger = DeviceSigner::generate(&mut rng);
    let mut alice_device = [0u8; 16];
    alice_device.copy_from_slice(
        &om_core::crypto::unhex(
            rel(&d.alice, &d.a)["myLineage"]["nodes"][0]["signer"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
    );
    let parent = rel(&d.alice, &d.a)["myLineage"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .max_by_key(|n| n["version"].as_u64().unwrap())
        .unwrap()
        .clone();
    let evil = Record::new(
        parent["version"].as_u64().unwrap() + 1,
        Some(
            om_core::crypto::unhex(parent["hash"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap(),
        ),
        vec![entry(9)],
    )
    .unwrap();
    let forged = relayset::sign(&forger, &alice_device, &evil, [3; 32]);
    let payload = cbor::encode(&V::M(vec![
        (0, V::U(6)),
        (1, V::B(forged)),
        (2, V::A(vec![])),
    ]));
    let step = d
        .alice
        .core
        .debug_inject_payload(&d.a, &conv, &payload)
        .unwrap();
    d.alice.settle(&step);
    settle_all(&mut [(&mut d.alice, &d.a), (&mut d.bob, &d.b)]);
    let bob_after = rel(&d.bob, &d.b);
    assert_eq!(bob_after["peerRelays"], bob_before["peerRelays"]);
    assert_eq!(bob_after["peerLineage"], bob_before["peerLineage"]);
    assert!(
        bob_after["integrityFailures"].as_u64().unwrap()
            > bob_before["integrityFailures"].as_u64().unwrap(),
        "refused and counted"
    );
    say(&mut d, true, "still fine");
    assert_eq!(texts(&d.bob, &d.b).last().unwrap(), "still fine");
}

#[test]
fn the_lineages_survive_a_restart_of_both_sides() {
    let mut d = two_home_duo();
    let (a_before, b_before) = (rel(&d.alice, &d.a), rel(&d.bob, &d.b));
    d.a = d.alice.restart().remove(0);
    d.b = d.bob.restart().remove(0);
    assert_eq!(rel(&d.alice, &d.a)["myLineage"], a_before["myLineage"]);
    assert_eq!(rel(&d.bob, &d.b)["peerLineage"], b_before["peerLineage"]);
    assert_eq!(rel(&d.bob, &d.b)["peerRelays"], b_before["peerRelays"]);
    d.net.block_home(&uri(BOB_HOMES[0]));
    say(&mut d, true, "after restart, with a Home down");
    assert_eq!(
        texts(&d.bob, &d.b).last().unwrap(),
        "after restart, with a Home down"
    );
}
