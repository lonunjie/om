// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::controller::{
    FIXED_JOIN_TIME, binding, ceremony_id, checked_reply, checked_vote, decode_op3_offer,
    preflight_input, raw_hash, reply_input, request_input,
};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305,
    aead::{Aead, Payload},
};
use om_core::crypto::kdf::hkdf_sha256;
use om_core::crypto::recovery_approval::{self as ra, ROLE_REQ, ROLE_RESP};
use std::collections::BTreeSet;

const MAX_PREFLIGHT: usize = 100_000;
const MAX_CREDENTIALS: usize = 18;

type CredentialRef = ([u8; 16], [u8; 32]);

fn credential_ref(credential: &Credential) -> CredentialRef {
    (credential.device_id, credential.hash())
}

fn committed(state: &Acs, device: &[u8; 16]) -> Option<CredentialRef> {
    state.credentials.iter().copied().find(|(d, _)| d == device)
}

pub(super) fn sorted_union(f: &Rotated) -> Vec<Credential> {
    let mut bodies = vec![
        f.previous_origin.clone(),
        f.ceremony.origin.clone(),
        f.ceremony.selected.clone(),
    ];
    bodies.sort_by_key(credential_ref);
    bodies
}

// The inputs describe authority already established by the receiver. The
// proposed envelope and its credentials cannot create any of these inputs.
pub(super) enum Authority<'a> {
    DurableParent(&'a [u8]),
    VerifiedPredecessor(&'a Acs),
    Untrusted,
}

pub(super) fn check_preflight(
    authority: Authority<'_>,
    forked_or_stale: bool,
    id: &[u8; 32],
    link_hash: &[u8; 32],
    role: u8,
    plaintext: &[u8],
) -> Option<Acs> {
    if forked_or_stale || role != ROLE_RESP || plaintext.len() > MAX_PREFLIGHT {
        return None;
    }
    let envelope = decode(plaintext).ok()?;
    let [V::U(3), V::B(body), V::B(proof)] = envelope.a().ok()? else {
        return None;
    };
    let proposal = decode(body).ok()?;
    if proposal.m().ok()?.len() != 11 || proposal.get(0).ok()?.u().ok()? != 2 {
        return None;
    }
    let parent_wire = required_bytes(&proposal, 1)?;
    let (parent, signed_parent) = signed_state(parent_wire)?;
    let signer_ids: BTreeSet<[u8; 16]> = signed_parent
        .get(2)
        .ok()?
        .a()
        .ok()?
        .iter()
        .map(|entry| required_bytes(entry, 0)?.try_into().ok())
        .collect::<Option<_>>()?;

    let raws = proposal.get(10).ok()?.a().ok()?;
    if !(2..=MAX_CREDENTIALS).contains(&raws.len()) {
        return None;
    }
    let mut bodies = Vec::with_capacity(raws.len());
    for raw in raws {
        let raw = raw.b().ok()?;
        let credential = Credential::from_body(raw).ok()?;
        if credential.body() != raw || credential.account_id != parent.account_id {
            return None;
        }
        bodies.push(credential);
    }
    let identities: Vec<_> = bodies.iter().map(credential_ref).collect();
    if !identities.windows(2).all(|w| w[0] < w[1]) {
        return None;
    }
    let parts = [committed(&parent, &ORIGIN)?, committed(&parent, &SELECTED)?];
    if ![ORIGIN, SELECTED]
        .iter()
        .all(|d| parent.control.contains(d) && !parent.quarantined.contains(d))
    {
        return None;
    }
    match authority {
        Authority::DurableParent(accepted) if accepted == parent_wire => {
            // Historical hashes are unchecked here. The supplied entries are
            // supplementary; the trusted exact envelope is the starting state.
            let supplements: Vec<_> = identities
                .iter()
                .filter(|pair| !parts.contains(pair))
                .collect();
            let supplement_ids: BTreeSet<_> = supplements.iter().map(|pair| pair.0).collect();
            if supplements.len() != supplement_ids.len()
                || !supplement_ids.is_subset(&signer_ids)
                || signer_ids
                    .iter()
                    .any(|d| !identities.iter().any(|pair| pair.0 == *d))
            {
                return None;
            }
        }
        Authority::VerifiedPredecessor(q) => {
            let mut expected = BTreeSet::from(parts);
            for signer in signer_ids {
                expected.insert(committed(q, &signer)?);
            }
            if !identities.iter().copied().eq(expected) {
                return None;
            }
            let credential_bytes: Vec<_> = bodies.iter().map(Credential::body).collect();
            if verify_successor(q, parent_wire, &credential_bytes).ok()? != parent {
                return None;
            }
        }
        _ => return None,
    }
    let origin = bodies.iter().find(|c| credential_ref(c) == parts[0])?;
    bodies.iter().find(|c| credential_ref(c) == parts[1])?;
    sig::verify(&origin.signing_pk, &preflight_input(id, body), proof).ok()?;

    let old = RecoveryPolicy::from_body(required_bytes(&proposal, 9)?).ok()?;
    let new = RecoveryPolicy::from_body(required_bytes(&proposal, 3)?).ok()?;
    let child_wire = required_bytes(&proposal, 2)?;
    let child = Acs::from_body(child_wire).ok()?;
    let mut expected = parent.clone();
    expected.generation = expected.generation.checked_add(1)?;
    expected.previous = Some(parent.hash());
    expected.recovery_commitment = Some(new.hash());
    let refs = array(
        parent
            .control
            .iter()
            .filter(|d| !parent.quarantined.contains(*d))
            .map(|d| {
                let (device, digest) = committed(&parent, d)?;
                Some(array([bytes(device), bytes(digest)]))
            })
            .collect::<Option<Vec<_>>>()?,
    );
    (child_wire == child.body()
        && required_bytes(&proposal, 4)? == parent.hash()
        && required_bytes(&proposal, 5)? == child.hash()
        && child.hash() == *link_hash
        && child == expected
        && parent.recovery_commitment == Some(old.hash())
        && proposal.get(6).ok()?.u().ok()? == old.epoch()
        && proposal.get(7).ok()?.u().ok()? == new.epoch()
        && old.epoch().checked_add(1)? == new.epoch()
        && proposal.get(8).ok()? == &refs)
        .then_some(child)
}

pub(super) fn preflight(proposal: &[u8], id: &[u8; 32], key: &DeviceSigner) -> Vec<u8> {
    let proof = key.sign_with(&preflight_input(id, proposal), [15; 32]);
    encode(&array([V::U(3), bytes(proposal), bytes(proof)]))
}

#[test]
fn cc3b_01_two_credential_eras_and_complete_transcript() {
    let f = Rotated::new();
    let c = &f.ceremony;
    let vector = golden("CC3B-01");
    let (origin, offer, hash) =
        decode_op3_offer(vector["link"].as_str().unwrap(), FIXED_JOIN_TIME).unwrap();
    assert_eq!(origin, ORIGIN);
    assert_eq!(hash, c.replacement.hash());
    assert_eq!(f.signed_predecessor, raw(&vector, "predecessor"));
    assert_eq!(
        f.predecessor.hash().as_slice(),
        raw(&vector, "predecessor_hash")
    );
    assert_eq!(c.signed_parent, raw(&vector, "parent"));
    assert_eq!(c.parent.hash().as_slice(), raw(&vector, "parent_hash"));
    assert_eq!(c.replacement.body(), raw(&vector, "child"));
    let binding = binding(&c.parent, c.selected.hash(), hash);
    let id = ceremony_id(&binding, &offer);
    assert_eq!(binding, raw(&vector, "binding"));
    assert_eq!(id.as_slice(), raw(&vector, "ceremony_id"));

    let proposal = encode(&map([
        (0, V::U(2)),
        (1, bytes(&c.signed_parent)),
        (2, bytes(c.replacement.body())),
        (3, bytes(c.current_policy.body())),
        (4, bytes(c.parent.hash())),
        (5, bytes(hash)),
        (6, V::U(OLD_EPOCH)),
        (7, V::U(EPOCH)),
        (
            8,
            array(
                c.parent
                    .credentials
                    .iter()
                    .map(|(d, h)| array([bytes(d), bytes(h)])),
            ),
        ),
        (9, bytes(c.old_policy.body())),
        (
            10,
            array(
                sorted_union(&f)
                    .iter()
                    .map(|credential| bytes(credential.body())),
            ),
        ),
    ]));
    assert_eq!(proposal, raw(&vector, "proposal"));
    assert_eq!(
        preflight_input(&id, &proposal),
        raw(&vector, "preflight_input")
    );
    let preflight = preflight(&proposal, &id, &c.origin_key);
    assert_eq!(preflight, raw(&vector, "preflight"));
    let child = check_preflight(
        Authority::VerifiedPredecessor(&f.predecessor),
        false,
        &id,
        &hash,
        ROLE_RESP,
        &preflight,
    )
    .unwrap();
    assert_eq!(child, c.replacement);

    let selected = vote(
        &c.selected_key,
        ROLE_CONTROLLER,
        &SELECTED,
        &child.body(),
        16,
    );
    assert_eq!(encode(&selected), raw(&vector, "selected_vote"));
    let partial = sig::envelope(child.body(), vec![selected.clone()]);
    assert!(verify_successor(&c.parent, &partial, &c.credentials()).is_err());
    let input = request_input(&id, &preflight, &partial);
    assert_eq!(input, raw(&vector, "request_input"));
    let request = encode(&array([
        V::U(1),
        bytes(c.selected_key.sign_with(&input, [17; 32])),
        bytes(partial),
        V::Null,
        raw_hash(&preflight),
    ]));
    assert_eq!(request, raw(&vector, "request"));
    assert_eq!(
        checked_vote(c, &child, &id, &preflight, &request, ROLE_REQ),
        Some(selected.clone())
    );
    assert!(checked_vote(c, &child, &id, &preflight, &request, ROLE_RESP).is_none());

    let origin = vote(&c.origin_key, ROLE_CONTROLLER, &ORIGIN, &child.body(), 18);
    assert_eq!(encode(&origin), raw(&vector, "origin_vote"));
    let head = sig::envelope(child.body(), vec![origin, selected.clone()]);
    let reply_body = encode(&array([V::U(2), bytes(head), V::Null]));
    let input = reply_input(&id, &request, &reply_body);
    assert_eq!(input, raw(&vector, "reply_input"));
    let reply = encode(&array([
        bytes(reply_body),
        bytes(c.origin_key.sign_with(&input, [19; 32])),
    ]));
    assert_eq!(reply, raw(&vector, "reply"));
    // Preflight and final reply are separate origin-to-selected messages.
    // A captured request or response cannot authenticate in the other role.
    let cipher = |purpose| {
        let info = encode(&array([string(ra::LBL_KDF), string(purpose)]));
        let mut key = [0; 32];
        hkdf_sha256(&id, &offer.secret, &info, &mut key);
        XChaCha20Poly1305::new((&key).into())
    };
    for (wire, purpose, other_purpose, role, other_role, nonce) in [
        (&preflight, "resp", "req", ROLE_RESP, ROLE_REQ, [1; 24]),
        (&request, "req", "resp", ROLE_REQ, ROLE_RESP, [2; 24]),
        (&reply, "resp", "req", ROLE_RESP, ROLE_REQ, [3; 24]),
    ] {
        let ad = ra::aead_ad(&id, role);
        let other_ad = ra::aead_ad(&id, other_role);
        let ciphertext = cipher(purpose)
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: wire,
                    aad: &ad,
                },
            )
            .unwrap();
        let authenticated = || Payload {
            msg: &ciphertext,
            aad: &ad,
        };
        let reversed = Payload {
            msg: &ciphertext,
            aad: &other_ad,
        };
        assert_eq!(
            cipher(purpose)
                .decrypt((&nonce).into(), authenticated())
                .unwrap(),
            *wire
        );
        assert!(cipher(purpose).decrypt((&nonce).into(), reversed).is_err());
        assert!(
            cipher(other_purpose)
                .decrypt((&nonce).into(), authenticated())
                .is_err()
        );
    }
    assert_eq!(
        checked_reply(c, &child, &id, &request, &selected, &reply, ROLE_RESP),
        Some(child.clone())
    );
    assert!(checked_reply(c, &child, &id, &request, &selected, &reply, ROLE_REQ).is_none());
    assert!(checked_reply(c, &child, &id, &request, &selected, &preflight, ROLE_RESP).is_none());
}

#[test]
fn cc3b_02_03_independent_authority_and_preflight_rejections() {
    let f = Rotated::new();
    let c = &f.ceremony;
    let vector = golden("CC3B-01");
    let id = raw(&vector, "ceremony_id").try_into().unwrap();
    let hash = c.replacement.hash();
    let wire = raw(&vector, "preflight");
    let check = |authority: Authority<'_>, bytes: &[u8]| {
        check_preflight(authority, false, &id, &hash, ROLE_RESP, bytes)
    };
    let anchor = || Authority::DurableParent(&c.signed_parent);
    let predecessor = || Authority::VerifiedPredecessor(&f.predecessor);
    assert_eq!(check(anchor(), &wire), Some(c.replacement.clone()));
    assert_eq!(check(predecessor(), &wire), Some(c.replacement.clone()));
    assert!(check(Authority::Untrusted, &wire).is_none());
    assert!(check(Authority::DurableParent(&f.signed_predecessor), &wire).is_none());
    assert!(check_preflight(anchor(), true, &id, &hash, ROLE_RESP, &wire).is_none());
    assert!(check_preflight(anchor(), false, &id, &hash, ROLE_REQ, &wire).is_none());
    assert!(check_preflight(anchor(), false, &id, &[0; 32], ROLE_RESP, &wire).is_none());

    let proposal_wire = raw(&vector, "proposal");
    let proposal = decode(&proposal_wire).unwrap();
    let mut tampered = wire.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(check(anchor(), &tampered).is_none());
    assert!(check(anchor(), &preflight(&proposal_wire, &id, &signer(0x81))).is_none());
    assert!(check(anchor(), &preflight(&proposal_wire, &id, &c.selected_key)).is_none());

    let rewrap = |key, field| {
        preflight(
            &encode(&with_field(&proposal, key, Some(field))),
            &id,
            &c.origin_key,
        )
    };
    for (field, value) in [
        (6, V::U(OLD_EPOCH - 1)),
        (7, V::U(EPOCH + 1)),
        (9, bytes(c.current_policy.body())),
    ] {
        assert!(check(anchor(), &rewrap(field, value)).is_none());
    }
    let bad_parent = sig::envelope(
        c.parent.body(),
        vec![vote(
            &c.origin_key,
            ROLE_CONTROLLER,
            &ORIGIN,
            &c.parent.body(),
            14,
        )],
    );
    let wrong_key = rewrap(1, bytes(bad_parent));
    assert!(check(anchor(), &wrong_key).is_none());
    assert!(check(predecessor(), &wrong_key).is_none());

    let bodies = proposal.get(10).unwrap().a().unwrap().to_vec();
    let mut swapped = bodies.clone();
    swapped.swap(0, 1);
    let mut extra = bodies.clone();
    extra.push(bytes(c.target.body()));
    let mut duplicate = bodies.clone();
    duplicate.insert(0, bodies[0].clone());
    let missing_old = array(c.credentials().into_iter().map(bytes));
    for invalid in [
        array(swapped),
        array(extra),
        array(duplicate),
        array((0..19).map(|i| {
            let mut cred = c.target.clone();
            cred.device_id = [i; 16];
            bytes(cred.body())
        })),
    ] {
        let wire = rewrap(10, invalid);
        assert!(check(anchor(), &wire).is_none());
        assert!(check(predecessor(), &wire).is_none());
    }
    assert!(check(predecessor(), &rewrap(10, missing_old)).is_none());
    let mut wrong_old = f.previous_origin.clone();
    wrong_old.signing_pk = signer(0x85).public();
    let mut wrong_union = vec![wrong_old, c.origin.clone(), c.selected.clone()];
    wrong_union.sort_by_key(credential_ref);
    let wrong_old = rewrap(10, array(wrong_union.iter().map(|c| bytes(c.body()))));
    assert!(check(predecessor(), &wrong_old).is_none());
    // The anchor-only receiver checks the exact parent and the current keys;
    // it has no authority for the supplied historical credential hash.
    assert_eq!(check(anchor(), &wrong_old), Some(c.replacement.clone()));
    assert!(
        check(
            predecessor(),
            &rewrap(10, array([bytes(vec![0; MAX_PREFLIGHT])]))
        )
        .is_none()
    );
    assert_eq!(golden("CC3B-03")["maximum_credentials"], MAX_CREDENTIALS);
    assert_eq!(golden("CC3B-03")["maximum_preflight_bytes"], MAX_PREFLIGHT);
    assert_eq!(v2("CC3A-01")["code"], "411048");
}
