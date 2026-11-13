// SPDX-License-Identifier: MPL-2.0
//! A world for the history-transfer tests: Alice (the existing device) and Carol have talked; the phone is empty.
#![allow(dead_code)]

use super::confirmation as nonce;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

pub struct World {
    pub net: LoopbackNetwork,
    pub alice: Runtime,
    pub carol: Runtime,
    pub phone: Runtime,
    pub a: ProfileExecutionContext,
    pub c: ProfileExecutionContext,
    pub n: ProfileExecutionContext,
    pub a_conv: String,
    pub c_conv: String,
}

pub fn conv_of(rt: &Runtime, ctx: &ProfileExecutionContext) -> String {
    let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: ctx.clone() });
    assert_eq!(l[0].relationship, RelationshipStateCode::Active);
    l[0].id.clone()
}

pub fn texts(rt: &Runtime, ctx: &ProfileExecutionContext, conv: &str) -> Vec<String> {
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

impl World {
    pub fn new(tag: &str) -> World {
        let net = LoopbackNetwork::default();
        let mut alice = Runtime::new(&format!("alice-{tag}"), &net);
        let mut carol = Runtime::new(&format!("carol-{tag}"), &net);
        let mut phone = Runtime::new(&format!("phone-{tag}"), &net);
        let a = super::create_profile(&mut alice, "p-a", "Alice");
        let c = super::create_profile(&mut carol, "p-c", "Carol");
        let n = phone.open("p-n");
        alice.dispatch(&a, "inv", IntentBody::CreateInvite);
        alice.flush_net(&a);
        let link = alice
            .query::<Option<InviteView>>(Query::GetInvite {
                ctx: a.clone(),
                operation_id: "inv".into(),
            })
            .unwrap()
            .link;
        carol.dispatch(&c, "acc", IntentBody::AcceptInvitation { link });
        settle_all(&mut [(&mut alice, &a), (&mut carol, &c)]);
        let intros: Vec<IntroductionRequest> =
            alice.query(Query::GetIntroductions { ctx: a.clone() });
        alice.dispatch(
            &a,
            "i",
            IntentBody::AcceptIntroduction {
                request_id: intros[0].id.clone(),
            },
        );
        settle_all(&mut [(&mut alice, &a), (&mut carol, &c)]);
        let (a_conv, c_conv) = (conv_of(&alice, &a), conv_of(&carol, &c));
        World {
            net,
            alice,
            carol,
            phone,
            a,
            c,
            n,
            a_conv,
            c_conv,
        }
    }

    pub fn settle(&mut self) {
        for _ in 0..2 {
            settle_all(&mut [
                (&mut self.alice, &self.a),
                (&mut self.carol, &self.c),
                (&mut self.phone, &self.n),
            ]);
        }
    }

    /// Alice and Carol exchange `n` messages each, alternating, with distinct text.
    pub fn talk(&mut self, n: usize) {
        for i in 0..n {
            self.alice.dispatch(
                &self.a,
                &format!("ta{i}"),
                IntentBody::SendMessage {
                    conversation_id: self.a_conv.clone(),
                    draft: Draft {
                        text: format!("alice says {i}"),
                    },
                },
            );
            self.settle();
            self.carol.dispatch(
                &self.c,
                &format!("tc{i}"),
                IntentBody::SendMessage {
                    conversation_id: self.c_conv.clone(),
                    draft: Draft {
                        text: format!("carol says {i}"),
                    },
                },
            );
            self.settle();
        }
    }

    /// Offer, ask and approve; returns the approval outcome.
    pub fn link(&mut self) -> Option<OutcomeClass> {
        self.alice
            .dispatch(&self.a, "offer", IntentBody::OfferDeviceLink);
        self.alice.flush_net(&self.a);
        let offer = self
            .alice
            .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
                ctx: self.a.clone(),
                operation_id: "offer".into(),
            })
            .unwrap();
        self.phone.dispatch(
            &self.n,
            "ask",
            IntentBody::RequestDeviceLink { link: offer.link },
        );
        self.settle();
        let pending: DevicesView = self.alice.query(Query::GetDevices {
            ctx: self.a.clone(),
        });
        let request_id = pending.link_requests[0].request_id.clone();
        let confirmation = nonce(&mut self.alice, &self.a, "ApproveDeviceLink", &request_id);
        self.alice.dispatch(
            &self.a,
            "ok",
            IntentBody::ApproveDeviceLink {
                request_id,
                confirmation,
            },
        );
        let out = self.alice.outcome(&self.a, "ok");
        self.settle();
        out
    }

    pub fn transfer(&self, phone: bool) -> Option<HistoryTransferView> {
        let (rt, ctx) = if phone {
            (&self.phone, &self.n)
        } else {
            (&self.alice, &self.a)
        };
        rt.query(Query::GetHistoryTransfer { ctx: ctx.clone() })
    }
}
