// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::v3::record::{Outcome, classify_v2, maximum_window};
use om_core::crypto::session::{aead_open, aead_seal};
use om_core::crypto::sig::digest;
use om_core::recovery::STATE_OFF_EXPLICIT;
use om_core::recovery_record::{self as record, Freshness, RecordKeys};
use om_core::recovery_sheet::OwnerCheckpoint;

const RECORD_AD: &str = "OM-RECOVERY-RECORD-AEAD-AD-v2"; // ADR-0248 provisional label.

fn associated_data(keys: &RecordKeys) -> Vec<u8> {
    encode(&array([
        string(RECORD_AD),
        bytes(SCOPE),
        bytes(keys.locator),
        V::U(2),
    ]))
}

fn pack(evidence: V, head: &Acs, epoch: u64) -> V {
    map([
        (0, V::U(36)),
        (1, V::U(2)),
        (2, V::U(head.generation << 20)),
        (3, V::U(epoch)),
        (
            4,
            map([(0, V::U(head.generation)), (1, digest(&head.hash()))]),
        ),
        (5, evidence),
        (
            6,
            map([(0, array([map([(0, string(HOST)), (2, bytes(SCOPE))])]))]),
        ),
        (7, map([(0, array([]))])),
    ])
}

fn padded(plain: &V, bucket: usize) -> Vec<u8> {
    let length = record::pad_length(encode(plain).len(), bucket).unwrap();
    let wire = encode(&with_field(plain, 22, Some(bytes(vec![0; length]))));
    assert_eq!(wire.len(), bucket);
    wire
}

fn evaluate(pin: OwnerCheckpoint, wire: &[u8]) -> Outcome {
    fn inspect(pin: OwnerCheckpoint, wire: &[u8]) -> Option<Outcome> {
        if wire.len() > record::MAX_PLAINTEXT_BYTES {
            return None;
        }
        let record = decode(wire).ok()?;
        if record.opt(0) != Some(&V::U(36)) || record.opt(1) != Some(&V::U(2)) {
            return None;
        }
        let evidence = record.opt(5)?;
        let (head, _) = signed_state(required_bytes(evidence, 0)?)?;
        if record.opt(4)? != &map([(0, V::U(head.generation)), (1, digest(&head.hash()))])
            || record::generation_acs(record.get(2).ok()?.u().ok()?) != head.generation
        {
            return None;
        }
        Some(classify_v2(pin, evidence, wire.len()))
    }
    inspect(pin, wire).unwrap_or(Outcome::IntegrityFailure)
}

fn seal(keys: &RecordKeys, nonce: [u8; 12], ad: &[u8], plain: &[u8]) -> Vec<u8> {
    let ciphertext = aead_seal(&keys.record_key, &nonce, ad, plain);
    encode(&map([
        (0, V::U(35)),
        (1, V::U(2)),
        (2, V::U(PROFILE_V1)),
        (3, bytes(nonce)),
        (4, bytes(ciphertext)),
    ]))
}

fn open(keys: &RecordKeys, wire: &[u8]) -> Option<Vec<u8>> {
    if wire.len() > record::MAX_ENVELOPE_BYTES {
        return None;
    }
    let envelope = decode(wire).ok()?;
    if envelope.opt(0) != Some(&V::U(35))
        || envelope.opt(1) != Some(&V::U(2))
        || envelope.opt(2) != Some(&V::U(PROFILE_V1))
    {
        return None;
    }
    let nonce = required_bytes(&envelope, 3)?.try_into().ok()?;
    let ciphertext = required_bytes(&envelope, 4)?;
    if !record::BUCKETS.contains(&ciphertext.len().checked_sub(16)?) {
        return None;
    }
    aead_open(&keys.record_key, &nonce, &associated_data(keys), ciphertext).ok()
}

fn run(state: &Acs, policy: &RecoveryPolicy) -> V {
    map([(0, bytes(state.hash())), (1, bytes(policy.body()))])
}

fn pair(
    start: Option<&RecoveryPolicy>,
    end: Option<&RecoveryPolicy>,
    epoch: u64,
) -> (OwnerCheckpoint, V) {
    let f = Fixture::new();
    let mut q = f.parent.clone();
    q.recovery_commitment = start.map(RecoveryPolicy::hash);
    let mut p = q.clone();
    p.generation += 1;
    p.previous = Some(q.hash());
    p.recovery_commitment = end.map(RecoveryPolicy::hash);
    let mut credentials = f.credentials();
    if start == end {
        p.messaging.insert(TARGET);
        p.credentials.push((TARGET, f.target.hash()));
        credentials.push(f.target.body());
    }
    let sign = |state: &Acs, rnd| {
        let body = state.body();
        sig::envelope(
            body.clone(),
            vec![
                vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, rnd),
                vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &body, rnd + 1),
            ],
        )
    };
    let mut runs = Vec::new();
    if let Some(policy) = start {
        runs.push(run(&q, policy));
    }
    if let Some(policy) = end.filter(|p| Some(*p) != start) {
        runs.push(run(&p, policy));
    }
    let evidence = map([
        (0, bytes(sign(&p, 73))),
        (1, array([bytes(sign(&q, 71))])),
        (2, array(credentials.iter().map(bytes))),
        (3, end.map_or(V::Null, |p| bytes(p.body()))),
    ]);
    let evidence = with_field(&evidence, 6, (!runs.is_empty()).then(|| array(runs)));
    (checkpoint(&q), pack(evidence, &p, epoch))
}

fn off_policy(f: &Fixture) -> RecoveryPolicy {
    let mut policy = f.current_policy.clone();
    policy.state = STATE_OFF_EXPLICIT;
    policy.methods.clear();
    policy
}

#[test]
fn rk2e_01_policy_removal_retains_historical_run_and_authenticates_v2() {
    let v = vector("RK2E-01");
    let f = Fixture::new();
    let (pin, record) = pair(Some(&f.old_policy), None, OLD_EPOCH);
    assert_eq!(pin.to_bytes(), raw(&v, "checkpoint"));
    let wire = encode(&record);
    assert_eq!(wire, raw(&v, "plaintext"));
    assert_eq!(evaluate(pin, &wire), Outcome::VerifiedOrdinaryPrefix);
    assert_eq!(v["expected"], "verified_read_only");
    let evidence = record.get(5).unwrap();
    assert_eq!(evidence.opt(3), Some(&V::Null));
    assert_eq!(evidence.get(6).unwrap().a().unwrap().len(), 1);
    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    let ad = associated_data(&keys);
    assert_eq!(ad, raw(&v, "ad"));
    let padded = padded(&record, 65_536);
    assert_eq!(hex(&sha256(&[&padded])), v["padded_sha256"]);
    let envelope = seal(&keys, [0xa4; 12], &ad, &padded);
    assert_eq!(envelope, raw(&v, "envelope"));
    let opened = open(&keys, &envelope).unwrap();
    assert_eq!(opened, padded);
    assert_eq!(evaluate(pin, &opened), Outcome::VerifiedOrdinaryPrefix);
}

#[test]
fn rk2e_02_absence_is_distinct_from_off_explicit() {
    let v = vector("RK2E-02");
    let f = Fixture::new();
    let off = off_policy(&f);
    for (name, (pin, record)) in [
        ("no_policy", pair(None, None, 0)),
        ("off_explicit", pair(None, Some(&off), EPOCH)),
    ] {
        assert_eq!(pin.to_bytes(), raw(&v[name], "checkpoint"));
        let wire = encode(&record);
        assert_eq!(wire, raw(&v[name], "plaintext"));
        assert_eq!(evaluate(pin, &wire), Outcome::VerifiedOrdinaryPrefix);
        let e = record.get(5).unwrap();
        if name == "no_policy" {
            assert_eq!(e.opt(3), Some(&V::Null));
            assert!(e.opt(6).is_none());
        } else {
            assert_eq!(required_bytes(e, 3).unwrap(), off.body());
            assert_eq!(e.get(6).unwrap().a().unwrap().len(), 1);
        }
    }
}

#[test]
fn rk2e_03_policy_mismatches_evidence_gaps_and_version_swaps() {
    let v = vector("RK2E-03");
    let f = Fixture::new();
    let (pin, removed) = pair(Some(&f.old_policy), None, OLD_EPOCH);
    let (off_pin, off) = pair(None, Some(&off_policy(&f)), EPOCH);
    let (absent_pin, absent) = pair(None, None, 0);
    let mutate = |record: &V, key, value| {
        encode(&with_field(
            record,
            5,
            Some(with_field(record.get(5).unwrap(), key, value)),
        ))
    };
    let run = removed.get(5).unwrap().get(6).unwrap().clone();
    for (name, pin, wire, expected) in [
        (
            "missing_key3",
            pin,
            mutate(&removed, 3, None),
            Outcome::NeedsEvidence,
        ),
        (
            "null_with_commitment",
            off_pin,
            mutate(&off, 3, Some(V::Null)),
            Outcome::IntegrityFailure,
        ),
        (
            "body_without_commitment",
            pin,
            mutate(&removed, 3, Some(bytes(f.old_policy.body()))),
            Outcome::IntegrityFailure,
        ),
        (
            "fabricated_run",
            absent_pin,
            mutate(&absent, 6, Some(run)),
            Outcome::IntegrityFailure,
        ),
        (
            "missing_historical_run",
            pin,
            mutate(&removed, 6, None),
            Outcome::NeedsEvidence,
        ),
    ] {
        assert_eq!(evaluate(pin, &wire), expected, "{name}");
        check_bytes(
            &v[name],
            &wire,
            if expected == Outcome::NeedsEvidence {
                "needs_evidence"
            } else {
                "integrity_failure"
            },
        );
    }
    let e = absent.get(5).unwrap();
    let (head, _) = signed_state(required_bytes(e, 0).unwrap()).unwrap();
    let body = head.body();
    let signed = sig::envelope(
        body.clone(),
        vec![vote(&f.old_key, ROLE_RECOVERY, &ACCOUNT, &body, 80)],
    );
    let damaged = mutate(&absent, 0, Some(bytes(signed)));
    assert_eq!(evaluate(absent_pin, &damaged), Outcome::IntegrityFailure);

    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    let aad = associated_data(&keys);
    let v1_ad = record::associated_data(&SCOPE, &keys.locator);
    let plain = padded(&removed, 65_536);
    let normal = seal(&keys, [0xa4; 12], &aad, &plain);
    let wrong_ad = seal(&keys, [0xa5; 12], &v1_ad, &plain);
    let decoded = decode(&normal).unwrap();
    let changed_nonce = encode(&with_field(&decoded, 3, Some(bytes([0xa6; 12]))));
    assert!(open(&keys, &wrong_ad).is_none());
    assert!(open(&keys, &changed_nonce).is_none());
    check_bytes(&v["wrong_ad"], &wrong_ad, "authentication_failure");
    check_bytes(&v["bad_nonce"], &changed_nonce, "authentication_failure");
    let changed_envelope_version = encode(&with_field(&decoded, 1, Some(V::U(1))));
    check_bytes(
        &v["wrong_envelope_version"],
        &changed_envelope_version,
        "refused",
    );
    assert!(open(&keys, &changed_envelope_version).is_none());
    assert!(record::open_envelope(&keys, &SCOPE, &changed_envelope_version).is_err());
    let wrong_plain_version = encode(&with_field(&removed, 1, Some(V::U(1))));
    check_bytes(
        &v["wrong_plaintext_version"],
        &wrong_plain_version,
        "refused",
    );
    assert_eq!(
        evaluate(pin, &wrong_plain_version),
        Outcome::IntegrityFailure
    );
    assert!(record::RecoveryRecord::from_bytes(&wrong_plain_version).is_err());
}

#[test]
fn rk2e_04_cap_floors_legacy_dispatch_and_writer_eligibility() {
    let v = vector("RK2E-04");
    let f = Fixture::new();
    let (pin, head, evidence) = maximum_window(&f);
    let plain = pack(evidence.clone(), &head, 65);
    let cap = record::MAX_PLAINTEXT_BYTES;
    let wire = padded(&plain, cap);
    assert_eq!(v["plaintext"], wire.len());
    assert_eq!(v["plaintext_sha256"], hex(&sha256(&[&wire])));
    for (name, field) in [("segment", 4), ("tail", 1), ("runs", 6), ("credentials", 2)] {
        assert_eq!(v[name], evidence.get(field).unwrap().a().unwrap().len());
    }
    assert_eq!(v["head"], 1);
    assert_eq!(v["credential_bytes"], record::MAX_CREDENTIAL_BYTES_TOTAL);
    assert_eq!(evaluate(pin, &wire), Outcome::VerifiedOrdinaryPrefix);
    assert_eq!(
        evaluate(pin, &padded(&plain, cap + 1)),
        Outcome::IntegrityFailure
    );
    let mut runs = evidence.get(6).unwrap().a().unwrap().to_vec();
    runs.push(runs[0].clone());
    let over_runs = pack(with_field(&evidence, 6, Some(array(runs))), &head, 65);
    assert_eq!(
        evaluate(pin, &encode(&over_runs)),
        Outcome::IntegrityFailure
    );

    let generation = plain.get(2).unwrap().u().unwrap();
    let before_restart = (generation, record::plaintext_digest(&wire));
    let restarted = Some(before_restart);
    assert_eq!(
        record::freshness(restarted, generation - 1, &wire),
        Freshness::Stale
    );
    assert_eq!(
        record::freshness(restarted, generation, &wire),
        Freshness::Same
    );
    let mut shifted = wire.clone();
    *shifted.last_mut().unwrap() ^= 1;
    assert_eq!(
        record::freshness(restarted, generation, &shifted),
        Freshness::Equivocation
    );
    assert_eq!(
        record::freshness(restarted, generation + 1, &shifted),
        Freshness::Newer
    );

    let (legacy_pin, legacy) = pair(Some(&f.old_policy), Some(&f.old_policy), OLD_EPOCH);
    let legacy_evidence = with_field(legacy.get(5).unwrap(), 6, None);
    let legacy = with_field(
        &with_field(&legacy, 5, Some(legacy_evidence)),
        1,
        Some(V::U(1)),
    );
    let legacy_plain = padded(&legacy, 65_536);
    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    let legacy_envelope = record::seal_envelope(&keys, &SCOPE, [0xa7; 12], &legacy_plain).unwrap();
    assert!(record::open(&keys, &SCOPE, &legacy_envelope).is_ok());
    assert!(open(&keys, &legacy_envelope).is_none());
    assert_eq!(
        evaluate(legacy_pin, &legacy_plain),
        Outcome::IntegrityFailure
    );
    assert!(record::read_plaintext(&wire).is_err());
    assert_eq!(v["old_reader"], "refused");

    let (pin, removed) = pair(Some(&f.old_policy), None, OLD_EPOCH);
    let read_only = evaluate(pin, &encode(&removed));
    assert_eq!(read_only, Outcome::VerifiedOrdinaryPrefix);
    let read_head = signed_state(required_bytes(removed.get(5).unwrap(), 0).unwrap())
        .unwrap()
        .0;
    let ready = read_only == Outcome::VerifiedOrdinaryPrefix
        && read_head.recovery_commitment == Some(f.old_policy.hash());
    assert!(!ready);
    assert_eq!(v["policy_free_writer"], "ineligible");
}
