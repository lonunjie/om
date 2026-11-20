// SPDX-License-Identifier: Apache-2.0
//! Offline evidence for ADR-0240–0242. These tests have no product entry points.

mod controller;
mod kit;
mod record;
#[path = "../v4/mod.rs"]
mod v4;

use super::{golden as v2, *};
use om_core::control::verify_successor;
use om_core::crypto::cbor::decode;
use om_core::crypto::sig::ROLE_RECOVERY;

fn golden(key: &str) -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../vectors/recovery-extension-v3.json"
    ))
    .unwrap();
    assert!(!corpus[key].is_null(), "missing {key}");
    corpus[key].clone()
}

struct Rotated {
    ceremony: Fixture,
    predecessor: Acs,
    signed_predecessor: Vec<u8>,
    previous_origin: Credential,
}

impl Rotated {
    fn new() -> Self {
        let mut f = Fixture::new();
        let predecessor = f.parent.clone();
        let signed_predecessor = f.signed_parent.clone();
        let previous_origin = f.origin.clone();
        f.origin_key = signer(0x84);
        f.origin.signing_pk = f.origin_key.public();
        f.origin.ik = x25519_pub(&[0x94; 32]);
        f.parent.generation = 2;
        f.parent.previous = Some(predecessor.hash());
        f.parent.credentials[0].1 = f.origin.hash();
        f.parent.key_epochs = vec![(ORIGIN, 1)];
        let body = f.parent.body();
        f.signed_parent = sig::envelope(
            body.clone(),
            vec![vote(&signer(0x81), ROLE_CONTROLLER, &ORIGIN, &body, 14)],
        );
        assert_eq!(
            verify_successor(&predecessor, &f.signed_parent, &[previous_origin.body()]).unwrap(),
            f.parent
        );
        f.replacement = f.parent.clone();
        f.replacement.generation += 1;
        f.replacement.previous = Some(f.parent.hash());
        f.replacement.recovery_commitment = Some(f.current_policy.hash());
        Self {
            ceremony: f,
            predecessor,
            signed_predecessor,
            previous_origin,
        }
    }
}

fn signed_state(wire: &[u8]) -> Option<(Acs, V)> {
    let envelope = decode(wire).ok()?;
    if envelope.m().ok()?.len() != 2 {
        return None;
    }
    let raw = required_bytes(&envelope, 1)?;
    let state = Acs::from_body(raw).ok()?;
    let credential_ids: std::collections::BTreeSet<_> =
        state.credentials.iter().map(|(device, _)| device).collect();
    if state.body() != raw
        || !(1..=64).contains(&state.control.len())
        || state.quarantined.len() > 64
        || state.messaging.len() > 256
        || state.revoked.len() > 1024
        || state.credentials.len() > 320
        || credential_ids.len() != state.credentials.len()
    {
        return None;
    }
    let entries = envelope.get(2).ok()?.a().ok()?;
    if !(1..=16).contains(&entries.len()) {
        return None;
    }
    let mut signers = std::collections::BTreeSet::new();
    for entry in entries {
        let signer = required_bytes(entry, 0)?;
        let valid_role = match entry.get(2).ok()?.u().ok()? {
            ROLE_CONTROLLER => signer.len() == 16,
            ROLE_RECOVERY => signer == state.account_id,
            _ => false,
        };
        if entry.m().ok()?.len() != 4
            || entry.get(1).ok()?.u().ok()? != PROFILE_V1
            || required_bytes(entry, 3)?.len() != sig::SIG_LEN
            || !valid_role
            || !signers.insert(signer)
        {
            return None;
        }
    }
    Some((state, envelope))
}
