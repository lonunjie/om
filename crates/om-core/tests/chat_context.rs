// SPDX-License-Identifier: MPL-2.0
//! Everyday chat context between two people (MESSENGER_BASELINE_V1, EVENT_MODEL `message.create`): a reply names a
//! message of the same conversation by EventID, the quoted text is derived locally and never sent, and a reply to a
//! message that is not there shows as unavailable.

mod common;

use om_core::api::*;
use om_core::crypto::cbor::V;
use om_core::crypto::unhex;
use om_core::events::{self, Header, Hlc};
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

struct Pair {
    #[allow(dead_code)]
    net: LoopbackNetwork,
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
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

fn settle(p: &mut Pair) {
    settle_all(&mut [(&mut p.alice, &p.a), (&mut p.bob, &p.b)]);
}

fn timeline(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<TimelineItem> {
    let v: ConversationView = rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    });
    v.timeline
}

fn send(p: &mut Pair, from_alice: bool, op: &str, text: &str, reply_to: Option<&str>) -> MessageId {
    let draft = Draft {
        text: text.into(),
        reply_to: reply_to.map(str::to_string),
    };
    if from_alice {
        p.alice.dispatch(
            &p.a,
            op,
            IntentBody::SendMessage {
                conversation_id: p.a_conv.clone(),
                draft,
            },
        );
    } else {
        p.bob.dispatch(
            &p.b,
            op,
            IntentBody::SendMessage {
                conversation_id: p.b_conv.clone(),
                draft,
            },
        );
    }
    let (rt, ctx) = if from_alice {
        (&p.alice, &p.a)
    } else {
        (&p.bob, &p.b)
    };
    let id = rt
        .core
        .debug_app_state(ctx)
        .unwrap()
        .get("ops")
        .and_then(|o| o.get(op))
        .and_then(|o| o.get("messageId"))
        .and_then(|m| m.as_str())
        .map(str::to_string);
    settle(p);
    id.unwrap_or_default()
}

fn id_of(items: &[TimelineItem], text: &str) -> MessageId {
    items
        .iter()
        .find_map(|i| match i {
            TimelineItem::Outgoing {
                message_id,
                text: t,
                ..
            }
            | TimelineItem::Incoming {
                message_id,
                text: t,
                ..
            } if t == text => Some(message_id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no message {text}"))
}

fn reply_of(items: &[TimelineItem], text: &str) -> Option<ReplyView> {
    items.iter().find_map(|i| match i {
        TimelineItem::Outgoing { text: t, reply, .. }
        | TimelineItem::Incoming { text: t, reply, .. }
            if t == text =>
        {
            reply.clone()
        }
        _ => None,
    })
}

#[test]
fn a_reply_quotes_the_original_for_both_people() {
    let mut p = active_pair();
    send(&mut p, true, "m1", "dinner at 7?", None);
    let orig = id_of(&timeline(&p.bob, &p.b, &p.b_conv), "dinner at 7?");
    send(&mut p, false, "m2", "sounds good", Some(&orig));

    // Bob's own view: his reply quotes Alice's message (not his).
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    let r = reply_of(&bob, "sounds good").expect("bob sees the quote");
    assert!(r.available && !r.outgoing);
    assert_eq!(r.preview, "dinner at 7?");
    assert_eq!(r.message_id, orig);

    // Alice received it: the quote resolves to her own message.
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    let r = reply_of(&alice, "sounds good").expect("alice sees the quote");
    assert!(r.available && r.outgoing);
    assert_eq!(r.preview, "dinner at 7?");
    assert_eq!(r.message_id, id_of(&alice, "dinner at 7?"));
    // The original itself carries no reply.
    assert!(reply_of(&alice, "dinner at 7?").is_none());
}

#[test]
fn the_reply_event_carries_the_event_id_and_never_the_quoted_text() {
    let mut p = active_pair();
    send(&mut p, true, "m1", "a secret plan", None);
    let orig = id_of(&timeline(&p.bob, &p.b, &p.b_conv), "a secret plan");
    send(&mut p, false, "m2", "ok", Some(&orig));
    let st = common::app_state(&p.bob, &p.b);
    let rel = st["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    let msgs = rel["messages"].as_array().unwrap();
    let reply = msgs.iter().find(|m| m["text"] == "ok").unwrap();
    assert_eq!(reply["social"]["replyTo"], orig.trim_start_matches("msg-"));
    // Only the reply's own text is in the reply record.
    assert!(!reply.to_string().contains("secret"));
}

#[test]
fn replying_to_something_that_is_not_in_the_conversation_is_refused() {
    let mut p = active_pair();
    send(&mut p, true, "m1", "hello", None);
    let bogus = format!("msg-{}", "ab".repeat(16));
    send(&mut p, false, "m2", "nope", Some(&bogus));
    assert_eq!(
        p.bob.outcome(&p.b, "m2"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert_eq!(bob.len(), 1, "nothing was sent or shown");
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    assert_eq!(alice.len(), 1);
}

struct Author {
    canon: [u8; 32],
    acct: [u8; 32],
    dev: [u8; 16],
}

/// The identifiers of Alice as Bob knows her: what a sender that builds its own events must use.
fn alice_as_author(p: &Pair) -> Author {
    let st = common::app_state(&p.bob, &p.b);
    let r = st["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    let h = |k: &str| unhex(r[k].as_str().unwrap()).unwrap();
    Author {
        canon: h("canonId").try_into().unwrap(),
        acct: h("peerAccountId").try_into().unwrap(),
        dev: h("peerDeviceId").try_into().unwrap(),
    }
}

fn alice_sends(p: &mut Pair, payload: &[u8]) {
    let step = p
        .alice
        .core
        .debug_inject_payload(&p.a, &p.a_conv, payload)
        .unwrap();
    p.alice.settle(&step);
    settle(p);
}

#[test]
fn a_reply_to_a_message_that_never_arrived_shows_as_unavailable() {
    let mut p = active_pair();
    let au = alice_as_author(&p);
    let ev = Header {
        event_id: [9; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: 1_790_000_000_000,
            logical: 0,
        },
        kind: events::K_CREATE,
        body: events::message_create_body_reply(Some("re: ?"), vec![], Some([7; 16])),
    }
    .encode();
    alice_sends(&mut p, &ev);
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    let r = reply_of(&bob, "re: ?").expect("the message is shown with a placeholder quote");
    assert!(!r.available);
    assert!(r.preview.is_empty());
    assert_eq!(r.message_id, format!("msg-{}", "07".repeat(16)));
}

#[test]
fn a_reply_to_itself_is_not_a_reply() {
    let mut p = active_pair();
    let au = alice_as_author(&p);
    let ev = Header {
        event_id: [9; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: 1_790_000_000_000,
            logical: 0,
        },
        kind: events::K_CREATE,
        body: events::message_create_body_reply(Some("loop"), vec![], Some([9; 16])),
    }
    .encode();
    alice_sends(&mut p, &ev);
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert!(reply_of(&bob, "loop").is_none());
}

#[test]
fn the_reply_body_is_canonical_w0() {
    let body: V = events::message_create_body_reply(Some("x"), vec![], Some([1; 16]));
    let bytes = Header {
        event_id: [2; 16],
        conversation_id: [3; 32],
        author_account: [4; 32],
        author_device: [5; 16],
        clock: Hlc {
            physical_ms: 1_790_000_000_000,
            logical: 0,
        },
        kind: events::K_CREATE,
        body,
    }
    .encode();
    om_codec::Registry::load()
        .check("ConversationEvent", &bytes, false)
        .expect("a reply is a valid W0 ConversationEvent");
}

// ---------------------------------------------------------------------------------------------- reactions

fn react(p: &mut Pair, from_alice: bool, op: &str, msg: &str, key: &str, present: bool) {
    let intent = |conv: &str| IntentBody::SetReaction {
        conversation_id: conv.into(),
        message_id: msg.into(),
        reaction: key.into(),
        present,
    };
    if from_alice {
        let c = p.a_conv.clone();
        p.alice.dispatch(&p.a, op, intent(&c));
    } else {
        let c = p.b_conv.clone();
        p.bob.dispatch(&p.b, op, intent(&c));
    }
    settle(p);
}

fn reactions_of(items: &[TimelineItem], text: &str) -> Vec<ReactionView> {
    items
        .iter()
        .find_map(|i| match i {
            TimelineItem::Outgoing {
                text: t, reactions, ..
            }
            | TimelineItem::Incoming {
                text: t, reactions, ..
            } if t == text => Some(reactions.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no message {text}"))
}

fn rv(key: &str, count: u32, mine: bool) -> ReactionView {
    ReactionView {
        key: key.into(),
        count,
        mine,
    }
}

#[test]
fn a_reaction_reaches_the_other_person_and_can_be_replaced_or_removed() {
    let mut p = active_pair();
    send(&mut p, true, "m1", "big news", None);
    let on_bob = id_of(&timeline(&p.bob, &p.b, &p.b_conv), "big news");
    react(&mut p, false, "r1", &on_bob, "\u{1F44D}", true);
    assert_eq!(
        p.bob.outcome(&p.b, "r1"),
        Some(OutcomeClass::Committed),
        "accepted by the delivery service"
    );

    // Bob sees his own; Alice sees one from someone else on her message.
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert_eq!(
        reactions_of(&bob, "big news"),
        vec![rv("\u{1F44D}", 1, true)]
    );
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    assert_eq!(
        reactions_of(&alice, "big news"),
        vec![rv("\u{1F44D}", 1, false)]
    );
    // The reaction is not a message: nothing was added to either timeline.
    assert_eq!((bob.len(), alice.len()), (1, 1));

    // Alice reacts the same way: one key, two people.
    let on_alice = id_of(&alice, "big news");
    react(&mut p, true, "r2", &on_alice, "\u{1F44D}", true);
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    assert_eq!(
        reactions_of(&alice, "big news"),
        vec![rv("\u{1F44D}", 2, true)]
    );
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert_eq!(
        reactions_of(&bob, "big news"),
        vec![rv("\u{1F44D}", 2, true)]
    );

    // Bob replaces his thumbs-up by a heart (remove, then add): the key is the register.
    react(&mut p, false, "r3", &on_bob, "\u{1F44D}", false);
    react(&mut p, false, "r4", &on_bob, "\u{2764}\u{FE0F}", true);
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    assert_eq!(
        reactions_of(&alice, "big news"),
        vec![rv("\u{1F44D}", 1, true), rv("\u{2764}\u{FE0F}", 1, false)]
    );

    // Removing the last one of a key drops it from the view.
    react(&mut p, true, "r5", &on_alice, "\u{1F44D}", false);
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert_eq!(
        reactions_of(&bob, "big news"),
        vec![rv("\u{2764}\u{FE0F}", 1, true)]
    );
}

#[test]
fn a_reaction_to_something_that_is_not_there_is_refused_and_a_bad_key_is_invalid() {
    let mut p = active_pair();
    send(&mut p, true, "m1", "hi", None);
    let on_bob = id_of(&timeline(&p.bob, &p.b, &p.b_conv), "hi");
    let bogus = format!("msg-{}", "cd".repeat(16));
    react(&mut p, false, "r1", &bogus, "\u{1F44D}", true);
    assert_eq!(
        p.bob.outcome(&p.b, "r1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::NotFound
        })
    );
    for (i, k) in ["", "  ", &"x".repeat(65), "a\u{0007}b"].iter().enumerate() {
        react(&mut p, false, &format!("bad{i}"), &on_bob, k, true);
        assert_eq!(
            p.bob.outcome(&p.b, &format!("bad{i}")),
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Invalid
            }),
            "{k:?}"
        );
    }
}

fn reaction_event(
    au: &Author,
    id: u8,
    ms: u64,
    target: [u8; 16],
    key: &str,
    present: bool,
) -> Vec<u8> {
    Header {
        event_id: [id; 16],
        conversation_id: au.canon,
        author_account: au.acct,
        author_device: au.dev,
        clock: Hlc {
            physical_ms: ms,
            logical: 0,
        },
        kind: events::K_REACTION,
        body: V::M(vec![
            (0, V::B(target.to_vec())),
            (1, V::T(key.into())),
            (2, V::Bool(present)),
        ]),
    }
    .encode()
}

#[test]
fn reordered_and_repeated_reactions_converge_and_a_dangling_one_changes_nothing() {
    let mut p = active_pair();
    let au = alice_as_author(&p);
    send(&mut p, false, "m1", "from bob", None);
    let on_alice = id_of(&timeline(&p.alice, &p.a, &p.a_conv), "from bob");
    let target: [u8; 16] = unhex(on_alice.trim_start_matches("msg-"))
        .unwrap()
        .try_into()
        .unwrap();
    // Alice's remove (later clock) arrives before her add (earlier clock): the later one wins, the add cannot revive it.
    alice_sends(
        &mut p,
        &reaction_event(&au, 21, 1_790_000_000_500, target, "\u{1F389}", false),
    );
    alice_sends(
        &mut p,
        &reaction_event(&au, 22, 1_790_000_000_100, target, "\u{1F389}", true),
    );
    assert!(reactions_of(&timeline(&p.bob, &p.b, &p.b_conv), "from bob").is_empty());
    // A newer add wins again; sending the same event twice changes nothing.
    let again = reaction_event(&au, 23, 1_790_000_000_900, target, "\u{1F389}", true);
    alice_sends(&mut p, &again);
    alice_sends(&mut p, &again);
    assert_eq!(
        reactions_of(&timeline(&p.bob, &p.b, &p.b_conv), "from bob"),
        vec![rv("\u{1F389}", 1, false)]
    );
    // Aimed at a message that does not exist: accepted, no effect, nothing shown.
    alice_sends(
        &mut p,
        &reaction_event(&au, 24, 1_790_000_001_000, [0xee; 16], "\u{1F389}", true),
    );
    assert_eq!(timeline(&p.bob, &p.b, &p.b_conv).len(), 1);
}

#[test]
fn a_person_cannot_make_us_keep_an_unbounded_number_of_reactions() {
    let mut p = active_pair();
    let au = alice_as_author(&p);
    send(&mut p, false, "m1", "from bob", None);
    let on_alice = id_of(&timeline(&p.alice, &p.a, &p.a_conv), "from bob");
    let target: [u8; 16] = unhex(on_alice.trim_start_matches("msg-"))
        .unwrap()
        .try_into()
        .unwrap();
    for i in 0..40u8 {
        let key = format!("k{i}");
        alice_sends(
            &mut p,
            &reaction_event(
                &au,
                30 + i,
                1_790_000_002_000 + u64::from(i),
                target,
                &key,
                true,
            ),
        );
    }
    let shown = reactions_of(&timeline(&p.bob, &p.b, &p.b_conv), "from bob");
    assert_eq!(shown.len(), 16);
}

#[test]
fn a_reaction_event_is_canonical_w0() {
    let ev = reaction_event(
        &Author {
            canon: [1; 32],
            acct: [2; 32],
            dev: [3; 16],
        },
        4,
        1_790_000_000_000,
        [5; 16],
        "\u{1F44D}",
        true,
    );
    om_codec::Registry::load()
        .check("ConversationEvent", &ev, false)
        .expect("a reaction is a valid W0 ConversationEvent");
}
