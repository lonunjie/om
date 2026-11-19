// SPDX-License-Identifier: Apache-2.0

use super::*;
use om_core::control::{
    RecoveryObservation, RecoveryVerdict, has_recovery_signature, verify_recovery_successor,
    verify_successor,
};
use om_core::crypto::cbor::decode;
use om_core::crypto::sha256;
use om_core::crypto::sig::{ROLE_RECOVERY, digest, ds_sig};
use om_core::recovery::{LBL_REQUEST_SIG, RecoveryRequest};
use om_core::recovery_record as record;
use om_core::recovery_sheet::{OwnerCheckpoint, Parsed, parse};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
enum Classification {
    OrdinaryPrefix,
    NeedsEvidence,
    OlderSheet,
    IntegrityFailure,
}

fn checkpoint(acs: &Acs) -> OwnerCheckpoint {
    OwnerCheckpoint {
        account_id: acs.account_id,
        generation: acs.generation,
        acs_hash: acs.hash(),
    }
}

fn state_from_signed(raw: &[u8]) -> Option<Acs> {
    let envelope = decode(raw).ok()?;
    if envelope.m().ok()?.len() != 2 {
        return None;
    }
    Acs::from_body(required_bytes(&envelope, 1)?).ok()
}

fn optional_array(value: &V, key: u64) -> Option<&[V]> {
    value.opt(key).map_or(Some(&[]), |v| v.a().ok())
}

fn bounded(evidence: &V, plaintext_size: usize) -> Option<()> {
    let segment = optional_array(evidence, 4)?;
    let tail = evidence.get(1).ok()?.a().ok()?;
    let credentials = evidence.get(2).ok()?.a().ok()?;
    let dossiers = optional_array(evidence, 5)?;
    let credential_bytes = credentials
        .iter()
        .try_fold(0usize, |n, cred| n.checked_add(cred.b().ok()?.len()))?;
    (segment.len() <= 56
        && tail.len() <= 8
        && segment.len() + tail.len() < 65
        && credentials.len() <= record::MAX_CREDENTIALS
        && credential_bytes <= record::MAX_CREDENTIAL_BYTES_TOTAL
        && dossiers.len() <= 64
        && plaintext_size <= record::MAX_PLAINTEXT_BYTES)
        .then_some(())
}

// Checks the bounded fixture prefix starting at a genesis checkpoint. General
// non-genesis pin verification and policy changes in an ordinary prefix need
// the independently available authority/commitments described in the ADRs.
fn classify(pin: Option<OwnerCheckpoint>, evidence: &V, plaintext_size: usize) -> Classification {
    fn check(
        pin: Option<OwnerCheckpoint>,
        evidence: &V,
        plaintext_size: usize,
    ) -> Option<Classification> {
        use Classification::*;
        bounded(evidence, plaintext_size)?;
        if evidence.m().ok()?.iter().any(|(key, _)| *key > 5) {
            return None;
        }
        let Some(pin) = pin else {
            return Some(NeedsEvidence);
        };
        let head = evidence.get(0).ok()?;
        let mut chain = optional_array(evidence, 4)?.to_vec();
        chain.extend_from_slice(evidence.get(1).ok()?.a().ok()?);
        chain.push(head.clone());
        let states: Vec<_> = chain
            .iter()
            .map(|s| state_from_signed(s.b().ok()?))
            .collect::<Option<_>>()?;
        let first = states.first()?;
        if first.generation != pin.generation {
            return Some(NeedsEvidence);
        }
        if checkpoint(first) != pin {
            return None;
        }
        if states
            .iter()
            .any(|state| state.account_id != pin.account_id)
        {
            return None;
        }
        if states
            .windows(2)
            .any(|w| w[0].generation.checked_add(1) != Some(w[1].generation))
        {
            return Some(NeedsEvidence);
        }
        if states
            .windows(2)
            .any(|w| w[1].previous != Some(w[0].hash()))
        {
            return None;
        }
        let head_policy = RecoveryPolicy::from_body(required_bytes(evidence, 3)?).ok()?;
        if states.last()?.recovery_commitment != Some(head_policy.hash()) {
            return None;
        }
        let credentials: Vec<Vec<u8>> = evidence
            .get(2)
            .ok()?
            .a()
            .ok()?
            .iter()
            .map(|entry| entry.b().ok().map(<[u8]>::to_vec))
            .collect::<Option<_>>()?;
        let required: BTreeSet<_> = states
            .iter()
            .flat_map(|s| s.credentials.iter().copied())
            .collect();
        let mut supplied = BTreeSet::new();
        for raw in &credentials {
            let c = Credential::from_body(raw).ok()?;
            if c.account_id != pin.account_id
                || c.body() != *raw
                || !supplied.insert((c.device_id, c.hash()))
            {
                return None;
            }
        }
        if !supplied.is_subset(&required) {
            return None;
        }
        if required != supplied {
            return Some(NeedsEvidence);
        }
        let first_refs = &first.credentials;
        let first_creds: Vec<_> = credentials
            .iter()
            .filter(|raw| {
                let c = Credential::from_body(raw).unwrap();
                first_refs.contains(&(c.device_id, c.hash()))
            })
            .cloned()
            .collect();
        if first.generation != 1 {
            return Some(NeedsEvidence);
        }
        let mut parent = verify_genesis(chain[0].b().ok()?, &first_creds).ok()?;
        let dossiers = optional_array(evidence, 5)?;
        let recovery_positions: Vec<_> = chain
            .iter()
            .skip(1)
            .zip(states.iter().skip(1))
            .filter(|(signed, _)| has_recovery_signature(signed.b().unwrap()))
            .map(|(_, state)| state.hash())
            .collect();
        let mut last_index = None;
        for dossier in dossiers {
            if dossier.m().ok()?.len() != 4 {
                return None;
            }
            let hash = required_bytes(dossier, 0)?;
            let index = recovery_positions.iter().position(|h| h == hash)?;
            if last_index.is_some_and(|previous| index <= previous) {
                return None;
            }
            last_index = Some(index);
        }
        for (signed, state) in chain.iter().zip(&states).skip(1) {
            let signed = signed.b().ok()?;
            if has_recovery_signature(signed) {
                let Some(dossier) = dossiers
                    .iter()
                    .find(|entry| required_bytes(entry, 0) == Some(state.hash().as_slice()))
                else {
                    return Some(NeedsEvidence);
                };
                let old = RecoveryPolicy::from_body(required_bytes(dossier, 1)?).ok()?;
                let next = RecoveryPolicy::from_body(required_bytes(dossier, 2)?).ok()?;
                let request = required_bytes(dossier, 3)?;
                let observation = RecoveryObservation {
                    elapsed_seconds: 0,
                    vetoes: &[],
                };
                let verdict = verify_recovery_successor(
                    &parent,
                    signed,
                    &credentials,
                    &old,
                    &next,
                    request,
                    &observation,
                )
                .ok()?;
                // Zero elapsed credit. Even a cryptographically co-signed
                // candidate supplies no historical adoption evidence here.
                return match verdict {
                    RecoveryVerdict::Pending { .. }
                    | RecoveryVerdict::Complete {
                        immediate: true, ..
                    } => Some(OlderSheet),
                    RecoveryVerdict::Complete {
                        immediate: false, ..
                    } => None,
                };
            }

            // The normal W0 transition verifier must still accept every
            // controller link in the prefix. This fixture keeps its policy.
            parent = verify_successor(&parent, signed, &credentials).ok()?;
        }
        Some(OrdinaryPrefix)
    }
    check(pin, evidence, plaintext_size).unwrap_or(Classification::IntegrityFailure)
}

fn evidence(
    head: &[u8],
    tail: &[V],
    creds: &[Vec<u8>],
    policy: &RecoveryPolicy,
    segment: &[V],
    dossier: Option<V>,
) -> V {
    let base = map([
        (0, bytes(head)),
        (1, array(tail.to_vec())),
        (2, array(creds.iter().map(bytes))),
        (3, bytes(policy.body())),
        (4, array(segment.to_vec())),
    ]);
    with_field(&base, 5, dossier.map(|d| array([d])))
}

fn request_entry(f: &Fixture, request: &RecoveryRequest) -> Vec<u8> {
    let body = request.body();
    let input = ds_sig(LBL_REQUEST_SIG, PROFILE_V1, ROLE_RECOVERY, &ACCOUNT, &body);
    let sig = f.old_key.sign_with(&input, [11; 32]);
    sig::envelope(
        body,
        vec![sig::signature_entry(
            &ACCOUNT,
            PROFILE_V1,
            ROLE_RECOVERY,
            sig,
        )],
    )
}

#[test]
fn rk2a_01_ordinary_prefix_and_spurious_dossier() {
    let f = Fixture::new();
    let mut ordinary = f.parent.clone();
    ordinary.generation += 1;
    ordinary.previous = Some(f.parent.hash());
    ordinary.messaging.insert(TARGET);
    ordinary.credentials.push((TARGET, f.target.hash()));
    let body = ordinary.body();
    let sig = vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 10);
    let signed = sig::envelope(body, vec![sig]);
    let mut creds = f.credentials();
    creds.push(f.target.body());
    let evidence = evidence(
        &signed,
        &[],
        &creds,
        &f.old_policy,
        &[bytes(&f.signed_parent)],
        None,
    );
    let vector = golden("RK2A-01");
    let pin = checkpoint(&f.parent);
    assert_eq!(pin.to_bytes(), raw(&vector, "checkpoint"));
    assert_eq!(encode(&evidence), raw(&vector, "evidence"));
    assert_eq!(
        classify(Some(pin), &evidence, encode(&evidence).len()),
        Classification::OrdinaryPrefix
    );
    let fake = decode(&raw(&golden("RK2A-02"), "dossier")).unwrap();
    let bad = with_field(
        &evidence,
        5,
        Some(array([with_field(&fake, 0, Some(bytes(ordinary.hash())))])),
    );
    assert_eq!(
        classify(Some(pin), &bad, encode(&bad).len()),
        Classification::IntegrityFailure
    );
}

#[test]
fn rk2a_02_and_03_exact_signed_dossier_and_corruption() {
    use Classification::*;
    let f = Fixture::new();
    let pin = checkpoint(&f.parent);
    let vector = golden("RK2A-02");
    let request = RecoveryRequest {
        account_id: ACCOUNT,
        recovery_epoch: OLD_EPOCH,
        new_device_id: TARGET,
        new_device_credential: f.target.hash(),
        parent_state_hash: f.parent.hash(),
        request_nonce: [0x78; 16],
    };
    let signed_request = request_entry(&f, &request);
    let recovered_policy = RecoveryPolicy::default_for(
        RecoveryRoot::from_bytes(OLD_ROOT).auth_public_key(EPOCH),
        EPOCH,
    );
    let mut child = f.replacement.clone();
    child.recovery_commitment = Some(recovered_policy.hash());
    child.control = [TARGET].into();
    child.revoked = [ORIGIN, SELECTED].into();
    child.credentials = vec![(TARGET, f.target.hash())];
    let child_body = child.body();
    let child_sig = vote(&f.old_key, ROLE_RECOVERY, &ACCOUNT, &child_body, 12);
    let signed_child = sig::envelope(child_body, vec![child_sig]);
    let dossier = map([
        (0, bytes(child.hash())),
        (1, bytes(f.old_policy.body())),
        (2, bytes(recovered_policy.body())),
        (3, bytes(&signed_request)),
    ]);
    let mut creds = f.credentials();
    creds.push(f.target.body());
    let evidence = evidence(
        &signed_child,
        &[],
        &creds,
        &recovered_policy,
        &[bytes(&f.signed_parent)],
        Some(dossier.clone()),
    );
    assert_eq!(pin.to_bytes(), raw(&vector, "checkpoint"));
    assert_eq!(signed_request, raw(&vector, "signed_request"));
    assert_eq!(signed_child, raw(&vector, "signed_child"));
    assert_eq!(encode(&dossier), raw(&vector, "dossier"));
    assert_eq!(encode(&evidence), raw(&vector, "evidence"));

    let classify = |evidence: &V| classify(Some(pin), evidence, encode(evidence).len());
    assert_eq!(classify(&evidence), OlderSheet);
    assert_eq!(classify(&with_field(&evidence, 5, None)), NeedsEvidence);
    assert_eq!(classify(&with_field(&evidence, 4, None)), NeedsEvidence);
    assert_eq!(
        classify(&with_field(
            &evidence,
            5,
            Some(array([dossier.clone(), dossier.clone()]))
        )),
        IntegrityFailure
    );
    let bad_dossier = |field, value| {
        let altered = with_field(&dossier, field, Some(value));
        with_field(&evidence, 5, Some(array([altered])))
    };
    assert_eq!(classify(&bad_dossier(0, bytes([0; 32]))), IntegrityFailure);
    assert_eq!(
        classify(&bad_dossier(1, bytes(f.current_policy.body()))),
        IntegrityFailure
    );
    assert_eq!(
        classify(&bad_dossier(2, bytes(f.old_policy.body()))),
        IntegrityFailure
    );
    for req in [
        RecoveryRequest {
            new_device_id: ORIGIN,
            ..request.clone()
        },
        RecoveryRequest {
            recovery_epoch: EPOCH,
            ..request.clone()
        },
        RecoveryRequest {
            new_device_credential: f.origin.hash(),
            ..request.clone()
        },
    ] {
        assert_eq!(
            classify(&bad_dossier(3, bytes(request_entry(&f, &req)))),
            IntegrityFailure
        );
    }
    let mut damaged_request = request.body();
    *damaged_request.last_mut().unwrap() ^= 1; // Signed nonce.
    let signatures = decode(&signed_request)
        .unwrap()
        .get(2)
        .unwrap()
        .a()
        .unwrap()
        .to_vec();
    let damaged_request = sig::envelope(damaged_request, signatures);
    let damaged_dossier = with_field(&dossier, 3, Some(bytes(&damaged_request)));
    assert_eq!(
        encode(&damaged_dossier),
        raw(&golden("RK2A-03"), "tampered_nonce_dossier"),
    );
    assert_eq!(
        classify(&bad_dossier(3, bytes(damaged_request))),
        IntegrityFailure
    );

    let fake_vote =
        sig::signature_entry(&ACCOUNT, PROFILE_V1, ROLE_RECOVERY, vec![0; sig::SIG_LEN]);
    let fabricated = bytes(sig::envelope(child.body(), vec![fake_vote]));
    let bad = with_field(&evidence, 0, Some(fabricated));
    assert_eq!(classify(&bad), IntegrityFailure);
    assert_eq!(classify(&with_field(&bad, 4, None)), NeedsEvidence);
    let missing = with_field(&evidence, 2, Some(array(creds[..2].iter().map(bytes))));
    assert_eq!(classify(&missing), NeedsEvidence);
    creds.push(creds[0].clone());
    let extra = with_field(&evidence, 2, Some(array(creds.iter().map(bytes))));
    assert_eq!(classify(&extra), IntegrityFailure);

    // A root holder can issue another valid request for the same transition.
    // W0 does not bind its nonce or digest into the signed ACS body.
    let substituted = RecoveryRequest {
        request_nonce: [0x79; 16],
        ..request
    };
    assert_eq!(
        classify(&bad_dossier(3, bytes(request_entry(&f, &substituted)))),
        OlderSheet
    );
}

#[test]
fn rk2a_04_bounds_and_rk2a_05_no_independent_checkpoint() {
    let f = Fixture::new();
    let fixture = raw(&golden("RK2A-02"), "evidence");
    let evidence = decode(&fixture).unwrap();
    assert!(bounded(&evidence, record::MAX_PLAINTEXT_BYTES).is_some());
    assert!(bounded(&evidence, record::MAX_PLAINTEXT_BYTES + 1).is_none());
    let limit = |value: &V| bounded(value, fixture.len()).is_some();
    let set_array = |value: &V, key, n| with_field(value, key, Some(array(vec![bytes([0]); n])));
    let segment = set_array(&evidence, 4, 56);
    let tail = set_array(&segment, 1, 8);
    assert!(limit(&set_array(&set_array(&tail, 2, 320), 5, 64)));
    for (key, size) in [(4, 57), (1, 9), (2, 321), (5, 65)] {
        assert!(!limit(&set_array(&tail, key, size)));
    }

    // AEAD authentication can be checked with the root even when the user has
    // no separately custodied checkpoint. The record carries no substitute.
    let keys = record::RecordKeys::derive(&OLD_ROOT, &SCOPE);
    let anchored_record = map([
        (0, V::U(36)),
        (1, V::U(1)),
        (2, V::U(2 << record::REFRESH_BITS)),
        (3, V::U(EPOCH)),
        (
            4,
            map([
                (0, V::U(2)),
                (
                    1,
                    digest(
                        &state_from_signed(required_bytes(&evidence, 0).unwrap())
                            .unwrap()
                            .hash(),
                    ),
                ),
            ]),
        ),
        (5, evidence),
        (
            6,
            map([(0, array([map([(0, string(HOST)), (2, bytes(SCOPE))])]))]),
        ),
        (7, map([(0, array([]))])),
    ]);
    let unpadded = encode(&anchored_record).len();
    let bucket = record::bucket_for(unpadded).unwrap();
    let padding = record::pad_length(unpadded, bucket).unwrap();
    let plaintext = encode(&with_field(
        &anchored_record,
        22,
        Some(bytes(vec![0; padding])),
    ));
    let envelope = record::seal_envelope(&keys, &SCOPE, [0x23; 12], &plaintext).unwrap();
    let opened = record::open_envelope(&keys, &SCOPE, &envelope).unwrap();
    let decoded = decode(&opened).unwrap();
    let check = sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", &OLD_ROOT]);
    let legacy = crate::relationship::encode_radix(
        &[&OLD_ROOT[..], &check[..3]].concat(),
        crate::relationship::BASE32,
        5,
    );
    assert_eq!(legacy, golden("RK2A-05")["root_only_compact"]);
    assert!(matches!(parse(&legacy).unwrap(), Parsed::Legacy(_)));
    assert_eq!(
        classify(None, decoded.get(5).unwrap(), opened.len()),
        Classification::NeedsEvidence
    );
    assert_eq!(
        classify(
            Some(checkpoint(&f.parent)),
            decoded.get(5).unwrap(),
            opened.len()
        ),
        Classification::OlderSheet
    );
    let mut tampered = envelope;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(record::open_envelope(&keys, &SCOPE, &tampered).is_err());
}

#[test]
fn rk2a_01_and_04_signed_65_state_prefix_and_gap() {
    use Classification::*;
    let f = Fixture::new();
    let mut state = f.parent.clone();
    state.key_epochs = vec![(ORIGIN, 1)];
    let pin = checkpoint(&state);
    let mut signed = vec![bytes(sig::envelope(
        state.body(),
        vec![
            vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &state.body(), 50),
            vote(
                &f.selected_key,
                ROLE_CONTROLLER,
                &SELECTED,
                &state.body(),
                51,
            ),
        ],
    ))];
    for generation in 2..=65 {
        state.previous = Some(state.hash());
        state.generation = generation;
        state.key_epochs[0].1 = generation;
        let body = state.body();
        let entry = vote(
            &f.origin_key,
            ROLE_CONTROLLER,
            &ORIGIN,
            &body,
            generation as u8,
        );
        signed.push(bytes(sig::envelope(body, vec![entry])));
    }
    let value = evidence(
        signed[64].b().unwrap(),
        &signed[56..64],
        &f.credentials(),
        &f.old_policy,
        &signed[..56],
        None,
    );
    let check = |value: &V| classify(Some(pin), value, encode(value).len());
    assert_eq!(check(&value), OrdinaryPrefix);
    let mut segment = signed[..56].to_vec();
    segment.remove(12);
    assert_eq!(
        check(&with_field(&value, 4, Some(array(segment)))),
        NeedsEvidence
    );
    assert_eq!(
        check(&with_field(&value, 4, Some(array(signed[..57].to_vec())))),
        IntegrityFailure
    );
}
