// SPDX-License-Identifier: MPL-2.0
//! Mentions in a one-to-one chat (MESSENGER_BASELINE_V1 "Mentions", ADR-0199): typed inline mentions plus the mention
//! set on `message.create`, shown to both people, the set never disagreeing with the text.

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

fn conv_view(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> ConversationView {
    rt.query(Query::GetConversationView {
        ctx: ctx.clone(),
        conversation_id: conv.into(),
    })
}

fn mentioning(p: &mut Pair, op: &str, text: &str, mentions: Vec<MentionInput>) {
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendMessageMentioning {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: text.into(),
                reply_to: None,
            },
            mentions,
        },
    );
    settle(p);
}

fn mentions_of(item: &TimelineItem) -> Vec<MentionView> {
    match item {
        TimelineItem::Incoming { mentions, .. } | TimelineItem::Outgoing { mentions, .. } => {
            mentions.clone()
        }
    }
}

#[test]
fn a_mention_reaches_the_person_named_and_flags_them() {
    let mut p = active_pair();
    let bob_account = conv_view(&p.bob, &p.b, &p.b_conv).own_account.unwrap();
    let text = "hi @Bob, lunch?";
    mentioning(
        &mut p,
        "m1",
        text,
        vec![MentionInput {
            start: 3,
            end: 7,
            account_id: bob_account.clone(),
        }],
    );
    let want = |me| {
        vec![MentionView {
            start: 3,
            end: 7,
            account_id: bob_account.clone(),
            me,
        }]
    };
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert!(matches!(&bob[0], TimelineItem::Incoming { text: t, .. } if t == text));
    assert_eq!(mentions_of(&bob[0]), want(true));
    let alice = timeline(&p.alice, &p.a, &p.a_conv);
    assert!(matches!(&alice[0], TimelineItem::Outgoing { text: t, .. } if t == text));
    assert_eq!(mentions_of(&alice[0]), want(false));
}

#[test]
fn a_mention_of_someone_outside_the_chat_or_a_bad_range_is_refused() {
    let mut p = active_pair();
    let bob_account = conv_view(&p.bob, &p.b, &p.b_conv).own_account.unwrap();
    for (n, m) in [
        MentionInput {
            start: 3,
            end: 7,
            account_id: "ab".repeat(32),
        },
        MentionInput {
            start: 3,
            end: 99,
            account_id: bob_account.clone(),
        },
        MentionInput {
            start: 5,
            end: 5,
            account_id: bob_account.clone(),
        },
        MentionInput {
            start: 3,
            end: 7,
            account_id: "zz".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        mentioning(&mut p, &format!("bad{n}"), "hi @Bob", vec![m]);
    }
    assert!(timeline(&p.alice, &p.a, &p.a_conv).is_empty());
    assert!(timeline(&p.bob, &p.b, &p.b_conv).is_empty());
}

#[test]
fn the_mention_body_is_canonical_w0_and_a_disagreeing_set_shows_plain_text() {
    let mut p = active_pair();
    let bob_account: [u8; 32] = unhex(&conv_view(&p.bob, &p.b, &p.b_conv).own_account.unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let au = alice_as_author(&p);
    let spans = [events::MentionSpan {
        start: 3,
        end: 7,
        account: bob_account,
    }];
    let header = |id: u8, body: V| {
        Header {
            event_id: [id; 16],
            conversation_id: au.canon,
            author_account: au.acct,
            author_device: au.dev,
            clock: Hlc {
                physical_ms: 1_790_000_000_000 + u64::from(id),
                logical: 0,
            },
            kind: events::K_CREATE,
            body,
        }
        .encode()
    };
    let good = events::message_create_body_mentions(Some("hi @Bob"), vec![], None, &spans);
    om_codec::Registry::load()
        .check("ConversationEvent", &header(1, good.clone()), false)
        .expect("a mention is a valid W0 ConversationEvent");
    // The set names an account the text does not mention: the text is kept, the mention is not.
    let good2 = good.clone();
    let mut bad = match good {
        V::M(m) => m,
        _ => unreachable!(),
    };
    for (k, v) in bad.iter_mut() {
        if *k == 3 {
            *v = V::A(vec![V::M(vec![(0, V::U(1)), (1, V::B(vec![9; 32]))])]);
        }
    }
    alice_sends(&mut p, &header(2, V::M(bad)));
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert!(matches!(&bob[0], TimelineItem::Incoming { text, .. } if text == "hi @Bob"));
    assert!(mentions_of(&bob[0]).is_empty());
    alice_sends(&mut p, &header(3, good2));
    let bob = timeline(&p.bob, &p.b, &p.b_conv);
    assert_eq!(bob.len(), 2);
    assert_eq!(mentions_of(&bob[1]).len(), 1);
}
