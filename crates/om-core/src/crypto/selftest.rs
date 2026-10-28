// SPDX-License-Identifier: MPL-2.0
//! Named selftest checks for V-001 (criteria 3, 6, 7, 8 and 9 of the gate ledger):
//! dual-signature device binding (ADR-0131 rule 1), one-time key handling and depletion, the exact
//! last-resort replay ledger with crash/reload and the 4096 cap (rule 2), the Double Ratchet AEAD
//! derivation (rule 3) and the MessageRequest HPKE envelope (rule 9). Returns pass/fail checks.

use crate::crypto::binding::{self, Acs, BundleFreshness, Credential, PeerView};
use crate::crypto::cbor::{self, V};
use crate::crypto::delivery;
use crate::crypto::hpke;
use crate::crypto::kdf::dr_aead_key_nonce;
use crate::crypto::ledger::LAST_RESORT_ACCEPT_LIMIT;
use crate::crypto::pqxdh::{self, PqKind};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::session::{self, SessionState};
use crate::crypto::sig::{self, ROLE_DEVICE, ds_hash, ds_sig};
use crate::crypto::store::{Device, MemVault, NOW, Provider};
use crate::crypto::{Error, PROFILE_V1};

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn chk(out: &mut Vec<Check>, name: &str, ok: bool, detail: impl Into<String>) {
    out.push(Check {
        name: name.into(),
        ok,
        detail: detail.into(),
    });
}

fn expect_err<T>(r: Result<T, Error>, want: &Error) -> (bool, String) {
    match r {
        Ok(_) => (false, "unexpectedly accepted".into()),
        Err(e) => (
            &e == want
                || (matches!(want, Error::Binding(_)) && matches!(e, Error::Binding(_)))
                || (matches!(want, Error::Decode(_)) && matches!(e, Error::Decode(_)))
                || (matches!(want, Error::SealedDelivery(_))
                    && matches!(e, Error::SealedDelivery(_))),
            format!("rejected: {}", e.name()),
        ),
    }
}

/// Re-sign a (possibly modified) bundle body as `signer` claiming `device_id`, with a chosen signature list.
fn envelope_with(body: &[u8], sigs: Vec<V>) -> Vec<u8> {
    sig::envelope(body.to_vec(), sigs)
}

fn bundle_parts(signed: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let v = cbor::decode(signed).unwrap();
    let body = v.get(1).unwrap().b().unwrap().to_vec();
    let s = v.get(2).unwrap().a().unwrap()[0]
        .get(3)
        .unwrap()
        .b()
        .unwrap()
        .to_vec();
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
    a.account_id = arr32(rng);
    rng.fill_bytes(&mut a.device_id);
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
    let ok = binding::verify_fetch(&fetch, &view)
        .map(|v| v.pq_one_time && v.ik == bob.ik() && v.account_id == bob.account_id);
    chk(
        out,
        "binding_dual_signed_bundle_accepted",
        ok == Ok(true),
        format!("{ok:?}"),
    );

    let (body, s) = bundle_parts(&fetch[0]);
    let msg = ds_sig(
        binding::LBL_BUNDLE_SIG,
        PROFILE_V1,
        ROLE_DEVICE,
        &did,
        &body,
    );
    let evil_sig = evil.signer.sign(&msg, rng);
    let entry = |sb: Vec<u8>| sig::signature_entry(&did, PROFILE_V1, ROLE_DEVICE, sb);
    let try_sig =
        |sb: Vec<u8>| binding::verify_fetch(&[envelope_with(&body, vec![entry(sb)])], &view);

    let mut x = evil_sig[..sig::ED_SIG].to_vec();
    x.extend_from_slice(&s[sig::ED_SIG..]);
    let (ok, d) = expect_err(try_sig(x), &Error::BadSignature);
    chk(
        out,
        "binding_ed25519_half_substituted_rejected",
        ok,
        format!("valid ML-DSA-65 half, Ed25519 half from another key: {d}"),
    );
    let mut x = s[..sig::ED_SIG].to_vec();
    x.extend_from_slice(&evil_sig[sig::ED_SIG..]);
    let (ok, d) = expect_err(try_sig(x), &Error::BadSignature);
    chk(
        out,
        "binding_mldsa65_half_substituted_rejected",
        ok,
        format!("valid Ed25519 half, ML-DSA-65 half from another key: {d}"),
    );
    let (ok, d) = expect_err(try_sig(s[..sig::ED_SIG].to_vec()), &Error::BadSignature);
    chk(out, "binding_ed25519_only_rejected", ok, d);
    let (ok, d) = expect_err(try_sig(s[sig::ED_SIG..].to_vec()), &Error::BadSignature);
    chk(out, "binding_mldsa65_only_rejected", ok, d);
    let two = envelope_with(
        &body,
        vec![
            entry(s[..sig::ED_SIG].to_vec()),
            entry(s[sig::ED_SIG..].to_vec()),
        ],
    );
    let (ok, d) = expect_err(binding::verify_fetch(&[two], &view), &Error::Binding(""));
    chk(
        out,
        "binding_split_into_two_signature_entries_rejected",
        ok,
        d,
    );
    let (ok, d) = expect_err(try_sig(evil_sig.clone()), &Error::BadSignature);
    chk(
        out,
        "binding_whole_signature_by_other_device_rejected",
        ok,
        d,
    );

    // Substitutions inside the signed body keep the original signature.
    let evil_pq = pqxdh::mlkem_pub(&evil.last_resort.seed);
    let b8 = set_key(
        &body,
        8,
        Some(V::M(vec![
            (0, V::U(bob.last_resort.id as u64)),
            (1, V::U(2)),
            (2, V::B(evil_pq.clone())),
        ])),
    );
    let (ok, d) = expect_err(
        binding::verify_fetch(&[envelope_with(&b8, vec![entry(s.clone())])], &view),
        &Error::BadSignature,
    );
    chk(out, "binding_substituted_pq_signed_prekey_rejected", ok, d);
    let b7 = set_key(
        &body,
        7,
        Some(V::M(vec![
            (0, V::U(bob.spk.id as u64)),
            (1, V::U(1)),
            (2, V::B(evil.ik().to_vec())),
        ])),
    );
    let (ok, d) = expect_err(
        binding::verify_fetch(&[envelope_with(&b7, vec![entry(s.clone())])], &view),
        &Error::BadSignature,
    );
    chk(out, "binding_substituted_ec_signed_prekey_rejected", ok, d);
    let mut item_forged = cbor::decode(&fetch[1]).unwrap();
    if let V::M(ref mut m) = item_forged {
        m.retain(|(k, _)| *k != 2);
        m.push((2, V::B(evil_pq.clone())));
    }
    let (ok, d) = expect_err(
        binding::verify_fetch(&[fetch[0].clone(), cbor::encode(&item_forged)], &view),
        &Error::BadSignature,
    );
    chk(
        out,
        "binding_substituted_pq_one_time_prekey_rejected",
        ok,
        d,
    );

    // A validly signed bundle with no ML-KEM signed prekey is still refused (no classical fallback).
    let b_nopq = set_key(&body, 8, None);
    let resigned = binding::sign_bundle(&bob.signer, &did, b_nopq, arr32(rng));
    let (ok, d) = expect_err(
        binding::verify_fetch(&[resigned], &view),
        &Error::Decode("missing map key"),
    );
    chk(
        out,
        "binding_bundle_without_pq_signed_prekey_refused",
        ok,
        d,
    );

    // Credential / ACS chain.
    let mut cred = Credential::from_body(&cred_b).unwrap();
    cred.ik = evil.ik();
    let cb = cred.body();
    let (ok, d) = expect_err(
        binding::verify_fetch(&fetch, &Device::peer_view(&acs_b, &cb)),
        &Error::Binding(""),
    );
    chk(
        out,
        "binding_substituted_identity_key_rejected",
        ok,
        format!("X25519 identity key swapped in DeviceCredential: {d}"),
    );
    let mut cred = Credential::from_body(&cred_b).unwrap();
    cred.signing_pk = evil.signer.public();
    let cb2 = cred.body();
    let (ok, d) = expect_err(
        binding::verify_fetch(&fetch, &Device::peer_view(&acs_b, &cb2)),
        &Error::Binding(""),
    );
    chk(out, "binding_substituted_signing_key_rejected", ok, d);
    let mut q = Acs::from_body(&acs_b).unwrap();
    q.quarantined.insert(did);
    let qb = q.body();
    let (ok, d) = expect_err(
        binding::verify_fetch(&fetch, &Device::peer_view(&qb, &cred_b)),
        &Error::Binding(""),
    );
    chk(out, "binding_quarantined_device_rejected", ok, d);
    // ADR-0140: provider-visible ACS generation/hash are gone. A newer relationship-pinned ACS head
    // that still commits the same active DeviceCredential therefore validates the same fresh bundle.
    let mut g2 = Acs::from_body(&acs_b).unwrap();
    g2.generation += 1;
    let g2b = g2.body();
    let r = binding::verify_fetch(&fetch, &Device::peer_view(&g2b, &cred_b));
    chk(
        out,
        "binding_bundle_uses_relationship_pinned_acs_not_provider_visible_generation",
        r.is_ok(),
        format!("{r:?}"),
    );

    // Provider pools share one signed-prekey generation floor, but one-time keys are pool-bound.
    let pool_a = [0x21; 16];
    let pool_b = [0x22; 16];
    let mut up_a = bob.generate_one_time_for_pool(pool_a, 0, 1, rng);
    let up_b = bob.generate_one_time_for_pool(pool_b, 0, 0, rng);
    let item_a = up_a.pqopks.remove(0);
    let current_acs = bob.acs().body();
    let current_cred = bob.credential().body();
    let current_view = Device::peer_view(&current_acs, &current_cred);
    let mut freshness = BundleFreshness::default();
    let first = binding::verify_fetch_with_freshness(
        &[up_a.bundle.clone(), item_a.clone()],
        &current_view,
        &mut freshness,
    );
    let same_generation_other_pool = binding::verify_fetch_with_freshness(
        std::slice::from_ref(&up_b.bundle),
        &current_view,
        &mut freshness,
    );
    chk(
        out,
        "binding_same_generation_same_core_across_provider_pools_accepted",
        first.is_ok() && same_generation_other_pool.is_ok(),
        format!("first={first:?}; other_pool={same_generation_other_pool:?}"),
    );
    let (ok, d) = expect_err(
        binding::verify_fetch_with_freshness(
            &[up_b.bundle.clone(), item_a],
            &current_view,
            &mut freshness,
        ),
        &Error::BadSignature,
    );
    chk(
        out,
        "binding_one_time_prekey_transplanted_across_provider_pool_rejected",
        ok,
        d,
    );

    // Signed-prekey freshness is global across provider pools. Once generation 2 is accepted, replay
    // of a still-valid generation 1 bundle is rejected even if a provider retained it.
    let old = up_a.bundle;
    let new_up = bob.rotate_signed_prekeys(rng);
    let newer = binding::verify_fetch_with_freshness(
        std::slice::from_ref(&new_up.bundle),
        &current_view,
        &mut freshness,
    );
    let (rollback_ok, rollback_detail) = expect_err(
        binding::verify_fetch_with_freshness(&[old], &current_view, &mut freshness),
        &Error::PrekeyGenerationRollback,
    );
    chk(
        out,
        "binding_provider_replay_of_lower_signed_prekey_generation_rejected",
        newer.is_ok() && rollback_ok,
        format!("newer={newer:?}; replay={rollback_detail}"),
    );

    // Equal generation with a different signed-prekey core is authenticated equivocation, not a
    // legitimate provider-pool variant.
    let (new_body, _) = bundle_parts(&new_up.bundle);
    let evil_spk = set_key(
        &new_body,
        7,
        Some(V::M(vec![
            (0, V::U(bob.spk.id as u64)),
            (1, V::U(1)),
            (2, V::B(evil.ik().to_vec())),
        ])),
    );
    let equiv = binding::sign_bundle(&bob.signer, &did, evil_spk, arr32(rng));
    let (eq_ok, eq_detail) = expect_err(
        binding::verify_fetch_with_freshness(&[equiv], &current_view, &mut freshness),
        &Error::PrekeyGenerationEquivocation,
    );
    chk(
        out,
        "binding_equal_generation_different_core_rejected_as_equivocation",
        eq_ok,
        eq_detail,
    );

    let wrong = PeerView {
        acs_body: &acs_b,
        trusted_acs_hash: [7; 32],
        credential_body: &cred_b,
        now: NOW,
    };
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
    let mut routes = vec![];
    let mut kinds = vec![];
    let mut opk_present = vec![];
    for i in 0..5 {
        let mut a = initiator(&tmpl, rng);
        bob.trust_device(&a).unwrap();
        routes.push(a.account_id);
        let f = prov.fetch(&did).expect("fetch");
        let (vb, _) = a.start_session(&f, &view, rng).expect("PQXDH initiate");
        kinds.push(if vb.pq_one_time {
            PqKind::OneTime
        } else {
            PqKind::LastResort
        });
        opk_present.push(vb.opk.is_some());
        envs.push(
            a.encrypt(&bob_ik, format!("hello from alice {i}").as_bytes())
                .expect("encrypt")
                .0,
        );
    }
    let want = [
        PqKind::OneTime,
        PqKind::OneTime,
        PqKind::OneTime,
        PqKind::LastResort,
        PqKind::LastResort,
    ];
    chk(
        out,
        "depletion_falls_back_to_signed_pq_prekey",
        kinds == want,
        format!(
            "pq kinds per fetch: {:?}; ec one-time present: {:?}",
            kinds.iter().map(|k| k.name()).collect::<Vec<_>>(),
            opk_present
        ),
    );
    let mut all_ok = true;
    for (i, e) in envs.iter().enumerate() {
        all_ok &= bob
            .decrypt(&binding::RelationshipContext::new(routes[i]), e, rng)
            .map(|x| x.1 == format!("hello from alice {i}").as_bytes())
            .unwrap_or(false);
    }
    let all_mlkem = envs.iter().all(|e| {
        let v = cbor::decode(e).unwrap();
        v.a().unwrap()[1]
            .get(3)
            .map(|c| {
                c.b()
                    .map(|b| b.len() == pqxdh::MLKEM768_CT_LEN)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    });
    chk(
        out,
        "every_handshake_includes_mlkem_no_classical_only",
        all_ok && all_mlkem && bob.sessions.len() == 5,
        format!(
            "{} sessions; every initial message carries a 1088-byte ML-KEM-768 ciphertext",
            bob.sessions.len()
        ),
    );
    chk(
        out,
        "one_time_private_keys_deleted_after_use",
        bob.opks.is_empty() && bob.pqopks.is_empty() && prov.inventory(&did) == (0, 0),
        format!(
            "device one-time keys left: ec={} pq={}; provider inventory {:?}",
            bob.opks.len(),
            bob.pqopks.len(),
            prov.inventory(&did)
        ),
    );
    chk(
        out,
        "one_time_initiations_do_not_consume_ledger_allowance",
        bob.ledger.count(bob.epoch) == 2,
        format!(
            "ledger entries in epoch {}: {} (only initiations 4 and 5 used no one-time key)",
            bob.epoch,
            bob.ledger.count(bob.epoch)
        ),
    );

    // Replays.
    let snapshot = bob.clone();
    let (ok, d) = match bob.decrypt(&binding::RelationshipContext::new(routes[0]), &envs[0], rng) {
        Err(e @ Error::DecryptFailed) => (true, format!("rejected: {}", e.name())),
        r => (false, format!("{:?}", r.map(|x| x.1))),
    };
    chk(
        out,
        "replay_to_existing_session_rejected_state_unchanged",
        ok && bob == snapshot,
        format!("{d}; state unchanged={}", bob == snapshot),
    );
    let mut fresh = bob.clone();
    fresh.sessions.clear();
    let (ok, d) = expect_err(
        fresh.decrypt(&binding::RelationshipContext::new(routes[0]), &envs[0], rng),
        &Error::UnknownOrConsumedPqPrekey,
    );
    chk(out, "replay_of_consumed_one_time_pq_prekey_rejected", ok, d);
    let (ok, d) = expect_err(
        fresh.decrypt(&binding::RelationshipContext::new(routes[2]), &envs[2], rng),
        &Error::UnknownOrConsumedPqPrekey,
    );
    chk(out, "replay_of_one_time_pq_without_ec_opk_rejected", ok, d);
    let (ok, d) = expect_err(
        fresh.decrypt(&binding::RelationshipContext::new(routes[3]), &envs[3], rng),
        &Error::ReplayedLastResortHandshake,
    );
    let (ok2, _) = expect_err(
        fresh.decrypt(&binding::RelationshipContext::new(routes[4]), &envs[4], rng),
        &Error::ReplayedLastResortHandshake,
    );
    chk(
        out,
        "replay_of_no_one_time_key_handshake_rejected_by_exact_ledger",
        ok && ok2,
        d,
    );
    // EC one-time key + last-resort PQ key: single use comes from the EC key; the ledger is not touched.
    let up = bob.generate_one_time(1, 0, rng);
    prov.publish(&bob, up);
    let mut a5 = initiator(&tmpl, rng);
    bob.trust_device(&a5).unwrap();
    let f5 = prov.fetch(&did).unwrap();
    let (vb5, _) = a5.start_session(&f5, &view, rng).unwrap();
    let env5 = a5
        .encrypt(&bob_ik, b"ec one-time, pq last resort")
        .unwrap()
        .0;
    let n0 = bob.ledger.count(bob.epoch);
    let accepted = bob
        .decrypt(
            &binding::RelationshipContext::new(a5.account_id),
            &env5,
            rng,
        )
        .is_ok();
    let mut fresh = bob.clone();
    fresh.sessions.clear();
    let (ok, d) = expect_err(
        fresh.decrypt(
            &binding::RelationshipContext::new(a5.account_id),
            &env5,
            rng,
        ),
        &Error::UnknownOrConsumedEcPrekey,
    );
    chk(
        out,
        "replay_of_ec_one_time_with_last_resort_pq_rejected",
        accepted
            && !vb5.pq_one_time
            && vb5.opk.is_some()
            && ok
            && bob.ledger.count(bob.epoch) == n0,
        d,
    );

    // Tampered initial message must not consume a one-time key; the genuine one then succeeds once.
    let up = bob.generate_one_time(1, 1, rng);
    prov.publish(&bob, up);
    let mut a6 = initiator(&tmpl, rng);
    bob.trust_device(&a6).unwrap();
    let f6 = prov.fetch(&did).unwrap();
    let (vb6, _) = a6.start_session(&f6, &view, rng).unwrap();
    chk(
        out,
        "replenished_pool_serves_one_time_again",
        vb6.pq_one_time && vb6.opk.is_some(),
        format!("pq_one_time={}", vb6.pq_one_time),
    );
    let env6 = a6.encrypt(&bob_ik, b"six").unwrap().0;
    let mut tampered = cbor::decode(&env6).unwrap();
    if let V::A(ref mut arr) = tampered
        && let V::M(ref mut m) = arr[1]
    {
        for (k, v) in m.iter_mut() {
            if *k == 3
                && let V::B(ct) = v
            {
                ct[10] ^= 0x01;
            }
        }
    }
    let before = (bob.opks.len(), bob.pqopks.len());
    let r = bob.decrypt(
        &binding::RelationshipContext::new(a6.account_id),
        &cbor::encode(&tampered),
        rng,
    );
    let after = (bob.opks.len(), bob.pqopks.len());
    chk(
        out,
        "tampered_initial_message_rejected_without_consuming_keys",
        r.is_err() && before == after && before == (1, 1),
        format!(
            "{}; one-time keys before={before:?} after={after:?}",
            r.err().map(|e| e.name()).unwrap_or_default()
        ),
    );
    let ok = bob
        .decrypt(
            &binding::RelationshipContext::new(a6.account_id),
            &env6,
            rng,
        )
        .map(|x| x.1 == b"six")
        .unwrap_or(false);
    chk(
        out,
        "genuine_initial_message_consumes_exactly_once",
        ok && bob.opks.is_empty() && bob.pqopks.is_empty(),
        "accepted once; keys deleted",
    );

    // Classical-only initial message (ML-KEM ciphertext removed) is rejected by the recipient.
    let mut a7 = initiator(&tmpl, rng);
    bob.trust_device(&a7).unwrap();
    let f7 = prov.fetch(&did).unwrap();
    a7.start_session(&f7, &view, rng).unwrap();
    let env7 = a7.encrypt(&bob_ik, b"seven").unwrap().0;
    let mut noct = cbor::decode(&env7).unwrap();
    if let V::A(ref mut arr) = noct
        && let V::M(ref mut m) = arr[1]
    {
        m.retain(|(k, _)| *k != 3);
    }
    let (ok, d) = expect_err(
        bob.decrypt(
            &binding::RelationshipContext::new(a7.account_id),
            &cbor::encode(&noct),
            rng,
        ),
        &Error::InvalidPqCiphertext,
    );
    chk(
        out,
        "initial_message_without_mlkem_ciphertext_rejected",
        ok,
        d,
    );
    bob.decrypt(
        &binding::RelationshipContext::new(a7.account_id),
        &env7,
        rng,
    )
    .expect("genuine a7");

    // Signed-prekey epoch rotation: grace period, then expiry together with that epoch's ledger.
    let mut a8 = initiator(&tmpl, rng);
    bob.trust_device(&a8).unwrap();
    let f8 = prov.fetch(&did).unwrap();
    a8.start_session(&f8, &view, rng).unwrap();
    let env8 = a8.encrypt(&bob_ik, b"eight").unwrap().0;
    let mut a9 = initiator(&tmpl, rng);
    bob.trust_device(&a9).unwrap();
    a9.start_session(&f8, &view, rng).unwrap();
    let env9 = a9.encrypt(&bob_ik, b"nine").unwrap().0;
    let old_epoch = bob.epoch;
    let up = bob.rotate_signed_prekeys(rng);
    prov.publish(&bob, up);
    let ok = bob
        .decrypt(
            &binding::RelationshipContext::new(a8.account_id),
            &env8,
            rng,
        )
        .is_ok();
    chk(
        out,
        "previous_epoch_accepted_during_grace",
        ok,
        "delayed initiation to the rotated-out signed prekeys accepted",
    );
    bob.expire_previous();
    let (ok, d) = expect_err(
        bob.decrypt(
            &binding::RelationshipContext::new(a9.account_id),
            &env9,
            rng,
        ),
        &Error::UnknownSignedPrekey,
    );
    chk(
        out,
        "expired_epoch_rejected_and_its_ledger_dropped_with_its_secrets",
        ok && bob.ledger.count(old_epoch) == 0,
        d,
    );
    let (acs2, cred2) = prov.view(&did);
    let v2 = binding::verify_fetch(
        &prov.fetch(&did).unwrap(),
        &Device::peer_view(&acs2, &cred2),
    )
    .unwrap();
    chk(
        out,
        "provider_serves_rotated_epoch",
        v2.epoch == bob.epoch && v2.pq_id == bob.last_resort.id,
        format!("epoch {}", v2.epoch),
    );

    let bytes = bob.to_bytes();
    let back = Device::from_bytes(&bytes)
        .map(|d| d.to_bytes() == bytes && d == bob)
        .unwrap_or(false);
    chk(
        out,
        "device_state_deterministic_cbor_roundtrip",
        back,
        format!("{} bytes, {} sessions", bytes.len(), bob.sessions.len()),
    );
}

fn ledger_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let did = bob.device_id;
    let bob_ik = bob.ik();
    let mut prov = Provider::default();
    prov.publish(
        &bob,
        crate::crypto::store::Upload {
            bundle: bob.signed_bundle(rng),
            ..Default::default()
        },
    );
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let tmpl = Device::new(rng);
    let lr_env = |rng: &mut R, prov: &mut Provider, bob: &mut Device| {
        let mut a = initiator(&tmpl, rng);
        bob.trust_device(&a).unwrap();
        a.start_session(&prov.fetch(&did).unwrap(), &view, rng)
            .unwrap();
        (a.encrypt(&bob_ik, b"last resort").unwrap().0, a.account_id)
    };

    // Commit ordering and crash/reload.
    let mut vault = MemVault::default();
    let (e1, e1_route) = lr_env(rng, &mut prov, &mut bob);
    let h1 = {
        let v = cbor::decode(&e1).unwrap();
        pqxdh::handshake_id(
            &did,
            bob.epoch,
            &pqxdh::InitHeader::from_cbor(&v.a().unwrap()[1]).unwrap(),
        )
    };
    vault.fail_next = true;
    let snap = bob.clone();
    let r = bob.receive(
        &binding::RelationshipContext::new(e1_route),
        &e1,
        &mut vault,
        rng,
    );
    chk(
        out,
        "ledger_vault_commit_failure_releases_nothing_and_changes_nothing",
        r.as_ref().err() == Some(&Error::VaultCommitFailed)
            && bob == snap
            && vault.stored.is_none(),
        format!("{:?}", r.err()),
    );
    let r = bob.receive(
        &binding::RelationshipContext::new(e1_route),
        &e1,
        &mut vault,
        rng,
    );
    let durable = vault
        .stored
        .as_ref()
        .and_then(|s| Device::from_bytes(s).ok())
        .map(|d| {
            d.ledger
                .epochs
                .get(&d.epoch)
                .map(|s| s.contains(&h1))
                .unwrap_or(false)
        });
    chk(
        out,
        "ledger_entry_durable_before_plaintext_released",
        r.is_ok() && durable == Some(true),
        "plaintext returned only after the vault held the handshake_id",
    );
    // Crash after commit, before ack: reload from the vault, then the same initiation arrives again.
    let mut reloaded = Device::from_bytes(vault.stored.as_ref().unwrap()).unwrap();
    reloaded.sessions.clear(); // even without the session, the ledger alone rejects it
    let (ok, d) = expect_err(
        reloaded.receive(
            &binding::RelationshipContext::new(e1_route),
            &e1,
            &mut vault,
            rng,
        ),
        &Error::ReplayedLastResortHandshake,
    );
    chk(out, "ledger_replay_rejected_after_crash_and_reload", ok, d);

    // Cap: fill the epoch to exactly 4096 accepted last-resort initiations.
    let mut n = bob.ledger.count(bob.epoch);
    let first = e1.clone();
    while n < LAST_RESORT_ACCEPT_LIMIT {
        let (e, route) = lr_env(rng, &mut prov, &mut bob);
        bob.decrypt(&binding::RelationshipContext::new(route), &e, rng)
            .expect("last-resort accept");
        n += 1;
    }
    let (extra, extra_route) = lr_env(rng, &mut prov, &mut bob);
    let before = bob.clone();
    let (ok, d) = expect_err(
        bob.decrypt(&binding::RelationshipContext::new(extra_route), &extra, rng),
        &Error::LastResortExhausted,
    );
    chk(
        out,
        "ledger_cap_4096_fails_closed",
        ok && bob == before && bob.ledger.count(bob.epoch) == LAST_RESORT_ACCEPT_LIMIT,
        format!("{d}; {} ids held", bob.ledger.count(bob.epoch)),
    );
    let mut again = bob.clone();
    again.sessions.clear();
    let (ok, d) = expect_err(
        again.decrypt(&binding::RelationshipContext::new(e1_route), &first, rng),
        &Error::ReplayedLastResortHandshake,
    );
    chk(
        out,
        "ledger_no_eviction_at_cap_first_id_still_rejected",
        ok,
        d,
    );
    let up = bob.generate_one_time(0, 1, rng);
    prov.publish(&bob, up);
    let mut a = initiator(&tmpl, rng);
    bob.trust_device(&a).unwrap();
    a.start_session(&prov.fetch(&did).unwrap(), &view, rng)
        .unwrap();
    let ot = a.encrypt(&bob_ik, b"one-time at cap").unwrap().0;
    chk(
        out,
        "ledger_one_time_initiation_still_accepted_at_cap",
        bob.decrypt(&binding::RelationshipContext::new(a.account_id), &ot, rng)
            .is_ok(),
        "PQ one-time handshake does not touch the exhausted allowance",
    );
    let old = bob.epoch;
    let up = bob.rotate_signed_prekeys(rng);
    prov.publish(&bob, up);
    let (acs2, cred2) = prov.view(&did);
    let view2 = Device::peer_view(&acs2, &cred2);
    let mut a = initiator(&tmpl, rng);
    bob.trust_device(&a).unwrap();
    a.start_session(&prov.fetch(&did).unwrap(), &view2, rng)
        .unwrap();
    let fresh = a.encrypt(&bob_ik, b"fresh epoch").unwrap().0;
    let ok = bob
        .decrypt(
            &binding::RelationshipContext::new(a.account_id),
            &fresh,
            rng,
        )
        .is_ok();
    let (ok2, d) = expect_err(
        bob.decrypt(&binding::RelationshipContext::new(extra_route), &extra, rng),
        &Error::LastResortExhausted,
    );
    chk(
        out,
        "ledger_fresh_epoch_accepts_old_epoch_stays_exhausted_during_grace",
        ok && ok2 && bob.ledger.count(old) == LAST_RESORT_ACCEPT_LIMIT,
        d,
    );
}

fn aead_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let sk = arr32(rng);
    let bob_spk = arr32(rng);
    let ad = vec![5u8; 66];
    let mut a = SessionState::init_alice(&sk, ad.clone(), &pqxdh::x25519_pub(&bob_spk), rng);
    let mut b = SessionState::init_bob(&sk, ad, &bob_spk);
    let e = a.encrypt(b"aead check", b"ctx").unwrap();
    let hh = ds_hash(crate::crypto::kdf::DR_HEADER_HASH, &e.header);
    let (k, n) = dr_aead_key_nonce(&e.keys.mk, &hh);
    chk(
        out,
        "aead_key_nonce_follow_adr0131_rule3",
        k == e.keys.key && n == e.keys.nonce && hh == e.keys.header_hash,
        "K/N recomputed from MK and the header hash",
    );
    let mut h2 = e.header.clone();
    let last = h2.len() - 1;
    h2[last] ^= 1; // n field
    let snap = b.clone();
    let r = b.decrypt(&h2, &e.ct, b"ctx", rng);
    chk(
        out,
        "aead_header_tamper_rejected_state_unchanged",
        r.is_err() && b == snap,
        format!("{:?}", r.err()),
    );
    let (ok, d) = expect_err(
        b.decrypt(&e.header, &e.ct, b"other envelope", rng),
        &Error::DecryptFailed,
    );
    chk(
        out,
        "aead_envelope_context_is_authenticated",
        ok && b == snap,
        d,
    );
    let ok = b
        .decrypt(&e.header, &e.ct, b"ctx", rng)
        .map(|x| x.0 == b"aead check")
        .unwrap_or(false);
    chk(
        out,
        "aead_genuine_message_accepted",
        ok,
        "AES-256-GCM-SIV open with the derived key/nonce",
    );
    let h = session::parse_header(&e.header).unwrap();
    chk(
        out,
        "dr_header_is_not_encrypted_in_v1",
        h.dh == a.ec.dhs_pub,
        "ratchet public key and counters are plaintext CBOR (ADR-0131 rule 4); V-015 records the exposure",
    );
}

fn hpke_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let seed = arr32(rng);
    let pk = hpke::public_key(&seed);
    let ticket = [9u8; 16];
    let rk_digest = ds_hash(
        "OM-REQUEST-KEY-HASH-v1",
        &cbor::encode(&V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(pk.clone()))])),
    );
    let info = hpke::message_request_info(PROFILE_V1, &ticket, &rk_digest);
    let mut rnd = [0u8; 64];
    rng.fill_bytes(&mut rnd);
    let (ss, enc) = hpke::encaps_derand(&pk, &rnd).unwrap();
    let ct = hpke::key_schedule(&ss, &info).seal(b"", b"signed MessageRequestPayload");
    let ssr = hpke::decaps(&seed, &enc).unwrap();
    let ok = hpke::key_schedule(&ssr, &info)
        .open(b"", &ct)
        .map(|p| p == b"signed MessageRequestPayload")
        .unwrap_or(false);
    chk(
        out,
        "hpke_message_request_roundtrip",
        ok && enc.len() == hpke::NENC,
        format!("enc {} bytes, ct {} bytes", enc.len(), ct.len()),
    );
    let info2 = hpke::message_request_info(PROFILE_V1, &[8u8; 16], &rk_digest);
    let (ok, d) = expect_err(
        hpke::key_schedule(&ssr, &info2).open(b"", &ct),
        &Error::DecryptFailed,
    );
    chk(out, "hpke_info_binds_ticket_id", ok, d);
    let mut bad = enc.clone();
    bad[1100] ^= 1; // X25519 half of enc
    let ss_bad = hpke::decaps(&seed, &bad).unwrap();
    let (ok, d) = expect_err(
        hpke::key_schedule(&ss_bad, &info).open(b"", &ct),
        &Error::DecryptFailed,
    );
    chk(out, "hpke_x25519_half_bound_into_shared_secret", ok, d);
    let (ok, d) = expect_err(
        hpke::decaps(&seed, &enc[..1088]),
        &Error::InvalidPqCiphertext,
    );
    chk(out, "hpke_pq_only_enc_rejected", ok, d);
}

fn sealed_delivery_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let seed = arr32(rng);
    let pk = hpke::public_key(&seed);
    let key_id = [0x31; 16];
    let relay = "https://relay.example.test/deposit";
    let cap = b"opaque-capability-test-only";
    let inner = b"ordinary authenticated OM inner envelope";
    let wrapper = delivery::seal(&pk, &key_id, relay, cap, NOW + 3600, inner, rng).unwrap();
    let opened = delivery::open(&wrapper, &seed, &key_id, relay, cap, NOW).unwrap();
    chk(
        out,
        "sealed_delivery_hpke_roundtrip",
        opened.inner_envelope == inner && opened.outer_hpke_opened,
        format!(
            "opened={}, inner_len={}",
            opened.outer_hpke_opened,
            opened.inner_envelope.len()
        ),
    );
    chk(
        out,
        "sealed_delivery_outer_open_is_not_sender_authentication",
        opened.outer_hpke_opened && !opened.sender_authenticated,
        "sender authentication delegated to mandatory RelationshipContext and inner OM session",
    );

    let (ok, d) = expect_err(
        delivery::open(&wrapper, &seed, &key_id, "https://other.test", cap, NOW),
        &Error::DecryptFailed,
    );
    chk(out, "sealed_delivery_wrong_route_rejected", ok, d);
    let (ok, d) = expect_err(
        delivery::open(&wrapper, &seed, &key_id, relay, b"other-capability", NOW),
        &Error::DecryptFailed,
    );
    chk(
        out,
        "sealed_delivery_tampered_route_bound_info_rejected",
        ok,
        d,
    );
    let wrong_id = [0x32; 16];
    let (ok, d) = expect_err(
        delivery::open(&wrapper, &seed, &wrong_id, relay, cap, NOW),
        &Error::SealedDelivery("key"),
    );
    chk(out, "sealed_delivery_wrong_key_id_rejected", ok, d);
    let wrong_seed = arr32(rng);
    let (ok, d) = expect_err(
        delivery::open(&wrapper, &wrong_seed, &key_id, relay, cap, NOW),
        &Error::DecryptFailed,
    );
    chk(
        out,
        "sealed_delivery_wrong_recipient_private_key_rejected",
        ok,
        d,
    );
    let (ok, d) = expect_err(
        delivery::open(&wrapper, &seed, &key_id, relay, cap, NOW + 3901),
        &Error::SealedDelivery("expired"),
    );
    chk(out, "sealed_delivery_expiry_skew_boundary_rejected", ok, d);

    let mutate = |key: u64, value: V| {
        let mut v = cbor::decode(&wrapper).unwrap();
        if let V::M(ref mut m) = v {
            for (k, x) in m {
                if *k == key {
                    *x = value.clone();
                }
            }
        }
        cbor::encode(&v)
    };
    let id2 = V::B(vec![0x44; 16]);
    let (ok, d) = expect_err(
        delivery::open(&mutate(4, id2), &seed, &key_id, relay, cap, NOW),
        &Error::DecryptFailed,
    );
    chk(
        out,
        "sealed_delivery_tampered_aad_delivery_id_rejected",
        ok,
        d,
    );
    let (ok, d) = expect_err(
        delivery::open(
            &mutate(7, V::U(NOW + 3601)),
            &seed,
            &key_id,
            relay,
            cap,
            NOW,
        ),
        &Error::DecryptFailed,
    );
    chk(out, "sealed_delivery_tampered_aad_expiry_rejected", ok, d);
    let mut enc = cbor::decode(&wrapper)
        .unwrap()
        .get(5)
        .unwrap()
        .b()
        .unwrap()
        .to_vec();
    enc[0] ^= 1;
    let (ok, d) = expect_err(
        delivery::open(&mutate(5, V::B(enc)), &seed, &key_id, relay, cap, NOW),
        &Error::DecryptFailed,
    );
    chk(
        out,
        "sealed_delivery_tampered_hybrid_encapsulation_rejected",
        ok,
        d,
    );
    let mut ct = cbor::decode(&wrapper)
        .unwrap()
        .get(6)
        .unwrap()
        .b()
        .unwrap()
        .to_vec();
    ct[0] ^= 1;
    let (ok, d) = expect_err(
        delivery::open(&mutate(6, V::B(ct)), &seed, &key_id, relay, cap, NOW),
        &Error::DecryptFailed,
    );
    chk(out, "sealed_delivery_tampered_ciphertext_rejected", ok, d);
    let (ok, d) = expect_err(
        delivery::open(
            &mutate(6, V::B(vec![0; 1056])),
            &seed,
            &key_id,
            relay,
            cap,
            NOW,
        ),
        &Error::SealedDelivery("bucket"),
    );
    chk(
        out,
        "sealed_delivery_nonconforming_padding_bucket_rejected",
        ok,
        d,
    );
    let (ok, d) = expect_err(
        delivery::seal_deterministic(
            &pk,
            &key_id,
            &[7; 16],
            relay,
            cap,
            NOW + 60,
            inner,
            &[9; 64],
            &[0; 10],
        ),
        &Error::SealedDelivery("padding"),
    );
    chk(
        out,
        "sealed_delivery_invalid_padding_length_rejected",
        ok,
        d,
    );
    let mut bucket_lengths = Vec::new();
    for n in [
        12usize, 1020, 1021, 4092, 4093, 16380, 16381, 65532, 65533, 131068,
    ] {
        let msg = vec![0x5a; n];
        let w = delivery::seal(&pk, &key_id, relay, cap, NOW + 3600, &msg, rng).unwrap();
        let parsed = cbor::decode(&w).unwrap();
        let got = parsed.get(6).unwrap().b().unwrap().len();
        let opened = delivery::open(&w, &seed, &key_id, relay, cap, NOW).unwrap();
        bucket_lengths.push(got);
        if opened.inner_envelope != msg {
            bucket_lengths.push(0);
        }
    }
    chk(
        out,
        "sealed_delivery_all_fixed_plaintext_buckets_roundtrip",
        bucket_lengths
            == vec![
                1040, 1040, 4112, 4112, 16400, 16400, 65552, 65552, 131088, 131088,
            ],
        format!("ciphertext lengths={bucket_lengths:?}"),
    );
    let unsealed = cbor::encode(&V::A(vec![V::T("ordinary clear-sender envelope".into())]));
    chk(
        out,
        "sealed_delivery_no_clear_sender_fallback",
        delivery::open(&unsealed, &seed, &key_id, relay, cap, NOW).is_err(),
        "relay receive boundary requires object type 23 and never accepts clear-sender fallback",
    );
}

fn adr0140_persistence_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let mut alice = Device::new(rng);
    let acs = bob.acs().body();
    let cred = bob.credential().body();
    let view = Device::peer_view(&acs, &cred);
    let pool_a = [0xa1; 16];
    let pool_b = [0xb2; 16];
    let first = bob.generate_one_time_for_pool(pool_a, 0, 0, rng);
    let migrated = bob.generate_one_time_for_pool(pool_b, 0, 0, rng);
    let r1 = alice.start_session(std::slice::from_ref(&first.bundle), &view, rng);
    let r2 = alice.start_session(std::slice::from_ref(&migrated.bundle), &view, rng);
    chk(
        out,
        "adr0140_cross_provider_migration_same_generation_core_accepted",
        r1.is_ok() && r2.is_ok(),
        format!("provider A={}; provider B={}", r1.is_ok(), r2.is_ok()),
    );

    let mut third = bob.rotate_signed_prekeys(rng);
    let r3 = alice.start_session(&[third.bundle.clone()], &view, rng);
    let before = alice.bundle_freshness.floors.len();
    let serialized = alice.to_bytes();
    let mut reloaded = Device::from_bytes(&serialized).unwrap();
    let persisted = reloaded.bundle_freshness.floors == alice.bundle_freshness.floors;
    chk(
        out,
        "adr0140_generation_floor_survives_device_reload",
        r3.is_ok() && persisted && before > 0,
        format!("accepted_new_generation={}; entries={before}", r3.is_ok()),
    );

    let mut replayed = reloaded.clone();
    let rollback = replayed.start_session(std::slice::from_ref(&first.bundle), &view, rng);
    chk(
        out,
        "adr0140_lower_generation_replay_rejected_after_reload",
        matches!(rollback, Err(Error::PrekeyGenerationRollback)),
        format!("{rollback:?}"),
    );

    // Same authenticated generation with a different signed-prekey core is equivocation even after restore.
    let (body, _) = bundle_parts(&third.bundle);
    let changed = set_key(
        &body,
        7,
        Some(V::M(vec![
            (0, V::U(9999)),
            (1, V::U(1)),
            (2, V::B([0x77; 32].to_vec())),
        ])),
    );
    third.bundle = binding::sign_bundle(&bob.signer, &bob.device_id, changed, arr32(rng));
    let equiv = reloaded.start_session(&[third.bundle], &view, rng);
    chk(
        out,
        "adr0140_equal_generation_core_equivocation_rejected_after_reload",
        matches!(equiv, Err(Error::PrekeyGenerationEquivocation)),
        format!("{equiv:?}"),
    );

    // Device/profile/generation/pool context is covered by the signed one-time item.
    let pool_a_upload = bob.generate_one_time_for_pool(pool_a, 0, 1, rng);
    let pool_b_upload = bob.generate_one_time_for_pool(pool_b, 0, 0, rng);
    let item_a = pool_a_upload.pqopks[0].clone();
    let pool_b_view = Device::peer_view(&acs, &cred);
    let mut fresh = BundleFreshness::default();
    let accepted = binding::verify_fetch_with_freshness(
        std::slice::from_ref(&pool_b_upload.bundle),
        &pool_b_view,
        &mut fresh,
    );
    let transplanted = binding::verify_fetch_with_freshness(
        &[pool_b_upload.bundle, item_a.clone()],
        &pool_b_view,
        &mut fresh,
    );
    chk(
        out,
        "adr0140_one_time_prekey_provider_pool_transplant_rejected",
        accepted.is_ok() && matches!(transplanted, Err(Error::BadSignature)),
        format!(
            "accepted_current_pool={}; transplant={transplanted:?}",
            accepted.is_ok()
        ),
    );

    // Same item bytes cannot be rebound to another device/profile/generation by provider relabeling.
    let (bbody, _) = bundle_parts(&pool_a_upload.bundle);
    let changed_generation = set_key(&bbody, 6, Some(V::U(99)));
    let rebound = binding::sign_bundle(&bob.signer, &bob.device_id, changed_generation, arr32(rng));
    let transplanted_generation = binding::verify_fetch(&[rebound, item_a.clone()], &pool_b_view);
    chk(
        out,
        "adr0140_one_time_prekey_generation_transplant_rejected",
        transplanted_generation.is_err(),
        format!("{transplanted_generation:?}"),
    );

    let mut other = Device::new(rng);
    let other_upload = other.generate_one_time_for_pool(pool_a, 0, 0, rng);
    let other_acs = other.acs().body();
    let other_cred = other.credential().body();
    let other_view = Device::peer_view(&other_acs, &other_cred);
    let cross_device = binding::verify_fetch(&[other_upload.bundle, item_a.clone()], &other_view);
    chk(
        out,
        "adr0140_one_time_prekey_device_transplant_rejected",
        cross_device.is_err(),
        format!("{cross_device:?}"),
    );
    let (profile_body, _) = bundle_parts(&pool_a_upload.bundle);
    let profile2 = set_key(&profile_body, 5, Some(V::U(2)));
    let profile_bundle = binding::sign_bundle(&bob.signer, &bob.device_id, profile2, arr32(rng));
    let cross_profile = binding::verify_fetch(&[profile_bundle, item_a], &pool_b_view);
    chk(
        out,
        "adr0140_one_time_prekey_profile_transplant_rejected",
        cross_profile.is_err(),
        format!("{cross_profile:?}"),
    );
}

/// ADR-0136 rule 4 (review F-4/F-9): the responder binds IK_A to exactly one active, non-quarantined
/// DeviceCredential of a trusted relationship peer before any secret-key operation.
fn initiator_binding_checks<R: RngCore + CryptoRng>(out: &mut Vec<Check>, rng: &mut R) {
    let mut bob = Device::new(rng);
    let did = bob.device_id;
    let bob_ik = bob.ik();
    let mut prov = Provider::default();
    let up = bob.generate_one_time(16, 16, rng);
    prov.publish(&bob, up);
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let mut send = |a: &mut Device, rng: &mut R| {
        a.sessions.clear();
        a.start_session(&prov.fetch(&did).unwrap(), &view, rng)
            .unwrap();
        a.encrypt(&bob_ik, b"hello bob").unwrap().0
    };
    let mut alice = Device::new(rng);
    let mut carol = Device::new(rng);
    let mut mallory = Device::new(rng);
    let rejected_on_route = |b: &Device, env: &[u8], route: [u8; 32], rng: &mut R| {
        let mut t = b.clone();
        let r = t.decrypt(&binding::RelationshipContext::new(route), env, rng);
        (
            matches!(r, Err(Error::Binding(_))) && t == *b,
            format!("{:?}; state unchanged={}", r.as_ref().err(), t == *b),
        )
    };

    let env_a = send(&mut alice, rng);
    let (ok, d) = rejected_on_route(&bob, &env_a, alice.account_id, rng);
    chk(
        out,
        "initiator_unknown_identity_key_rejected_nothing_consumed",
        ok,
        d,
    );

    bob.trust_device(&alice).unwrap();
    bob.trust_device(&carol).unwrap();
    let ok = bob
        .clone()
        .decrypt(
            &binding::RelationshipContext::new(alice.account_id),
            &env_a,
            rng,
        )
        .map(|x| x.1 == b"hello bob")
        .unwrap_or(false);
    chk(
        out,
        "initiator_bound_to_trusted_peer_credential_accepted",
        ok,
        "IK_A is the key of an active credential committed by the peer's trusted ACS head",
    );
    let (ok, d) = rejected_on_route(&bob, &env_a, carol.account_id, rng);
    let ok2 = bob
        .clone()
        .decrypt(
            &binding::RelationshipContext::new(alice.account_id),
            &env_a,
            rng,
        )
        .is_ok();
    chk(
        out,
        "initiator_pinned_to_relationship_route",
        ok && ok2,
        format!("carol's route: {d}; alice's route accepted={ok2}"),
    );

    // Established sessions remain route-bound too. Complete the first exchange so Alice's
    // follow-up carries no PQXDH prefix, then try that ordinary ratchet message on Carol's route.
    let mut established_bob = bob.clone();
    let initial_ok = established_bob
        .decrypt(
            &binding::RelationshipContext::new(alice.account_id),
            &env_a,
            rng,
        )
        .is_ok();
    let reply = established_bob
        .encrypt(&alice.ik(), b"binding settle")
        .unwrap()
        .0;
    let reply_ok = alice
        .decrypt(
            &binding::RelationshipContext::new(established_bob.account_id),
            &reply,
            rng,
        )
        .is_ok();
    let follow = alice
        .encrypt(&bob_ik, b"established route binding")
        .unwrap()
        .0;
    let follow_v = cbor::decode(&follow).unwrap();
    let no_init = follow_v.a().unwrap()[1].is_null();
    let mut established_wrong = established_bob.clone();
    let before = established_wrong.clone();
    let r = established_wrong.decrypt(
        &binding::RelationshipContext::new(carol.account_id),
        &follow,
        rng,
    );
    let wrong_ok = matches!(r, Err(Error::Binding(_))) && established_wrong == before;
    chk(
        out,
        "initiator_established_session_wrong_route_rejected_state_unchanged",
        initial_ok && reply_ok && no_init && wrong_ok,
        format!(
            "initial={initial_ok}; reply={reply_ok}; no_init={no_init}; wrong={:?}; unchanged={}",
            r.as_ref().err(),
            established_wrong == before
        ),
    );
    let established_ok = established_wrong
        .decrypt(
            &binding::RelationshipContext::new(alice.account_id),
            &follow,
            rng,
        )
        .map(|x| x.1 == b"established route binding")
        .unwrap_or(false);
    chk(
        out,
        "initiator_established_session_correct_route_accepted",
        established_ok,
        format!("accepted={established_ok}"),
    );

    // Persistence cannot erase the relationship pin: peer heads are serialized and the derived
    // DeviceCredential index is rebuilt during from_bytes() before receive() can be called.
    let bytes = established_bob.to_bytes();
    let mut reloaded = Device::from_bytes(&bytes).unwrap();
    let reloaded_before = reloaded.to_bytes();
    let mut wrong_vault = MemVault::default();
    let r = reloaded.receive(
        &binding::RelationshipContext::new(carol.account_id),
        &follow,
        &mut wrong_vault,
        rng,
    );
    let reload_wrong_ok = matches!(r, Err(Error::Binding(_)))
        && reloaded.to_bytes() == reloaded_before
        && wrong_vault.stored.is_none();
    chk(
        out,
        "initiator_reloaded_established_session_wrong_route_rejected_before_commit",
        reload_wrong_ok,
        format!(
            "wrong={:?}; unchanged={}; committed={}",
            r.as_ref().err(),
            reloaded.to_bytes() == reloaded_before,
            wrong_vault.stored.is_some()
        ),
    );
    let mut good_vault = MemVault::default();
    let reload_good = reloaded
        .receive(
            &binding::RelationshipContext::new(alice.account_id),
            &follow,
            &mut good_vault,
            rng,
        )
        .map(|x| x.1 == b"established route binding")
        .unwrap_or(false);
    let reloaded_after = reloaded.to_bytes();
    let reload_committed = good_vault.stored.as_deref() == Some(reloaded_after.as_slice());
    chk(
        out,
        "initiator_reloaded_established_session_correct_route_accepted_and_committed",
        reload_good && reload_committed,
        format!("accepted={reload_good}; committed={reload_committed}"),
    );

    // Ambiguous: two active credentials of the same trusted AccountID commit the same IK_A.
    // Even though the route is correct, the binding must fail closed rather than choose one.
    let mut amb = bob.clone();
    let mut dup = alice.credential();
    dup.device_id = carol.device_id;
    let mut aacs = alice.acs();
    aacs.generation += 1;
    aacs.messaging.insert(dup.device_id);
    aacs.credentials.push((dup.device_id, dup.hash()));
    amb.trust_peer(aacs.body(), vec![alice.credential().body(), dup.body()])
        .unwrap();
    let (amb_ok, amb_d) = rejected_on_route(&amb, &env_a, alice.account_id, rng);
    chk(
        out,
        "initiator_ambiguous_same_account_identity_key_rejected",
        amb_ok,
        amb_d,
    );

    // Revoked and quarantined at the trusted head.
    for (name, revoke) in [
        ("initiator_revoked_device_rejected", true),
        ("initiator_quarantined_device_rejected", false),
    ] {
        let mut b = bob.clone();
        let mut acs = alice.acs();
        acs.generation += 1;
        if revoke {
            acs.control.remove(&alice.device_id);
            acs.revoked.insert(alice.device_id);
        } else {
            acs.quarantined.insert(alice.device_id);
        }
        b.trust_peer(acs.body(), vec![alice.credential().body()])
            .unwrap();
        let (ok, d) = rejected_on_route(&b, &env_a, alice.account_id, rng);
        chk(out, name, ok, d);
    }

    // Stale: alice rotates her identity key (generation 2); the old key no longer binds, and K7 refuses the old head.
    let mut b = bob.clone();
    let mut alice2 = alice.clone();
    alice2.ik_secret = arr32(rng);
    alice2.acs_generation = 2;
    b.trust_device(&alice2).unwrap();
    let (ok, d) = rejected_on_route(&b, &env_a, alice.account_id, rng);
    let k7 = matches!(b.trust_device(&alice), Err(Error::Binding(_)));
    let ok2 = b
        .clone()
        .decrypt(
            &binding::RelationshipContext::new(alice2.account_id),
            &send(&mut alice2, rng),
            rng,
        )
        .is_ok();
    chk(
        out,
        "initiator_stale_credential_rejected_after_rotation",
        ok && k7 && ok2,
        format!("{d}; older head refused={k7}; new key accepted={ok2}"),
    );

    // Substituted: a credential body claiming mallory's key under alice's account is not committed by her head.
    let mut b = bob.clone();
    let mut forged = alice.credential();
    forged.ik = mallory.ik();
    b.trust_peer(
        alice.acs().body(),
        vec![alice.credential().body(), forged.body()],
    )
    .unwrap();
    mallory.account_id = alice.account_id;
    mallory.device_id = alice.device_id;
    let env_m = send(&mut mallory, rng);
    let (ok, d) = rejected_on_route(&b, &env_m, alice.account_id, rng);
    chk(out, "initiator_substituted_identity_key_rejected", ok, d);

    // The same IK may appear under two independently trusted relationships; mandatory route context
    // prevents one relationship from authenticating traffic delivered on the other.
    let mut b = bob.clone();
    let mut cc = carol.credential();
    cc.ik = alice.ik();
    let mut cacs = carol.acs();
    cacs.generation += 1;
    cacs.credentials = vec![(carol.device_id, cc.hash())];
    b.trust_peer(cacs.body(), vec![cc.body()]).unwrap();
    let alice_ok = b
        .clone()
        .decrypt(
            &binding::RelationshipContext::new(alice.account_id),
            &env_a,
            rng,
        )
        .is_ok();
    let carol_ok = b
        .clone()
        .decrypt(
            &binding::RelationshipContext::new(carol.account_id),
            &env_a,
            rng,
        )
        .is_ok();
    chk(
        out,
        "initiator_same_ik_across_relationships_resolved_by_mandatory_route",
        alice_ok && carol_ok,
        format!(
            "each trusted relationship independently commits the IK; alice route={alice_ok}, carol route={carol_ok}"
        ),
    );

    // Crossed initiation between bound peers still reaches the session rule (SessionConflict), not the binding.
    let mut b = bob.clone();
    carol.trust_device(&b).unwrap();
    let mut cprov = Provider::default();
    let up = carol.generate_one_time(1, 1, rng);
    cprov.publish(&carol, up);
    let (acs_c, cred_c) = cprov.view(&carol.device_id);
    b.start_session(
        &cprov.fetch(&carol.device_id).unwrap(),
        &Device::peer_view(&acs_c, &cred_c),
        rng,
    )
    .unwrap();
    let env_c = send(&mut carol, rng);
    let (ok, d) = expect_err(
        b.decrypt(
            &binding::RelationshipContext::new(carol.account_id),
            &env_c,
            rng,
        ),
        &Error::SessionConflict,
    );
    chk(
        out,
        "initiator_crossed_initiation_between_bound_peers_reaches_session_rule",
        ok,
        d,
    );
}

pub fn run<R: RngCore + CryptoRng>(rng: &mut R) -> Vec<Check> {
    let mut out = vec![];
    binding_checks(&mut out, rng);
    initiator_binding_checks(&mut out, rng);
    otk_checks(&mut out, rng);
    ledger_checks(&mut out, rng);
    aead_checks(&mut out, rng);
    hpke_checks(&mut out, rng);
    sealed_delivery_checks(&mut out, rng);
    adr0140_persistence_checks(&mut out, rng);
    out
}
