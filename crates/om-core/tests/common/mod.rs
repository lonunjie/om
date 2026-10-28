// SPDX-License-Identifier: MPL-2.0
//! Shared helpers for first-contact tests (ADR-0181): a hand-built requester that deposits a MessageRequest for an
//! invite, reads the inviter's Accept, and answers with a Bind it fully controls, so a test can present exactly the
//! intro identity, reply-key lifetime, chain and binding it wants to the inviter's Core.
#![allow(dead_code)]

use om_core::api::*;
use om_core::control;
use om_core::crypto::PROFILE_V1;
use om_core::crypto::binding::Acs;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::{DeviceSigner, digest};
use om_core::crypto::store::Device;
use om_core::crypto::{delivery, hpke, sha256};
use om_core::host::NetCall;
use om_core::intro;
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

/// Create an invite and return `(link bytes, the introduction record the service stores)`.
pub fn invite(rt: &mut Runtime, ctx: &ProfileExecutionContext, op: &str) -> (Vec<u8>, Vec<u8>) {
    rt.dispatch(ctx, op, IntentBody::CreateInvite);
    let record = rt
        .core
        .net_ops(ctx)
        .unwrap()
        .into_iter()
        .find_map(|o| match o.call {
            NetCall::PutInvite { descriptor, .. } => Some(descriptor),
            _ => None,
        })
        .expect("record");
    rt.flush_net(ctx);
    let v: Option<InviteView> = rt.query(Query::GetInvite {
        ctx: ctx.clone(),
        operation_id: op.into(),
    });
    let link = v.unwrap().link;
    (b64_decode(link.trim_start_matches("om:invite/")), record)
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

pub fn intros(alice: &mut Runtime, a: &ProfileExecutionContext) -> Vec<IntroductionRequest> {
    settle_all(&mut [(alice, a)]);
    alice.query(Query::GetIntroductions { ctx: a.clone() })
}

/// The app state of a profile as JSON (replay records, relationship states, limiter counters).
pub fn app_state(rt: &Runtime, ctx: &ProfileExecutionContext) -> serde_json::Value {
    rt.core.debug_app_state(ctx).unwrap()
}

/// A requester with its own account (standalone genesis) that the test drives by hand.
pub struct FakeRequester {
    pub dev: Device,
    pub chain: Vec<Vec<u8>>,
    pub creds: Vec<Vec<u8>>,
    pub intro: DeviceSigner,
    pub invite_id: [u8; 16],
    pub rk_digest: [u8; 32],
    pub inviter_intro_pk: Vec<u8>,
    pub inviter_mailbox: Vec<u8>,
    /// Reply mailbox, bootstrap key (request-scoped) and the established key it will advertise in the Bind.
    pub mailbox: [u8; 16],
    pub boot_seed: [u8; 32],
    pub boot_km: delivery::KeyMaterial,
    pub est_seed: [u8; 32],
    pub est_km: delivery::KeyMaterial,
}

/// What the inviter's Accept carried.
pub struct Accept {
    pub mailbox: Vec<u8>,
    pub record: Vec<u8>,
    pub canon: Vec<u8>,
    pub binding: Vec<u8>,
    pub chain: Vec<Vec<u8>>,
    pub creds: Vec<Vec<u8>>,
}

/// Knobs a test turns to make the Bind wrong in exactly one way.
#[derive(Default)]
pub struct BindOpts {
    pub chain: Option<Vec<Vec<u8>>>,
    pub creds: Option<Vec<Vec<u8>>>,
    pub intro_identity: Option<Vec<u8>>,
    pub request_digest: Option<[u8; 32]>,
}

#[allow(clippy::too_many_arguments)]
impl FakeRequester {
    pub fn new(rng: &mut InsecureTestRng) -> Self {
        let dev = Device::new(rng);
        let genesis = control::sign_acs(&dev.acs().body(), &[(&dev.device_id, &dev.signer)], rng);
        let mailbox = [7u8; 16];
        let (boot_seed, boot_km) = delivery::new_key(&mailbox, 0, rng);
        let (est_seed, est_km) = delivery::new_key(&mailbox, 0, rng);
        FakeRequester {
            creds: vec![dev.credential().body()],
            chain: vec![genesis],
            dev,
            intro: DeviceSigner::generate(rng),
            invite_id: [0; 16],
            rk_digest: [0; 32],
            inviter_intro_pk: vec![],
            inviter_mailbox: vec![],
            mailbox,
            boot_seed,
            boot_km,
            est_seed,
            est_km,
        }
    }

    /// Deposit a hand-built MessageRequest for `link`/`record` whose request-scoped reply key lives `reply_ttl_s`.
    pub fn request(
        &mut self,
        net: &LoopbackNetwork,
        rng: &mut InsecureTestRng,
        link: &[u8],
        record: &[u8],
        now_ms: u64,
        reply_ttl_s: u64,
        label: &str,
    ) {
        let rec = cbor::decode(record).unwrap();
        let inviter_pk = rec.get(3).unwrap().b().unwrap().to_vec();
        let d = intro::open_intro(
            rec.get(1).unwrap().b().unwrap(),
            intro::LBL_DESCRIPTOR_SIG,
            &inviter_pk,
        )
        .expect("descriptor verifies");
        let desc = intro::read_descriptor(&d.body).unwrap();
        let pk = desc.request_key.clone().unwrap();
        self.inviter_intro_pk = inviter_pk;
        self.inviter_mailbox = intro::parse_service_uri(&desc.intro_service)
            .unwrap()
            .to_vec();
        self.invite_id = link[..16].try_into().unwrap();
        self.rk_digest = om_core::request_key_digest(&pk);
        let now_s = now_ms / 1000;
        let (boot_seed, boot_km) = delivery::new_key(&self.mailbox, now_s + reply_ttl_s, rng);
        self.boot_seed = boot_seed;
        self.boot_km = boot_km;
        let (est_seed, est_km) = delivery::new_key(&self.mailbox, now_s + 90 * 24 * 3600, rng);
        self.est_seed = est_seed;
        self.est_km = est_km;
        let sender_descriptor = intro::sign_intro(
            &self.intro,
            intro::LBL_DESCRIPTOR_SIG,
            intro::descriptor_body(
                &[9; 16],
                label,
                intro::INTRO_SERVICE_URI,
                now_s + 3600,
                None,
            ),
            rng,
        );
        let payload = intro::sign_intro(
            &self.intro,
            intro::LBL_PAYLOAD_SIG,
            intro::payload_body(
                &self.invite_id,
                &self.rk_digest,
                &self.intro.public(),
                &sender_descriptor,
                &intro::RouteEntry {
                    relay_uri: delivery::CORE_ROUTE_URI,
                    capability: &self.mailbox,
                    expires_at_s: now_s + reply_ttl_s,
                },
                self.boot_km.to_v(),
            ),
            rng,
        );
        let mut rnd = [0u8; 64];
        rand_core::RngCore::fill_bytes(rng, &mut rnd);
        let (ss, enc) = hpke::encaps_derand(&pk, &rnd).unwrap();
        let info = hpke::message_request_info(PROFILE_V1, &self.invite_id, &self.rk_digest);
        let ct = hpke::key_schedule(&ss, &info).seal(b"", &payload);
        deposit_request(
            net,
            &self.inviter_mailbox,
            &self.invite_id,
            &self.rk_digest,
            enc,
            ct,
        );
    }

    /// The Accept the inviter deposited for this requester's reply route, if any.
    pub fn accept(&self, net: &LoopbackNetwork, now_s: u64) -> Option<Accept> {
        for wrapper in net.mailbox_bytes(&self.mailbox) {
            let Ok(o) = delivery::open(
                &wrapper,
                &self.boot_seed,
                &self.boot_km.key_id,
                delivery::CORE_ROUTE_URI,
                &self.mailbox,
                now_s,
            ) else {
                continue;
            };
            let p = cbor::decode(&o.inner_envelope).unwrap();
            if p.get(0).unwrap().u().unwrap() != 4 {
                continue;
            }
            let list = |k| -> Vec<Vec<u8>> {
                p.get(k)
                    .unwrap()
                    .a()
                    .unwrap()
                    .iter()
                    .map(|x| x.b().unwrap().to_vec())
                    .collect()
            };
            return Some(Accept {
                mailbox: p.get(1).unwrap().b().unwrap().to_vec(),
                record: p.get(2).unwrap().b().unwrap().to_vec(),
                canon: p.get(3).unwrap().b().unwrap().to_vec(),
                binding: p.get(4).unwrap().b().unwrap().to_vec(),
                chain: list(5),
                creds: list(6),
            });
        }
        None
    }

    /// The inner Bind payload (kind 5): our key record, binding, chain and credentials.
    pub fn bind_payload(&self, rng: &mut InsecureTestRng, now_s: u64, o: BindOpts) -> Vec<u8> {
        let head: Acs = control::verify_chain(&self.chain, &self.creds).unwrap();
        let record =
            delivery::sign_key_record(&self.dev.signer, &self.dev.device_id, &self.est_km, [5; 32]);
        let body = intro::binding_body(
            &head.account_id,
            &o.intro_identity.unwrap_or_else(|| self.intro.public()),
            &o.request_digest.unwrap_or(self.rk_digest),
            head.generation,
            &head.hash(),
            now_s,
        );
        let binding = intro::sign_binding(&self.dev.signer, &self.dev.device_id, body, rng_32(rng));
        cbor::encode(&V::M(vec![
            (0, V::U(5)),
            (1, V::B(record)),
            (2, V::B(binding)),
            (
                3,
                V::A(
                    o.chain
                        .unwrap_or_else(|| self.chain.clone())
                        .into_iter()
                        .map(V::B)
                        .collect(),
                ),
            ),
            (
                4,
                V::A(
                    o.creds
                        .unwrap_or_else(|| self.creds.clone())
                        .into_iter()
                        .map(V::B)
                        .collect(),
                ),
            ),
        ]))
    }

    /// Seal `inner` to the inviter's established key from its Accept and deposit it in the inviter's mailbox.
    pub fn deposit_to_inviter(
        &self,
        net: &LoopbackNetwork,
        rng: &mut InsecureTestRng,
        accept: &Accept,
        inner: &[u8],
        now_s: u64,
    ) {
        let rec = cbor::decode(&accept.record).unwrap();
        let body = cbor::decode(rec.get(1).unwrap().b().unwrap()).unwrap();
        let km = delivery::KeyMaterial::from_v(body.get(3).unwrap()).unwrap();
        let wrapper = delivery::seal(
            &km.public_key,
            &km.key_id,
            delivery::CORE_ROUTE_URI,
            &accept.mailbox,
            now_s + 7 * 24 * 3600,
            inner,
            rng,
        )
        .unwrap();
        net.perform(&NetCall::Deposit {
            mailbox: accept.mailbox.clone(),
            bytes: wrapper,
        });
    }
}

fn rng_32(rng: &mut InsecureTestRng) -> [u8; 32] {
    let mut b = [0u8; 32];
    rand_core::RngCore::fill_bytes(rng, &mut b);
    b
}

/// Deposit a W0 MessageRequest object into an inviter's request mailbox.
pub fn deposit_request(
    net: &LoopbackNetwork,
    inbox: &[u8],
    ticket: &[u8; 16],
    rk_digest: &[u8; 32],
    enc: Vec<u8>,
    ct: Vec<u8>,
) {
    let request = cbor::encode(&V::M(vec![
        (0, V::U(19)),
        (1, V::U(1)),
        (2, V::B(ticket.to_vec())),
        (3, digest(rk_digest)),
        (4, V::U(PROFILE_V1)),
        (5, V::B(enc)),
        (6, V::B(ct)),
    ]));
    net.perform(&NetCall::Deposit {
        mailbox: inbox.to_vec(),
        bytes: request,
    });
}

pub fn digest_of(b: &[u8]) -> [u8; 32] {
    sha256(&[b])
}

/// A Core-issued confirmation nonce for `kind` and `target` (M2): dispatch `IssueConfirmation`, read it back.
pub fn confirmation(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    kind: &str,
    target: &str,
) -> ConfirmationToken {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let op = format!("confirm-{}", N.fetch_add(1, Ordering::Relaxed));
    rt.dispatch(
        ctx,
        &op,
        IntentBody::IssueConfirmation {
            intent_kind: kind.into(),
            target: target.into(),
        },
    );
    let v: Option<ConfirmationView> = rt.query(Query::GetConfirmation {
        ctx: ctx.clone(),
        operation_id: op,
    });
    serde_json::json!({ "nonce": v.expect("a nonce").nonce })
}
