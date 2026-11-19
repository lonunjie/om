// SPDX-License-Identifier: Apache-2.0

use super::*;
use om_core::crypto::cbor::decode;
use om_core::crypto::sig::{ROLE_RECOVERY, ds_hash, ds_sig};
use om_core::recovery_record::{RecordKeys, RecoveryKit};
use om_core::recovery_sheet::valid_host;

const PLACE: &str = "OM-RECOVERY-KIT-PLACE-BIND-SIG-v1";
const TRANSFER_HASH: &str = "OM-RECOVERY-KIT-TRANSFER-HASH-v1";
const ACK_BIND: &str = "OM-RECOVERY-KIT-ACK-BIND-v1";
const TRANSFER: [u8; 16] = [0x55; 16];

pub(super) fn place_fields(kit: &RecoveryKit, account: &[u8], commitment: &[u8]) -> V {
    let fields = map([
        (0, V::U(1)),
        (1, bytes(account)),
        (2, V::U(kit.epoch)),
        (3, bytes(commitment)),
        (4, bytes(kit.scope)),
        (5, bytes(kit.locator())),
        (6, string(&kit.host)),
    ]);
    with_field(&fields, 7, kit.port.map(|port| V::U(port.into())))
}

pub(super) fn signed_place(
    body: &[u8],
    signer: &DeviceSigner,
    rnd: u8,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let input = ds_sig(PLACE, PROFILE_V1, ROLE_RECOVERY, &ACCOUNT, body);
    let signature = signer.sign_with(&input, [rnd; 32]);
    let entry = sig::signature_entry(&ACCOUNT, PROFILE_V1, ROLE_RECOVERY, signature.clone());
    let certificate = sig::envelope(body.to_vec(), vec![entry]);
    (input, signature, certificate)
}

pub(super) fn transfer_digest(send: &V) -> [u8; 32] {
    let tuple = array([3, 6, 7, 9].map(|field| send.get(field).unwrap().clone()));
    ds_hash(TRANSFER_HASH, &encode(&tuple))
}

pub(super) fn ack_from_send(send: &V) -> V {
    let mut fields: Vec<_> = (0..9).map(|k| (k, send.get(k).unwrap().clone())).collect();
    fields[0].1 = V::U(20);
    fields.swap(4, 5);
    fields[4].0 = 4;
    fields[5].0 = 5;
    V::M(fields)
}

// Verifies the offline case with one known ancestor and capabilities for the
// fixed HTTPS Home. The head and sibling identities enter as independent inputs.
fn admit(
    raw: &[u8],
    head: Option<&Acs>,
    anchor: &Acs,
    sibling: (&[u8; 16], &[u8; 16]),
    advertised_scope: &[u8; 32],
) -> Option<RecoveryKit> {
    admit_bounded(
        raw,
        head,
        anchor,
        sibling,
        advertised_scope,
        om_core::recovery_record::MAX_KIT_BYTES,
        |base| RecoveryKit::from_bytes(&encode(base)).ok(),
    )
}

pub(super) fn admit_bounded(
    raw: &[u8],
    head: Option<&Acs>,
    anchor: &Acs,
    sibling: (&[u8; 16], &[u8; 16]),
    advertised_scope: &[u8; 32],
    max_kit_bytes: usize,
    decode_base: impl FnOnce(&V) -> Option<RecoveryKit>,
) -> Option<RecoveryKit> {
    let head = head?;
    let send = decode(raw).ok()?;
    if send.m().ok()?.len() != 11
        || send.get(0).ok()?.u().ok()? != 19
        || send.get(1).ok()?.u().ok()? != 2
        || required_bytes(&send, 2)?.len() != 16
        || required_bytes(&send, 3)? != head.account_id
        || required_bytes(&send, 4)? != sibling.0
        || required_bytes(&send, 5)? != sibling.1
        || ![sibling.0, sibling.1]
            .iter()
            .all(|d| head.control.contains(*d) && !head.quarantined.contains(*d))
    {
        return None;
    }
    let kit_bytes = required_bytes(&send, 9)?;
    if kit_bytes.len() > max_kit_bytes {
        return None;
    }
    let policy = RecoveryPolicy::from_body(required_bytes(&send, 10)?).ok()?;
    let commitment = policy.hash();
    if head.recovery_commitment != Some(commitment)
        || required_bytes(&send, 7)? != commitment
        || send.get(6).ok()?.u().ok()? != policy.epoch()
        || required_bytes(&send, 8)? != transfer_digest(&send)
    {
        return None;
    }

    let kit_value = decode(kit_bytes).ok()?;
    if let Some(ref_) = kit_value.opt(11) {
        if ref_.m().ok()?.len() != 3
            || required_bytes(ref_, 0)? != anchor.account_id
            || ref_.get(1).ok()?.u().ok()? != anchor.generation
            || required_bytes(ref_, 2)? != anchor.hash()
        {
            return None;
        }
    }
    let certificate = required_bytes(&kit_value, 12)?;
    let envelope = decode(certificate).ok()?;
    if envelope.m().ok()?.len() != 2
        || envelope.get(2).ok()?.a().ok()?.first()?.m().ok()?.len() != 4
    {
        return None;
    }
    let body = sig::open_envelope(
        certificate,
        PLACE,
        PROFILE_V1,
        ROLE_RECOVERY,
        &head.account_id,
        policy.recovery_key()?,
    )
    .ok()?;
    let base = with_field(&with_field(&kit_value, 11, None), 12, None);
    let kit = decode_base(&base)?;
    (kit.epoch == policy.epoch()
        && valid_host(&kit.host)
        && &kit.scope == advertised_scope
        && body == encode(&place_fields(&kit, &head.account_id, &commitment)))
    .then_some(kit)
}

#[test]
fn kits2_01_root_signed_place_and_current_head_policy_goldens() {
    let f = Fixture::new();
    let vector = golden("KITS2-01");
    let head = f.pinned_replacement();
    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    assert_eq!(keys.read_cap.as_slice(), raw(&vector, "read_cap"));
    assert_eq!(keys.write_cap.as_slice(), raw(&vector, "write_cap"));
    assert_eq!(keys.record_key.as_slice(), raw(&vector, "record_key"));
    assert_eq!(keys.locator.as_slice(), raw(&vector, "locator"));
    let policy = f.current_policy.body();
    let commitment = f.current_policy.hash();
    assert_eq!(policy, raw(&vector, "policy"));
    assert_eq!(commitment.as_slice(), raw(&vector, "policy_digest"));

    let kit = RecoveryKit {
        scope: SCOPE,
        host: HOST.into(),
        port: None,
        read_cap: keys.read_cap,
        write_cap: keys.write_cap,
        record_key: keys.record_key,
        inbox_cap: [0x64; 32],
        snapshot_key: None,
        epoch: EPOCH,
    };
    let body = encode(&place_fields(&kit, &ACCOUNT, &commitment));
    assert_eq!(body, raw(&vector, "place_body"));
    let (input, signature, certificate) = signed_place(&body, &f.current_key, 8);
    assert_eq!(input, raw(&vector, "place_input"));
    assert_eq!(signature, raw(&vector, "place_signature"));
    assert_eq!(certificate, raw(&vector, "certificate"));
    let anchor = map([
        (0, bytes(ACCOUNT)),
        (1, V::U(f.parent.generation)),
        (2, bytes(f.parent.hash())),
    ]);
    let kit_value = with_field(&decode(&kit.to_bytes()).unwrap(), 11, Some(anchor));
    let kit_bytes = encode(&with_field(&kit_value, 12, Some(bytes(certificate))));
    assert_eq!(kit_bytes, raw(&vector, "kit"));

    let send = map([
        (0, V::U(19)),
        (1, V::U(2)),
        (2, bytes(TRANSFER)),
        (3, bytes(ACCOUNT)),
        (4, bytes(ORIGIN)),
        (5, bytes(SELECTED)),
        (6, V::U(EPOCH)),
        (7, bytes(commitment)),
        (8, V::Null),
        (9, bytes(kit_bytes)),
        (10, bytes(policy)),
    ]);
    let digest = transfer_digest(&send);
    assert_eq!(digest.as_slice(), raw(&vector, "kit_digest"));
    let send = with_field(&send, 8, Some(bytes(digest)));
    assert_eq!(encode(&send), raw(&vector, "send"));
    assert_eq!(encode(&ack_from_send(&send)), raw(&vector, "ack"));
    let ack_binding = encode(&array([
        string(ACK_BIND),
        bytes(ACCOUNT),
        bytes(SELECTED),
        bytes(ORIGIN),
        V::U(EPOCH),
        bytes(digest),
        bytes(TRANSFER),
    ]));
    assert_eq!(ack_binding, raw(&vector, "ack_binding"));
    let delivered = admit(
        &encode(&send),
        Some(&head),
        &f.parent,
        (&ORIGIN, &SELECTED),
        &SCOPE,
    )
    .unwrap();
    assert_eq!(delivered.to_bytes(), kit.to_bytes());
}

#[test]
fn kits2_02_and_04_reject_untrusted_place_and_policy() {
    let f = Fixture::new();
    let vector = golden("KITS2-01");
    let wire = raw(&vector, "send");
    let send = decode(&wire).unwrap();
    let kit = decode(required_bytes(&send, 9).unwrap()).unwrap();
    let head = f.pinned_replacement();
    let admit_send = |send: &V| {
        admit(
            &encode(send),
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED),
            &SCOPE,
        )
    };
    assert!(admit_send(&send).is_some());
    assert!(admit(&wire, None, &f.parent, (&ORIGIN, &SELECTED), &SCOPE).is_none());
    assert!(admit(&wire, Some(&head), &f.parent, (&SELECTED, &ORIGIN), &SCOPE).is_none());
    assert!(
        admit(
            &wire,
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED),
            &[0; 32]
        )
        .is_none()
    );

    let mut excluded = head.clone();
    excluded.control.remove(&ORIGIN);
    assert!(
        admit(
            &wire,
            Some(&excluded),
            &f.parent,
            (&ORIGIN, &SELECTED),
            &SCOPE
        )
        .is_none()
    );
    for (field, change) in [
        (1, Some(V::U(1))),
        (3, Some(bytes([0; 32]))),
        (6, Some(V::U(OLD_EPOCH))),
        (7, Some(bytes([0x44; 32]))),
        (10, None),
        (10, Some(bytes(f.old_policy.body()))),
    ] {
        let bad = with_field(&send, field, change);
        assert!(admit_send(&bad).is_none(), "send field {field}");
    }

    // Recompute the unkeyed transfer digest, just as a faulty authenticated
    // sibling could. The independently bound place still has to verify.
    let repack = |kit: &V| {
        let bad = with_field(&send, 9, Some(bytes(encode(kit))));
        with_field(&bad, 8, Some(bytes(transfer_digest(&bad))))
    };
    for (field, change) in [
        (2, Some(bytes([0x61; 32]))),
        (3, Some(string("other.example"))),
        (3, Some(string("xn--.example"))),
        (4, Some(V::U(8443))),
        (5, Some(bytes([0x62; 32]))),
        (10, Some(V::U(OLD_EPOCH))),
        (12, None),
    ] {
        let bad = repack(&with_field(&kit, field, change));
        assert!(admit_send(&bad).is_none(), "kit field {field}");
    }

    let body = raw(&vector, "place_body");
    let (_, _, old_signer) = signed_place(&body, &f.old_key, 42);
    assert!(admit_send(&repack(&with_field(&kit, 12, Some(bytes(old_signer))))).is_none());
    let certified = decode(&raw(&vector, "certificate")).unwrap();
    let entry = &certified.get(2).unwrap().a().unwrap()[0];
    for flipped in [0, sig::ED_SIG] {
        let mut corrupt = required_bytes(entry, 3).unwrap().to_vec();
        corrupt[flipped] ^= 1;
        let bad = sig::envelope(
            body.clone(),
            vec![with_field(entry, 3, Some(bytes(corrupt)))],
        );
        assert!(admit_send(&repack(&with_field(&kit, 12, Some(bytes(bad))))).is_none());
    }
    let mut noncanonical = vec![0xb8, 11]; // non-minimal map length
    noncanonical.extend_from_slice(&wire[1..]);
    assert!(
        admit(
            &noncanonical,
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED),
            &SCOPE
        )
        .is_none()
    );

    let stale = with_field(&send, 6, Some(V::U(OLD_EPOCH)));
    let stale = with_field(&stale, 8, Some(bytes(transfer_digest(&stale))));
    assert_eq!(
        encode(&stale),
        raw(&golden("KITS2-02"), "stale_sender_epoch")
    );
    assert!(admit_send(&stale).is_none());

    let placeholder = golden("KITS2-02-placeholder");
    let place = decode(&body).unwrap();
    let replaced = encode(&with_field(&place, 3, Some(bytes([0x44; 32]))));
    assert_eq!(replaced, raw(&placeholder, "place_body"));
    let (input, _, certificate) = signed_place(&replaced, &f.current_key, 9);
    assert_eq!(input, raw(&placeholder, "place_input"));
    assert_eq!(certificate, raw(&placeholder, "certificate"));
    assert_eq!(
        sig::open_envelope(
            &certificate,
            PLACE,
            PROFILE_V1,
            ROLE_RECOVERY,
            &ACCOUNT,
            &f.current_key.public()
        )
        .unwrap(),
        replaced
    );
    let certified = repack(&with_field(&kit, 12, Some(bytes(certificate))));
    assert!(
        admit_send(&certified).is_none(),
        "valid proof but wrong committed digest"
    );
    let claimed = with_field(&certified, 7, Some(bytes([0x44; 32])));
    let claimed = with_field(&claimed, 8, Some(bytes(transfer_digest(&claimed))));
    assert!(
        admit_send(&claimed).is_none(),
        "kit digest does not make a placeholder policy current"
    );
}

#[test]
fn kits2_03_verified_later_head_and_ack_binding() {
    let f = Fixture::new();
    let previous = f.pinned_replacement();
    let mut later = previous.clone();
    later.generation += 1;
    later.previous = Some(previous.hash());
    later.messaging.insert(TARGET);
    later.credentials.push((TARGET, f.target.hash()));
    let body = later.body();
    let entry = vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 43);
    let signed = sig::envelope(body, vec![entry]);
    let mut credentials = f.credentials();
    credentials.push(f.target.body());
    let head = om_core::control::verify_successor(&previous, &signed, &credentials).unwrap();

    let vector = golden("KITS2-01");
    let wire = raw(&vector, "send");
    let sibling = (&ORIGIN, &SELECTED);
    assert!(admit(&wire, None, &f.parent, sibling, &SCOPE).is_none());
    let first = admit(&wire, Some(&head), &f.parent, sibling, &SCOPE).unwrap();
    let repeated = admit(&wire, Some(&head), &f.parent, sibling, &SCOPE).unwrap();
    assert_eq!(first.to_bytes(), repeated.to_bytes());

    // Compare the complete ACK to the expected authenticated sibling payload.
    // Vault persistence and sender debt lifecycle require integration tests.
    let expected_ack = ack_from_send(&decode(&wire).unwrap());
    let matches_ack = |wire: &[u8], authenticated_sender, authenticated_recipient| {
        decode(wire).is_ok_and(|ack| {
            required_bytes(&ack, 4) == Some(authenticated_sender)
                && required_bytes(&ack, 5) == Some(authenticated_recipient)
                && ack == expected_ack
        })
    };
    let ack = raw(&vector, "ack");
    assert_eq!(encode(&expected_ack), ack);
    assert!(matches_ack(&ack, &SELECTED, &ORIGIN));
    assert!(!matches_ack(&ack, &ORIGIN, &SELECTED));
    let other_ack = |key, value| encode(&with_field(&expected_ack, key, Some(value)));
    for (field, mismatch) in [
        (1, V::U(1)),
        (2, bytes([0; 16])),
        (4, bytes(ORIGIN)),
        (5, bytes(SELECTED)),
        (6, V::U(OLD_EPOCH)),
        (7, bytes([0x44; 32])),
        (8, bytes([0; 32])),
    ] {
        assert!(!matches_ack(
            &other_ack(field, mismatch),
            &SELECTED,
            &ORIGIN
        ));
    }
}

#[test]
fn kits2_04_valid_max_hostname_exposes_existing_kit_size_limit() {
    let f = Fixture::new();
    let vectors = golden("KITS2-01");
    let raw_kit = raw(&vectors, "kit");
    let kit = decode(&raw_kit).unwrap();
    let host = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert_eq!(host.len(), 253);
    assert!(valid_host(&host));
    let place = decode(&raw(&vectors, "place_body")).unwrap();
    let body = encode(&with_field(&place, 6, Some(string(&host))));
    let (_, _, certificate) = signed_place(&body, &f.current_key, 44);
    let kit = with_field(&kit, 3, Some(string(&host)));
    let kit = encode(&with_field(&kit, 12, Some(bytes(certificate))));
    let limit = golden("KITS2-04");
    assert_eq!(limit["max_hostname"], host);
    assert_eq!(limit["certified_kit_length"], kit.len());
    assert!(kit.len() > om_core::recovery_record::MAX_KIT_BYTES);
}
