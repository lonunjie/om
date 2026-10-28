// SPDX-License-Identifier: MPL-2.0
//! B1 (audit 2026-10-28), end to end on the first-contact path of ADR-0181: a requester that presents a forged
//! AccountControlState successor of someone else's chain in its Bind must not become a bound contact. Control: the
//! same Bind carrying the honest chain verifies, so the test does reach the verifier.

mod common;

use common::{BindOpts, FakeRequester, app_state, create_profile, invite};
use om_core::api::*;
use om_core::control;
use om_core::crypto::binding::Acs;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::runtime::{LoopbackNetwork, Runtime};

fn states(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<String> {
    app_state(rt, ctx)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .map(|r| r["state"].as_str().unwrap().to_string())
        .collect()
}

/// Alice accepts a request from `req`; returns Alice's view after her Accept is deposited.
fn accepted(
    net: &LoopbackNetwork,
    alice: &mut Runtime,
    a: &ProfileExecutionContext,
    req: &mut FakeRequester,
    rng: &mut InsecureTestRng,
    op: &str,
) -> common::Accept {
    let now_ms = alice.clock.get();
    let (link, record) = invite(alice, a, op);
    req.request(net, rng, &link, &record, now_ms, 3600, "Bob");
    alice.poll(a);
    let list: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].verification, VerificationCode::Unverified);
    alice.dispatch(
        a,
        &format!("{op}-acc"),
        IntentBody::AcceptIntroduction {
            request_id: list[0].id.clone(),
        },
    );
    alice.flush_net(a);
    req.accept(net, now_ms / 1000).expect("Alice's Accept")
}

#[test]
fn the_inviter_binds_an_honest_chain_and_refuses_a_forged_successor_of_it() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"forger");
    let now_s = alice.clock.get() / 1000;

    // Control: Bob's genuine chain and binding are accepted (the relationship leaves ACCEPTED_UNBOUND).
    let mut bob = FakeRequester::new(&mut rng);
    let acc = accepted(&net, &mut alice, &a, &mut bob, &mut rng, "inv-1");
    assert_eq!(states(&alice, &a), vec!["accepted_unbound"]);
    let bind = bob.bind_payload(&mut rng, now_s, BindOpts::default());
    bob.deposit_to_inviter(&net, &mut rng, &acc, &bind, now_s);
    alice.poll(&a);
    assert_eq!(
        states(&alice, &a),
        vec!["establishing"],
        "honest chain is bound"
    );

    // Forgery: the attacker extends Bob's chain with a successor that swaps Bob's controller credential for its own
    // key and adds its own messaging device, signed with the attacker's key.
    let mut mallory = FakeRequester::new(&mut rng);
    let head = control::verify_chain(&mallory.chain, &mallory.creds).unwrap();
    let victim = *head.control.iter().next().unwrap();
    let mut forged = Device::new(&mut rng);
    forged.device_id = victim;
    forged.account_id = head.account_id;
    let mut attacker = Device::new(&mut rng);
    attacker.account_id = head.account_id;
    let mut child: Acs = head.clone();
    child.generation = head.generation + 1;
    child.previous = Some(head.hash());
    child
        .credentials
        .iter_mut()
        .find(|(d, _)| *d == victim)
        .unwrap()
        .1 = forged.credential().hash();
    child.messaging.insert(attacker.device_id);
    child
        .credentials
        .push((attacker.device_id, attacker.credential().hash()));
    let link_bytes = control::sign_acs(&child.body(), &[(&victim, &forged.signer)], &mut rng);
    let mut chain = mallory.chain.clone();
    chain.push(link_bytes);
    let mut creds = vec![forged.credential().body(), attacker.credential().body()];
    creds.extend(mallory.creds.clone());

    let acc = accepted(&net, &mut alice, &a, &mut mallory, &mut rng, "inv-2");
    let bind = mallory.bind_payload(
        &mut rng,
        now_s,
        BindOpts {
            chain: Some(chain),
            creds: Some(creds),
            ..BindOpts::default()
        },
    );
    mallory.deposit_to_inviter(&net, &mut rng, &acc, &bind, now_s);
    alice.poll(&a);
    let mut s = states(&alice, &a);
    s.sort();
    assert_eq!(
        s,
        vec!["accepted_unbound", "establishing"],
        "a forged successor of the chain must not bind (the second contact stays unbound)"
    );
}
