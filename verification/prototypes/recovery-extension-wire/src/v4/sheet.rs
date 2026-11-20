// SPDX-License-Identifier: Apache-2.0

use super::*;
use om_core::crypto::sig::ds_hash;
use om_core::recovery_sheet::{LBL_SHEET_CHECK, OwnerCheckpoint, Sheet, valid_host};

const PIN: &str = "OM-ACS-SIGNED-ENVELOPE-PIN-v1"; // ADR-0247 provisional label.

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

fn encode_bit_groups(data: &[u8], bits: usize, alphabet: &[u8]) -> String {
    let mut word = 0u16;
    let mut pending = 0usize;
    let mut out = String::new();
    for octet in data {
        word = (word << 8) | u16::from(*octet);
        pending += 8;
        while pending >= bits {
            pending -= bits;
            out.push(alphabet[(usize::from(word) >> pending) & (alphabet.len() - 1)] as char);
        }
    }
    if pending > 0 {
        out.push(alphabet[(usize::from(word) << (bits - pending)) & (alphabet.len() - 1)] as char);
    }
    out
}

fn forms(wire: &[u8]) -> Value {
    json!({
        "binary": hex(wire),
        "base32": encode_bit_groups(wire, 5, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"),
        "qr": format!("om:recover/{}", encode_bit_groups(
            wire, 6, b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")),
    })
}

fn checksummed(mut wire: Vec<u8>) -> Vec<u8> {
    wire.extend_from_slice(&ds_hash(LBL_SHEET_CHECK, &wire)[..4]);
    wire
}

fn sheet(
    version: u8,
    kind: u8,
    ck: OwnerCheckpoint,
    secret: &[u8; 32],
    pin: Option<[u8; 32]>,
) -> Vec<u8> {
    let mut wire = vec![version, kind];
    wire.extend_from_slice(secret);
    wire.extend_from_slice(&SCOPE);
    wire.extend_from_slice(&[0, 0, HOST.len() as u8]);
    wire.extend_from_slice(HOST.as_bytes());
    wire.extend_from_slice(&ck.to_bytes());
    if let Some(pin) = pin {
        wire.extend_from_slice(&pin);
    }
    checksummed(wire)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Parsed {
    version: u8,
    kind: u8,
    scope: [u8; 32],
    port: u16,
    host: String,
    ck: OwnerCheckpoint,
    envelope_pin: Option<[u8; 32]>,
}

fn parse(wire: &[u8]) -> Option<Parsed> {
    let version = *wire.first()?;
    let kind = *wire.get(1)?;
    if ![2, 3].contains(&version) || ![1, 3].contains(&kind) {
        return None;
    }
    let host_len = *wire.get(68)? as usize;
    let host_end = 69 + host_len;
    let host = std::str::from_utf8(wire.get(69..host_end)?).ok()?;
    let check_start = wire.len().checked_sub(4)?;
    let ck_end = check_start.checked_sub(if version == 3 { 32 } else { 0 })?;
    let cb = wire.get(host_end..ck_end)?;
    if !valid_host(host)
        || !(82..=90).contains(&cb.len())
        || !same(
            &wire[check_start..],
            &ds_hash(LBL_SHEET_CHECK, &wire[..check_start])[..4],
        )
    {
        return None;
    }
    let ck = OwnerCheckpoint::from_bytes(cb)?;
    if ck.to_bytes() != cb {
        return None;
    }
    Some(Parsed {
        version,
        kind,
        scope: wire[34..66].try_into().ok()?,
        port: u16::from_be_bytes(wire[66..68].try_into().ok()?),
        host: host.into(),
        ck,
        envelope_pin: if version == 3 {
            Some(wire[ck_end..check_start].try_into().ok()?)
        } else {
            None
        },
    })
}

fn paired(root: &Parsed, relationship: &Parsed) -> bool {
    root.kind == 1
        && relationship.kind == 3
        && root.version == relationship.version
        && root.ck == relationship.ck
        && root.envelope_pin == relationship.envelope_pin
        && root.scope == relationship.scope
        && root.host == relationship.host
        && root.port == relationship.port
}

fn anchor(sheet: &Parsed, envelope: &[u8]) -> Option<Acs> {
    if let Some(pin) = sheet.envelope_pin {
        if !same(&pin, &ds_hash(PIN, envelope)) {
            return None;
        }
    }
    let (state, _) = signed_state(envelope)?;
    (checkpoint(&state) == sheet.ck).then_some(state)
}

fn root_v3(state: &Acs, wire: &[u8]) -> Vec<u8> {
    sheet(3, 1, checkpoint(state), &ROOT, Some(ds_hash(PIN, wire)))
}

#[test]
fn rk2d_01_two_v3_sheets_custody_the_exact_non_genesis_envelope() {
    let v = vector("RK2D-01");
    let r = Rotated::new();
    let f = &r.ceremony;
    assert_eq!(&f.parent.hash()[..], raw(&v, "body_hash"));
    assert_eq!(f.signed_parent, raw(&v, "signed_envelope"));
    let ck = checkpoint(&f.parent);
    assert_eq!(ck.to_bytes(), raw(&v, "checkpoint"));
    let pin = ds_hash(PIN, &f.signed_parent);
    assert_eq!(pin.as_slice(), raw(&v, "envelope_pin"));
    let root = root_v3(&f.parent, &f.signed_parent);
    let relationship = sheet(3, 3, ck, &std::array::from_fn(|i| i as u8), Some(pin));
    assert_eq!(forms(&root), v["root"]);
    assert_eq!(forms(&relationship), v["relationship"]);
    for wire in [&root, &relationship] {
        assert_eq!(wire.len(), 105 + HOST.len() + ck.to_bytes().len());
        assert_eq!(
            anchor(&parse(wire).unwrap(), &f.signed_parent),
            Some(f.parent.clone())
        );
    }
    assert!(paired(
        &parse(&root).unwrap(),
        &parse(&relationship).unwrap()
    ));
    assert!(verify_successor(&r.predecessor, &f.signed_parent, &f.credentials()).is_err());
}

#[test]
fn rk2d_02_signature_only_substitution_has_same_body_and_different_envelope_pin() {
    let v = vector("RK2D-02");
    let r = Rotated::new();
    let f = &r.ceremony;
    let body = f.parent.body();
    let old_key = signer(0x81);
    let original = vote(&old_key, ROLE_CONTROLLER, &ORIGIN, &body, 14);
    let extra = vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &body, 68);
    let mut signature = required_bytes(&original, 3).unwrap().to_vec();
    signature[0] ^= 1;
    let tampered = with_field(&original, 3, Some(bytes(signature)));
    let added = sig::envelope(body.clone(), vec![original.clone(), extra.clone()]);
    let both = parse(&root_v3(&f.parent, &added)).unwrap();
    let one = parse(&root_v3(&f.parent, &f.signed_parent)).unwrap();
    let resigned = sig::envelope(
        body.clone(),
        vec![vote(&old_key, ROLE_CONTROLLER, &ORIGIN, &body, 69)],
    );
    for (name, wire, custodied) in [
        (
            "signature_byte",
            sig::envelope(body.clone(), vec![tampered]),
            &one,
        ),
        ("signature_added", added.clone(), &one),
        (
            "signature_reordered",
            sig::envelope(body.clone(), vec![extra, original.clone()]),
            &both,
        ),
        ("signature_removed", f.signed_parent.clone(), &both),
        (
            "signature_duplicate",
            sig::envelope(body.clone(), vec![original.clone(), original]),
            &one,
        ),
        ("signed_again", resigned.clone(), &one),
    ] {
        let envelope = decode(&wire).unwrap();
        let candidate = Acs::from_body(required_bytes(&envelope, 1).unwrap()).unwrap();
        assert_eq!(f.parent.hash(), candidate.hash());
        assert_eq!(ds_hash(PIN, &wire).as_slice(), raw(&v, name));
        assert!(anchor(custodied, &wire).is_none(), "{name}");
    }
    assert!(anchor(&both, &added).is_some());
    let q_creds = [r.previous_origin.body(), f.selected.body()];
    let (_, _, signers) =
        om_core::control::open_signed(&added, &q_creds, Some(&r.predecessor)).unwrap();
    assert_eq!(signers, [ORIGIN, SELECTED].into());
    assert!(verify_successor(&r.predecessor, &resigned, &q_creds).is_ok());
}

#[test]
fn rk2d_03_legacy_body_pin_and_sheet_format_negatives() {
    let v = vector("RK2D-03");
    let r = Rotated::new();
    let f = &r.ceremony;
    let ck = checkpoint(&f.parent);
    let v2root = sheet(2, 1, ck, &ROOT, None);
    let v2rel = sheet(2, 3, ck, &std::array::from_fn(|i| i as u8), None);
    assert_eq!(
        v2root,
        Sheet::full_v2(ROOT, SCOPE, None, HOST, ck)
            .unwrap()
            .to_bytes()
    );
    assert_eq!(forms(&v2root), v["legacy_root"]);
    assert_eq!(forms(&v2rel), v["legacy_relationship"]);
    let v3root = root_v3(&f.parent, &f.signed_parent);
    assert!(Sheet::from_bytes(&v3root).is_err());
    let root = parse(&v3root).unwrap();
    assert!(paired(&parse(&v2root).unwrap(), &parse(&v2rel).unwrap()));
    assert!(!paired(&root, &parse(&v2rel).unwrap()));
    assert_eq!(v["v3_v2_pair"], "refused");
    let body = f.parent.body();
    let different = sig::envelope(
        body.clone(),
        vec![vote(&signer(0x81), ROLE_CONTROLLER, &ORIGIN, &body, 69)],
    );
    assert!(anchor(&parse(&v2root).unwrap(), &different).is_some());
    assert!(anchor(&root, &different).is_none());
    let mut altered = v3root.clone();
    altered[30] ^= 1;
    assert!(parse(&altered).is_none());
    assert!(parse(&v3root[..v3root.len() - 1]).is_none());
    for (offset, unknown) in [(0, 4), (1, 2)] {
        let mut body = v3root[..v3root.len() - 4].to_vec();
        body[offset] = unknown;
        assert!(parse(&checksummed(body)).is_none());
    }
    let other = sheet(3, 3, ck, &[1; 32], Some(ds_hash(PIN, &different)));
    assert!(!paired(&root, &parse(&other).unwrap()));
}

// Model one atomic durable write of the issued sheet and its exact verified envelope.
#[derive(Default)]
struct Custody {
    committed: Option<(Vec<u8>, Vec<u8>)>,
}

impl Custody {
    fn publish(&self) -> Option<(&[u8], Acs)> {
        let (sheet, envelope) = self.committed.as_ref()?;
        Some((sheet, anchor(&parse(sheet)?, envelope)?))
    }
}

#[test]
fn rk2d_04_issue_atomically_and_verify_next_link_after_restart() {
    let v = vector("RK2D-04");
    let r = Rotated::new();
    let f = &r.ceremony;
    assert_eq!(
        verify_successor(
            &r.predecessor,
            &f.signed_parent,
            &[r.previous_origin.body(), f.selected.body()]
        )
        .unwrap(),
        f.parent
    );
    let draft = root_v3(&f.parent, &f.signed_parent);
    let mut store = Custody::default();
    assert!(store.publish().is_none()); // restart before commit
    store.committed = Some((draft, f.signed_parent.clone()));
    let restarted = Custody {
        committed: store.committed,
    };
    let (published, parent) = restarted.publish().unwrap();
    assert_eq!(v["after_restart"], "exact_anchor");
    let parsed = parse(published).unwrap();
    assert!(anchor(&parsed, &r.signed_predecessor).is_none());
    let mut fork = parent.clone();
    fork.recovery_commitment = Some(f.current_policy.hash());
    let fork_body = fork.body();
    let fork = sig::envelope(
        fork_body.clone(),
        vec![
            vote(&signer(0x81), ROLE_CONTROLLER, &ORIGIN, &fork_body, 14),
            vote(&f.selected_key, ROLE_CONTROLLER, &SELECTED, &fork_body, 15),
        ],
    );
    assert!(anchor(&parsed, &fork).is_none());
    let mut child = parent.clone();
    child.generation += 1;
    child.previous = Some(parent.hash());
    child.messaging.insert(TARGET);
    child.credentials.push((TARGET, f.target.hash()));
    let body = child.body();
    let signed = sig::envelope(
        body.clone(),
        vec![vote(&f.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 70)],
    );
    assert_eq!(signed, raw(&v, "signed_child"));
    assert_eq!(f.origin.body(), raw(&v, "rotated_credential"));
    assert_eq!(
        verify_successor(
            &parent,
            &signed,
            &[f.origin.body(), f.selected.body(), f.target.body()]
        )
        .unwrap(),
        child
    );
    let old = sig::envelope(
        body.clone(),
        vec![vote(&signer(0x81), ROLE_CONTROLLER, &ORIGIN, &body, 70)],
    );
    assert!(
        verify_successor(
            &parent,
            &old,
            &[r.previous_origin.body(), f.selected.body()]
        )
        .is_err()
    );
}
