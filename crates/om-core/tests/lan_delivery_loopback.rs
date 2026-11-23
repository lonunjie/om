// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 3 (delivery half): a 1:1 message travels over an ephemeral `om+lan://v1` route as an ordinary
//! sealed delivery, the LAN route is bound by the unchanged W0 route binding, and a later WAN retry of the very same bytes
//! materializes nothing new (NT-06, ADR-0158 section 8).

mod common;

use om_core::api::*;
use om_core::crypto::unhex;
use om_core::host::NetCall;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

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

#[test]
fn a_message_over_a_lan_route_is_an_ordinary_sealed_delivery_and_a_retry_applies_once() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("lan-alice", &net);
    let mut bob = Runtime::new("lan-bob", &net);
    alice.lan_routes = true;
    bob.lan_routes = true;
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

    // Both sides now hold LAN routes (the relationship's boxes carry om+lan://v1 URIs).
    let state = alice.core.debug_app_state(&a).unwrap();
    let rel = state["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    let uri = rel["myBoxes"][0]["route"]["relayUri"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(uri.starts_with("om+lan://v1/"), "route uri {uri}");

    alice.dispatch(
        &a,
        "m1",
        IntentBody::SendMessage {
            conversation_id: a_conv.clone(),
            draft: Draft {
                text: "over the wifi".into(),
                reply_to: None,
            },
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(texts(&bob, &b, &b_conv), vec!["over the wifi".to_string()]);

    // A WAN retry of the very same sealed bytes (same EventID) must not create a second message.
    let bob_state = bob.core.debug_app_state(&b).unwrap();
    let brel = bob_state["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    let route = &brel["myBoxes"][0]["route"];
    let cap = unhex(route["capability"].as_str().unwrap()).unwrap();
    let to_bob: Vec<Vec<u8>> = net
        .deposit_log()
        .into_iter()
        .filter(|(c, _)| *c == cap || c.ends_with(&cap[cap.len() - 16..]))
        .map(|(_, bytes)| bytes)
        .collect();
    assert!(
        !to_bob.is_empty(),
        "the message was deposited at Bob's LAN route"
    );
    for bytes in to_bob {
        net.perform(&NetCall::Deposit {
            mailbox: cap.clone(),
            relay_uri: Some(route["relayUri"].as_str().unwrap().to_string()),
            bytes,
        });
    }
    bob.poll(&b);
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(
        texts(&bob, &b, &b_conv),
        vec!["over the wifi".to_string()],
        "applied once"
    );
}
