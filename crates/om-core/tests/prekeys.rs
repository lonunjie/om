// SPDX-License-Identifier: MPL-2.0
//! One-time prekey replenishment: an invitee that many inviters fetch from republishes before running out, and the
//! signed last-resort keys remain the (ledgered) fallback only if it truly runs dry.

use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

fn profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "c",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

#[test]
fn an_invitee_that_many_inviters_fetch_from_republishes_one_time_prekeys() {
    let net = LoopbackNetwork::default();
    let mut bob = Runtime::new("bob", &net);
    let b = profile(&mut bob, "pb", "Bob");
    let bob_device = net.key_inventory()[0].0.clone();
    assert_eq!(
        net.key_inventory()[0].1,
        10,
        "ten one-time EC prekeys at first publish"
    );
    let mut lowest = 10;
    for i in 0..8 {
        let mut alice = Runtime::new(&format!("alice{i}"), &net);
        let a = profile(&mut alice, "pa", "Alice");
        alice.dispatch(&a, "inv", IntentBody::CreateInvite);
        alice.flush_net(&a);
        let link = alice
            .query::<Option<InviteView>>(Query::GetInvite {
                ctx: a.clone(),
                operation_id: "inv".into(),
            })
            .unwrap()
            .link;
        bob.dispatch(
            &b,
            &format!("acc{i}"),
            IntentBody::AcceptInvitation { link },
        );
        settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
        let intros: Vec<IntroductionRequest> =
            alice.query(Query::GetIntroductions { ctx: a.clone() });
        alice.dispatch(
            &a,
            "i",
            IntentBody::AcceptIntroduction {
                request_id: intros[0].id.clone(),
            },
        );
        settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
        let (_, ec, pq) = net
            .key_inventory()
            .into_iter()
            .find(|(d, _, _)| *d == bob_device)
            .unwrap();
        lowest = lowest.min(ec).min(pq);
        let active: Vec<ConversationSummary> =
            alice.query(Query::GetConversationList { ctx: a.clone() });
        assert_eq!(active.len(), 1, "contact {i} established");
    }
    // The provider-side inventory dipped but was topped up before it ran out.
    let (_, ec, pq) = net
        .key_inventory()
        .into_iter()
        .find(|(d, _, _)| *d == bob_device)
        .unwrap();
    assert!(lowest >= 3, "never emptied (lowest {lowest})");
    assert!(ec >= 4 && pq >= 4, "replenished (ec {ec}, pq {pq})");
}
