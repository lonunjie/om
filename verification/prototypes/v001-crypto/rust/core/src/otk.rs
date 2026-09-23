// SPDX-License-Identifier: Apache-2.0
//! One-time key handling, depletion, replay and no-downgrade checks (V-001 criterion 3,
//! `ASYNC_KEY_MATERIAL.md` §4/§6). Returns named pass/fail checks for the results JSON.

use crate::cbor::{self, V};
use crate::pqxdh::{PqKind, PrekeyBundle};
use crate::rng::{CryptoRng, RngCore};
use crate::store::{Device, Provider};
use crate::triple::{encode_header, parse_header};
use crate::Error;

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn chk(out: &mut Vec<Check>, name: &str, ok: bool, detail: impl Into<String>) {
    out.push(Check { name: name.into(), ok, detail: detail.into() });
}

fn expect_err<T>(r: Result<T, Error>, want: &Error) -> (bool, String) {
    match r {
        Ok(_) => (false, "unexpectedly accepted".into()),
        Err(e) => (&e == want || (matches!(want, Error::Spqr(_)) && matches!(e, Error::Spqr(_))), format!("rejected: {}", e.name())),
    }
}

pub fn run<R: RngCore + CryptoRng>(rng: &mut R) -> Vec<Check> {
    let mut out = vec![];
    let mut bob = Device::new(rng);
    let bob_ik = bob.ik();
    let mut prov = Provider::default();
    let up = bob.generate_one_time(2, 3, rng);
    prov.publish(&bob, up);

    // 1. Depletion: 5 initiators vs 2 EC + 3 PQ one-time keys.
    let mut alices: Vec<Device> = (0..5).map(|_| Device::new(rng)).collect();
    let mut envs = vec![];
    let mut kinds = vec![];
    let mut opk_present = vec![];
    for (i, a) in alices.iter_mut().enumerate() {
        let b = prov.fetch(&bob_ik).expect("bundle");
        kinds.push(b.pq_kind);
        opk_present.push(b.opk.is_some());
        a.start_session(&b, rng).expect("PQXDH initiate");
        envs.push(a.encrypt(&bob_ik, format!("hello from alice {i}").as_bytes(), rng).expect("encrypt").0);
    }
    let want_kinds = [PqKind::OneTime, PqKind::OneTime, PqKind::OneTime, PqKind::LastResort, PqKind::LastResort];
    chk(&mut out, "depletion_falls_back_to_signed_last_resort_pq_prekey", kinds == want_kinds,
        format!("pq kinds per fetch: {:?}; opk present: {:?}", kinds.iter().map(|k| k.name()).collect::<Vec<_>>(), opk_present));
    let mut all_ok = true;
    for (i, e) in envs.iter().enumerate() {
        match bob.decrypt(e, rng) {
            Ok((_, pt, _)) => all_ok &= pt == format!("hello from alice {i}").as_bytes(),
            Err(_) => all_ok = false,
        }
    }
    let every_session_pq = bob.sessions.values().all(|s| !s.tr.pq.is_empty());
    chk(&mut out, "every_handshake_includes_mlkem_and_spqr_no_classical_only", all_ok && every_session_pq && bob.sessions.len() == 5,
        format!("{} sessions established; all carry an ML-KEM-768 PQXDH secret and a V1 SPQR state", bob.sessions.len()));
    chk(&mut out, "one_time_private_keys_deleted_after_use", bob.opks.is_empty() && bob.pqopks.is_empty() && prov.inventory(&bob_ik) == (0, 0),
        format!("device one-time keys left: ec={} pq={}; provider inventory {:?}", bob.opks.len(), bob.pqopks.len(), prov.inventory(&bob_ik)));

    // 2. Replay of a consumed one-time handshake.
    let snapshot = bob.clone();
    let (ok, d) = match bob.decrypt(&envs[0], rng) {
        Err(e @ (Error::Spqr(_) | Error::DecryptFailed)) => (true, format!("rejected: {}", e.name())),
        r => (false, format!("{:?}", r.map(|x| x.1))),
    };
    chk(&mut out, "replay_to_existing_session_rejected_state_unchanged", ok && bob == snapshot, format!("{d}; state unchanged={}", bob == snapshot));
    let mut fresh = bob.clone();
    fresh.sessions.clear();
    let (ok, d) = expect_err(fresh.decrypt(&envs[0], rng), &Error::UnknownOrConsumedPqPrekey);
    chk(&mut out, "replay_of_consumed_one_time_pq_prekey_rejected", ok, d);
    let (ok, d) = expect_err(fresh.decrypt(&envs[2], rng), &Error::UnknownOrConsumedPqPrekey);
    chk(&mut out, "replay_of_one_time_pq_without_ec_opk_rejected", ok, d);
    let (ok, d) = expect_err(fresh.decrypt(&envs[3], rng), &Error::ReplayedLastResortHandshake);
    chk(&mut out, "replay_of_last_resort_handshake_rejected_by_replay_cache", ok, d);

    // 3. Tampered initial message must not consume a one-time key; the genuine one then succeeds once.
    let up = bob.generate_one_time(1, 1, rng);
    prov.publish(&bob, up);
    let mut a6 = Device::new(rng);
    let b6 = prov.fetch(&bob_ik).unwrap();
    chk(&mut out, "replenished_pool_serves_one_time_again", b6.pq_kind == PqKind::OneTime && b6.opk.is_some(), format!("{:?}", b6.pq_kind));
    a6.start_session(&b6, rng).unwrap();
    let env6 = a6.encrypt(&bob_ik, b"six", rng).unwrap().0;
    let mut tampered = cbor::decode(&env6).unwrap();
    if let V::A(ref mut arr) = tampered {
        if let V::M(ref mut m) = arr[1] {
            for (k, v) in m.iter_mut() {
                if *k == 3 {
                    if let V::B(ref mut ct) = v {
                        ct[10] ^= 0x01;
                    }
                }
            }
        }
    }
    let before = (bob.opks.len(), bob.pqopks.len());
    let r = bob.decrypt(&cbor::encode(&tampered), rng);
    let after = (bob.opks.len(), bob.pqopks.len());
    chk(&mut out, "tampered_initial_message_rejected_without_consuming_keys", r.is_err() && before == after && before == (1, 1),
        format!("{}; one-time keys before={before:?} after={after:?}", r.err().map(|e| e.name()).unwrap_or_default()));
    let ok = bob.decrypt(&env6, rng).map(|x| x.1 == b"six").unwrap_or(false);
    chk(&mut out, "genuine_initial_message_consumes_exactly_once", ok && bob.opks.is_empty() && bob.pqopks.is_empty(), "accepted once; keys deleted");

    // 4. Downgrade attempts by a malicious provider / network.
    let up = bob.generate_one_time(0, 1, rng);
    prov.publish(&bob, up);
    let good = prov.fetch(&bob_ik).unwrap();
    let mut stripped = good.to_cbor();
    if let V::M(ref mut m) = stripped {
        m.retain(|(k, _)| !(5..=8).contains(k));
    }
    let (ok, d) = expect_err(PrekeyBundle::from_cbor(&stripped), &Error::MissingPqPrekey);
    chk(&mut out, "bundle_without_pq_prekey_refused_no_classical_fallback", ok, d);
    let mut forged = good.clone();
    let evil = Device::new(rng);
    forged.pq = crate::pqxdh::mlkem_pub(&evil.last_resort.seed);
    let mut a7 = Device::new(rng);
    let (ok, d) = expect_err(a7.start_session(&forged, rng), &Error::BadSignature);
    chk(&mut out, "substituted_unsigned_pq_prekey_refused", ok, d);
    let mut forged_spk = good.clone();
    forged_spk.spk = evil.ik();
    let (ok, d) = expect_err(a7.start_session(&forged_spk, rng), &Error::BadSignature);
    chk(&mut out, "substituted_signed_prekey_refused", ok, d);

    // Classical-only initial message (ML-KEM ciphertext removed) is rejected by the recipient.
    a7.start_session(&good, rng).unwrap();
    let env7 = a7.encrypt(&bob_ik, b"seven", rng).unwrap().0;
    let mut noct = cbor::decode(&env7).unwrap();
    if let V::A(ref mut arr) = noct {
        if let V::M(ref mut m) = arr[1] {
            m.retain(|(k, _)| *k != 3);
        }
    }
    let (ok, d) = expect_err(bob.decrypt(&cbor::encode(&noct), rng), &Error::InvalidPqCiphertext);
    chk(&mut out, "initial_message_without_mlkem_ciphertext_rejected", ok, d);
    bob.decrypt(&env7, rng).expect("genuine a7");

    // SPQR downgrade: replace the SCKA message with the empty V0 message (PQ ratchet off).
    let reply = bob.encrypt(&a7.ik(), b"reply", rng).unwrap().0;
    let rv = cbor::decode(&reply).unwrap();
    let ra = rv.a().unwrap();
    let h = parse_header(ra[2].b().unwrap()).unwrap();
    let v0 = cbor::encode(&V::A(vec![ra[0].clone(), ra[1].clone(), V::B(encode_header(&h.dh, h.pn, h.n, &[])), ra[3].clone()]));
    let snap = a7.clone();
    let (ok, d) = expect_err(a7.decrypt(&v0, rng), &Error::Spqr(String::new()));
    chk(&mut out, "spqr_v0_downgrade_message_rejected_min_version_v1", ok && a7 == snap, d);
    let ok = a7.decrypt(&reply, rng).map(|x| x.1 == b"reply").unwrap_or(false);
    chk(&mut out, "session_continues_after_rejected_downgrade", ok, "genuine reply accepted after the rejected V0 variant");

    // 5. Last-resort rotation: grace period, then expiry.
    let mut a8 = Device::new(rng);
    let lr_bundle = prov.fetch(&bob_ik).unwrap(); // pool empty -> last resort
    a8.start_session(&lr_bundle, rng).unwrap();
    let env8 = a8.encrypt(&bob_ik, b"eight", rng).unwrap().0;
    let mut a9 = Device::new(rng);
    a9.start_session(&lr_bundle, rng).unwrap();
    let env9 = a9.encrypt(&bob_ik, b"nine", rng).unwrap().0;
    bob.rotate_last_resort(rng);
    prov.publish(&bob, Default::default());
    let ok = bob.decrypt(&env8, rng).is_ok();
    chk(&mut out, "previous_last_resort_accepted_during_grace", lr_bundle.pq_kind == PqKind::LastResort && ok, "delayed message using rotated-out last-resort key accepted");
    bob.expire_previous();
    let (ok, d) = expect_err(bob.decrypt(&env9, rng), &Error::UnknownOrConsumedPqPrekey);
    chk(&mut out, "expired_last_resort_rejected", ok, d);
    let nb = prov.fetch(&bob_ik).unwrap();
    chk(&mut out, "provider_serves_rotated_last_resort", nb.pq_id == bob.last_resort.id && nb.pq_kind == PqKind::LastResort, format!("pq_id {}", nb.pq_id));

    // 6. Persistence of the whole device, byte-identical re-encode.
    let bytes = bob.to_bytes();
    let back = Device::from_bytes(&bytes).map(|d| d.to_bytes() == bytes && d == bob).unwrap_or(false);
    chk(&mut out, "device_state_deterministic_cbor_roundtrip", back, format!("{} bytes, {} sessions", bytes.len(), bob.sessions.len()));
    out
}
