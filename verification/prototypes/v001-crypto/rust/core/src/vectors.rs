// SPDX-License-Identifier: Apache-2.0
//! Deterministic test vectors (insecure seeded RNG) for a second, independent implementation
//! (python/check_vectors.py). The consumer must reproduce or verify:
//!
//! * binding: DeviceCredential / relationship-pinned AccountControlState digests, composite Ed25519 +
//!   ML-DSA-65 key derivation, privacy-preserving bundle signatures, context-bound one-time item
//!   signatures, and the verdict of every negative binding vector;
//! * sessions: X25519 / ML-KEM-768 keys from seeds, PQXDH DH1..DH4/SS/SK/AD, handshake_id, every Double
//!   Ratchet message key, header hash, AEAD key/nonce/AD and AES-256-GCM-SIV ciphertexts;
//! * hpke: the OM MessageRequest envelope (info construction, enc, ciphertext).

use crate::binding::{self, Acs, Credential, RelationshipContext};
use crate::cbor::{self, V};
use crate::hex;
use crate::hpke;
use crate::kdf;
use crate::pqxdh;
use crate::rng::insecure_test_rng::InsecureTestRng;
use crate::rng::{arr32, RngCore};
use crate::sig::{self, ds_hash, ds_sig, ROLE_DEVICE};
use crate::store::{Device, Provider, NOW};
use crate::PROFILE_V1;
use serde_json::{json, Value};

enum Ev {
    Send(bool, &'static str), // (from Alice?, label)
    Recv(bool, &'static str), // (to Alice?, label)
}

fn hexes(v: &[Vec<u8>]) -> Vec<String> {
    v.iter().map(|b| hex(b)).collect()
}

fn case(name: &str, with_one_time: bool, schedule: &[Ev]) -> Value {
    let mut rng = InsecureTestRng::from_label(format!("om-v001-vectors/{name}").as_bytes());
    let mut bob = Device::new(&mut rng);
    let mut alice = Device::new(&mut rng);
    bob.trust_device(&alice).unwrap();
    alice.trust_device(&bob).unwrap();
    let mut prov = Provider::default();
    let up = if with_one_time { bob.generate_one_time(1, 1, &mut rng) } else { crate::store::Upload { bundle: bob.signed_bundle(&mut rng), ..Default::default() } };
    prov.publish(&bob, up);
    let (acs_b, cred_b) = prov.view(&bob.device_id);
    let fetch = prov.fetch(&bob.device_id).unwrap();
    let (vb, tr) = alice.start_session(&fetch, &Device::peer_view(&acs_b, &cred_b), &mut rng).unwrap();
    let a_sess = alice.sessions.values().next().unwrap().clone();
    let init = a_sess.pending_init.clone().unwrap();
    let pq_seed = if vb.pq_one_time { bob.pqopks[&vb.pq_id] } else { bob.last_resort.seed };

    let bob_json = json!({
        "device_id": hex(&bob.device_id), "epoch": bob.epoch,
        "ik_secret": hex(&bob.ik_secret), "ik_pub": hex(&bob.ik()),
        "spk_id": bob.spk.id, "spk_secret": hex(&bob.spk.secret), "spk_pub": hex(&vb.spk),
        "pq_id": vb.pq_id, "pq_kind": if vb.pq_one_time { "one_time" } else { "last_resort" }, "pq_seed_d_z": hex(&pq_seed),
        "pq_pub_sha256": hex(&crate::sha256(&[&vb.pq])),
        "opk_id": vb.opk.map(|(i, _)| i), "opk_secret": vb.opk.map(|(i, _)| hex(&bob.opks[&i])),
        "opk_pub": vb.opk.map(|(_, k)| hex(&k)),
        "acs_body": hex(&acs_b), "credential_body": hex(&cred_b), "fetch": hexes(&fetch),
    });
    let alice_json = json!({
        "ik_secret": hex(&alice.ik_secret), "ik_pub": hex(&alice.ik()),
        "ek_secret": hex(&tr.ek_secret), "ek_pub": hex(&pqxdh::x25519_pub(&tr.ek_secret)),
        "kem_encaps_m": hex(&tr.kem_m),
        "ratchet_init_secret": hex(&a_sess.dr.ec.dhs_secret),
    });
    let ledgered = !vb.pq_one_time && vb.opk.is_none();
    let handshake = json!({
        "ct": hex(&init.ct),
        "ss": hex(&tr.ss),
        "dh": tr.dh.iter().map(|d| hex(d)).collect::<Vec<_>>(),
        "sk": hex(&tr.sk), "ad": hex(&tr.ad),
        "init_header_cbor": hex(&cbor::encode(&init.to_cbor())),
        "uses_last_resort_ledger": ledgered,
        "handshake_id": hex(&pqxdh::handshake_id(&bob.device_id, bob.epoch, &init)),
    });

    let bob_ik = bob.ik();
    let alice_ik = alice.ik();
    let mut queue: Vec<(&str, Vec<u8>)> = vec![];
    let mut events = vec![];
    for ev in schedule {
        match ev {
            Ev::Send(from_a, label) => {
                let pt = format!("vector message {label}").into_bytes();
                let (s, peer) = if *from_a { (&mut alice, bob_ik) } else { (&mut bob, alice_ik) };
                let (env, k) = s.encrypt(&peer, &pt).unwrap();
                let v = cbor::decode(&env).unwrap();
                let a = v.a().unwrap();
                events.push(json!({
                    "type": "send", "from": if *from_a { "A" } else { "B" }, "label": label,
                    "plaintext": hex(&pt), "envelope_context": hex(&cbor::encode(&V::A(vec![a[0].clone(), a[1].clone()]))),
                    "header": hex(a[2].b().unwrap()), "ciphertext": hex(a[3].b().unwrap()),
                    "has_init_prefix": !a[1].is_null(),
                    "mk": hex(&k.mk), "header_hash": hex(&k.header_hash), "aead_key": hex(&k.key), "aead_nonce": hex(&k.nonce), "aad": hex(&k.aad),
                }));
                queue.push((label, env));
            }
            Ev::Recv(to_a, label) => {
                let i = queue.iter().position(|(l, _)| l == label).unwrap();
                let (_, env) = queue.remove(i);
                let route = if *to_a { bob.account_id } else { alice.account_id };
                let r = if *to_a { &mut alice } else { &mut bob };
                let before = r.sessions.values().next().map(|s| s.dr.ec.dhs_secret);
                let (_, pt, k) = r.decrypt(&RelationshipContext::new(route), &env, &mut rng).unwrap();
                let after = r.sessions.values().next().unwrap().dr.ec.dhs_secret;
                events.push(json!({
                    "type": "recv", "to": if *to_a { "A" } else { "B" }, "label": label,
                    "plaintext": hex(&pt), "mk": hex(&k.mk),
                    "new_ratchet_secret": if before == Some(after) { Value::Null } else { json!(hex(&after)) },
                }));
            }
        }
    }
    json!({ "name": name, "bob": bob_json, "alice": alice_json, "handshake": handshake, "events": events })
}

fn with_key(body: &[u8], key: u64, val: Option<V>) -> Vec<u8> {
    let mut v = cbor::decode(body).unwrap();
    if let V::M(ref mut m) = v {
        m.retain(|(k, _)| *k != key);
        if let Some(x) = val {
            m.push((key, x));
        }
    }
    cbor::encode(&v)
}

/// Initiator-binding vectors (ADR-0137): trusted relationship peers, an inbound IK_A
/// and a mandatory relationship route; the responder must bind IK_A to exactly one active committed credential of that account.
fn initiator_binding_vectors() -> Value {
    let mut rng = InsecureTestRng::from_label(b"om-v001-vectors/initiator-binding");
    let alice = Device::new(&mut rng);
    let carol = Device::new(&mut rng);
    let mallory = Device::new(&mut rng);
    let peer = |acs: &Acs, creds: Vec<Vec<u8>>| (acs.body(), ds_hash(binding::LBL_ACS_HASH, &acs.body()), creds);
    let pa = peer(&alice.acs(), vec![alice.credential().body()]);
    let pc = peer(&carol.acs(), vec![carol.credential().body()]);
    let mut rev = alice.acs();
    rev.generation += 1;
    rev.control.remove(&alice.device_id);
    rev.revoked.insert(alice.device_id);
    let mut qua = alice.acs();
    qua.generation += 1;
    qua.quarantined.insert(alice.device_id);
    let mut a2 = alice.clone();
    a2.ik_secret = arr32(&mut rng);
    a2.acs_generation = 2;
    let mut forged = alice.credential();
    forged.ik = mallory.ik();
    let mut cc = carol.credential();
    cc.ik = alice.ik();
    let mut cacs = carol.acs();
    cacs.credentials = vec![(carol.device_id, cc.hash())];
    let mut untrusted = pa.clone();
    untrusted.1 = [0u8; 32];
    type P = (Vec<u8>, [u8; 32], Vec<Vec<u8>>);
    let cases: Vec<(&str, Vec<P>, [u8; 32], [u8; 32], bool)> = vec![
        ("bound_to_trusted_peer", vec![pa.clone()], alice.ik(), alice.account_id, true),
        ("unknown_identity_key", vec![pc.clone()], alice.ik(), carol.account_id, false),
        ("route_of_another_relationship", vec![pa.clone(), pc.clone()], alice.ik(), carol.account_id, false),
        ("route_of_the_relationship", vec![pa.clone(), pc.clone()], alice.ik(), alice.account_id, true),
        ("revoked_device", vec![peer(&rev, vec![alice.credential().body()])], alice.ik(), alice.account_id, false),
        ("quarantined_device", vec![peer(&qua, vec![alice.credential().body()])], alice.ik(), alice.account_id, false),
        ("stale_credential_after_rotation", vec![peer(&a2.acs(), vec![a2.credential().body(), alice.credential().body()])], alice.ik(), alice.account_id, false),
        ("substituted_identity_key_not_committed", vec![peer(&alice.acs(), vec![alice.credential().body(), forged.body()])], mallory.ik(), alice.account_id, false),
        ("same_identity_key_other_relationship_route", vec![pa.clone(), peer(&cacs, vec![cc.body()])], alice.ik(), carol.account_id, true),
        ("same_identity_key_correct_relationship_route", vec![pa.clone(), peer(&cacs, vec![cc.body()])], alice.ik(), alice.account_id, true),
        ("peer_acs_not_the_trusted_head", vec![untrusted], alice.ik(), alice.account_id, false),
    ];
    let mut out = vec![];
    for (name, peers, ik, route, expect) in cases {
        let mut devs = vec![];
        let mut err = None;
        for (a, h, c) in &peers {
            match binding::peer_devices(&binding::TrustedPeer { acs_body: a.clone(), credential_bodies: c.clone() }, h) {
                Ok(d) => devs.extend(d),
                Err(e) => err = Some(e),
            }
        }
        let r = match err {
            Some(e) => Err(e),
            None => binding::verify_initiator(&ik, &devs, &RelationshipContext::new(route)),
        };
        assert_eq!(r.is_ok(), expect, "initiator binding vector {name}: {r:?}");
        out.push(json!({
            "name": name, "ik_a": hex(&ik), "route_account": hex(&route),
            "peers": peers.iter().map(|(a, h, c)| json!({"acs_body": hex(a), "trusted_acs_hash": hex(h), "credential_bodies": hexes(c)})).collect::<Vec<_>>(),
            "expect": if expect { "accept" } else { "reject" },
            "rust_result": match &r { Ok((a, d)) => json!({"account_id": hex(a), "device_id": hex(d)}), Err(e) => json!(e.name()) },
        }));
    }
    json!(out)
}

/// Binding vectors: one positive fetch and named negatives, each with the inputs a verifier needs.
fn binding_vectors() -> Value {
    let mut rng = InsecureTestRng::from_label(b"om-v001-vectors/binding");
    let mut bob = Device::new(&mut rng);
    let evil = Device::new(&mut rng);
    let did = bob.device_id;
    let mut prov = Provider::default();
    let up = bob.generate_one_time(1, 1, &mut rng);
    prov.publish(&bob, up);
    let (acs_b, cred_b) = prov.view(&did);
    let fetch = prov.fetch(&did).unwrap();
    let trusted = ds_hash(binding::LBL_ACS_HASH, &acs_b);

    let v = cbor::decode(&fetch[0]).unwrap();
    let body = v.get(1).unwrap().b().unwrap().to_vec();
    let s = v.get(2).unwrap().a().unwrap()[0].get(3).unwrap().b().unwrap().to_vec();
    let msg = ds_sig(binding::LBL_BUNDLE_SIG, PROFILE_V1, ROLE_DEVICE, &did, &body);
    let es = evil.signer.sign(&msg, &mut rng);
    let entry = |sb: Vec<u8>| sig::signature_entry(&did, PROFILE_V1, ROLE_DEVICE, sb);
    let env = |b: &[u8], sigs: Vec<V>| sig::envelope(b.to_vec(), sigs);
    let rest = |first: Vec<u8>| {
        let mut f = vec![first];
        f.extend_from_slice(&fetch[1..]);
        f
    };
    let cat = |a: &[u8], b: &[u8]| [a, b].concat();

    let mut neg: Vec<(String, Vec<Vec<u8>>, Vec<u8>, Vec<u8>, [u8; 32])> = vec![];
    let mut push = |n: &str, f: Vec<Vec<u8>>, a: Vec<u8>, c: Vec<u8>, t: [u8; 32]| neg.push((n.into(), f, a, c, t));
    push("ed25519_half_from_other_key", rest(env(&body, vec![entry(cat(&es[..64], &s[64..]))])), acs_b.clone(), cred_b.clone(), trusted);
    push("mldsa65_half_from_other_key", rest(env(&body, vec![entry(cat(&s[..64], &es[64..]))])), acs_b.clone(), cred_b.clone(), trusted);
    push("ed25519_only", rest(env(&body, vec![entry(s[..64].to_vec())])), acs_b.clone(), cred_b.clone(), trusted);
    push("mldsa65_only", rest(env(&body, vec![entry(s[64..].to_vec())])), acs_b.clone(), cred_b.clone(), trusted);
    push("split_into_two_entries", rest(env(&body, vec![entry(s[..64].to_vec()), entry(s[64..].to_vec())])), acs_b.clone(), cred_b.clone(), trusted);
    let evil_pq = pqxdh::mlkem_pub(&evil.last_resort.seed);
    let b8 = with_key(&body, 8, Some(V::M(vec![(0, V::U(bob.last_resort.id as u64)), (1, V::U(2)), (2, V::B(evil_pq.clone()))])));
    push("pq_signed_prekey_substituted", rest(env(&b8, vec![entry(s.clone())])), acs_b.clone(), cred_b.clone(), trusted);
    let b7 = with_key(&body, 7, Some(V::M(vec![(0, V::U(bob.spk.id as u64)), (1, V::U(1)), (2, V::B(evil.ik().to_vec()))])));
    push("ec_signed_prekey_substituted", rest(env(&b7, vec![entry(s.clone())])), acs_b.clone(), cred_b.clone(), trusted);
    let mut item = cbor::decode(&fetch[2]).unwrap();
    if let V::M(ref mut m) = item {
        m.retain(|(k, _)| *k != 2);
        m.push((2, V::B(evil_pq)));
    }
    push("pq_one_time_item_substituted", vec![fetch[0].clone(), fetch[1].clone(), cbor::encode(&item)], acs_b.clone(), cred_b.clone(), trusted);
    let nopq = binding::sign_bundle(&bob.signer, &did, with_key(&body, 8, None), arr32(&mut rng));
    push("validly_signed_bundle_without_pq_signed_prekey", vec![nopq], acs_b.clone(), cred_b.clone(), trusted);
    let mut c = Credential::from_body(&cred_b).unwrap();
    c.ik = evil.ik();
    push("identity_key_substituted_in_credential", fetch.clone(), acs_b.clone(), c.body(), trusted);
    let mut q = Acs::from_body(&acs_b).unwrap();
    q.quarantined.insert(did);
    let qb = q.body();
    push("device_quarantined_at_trusted_head", fetch.clone(), qb.clone(), cred_b.clone(), ds_hash(binding::LBL_ACS_HASH, &qb));
    // Keys 3/4 were retired before first release by ADR-0140. Even a device-signed bundle carrying
    // them is rejected so providers cannot reintroduce account-wide ACS linkage.
    let retired3 = binding::sign_bundle(&bob.signer, &did, with_key(&body, 3, Some(V::U(1))), arr32(&mut rng));
    push("retired_provider_visible_acs_generation_present", rest(retired3), acs_b.clone(), cred_b.clone(), trusted);
    let retired4 = binding::sign_bundle(&bob.signer, &did, with_key(&body, 4, Some(sig::digest(&trusted))), arr32(&mut rng));
    push("retired_provider_visible_acs_state_hash_present", rest(retired4), acs_b.clone(), cred_b.clone(), trusted);

    // A one-time item from pool A cannot be transplanted under a valid pool-B bundle.
    let pool_a = [0x31; 16];
    let pool_b = [0x32; 16];
    let mut pa = bob.generate_one_time_for_pool(pool_a, 0, 1, &mut rng);
    let pb = bob.generate_one_time_for_pool(pool_b, 0, 0, &mut rng);
    push("one_time_prekey_transplanted_across_provider_pool", vec![pb.bundle, pa.pqopks.remove(0)], acs_b.clone(), cred_b.clone(), trusted);

    let mut negatives = vec![];
    for (name, f, a, c, t) in &neg {
        let view = binding::PeerView { acs_body: a, trusted_acs_hash: *t, credential_body: c, now: NOW };
        let r = binding::verify_fetch(f, &view);
        assert!(r.is_err(), "negative binding vector {name} was accepted");
        negatives.push(json!({
            "name": name, "fetch": hexes(f), "acs_body": hex(a), "credential_body": hex(c), "trusted_acs_hash": hex(t),
            "rust_error": r.err().unwrap().name(),
        }));
    }
    let cred = Credential::from_body(&cred_b).unwrap();
    json!({
        "now": NOW,
        "signer": { "ed25519_seed": hex(&bob.signer.ed_seed), "mldsa65_seed": hex(&bob.signer.mldsa_seed),
                    "composite_public_key_sha256": hex(&crate::sha256(&[&cred.signing_pk])) },
        "account_id": hex(&bob.account_id), "device_id": hex(&did), "ik_pub": hex(&bob.ik()),
        "acs_body": hex(&acs_b), "acs_hash": hex(&trusted), "credential_body": hex(&cred_b), "credential_hash": hex(&cred.hash()),
        "positive_fetch": hexes(&fetch),
        "negatives": negatives,
    })
}

fn hpke_vector() -> Value {
    let mut rng = InsecureTestRng::from_label(b"om-v001-vectors/hpke");
    let seed = arr32(&mut rng);
    let pk = hpke::public_key(&seed);
    let mut ticket = [0u8; 16];
    rng.fill_bytes(&mut ticket);
    let request_key = cbor::encode(&V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(pk.clone()))]));
    let rk_digest = ds_hash("OM-REQUEST-KEY-HASH-v1", &request_key);
    let info = hpke::message_request_info(PROFILE_V1, &ticket, &rk_digest);
    let mut rnd = [0u8; 64];
    rng.fill_bytes(&mut rnd);
    let pt = b"stand-in bytes for a signed-MessageRequestPayload".to_vec();
    let (ss, enc) = hpke::encaps_derand(&pk, &rnd).unwrap();
    let mut ctx = hpke::key_schedule(&ss, &info);
    let ct = ctx.seal(b"", &pt);
    json!({
        "suite": {"spec": "draft-ietf-hpke-pq-05", "kem_id": hpke::KEM_ID, "kdf_id": hpke::KDF_ID, "aead_id": hpke::AEAD_ID, "mode": 0},
        "sk_seed": hex(&seed), "pk": hex(&pk), "ticket_id": hex(&ticket), "request_key": hex(&request_key),
        "request_key_digest": hex(&rk_digest), "info": hex(&info), "encaps_randomness": hex(&rnd),
        "shared_secret": hex(&ss), "enc": hex(&enc), "key": hex(&ctx.key), "base_nonce": hex(&ctx.base_nonce),
        "aad": "", "plaintext": hex(&pt), "ciphertext": hex(&ct),
    })
}

pub fn generate() -> Value {
    use Ev::*;
    let c1 = [
        Send(true, "a0"), Send(true, "a1"), Send(true, "a2"),
        Recv(false, "a0"), Recv(false, "a2"),
        Send(false, "b0"), Send(false, "b1"),
        Recv(true, "b1"), Recv(true, "b0"),
        Send(true, "a3"),
        Recv(false, "a1"), Recv(false, "a3"),
        Send(false, "b2"), Recv(true, "b2"),
        Send(true, "a4"), Send(true, "a5"), Recv(false, "a5"), Recv(false, "a4"),
        Send(false, "b3"), Send(false, "b4"), Recv(true, "b4"), Recv(true, "b3"),
        Send(true, "a6"), Recv(false, "a6"),
    ];
    let c2 = [
        Send(true, "a0"), Recv(false, "a0"), Send(false, "b0"), Recv(true, "b0"),
        Send(true, "a1"), Send(true, "a2"), Recv(false, "a2"), Recv(false, "a1"),
    ];
    json!({
        "format": "om-v001-vectors/2",
        "status": "Feasibility prototype vectors for V-001 (OM-Crypto-v1, ADR-0131/ADR-0135). INSECURE seeded RNG. Not product vectors.",
        "specs": {
            "pqxdh": "Signal PQXDH revision 3 (2023-05-24, updated 2024-01-23); prekeys authenticated by the OM composite device key (ADR-0135 §2)",
            "double_ratchet": "Signal Double Ratchet revision 4 (2025-11-04) §3; AEAD per ADR-0131 rule 3",
            "signatures": "Ed25519 (RFC 8032) AND ML-DSA-65 (FIPS 204, empty context) over DS-SIG bytes",
            "mlkem": "FIPS 203 ML-KEM-768 (keygen seed = d || z)",
            "aead": "RFC 8452 AES-256-GCM-SIV",
            "hpke": "draft-ietf-hpke-pq-05 MLKEM768-X25519 / HKDF-SHA256 / ChaCha20Poly1305, base mode",
        },
        "constants": {
            "pqxdh_info": String::from_utf8_lossy(kdf::PQXDH_INFO),
            "dr_rk_info": String::from_utf8_lossy(kdf::DR_RK_INFO),
            "dr_aead_key_info": String::from_utf8_lossy(kdf::DR_AEAD_KEY_INFO),
            "dr_aead_nonce_info": String::from_utf8_lossy(kdf::DR_AEAD_NONCE_INFO),
            "dr_header_hash_label": kdf::DR_HEADER_HASH,
            "event_aead_ad_label": kdf::EVENT_AEAD_AD,
            "handshake_id_label": kdf::HANDSHAKE_ID,
            "ec_type_byte": pqxdh::EC_TYPE_X25519,
            "profile": PROFILE_V1,
        },
        "binding": binding_vectors(),
        "initiator_binding": initiator_binding_vectors(),
        "cases": [
            case("one_time_pq_and_ec_prekeys", true, &c1),
            case("last_resort_pq_no_one_time_keys", false, &c2),
        ],
        "hpke_message_request": hpke_vector(),
    })
}
