// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::kit::{ack_from_send, admit_bounded, place_fields, signed_place, transfer_digest};
use om_core::crypto::binding::RelationshipContext;
use om_core::crypto::pqxdh::{PqKind, encode_ec};
use om_core::crypto::store::{Device, Session};
use om_core::crypto::{delivery, hpke, rng::system_rng, session::SessionState, sha256};
use om_core::recovery_record::{MAX_KIT_BYTES, RecordKeys, RecoveryKit};

const CERTIFIED_CAP: usize = 8192;
const CEREMONY_CIPHERTEXT_CAP: usize = 4112;
const TRANSFER: [u8; 16] = [0x55; 16];

fn admit_certified(
    wire: &[u8],
    head: Option<&Acs>,
    anchor: &Acs,
    sibling: (&[u8; 16], &[u8; 16]),
) -> Option<RecoveryKit> {
    admit_bounded(wire, head, anchor, sibling, &SCOPE, CERTIFIED_CAP, |base| {
        // Key 23 already permits inert private extensions. Apply common.cddl
        // here because the legacy type-38 registry still caps the whole input
        // at 4,096. The transfer and vault keep the exact certified bytes.
        if let Some(extensions) = base.opt(23) {
            let V::TM(entries) = extensions else {
                return None;
            };
            if entries.is_empty()
                || entries.iter().any(|(id, entry)| {
                    id.is_empty()
                        || id.len() > 128
                        || id.starts_with("om.critical.")
                        || entry.m().ok().is_none_or(|fields| fields.len() != 2)
                        || entry.opt(0) != Some(&V::Bool(false))
                        || entry.opt(1).is_none()
                })
            {
                return None;
            }
        }
        RecoveryKit::from_bytes(&encode(&with_field(base, 23, None))).ok()
    })
}

pub(super) fn boundary_kit(original: &V, size: usize, critical: bool) -> Vec<u8> {
    let id = "3af5254d-9ecb-46b4-867d-4226aab2edaa";
    let id = if critical {
        format!("om.critical.private.{id}.fixture")
    } else {
        id.into()
    };
    let encode_extension = |count| {
        let value = map([(0, V::Bool(critical)), (1, bytes(vec![0; count]))]);
        encode(&with_field(
            original,
            23,
            Some(V::TM(vec![(id.clone(), value)])),
        ))
    };
    let overhead = encode_extension(256).len() - 256;
    encode_extension(size - overhead)
}

pub(super) fn repack(send: &V, kit: &[u8]) -> Vec<u8> {
    let send = with_field(send, 9, Some(bytes(kit)));
    encode(&with_field(&send, 8, Some(bytes(transfer_digest(&send)))))
}

fn max_host() -> String {
    format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    )
}

fn promotion(f: &Fixture, policy: &RecoveryPolicy) -> (Acs, Vec<u8>, Vec<u8>) {
    let mut parent = f.parent.clone();
    parent.control.remove(&SELECTED);
    parent.messaging.insert(SELECTED);
    parent.recovery_commitment = Some(policy.hash());
    let parent_body = parent.body();
    let signed_parent = sig::envelope(
        parent_body.clone(),
        vec![vote(
            &f.origin_key,
            ROLE_CONTROLLER,
            &ORIGIN,
            &parent_body,
            47,
        )],
    );
    assert_eq!(
        verify_genesis(&signed_parent, &f.credentials()).unwrap(),
        parent
    );
    let mut child = parent.clone();
    child.previous = Some(parent.hash());
    child.generation += 1;
    child.control.insert(SELECTED);
    let body = child.body();
    let signed_promotion = sig::envelope(
        body.clone(),
        vec![vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 45)],
    );
    (parent, signed_parent, signed_promotion)
}

fn check_kit_golden(
    vector: &Value,
    anchor: &Acs,
    head: &Acs,
    host: &str,
    epoch: u64,
    port: Option<u16>,
    snapshot_key: Option<[u8; 32]>,
    randomness: u8,
) {
    let root = RecoveryRoot::from_bytes(ROOT);
    let key = root.auth_signer(epoch);
    let policy = RecoveryPolicy::default_for(key.public(), epoch);
    let commitment = policy.hash();
    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    let kit = RecoveryKit {
        scope: SCOPE,
        host: host.into(),
        port,
        epoch,
        snapshot_key,
        read_cap: keys.read_cap,
        write_cap: keys.write_cap,
        record_key: keys.record_key,
        inbox_cap: [0x64; 32],
    };
    let (input, _, certificate) = signed_place(
        &encode(&place_fields(&kit, &ACCOUNT, &commitment)),
        &key,
        randomness,
    );
    let with_anchor = with_field(
        &decode(&kit.to_bytes()).unwrap(),
        11,
        Some(map([
            (0, bytes(ACCOUNT)),
            (1, V::U(anchor.generation)),
            (2, bytes(anchor.hash())),
        ])),
    );
    let kit_wire = encode(&with_field(&with_anchor, 12, Some(bytes(&certificate))));
    let transfer = map([
        (0, V::U(19)),
        (1, V::U(2)),
        (2, bytes(TRANSFER)),
        (3, bytes(ACCOUNT)),
        (4, bytes(ORIGIN)),
        (5, bytes(SELECTED)),
        (6, V::U(epoch)),
        (7, bytes(commitment)),
        (8, V::Null),
        (9, bytes(&kit_wire)),
        (10, bytes(policy.body())),
    ]);
    let digest = transfer_digest(&transfer);
    let transfer = with_field(&transfer, 8, Some(bytes(digest)));
    let send = encode(&transfer);
    let ack = encode(&ack_from_send(&transfer));
    let ack_binding = encode(&array([
        string("OM-RECOVERY-KIT-ACK-BIND-v1"),
        bytes(ACCOUNT),
        bytes(SELECTED),
        bytes(ORIGIN),
        V::U(epoch),
        bytes(digest),
        bytes(TRANSFER),
    ]));
    assert_eq!(vector["kit_length"], kit_wire.len());
    for (field, actual) in [
        ("kit", kit_wire),
        ("policy", policy.body()),
        ("place_input", input),
        ("certificate", certificate),
        ("kit_digest", digest.to_vec()),
        ("send", send.clone()),
        ("ack", ack),
        ("ack_binding", ack_binding),
    ] {
        assert!(actual == raw(vector, field), "{field} differs");
    }
    assert_eq!(
        admit_certified(&send, Some(head), anchor, (&ORIGIN, &SELECTED))
            .unwrap()
            .to_bytes(),
        kit.to_bytes()
    );
}

#[test]
fn kits3_01_max_hostname_with_and_without_optional_fields() {
    let f = Fixture::new();
    let host = max_host();
    let vector = golden("KITS3-01");
    assert_eq!(vector["max_hostname"], host);
    assert_eq!(host.len(), 253);
    assert_eq!(
        vector["legacy"]["kit_length"],
        v2("KITS2-04")["certified_kit_length"]
    );
    assert_eq!(vector["legacy"]["kit_length"], 4342);
    check_kit_golden(
        &vector["legacy"],
        &f.parent,
        &f.pinned_replacement(),
        &host,
        EPOCH,
        None,
        None,
        44,
    );
    let epoch = u64::MAX;
    let policy = RecoveryPolicy::default_for(
        RecoveryRoot::from_bytes(ROOT).auth_signer(epoch).public(),
        epoch,
    );
    let (parent, _, signed) = promotion(&f, &policy);
    let head = verify_successor(&parent, &signed, &f.credentials()).unwrap();
    check_kit_golden(
        &vector["max_port_epoch_without_snapshot"],
        &parent,
        &head,
        &host,
        epoch,
        Some(u16::MAX),
        None,
        46,
    );
    check_kit_golden(
        &vector["max_fields"],
        &parent,
        &head,
        &host,
        epoch,
        Some(u16::MAX),
        Some(std::array::from_fn(|i| i as u8)),
        46,
    );
}

#[derive(Default)]
struct DeliveryState {
    vault: Option<Vec<u8>>,
    debt: Option<Vec<u8>>,
}

impl DeliveryState {
    fn receive(
        &mut self,
        send: &[u8],
        head: Option<&Acs>,
        anchor: &Acs,
        sibling: (&[u8; 16], &[u8; 16]),
        commit_vault: bool,
    ) -> Option<Vec<u8>> {
        admit_certified(send, head, anchor, sibling)?;
        let parsed = decode(send).ok()?;
        let exact_kit = required_bytes(&parsed, 9)?;
        if self.vault.as_deref() != Some(exact_kit) {
            if !commit_vault {
                return None;
            }
            self.vault = Some(exact_kit.to_vec());
        }
        Some(encode(&ack_from_send(&parsed)))
    }

    fn acknowledge(&mut self, ack: &[u8], sibling: (&[u8; 16], &[u8; 16])) {
        if sibling == (&SELECTED, &ORIGIN) && self.debt.as_deref() == Some(ack) {
            self.debt = None;
        }
    }

    fn reconcile(&mut self, independently_verified_head: Option<&Acs>) {
        if independently_verified_head.is_some_and(|head| !head.control.contains(&SELECTED)) {
            self.debt = None;
        }
    }
}

#[test]
fn kits3_02_null_promotion_delayed_transfer_and_durable_receipt_model() {
    let f = Fixture::new();
    let vector = golden("KITS3-02");
    let wire = golden("KITS3-01")["max_fields"].clone();
    let policy = RecoveryPolicy::from_body(&raw(&wire, "policy")).unwrap();
    let (parent, signed_parent, signed) = promotion(&f, &policy);
    let promotion_reply = encode(&array([V::U(2), bytes(&signed), V::Null]));
    assert_eq!(signed_parent, raw(&vector, "parent"));
    assert_eq!(signed, raw(&vector, "promoted"));
    assert_eq!(promotion_reply, raw(&vector, "promotion_reply_body"));
    let head = verify_successor(&parent, &signed, &f.credentials()).unwrap();
    let send = raw(&wire, "send");
    let sibling = (&ORIGIN, &SELECTED);
    let expected = raw(&wire, "ack");
    let mut state = DeliveryState {
        debt: Some(expected.clone()),
        ..Default::default()
    };
    state.acknowledge(&promotion_reply, (&SELECTED, &ORIGIN));
    assert!(state.debt.is_some());
    assert!(state.receive(&send, None, &parent, sibling, true).is_none());
    assert!(
        state
            .receive(&send, Some(&head), &parent, sibling, false)
            .is_none()
    );
    assert!(state.vault.is_none());

    let ack = state
        .receive(&send, Some(&head), &parent, sibling, true)
        .unwrap();
    assert_eq!(ack, expected);
    assert_eq!(state.vault.as_deref(), Some(raw(&wire, "kit").as_slice()));
    // Simulate restart from the committed vault and sender debt after ACK loss.
    let mut state = DeliveryState {
        vault: state.vault.take(),
        debt: state.debt.take(),
    };
    assert_eq!(
        state.receive(&send, Some(&head), &parent, sibling, false),
        Some(expected.clone())
    );
    state.acknowledge(&expected, sibling);
    let ack = decode(&expected).unwrap();
    state.acknowledge(
        &encode(&with_field(&ack, 8, Some(bytes([0; 32])))),
        (&SELECTED, &ORIGIN),
    );
    assert!(state.debt.is_some());
    state.acknowledge(&expected, (&SELECTED, &ORIGIN));
    assert!(state.debt.is_none());

    let mut excluded = head.clone();
    excluded.generation += 1;
    excluded.previous = Some(head.hash());
    excluded.control.remove(&SELECTED);
    excluded.messaging.remove(&SELECTED);
    excluded.revoked.insert(SELECTED);
    excluded.credentials.retain(|(d, _)| d != &SELECTED);
    let body = excluded.body();
    let revoked = sig::envelope(
        body.clone(),
        vec![
            vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 48),
            vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &body, 49),
        ],
    );
    let excluded = verify_successor(&head, &revoked, &f.credentials()).unwrap();
    state.debt = Some(expected);
    state.reconcile(None);
    assert!(state.debt.is_some());
    state.reconcile(Some(&excluded));
    assert!(state.debt.is_none());
    let mut empty = DeliveryState::default();
    assert!(
        empty
            .receive(&send, Some(&excluded), &parent, sibling, true)
            .is_none()
    );
    assert!(empty.vault.is_none());
}

#[test]
fn kits3_03_byte_boundary_is_separate_from_semantic_admission() {
    let f = Fixture::new();
    let head = f.pinned_replacement();
    let vector = golden("KITS3-03");
    let send_wire = raw(&golden("KITS3-01")["legacy"], "send");
    let send = decode(&send_wire).unwrap();
    let original = decode(required_bytes(&send, 9).unwrap()).unwrap();
    let mut state = DeliveryState::default();
    let sibling = (&ORIGIN, &SELECTED);
    let mut receive = |wire: &[u8]| {
        assert!(
            state
                .receive(wire, Some(&head), &f.parent, sibling, true)
                .is_none()
        );
        assert!(state.vault.is_none());
    };

    let old = f.old_policy.hash();
    let mut active_code = old;
    let mut confirm = |candidate: &[u8]| {
        let send = decode(candidate).unwrap();
        let kit = required_bytes(&send, 9).unwrap();
        if kit.len() > CERTIFIED_CAP {
            return Err("oversize");
        }
        admit_certified(candidate, Some(&head), &f.parent, sibling)
            .ok_or("invalid_certificate_or_policy")?;
        active_code = head.recovery_commitment.unwrap();
        Ok(active_code)
    };
    let cap = boundary_kit(&original, CERTIFIED_CAP, false);
    let oversize = boundary_kit(&original, CERTIFIED_CAP + 1, false);
    let critical = boundary_kit(&original, CERTIFIED_CAP, true);
    for (kit, size, field) in [
        (&cap, CERTIFIED_CAP, "at_cap_sha256"),
        (&oversize, CERTIFIED_CAP + 1, "over_cap_sha256"),
        (&critical, CERTIFIED_CAP, "unknown_critical_at_cap_sha256"),
    ] {
        assert_eq!(kit.len(), size);
        assert_eq!(sha256(&[kit]).as_slice(), raw(&vector, field));
    }
    for (kit, expected) in [
        (&oversize, "oversize"),
        (&critical, "invalid_certificate_or_policy"),
    ] {
        let candidate = repack(&send, kit);
        assert_eq!(confirm(&candidate), Err(expected));
        receive(&candidate);
    }
    assert_eq!(vector["cap"], CERTIFIED_CAP);
    assert_eq!(vector["oversize"], CERTIFIED_CAP + 1);
    assert!(!om_core::recovery_sheet::valid_host(
        vector["invalid_host"].as_str().unwrap()
    ));
    for (field, value) in [
        (3, string(vector["invalid_host"].as_str().unwrap())),
        (5, bytes([0; 32])),
        (10, V::U(EPOCH + 1)),
    ] {
        receive(&repack(
            &send,
            &encode(&with_field(&original, field, Some(value))),
        ));
    }
    let mut cert = required_bytes(&original, 12).unwrap().to_vec();
    *cert.last_mut().unwrap() ^= 1;
    receive(&repack(
        &send,
        &encode(&with_field(&original, 12, Some(bytes(cert)))),
    ));
    receive(&encode(&with_field(
        &send,
        10,
        Some(bytes(f.old_policy.body())),
    )));
    receive(&encode(&with_field(&send, 6, Some(V::U(OLD_EPOCH)))));
    let legacy: Value = serde_json::from_str(crate::tests::GOLDEN).unwrap();
    receive(&raw(&legacy["KITS-01"], "send"));
    assert_eq!(active_code, old);
    let at_limit = repack(&send, &cap);
    let mut confirm_state = DeliveryState::default();
    let ack = confirm_state
        .receive(&at_limit, Some(&head), &f.parent, sibling, true)
        .unwrap();
    assert_eq!(confirm_state.vault, Some(cap));
    assert_eq!(ack, encode(&ack_from_send(&decode(&at_limit).unwrap())));
}

#[test]
fn kits3_04_promotion_limit_and_session_replay() {
    let vector = golden("KITS3-04");
    let configured = MAX_KIT_BYTES + 16;
    assert_eq!(configured, CEREMONY_CIPHERTEXT_CAP);
    assert_eq!(vector["promotion_ciphertext_cap"], configured);
    let ceremony_fits = |ciphertext| (17..=CEREMONY_CIPHERTEXT_CAP).contains(&ciphertext);
    assert!(ceremony_fits(4112));
    assert!(!ceremony_fits(4113));
    let kit_wire = golden("KITS3-01");
    assert!(!ceremony_fits(raw(&kit_wire["legacy"], "kit").len() + 16));

    let wire = &kit_wire["max_fields"];
    let send = raw(wire, "send");
    let f = Fixture::new();
    let policy = RecoveryPolicy::from_body(&raw(wire, "policy")).unwrap();
    let (parent, _, signed) = promotion(&f, &policy);
    let head = verify_successor(&parent, &signed, &f.credentials()).unwrap();
    let mut rng = system_rng();
    // Start with an already established same-account ratchet. Use the
    // existing 1:1 envelope codec in both directions of this loopback.
    let mut origin = Device::new(&mut rng);
    origin.account_id = ACCOUNT;
    origin.device_id = ORIGIN;
    origin.ik_secret = [0x91; 32];
    let mut selected = Device::new(&mut rng);
    selected.account_id = ACCOUNT;
    selected.device_id = SELECTED;
    selected.ik_secret = [0x92; 32];
    let shared = [0x45; 32];
    let ad = [encode_ec(&origin.ik()), encode_ec(&selected.ik())].concat();
    let alice_dr = SessionState::init_alice(
        &shared,
        ad.clone(),
        &x25519_pub(&selected.spk.secret),
        &mut rng,
    );
    let bob_dr = SessionState::init_bob(&shared, ad, &selected.spk.secret);
    for (device, peer, dr, initiator) in [
        (&mut origin, f.selected.ik, alice_dr, true),
        (&mut selected, f.origin.ik, bob_dr, false),
    ] {
        device.trust_peer(head.body(), f.credentials()).unwrap();
        device.sessions.insert(
            encode_ec(&peer).to_vec(),
            Session {
                dr,
                initiator,
                pending_init: None,
                init_ek: [0; 32],
                pq_kind: PqKind::LastResort,
            },
        );
    }
    let (inner, _) = origin.encrypt(&selected.ik(), &send).unwrap();
    let bucket = delivery::BUCKETS
        .into_iter()
        .find(|size| *size >= inner.len() + 4)
        .unwrap();
    assert_eq!(vector["sealed_delivery_bucket"], bucket);
    let seed = [0x48; 32];
    let recipient = hpke::public_key(&seed);
    let wrapper = delivery::seal_deterministic(
        &recipient,
        &[1; 16],
        &[2; 16],
        "https://home.example",
        &[3; 32],
        1800000000,
        &inner,
        &[4; 64],
        &vec![0; bucket - 4 - inner.len()],
    )
    .unwrap();
    assert_eq!(
        delivery::open(
            &wrapper,
            &seed,
            &[1; 16],
            "https://home.example",
            &[3; 32],
            1799999700
        )
        .unwrap()
        .inner_envelope,
        inner
    );
    let ctx = RelationshipContext::new(ACCOUNT);
    assert!(
        selected
            .decrypt(&RelationshipContext::new([0; 32]), &inner, &mut rng)
            .is_err()
    );
    let (sender, opened, _) = selected.decrypt(&ctx, &inner, &mut rng).unwrap();
    assert_eq!(sender, f.origin.ik);
    assert!(selected.decrypt(&ctx, &inner, &mut rng).is_err());
    assert_eq!(opened, send);
    let mut state = DeliveryState::default();
    assert!(
        state
            .receive(&opened, Some(&head), &parent, (&SELECTED, &ORIGIN), true)
            .is_none()
    );
    let field = decode(&opened).unwrap();
    let wrong_digest = encode(&with_field(&field, 8, Some(bytes([0; 32]))));
    assert!(
        state
            .receive(
                &wrong_digest,
                Some(&head),
                &parent,
                (&ORIGIN, &SELECTED),
                true
            )
            .is_none()
    );
    assert!(state.vault.is_none());
    let ack = state
        .receive(&opened, Some(&head), &parent, (&ORIGIN, &SELECTED), true)
        .unwrap();
    assert_eq!(ack, raw(wire, "ack"));
    let (return_envelope, _) = selected.encrypt(&origin.ik(), &ack).unwrap();
    let (recipient, returned, _) = origin.decrypt(&ctx, &return_envelope, &mut rng).unwrap();
    assert_eq!(recipient, f.selected.ik);
    assert_eq!(returned, ack);

    let kit = decode(required_bytes(&field, 9).unwrap()).unwrap();
    let at_cap = boundary_kit(&kit, CERTIFIED_CAP, false);
    let at_cap = repack(&field, &at_cap);
    let (full_envelope, _) = origin.encrypt(&selected.ik(), &at_cap).unwrap();
    assert_eq!(
        delivery::BUCKETS
            .into_iter()
            .find(|b| *b >= full_envelope.len() + 4),
        Some(bucket)
    );
    assert_eq!(
        selected.decrypt(&ctx, &full_envelope, &mut rng).unwrap().1,
        at_cap
    );
}
