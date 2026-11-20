// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::v2::kit::{ack_from_send, admit_bounded, place_fields, signed_place, transfer_digest};
use crate::v2::v3::kit::{boundary_kit, repack};
use om_core::crypto::binding::RelationshipContext;
use om_core::crypto::pqxdh::{PqKind, encode_ec};
use om_core::crypto::store::{Device, Session};
use om_core::crypto::{delivery, hpke, rng::system_rng, session::SessionState};
use om_core::recovery_record::{RecordKeys, RecoveryKit};
use om_core::recovery_sheet::valid_host;

const CERTIFIED: u64 = 47; // ADR-0246 provisional type, local to the fixture.
const CERTIFIED_CAP: usize = 8192;

fn kit_base(value: &V) -> Option<RecoveryKit> {
    if value.opt(0) != Some(&V::U(CERTIFIED)) || value.opt(1) != Some(&V::U(1)) {
        return None;
    }
    if let Some(ext) = value.opt(23) {
        let V::TM(entries) = ext else {
            return None;
        };
        if entries.is_empty()
            || entries.iter().any(|(name, entry)| {
                name.is_empty()
                    || name.len() > 128
                    || name.starts_with("om.critical.")
                    || entry.m().ok().is_none_or(|fields| fields.len() != 2)
                    || entry.opt(0) != Some(&V::Bool(false))
                    || entry.opt(1).is_none()
            })
        {
            return None;
        }
    }
    // Validate the unchanged common fields via the legacy codec after its
    // typed/size dispatch and the certified-carrier fields have been checked.
    let base = with_field(&with_field(value, 23, None), 0, Some(V::U(38)));
    RecoveryKit::from_bytes(&encode(&base)).ok()
}

fn certified_wire(raw: &[u8]) -> Option<V> {
    if raw.len() > CERTIFIED_CAP {
        return None;
    }
    let value = decode(raw).ok()?;
    value.m().ok()?;
    kit_base(&with_field(&with_field(&value, 11, None), 12, None))?;
    required_bytes(&value, 12)?;
    Some(value)
}

fn admit(
    send: &[u8],
    head: Option<&Acs>,
    anchor: &Acs,
    party: (&[u8; 16], &[u8; 16]),
) -> Option<RecoveryKit> {
    let outer = decode(send).ok()?;
    certified_wire(required_bytes(&outer, 9)?)?;
    admit_bounded(send, head, anchor, party, &SCOPE, CERTIFIED_CAP, kit_base)
}

struct Wire {
    kit: Vec<u8>,
    legacy: Vec<u8>,
    send: Vec<u8>,
    ack: Vec<u8>,
}

fn fixture(f: &Fixture) -> Wire {
    let keys = RecordKeys::derive(&ROOT, &SCOPE);
    let hostname = [
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61),
    ]
    .join(".");
    let base = RecoveryKit {
        scope: SCOPE,
        host: hostname,
        port: None,
        read_cap: keys.read_cap,
        write_cap: keys.write_cap,
        record_key: keys.record_key,
        inbox_cap: [0x64; 32],
        snapshot_key: None,
        epoch: EPOCH,
    };
    let commitment = f.current_policy.hash();
    let body = encode(&place_fields(&base, &ACCOUNT, &commitment));
    let (_, _, cert) = signed_place(&body, &f.current_key, 44);
    let fields = decode(&base.to_bytes()).unwrap();
    let anchor = map([
        (0, bytes(ACCOUNT)),
        (1, V::U(1)),
        (2, bytes(f.parent.hash())),
    ]);
    let legacy = with_field(
        &with_field(&fields, 11, Some(anchor)),
        12,
        Some(bytes(cert)),
    );
    let kit = encode(&with_field(&legacy, 0, Some(V::U(CERTIFIED))));
    let fields = map([
        (0, V::U(19)),
        (1, V::U(2)),
        (2, bytes([0x55; 16])),
        (3, bytes(ACCOUNT)),
        (4, bytes(ORIGIN)),
        (5, bytes(SELECTED)),
        (6, V::U(EPOCH)),
        (7, bytes(commitment)),
        (9, bytes(&kit)),
        (10, bytes(f.current_policy.body())),
    ]);
    let send = repack(&fields, &kit);
    let ack = encode(&ack_from_send(&decode(&send).unwrap()));
    Wire {
        kit,
        legacy: encode(&legacy),
        send,
        ack,
    }
}

fn check_transfer(v: &Value, kit: &[u8], send: &[u8]) {
    let fields = decode(send).unwrap();
    assert_eq!(kit, raw(v, "kit"));
    assert_eq!(send, raw(v, "send"));
    let digest = transfer_digest(&fields);
    assert_eq!(digest, raw(v, "kit_digest").as_slice());
    assert_eq!(encode(&ack_from_send(&fields)), raw(v, "ack"));
    assert_eq!(
        encode(&array([
            string("OM-RECOVERY-KIT-ACK-BIND-v1"),
            bytes(ACCOUNT),
            bytes(SELECTED),
            bytes(ORIGIN),
            V::U(EPOCH),
            bytes(digest),
            bytes([0x55; 16]),
        ])),
        raw(v, "ack_binding")
    );
}

#[test]
fn kits4_01_frozen_registry_refuses_exact_historical_type38_and_type47_fits() {
    let v = vector("KITS4-01");
    let f = Fixture::new();
    let wire = fixture(&f);
    let head = f.pinned_replacement();
    assert_eq!(wire.legacy, raw(&v, "type38"));
    assert_eq!(wire.legacy, raw(&golden("KITS3-01")["legacy"], "kit"));
    assert_eq!(v["size"], wire.legacy.len());
    assert_eq!(wire.legacy.len(), 4342);
    assert_eq!(wire.legacy.len(), wire.kit.len());
    let reg = om_codec::Registry::load();
    assert_eq!(
        reg.check("RecoveryKit", &wire.legacy, false).unwrap_err(),
        "size_limit"
    );
    assert!(certified_wire(&wire.legacy).is_none());
    assert!(
        admit(
            &repack(&decode(&wire.send).unwrap(), &wire.legacy),
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED)
        )
        .is_none()
    );
    assert_eq!(v["type38_expected"], "refused_before_decode");
    assert_eq!(v["type47_expected"], "admitted");
    check_transfer(&v["type47"], &wire.kit, &wire.send);
    let stored = admit(&wire.send, Some(&head), &f.parent, (&ORIGIN, &SELECTED)).unwrap();
    assert_eq!(stored.host.len(), 253);
    assert_eq!(v["hostname"], stored.host);
    assert!(valid_host(&stored.host));
    let old_send = decode(&raw(&golden("KITS3-01")["legacy"], "send")).unwrap();
    assert_ne!(
        transfer_digest(&old_send),
        transfer_digest(&decode(&wire.send).unwrap())
    );
}

#[test]
fn kits4_02_boundary_and_issuer_failure() {
    let v = vector("KITS4-02");
    let f = Fixture::new();
    let wire = fixture(&f);
    let head = f.pinned_replacement();
    let kit = decode(&wire.kit).unwrap();
    let send = decode(&wire.send).unwrap();
    let at_cap = boundary_kit(&kit, CERTIFIED_CAP, false);
    let over = boundary_kit(&kit, CERTIFIED_CAP + 1, false);
    let oversized_send = repack(&send, &over);
    let at_cap_send = repack(&send, &at_cap);
    check_transfer(&v["at_cap"], &at_cap, &at_cap_send);
    check_bytes(&v["over_cap"], &over, "refused_before_decode");
    assert_eq!(at_cap.len(), CERTIFIED_CAP);
    assert!(certified_wire(&at_cap).is_some());
    assert!(admit(&at_cap_send, Some(&head), &f.parent, (&ORIGIN, &SELECTED)).is_some());
    assert!(certified_wire(&over).is_none());
    assert!(
        admit(
            &oversized_send,
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED)
        )
        .is_none()
    );

    let critical = boundary_kit(&kit, CERTIFIED_CAP, true);
    check_bytes(&v["unknown_critical"], &critical, "refused");
    let invalid_host = with_field(
        &kit,
        3,
        Some(string(v["invalid_hostname"].as_str().unwrap())),
    );
    assert_eq!(invalid_host.get(3).unwrap().t().unwrap().len(), 254);
    let cert = required_bytes(&kit, 12).unwrap();
    let mut bad_cert = cert.to_vec();
    *bad_cert.last_mut().unwrap() ^= 1;
    let invalid_signature = with_field(&kit, 12, Some(bytes(&bad_cert)));
    let invalid_version = with_field(&kit, 1, Some(V::U(2)));
    for (name, specimen) in [
        ("hostname", &invalid_host),
        ("signature", &invalid_signature),
        ("version", &invalid_version),
    ] {
        check_bytes(&v["invalid"][name], &encode(specimen), "refused");
    }
    let mut retained_policy = f.old_policy.hash();
    for invalid in [
        over,
        critical,
        encode(&invalid_host),
        encode(&invalid_signature),
        encode(&invalid_version),
    ] {
        // Issuance models the same size and independent certificate checks
        // before the new code or ACS is made active.
        if admit(
            &repack(&send, &invalid),
            Some(&head),
            &f.parent,
            (&ORIGIN, &SELECTED),
        )
        .is_some()
        {
            retained_policy = f.current_policy.hash();
        }
        assert_eq!(retained_policy, f.old_policy.hash());
    }
}

#[derive(Default)]
struct Delivery {
    vault: Option<Vec<u8>>,
    pending_ack: Option<Vec<u8>>,
}

impl Delivery {
    fn receive(&mut self, send: &[u8], f: &Fixture, head: &Acs, commit: bool) -> Option<Vec<u8>> {
        admit(send, Some(head), &f.parent, (&ORIGIN, &SELECTED))?;
        let value = decode(send).ok()?;
        let kit = required_bytes(&value, 9)?;
        if self.vault.as_deref() != Some(kit) {
            if !commit {
                return None;
            }
            self.vault = Some(kit.to_vec());
        }
        Some(encode(&ack_from_send(&value)))
    }

    fn acknowledge(&mut self, authenticated_sender: &[u8; 16], ack: &[u8]) {
        if authenticated_sender == &SELECTED && self.pending_ack.as_deref() == Some(ack) {
            self.pending_ack = None;
        }
    }
}

#[test]
fn kits4_03_promotion_uses_null_then_vault_commit_and_retry_ack() {
    let v = vector("KITS4-03");
    let f = Fixture::new();
    let wire = fixture(&f);
    let head = f.pinned_replacement();
    assert_eq!(v["promotion_ciphertext_cap"], 4112);
    assert_eq!(v["sealed_kit_minimum"], wire.kit.len() + 16);
    assert!(wire.kit.len() + 16 > 4112);
    let promotion = decode(&raw(&golden("KITS3-02"), "promotion_reply_body")).unwrap();
    assert_eq!(promotion.a().unwrap()[2], V::Null);
    assert!(v["promotion_kit_slot"].is_null());
    let mut issue = Delivery {
        pending_ack: Some(wire.ack.clone()),
        ..Delivery::default()
    };
    let mut receive = Delivery::default();
    assert!(receive.receive(&wire.send, &f, &head, false).is_none());
    assert!(receive.vault.is_none());
    assert_eq!(
        receive.receive(&wire.send, &f, &head, true),
        Some(wire.ack.clone())
    );
    assert_eq!(v["vault_before_ack"], true);
    let mut restarted = Delivery {
        vault: receive.vault,
        pending_ack: issue.pending_ack.take(),
    };
    assert_eq!(restarted.vault.as_deref(), Some(wire.kit.as_slice()));
    assert_eq!(
        restarted.receive(&wire.send, &f, &head, false),
        Some(wire.ack.clone())
    );
    restarted.acknowledge(&ORIGIN, &wire.ack);
    restarted.acknowledge(&SELECTED, b"wrong ACK");
    assert!(restarted.pending_ack.is_some());
    restarted.acknowledge(&SELECTED, &wire.ack);
    assert!(restarted.pending_ack.is_none());

    let mut excluded = head.clone();
    excluded.generation += 1;
    excluded.previous = Some(head.hash());
    excluded.control.remove(&SELECTED);
    excluded.credentials.retain(|(id, _)| id != &SELECTED);
    excluded.revoked.insert(SELECTED);
    let body = excluded.body();
    let signed = sig::envelope(
        body.clone(),
        vec![
            vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 78),
            vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &body, 79),
        ],
    );
    let excluded = verify_successor(&head, &signed, &f.credentials()).unwrap();
    let mut pending = Delivery {
        pending_ack: Some(wire.ack.clone()),
        ..Delivery::default()
    };
    assert!(pending.receive(&wire.send, &f, &excluded, true).is_none());
    assert!(pending.vault.is_none());
    if !excluded.control.contains(&SELECTED) {
        pending.pending_ack = None;
    }
    assert!(pending.pending_ack.is_none());
}

#[test]
fn kits4_04_typed_admission_and_sealed_delivery_measurement() {
    let v = vector("KITS4-04");
    let f = Fixture::new();
    let wire = fixture(&f);
    let head = f.pinned_replacement();
    assert_eq!(v["legacy_cap"], om_core::recovery_record::MAX_KIT_BYTES);
    assert_eq!(v["certified_cap"], CERTIFIED_CAP);
    assert!(
        om_codec::Registry::load()
            .check("CertifiedRecoveryKit", &wire.kit, false)
            .is_err()
    );
    assert!(RecoveryKit::from_bytes(&wire.kit).is_err());
    assert!(certified_wire(&wire.kit).is_some());
    let cap_kit = boundary_kit(&decode(&wire.kit).unwrap(), CERTIFIED_CAP, false);
    let cap_send = repack(&decode(&wire.send).unwrap(), &cap_kit);
    let party = (&ORIGIN, &SELECTED);
    assert!(admit(&cap_send, None, &f.parent, party).is_none());
    assert!(admit(&cap_send, Some(&head), &f.parent, (&SELECTED, &ORIGIN)).is_none());

    let mut rng = system_rng();
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
    let alice = SessionState::init_alice(
        &shared,
        ad.clone(),
        &x25519_pub(&selected.spk.secret),
        &mut rng,
    );
    let bob = SessionState::init_bob(&shared, ad, &selected.spk.secret);
    for (device, peer, dr, initiator) in [
        (&mut origin, f.selected.ik, alice, true),
        (&mut selected, f.origin.ik, bob, false),
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
    for (message, stored_len) in [(&wire.send, wire.kit.len()), (&cap_send, cap_kit.len())] {
        let (inner, _) = origin.encrypt(&selected.ik(), message).unwrap();
        let bucket = delivery::BUCKETS
            .into_iter()
            .find(|size| *size >= inner.len() + 4)
            .unwrap();
        assert_eq!(v["sealed_delivery_bucket"], bucket);
        let seed = [0x48; 32];
        let wrapper = delivery::seal_deterministic(
            &hpke::public_key(&seed),
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
        assert!(
            !inner
                .windows(stored_len)
                .any(|part| part == required_bytes(&decode(message).unwrap(), 9).unwrap())
        );
        assert!(!wrapper.windows(message.len()).any(|part| part == message));
        let opened = delivery::open(
            &wrapper,
            &seed,
            &[1; 16],
            "https://home.example",
            &[3; 32],
            1799999700,
        )
        .unwrap()
        .inner_envelope;
        assert_eq!(opened, inner);
        assert!(
            selected
                .decrypt(&RelationshipContext::new([0; 32]), &opened, &mut rng)
                .is_err()
        );
        let (sender, decrypted, _) = selected
            .decrypt(&RelationshipContext::new(ACCOUNT), &opened, &mut rng)
            .unwrap();
        assert_eq!(sender, f.origin.ik);
        assert_eq!(decrypted, *message);
        let mut state = Delivery::default();
        assert!(state.receive(&decrypted, &f, &head, true).is_some());
        assert_eq!(state.vault.unwrap().len(), stored_len);
    }
}
