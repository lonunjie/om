// SPDX-License-Identifier: MPL-2.0
//! Test helper for the groups flow tests: builds the validated account heads Core consults
//! when it proves an MLS leaf against an ACS-committed `DeviceCredential`.
#![allow(dead_code)]

use om_core::crypto::binding::{Acs, Credential};
use om_core::crypto::cbor::{self, V};
use om_core::crypto::hex;
use om_core::state::AppState;

/// One device of an account: its 16-byte id and its full OM signing public key.
pub type Dev = (Vec<u8>, Vec<u8>);

pub struct Head {
    pub account: Vec<u8>,
    pub acs: Acs,
    pub creds: Vec<Credential>,
}

/// A head of `account` that lists every device in `control` as a control device and every device in `messaging` as a
/// messaging device, each with its credential committed.
pub fn head(account: &[u8], control: &[Dev], messaging: &[Dev]) -> Head {
    head_with(account, control, messaging, &[], &[])
}

/// As [`head`], plus `revoked` and `quarantined` devices: their credentials stay committed (the head still lists the
/// hash) but the device is no longer active.
pub fn head_with(
    account: &[u8],
    control: &[Dev],
    messaging: &[Dev],
    revoked: &[Dev],
    quarantined: &[Dev],
) -> Head {
    let cred = |d: &Dev| Credential {
        device_id: d.0.clone().try_into().unwrap(),
        account_id: account.try_into().unwrap(),
        signing_pk: d.1.clone(),
        ik: [7; 32],
    };
    let creds: Vec<Credential> = control
        .iter()
        .chain(messaging)
        .chain(revoked)
        .chain(quarantined)
        .map(cred)
        .collect();
    let acs = Acs {
        account_id: account.try_into().unwrap(),
        generation: 1,
        control: control
            .iter()
            .map(|d| d.0.clone().try_into().unwrap())
            .collect(),
        quarantined: quarantined
            .iter()
            .map(|d| d.0.clone().try_into().unwrap())
            .collect(),
        messaging: messaging
            .iter()
            .map(|d| d.0.clone().try_into().unwrap())
            .collect(),
        revoked: revoked
            .iter()
            .map(|d| d.0.clone().try_into().unwrap())
            .collect(),
        credentials: creds.iter().map(|c| (c.device_id, c.hash())).collect(),
        previous: None,
        constitutional: (1, None),
        recovery_commitment: None,
        min_profile: 1,
        key_epochs: vec![],
        transparency: None,
    };
    Head {
        account: account.to_vec(),
        acs,
        creds,
    }
}

/// Make `own` this device's own validated chain and pin each of `peers` on the relationship with that account.
pub fn install(app: &mut AppState, own: &Head, peers: &[&Head]) {
    app.acs_chain = vec![hex(&cbor::encode(&V::M(vec![
        (0, V::U(0)),
        (1, V::B(own.acs.body())),
    ])))];
    app.acs_credentials = own.creds.iter().map(|c| hex(&c.body())).collect();
    for r in app.relationships.values_mut() {
        if let Some(p) = peers.iter().find(|p| hex(&p.account) == r.peer_account_id) {
            r.peer_acs_body = hex(&p.acs.body());
            r.peer_credentials = p.creds.iter().map(|c| hex(&c.body())).collect();
        } else {
            r.peer_acs_body = "00".into();
            r.peer_credentials = vec![];
        }
    }
}
