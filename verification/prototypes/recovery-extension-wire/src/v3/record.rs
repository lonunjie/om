// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::record::{bounded, optional_array};
use om_core::control::{
    RecoveryObservation, RecoveryVerdict, has_recovery_signature, verify_recovery_successor,
};
use om_core::crypto::sha256;
use om_core::crypto::sig::{digest, ds_sig};
use om_core::recovery::{LBL_REQUEST_SIG, RecoveryRequest, STATE_OFF_EXPLICIT};
use om_core::recovery_record as record;
use om_core::recovery_sheet::OwnerCheckpoint;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    VerifiedOrdinaryPrefix,
    NeedsEvidence,
    IntegrityFailure,
}

fn checkpoint(acs: &Acs) -> OwnerCheckpoint {
    OwnerCheckpoint {
        account_id: acs.account_id,
        generation: acs.generation,
        acs_hash: acs.hash(),
    }
}

fn inside_limits(evidence: &V, plaintext: usize) -> bool {
    bounded(evidence, plaintext).is_some()
        && optional_array(evidence, 6).is_some_and(|runs| runs.len() <= 65)
}

fn policy_body(raw: &[u8]) -> Option<RecoveryPolicy> {
    let policy = RecoveryPolicy::from_body(raw).ok()?;
    (policy.body() == raw).then_some(policy)
}

// Each supplied run must be a valid entry in the expected sequence. A
// well-formed strict subsequence indicates missing evidence.
fn policy_runs(evidence: &V, states: &[Acs]) -> Result<Vec<Option<Vec<u8>>>, Outcome> {
    use Outcome::*;
    let hashes: Vec<_> = states.iter().map(Acs::hash).collect();
    let expected: Vec<_> = states
        .iter()
        .enumerate()
        .filter_map(|(i, state)| {
            let changed = i == 0 || state.recovery_commitment != states[i - 1].recovery_commitment;
            state
                .recovery_commitment
                .filter(|_| changed)
                .map(|digest| (hashes[i], digest))
        })
        .collect();
    let mut next_position = 0;
    let mut runs = std::collections::BTreeMap::new();
    for run in optional_array(evidence, 6).ok_or(IntegrityFailure)? {
        let state = required_bytes(run, 0).ok_or(IntegrityFailure)?;
        let raw = required_bytes(run, 1).ok_or(IntegrityFailure)?;
        let n = expected
            .iter()
            .position(|(hash, _)| state == hash)
            .ok_or(IntegrityFailure)?;
        let policy = policy_body(raw).ok_or(IntegrityFailure)?;
        if run.m().map_err(|_| IntegrityFailure)?.len() != 2
            || n < next_position
            || policy.hash() != expected[n].1
        {
            return Err(IntegrityFailure);
        }
        runs.insert(expected[n].0, raw.to_vec());
        next_position = n + 1;
    }
    let head_raw = required_bytes(evidence, 3).ok_or(IntegrityFailure)?;
    if states.last().and_then(|s| s.recovery_commitment)
        != Some(policy_body(head_raw).ok_or(IntegrityFailure)?.hash())
    {
        return Err(IntegrityFailure);
    }
    if runs.len() != expected.len() {
        return Err(NeedsEvidence);
    }
    let mut current = None;
    let mut history = Vec::with_capacity(states.len());
    for (state, hash) in states.iter().zip(hashes) {
        if state.recovery_commitment.is_none() {
            current = None;
        } else if let Some(new) = runs.remove(&hash) {
            current = Some(new);
        }
        history.push(current.clone());
    }
    if current.as_deref() != Some(head_raw) {
        return Err(IntegrityFailure);
    }
    Ok(history)
}

fn classify(
    pin: Option<OwnerCheckpoint>,
    evidence: &V,
    plaintext: usize,
    historical: Option<(&Acs, &[Vec<u8>])>,
) -> Outcome {
    use Outcome::*;
    fn check(
        pin: Option<OwnerCheckpoint>,
        evidence: &V,
        plaintext: usize,
        historical: Option<(&Acs, &[Vec<u8>])>,
    ) -> Result<Outcome, Outcome> {
        if !inside_limits(evidence, plaintext)
            || evidence
                .m()
                .map_err(|_| IntegrityFailure)?
                .iter()
                .any(|(k, _)| *k > 6)
        {
            return Err(IntegrityFailure);
        }
        let pin = pin.ok_or(NeedsEvidence)?;
        let mut chain = optional_array(evidence, 4)
            .ok_or(IntegrityFailure)?
            .to_vec();
        chain.extend_from_slice(optional_array(evidence, 1).ok_or(IntegrityFailure)?);
        chain.push(evidence.get(0).map_err(|_| IntegrityFailure)?.clone());
        let raws: Vec<_> = chain
            .iter()
            .map(|entry| entry.b().map_err(|_| IntegrityFailure))
            .collect::<Result<_, _>>()?;
        let states: Vec<_> = raws
            .iter()
            .map(|wire| {
                signed_state(wire)
                    .map(|(state, _)| state)
                    .ok_or(IntegrityFailure)
            })
            .collect::<Result<_, _>>()?;
        let first = &states[0];
        if first.generation != pin.generation {
            return Err(NeedsEvidence);
        }
        if checkpoint(first) != pin || states.iter().any(|s| s.account_id != first.account_id) {
            return Err(IntegrityFailure);
        }
        if let Some((predecessor, credentials)) = historical {
            verify_successor(predecessor, raws[0], credentials).map_err(|_| IntegrityFailure)?;
        }

        let needed: BTreeSet<_> = states
            .iter()
            .flat_map(|s| s.credentials.iter().copied())
            .collect();
        let mut supplied = BTreeSet::new();
        let mut bodies = Vec::new();
        for entry in optional_array(evidence, 2).ok_or(IntegrityFailure)? {
            let raw = entry.b().map_err(|_| IntegrityFailure)?;
            let credential = Credential::from_body(raw).map_err(|_| IntegrityFailure)?;
            let pair = (credential.device_id, credential.hash());
            if credential.account_id != pin.account_id
                || credential.body() != raw
                || !needed.contains(&pair)
                || !supplied.insert(pair)
            {
                return Err(IntegrityFailure);
            }
            bodies.push(raw.to_vec());
        }
        if supplied != needed {
            return Err(NeedsEvidence);
        }
        let policies = policy_runs(evidence, &states)?;
        let dossiers = optional_array(evidence, 5).ok_or(IntegrityFailure)?;
        let mut previous_dossier = None;
        for dossier in dossiers {
            let hash = required_bytes(dossier, 0).ok_or(IntegrityFailure)?;
            let position = states
                .iter()
                .zip(&raws)
                .enumerate()
                .skip(1)
                .find(|(_, (s, raw))| s.hash() == hash && has_recovery_signature(raw))
                .map(|(i, _)| i)
                .ok_or(IntegrityFailure)?;
            if dossier.m().map_err(|_| IntegrityFailure)?.len() != 4
                || previous_dossier.is_some_and(|prior| position <= prior)
            {
                return Err(IntegrityFailure);
            }
            previous_dossier = Some(position);
        }
        for i in 1..states.len() {
            let (parent, child) = (&states[i - 1], &states[i]);
            if parent.generation.checked_add(1) != Some(child.generation) {
                return Err(NeedsEvidence);
            }
            if child.previous != Some(parent.hash()) {
                return Err(IntegrityFailure);
            }
            if has_recovery_signature(raws[i]) {
                let dossier = dossiers
                    .iter()
                    .find(|entry| required_bytes(entry, 0) == Some(child.hash().as_slice()))
                    .ok_or(NeedsEvidence)?;
                let old = required_bytes(dossier, 1).ok_or(IntegrityFailure)?;
                let next = required_bytes(dossier, 2).ok_or(IntegrityFailure)?;
                if policies[i - 1].as_deref() != Some(old) || policies[i].as_deref() != Some(next) {
                    return Err(IntegrityFailure);
                }
                let request = required_bytes(dossier, 3).ok_or(IntegrityFailure)?;
                verify_recovery_successor(
                    parent,
                    raws[i],
                    &bodies,
                    &policy_body(old).ok_or(IntegrityFailure)?,
                    &policy_body(next).ok_or(IntegrityFailure)?,
                    request,
                    &RecoveryObservation {
                        elapsed_seconds: 0,
                        vetoes: &[],
                    },
                )
                .map_err(|_| IntegrityFailure)?;
                // Historical W0 has no independently bound request identity.
                // This prototype has neither an accepted extension nor any
                // pre-existing verifier-local request-to-child evidence.
                return Err(NeedsEvidence);
            }
            verify_successor(parent, raws[i], &bodies).map_err(|_| IntegrityFailure)?;
        }
        Ok(VerifiedOrdinaryPrefix)
    }
    check(pin, evidence, plaintext, historical).unwrap_or_else(|outcome| outcome)
}

fn run_at(state: &Acs, policy: &RecoveryPolicy) -> V {
    map([(0, bytes(state.hash())), (1, bytes(policy.body()))])
}

fn evidence(
    head: &[u8],
    prefix: &[Vec<u8>],
    creds: &[Vec<u8>],
    policy: &RecoveryPolicy,
    runs: &[V],
) -> V {
    let boundary = prefix.len().saturating_sub(8);
    map([
        (0, bytes(head)),
        (1, array(prefix[boundary..].iter().map(bytes))),
        (2, array(creds.iter().map(bytes))),
        (3, bytes(policy.body())),
        (4, array(prefix[..boundary].iter().map(bytes))),
        (6, array(runs.iter().cloned())),
    ])
}

struct Walk {
    first: V,
    ordinary: V,
    legacy: V,
    legacy_target: Credential,
}

impl Walk {
    fn new(f: &Rotated) -> Self {
        let c = &f.ceremony;
        let root = RecoveryRoot::from_bytes(ROOT);
        let policies = [
            c.old_policy.clone(),
            c.current_policy.clone(),
            RecoveryPolicy::default_for(root.auth_signer(8).public(), 8),
            RecoveryPolicy::default_for(root.auth_signer(9).public(), 9),
        ];
        let mut chain = vec![c.signed_parent.clone()];
        let mut runs = vec![run_at(&c.parent, &policies[0])];
        let mut state = c.parent.clone();
        let mut first = None;
        for (n, index) in [1, 1, 2, 2, 3].into_iter().enumerate() {
            state.previous = Some(state.hash());
            state.generation += 1;
            let policy = &policies[index];
            state.recovery_commitment = Some(policy.hash());
            match state.generation {
                4 => {
                    state.messaging.insert(TARGET);
                    state.credentials.push((TARGET, c.target.hash()));
                }
                6 => {
                    state.messaging.remove(&TARGET);
                    state.credentials.retain(|(d, _)| d != &TARGET);
                    state.revoked.insert(TARGET);
                }
                _ => {}
            }
            let mut votes = vec![vote(
                &c.origin_key,
                ROLE_CONTROLLER,
                &ORIGIN,
                &state.body(),
                23 + n as u8,
            )];
            if state.generation % 2 == 1 {
                votes.push(vote(
                    &c.selected_key,
                    ROLE_CONTROLLER,
                    &SELECTED,
                    &state.body(),
                    33 + n as u8,
                ));
                runs.push(run_at(&state, policy));
            }
            let signed = sig::envelope(state.body(), votes);
            if n == 0 {
                first = Some(evidence(&signed, &chain, &c.credentials(), policy, &runs));
            }
            chain.push(signed);
        }
        let mut creds = c.credentials();
        creds.push(c.target.body());
        let ordinary = evidence(
            chain.last().unwrap(),
            &chain[..chain.len() - 1],
            &creds,
            &policies[3],
            &runs,
        );

        assert_eq!(state.generation, 7);
        let legacy_target = Credential {
            device_id: [0x78; 16],
            account_id: ACCOUNT,
            signing_pk: signer(0x85).public(),
            ik: x25519_pub(&[0x95; 32]),
        };
        let target_hash = legacy_target.hash();
        let request = RecoveryRequest {
            account_id: ACCOUNT,
            recovery_epoch: 9,
            new_device_id: legacy_target.device_id,
            new_device_credential: target_hash,
            parent_state_hash: state.hash(),
            request_nonce: [0x79; 16],
        };
        let key = root.auth_signer(9);
        let input = ds_sig(
            LBL_REQUEST_SIG,
            PROFILE_V1,
            ROLE_RECOVERY,
            &ACCOUNT,
            &request.body(),
        );
        let signed_request = sig::envelope(
            request.body(),
            vec![sig::signature_entry(
                &ACCOUNT,
                PROFILE_V1,
                ROLE_RECOVERY,
                key.sign_with(&input, [36; 32]),
            )],
        );
        let next = RecoveryPolicy::default_for(root.auth_signer(10).public(), 10);
        let parent = state.clone();
        state.previous = Some(state.hash());
        state.generation += 1;
        state.revoked.extend(&state.control);
        state.control = [legacy_target.device_id].into();
        state.credentials = vec![(legacy_target.device_id, target_hash)];
        state.key_epochs.clear();
        state.recovery_commitment = Some(next.hash());
        let signed = sig::envelope(
            state.body(),
            vec![vote(&key, ROLE_RECOVERY, &ACCOUNT, &state.body(), 37)],
        );
        runs.push(run_at(&state, &next));
        creds.push(legacy_target.body());
        assert!(matches!(
            verify_recovery_successor(
                &parent,
                &signed,
                &creds,
                &policies[3],
                &next,
                &signed_request,
                &RecoveryObservation {
                    elapsed_seconds: 0,
                    vetoes: &[]
                }
            )
            .unwrap(),
            RecoveryVerdict::Pending { .. }
        ));
        let dossier = map([
            (0, bytes(state.hash())),
            (1, bytes(policies[3].body())),
            (2, bytes(next.body())),
            (3, bytes(signed_request)),
        ]);
        let legacy = with_field(
            &evidence(&signed, &chain, &creds, &next, &runs),
            5,
            Some(array([dossier])),
        );
        Self {
            first: first.unwrap(),
            ordinary,
            legacy,
            legacy_target,
        }
    }
}

#[test]
fn rk2b_01_body_pin_and_optional_historical_reverification() {
    use Outcome::*;
    let f = Rotated::new();
    let c = &f.ceremony;
    let walk = Walk::new(&f);
    let wire = encode(&walk.first);
    let vector = golden("RK2B-01");
    assert_eq!(wire, raw(&vector, "evidence"));
    let pin = checkpoint(&c.parent);
    assert_eq!(pin.to_bytes(), raw(&vector, "checkpoint"));
    assert_eq!(
        classify(Some(pin), &walk.first, wire.len(), None),
        VerifiedOrdinaryPrefix
    );
    assert_eq!(classify(None, &walk.first, wire.len(), None), NeedsEvidence);
    let history = [f.previous_origin.body()];
    let predecessor = Some((&f.predecessor, history.as_slice()));
    assert_eq!(
        classify(Some(pin), &walk.first, wire.len(), predecessor),
        VerifiedOrdinaryPrefix
    );
    assert!(verify_successor(&f.predecessor, &c.signed_parent, &c.credentials()).is_err());

    let signature = vote(
        &c.origin_key,
        ROLE_CONTROLLER,
        &ORIGIN,
        &c.parent.body(),
        14,
    );
    let same_body_invalid_signature = sig::envelope(c.parent.body(), vec![signature]);
    let changed = |envelope| with_field(&walk.first, 1, Some(array([bytes(envelope)])));
    let candidate = changed(same_body_invalid_signature);
    assert_eq!(
        classify(Some(pin), &candidate, wire.len(), predecessor),
        IntegrityFailure
    );
    // The owner sheet commits the body hash. Without Q this validly encoded
    // envelope cannot be distinguished by the sheet alone.
    assert_eq!(
        classify(Some(pin), &candidate, wire.len(), None),
        VerifiedOrdinaryPrefix
    );
    let (_, old_envelope) = signed_state(&c.signed_parent).unwrap();
    let old_entry = &old_envelope.get(2).unwrap().a().unwrap()[0];
    let malformed = sig::envelope(
        c.parent.body(),
        vec![with_field(old_entry, 3, Some(bytes([0])))],
    );
    assert_eq!(
        classify(Some(pin), &changed(malformed), wire.len(), None),
        IntegrityFailure
    );
    let mut wrong_body = c.parent.clone();
    wrong_body.recovery_commitment = Some(c.current_policy.hash());
    let wrong = sig::envelope(wrong_body.body(), vec![old_entry.clone()]);
    assert_eq!(
        classify(Some(pin), &changed(wrong), wire.len(), None),
        IntegrityFailure
    );
}

#[test]
fn rk2b_02_intermediate_policies_and_transition_authority() {
    use Outcome::*;
    let f = Rotated::new();
    let walk = Walk::new(&f);
    let wire = encode(&walk.ordinary);
    assert_eq!(wire, raw(&golden("RK2B-02"), "evidence"));
    let check = |evidence: &V| {
        classify(
            Some(checkpoint(&f.ceremony.parent)),
            evidence,
            wire.len(),
            None,
        )
    };
    assert_eq!(check(&walk.ordinary), VerifiedOrdinaryPrefix);
    let runs = walk.ordinary.get(6).unwrap().a().unwrap().to_vec();
    let replace_runs = |runs| with_field(&walk.ordinary, 6, Some(array(runs)));
    let mut missing = runs.clone();
    missing.remove(2);
    assert_eq!(check(&replace_runs(missing)), NeedsEvidence);
    assert_eq!(check(&with_field(&walk.ordinary, 6, None)), NeedsEvidence);
    let mut misordered = runs.clone();
    misordered.swap(1, 2);
    let mut extra = runs.clone();
    extra.push(runs[0].clone());
    let mut wrong = runs.clone();
    wrong[2] = with_field(&wrong[2], 1, Some(bytes(f.ceremony.old_policy.body())));
    for invalid in [misordered, extra, wrong] {
        assert_eq!(check(&replace_runs(invalid)), IntegrityFailure);
    }
    let wrong_key3 = with_field(&walk.ordinary, 3, Some(bytes(f.ceremony.old_policy.body())));
    assert_eq!(check(&wrong_key3), IntegrityFailure);
    let (head, _) = signed_state(required_bytes(&walk.ordinary, 0).unwrap()).unwrap();
    let half_quorum = sig::envelope(
        head.body(),
        vec![vote(
            &f.ceremony.origin_key,
            ROLE_CONTROLLER,
            &ORIGIN,
            &head.body(),
            1,
        )],
    );
    assert_eq!(
        check(&with_field(&walk.ordinary, 0, Some(bytes(half_quorum)))),
        IntegrityFailure
    );
}

#[test]
fn rk2b_03_legacy_recovery_link_stays_needs_evidence() {
    use Outcome::*;
    let f = Rotated::new();
    let walk = Walk::new(&f);
    let wire = encode(&walk.legacy);
    let vector = golden("RK2B-03");
    assert_eq!(wire, raw(&vector, "evidence"));
    let check = |evidence: &V| {
        classify(
            Some(checkpoint(&f.ceremony.parent)),
            evidence,
            wire.len(),
            None,
        )
    };
    assert_eq!(vector["expected"], "needs_evidence");
    assert_eq!(check(&walk.legacy), NeedsEvidence);
    assert_eq!(check(&with_field(&walk.legacy, 5, None)), NeedsEvidence);
    let missing = array(
        walk.legacy
            .get(2)
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .filter(|v| v.b().unwrap() != walk.legacy_target.body())
            .cloned(),
    );
    assert_eq!(
        check(&with_field(&walk.legacy, 2, Some(missing))),
        NeedsEvidence
    );
    let dossier = &walk.legacy.get(5).unwrap().a().unwrap()[0];
    for field in [1, 2] {
        let bad = with_field(dossier, field, Some(bytes(f.ceremony.old_policy.body())));
        assert_eq!(
            check(&with_field(&walk.legacy, 5, Some(array([bad])))),
            IntegrityFailure
        );
    }
}

fn maximum_window(f: &Fixture) -> (OwnerCheckpoint, Acs, V) {
    let mut state = f.parent.clone();
    state.control.remove(&SELECTED);
    state.credentials.retain(|(id, _)| id == &ORIGIN);
    let origin_ref = state.credentials[0];
    // Synthetic public keys reused for unexercised messaging devices; each
    // credential binds a different DeviceID and has a distinct digest.
    let bodies: Vec<_> = (1000..1319u128)
        .map(|i| Credential {
            device_id: i.to_be_bytes(),
            ..f.target.clone()
        })
        .collect();
    let refs: Vec<_> = bodies.iter().map(|c| (c.device_id, c.hash())).collect();
    let mut policy = f.old_policy.clone();
    policy.state = STATE_OFF_EXPLICIT;
    policy.methods.clear();
    let mut chain = Vec::new();
    let mut runs = Vec::new();
    let mut pin = None;
    for generation in 1..=65 {
        if generation > 1 {
            state.previous = Some(state.hash());
        }
        state.generation = generation;
        policy.rotation_epoch = Some(generation);
        state.recovery_commitment = Some(policy.hash());
        match generation {
            63 => {
                state.messaging.extend(refs[..160].iter().map(|(d, _)| *d));
                state.credentials.extend_from_slice(&refs[..160]);
            }
            64 => {
                state.revoked.append(&mut state.messaging);
                state.credentials = vec![origin_ref];
            }
            65 => {
                state.messaging.extend(refs[160..].iter().map(|(d, _)| *d));
                state.credentials.extend_from_slice(&refs[160..]);
            }
            _ => {}
        }
        state.credentials.sort();
        let body = state.body();
        chain.push(sig::envelope(
            body.clone(),
            vec![vote(
                &f.origin_key,
                ROLE_CONTROLLER,
                &ORIGIN,
                &body,
                generation as u8,
            )],
        ));
        runs.push(run_at(&state, &policy));
        if generation == 1 {
            pin = Some(checkpoint(&state));
        }
    }
    let creds: Vec<_> = std::iter::once(f.origin.body())
        .chain(bodies.iter().map(Credential::body))
        .collect();
    let value = evidence(&chain[64], &chain[..64], &creds, &policy, &runs);
    (pin.unwrap(), state, value)
}

fn writer_status(
    pin: OwnerCheckpoint,
    value: &V,
    size: usize,
    acknowledged_current_head: bool,
) -> &'static str {
    match classify(Some(pin), value, size, None) {
        Outcome::VerifiedOrdinaryPrefix if acknowledged_current_head => "on",
        Outcome::VerifiedOrdinaryPrefix => "pending_record_ack",
        _ => "needs_new_safety_code",
    }
}

#[test]
fn rk2b_04_maximum_signed_window_and_record_floor_restart() {
    use Outcome::*;
    use record::Freshness;
    let f = Fixture::new();
    let (pin, head, evidence) = maximum_window(&f);
    let vector = golden("RK2B-04");
    for (label, field, expected) in [
        ("segment", 4, 56),
        ("tail", 1, 8),
        ("runs", 6, 65),
        ("credentials", 2, 320),
    ] {
        let values = evidence.get(field).unwrap().a().unwrap();
        assert_eq!(values.len(), expected);
        assert_eq!(vector[label], expected);
        let mut overflow = values.to_vec();
        overflow.push(values[0].clone());
        assert!(!inside_limits(
            &with_field(&evidence, field, Some(array(overflow))),
            record::MAX_PLAINTEXT_BYTES,
        ));
    }
    assert_eq!(vector["head"], 1);
    assert_eq!(
        vector["credential_bytes"],
        record::MAX_CREDENTIAL_BYTES_TOTAL
    );
    for bytes_count in [
        record::MAX_CREDENTIAL_BYTES_TOTAL,
        record::MAX_CREDENTIAL_BYTES_TOTAL + 1,
    ] {
        let size_evidence = with_field(&evidence, 2, Some(array([bytes(vec![0; bytes_count])])));
        assert_eq!(
            inside_limits(&size_evidence, record::MAX_PLAINTEXT_BYTES),
            bytes_count == record::MAX_CREDENTIAL_BYTES_TOTAL,
        );
    }

    let generation = record::record_generation(head.generation, 0).unwrap();
    let base = map([
        (0, V::U(36)),
        (1, V::U(1)),
        (2, V::U(generation)),
        (3, V::U(65)),
        (
            4,
            map([(0, V::U(head.generation)), (1, digest(&head.hash()))]),
        ),
        (5, evidence.clone()),
        (
            6,
            map([(0, array([map([(0, string(HOST)), (2, bytes(SCOPE))])]))]),
        ),
        (7, map([(0, array([]))])),
    ]);
    let cap = record::MAX_PLAINTEXT_BYTES;
    assert_eq!(vector["plaintext"], cap);
    let padding = record::pad_length(encode(&base).len(), cap).unwrap();
    let plaintext = encode(&with_field(&base, 22, Some(bytes(vec![0; padding]))));
    assert_eq!(plaintext.len(), cap);
    assert_eq!(
        sha256(&[&plaintext]).as_slice(),
        raw(&vector, "plaintext_sha256")
    );
    let overflow = encode(&with_field(&base, 22, Some(bytes(vec![0; padding + 1]))));
    assert_eq!(overflow.len(), cap + 1);
    assert_eq!(
        writer_status(pin, &evidence, overflow.len(), true),
        "needs_new_safety_code"
    );
    assert_eq!(
        writer_status(pin, &with_field(&evidence, 6, None), cap, true),
        "needs_new_safety_code"
    );
    assert_eq!(
        writer_status(pin, &evidence, cap, false),
        "pending_record_ack"
    );

    let keys = record::RecordKeys::derive(&OLD_ROOT, &SCOPE);
    assert!(record::seal_envelope(&keys, &SCOPE, [0x24; 12], &overflow).is_err());
    let sealed = record::seal_envelope(&keys, &SCOPE, [0x25; 12], &plaintext).unwrap();
    let opened = record::open_envelope(&keys, &SCOPE, &sealed).unwrap();
    let opened_value = decode(&opened).unwrap();
    assert_eq!(
        classify(Some(pin), opened_value.get(5).unwrap(), opened.len(), None),
        VerifiedOrdinaryPrefix
    );
    assert_eq!(
        record::freshness(None, generation, &opened),
        Freshness::Newer
    );
    assert_eq!(writer_status(pin, &evidence, cap, true), "on");

    // Persist only after authenticated record/opened evidence checks pass.
    let stored = encode(&map([
        (0, bytes(SCOPE)),
        (1, bytes(keys.locator)),
        (2, bytes(ACCOUNT)),
        (3, V::U(generation)),
        (4, bytes(record::plaintext_digest(&opened))),
    ]));
    let restarted = decode(&stored).unwrap();
    assert_eq!(required_bytes(&restarted, 0), Some(SCOPE.as_slice()));
    assert_eq!(required_bytes(&restarted, 1), Some(keys.locator.as_slice()));
    assert_eq!(required_bytes(&restarted, 2), Some(ACCOUNT.as_slice()));
    let mark = Some((
        restarted.get(3).unwrap().u().unwrap(),
        restarted.get(4).unwrap().b32().unwrap(),
    ));
    assert_eq!(
        record::freshness(mark, generation, &plaintext),
        Freshness::Same
    );
    assert_eq!(
        record::freshness(mark, generation - 1, &plaintext),
        Freshness::Stale
    );
    assert_eq!(
        record::freshness(mark, generation, &overflow),
        Freshness::Equivocation
    );
    let mut tampered = sealed;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(record::open_envelope(&keys, &SCOPE, &tampered).is_err());
}
