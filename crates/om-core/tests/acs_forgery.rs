// SPDX-License-Identifier: MPL-2.0
//! B1 (audit 2026-10-28), end to end: a requester that presents a forged AccountControlState successor of someone
//! else's published chain must not become an introduction request on the inviter's side. Control: the same
//! request carrying the honest chain is listed, so the harness does reach the verifier.

mod common;

use common::{Parts, create_profile, intros, invite, parts, send_request};
use om_core::control;
use om_core::crypto::binding::Acs;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::runtime::{LoopbackNetwork, Runtime};

#[test]
fn the_inviter_lists_an_honest_chain_and_refuses_a_forged_successor_of_it() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    // Bob publishes an invite, which carries his whole chain for anyone to read.
    let (_, bob_desc) = invite(&mut bob, &b, "bob-inv");
    let honest = parts(&bob_desc);
    let now_ms = alice.clock.get();
    let mut rng = InsecureTestRng::from_label(b"forger");

    // Control: Bob's genuine material is listed.
    let (link1, desc1) = invite(&mut alice, &a, "inv-1");
    send_request(
        &net,
        &mut rng,
        &link1,
        &desc1,
        Parts {
            acs: honest.acs.clone(),
            creds: honest.creds.clone(),
            chain: honest.chain.clone(),
        },
        now_ms,
        3600,
    );
    assert_eq!(intros(&mut alice, &a).len(), 1, "honest chain is listed");

    // Forgery: the attacker extends Bob's chain with a successor that swaps Bob's controller credential for its
    // own key and adds its own messaging device, signed with the attacker's key.
    let head = control::verify_chain(&honest.chain, &honest.creds).unwrap();
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
    let mut chain = honest.chain.clone();
    chain.push(link_bytes);
    // The attacker lists its own credentials first.
    let mut creds = vec![forged.credential().body(), attacker.credential().body()];
    creds.extend(honest.creds.clone());

    let (link2, desc2) = invite(&mut alice, &a, "inv-2");
    send_request(
        &net,
        &mut rng,
        &link2,
        &desc2,
        Parts {
            acs: child.body(),
            creds,
            chain,
        },
        now_ms,
        3600,
    );
    assert_eq!(
        intros(&mut alice, &a).len(),
        1,
        "a forged successor of Bob's chain must not become a second introduction"
    );
}
