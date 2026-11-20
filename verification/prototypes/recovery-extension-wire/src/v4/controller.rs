// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::controller::{
    FIXED_JOIN_TIME, binding, ceremony_id, checked_vote, decode_op3_offer, raw_hash, request_input,
};
use crate::v2::v3::controller::{Authority, check_preflight, preflight, sorted_union};
use om_core::control::{RecoveryObservation, RecoveryVerdict, verify_recovery_successor};
use om_core::crypto::recovery_approval::{ROLE_REQ, ROLE_RESP};
use om_core::crypto::sig::ds_sig;
use om_core::recovery::{LBL_REQUEST_SIG, LBL_VETO_SIG, RecoveryRequest, RecoveryVeto};
use std::collections::BTreeSet;

type Reference = ([u8; 16], [u8; 32]);

fn reference(c: &Credential) -> Reference {
    (c.device_id, c.hash())
}

fn credentials(mut bodies: Vec<Credential>) -> V {
    bodies.sort_by_key(reference);
    array(bodies.iter().map(|c| bytes(c.body())))
}

struct Op3 {
    rotated: Rotated,
    proposal: V,
    id: [u8; 32],
}

impl Op3 {
    fn new() -> Self {
        let rotated = Rotated::new();
        let f = &rotated.ceremony;
        let hash = f.replacement.hash();
        let link = golden("CC3B-01");
        let (_, offer, link_hash) =
            decode_op3_offer(link["link"].as_str().unwrap(), FIXED_JOIN_TIME).unwrap();
        assert_eq!(link_hash, hash);
        let id = ceremony_id(&binding(&f.parent, f.selected.hash(), hash), &offer);
        let proposal = map([
            (0, V::U(2)),
            (1, bytes(&f.signed_parent)),
            (2, bytes(f.replacement.body())),
            (3, bytes(f.current_policy.body())),
            (4, bytes(f.parent.hash())),
            (5, bytes(hash)),
            (6, V::U(OLD_EPOCH)),
            (7, V::U(EPOCH)),
            (
                8,
                array(
                    f.parent
                        .credentials
                        .iter()
                        .map(|(d, h)| array([bytes(d), bytes(h)])),
                ),
            ),
            (9, bytes(f.old_policy.body())),
            (10, credentials(sorted_union(&rotated))),
        ]);
        assert_eq!(encode(&proposal), raw(&link, "proposal"));
        Self {
            rotated,
            proposal,
            id,
        }
    }

    fn seal(&self, proposal: &V, key: &DeviceSigner) -> Vec<u8> {
        preflight(&encode(proposal), &self.id, key)
    }

    fn wire(&self, proposal: &V) -> Vec<u8> {
        self.seal(proposal, &self.rotated.ceremony.origin_key)
    }

    fn check(&self, authority: Authority<'_>, stale_or_fork: bool, wire: &[u8]) -> Option<Acs> {
        check_preflight(
            authority,
            stale_or_fork,
            &self.id,
            &self.rotated.ceremony.replacement.hash(),
            ROLE_RESP,
            wire,
        )
    }

    fn membership_mutations(&self) -> Vec<(&'static str, Vec<u8>)> {
        let f = &self.rotated.ceremony;
        let old = self.rotated.previous_origin.clone();
        let current = f.origin.clone();
        let selected = f.selected.clone();
        let wrong_old = Credential {
            device_id: ORIGIN,
            ..f.target.clone()
        };
        let change = |bodies| with_field(&self.proposal, 10, Some(credentials(bodies)));
        let swapped_parent = sig::envelope(
            f.parent.body(),
            vec![vote(
                &f.origin_key,
                ROLE_CONTROLLER,
                &ORIGIN,
                &f.parent.body(),
                14,
            )],
        );
        vec![
            (
                "wrong_old",
                self.wire(&change(vec![wrong_old, current.clone(), selected.clone()])),
            ),
            (
                "old_current_swap",
                self.seal(
                    &with_field(&self.proposal, 1, Some(bytes(swapped_parent))),
                    &signer(0x81),
                ),
            ),
            (
                "omit_old",
                self.wire(&change(vec![current.clone(), selected.clone()])),
            ),
            (
                "collapse_keep_old",
                self.wire(&change(vec![old.clone(), selected.clone()])),
            ),
            (
                "extra",
                self.wire(&change(vec![old, current, selected, f.target.clone()])),
            ),
        ]
    }

    fn verify_selected_vote(&self, child: &Acs, preflight: &[u8], request: &[u8]) -> bool {
        checked_vote(
            &self.rotated.ceremony,
            child,
            &self.id,
            preflight,
            request,
            ROLE_REQ,
        )
        .is_some()
    }

    fn selected_request(&self, child: &Acs, preflight: &[u8]) -> Vec<u8> {
        let f = &self.rotated.ceremony;
        let body = child.body();
        let signed = sig::envelope(
            body.clone(),
            vec![vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &body, 16)],
        );
        let proof = f
            .selected_key
            .sign_with(&request_input(&self.id, preflight, &signed), [17; 32]);
        encode(&array([
            V::U(1),
            bytes(proof),
            bytes(signed),
            V::Null,
            raw_hash(preflight),
        ]))
    }
}

#[test]
fn cc3c_01_durable_parent_uses_current_keys_without_historical_authority() {
    let v = vector("CC3C-01");
    let op = Op3::new();
    let f = &op.rotated.ceremony;
    let wire = op.wire(&op.proposal);
    assert_eq!(wire, raw(&golden("CC3B-01"), "preflight"));
    assert_eq!(v["preflight_sha256"], hex(&sha256(&[&wire])));
    let child = op
        .check(Authority::DurableParent(&f.signed_parent), false, &wire)
        .unwrap();
    let vote_request = op.selected_request(&child, &wire);
    assert_eq!(vote_request, raw(&golden("CC3B-01"), "request"));
    assert!(op.verify_selected_vote(&child, &wire, &vote_request));
    assert_eq!(
        v["accepted_parent"],
        "vote_permitted_historical_bodies_unverified"
    );
    assert!(op.check(Authority::Untrusted, false, &wire).is_none());
    assert_eq!(v["untrusted_predecessor"], "refused");
    for (name, mutation) in op.membership_mutations() {
        if ["wrong_old", "omit_old"].contains(&name) {
            let child = op
                .check(Authority::DurableParent(&f.signed_parent), false, &mutation)
                .unwrap();
            let request = op.selected_request(&child, &mutation);
            assert!(op.verify_selected_vote(&child, &mutation, &request));
            assert_eq!(
                v["variants_without_historical_authority"][name],
                "vote_permitted"
            );
        }
    }
}

#[test]
fn cc3c_02_reverification_requires_the_exact_two_era_union() {
    let v = vector("CC3C-02");
    let op = Op3::new();
    let f = &op.rotated.ceremony;
    assert_eq!(raw(&v, "old_digest"), op.rotated.previous_origin.hash());
    assert_eq!(raw(&v, "current_digest"), f.origin.hash());
    let q = &op.rotated.predecessor;
    assert!(
        op.check(
            Authority::VerifiedPredecessor(q),
            false,
            &op.wire(&op.proposal)
        )
        .is_some()
    );
    for (name, wire) in op.membership_mutations() {
        check_bytes(&v["mutations"][name], &wire, "refused");
        assert!(
            op.check(Authority::VerifiedPredecessor(q), false, &wire)
                .is_none(),
            "{name}"
        );
    }
}

// Fixture trust context. The ADR gives no receipt storage format or MAC.
// Only a receipt issued by actual local verification can enter the trusted slot.
#[derive(Clone)]
struct Receipt {
    parent: [u8; 32],
    predecessor: [u8; 32],
    signers: BTreeSet<Reference>,
}

impl Receipt {
    fn from_local_acceptance(q: &Acs, p_wire: &[u8], credentials: &[Vec<u8>]) -> Option<Self> {
        let parent = verify_successor(q, p_wire, credentials).ok()?.hash();
        let (_, envelope) = signed_state(p_wire)?;
        let signers = envelope
            .get(2)
            .ok()?
            .a()
            .ok()?
            .iter()
            .map(|entry| {
                let id = required_bytes(entry, 0)?;
                q.credentials
                    .iter()
                    .copied()
                    .find(|(device, _)| device.as_slice() == id)
            })
            .collect::<Option<_>>()?;
        Some(Self {
            parent,
            predecessor: q.hash(),
            signers,
        })
    }
}

fn receipt_authorizes(
    op: &Op3,
    receipt: Option<&Receipt>,
    accepted: Option<&[u8]>,
    wire: &[u8],
) -> Option<bool> {
    let accepted = accepted?;
    let f = &op.rotated.ceremony;
    op.check(Authority::DurableParent(accepted), false, wire)?;
    let Some(receipt) =
        receipt.filter(|r| r.parent == f.parent.hash() && Some(r.predecessor) == f.parent.previous)
    else {
        return Some(false); // independently accepted parent, supplementary bodies only
    };
    let outer = decode(wire).ok()?;
    let proposal = decode(outer.a().ok()?.get(1)?.b().ok()?).ok()?;
    let supplied = proposal
        .get(10)
        .ok()?
        .a()
        .ok()?
        .iter()
        .map(|v| Credential::from_body(v.b()?).map(|c| reference(&c)))
        .collect::<Result<BTreeSet<_>, _>>()
        .ok()?;
    let expected = receipt
        .signers
        .iter()
        .copied()
        .chain([reference(&f.origin), reference(&f.selected)])
        .collect();
    (supplied == expected).then_some(true)
}

#[test]
fn cc3c_03_only_preflight_independent_receipt_can_check_historical_membership() {
    let v = vector("CC3C-03");
    let op = Op3::new();
    let f = &op.rotated;
    let good = op.wire(&op.proposal);
    let receipt = Receipt::from_local_acceptance(
        &f.predecessor,
        &f.ceremony.signed_parent,
        &[f.previous_origin.body()],
    )
    .unwrap();
    assert_eq!(raw(&v, "parent_hash"), receipt.parent);
    assert_eq!(raw(&v, "predecessor_hash"), receipt.predecessor);
    assert_eq!(raw(&v, "signer_digest"), receipt.signers.first().unwrap().1);
    let pinned = Some(f.ceremony.signed_parent.as_slice());
    assert_eq!(
        receipt_authorizes(&op, Some(&receipt), pinned, &good),
        Some(true)
    );
    assert_eq!(v["verified_receipt"], "historical_membership_verified");
    for (_, wire) in op.membership_mutations() {
        assert_eq!(receipt_authorizes(&op, Some(&receipt), pinned, &wire), None);
    }
    let stale = Receipt {
        predecessor: [1; 32],
        ..receipt.clone()
    };
    let transplanted = Receipt {
        parent: [2; 32],
        ..receipt.clone()
    };
    for (name, trusted_receipt) in [
        ("forged", None),
        ("transplanted", Some(&transplanted)),
        ("stale", Some(&stale)),
        ("created_from_preflight", None),
    ] {
        assert!(
            v["invalid_receipts"]
                .as_array()
                .unwrap()
                .contains(&json!(name))
        );
        assert_eq!(receipt_authorizes(&op, trusted_receipt, None, &good), None);
        assert_eq!(
            receipt_authorizes(&op, trusted_receipt, pinned, &good),
            Some(false)
        );
    }
    assert_eq!(v["fallback"], "requires_independently_accepted_parent");
}

#[test]
fn cc3c_04_both_modes_check_bounds_current_proofs_and_parent_eligibility() {
    let v = vector("CC3C-04");
    let op = Op3::new();
    let f = &op.rotated.ceremony;
    let signed = op.proposal.get(10).unwrap().a().unwrap();
    let replace = |bodies| with_field(&op.proposal, 10, Some(array(bodies)));
    let duplicate = std::iter::once(signed[0].clone())
        .chain(signed.to_vec())
        .collect();
    let mut nineteen = sorted_union(&op.rotated);
    nineteen.extend((1000..1016u128).map(|id| Credential {
        device_id: id.to_be_bytes(),
        ..f.target.clone()
    }));
    assert_eq!(v["maximum_credentials"], 18);
    assert_eq!(v["maximum_preflight_bytes"], 100_000);
    for (name, wire) in [
        (
            "unsorted",
            op.wire(&replace(signed.iter().rev().cloned().collect())),
        ),
        ("duplicate", op.wire(&replace(duplicate))),
        (
            "malformed_body",
            op.wire(&replace(vec![
                bytes([0xa0]),
                signed[1].clone(),
                signed[2].clone(),
            ])),
        ),
        (
            "nineteen",
            op.wire(&with_field(&op.proposal, 10, Some(credentials(nineteen)))),
        ),
        (
            "preflight_over_cap",
            op.wire(&replace(vec![
                signed[0].clone(),
                signed[1].clone(),
                bytes(vec![0; 100_000]),
            ])),
        ),
        ("old_proof_key", op.seal(&op.proposal, &signer(0x81))),
    ] {
        check_bytes(&v["mutations"][name], &wire, "refused");
        assert!(
            op.check(Authority::DurableParent(&f.signed_parent), false, &wire)
                .is_none(),
            "{name}"
        );
        assert!(
            op.check(
                Authority::VerifiedPredecessor(&op.rotated.predecessor),
                false,
                &wire
            )
            .is_none(),
            "{name}"
        );
    }
    let wire = op.wire(&op.proposal);
    let (_, wrong_current) = op
        .membership_mutations()
        .into_iter()
        .find(|(name, _)| *name == "collapse_keep_old")
        .unwrap();
    assert!(
        op.check(
            Authority::DurableParent(&f.signed_parent),
            false,
            &wrong_current
        )
        .is_none()
    );
    assert!(
        op.check(
            Authority::DurableParent(&op.rotated.signed_predecessor),
            false,
            &wire
        )
        .is_none()
    );
    for context in ["stale_parent", "forked_parent"] {
        assert!(v["contexts"].as_array().unwrap().contains(&json!(context)));
        assert!(
            op.check(Authority::DurableParent(&f.signed_parent), true, &wire)
                .is_none()
        );
        assert!(
            op.check(
                Authority::VerifiedPredecessor(&op.rotated.predecessor),
                true,
                &wire
            )
            .is_none()
        );
    }
    let mut request = op.selected_request(&f.replacement, &wire);
    request[5] ^= 1;
    check_bytes(&v["mutations"]["wrong_child_proof"], &request, "refused");
    for authority in [
        Authority::DurableParent(&f.signed_parent),
        Authority::VerifiedPredecessor(&op.rotated.predecessor),
    ] {
        let child = op.check(authority, false, &wire).unwrap();
        assert!(!op.verify_selected_vote(&child, &wire, &request));
    }
    assert_eq!(raw(&v, "child_hash"), f.replacement.hash());
    assert_eq!(v["fixed_link_and_code"], v2("CC3A-01"));
}

fn signed_for(
    key: &DeviceSigner,
    label: &str,
    role: u64,
    id: &[u8],
    body: &[u8],
    rnd: u8,
) -> Vec<u8> {
    let proof = key.sign_with(&ds_sig(label, PROFILE_V1, role, id, body), [rnd; 32]);
    sig::envelope(
        body.to_vec(),
        vec![sig::signature_entry(id, PROFILE_V1, role, proof)],
    )
}

fn accepted_recovery_pin(
    local_pin: Option<&[u8]>,
    offered: &[u8],
    current_hash: [u8; 32],
    forked: bool,
) -> Option<Acs> {
    if forked || local_pin? != offered {
        return None;
    }
    let (p, _) = signed_state(offered)?;
    (p.hash() == current_hash).then_some(p)
}

fn carrier_parts<'a>(
    carrier: &'a V,
    accepted_q: &[u8],
) -> Option<(RecoveryPolicy, &'a [u8], Credential)> {
    if carrier.m().ok()?.len() != 4 || required_bytes(carrier, 0)? != accepted_q {
        return None;
    }
    let policy_bytes = required_bytes(carrier, 1)?;
    let policy = RecoveryPolicy::from_body(policy_bytes).ok()?;
    let target_bytes = required_bytes(carrier, 3)?;
    let target = Credential::from_body(target_bytes).ok()?;
    (policy.body() == policy_bytes && target.body() == target_bytes).then_some((
        policy,
        required_bytes(carrier, 2)?,
        target,
    ))
}

#[test]
fn cc3d_01_04_recovery_parent_provenance_and_operation_3_eligibility() {
    let op = Op3::new();
    let f = &op.rotated.ceremony;
    let q = verify_genesis(
        &op.rotated.signed_predecessor,
        &[op.rotated.previous_origin.body(), f.selected.body()],
    )
    .unwrap();
    let request = RecoveryRequest {
        account_id: ACCOUNT,
        recovery_epoch: OLD_EPOCH,
        new_device_id: TARGET,
        new_device_credential: f.target.hash(),
        parent_state_hash: q.hash(),
        request_nonce: [0x79; 16],
    };
    let sign_request = |r: &RecoveryRequest, key, rnd| {
        signed_for(
            key,
            LBL_REQUEST_SIG,
            ROLE_RECOVERY,
            &ACCOUNT,
            &r.body(),
            rnd,
        )
    };
    let original = sign_request(&request, &f.old_key, 61);
    let mut p = q.clone();
    p.generation += 1;
    p.previous = Some(q.hash());
    p.control = [TARGET].into();
    p.revoked = [ORIGIN, SELECTED].into();
    p.credentials = vec![(TARGET, f.target.hash())];
    p.recovery_commitment = Some(f.current_policy.hash());
    let body = p.body();
    let recovery = vote(&f.old_key, ROLE_RECOVERY, &ACCOUNT, &body, 66);
    let cosign = vote(&signer(0x81), ROLE_CONTROLLER, &ORIGIN, &body, 67);
    let parent = sig::envelope(body.clone(), vec![cosign.clone(), recovery.clone()]);
    let solo = sig::envelope(body.clone(), vec![recovery]);
    let v = vector("CC3D-02");
    assert_eq!(op.rotated.signed_predecessor, raw(&v, "predecessor"));
    assert_eq!(original, raw(&v, "request"));
    assert_eq!(request.digest(), raw(&v, "request_digest").as_slice());
    assert_eq!(solo, raw(&v, "recovery_only_parent"));
    let carrier = map([
        (0, bytes(&op.rotated.signed_predecessor)),
        (1, bytes(f.old_policy.body())),
        (2, bytes(&original)),
        (3, bytes(f.target.body())),
    ]);
    assert_eq!(encode(&carrier), raw(&v, "carrier"));
    let evidence = [
        op.rotated.previous_origin.body(),
        f.selected.body(),
        f.target.body(),
    ];
    let attest = |q: Option<&Acs>,
                  original_digest: Option<[u8; 32]>,
                  wire: &[u8],
                  policy: &RecoveryPolicy,
                  creds: &[Vec<u8>],
                  parent: &[u8],
                  forked,
                  local_elapsed,
                  vetoes: &[Vec<u8>]| {
        if forked {
            return None;
        }
        let q = q?;
        let request = om_core::recovery::open_request(wire, policy.recovery_key()?).ok()?;
        if Some(request.digest()) != original_digest {
            return None;
        }
        verify_recovery_successor(
            q,
            parent,
            creds,
            policy,
            &f.current_policy,
            wire,
            &RecoveryObservation {
                elapsed_seconds: local_elapsed,
                vetoes,
            },
        )
        .ok()
    };
    let accept = |wire| {
        attest(
            Some(&q),
            Some(request.digest()),
            wire,
            &f.old_policy,
            &evidence,
            &parent,
            false,
            0,
            &[],
        )
    };
    let (carried_policy, carried_request, carried_target) =
        carrier_parts(&carrier, &op.rotated.signed_predecessor).unwrap();
    assert_eq!(carried_policy, f.old_policy);
    assert_eq!(carried_target.body(), f.target.body());
    assert!(carrier_parts(&carrier, &parent).is_none());
    assert_eq!(
        accept(carried_request),
        Some(RecoveryVerdict::Complete {
            child: p.clone(),
            immediate: true
        })
    );
    assert_eq!(v["expected"], "historical_recovery_authorized");
    assert_eq!(
        attest(
            Some(&q),
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &solo,
            false,
            f.old_policy.delay_seconds,
            &[]
        ),
        Some(RecoveryVerdict::Complete {
            child: p.clone(),
            immediate: false
        })
    );
    let v = vector("CC3D-01");
    assert_eq!(parent, raw(&v, "parent"));
    assert_eq!(signed_state(&parent).unwrap().0, p);
    let durable = parent.clone();
    let anchored = accepted_recovery_pin(Some(&durable), &parent, p.hash(), false).unwrap();
    assert_eq!(anchored, p);
    assert!(accepted_recovery_pin(None, &parent, p.hash(), false).is_none());
    assert!(accepted_recovery_pin(Some(&durable), &solo, p.hash(), false).is_none());
    assert!(accepted_recovery_pin(Some(&durable), &parent, p.hash(), true).is_none());
    assert_eq!(
        anchored.control.difference(&anchored.quarantined).count(),
        1
    );
    assert!(anchored.control.contains(&TARGET));
    assert_eq!(
        v["expected"],
        "anchor_exact_vote_blocked_ineligible_parties"
    );

    let v = vector("CC3D-03");
    let nonce = sign_request(
        &RecoveryRequest {
            request_nonce: [0x7a; 16],
            ..request.clone()
        },
        &f.old_key,
        62,
    );
    let target = sign_request(
        &RecoveryRequest {
            new_device_id: [0x78; 16],
            ..request.clone()
        },
        &f.old_key,
        63,
    );
    let epoch = sign_request(
        &RecoveryRequest {
            recovery_epoch: 7,
            ..request.clone()
        },
        &f.old_key,
        64,
    );
    let wrong_method = sign_request(&request, &f.current_key, 65);
    let wrong_parent_method = sig::envelope(
        body.clone(),
        vec![
            cosign,
            vote(&f.current_key, ROLE_RECOVERY, &ACCOUNT, &body, 66),
        ],
    );
    assert_eq!(wrong_parent_method, raw(&v, "wrong_parent_method"));
    assert!(
        attest(
            Some(&q),
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &wrong_parent_method,
            false,
            0,
            &[]
        )
        .is_none()
    );
    for (name, wire) in [
        ("nonce_substitution", &nonce),
        ("wrong_target", &target),
        ("wrong_epoch", &epoch),
        ("wrong_method", &wrong_method),
    ] {
        assert_eq!(*wire, raw(&v, name));
        assert!(accept(wire).is_none(), "{name}");
    }
    let changed_carrier = with_field(&carrier, 2, Some(bytes(&nonce)));
    assert_eq!(encode(&changed_carrier), raw(&v, "changed_carrier"));
    let (_, substituted_request, _) =
        carrier_parts(&changed_carrier, &op.rotated.signed_predecessor).unwrap();
    assert!(accept(substituted_request).is_none());
    let asserted_time = with_field(&carrier, 4, Some(V::U(f.old_policy.delay_seconds)));
    assert_eq!(encode(&asserted_time), raw(&v, "borrowed_wait_carrier"));
    assert!(carrier_parts(&asserted_time, &op.rotated.signed_predecessor).is_none());
    assert!(
        verify_recovery_successor(
            &q,
            &parent,
            &evidence,
            &f.current_policy,
            &f.current_policy,
            &original,
            &RecoveryObservation {
                elapsed_seconds: 0,
                vetoes: &[]
            }
        )
        .is_err()
    );
    assert!(
        verify_recovery_successor(
            &q,
            &parent,
            &f.credentials(),
            &f.old_policy,
            &f.current_policy,
            &original,
            &RecoveryObservation {
                elapsed_seconds: 0,
                vetoes: &[]
            }
        )
        .is_err()
    );
    let credential = with_field(&decode(&f.target.body()).unwrap(), 2, Some(bytes(ACCOUNT)));
    assert_eq!(encode(&credential), raw(&v, "fake_account_credential"));
    assert!(Credential::from_body(&encode(&credential)).is_err());
    let veto = RecoveryVeto {
        account_id: ACCOUNT,
        recovery_epoch: OLD_EPOCH,
        request_digest: request.digest(),
    };
    let veto = signed_for(
        &signer(0x81),
        LBL_VETO_SIG,
        ROLE_CONTROLLER,
        &ORIGIN,
        &veto.body(),
        65,
    );
    assert_eq!(veto, raw(&v, "veto"));
    assert!(
        attest(
            Some(&q),
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &parent,
            false,
            f.old_policy.delay_seconds,
            &[veto]
        )
        .is_none()
    );
    assert!(
        attest(
            None,
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &parent,
            false,
            0,
            &[]
        )
        .is_none()
    );
    assert!(
        attest(
            Some(&q),
            None,
            &original,
            &f.old_policy,
            &evidence,
            &parent,
            false,
            0,
            &[]
        )
        .is_none()
    );
    assert!(
        attest(
            Some(&q),
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &parent,
            true,
            0,
            &[]
        )
        .is_none()
    );
    assert!(matches!(
        attest(
            Some(&q),
            Some(request.digest()),
            &original,
            &f.old_policy,
            &evidence,
            &solo,
            false,
            0,
            &[]
        ),
        Some(RecoveryVerdict::Pending { .. })
    ));
    assert_eq!(v["expected"], "refused");

    let v = vector("CC3D-04");
    let over = with_field(&carrier, 2, Some(bytes(vec![0; 100_000])));
    let proposal = with_field(
        &with_field(&op.proposal, 1, Some(bytes(&parent))),
        11,
        Some(over),
    );
    let wire = op.wire(&proposal);
    check_bytes(&v["oversize_preflight"], &wire, "refused_before_decode");
    assert!(wire.len() > 100_000);
    assert!(
        op.check(Authority::DurableParent(&durable), false, &wire)
            .is_none()
    );
    assert!(
        op.check(Authority::VerifiedPredecessor(&q), false, &wire)
            .is_none()
    );
    assert_eq!(v["optional_key"], 11);
    assert_eq!(v["fixed_link_and_code"], v2("CC3A-01"));
    assert!(
        op.check(
            Authority::DurableParent(&f.signed_parent),
            false,
            &op.wire(&op.proposal)
        )
        .is_some()
    );
}
