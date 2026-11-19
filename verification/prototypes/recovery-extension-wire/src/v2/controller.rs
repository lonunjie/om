// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tests::transcript;
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305,
    aead::{Aead, Payload},
};
use om_core::control::{open_signed, verify_successor};
use om_core::crypto::cbor::decode;
use om_core::crypto::kdf::hkdf_sha256;
use om_core::crypto::recovery_approval as ra;
use om_core::crypto::sig::{ROLE_DEVICE, ds_hash, ds_sig};
use om_core::crypto::{hex, sha256};
use serde_json::json;

const PREFIX: &str = "om:controller-change/";
const PURPOSE: &str = "om.controller_change.v1";
const PREFLIGHT: &str = "OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1";
const REQUEST: &str = "OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1";
const MAX_PLAINTEXT: usize = 100_000;
const FIXED_JOIN_TIME: u32 = 1_799_999_700;

fn decode_op3_offer(link: &str, now: u32) -> Option<([u8; 16], ra::Offer, [u8; 32])> {
    fn lower_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
        if text.len() != N * 2 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return None;
        }
        unhex(text).ok()?.try_into().ok()
    }

    let parts: Vec<_> = link.strip_prefix(PREFIX)?.split('/').collect();
    let ["3", origin, token, child_hash] = parts.as_slice() else {
        return None;
    };
    if token.len() > ra::MAX_OFFER_TOKEN
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return None;
    }
    let carrier = format!("{}{token}", ra::OFFER_PREFIX);
    let offer = ra::decode_offer(&carrier).ok()?;
    let remaining = offer.expiry_s.checked_sub(now)?;
    ((1..=360).contains(&remaining) && ra::encode_offer(&offer)? == carrier).then_some((
        lower_hex(origin)?,
        offer,
        lower_hex(child_hash)?,
    ))
}

#[test]
fn cc3a_01_fixed_link_preserves_binding_and_legacy_transcripts() {
    let old = transcript(3);
    let short_link = old["link"].as_str().unwrap();
    let mut new = old.clone();
    new["link"] = json!(format!("{short_link}/{}", "64".repeat(32)));
    assert_eq!(new, golden("CC3A-01"));
    assert_eq!(new["code"], "411048");
    let link = new["link"].as_str().unwrap();
    let (origin, offer, hash) = decode_op3_offer(link, FIXED_JOIN_TIME).unwrap();
    assert_eq!(origin, [0x31; 16]);
    assert_eq!(offer.expiry_s, 1_800_000_000);
    assert_eq!(hash, [0x64; 32]);
    assert!(decode_op3_offer(link, offer.expiry_s - 360).is_some());
    assert!(decode_op3_offer(link, offer.expiry_s - 361).is_none());
    assert!(decode_op3_offer(link, offer.expiry_s).is_none());
    for invalid in [
        short_link.to_owned(),
        format!("{short_link}/{}", "AF".repeat(32)),
        format!("{short_link}/{}", "z0".repeat(32)),
        format!("{link}#fragment"),
        format!("{link}?query"),
        format!("{link}/extra"),
        link.replacen("/3/", "/1/", 1),
    ] {
        assert!(
            decode_op3_offer(&invalid, FIXED_JOIN_TIME).is_none(),
            "{invalid}"
        );
    }
    assert_eq!(transcript(1), golden("operation-1-transcript"));
    let previous: Value = serde_json::from_str(include_str!(
        "../../../../vectors/controller-change-v1.json"
    ))
    .unwrap();
    assert_eq!(transcript(0), previous);
}

fn binding(parent: &Acs, selected_hash: [u8; 32], child_hash: [u8; 32]) -> Vec<u8> {
    encode(&array([
        string(PURPOSE),
        V::U(3),
        bytes(parent.account_id),
        bytes(parent.hash()),
        bytes(ORIGIN),
        bytes(SELECTED),
        bytes(selected_hash),
        bytes(child_hash),
    ]))
}

fn ceremony_id(binding: &[u8], offer: &ra::Offer) -> [u8; 32] {
    let route = ra::context(
        offer.expiry_s.into(),
        offer.uri.as_bytes(),
        &offer.a_cap,
        &offer.b_recv,
    );
    ds_hash(ra::LBL_KDF, &encode(&array([bytes(binding), bytes(route)])))
}

fn raw_hash(data: &[u8]) -> V {
    bytes(sha256(&[data]))
}

fn preflight_input(id: &[u8; 32], proposal: &[u8]) -> Vec<u8> {
    let body = encode(&array([bytes(id), raw_hash(proposal)]));
    ds_sig(PREFLIGHT, PROFILE_V1, ROLE_CONTROLLER, &ORIGIN, &body)
}

fn request_input(id: &[u8; 32], preflight: &[u8], partial_child: &[u8]) -> Vec<u8> {
    let body = encode(&array([
        bytes(id),
        raw_hash(preflight),
        raw_hash(partial_child),
    ]));
    ds_sig(REQUEST, PROFILE_V1, ROLE_DEVICE, &SELECTED, &body)
}

fn reply_input(id: &[u8; 32], request: &[u8], reply_body: &[u8]) -> Vec<u8> {
    encode(&array([
        string(PURPOSE),
        string("reply"),
        bytes(id),
        raw_hash(request),
        raw_hash(reply_body),
    ]))
}

fn proposal(f: &Fixture) -> Vec<u8> {
    let refs = f
        .parent
        .credentials
        .iter()
        .map(|(device, hash)| array([bytes(device), bytes(hash)]));
    encode(&map([
        (0, V::U(2)),
        (1, bytes(&f.signed_parent)),
        (2, bytes(f.replacement.body())),
        (3, bytes(f.current_policy.body())),
        (4, bytes(f.parent.hash())),
        (5, bytes(f.replacement.hash())),
        (6, V::U(OLD_EPOCH)),
        (7, V::U(EPOCH)),
        (8, array(refs)),
        (9, bytes(f.old_policy.body())),
        (10, array(f.credentials().into_iter().map(bytes))),
    ]))
}

// These validators are scoped to the fixed, independently verified two-controller
// fixture. They have no persistence, timers, authority or network entry point.
fn checked_preflight(
    f: &Fixture,
    pin: &Acs,
    id: &[u8; 32],
    link_hash: &[u8; 32],
    plaintext: &[u8],
) -> Option<Acs> {
    if plaintext.len() > MAX_PLAINTEXT {
        return None;
    }
    let value = decode(plaintext).ok()?;
    let [V::U(3), V::B(body), V::B(proof)] = value.a().ok()? else {
        return None;
    };
    sig::verify(&f.origin.signing_pk, &preflight_input(id, body), proof).ok()?;
    let p = decode(body).ok()?;
    if p.m().ok()?.len() != 11 || p.get(0).ok()?.u().ok()? != 2 {
        return None;
    }
    let parent = verify_genesis(required_bytes(&p, 1)?, &f.credentials()).ok()?;
    let child = Acs::from_body(required_bytes(&p, 2)?).ok()?;
    let old = RecoveryPolicy::from_body(required_bytes(&p, 9)?).ok()?;
    let new = RecoveryPolicy::from_body(required_bytes(&p, 3)?).ok()?;
    let mut expected = pin.clone();
    expected.generation = expected.generation.checked_add(1)?;
    expected.previous = Some(pin.hash());
    expected.recovery_commitment = Some(new.hash());
    let refs = array(
        pin.credentials
            .iter()
            .map(|(d, h)| array([bytes(d), bytes(h)])),
    );
    (parent == *pin
        && required_bytes(&p, 4)? == pin.hash()
        && required_bytes(&p, 5)? == child.hash()
        && child.hash() == *link_hash
        && child == expected
        && parent.recovery_commitment == Some(old.hash())
        && p.get(6).ok()?.u().ok()? == old.epoch()
        && p.get(7).ok()?.u().ok()? == new.epoch()
        && old.epoch().checked_add(1)? == new.epoch()
        && p.get(8).ok()? == &refs
        && p.get(10).ok()? == &array(f.credentials().into_iter().map(bytes)))
        .then_some(child)
}

fn checked_vote(
    f: &Fixture,
    child: &Acs,
    id: &[u8; 32],
    preflight: &[u8],
    request: &[u8],
    role: u8,
) -> Option<V> {
    if role != ra::ROLE_REQ {
        return None;
    }
    let value = decode(request).ok()?;
    let [V::U(1), V::B(proof), V::B(partial), V::Null, hash] = value.a().ok()? else {
        return None;
    };
    if *hash != raw_hash(preflight) {
        return None;
    }
    sig::verify(
        &f.selected.signing_pk,
        &request_input(id, preflight, partial),
        proof,
    )
    .ok()?;
    let (proposed, _, signers) = open_signed(partial, &f.credentials(), Some(&f.parent)).ok()?;
    if proposed != *child || signers != [SELECTED].into() {
        return None;
    }
    decode(partial)
        .ok()?
        .get(2)
        .ok()?
        .a()
        .ok()?
        .first()
        .cloned()
}

fn checked_reply(
    f: &Fixture,
    child: &Acs,
    id: &[u8; 32],
    request: &[u8],
    selected_vote: &V,
    reply: &[u8],
    role: u8,
) -> Option<Acs> {
    if role != ra::ROLE_RESP {
        return None;
    }
    let val = decode(reply).ok()?;
    let [V::B(body), V::B(proof)] = val.a().ok()? else {
        return None;
    };
    sig::verify(&f.origin.signing_pk, &reply_input(id, request, body), proof).ok()?;
    let body = decode(body).ok()?;
    let [V::U(2), V::B(signed), V::Null] = body.a().ok()? else {
        return None;
    };
    let envelope = decode(signed).ok()?;
    let entries = envelope.get(2).ok()?.a().ok()?;
    if entries.len() != 2 || !entries.contains(selected_vote) {
        return None;
    }
    let verified = verify_successor(&f.parent, signed, &f.credentials()).ok()?;
    (verified == *child).then_some(verified)
}

#[test]
fn cc3a_02_signed_preflight_selected_vote_and_origin_reply() {
    let f = Fixture::new();
    let vector = golden("CC3A-02");
    let old = transcript(3);
    let token = old["link"].as_str().unwrap().rsplit('/').next().unwrap();
    let link = format!(
        "{PREFIX}3/{}/{token}/{}",
        hex(&ORIGIN),
        hex(&f.replacement.hash())
    );
    assert_eq!(link, vector["link"]);
    let (origin, offer, link_hash) = decode_op3_offer(&link, FIXED_JOIN_TIME).unwrap();
    assert_eq!(origin, ORIGIN);
    let binding = binding(&f.parent, f.selected.hash(), link_hash);
    assert_eq!(binding, raw(&vector, "binding"));
    let id = ceremony_id(&binding, &offer);
    assert_eq!(id.as_slice(), raw(&vector, "ceremony_id"));
    assert_eq!(f.parent.body(), raw(&vector, "parent_body"));
    assert_eq!(f.replacement.body(), raw(&vector, "child_body"));

    let proposal = proposal(&f);
    assert_eq!(proposal, raw(&vector, "proposal"));
    let input = preflight_input(&id, &proposal);
    assert_eq!(input, raw(&vector, "preflight_input"));
    let proof = f.origin_key.sign_with(&input, [3; 32]);
    assert_eq!(proof, raw(&vector, "preflight_signature"));
    let preflight = encode(&array([V::U(3), bytes(proposal), bytes(proof)]));
    assert_eq!(preflight, raw(&vector, "preflight"));
    let candidate = checked_preflight(&f, &f.parent, &id, &link_hash, &preflight).unwrap();

    // The test invokes signing in the order of the two required code confirmations.
    // It does not exercise the UI or durable state that must enforce that ordering.
    let selected = vote(
        &f.selected_key,
        ROLE_CONTROLLER,
        &SELECTED,
        &candidate.body(),
        4,
    );
    assert_eq!(encode(&selected), raw(&vector, "selected_vote"));
    let partial = sig::envelope(candidate.body(), vec![selected.clone()]);
    assert!(verify_successor(&f.parent, &partial, &f.credentials()).is_err());
    let input = request_input(&id, &preflight, &partial);
    assert_eq!(input, raw(&vector, "request_input"));
    let proof = f.selected_key.sign_with(&input, [5; 32]);
    assert_eq!(proof, raw(&vector, "request_signature"));
    let request = encode(&array([
        V::U(1),
        bytes(proof),
        bytes(partial),
        V::Null,
        raw_hash(&preflight),
    ]));
    assert_eq!(request, raw(&vector, "request"));
    assert_eq!(
        checked_vote(&f, &candidate, &id, &preflight, &request, ra::ROLE_REQ),
        Some(selected.clone())
    );

    let origin = vote(
        &f.origin_key,
        ROLE_CONTROLLER,
        &ORIGIN,
        &candidate.body(),
        6,
    );
    assert_eq!(encode(&origin), raw(&vector, "origin_vote"));
    let distinct = sig::envelope(candidate.body(), vec![origin, selected.clone()]);
    let duplicate = sig::envelope(candidate.body(), vec![selected.clone(), selected.clone()]);
    assert!(verify_successor(&f.parent, &duplicate, &f.credentials()).is_err());
    let reply_body = encode(&array([V::U(2), bytes(distinct), V::Null]));
    let input = reply_input(&id, &request, &reply_body);
    assert_eq!(input, raw(&vector, "reply_input"));
    let proof = f.origin_key.sign_with(&input, [7; 32]);
    assert_eq!(proof, raw(&vector, "reply_signature"));
    let reply = encode(&array([bytes(reply_body), bytes(proof)]));
    assert_eq!(reply, raw(&vector, "reply"));
    assert_eq!(
        checked_reply(
            &f,
            &candidate,
            &id,
            &request,
            &selected,
            &reply,
            ra::ROLE_RESP
        ),
        Some(candidate)
    );
}

#[test]
fn cc3a_03_reject_mutated_transcript_and_wrong_signers() {
    let f = Fixture::new();
    let vector = golden("CC3A-02");
    let id = raw(&vector, "ceremony_id").try_into().unwrap();
    let preflight = raw(&vector, "preflight");
    let proposal = raw(&vector, "proposal");
    let request = raw(&vector, "request");
    let reply = raw(&vector, "reply");
    let selected = decode(&raw(&vector, "selected_vote")).unwrap();
    let link_hash = f.replacement.hash();
    let wrong_link_hash = [0x64; 32];
    assert!(checked_preflight(&f, &f.parent, &id, &wrong_link_hash, &preflight).is_none());
    assert!(checked_preflight(&f, &f.replacement, &id, &link_hash, &preflight).is_none());

    // Re-sign modified proposals with the correct origin so consistency checks,
    // rather than merely proof failure, must reject them.
    let decoded = decode(&proposal).unwrap();
    for field in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
        let mutated = match decoded.get(field).unwrap() {
            V::B(data) => {
                let mut data = data.clone();
                *data.last_mut().unwrap() ^= 1;
                V::B(data)
            }
            V::U(n) => V::U(n + 1),
            V::A(items) => array(items.iter().rev().cloned()),
            _ => unreachable!(),
        };
        let body = encode(&with_field(&decoded, field, Some(mutated)));
        let proof = f
            .origin_key
            .sign_with(&preflight_input(&id, &body), [31; 32]);
        let bad = encode(&array([V::U(3), bytes(body), bytes(proof)]));
        assert!(
            checked_preflight(&f, &f.parent, &id, &link_hash, &bad).is_none(),
            "proposal field {field}"
        );
    }
    let wrong_proof = f
        .selected_key
        .sign_with(&preflight_input(&id, &proposal), [32; 32]);
    let bad = encode(&array([V::U(3), bytes(&proposal), bytes(wrong_proof)]));
    let negatives = golden("CC3A-03");
    assert_eq!(bad, raw(&negatives, "wrong_origin_preflight"));
    assert!(checked_preflight(&f, &f.parent, &id, &link_hash, &bad).is_none());

    let req = decode(&request).unwrap();
    let mut fields = req.a().unwrap().to_vec();
    fields[4] = bytes([0; 32]);
    assert!(
        checked_vote(
            &f,
            &f.replacement,
            &id,
            &preflight,
            &encode(&array(fields)),
            ra::ROLE_REQ
        )
        .is_none()
    );
    let wrong_direction = negatives["vote_with_response_role"].as_u64().unwrap() as u8;
    assert_eq!(wrong_direction, ra::ROLE_RESP);
    assert!(
        checked_vote(
            &f,
            &f.replacement,
            &id,
            &preflight,
            &request,
            wrong_direction
        )
        .is_none()
    );
    let wrong_vote = vote(
        &f.origin_key,
        ROLE_CONTROLLER,
        &ORIGIN,
        &f.replacement.body(),
        33,
    );
    let wrong_child = sig::envelope(f.replacement.body(), vec![wrong_vote]);
    for (partial, proof_key) in [
        (wrong_child.as_slice(), &f.selected_key),
        (req.a().unwrap()[2].b().unwrap(), &f.origin_key),
    ] {
        let proof = proof_key.sign_with(&request_input(&id, &preflight, partial), [34; 32]);
        let bad = encode(&array([
            V::U(1),
            bytes(proof),
            bytes(partial),
            V::Null,
            raw_hash(&preflight),
        ]));
        assert!(checked_vote(&f, &f.replacement, &id, &preflight, &bad, ra::ROLE_REQ).is_none());
    }
    let replay = |request: &[u8], reply: &[u8], role| {
        checked_reply(&f, &f.replacement, &id, request, &selected, reply, role)
    };
    assert!(replay(&request, &reply, ra::ROLE_REQ).is_none());
    assert!(replay(&preflight, &reply, ra::ROLE_RESP).is_none());
    assert!(replay(&request, &preflight, ra::ROLE_RESP).is_none());
    let reply_object = decode(&reply).unwrap();
    let reply_body = reply_object.a().unwrap()[0].b().unwrap();
    let body_object = decode(reply_body).unwrap();
    let signed = decode(body_object.a().unwrap()[1].b().unwrap()).unwrap();
    for index in 0..2 {
        let mut entries = signed.get(2).unwrap().a().unwrap().to_vec();
        let mut corrupt = entries[index].get(3).unwrap().b().unwrap().to_vec();
        corrupt[64] ^= 1;
        entries[index] = with_field(&entries[index], 3, Some(bytes(corrupt)));
        let tampered = sig::envelope(f.replacement.body(), entries);
        let bad_body = encode(&array([V::U(2), bytes(tampered), V::Null]));
        let bad_proof = f
            .origin_key
            .sign_with(&reply_input(&id, &request, &bad_body), [35; 32]);
        assert!(
            replay(
                &request,
                &encode(&array([bytes(bad_body), bytes(bad_proof)])),
                ra::ROLE_RESP
            )
            .is_none()
        );
    }
    let wrong_reply_proof = f
        .selected_key
        .sign_with(&reply_input(&id, &request, reply_body), [36; 32]);
    assert!(
        replay(
            &request,
            &encode(&array([bytes(reply_body), bytes(wrong_reply_proof)])),
            ra::ROLE_RESP
        )
        .is_none()
    );
    let mut reply_bytes = reply_body.to_vec();
    reply_bytes[0] ^= 1;
    assert!(
        replay(
            &request,
            &encode(&array([
                bytes(reply_bytes),
                reply_object.a().unwrap()[1].clone()
            ])),
            ra::ROLE_RESP
        )
        .is_none()
    );
}

#[test]
fn cc3a_04_role_context_nonce_and_plaintext_bound() {
    let f = Fixture::new();
    let vector = golden("CC3A-02");
    let (_, offer, link_hash) =
        decode_op3_offer(vector["link"].as_str().unwrap(), FIXED_JOIN_TIME).unwrap();
    let binding = binding(&f.parent, f.selected.hash(), link_hash);
    let id = ceremony_id(&binding, &offer);
    let key = |id: &[u8; 32], purpose| {
        let mut key = [0; 32];
        let info = encode(&array([string(ra::LBL_KDF), string(purpose)]));
        hkdf_sha256(id, &offer.secret, &info, &mut key);
        XChaCha20Poly1305::new((&key).into())
    };
    let resp_key = key(&id, "resp");
    let wrong_key = key(&id, "req");
    let resp = ra::aead_ad(&id, ra::ROLE_RESP);
    let req = ra::aead_ad(&id, ra::ROLE_REQ);
    let changed_binding = binding
        .iter()
        .enumerate()
        .map(|(i, byte)| {
            if i == binding.len() - 1 {
                byte ^ 1
            } else {
                *byte
            }
        })
        .collect::<Vec<_>>();
    let changed_id = ceremony_id(&changed_binding, &offer);

    // Primitive-level direction/context negatives, independent of live mailbox state.
    for (field, nonce_byte) in [("preflight", 1), ("reply", 2)] {
        let pt = raw(&vector, field);
        let nonce = [nonce_byte; 24];
        let ciphertext = resp_key
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &pt,
                    aad: &resp,
                },
            )
            .unwrap();
        let payload = |aad| Payload {
            msg: &ciphertext,
            aad,
        };
        assert_eq!(
            resp_key.decrypt((&nonce).into(), payload(&resp)).unwrap(),
            pt
        );
        assert!(resp_key.decrypt((&[0; 24]).into(), payload(&resp)).is_err());
        assert!(resp_key.decrypt((&nonce).into(), payload(&req)).is_err());
        assert!(wrong_key.decrypt((&nonce).into(), payload(&resp)).is_err());
        assert!(
            key(&changed_id, "resp")
                .decrypt((&nonce).into(), payload(&resp))
                .is_err()
        );
        let legacy_id: [u8; 32] = raw(&transcript(1), "ceremony_id").try_into().unwrap();
        assert!(
            key(&legacy_id, "resp")
                .decrypt((&nonce).into(), payload(&resp))
                .is_err()
        );
    }
    assert!(
        checked_preflight(&f, &f.parent, &id, &link_hash, &vec![0; MAX_PLAINTEXT + 1]).is_none()
    );
}
