// SPDX-License-Identifier: Apache-2.0
//! ADR-0232 plaintext evidence. Admission and vault transactions stay outside this fixture.

use crate::tests::{GOLDEN, bytes, string};
use om_core::crypto::cbor::{V, decode, encode};
use om_core::crypto::sig::ds_hash;
use om_core::crypto::{hex, unhex};
use om_core::recovery_record::RecoveryKit;
use serde_json::{Value, json};

const SEND: u64 = 19;
const ACK: u64 = 20;
const TRANSFER_HASH: &str = "OM-RECOVERY-KIT-TRANSFER-HASH-v1";
const ACK_BIND: &str = "OM-RECOVERY-KIT-ACK-BIND-v1";

fn kit_digest(account: &V, epoch: &V, policy: &V, kit: &V) -> [u8; 32] {
    let context = V::A(vec![
        account.clone(),
        epoch.clone(),
        policy.clone(),
        kit.clone(),
    ]);
    ds_hash(TRANSFER_HASH, &encode(&context))
}

fn kit_with_anchor() -> Vec<u8> {
    let base = RecoveryKit {
        scope: [0x60; 32],
        host: "home.example".into(),
        port: None,
        read_cap: [0x61; 32],
        write_cap: [0x62; 32],
        record_key: [0x63; 32],
        inbox_cap: [0x64; 32],
        snapshot_key: Some([0x65; 32]),
        epoch: 7,
    };
    let V::M(mut fields) = decode(&base.to_bytes()).unwrap() else {
        unreachable!("RecoveryKit is a map")
    };
    fields.push((
        11,
        V::M(vec![
            (0, bytes([0x11; 32])),
            (1, V::U(4)),
            (2, bytes([0x66; 32])),
        ]),
    ));
    encode(&V::M(fields))
}

fn read_kit(kit: &[u8], account: &[u8], epoch: u64) -> Option<RecoveryKit> {
    let V::M(mut fields) = decode(kit).ok()? else {
        return None;
    };
    if let Some(index) = fields.iter().position(|(key, _)| *key == 11) {
        let (_, anchor) = fields.remove(index);
        if anchor.m().ok()?.len() != 3 || anchor.get(0).ok()?.b().ok()? != account {
            return None;
        }
        anchor.get(1).ok()?.u().ok()?;
        anchor.get(2).ok()?.b32().ok()?;
    }
    let base = RecoveryKit::from_bytes(&encode(&V::M(fields))).ok()?;
    (base.epoch == epoch).then_some(base)
}

fn read_message(message: &[u8]) -> Option<V> {
    let payload = decode(message).ok()?;
    let fields = payload.m().ok()?;
    let kind = payload.get(0).ok()?.u().ok()?;
    let expected = match kind {
        SEND => 10,
        ACK => 9,
        _ => return None,
    };
    if fields.len() != expected || payload.get(1).ok()?.u().ok()? != 1 {
        return None;
    }
    for key in [2, 4, 5] {
        <[u8; 16]>::try_from(payload.get(key).ok()?.b().ok()?).ok()?;
    }
    let account = payload.get(3).ok()?;
    let epoch = payload.get(6).ok()?;
    let policy = payload.get(7).ok()?;
    account.b32().ok()?;
    let epoch_value = epoch.u().ok()?;
    policy.b32().ok()?;
    let digest = payload.get(8).ok()?.b32().ok()?;
    if kind == SEND {
        let kit = payload.get(9).ok()?;
        if kit_digest(account, epoch, policy, kit) != digest {
            return None;
        }
        read_kit(kit.b().ok()?, account.b().ok()?, epoch_value)?;
    }
    Some(payload)
}

#[test]
fn kits_01_transfer_golden() {
    let transfer = bytes([0x55; 16]);
    let account = bytes([0x11; 32]);
    let source = bytes([0x22; 16]);
    let target = bytes([0x33; 16]);
    let epoch = V::U(7);
    let policy = bytes([0x44; 32]);
    let kit = kit_with_anchor();
    let digest = kit_digest(&account, &epoch, &policy, &bytes(&kit));
    let shared = |kind, sender, recipient| {
        vec![
            (0, V::U(kind)),
            (1, V::U(1)),
            (2, transfer.clone()),
            (3, account.clone()),
            (4, sender),
            (5, recipient),
            (6, epoch.clone()),
            (7, policy.clone()),
            (8, bytes(digest)),
        ]
    };
    let mut fields = shared(SEND, source.clone(), target.clone());
    fields.push((9, bytes(&kit)));
    let send = encode(&V::M(fields));
    let ack = encode(&V::M(shared(ACK, target.clone(), source.clone())));
    let binding = encode(&V::A(vec![
        string(ACK_BIND),
        account,
        target,
        source,
        epoch,
        bytes(digest),
        transfer,
    ]));
    let actual = json!({
        "kit": hex(&kit), "kit_digest": hex(&digest), "send": hex(&send),
        "ack": hex(&ack), "ack_binding": hex(&binding)
    });
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(actual, golden["KITS-01"]);
    for message in [send, ack] {
        assert_eq!(encode(&read_message(&message).unwrap()), message);
    }
}

#[test]
fn kits_02_plaintext_rejections() {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    let mut send = unhex(golden["KITS-01"]["send"].as_str().unwrap()).unwrap();
    send.extend_from_slice(&[0]);
    assert!(read_message(&send).is_none(), "trailing byte");
    send.pop();
    let malformed = [&[0xb8, 10][..], &send[1..]].concat();
    assert!(read_message(&malformed).is_none(), "non-canonical map");
    let V::M(mut fields) = read_message(&send).unwrap() else {
        unreachable!("transfer is a map")
    };
    fields.push((10, V::Null));
    assert!(
        read_message(&encode(&V::M(fields.clone()))).is_none(),
        "unknown key"
    );
    fields.pop();
    fields[7].1 = bytes([0x45; 32]);
    assert!(
        read_message(&encode(&V::M(fields))).is_none(),
        "digest commits to policy"
    );
}
