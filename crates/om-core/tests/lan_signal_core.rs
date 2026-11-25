// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 4 at the Core boundary: a real accepted relationship between two profiles yields the same LAN
//! rendezvous seed on both sides, so one profile's sealed signal opens on the other and nowhere else. The seed is never
//! visible here: the only handles are seal and open, keyed by conversation.

mod common;

use om_core::api::*;
use om_core::crypto::lan_signal::{Kind, SignalError};
use om_core::host::CoreError;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Pair {
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn active_pair() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
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
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

const SDP: &str = "v=0\r\no=- 7 1 IN IP4 0.0.0.0\r\ns=-\r\n";

#[test]
fn an_accepted_relationship_signals_in_both_directions_and_nowhere_else() {
    let mut p = active_pair();
    let offer = p
        .alice
        .core
        .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [5; 16], SDP)
        .unwrap()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&offer).contains("IN IP4"),
        "sealed"
    );
    let (k, sid, sdp) = p
        .bob
        .core
        .lan_signal_open(&p.b, &p.b_conv, &offer)
        .unwrap()
        .unwrap();
    assert_eq!((k, sid, sdp.as_str()), (Kind::Offer, [5; 16], SDP));
    // The same bytes are a replay on the second open.
    assert_eq!(
        p.bob.core.lan_signal_open(&p.b, &p.b_conv, &offer).unwrap(),
        Err(SignalError::Replay)
    );
    // The answer goes back under the same pair seed.
    let answer = p
        .bob
        .core
        .lan_signal_seal(&p.b, &p.b_conv, Kind::Answer, [5; 16], "answer")
        .unwrap()
        .unwrap();
    assert_eq!(
        p.alice
            .core
            .lan_signal_open(&p.a, &p.a_conv, &answer)
            .unwrap()
            .unwrap()
            .2,
        "answer"
    );
    // An unknown conversation is refused before any crypto.
    assert!(matches!(
        p.alice
            .core
            .lan_signal_seal(&p.a, "no-such-conversation", Kind::Offer, [1; 16], SDP),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn two_seals_use_two_nonces_and_oversized_sdp_is_refused() {
    let mut p = active_pair();
    let one = p
        .alice
        .core
        .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [5; 16], SDP)
        .unwrap()
        .unwrap();
    let two = p
        .alice
        .core
        .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [5; 16], SDP)
        .unwrap()
        .unwrap();
    assert_ne!(one, two, "a fresh nonce for every envelope");
    let big = "x".repeat(om_core::crypto::lan_signal::MAX_SDP + 1);
    assert_eq!(
        p.alice
            .core
            .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [6; 16], &big)
            .unwrap(),
        Err(SignalError::Payload)
    );
}

#[test]
fn blocking_a_person_destroys_the_seed_in_both_directions_of_use() {
    let mut p = active_pair();
    let before = p
        .alice
        .core
        .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [5; 16], SDP)
        .unwrap()
        .unwrap();
    let chat: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv.clone(),
    });
    let confirmation =
        common::confirmation(&mut p.bob, &p.b, "BlockPerson", &chat.summary.person_id);
    p.bob.dispatch(
        &p.b,
        "block",
        IntentBody::BlockPerson {
            person_id: chat.summary.person_id,
            scope: BlockScope::Local,
            confirmation,
        },
    );
    assert_eq!(p.bob.outcome(&p.b, "block"), Some(OutcomeClass::Committed));
    // The blocker can neither open what the blocked person sealed earlier nor seal anything for them.
    assert_eq!(
        p.bob
            .core
            .lan_signal_open(&p.b, &p.b_conv, &before)
            .unwrap(),
        Err(SignalError::NoSeed)
    );
    assert_eq!(
        p.bob
            .core
            .lan_signal_seal(&p.b, &p.b_conv, Kind::Answer, [5; 16], "a")
            .unwrap(),
        Err(SignalError::NoSeed)
    );
    // The other person's seed is untouched (they have not blocked anyone), so nothing else changed for them.
    assert!(
        p.alice
            .core
            .lan_signal_seal(&p.a, &p.a_conv, Kind::Offer, [6; 16], SDP)
            .unwrap()
            .is_ok()
    );
}

#[test]
fn nearby_is_off_until_chosen_advertises_and_matches_only_when_both_turned_it_on() {
    let mut p = active_pair();
    let s = p.alice.core.lan_nearby_state(&p.a, &p.a_conv).unwrap();
    assert!(
        s.available && !s.enabled,
        "off by default, available at the default level"
    );
    assert!(
        p.alice.core.lan_advertise(&p.a).unwrap().is_empty(),
        "nothing is advertised until someone is turned on"
    );
    assert!(
        p.alice
            .core
            .lan_nearby_set(&p.a, &p.a_conv, true)
            .unwrap()
            .enabled
    );
    assert!(
        p.bob
            .core
            .lan_nearby_set(&p.b, &p.b_conv, true)
            .unwrap()
            .enabled
    );
    let tags = p.alice.core.lan_advertise(&p.a).unwrap();
    assert_eq!(tags.len(), 8);
    // Real tags are mixed with decoys, so exactly one of the eight is the real one.
    let hits: Vec<_> = tags
        .iter()
        .filter_map(|t| p.bob.core.lan_observe(&p.b, t).unwrap())
        .collect();
    assert_eq!(hits, vec![p.b_conv.clone()]);
    assert_eq!(p.bob.core.lan_observe(&p.b, &[9u8; 16]).unwrap(), None);
    // Turning it off stops advertising and matching.
    assert!(
        !p.bob
            .core
            .lan_nearby_set(&p.b, &p.b_conv, false)
            .unwrap()
            .enabled
    );
    assert!(
        tags.iter()
            .all(|t| p.bob.core.lan_observe(&p.b, t).unwrap().is_none())
    );
    assert!(p.bob.core.lan_advertise(&p.b).unwrap().is_empty());
}
