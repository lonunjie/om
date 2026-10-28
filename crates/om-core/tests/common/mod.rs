// SPDX-License-Identifier: MPL-2.0
//! Shared helpers for first-contact tests: build a MessageRequest by hand and deposit it for an invite, so a test can
//! present exactly the identity material (and reply-key lifetime) it wants to the inviter's Core.
#![allow(dead_code)]

use om_core::api::*;
use om_core::crypto::PROFILE_V1;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::{delivery, hpke, sig};
use om_core::host::NetCall;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

pub fn create_profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
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
pub fn invite(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str) -> (Vec<u8>, Vec<u8>) {
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

pub fn b64_decode(s: &str) -> Vec<u8> {
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

pub struct Parts {
    pub acs: Vec<u8>,
    pub creds: Vec<Vec<u8>>,
    pub chain: Vec<Vec<u8>>,
}

pub fn parts(descriptor: &[u8]) -> Parts {
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
pub fn send_request(
    net: &LoopbackNetwork,
    rng: &mut InsecureTestRng,
    link: &[u8],
    descriptor: &[u8],
    who: Parts,
    now_ms: u64,
    reply_ttl_s: u64,
) {
    let d = cbor::decode(descriptor).unwrap();
    let pk = d.get(3).unwrap().b().unwrap().to_vec();
    let inbox = d.get(4).unwrap().b().unwrap().to_vec();
    let invite16: [u8; 16] = link[..16].try_into().unwrap();
    let rk: [u8; 32] = link[48..80].try_into().unwrap();
    let reply_mailbox = [7u8; 16];
    let (_, reply_key) = delivery::new_key(&reply_mailbox, now_ms / 1000 + reply_ttl_s, rng);
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

pub fn intros(alice: &mut Runtime, a: &ProfileExecutionContext) -> Vec<IntroductionRequest> {
    settle_all(&mut [(alice, a)]);
    alice.query(Query::GetIntroductions { ctx: a.clone() })
}
