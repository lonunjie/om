// SPDX-License-Identifier: Apache-2.0
//! Offline ADR-0236–0238 fixtures using the existing W0 types and key primitives.

mod controller;
mod kit;
mod record;

use crate::tests::{bytes, string};
use om_core::control::{LBL_ACS_SIG, verify_genesis};
use om_core::crypto::binding::{Acs, Credential};
use om_core::crypto::cbor::{V, encode};
use om_core::crypto::pqxdh::x25519_pub;
use om_core::crypto::sig::{self, DeviceSigner, ROLE_CONTROLLER};
use om_core::crypto::{PROFILE_V1, unhex};
use om_core::recovery::{RecoveryPolicy, RecoveryRoot};
use serde_json::Value;

const ACCOUNT: [u8; 32] = [0x11; 32];
const ORIGIN: [u8; 16] = [0x22; 16];
const SELECTED: [u8; 16] = [0x33; 16];
const TARGET: [u8; 16] = [0x77; 16];
const SCOPE: [u8; 32] = [0x60; 32];
const ROOT: [u8; 32] = {
    let mut root = [0; 32];
    let mut i = 0;
    while i < 32 {
        root[i] = 0x40 + i as u8;
        i += 1;
    }
    root
};
const OLD_ROOT: [u8; 32] = [0x80; 32];
const OLD_EPOCH: u64 = 6;
const EPOCH: u64 = 7;
const HOST: &str = "home.example";

pub(super) fn golden(key: &str) -> Value {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../vectors/recovery-extension-v2.json"
    ))
    .unwrap();
    assert!(!corpus[key].is_null(), "missing {key}");
    corpus[key].clone()
}

fn raw(vector: &Value, key: &str) -> Vec<u8> {
    unhex(vector[key].as_str().unwrap()).unwrap()
}

fn array(values: impl IntoIterator<Item = V>) -> V {
    V::A(values.into_iter().collect())
}

fn map<const N: usize>(fields: [(u64, V); N]) -> V {
    V::M(fields.to_vec())
}

fn with_field(value: &V, field: u64, replacement: Option<V>) -> V {
    let mut fields = value.m().unwrap().to_vec();
    fields.retain(|(key, _)| *key != field);
    if let Some(replacement) = replacement {
        fields.push((field, replacement));
    }
    V::M(fields)
}

fn signer(seed: u8) -> DeviceSigner {
    DeviceSigner {
        ed_seed: [seed; 32],
        mldsa_seed: [seed; 32],
    }
}

fn vote(key: &DeviceSigner, role: u64, signer: &[u8], body: &[u8], randomness: u8) -> V {
    let input = sig::ds_sig(LBL_ACS_SIG, PROFILE_V1, role, signer, body);
    sig::signature_entry(
        signer,
        PROFILE_V1,
        role,
        key.sign_with(&input, [randomness; 32]),
    )
}

struct Fixture {
    origin_key: DeviceSigner,
    selected_key: DeviceSigner,
    old_key: DeviceSigner,
    current_key: DeviceSigner,
    origin: Credential,
    selected: Credential,
    target: Credential,
    old_policy: RecoveryPolicy,
    current_policy: RecoveryPolicy,
    parent: Acs,
    signed_parent: Vec<u8>,
    replacement: Acs,
}

impl Fixture {
    fn new() -> Self {
        let origin_key = signer(0x81);
        let selected_key = signer(0x82);
        let old_key = RecoveryRoot::from_bytes(OLD_ROOT).auth_signer(OLD_EPOCH);
        let current_key = RecoveryRoot::from_bytes(ROOT).auth_signer(EPOCH);
        let old_policy = RecoveryPolicy::default_for(old_key.public(), OLD_EPOCH);
        let current_policy = RecoveryPolicy::default_for(current_key.public(), EPOCH);
        let credential = |device_id, key: &DeviceSigner, ik_seed| Credential {
            device_id,
            account_id: ACCOUNT,
            signing_pk: key.public(),
            ik: x25519_pub(&[ik_seed; 32]),
        };
        let origin = credential(ORIGIN, &origin_key, 0x91);
        let selected = credential(SELECTED, &selected_key, 0x92);
        let target = credential(TARGET, &signer(0x83), 0x93);
        let parent = Acs {
            account_id: ACCOUNT,
            generation: 1,
            control: [ORIGIN, SELECTED].into(),
            quarantined: Default::default(),
            messaging: Default::default(),
            revoked: Default::default(),
            credentials: vec![(ORIGIN, origin.hash()), (SELECTED, selected.hash())],
            previous: None,
            constitutional: (1, None),
            recovery_commitment: Some(old_policy.hash()),
            min_profile: PROFILE_V1,
            key_epochs: vec![],
            transparency: None,
        };
        let signed_parent = sig::envelope(
            parent.body(),
            vec![
                vote(&origin_key, ROLE_CONTROLLER, &ORIGIN, &parent.body(), 1),
                vote(&selected_key, ROLE_CONTROLLER, &SELECTED, &parent.body(), 2),
            ],
        );
        assert_eq!(
            verify_genesis(&signed_parent, &[origin.body(), selected.body()]).unwrap(),
            parent
        );
        let mut replacement = parent.clone();
        replacement.generation += 1;
        replacement.previous = Some(parent.hash());
        replacement.recovery_commitment = Some(current_policy.hash());
        Self {
            origin_key,
            selected_key,
            old_key,
            current_key,
            origin,
            selected,
            target,
            old_policy,
            current_policy,
            parent,
            signed_parent,
            replacement,
        }
    }

    fn credentials(&self) -> Vec<Vec<u8>> {
        vec![self.origin.body(), self.selected.body()]
    }

    fn replacement_votes(&self) -> (V, V) {
        let body = self.replacement.body();
        (
            vote(&self.origin_key, ROLE_CONTROLLER, &ORIGIN, &body, 6),
            vote(&self.selected_key, ROLE_CONTROLLER, &SELECTED, &body, 4),
        )
    }

    fn pinned_replacement(&self) -> Acs {
        let (origin, selected) = self.replacement_votes();
        let signed = sig::envelope(self.replacement.body(), vec![origin, selected]);
        assert_eq!(signed, raw(&golden("KITS2-01"), "signed_head"));
        om_core::control::verify_successor(&self.parent, &signed, &self.credentials()).unwrap()
    }
}

fn required_bytes(value: &V, key: u64) -> Option<&[u8]> {
    value.get(key).ok()?.b().ok()
}
