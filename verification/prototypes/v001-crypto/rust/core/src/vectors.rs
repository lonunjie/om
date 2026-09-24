// SPDX-License-Identifier: Apache-2.0
//! Deterministic test vectors (insecure seeded RNG) for a second, independent implementation.
//! The consumer must reproduce: X25519 public keys, XEdDSA prekey-signature verification,
//! ML-KEM-768 keygen/encaps/decaps, PQXDH DH1..DH4/SK/AD, the Triple-Ratchet SK split, every
//! EC Double Ratchet message key, KDF_HYBRID, header encoding and AES-256-GCM-SIV ciphertexts.
//! SPQR message keys (pq_mk) come from Signal's `spqr` crate and are given as inputs.

use crate::hex;
use crate::kdf;
use crate::pqxdh::{self, PqKind};
use crate::rng::insecure_test_rng::InsecureTestRng;
use crate::store::{Device, Provider};
use serde_json::{json, Value};

enum Ev {
    Send(bool, &'static str), // (from Alice?, label)
    Recv(bool, &'static str), // (to Alice?, label)
}

fn case(name: &str, with_one_time: bool, schedule: &[Ev]) -> Value {
    let mut rng = InsecureTestRng::from_label(format!("om-v001-vectors/{name}").as_bytes());
    let mut bob = Device::new(&mut rng);
    let mut alice = Device::new(&mut rng);
    let mut prov = Provider::default();
    let up = if with_one_time { bob.generate_one_time(1, 1, &mut rng) } else { Default::default() };
    prov.publish(&bob, up);
    let bundle = prov.fetch(&bob.ik()).unwrap();
    let tr = alice.start_session(&bundle, &mut rng).unwrap();
    let a_sess = alice.sessions.values().next().unwrap().clone();
    let (sk_ec, sk_scka) = kdf::tr_split(&tr.sk);

    let pq_seed = match bundle.pq_kind {
        PqKind::OneTime => bob.pqopks[&bundle.pq_id].seed,
        PqKind::LastResort => bob.last_resort.seed,
    };
    let bob_json = json!({
        "ik_secret": hex(&bob.ik_secret), "ik_pub": hex(&bob.ik()),
        "spk_id": bob.spk.id, "spk_secret": hex(&bob.spk.secret), "spk_pub": hex(&bundle.spk), "spk_sig": hex(&bob.spk.sig),
        "pq_id": bundle.pq_id, "pq_kind": bundle.pq_kind.name(), "pq_seed_d_z": hex(&pq_seed),
        "pq_pub_sha256": hex(&crate::sha256(&[&bundle.pq])), "pq_sig": hex(&bundle.pq_sig),
        "opk_id": bundle.opk.map(|(i, _)| i), "opk_secret": bundle.opk.map(|(i, _)| hex(&bob.opks[&i])),
        "opk_pub": bundle.opk.map(|(_, k)| hex(&k)),
        "bundle_cbor": hex(&pqxdh::bundle_bytes(&bundle)),
    });
    let alice_json = json!({
        "ik_secret": hex(&alice.ik_secret), "ik_pub": hex(&alice.ik()),
        "ek_secret": hex(&tr.ek_secret), "ek_pub": hex(&pqxdh::x25519_pub(&tr.ek_secret)),
        "kem_encaps_m": hex(&tr.kem_m),
        "ratchet_init_secret": hex(&a_sess.tr.ec.dhs_secret),
    });
    let handshake = json!({
        "ct": hex(&a_sess.pending_init.as_ref().unwrap().ct),
        "ss": hex(&tr.ss),
        "dh": tr.dh.iter().map(|d| hex(d)).collect::<Vec<_>>(),
        "sk": hex(&tr.sk), "ad": hex(&tr.ad),
        "sk_ec": hex(&sk_ec), "sk_scka": hex(&sk_scka),
        "init_header_cbor": hex(&crate::cbor::encode(&a_sess.pending_init.as_ref().unwrap().to_cbor())),
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
                let (env, k) = s.encrypt(&peer, &pt, &mut rng).unwrap();
                let v = crate::cbor::decode(&env).unwrap();
                let a = v.a().unwrap();
                events.push(json!({
                    "type": "send", "from": if *from_a { "A" } else { "B" }, "label": label,
                    "plaintext": hex(&pt), "header": hex(a[2].b().unwrap()), "ciphertext": hex(a[3].b().unwrap()),
                    "has_init_prefix": !a[1].is_null(),
                    "ec_mk": hex(&k.ec_mk), "pq_mk": hex(&k.pq_mk), "aead_key": hex(&k.key), "aead_nonce": hex(&k.nonce),
                }));
                queue.push((label, env));
            }
            Ev::Recv(to_a, label) => {
                let i = queue.iter().position(|(l, _)| l == label).unwrap();
                let (_, env) = queue.remove(i);
                let r = if *to_a { &mut alice } else { &mut bob };
                let before = r.sessions.values().next().map(|s| s.tr.ec.dhs_secret);
                let (_, pt, k) = r.decrypt(&env, &mut rng).unwrap();
                let after = r.sessions.values().next().unwrap().tr.ec.dhs_secret;
                events.push(json!({
                    "type": "recv", "to": if *to_a { "A" } else { "B" }, "label": label,
                    "plaintext": hex(&pt), "ec_mk": hex(&k.ec_mk), "pq_mk": hex(&k.pq_mk),
                    "new_ratchet_secret": if before == Some(after) { Value::Null } else { json!(hex(&after)) },
                }));
            }
        }
    }
    json!({ "name": name, "bob": bob_json, "alice": alice_json, "handshake": handshake, "events": events })
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
        "format": "om-v001-proto-vectors/1",
        "status": "Feasibility prototype vectors for V-001. INSECURE seeded RNG. Not product vectors; no value here is a frozen OM registry entry.",
        "specs": {
            "pqxdh": "Signal PQXDH revision 3 (2023-05-24, updated 2024-01-23)",
            "double_ratchet": "Signal Double Ratchet revision 4 (2025-11-04) §3, §6, §7",
            "xeddsa": "Signal XEdDSA revision 1 (2016-10-20)",
            "mlkem": "FIPS 203 ML-KEM-768 (keygen seed = d || z)",
            "aead": "RFC 8452 AES-256-GCM-SIV",
        },
        "constants": {
            "pqxdh_info": String::from_utf8_lossy(kdf::PQXDH_INFO),
            "tr_split_info": String::from_utf8_lossy(kdf::TR_SPLIT_INFO),
            "dr_rk_info": String::from_utf8_lossy(kdf::DR_RK_INFO),
            "tr_protocol_info": String::from_utf8_lossy(kdf::TR_PROTOCOL_INFO),
            "ec_type_byte": pqxdh::EC_TYPE_X25519, "kem_type_byte": pqxdh::KEM_TYPE_MLKEM768,
            "header_profile": crate::PROFILE_PROTO,
        },
        "cases": [
            case("one_time_pq_and_ec_prekeys", true, &c1),
            case("last_resort_pq_no_ec_one_time", false, &c2),
        ],
    })
}
