// SPDX-License-Identifier: Apache-2.0
//! Named selftest checks for V-001 (criteria 3, 6, 7, 8 and 9 of the gate ledger):
//! dual-signature device binding (ADR-0131 rule 1), one-time key handling and depletion, the exact
//! last-resort replay ledger with crash/reload and the 4096 cap (rule 2), the Double Ratchet AEAD
//! derivation (rule 3) and the MessageRequest HPKE envelope (rule 9). Returns pass/fail checks.

use crate::binding::{self, Acs, Credential, PeerView};
use crate::cbor::{self, V};
use crate::hpke;
use crate::kdf::dr_aead_key_nonce;
use crate::ledger::LAST_RESORT_ACCEPT_LIMIT;
use crate::pqxdh::{self, PqKind};
use crate::rng::{arr32, CryptoRng, RngCore};
use crate::session::{self, SessionState};
use crate::sig::{self, ds_hash, ds_sig, ROLE_DEVICE};
use crate::store::{Device, MemVault, Provider, NOW};
use crate::{Error, PROFILE_V1};

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
        Err(e) => (&e == want || (matches!(want, Error::Binding(_)) && matches!(e, Error::Binding(_))), format!("rejected: {}", e.name())),
    }
}

/// Re-sign a (possibly modified) bundle body as `signer` claiming `device_id`, with a chosen signature list.
fn envelope_with(body: &[u8], sigs: Vec<V>) -> Vec<u8> {
    sig::envelope(body.to_vec(), sigs)
}

fn bundle_parts(signed: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let v = cbor::decode(signed).unwrap();
    let body = v.get(1).unwrap().b().unwrap().to_vec();
    let s = v.get(2).unwrap().a().unwrap()[0].get(3).unwrap().b().unwrap().to_vec();
    (body, s)
}

fn set_key(body: &[u8], key: u64, val: Option<V>) -> Vec<u8> {
    let mut v = cbor::decode(body).unwrap();
    if let V::M(ref mut m) = v {
        m.retain(|(k, _)| *k != key);
        if let Some(x) = val {
            m.push((key, x));
        }
    }
    cbor::encode(&v)
}

/// A second, independent Alice identity with only an X25519 identity key changed (cheap initiators).
fn initiator<R: RngCore + CryptoRng>(template: &Device, rng: &mut R) -> Device {
    let mut a = template.clone();
    a.ik_secret = arr32(rng);
    a.sessions.clear();
    a
}

fn binding_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let evil = Device::new(rng);
    // (bob is mutated by generate_one_time)
    let did = bob.device_id;
    let mut prov = Provider::default();
    let up = bob.generate_one_time(0, 1, rng);
    prov.publish(&bob, up);
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let fetch = prov.fetch(&did).unwrap();
    let ok = binding::verify_fetch(&fetch, &view).map(|v| v.pq_one_time && v.ik == bob.ik() && v.account_id == bob.account_id);
    chk(out, "binding_dual_signed_bundle_accepted", ok == Ok(true), format!("{ok:?}"));

    let (body, s) = bundle_parts(&fetch[0]);
    let msg = ds_sig(binding::LBL_BUNDLE_SIG, PROFILE_V1, ROLE_DEVICE, &did, &body);
    let evil_sig = evil.signer.sign(&msg, rng);
    let entry = |sb: Vec<u8>| sig::signature_entry(&did, PROFILE_V1, ROLE_DEVICE, sb);
    let try_sig = |sb: Vec<u8>| binding::verify_fetch(&[envelope_with(&body, vec![entry(sb)])], &view);

    let mut x = evil_sig[..sig::ED_SIG].to_vec();
    x.extend_from_slice(&s[sig::ED_SIG..]);
    let (ok, d) = expect_err(try_sig(x), &Error::BadSignature);
    chk(out, "binding_ed25519_half_substituted_rejected", ok, format!("valid ML-DSA-65 half, Ed25519 half from another key: {d}"));
    let mut x = s[..sig::ED_SIG].to_vec();
    x.extend_from_slice(&evil_sig[sig::ED_SIG..]);
    let (ok, d) = expect_err(try_sig(x), &Error::BadSignature);
    chk(out, "binding_mldsa65_half_substituted_rejected", ok, format!("valid Ed25519 half, ML-DSA-65 half from another key: {d}"));
    let (ok, d) = expect_err(try_sig(s[..sig::ED_SIG].to_vec()), &Error::BadSignature);
    chk(out, "binding_ed25519_only_rejected", ok, d);
    let (ok, d) = expect_err(try_sig(s[sig::ED_SIG..].to_vec()), &Error::BadSignature);
    chk(out, "binding_mldsa65_only_rejected", ok, d);
    let two = envelope_with(&body, vec![entry(s[..sig::ED_SIG].to_vec()), entry(s[sig::ED_SIG..].to_vec())]);
    let (ok, d) = expect_err(binding::verify_fetch(&[two], &view), &Error::Binding(""));
    chk(out, "binding_split_into_two_signature_entries_rejected", ok, d);
    let (ok, d) = expect_err(try_sig(evil_sig.clone()), &Error::BadSignature);
    chk(out, "binding_whole_signature_by_other_device_rejected", ok, d);

    // Substitutions inside the signed body keep the original signature.
    let evil_pq = pqxdh::mlkem_pub(&evil.last_resort.seed);
    let b8 = set_key(&body, 8, Some(V::M(vec![(0, V::U(bob.last_resort.id as u64)), (1, V::U(2)), (2, V::B(evil_pq.clone()))])));
    let (ok, d) = expect_err(binding::verify_fetch(&[envelope_with(&b8, vec![entry(s.clone())])], &view), &Error::BadSignature);
    chk(out, "binding_substituted_pq_signed_prekey_rejected", ok, d);
    let b7 = set_key(&body, 7, Some(V::M(vec![(0, V::U(bob.spk.id as u64)), (1, V::U(1)), (2, V::B(evil.ik().to_vec()))])));
    let (ok, d) = expect_err(binding::verify_fetch(&[envelope_with(&b7, vec![entry(s.clone())])], &view), &Error::BadSignature);
    chk(out, "binding_substituted_ec_signed_prekey_rejected", ok, d);
    let mut item_forged = cbor::decode(&fetch[1]).unwrap();
    if let V::M(ref mut m) = item_forged {
        m.retain(|(k, _)| *k != 2);
        m.push((2, V::B(evil_pq.clone())));
    }
    let (ok, d) = expect_err(binding::verify_fetch(&[fetch[0].clone(), cbor::encode(&item_forged)], &view), &Error::BadSignature);
    chk(out, "binding_substituted_pq_one_time_prekey_rejected", ok, d);

    // A validly signed bundle with no ML-KEM signed prekey is still refused (no classical fallback).
    let b_nopq = set_key(&body, 8, None);
    let resigned = binding::sign_bundle(&bob.signer, &did, b_nopq, arr32(rng));
    let (ok, d) = expect_err(binding::verify_fetch(&[resigned], &view), &Error::MissingPqPrekey);
    chk(out, "binding_bundle_without_pq_signed_prekey_refused", ok, d);

    // Credential / ACS chain.
    let mut cred = Credential::from_body(&cred_b).unwrap();
    cred.ik = evil.ik();
    let cb = cred.body();
    let (ok, d) = expect_err(binding::verify_fetch(&fetch, &Device::peer_view(&acs_b, &cb)), &Error::Binding(""));
    chk(out, "binding_substituted_identity_key_rejected", ok, format!("X25519 identity key swapped in DeviceCredential: {d}"));
    let mut cred = Credential::from_body(&cred_b).unwrap();
    cred.signing_pk = evil.signer.public();
    let cb2 = cred.body();
    let (ok, d) = expect_err(binding::verify_fetch(&fetch, &Device::peer_view(&acs_b, &cb2)), &Error::Binding(""));
    chk(out, "binding_substituted_signing_key_rejected", ok, d);
    let mut q = Acs::from_body(&acs_b).unwrap();
    q.quarantined.insert(did);
    let qb = q.body();
    let (ok, d) = expect_err(binding::verify_fetch(&fetch, &Device::peer_view(&qb, &cred_b)), &Error::Binding(""));
    chk(out, "binding_quarantined_device_rejected", ok, d);
    let mut g2 = Acs::from_body(&acs_b).unwrap();
    g2.generation += 1;
    let g2b = g2.body();
    let (ok, d) = expect_err(binding::verify_fetch(&fetch, &Device::peer_view(&g2b, &cred_b)), &Error::Binding(""));
    chk(out, "binding_bundle_for_stale_acs_head_rejected", ok, d);
    let wrong = PeerView { acs_body: &acs_b, trusted_acs_hash: [7; 32], credential_body: &cred_b, now: NOW };
    let (ok, d) = expect_err(binding::verify_fetch(&fetch, &wrong), &Error::Binding(""));
    chk(out, "binding_untrusted_acs_body_rejected", ok, d);
}

fn otk_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let did = bob.device_id;
    let bob_ik = bob.ik();
    let mut prov = Provider::default();
    let up = bob.generate_one_time(2, 3, rng);
    prov.publish(&bob, up);
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let tmpl = Device::new(rng);

    // Depletion: 5 initiators vs 2 EC + 3 PQ one-time keys.
    let mut envs = vec![];
    let mut kinds = vec![];
    let mut opk_present = vec![];
    for i in 0..5 {
        let mut a = initiator(&tmpl, rng);
        let f = prov.fetch(&did).expect("fetch");
        let (vb, _) = a.start_session(&f, &view, rng).expect("PQXDH initiate");
        kinds.push(if vb.pq_one_time { PqKind::OneTime } else { PqKind::LastResort });
        opk_present.push(vb.opk.is_some());
        envs.push(a.encrypt(&bob_ik, format!("hello from alice {i}").as_bytes()).expect("encrypt").0);
    }
    let want = [PqKind::OneTime, PqKind::OneTime, PqKind::OneTime, PqKind::LastResort, PqKind::LastResort];
    chk(out, "depletion_falls_back_to_signed_pq_prekey", kinds == want,
        format!("pq kinds per fetch: {:?}; ec one-time present: {:?}", kinds.iter().map(|k| k.name()).collect::<Vec<_>>(), opk_present));
    let mut all_ok = true;
    for (i, e) in envs.iter().enumerate() {
        all_ok &= bob.decrypt(e, rng).map(|x| x.1 == format!("hello from alice {i}").as_bytes()).unwrap_or(false);
    }
    let all_mlkem = envs.iter().all(|e| {
        let v = cbor::decode(e).unwrap();
        v.a().unwrap()[1].get(3).map(|c| c.b().map(|b| b.len() == pqxdh::MLKEM768_CT_LEN).unwrap_or(false)).unwrap_or(false)
    });
    chk(out, "every_handshake_includes_mlkem_no_classical_only", all_ok && all_mlkem && bob.sessions.len() == 5,
        format!("{} sessions; every initial message carries a 1088-byte ML-KEM-768 ciphertext", bob.sessions.len()));
    chk(out, "one_time_private_keys_deleted_after_use", bob.opks.is_empty() && bob.pqopks.is_empty() && prov.inventory(&did) == (0, 0),
        format!("device one-time keys left: ec={} pq={}; provider inventory {:?}", bob.opks.len(), bob.pqopks.len(), prov.inventory(&did)));
    chk(out, "one_time_initiations_do_not_consume_ledger_allowance", bob.ledger.count(bob.epoch) == 2,
        format!("ledger entries in epoch {}: {} (only initiations 4 and 5 used no one-time key)", bob.epoch, bob.ledger.count(bob.epoch)));

    // Replays.
    let snapshot = bob.clone();
    let (ok, d) = match bob.decrypt(&envs[0], rng) {
        Err(e @ Error::DecryptFailed) => (true, format!("rejected: {}", e.name())),
        r => (false, format!("{:?}", r.map(|x| x.1))),
    };
    chk(out, "replay_to_existing_session_rejected_state_unchanged", ok && bob == snapshot, format!("{d}; state unchanged={}", bob == snapshot));
    let mut fresh = bob.clone();
    fresh.sessions.clear();
    let (ok, d) = expect_err(fresh.decrypt(&envs[0], rng), &Error::UnknownOrConsumedPqPrekey);
    chk(out, "replay_of_consumed_one_time_pq_prekey_rejected", ok, d);
    let (ok, d) = expect_err(fresh.decrypt(&envs[2], rng), &Error::UnknownOrConsumedPqPrekey);
    chk(out, "replay_of_one_time_pq_without_ec_opk_rejected", ok, d);
    let (ok, d) = expect_err(fresh.decrypt(&envs[3], rng), &Error::ReplayedLastResortHandshake);
    let (ok2, _) = expect_err(fresh.decrypt(&envs[4], rng), &Error::ReplayedLastResortHandshake);
    chk(out, "replay_of_no_one_time_key_handshake_rejected_by_exact_ledger", ok && ok2, d);
    // EC one-time key + last-resort PQ key: single use comes from the EC key; the ledger is not touched.
    let up = bob.generate_one_time(1, 0, rng);
    prov.publish(&bob, up);
    let mut a5 = initiator(&tmpl, rng);
    let f5 = prov.fetch(&did).unwrap();
    let (vb5, _) = a5.start_session(&f5, &view, rng).unwrap();
    let env5 = a5.encrypt(&bob_ik, b"ec one-time, pq last resort").unwrap().0;
    let n0 = bob.ledger.count(bob.epoch);
    let accepted = bob.decrypt(&env5, rng).is_ok();
    let mut fresh = bob.clone();
    fresh.sessions.clear();
    let (ok, d) = expect_err(fresh.decrypt(&env5, rng), &Error::UnknownOrConsumedEcPrekey);
    chk(out, "replay_of_ec_one_time_with_last_resort_pq_rejected", accepted && !vb5.pq_one_time && vb5.opk.is_some() && ok && bob.ledger.count(bob.epoch) == n0, d);

    // Tampered initial message must not consume a one-time key; the genuine one then succeeds once.
    let up = bob.generate_one_time(1, 1, rng);
    prov.publish(&bob, up);
    let mut a6 = initiator(&tmpl, rng);
    let f6 = prov.fetch(&did).unwrap();
    let (vb6, _) = a6.start_session(&f6, &view, rng).unwrap();
    chk(out, "replenished_pool_serves_one_time_again", vb6.pq_one_time && vb6.opk.is_some(), format!("pq_one_time={}", vb6.pq_one_time));
    let env6 = a6.encrypt(&bob_ik, b"six").unwrap().0;
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
    chk(out, "tampered_initial_message_rejected_without_consuming_keys", r.is_err() && before == after && before == (1, 1),
        format!("{}; one-time keys before={before:?} after={after:?}", r.err().map(|e| e.name()).unwrap_or_default()));
    let ok = bob.decrypt(&env6, rng).map(|x| x.1 == b"six").unwrap_or(false);
    chk(out, "genuine_initial_message_consumes_exactly_once", ok && bob.opks.is_empty() && bob.pqopks.is_empty(), "accepted once; keys deleted");

    // Classical-only initial message (ML-KEM ciphertext removed) is rejected by the recipient.
    let mut a7 = initiator(&tmpl, rng);
    let f7 = prov.fetch(&did).unwrap();
    a7.start_session(&f7, &view, rng).unwrap();
    let env7 = a7.encrypt(&bob_ik, b"seven").unwrap().0;
    let mut noct = cbor::decode(&env7).unwrap();
    if let V::A(ref mut arr) = noct {
        if let V::M(ref mut m) = arr[1] {
            m.retain(|(k, _)| *k != 3);
        }
    }
    let (ok, d) = expect_err(bob.decrypt(&cbor::encode(&noct), rng), &Error::InvalidPqCiphertext);
    chk(out, "initial_message_without_mlkem_ciphertext_rejected", ok, d);
    bob.decrypt(&env7, rng).expect("genuine a7");

    // Signed-prekey epoch rotation: grace period, then expiry together with that epoch's ledger.
    let mut a8 = initiator(&tmpl, rng);
    let f8 = prov.fetch(&did).unwrap();
    a8.start_session(&f8, &view, rng).unwrap();
    let env8 = a8.encrypt(&bob_ik, b"eight").unwrap().0;
    let mut a9 = initiator(&tmpl, rng);
    a9.start_session(&f8, &view, rng).unwrap();
    let env9 = a9.encrypt(&bob_ik, b"nine").unwrap().0;
    let old_epoch = bob.epoch;
    let up = bob.rotate_signed_prekeys(rng);
    prov.publish(&bob, up);
    let ok = bob.decrypt(&env8, rng).is_ok();
    chk(out, "previous_epoch_accepted_during_grace", ok, "delayed initiation to the rotated-out signed prekeys accepted");
    bob.expire_previous();
    let (ok, d) = expect_err(bob.decrypt(&env9, rng), &Error::UnknownSignedPrekey);
    chk(out, "expired_epoch_rejected_and_its_ledger_dropped_with_its_secrets", ok && bob.ledger.count(old_epoch) == 0, d);
    let (acs2, cred2) = prov.view(&did);
    let v2 = binding::verify_fetch(&prov.fetch(&did).unwrap(), &Device::peer_view(&acs2, &cred2)).unwrap();
    chk(out, "provider_serves_rotated_epoch", v2.epoch == bob.epoch && v2.pq_id == bob.last_resort.id, format!("epoch {}", v2.epoch));

    let bytes = bob.to_bytes();
    let back = Device::from_bytes(&bytes).map(|d| d.to_bytes() == bytes && d == bob).unwrap_or(false);
    chk(out, "device_state_deterministic_cbor_roundtrip", back, format!("{} bytes, {} sessions", bytes.len(), bob.sessions.len()));
}

fn ledger_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let did = bob.device_id;
    let bob_ik = bob.ik();
    let mut prov = Provider::default();
    prov.publish(&bob, crate::store::Upload { bundle: bob.signed_bundle(rng), ..Default::default() });
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let tmpl = Device::new(rng);
    let lr_env = |rng: &mut R, prov: &mut Provider| {
        let mut a = initiator(&tmpl, rng);
        a.start_session(&prov.fetch(&did).unwrap(), &view, rng).unwrap();
        a.encrypt(&bob_ik, b"last resort").unwrap().0
    };

    // Commit ordering and crash/reload.
    let mut vault = MemVault::default();
    let e1 = lr_env(rng, &mut prov);
    let h1 = {
        let v = cbor::decode(&e1).unwrap();
        pqxdh::handshake_id(&did, bob.epoch, &pqxdh::InitHeader::from_cbor(&v.a().unwrap()[1]).unwrap())
    };
    vault.fail_next = true;
    let snap = bob.clone();
    let r = bob.receive(&e1, &mut vault, rng);
    chk(out, "ledger_vault_commit_failure_releases_nothing_and_changes_nothing", r.as_ref().err() == Some(&Error::VaultCommitFailed) && bob == snap && vault.stored.is_none(),
        format!("{:?}", r.err()));
    let r = bob.receive(&e1, &mut vault, rng);
    let durable = vault.stored.as_ref().and_then(|s| Device::from_bytes(s).ok()).map(|d| d.ledger.epochs.get(&d.epoch).map(|s| s.contains(&h1)).unwrap_or(false));
    chk(out, "ledger_entry_durable_before_plaintext_released", r.is_ok() && durable == Some(true), "plaintext returned only after the vault held the handshake_id");
    // Crash after commit, before ack: reload from the vault, then the same initiation arrives again.
    let mut reloaded = Device::from_bytes(vault.stored.as_ref().unwrap()).unwrap();
    reloaded.sessions.clear(); // even without the session, the ledger alone rejects it
    let (ok, d) = expect_err(reloaded.receive(&e1, &mut vault, rng), &Error::ReplayedLastResortHandshake);
    chk(out, "ledger_replay_rejected_after_crash_and_reload", ok, d);

    // Cap: fill the epoch to exactly 4096 accepted last-resort initiations.
    let mut n = bob.ledger.count(bob.epoch);
    let first = e1.clone();
    while n < LAST_RESORT_ACCEPT_LIMIT {
        let e = lr_env(rng, &mut prov);
        bob.decrypt(&e, rng).expect("last-resort accept");
        n += 1;
    }
    let extra = lr_env(rng, &mut prov);
    let before = bob.clone();
    let (ok, d) = expect_err(bob.decrypt(&extra, rng), &Error::LastResortExhausted);
    chk(out, "ledger_cap_4096_fails_closed", ok && bob == before && bob.ledger.count(bob.epoch) == LAST_RESORT_ACCEPT_LIMIT,
        format!("{d}; {} ids held", bob.ledger.count(bob.epoch)));
    let mut again = bob.clone();
    again.sessions.clear();
    let (ok, d) = expect_err(again.decrypt(&first, rng), &Error::ReplayedLastResortHandshake);
    chk(out, "ledger_no_eviction_at_cap_first_id_still_rejected", ok, d);
    let up = bob.generate_one_time(0, 1, rng);
    prov.publish(&bob, up);
    let mut a = initiator(&tmpl, rng);
    a.start_session(&prov.fetch(&did).unwrap(), &view, rng).unwrap();
    let ot = a.encrypt(&bob_ik, b"one-time at cap").unwrap().0;
    chk(out, "ledger_one_time_initiation_still_accepted_at_cap", bob.decrypt(&ot, rng).is_ok(), "PQ one-time handshake does not touch the exhausted allowance");
    let old = bob.epoch;
    let up = bob.rotate_signed_prekeys(rng);
    prov.publish(&bob, up);
    let (acs2, cred2) = prov.view(&did);
    let view2 = Device::peer_view(&acs2, &cred2);
    let mut a = initiator(&tmpl, rng);
    a.start_session(&prov.fetch(&did).unwrap(), &view2, rng).unwrap();
    let fresh = a.encrypt(&bob_ik, b"fresh epoch").unwrap().0;
    let ok = bob.decrypt(&fresh, rng).is_ok();
    let (ok2, d) = expect_err(bob.decrypt(&extra, rng), &Error::LastResortExhausted);
    chk(out, "ledger_fresh_epoch_accepts_old_epoch_stays_exhausted_during_grace", ok && ok2 && bob.ledger.count(old) == LAST_RESORT_ACCEPT_LIMIT, d);
}

fn aead_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let sk = arr32(rng);
    let bob_spk = arr32(rng);
    let ad = vec![5u8; 66];
    let mut a = SessionState::init_alice(&sk, ad.clone(), &pqxdh::x25519_pub(&bob_spk), rng);
    let mut b = SessionState::init_bob(&sk, ad, &bob_spk);
    let e = a.encrypt(b"aead check", b"ctx").unwrap();
    let hh = ds_hash(crate::kdf::DR_HEADER_HASH, &e.header);
    let (k, n) = dr_aead_key_nonce(&e.keys.mk, &hh);
    chk(out, "aead_key_nonce_follow_adr0131_rule3", k == e.keys.key && n == e.keys.nonce && hh == e.keys.header_hash, "K/N recomputed from MK and the header hash");
    let mut h2 = e.header.clone();
    let last = h2.len() - 1;
    h2[last] ^= 1; // n field
    let snap = b.clone();
    let r = b.decrypt(&h2, &e.ct, b"ctx", rng);
    chk(out, "aead_header_tamper_rejected_state_unchanged", r.is_err() && b == snap, format!("{:?}", r.err()));
    let (ok, d) = expect_err(b.decrypt(&e.header, &e.ct, b"other envelope", rng), &Error::DecryptFailed);
    chk(out, "aead_envelope_context_is_authenticated", ok && b == snap, d);
    let ok = b.decrypt(&e.header, &e.ct, b"ctx", rng).map(|x| x.0 == b"aead check").unwrap_or(false);
    chk(out, "aead_genuine_message_accepted", ok, "AES-256-GCM-SIV open with the derived key/nonce");
    let h = session::parse_header(&e.header).unwrap();
    chk(out, "dr_header_is_not_encrypted_in_v1", h.dh == a.ec.dhs_pub, "ratchet public key and counters are plaintext CBOR (ADR-0131 rule 4); V-015 records the exposure");
}

fn hpke_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let seed = arr32(rng);
    let pk = hpke::public_key(&seed);
    let ticket = [9u8; 16];
    let rk_digest = ds_hash("OM-REQUEST-KEY-HASH-v1", &cbor::encode(&V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(pk.clone()))])));
    let info = hpke::message_request_info(PROFILE_V1, &ticket, &rk_digest);
    let mut rnd = [0u8; 64];
    rng.fill_bytes(&mut rnd);
    let (ss, enc) = hpke::encaps_derand(&pk, &rnd).unwrap();
    let ct = hpke::key_schedule(&ss, &info).seal(b"", b"signed MessageRequestPayload");
    let ssr = hpke::decaps(&seed, &enc).unwrap();
    let ok = hpke::key_schedule(&ssr, &info).open(b"", &ct).map(|p| p == b"signed MessageRequestPayload").unwrap_or(false);
    chk(out, "hpke_message_request_roundtrip", ok && enc.len() == hpke::NENC, format!("enc {} bytes, ct {} bytes", enc.len(), ct.len()));
    let info2 = hpke::message_request_info(PROFILE_V1, &[8u8; 16], &rk_digest);
    let (ok, d) = expect_err(hpke::key_schedule(&ssr, &info2).open(b"", &ct), &Error::DecryptFailed);
    chk(out, "hpke_info_binds_ticket_id", ok, d);
    let mut bad = enc.clone();
    bad[1100] ^= 1; // X25519 half of enc
    let ss_bad = hpke::decaps(&seed, &bad).unwrap();
    let (ok, d) = expect_err(hpke::key_schedule(&ss_bad, &info).open(b"", &ct), &Error::DecryptFailed);
    chk(out, "hpke_x25519_half_bound_into_shared_secret", ok, d);
    let (ok, d) = expect_err(hpke::decaps(&seed, &enc[..1088]), &Error::InvalidPqCiphertext);
    chk(out, "hpke_pq_only_enc_rejected", ok, d);
}

pub fn run<R: RngCore + CryptoRng>(rng: &mut R) -> Vec<Check> {
    let mut out = vec![];
    binding_checks(&mut out, rng);
    otk_checks(&mut out, rng);
    ledger_checks(&mut out, rng);
    aead_checks(&mut out, rng);
    hpke_checks(&mut out, rng);
    out
}
