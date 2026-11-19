// SPDX-License-Identifier: Apache-2.0
//! ADR-0230 second-code wire evidence for fixed synthetic keys.

use crate::tests::GOLDEN;
use om_core::control::LBL_ACS_SIG;
use om_core::crypto::binding::{Acs, Credential};
use om_core::crypto::cbor::{V, decode, encode};
use om_core::crypto::sig::{self, DeviceSigner, ROLE_CONTROLLER, ds_hash, ds_sig};
use om_core::crypto::{PROFILE_V1, hex, sha256};
use om_core::recovery::{RecoveryPolicy, RecoveryRoot};
use om_core::recovery_record::{
    self as record, AcsEvidence, AcsHeadRef, ProviderHome, RecoveryRecord, RelationshipSnapshot,
    Relationships,
};
use om_core::recovery_sheet::{LBL_SHEET_CHECK, OwnerCheckpoint, Sheet, line_check, parse};
use serde_json::{Value, json};

pub(super) const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const KEY: [u8; 32] = sequence(0);
const ROOT: [u8; 32] = sequence(0x40);
const SCOPE: [u8; 32] = sequence(0xa0);
const GENERATION: u64 = 0x1234;
const CHECKPOINT: OwnerCheckpoint = OwnerCheckpoint {
    account_id: [0x11; 32],
    generation: 7,
    acs_hash: [0x22; 32],
};

const fn sequence<const N: usize>(start: u8) -> [u8; N] {
    let mut result = [0; N];
    let mut i = 0;
    while i < N {
        result[i] = start + i as u8;
        i += 1;
    }
    result
}

fn check(body: &[u8]) -> [u8; 4] {
    ds_hash(LBL_SHEET_CHECK, body)[..4].try_into().unwrap()
}

fn relationship_sheet(root: &Sheet, key: &[u8; 32]) -> Vec<u8> {
    assert!(root.is_full() && root.checkpoint.is_some());
    let mut binary = root.to_bytes();
    binary[1] = 3;
    binary[2..34].copy_from_slice(key);
    let end = binary.len() - 4;
    let checksum = check(&binary[..end]);
    binary[end..].copy_from_slice(&checksum);
    binary
}

// A matched root sheet supplies the validated canonical scope, port, host and checkpoint.
// Compare the entire public suffix exactly, including the checkpoint's original bytes.
fn relationship_key<'a>(binary: &'a [u8], root: &Sheet) -> Option<&'a [u8; 32]> {
    if binary.get(..2)? != [2, 3] || !root.is_full() || root.checkpoint.is_none() {
        return None;
    }
    let expected = root.to_bytes();
    if binary.len() != expected.len() {
        return None;
    }
    let end = binary.len() - 4;
    let bad_check = binary[end..]
        .iter()
        .zip(check(&binary[..end]))
        .fold(0, |diff, (actual, expected)| diff | (actual ^ expected));
    if binary[34..end] != expected[34..end] || bad_check != 0 {
        return None;
    }
    binary[2..34].try_into().ok()
}

pub(super) fn encode_radix(bytes: &[u8], alphabet: &[u8], width: usize) -> String {
    let mut bits = 0;
    let mut buffer = 0u16;
    let mut text = String::new();
    for byte in bytes {
        buffer = (buffer << 8) | u16::from(*byte);
        bits += 8;
        while bits >= width {
            bits -= width;
            let position = (buffer >> bits) as usize & (alphabet.len() - 1);
            text.push(char::from(alphabet[position]));
        }
    }
    if bits != 0 {
        let position = (buffer << (width - bits)) as usize & (alphabet.len() - 1);
        text.push(char::from(alphabet[position]));
    }
    text
}

fn decode_radix(text: &str, alphabet: &[u8], width: usize) -> Option<Vec<u8>> {
    let mut bits = 0;
    let mut buffer = 0u16;
    let mut output = Vec::new();
    for c in text.bytes() {
        let index = alphabet.iter().position(|value| *value == c)? as u16;
        buffer = (buffer << width) | index;
        bits += width;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
        }
    }
    (bits < width && buffer & ((1 << bits) - 1) == 0).then_some(output)
}

fn relationship_text(text: &str, root: &Sheet) -> Option<[u8; 32]> {
    let text = text.trim();
    let binary = if let Some(token) = text.strip_prefix("om:recover/") {
        decode_radix(token, BASE64URL, 6)?
    } else {
        let mut chars = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase());
        let mut payload = String::new();
        let mut start = 0;
        let mut index = 0;
        while let Some(c) = chars.next() {
            if c == ':' {
                let suffix: String = chars.by_ref().take(2).collect();
                if suffix != line_check(index, &payload[start..]) {
                    return None;
                }
                index += 1;
                start = payload.len();
            } else if c.is_ascii() && BASE32.contains(&(c as u8)) {
                payload.push(c);
            } else {
                return None;
            }
        }
        decode_radix(&payload, BASE32, 5)?
    };
    relationship_key(&binary, root).copied()
}

fn to_sheet_form(compact: &str) -> String {
    compact
        .as_bytes()
        .chunks(24)
        .enumerate()
        .map(|(index, line)| {
            let groups: Vec<_> = line
                .chunks(4)
                .map(|g| std::str::from_utf8(g).unwrap())
                .collect();
            let plain = std::str::from_utf8(line).unwrap();
            format!("{} : {}", groups.join(" "), line_check(index, plain))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn root_sheet() -> Sheet {
    Sheet::full_v2(ROOT, SCOPE, None, "home.example", CHECKPOINT).unwrap()
}

#[test]
fn rk2_01_second_sheet_golden() {
    let root = root_sheet();
    let binary = relationship_sheet(&root, &KEY);
    let compact = encode_radix(&binary, BASE32, 5);
    let printed = to_sheet_form(&compact);
    let qr = format!("om:recover/{}", encode_radix(&binary, BASE64URL, 6));
    assert_eq!(binary.len(), 85 + CHECKPOINT.to_bytes().len());
    let actual = json!({
        "binary": hex(&binary), "compact": compact, "sheet": printed,
        "qr": qr, "checkpoint": hex(&CHECKPOINT.to_bytes()), "root": hex(&ROOT)
    });
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(actual, golden["RK2-01"]);
    assert_eq!(relationship_key(&binary, &root), Some(&KEY));
    for text in [compact, printed, qr] {
        assert!(
            parse(&text).is_err(),
            "account/place parser must reject kind 3"
        );
        assert_eq!(relationship_text(&text, &root), Some(KEY));
    }
    let wrong_line = to_sheet_form(&encode_radix(&binary, BASE32, 5)).replacen(" : ", " : !", 1);
    assert!(relationship_text(&wrong_line, &root).is_none());
}

#[test]
fn rk2_02_types_and_matching() {
    let root = root_sheet();
    let legacy_check = sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", &ROOT]);
    for wrong_kind in [
        root.to_bytes(),
        Sheet::full(ROOT, SCOPE, None, "home.example")
            .unwrap()
            .to_bytes(),
        root.place().to_bytes(),
        [&ROOT[..], &legacy_check[..3]].concat(),
    ] {
        assert!(relationship_key(&wrong_kind, &root).is_none());
        let compact = encode_radix(&wrong_kind, BASE32, 5);
        assert!(relationship_text(&compact, &root).is_none());
    }

    let good = relationship_sheet(&root, &KEY);
    for (index, byte) in [(0, 0), (34, 0xa1), (67, 1), (69, b'H'), (100, 0x33)] {
        let mut wrong = good.clone();
        assert_ne!(wrong[index], byte);
        wrong[index] = byte;
        let end = wrong.len() - 4;
        let checksum = check(&wrong[..end]);
        wrong[end..].copy_from_slice(&checksum);
        assert!(relationship_key(&wrong, &root).is_none());
    }
    for host in ["xn--.example", "home..example", "Home.example"] {
        let mut wrong = good[..69].to_vec();
        wrong[68] = host.len() as u8;
        wrong.extend_from_slice(host.as_bytes());
        wrong.extend_from_slice(&CHECKPOINT.to_bytes());
        wrong.extend_from_slice(&check(&wrong));
        assert!(relationship_key(&wrong, &root).is_none());
    }
    let mut bad_checksum = good.clone();
    *bad_checksum.last_mut().unwrap() ^= 1;
    assert!(relationship_key(&bad_checksum, &root).is_none());
    assert!(relationship_key(&[good, vec![0]].concat(), &root).is_none());

    let punycode = Sheet::full_v2(ROOT, SCOPE, None, "xn--bcher-kva.example", CHECKPOINT).unwrap();
    assert_eq!(
        relationship_key(&relationship_sheet(&punycode, &KEY), &punycode),
        Some(&KEY)
    );
    assert_ne!(KEY, ROOT);
    assert_ne!(KEY, record::RecordKeys::derive(&ROOT, &SCOPE).snapshot_key);
}

#[test]
fn rk2_03_snapshot_layer() {
    let snapshot = RelationshipSnapshot::default();
    let nonce = sequence(0);
    let aad = record::snapshot_associated_data(&SCOPE, GENERATION);
    let layer = record::seal_snapshot_layer(&KEY, &SCOPE, GENERATION, nonce, &snapshot);
    let actual = json!({
        "plaintext": hex(&snapshot.to_bytes()), "aad": hex(&aad), "nonce": hex(&nonce),
        "ciphertext_and_tag": hex(&layer[12..])
    });
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(actual, golden["RK2-03"]);
    assert_eq!(
        record::open_snapshot_layer(&KEY, &SCOPE, GENERATION, &layer).unwrap(),
        snapshot
    );
    for (key, scope, generation) in [
        (ROOT, SCOPE, GENERATION),
        (KEY, [0; 32], GENERATION),
        (KEY, SCOPE, GENERATION + 1),
    ] {
        assert!(record::open_snapshot_layer(&key, &scope, generation, &layer).is_err());
    }

    // The mutually-exclusive keys check also runs on a record with valid signed evidence.
    let device = [0x31; 16];
    let signer = DeviceSigner {
        ed_seed: [0x51; 32],
        mldsa_seed: [0x52; 32],
    };
    let credential = Credential {
        device_id: device,
        account_id: CHECKPOINT.account_id,
        signing_pk: signer.public(),
        ik: [0x53; 32],
    };
    let policy = RecoveryPolicy::default_for(RecoveryRoot::from_bytes(ROOT).auth_public_key(7), 7);
    let head = Acs {
        account_id: credential.account_id,
        generation: 1,
        control: [device].into(),
        messaging: [device].into(),
        quarantined: Default::default(),
        revoked: Default::default(),
        credentials: vec![(device, credential.hash())],
        previous: None,
        constitutional: (1, None),
        recovery_commitment: Some(policy.hash()),
        min_profile: PROFILE_V1,
        key_epochs: vec![],
        transparency: None,
    };
    let body = head.body();
    let proof = ds_sig(LBL_ACS_SIG, PROFILE_V1, ROLE_CONTROLLER, &device, &body);
    let signature = sig::signature_entry(
        &device,
        PROFILE_V1,
        ROLE_CONTROLLER,
        signer.sign_with(&proof, [0x54; 32]),
    );
    let generation = record::record_generation(head.generation, 0x1234).unwrap();
    let record = RecoveryRecord {
        generation,
        recovery_epoch: 7,
        head: AcsHeadRef {
            generation: head.generation,
            state_hash: head.hash(),
        },
        evidence: AcsEvidence {
            head: sig::envelope(body, vec![signature]),
            tail: vec![],
            credentials: vec![credential.body()],
            policy: policy.body(),
        },
        providers: vec![ProviderHome::new("home.example", None, Some(SCOPE))],
        relationships: Relationships::Sealed(record::seal_snapshot_layer(
            &KEY,
            &SCOPE,
            generation,
            sequence(0x60),
            &snapshot,
        )),
        extensions: None,
    };
    let raw = record.to_padded_bytes().unwrap();
    RecoveryRecord::from_bytes(&raw).unwrap().verify().unwrap();
    let V::M(mut fields) = decode(&raw).unwrap() else {
        unreachable!("RecoveryRecord is a map")
    };
    fields.push((7, decode(&snapshot.to_bytes()).unwrap()));
    let bad = RecoveryRecord::from_bytes(&encode(&V::M(fields))).unwrap_err();
    assert!(matches!(
        bad,
        om_core::crypto::Error::Binding("record: exactly one of keys 7 and 8")
    ));
}

#[test]
fn rk2a_06_hostname_sheet_goldens() {
    let vector = crate::v2::golden("RK2A-06");
    for (host, expected) in vector.as_object().unwrap() {
        // Include the deliberately invalid spellings with recomputed checks so
        // failures distinguish host syntax from corruption.
        let original = relationship_sheet(&root_sheet(), &KEY);
        let mut binary = original[..68].to_vec();
        binary.push(host.len() as u8);
        binary.extend_from_slice(host.as_bytes());
        binary.extend_from_slice(&CHECKPOINT.to_bytes());
        binary.extend_from_slice(&check(&binary));
        let compact = encode_radix(&binary, BASE32, 5);
        let sheet = to_sheet_form(&compact);
        let qr = format!("om:recover/{}", encode_radix(&binary, BASE64URL, 6));
        let root = Sheet::full_v2(ROOT, SCOPE, None, host, CHECKPOINT);
        assert_eq!(
            json!({"valid": root.is_ok(), "binary": hex(&binary), "compact": compact, "sheet": sheet, "qr": qr}),
            *expected
        );
        for representation in [&compact, &sheet, &qr] {
            assert!(
                parse(representation).is_err(),
                "relationship kind rejected at root entry"
            );
        }
        if let Ok(root) = root {
            assert_eq!(relationship_sheet(&root, &KEY), binary);
            for representation in [&compact, &sheet, &qr] {
                assert_eq!(relationship_text(representation, &root), Some(KEY));
            }
            let mut corrupt = binary;
            *corrupt.last_mut().unwrap() ^= 1;
            assert!(relationship_key(&corrupt, &root).is_none());
        } else {
            // Change only the kind and recompute the check. The normal sheet
            // parser must still reject the host independently of kind/check.
            binary[1] = 1;
            let end = binary.len() - 4;
            let checksum = check(&binary[..end]);
            binary[end..].copy_from_slice(&checksum);
            assert!(Sheet::from_bytes(&binary).is_err(), "{host}");
        }
    }
}
