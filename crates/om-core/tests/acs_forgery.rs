// SPDX-License-Identifier: MPL-2.0
//! B1 (audit 2026-10-28), end to end: a requester that presents a forged AccountControlState successor of someone
//! else's published chain must not become an introduction request on the inviter's side. Control: the same
//! request carrying the honest chain is listed, so the harness does reach the verifier.

use om_core::api::*;
use om_core::control;
use om_core::crypto::PROFILE_V1;
use om_core::crypto::binding::Acs;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::{delivery, hpke, sig};
use om_core::host::NetCall;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

fn create_profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "create-1",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

/// Create an invite and return `(link bytes, invite descriptor)`.
fn invite(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str) -> (Vec<u8>, Vec<u8>) {
    rt.dispatch(ctx, op, IntentBody::CreateInvite);
    let descriptor = rt
        .core
        .net_ops(ctx)
        .unwrap()
        .into_iter()
        .find_map(|o| match o.call {
            NetCall::PutInvite { descriptor, .. } => Some(descriptor),
            _ => None,
        })
        .expect("descriptor");
    rt.flush_net(ctx);
    let v: Option<InviteView> = rt.query(Query::GetInvite {
        ctx: ctx.clone(),
        operation_id: op.into(),
    });
    let link = v.unwrap().link;
    (
        b64_decode(link.trim_start_matches("om:invite/")),
        descriptor,
    )
}

fn b64_decode(s: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        acc = acc << 6 | alphabet.iter().position(|&a| a == c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    out
}

struct Parts {
    acs: Vec<u8>,
    creds: Vec<Vec<u8>>,
    chain: Vec<Vec<u8>>,
}

fn parts(descriptor: &[u8]) -> Parts {
    let v = cbor::decode(descriptor).unwrap();
    let list = |k| -> Vec<Vec<u8>> {
        v.get(k)
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .map(|x| x.b().unwrap().to_vec())
            .collect()
    };
    Parts {
        acs: v.get(1).unwrap().b().unwrap().to_vec(),
        creds: list(2),
        chain: list(7),
    }
}

/// Deposit a hand-built MessageRequest for `link`/`descriptor` carrying `who`'s identity material.
fn send_request(
    net: &LoopbackNetwork,
    rng: &mut InsecureTestRng,
    link: &[u8],
    descriptor: &[u8],
    who: Parts,
    now_ms: u64,
) {
    let d = cbor::decode(descriptor).unwrap();
    let pk = d.get(3).unwrap().b().unwrap().to_vec();
    let inbox = d.get(4).unwrap().b().unwrap().to_vec();
    let invite16: [u8; 16] = link[..16].try_into().unwrap();
    let rk: [u8; 32] = link[48..80].try_into().unwrap();
    let reply_mailbox = [7u8; 16];
    let (_, reply_key) = delivery::new_key(&reply_mailbox, now_ms / 1000 + 3600, rng);
    let payload = cbor::encode(&V::M(vec![
        (1, V::B(who.acs)),
        (2, V::A(who.creds.into_iter().map(V::B).collect())),
        (3, V::B(reply_mailbox.to_vec())),
        (4, V::T("Bob".into())),
        (5, V::U(now_ms)),
        (6, reply_key.to_v()),
        (7, V::A(who.chain.into_iter().map(V::B).collect())),
    ]));
    let mut rnd = [0u8; 64];
    rand_core::RngCore::fill_bytes(rng, &mut rnd);
    let (ss, enc) = hpke::encaps_derand(&pk, &rnd).unwrap();
    let info = hpke::message_request_info(PROFILE_V1, &invite16, &rk);
    let ct = hpke::key_schedule(&ss, &info).seal(b"", &payload);
    let request = cbor::encode(&V::M(vec![
        (0, V::U(19)),
        (1, V::U(1)),
        (2, V::B(invite16.to_vec())),
        (3, sig::digest(&rk)),
        (4, V::U(PROFILE_V1)),
        (5, V::B(enc)),
        (6, V::B(ct)),
    ]));
    net.perform(&NetCall::Deposit {
        mailbox: inbox,
        bytes: request,
    });
}

fn intros(alice: &mut Runtime, a: &ProfileExecutionContext) -> Vec<IntroductionRequest> {
    settle_all(&mut [(alice, a)]);
    alice.query(Query::GetIntroductions { ctx: a.clone() })
}

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
    );
    assert_eq!(
        intros(&mut alice, &a).len(),
        1,
        "a forged successor of Bob's chain must not become a second introduction"
    );
}
