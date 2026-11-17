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
