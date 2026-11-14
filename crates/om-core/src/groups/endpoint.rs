// SPDX-License-Identifier: MPL-2.0
//! Binding of an MLS leaf to an ACS-committed `DeviceCredential` (GROUP_CHAT_ARCHITECTURE "OM
//! authentication for MLS", ADR-0189 1.4(b)(c)).
//!
//! An MLS leaf is admitted, in an outgoing Add, an incoming Commit or a Welcome, only when its credential names a
//! DeviceID and its leaf signature key is the Ed25519 half of the `signing_public_key` of a `DeviceCredential` that the
//! **current validated head** of an account commits to, and that head lists the device as an active control or
//! messaging device (not revoked, not quarantined). The heads consulted are the ones Core itself validated: its own
//! account's chain tip and the pinned head of each `Active` relationship. Nothing a peer asserts about a leaf (a
//! release's `device`/`pk`, a Commit's `added` list, a Welcome's directory) is authority; those fields are only
//! compared against what the heads prove. Fail closed: an endpoint no head proves is refused (`endpoint_unbound`).
//! A Room needs more than two accounts only once the heads of non-relationship accounts can be anchored (D37, D44).

use super::mls::Endpoint;
use super::{GroupError, R, err};
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::unhex;
use crate::state::{AppState, RelState};
use std::collections::BTreeSet;

/// An endpoint the heads prove: the device, its account and the full credential key (the directory key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub device: Vec<u8>,
    pub account: Vec<u8>,
    pub pk: Vec<u8>,
}

/// Whether `acs` (a validated head) lists `cred` as an active device credential of its own account.
pub fn active_in(acs: &Acs, cred: &Credential) -> bool {
    let d = cred.device_id;
    cred.account_id == acs.account_id
        && !acs.revoked.contains(&d)
        && !acs.quarantined.contains(&d)
        && (acs.control.contains(&d) || acs.messaging.contains(&d))
        && acs
            .credentials
            .iter()
            .any(|(x, hh)| *x == d && *hh == cred.hash())
}

fn endpoint_in(acs: &Acs, creds: &[Credential], ep: &Endpoint) -> Option<Bound> {
    let did: [u8; 16] = ep.device.as_slice().try_into().ok()?;
    let cred = creds
        .iter()
        .find(|c| c.device_id == did && active_in(acs, c))?;
    (cred.signing_pk.len() >= 32 && ep.signature_key == cred.signing_pk[..32]).then(|| Bound {
        device: ep.device.clone(),
        account: cred.account_id.to_vec(),
        pk: cred.signing_pk.clone(),
    })
}

impl AppState {
    /// The validated heads this device holds: its own account's chain tip, and the pinned head of each active
    /// relationship.
    pub(super) fn group_heads(&self) -> Vec<(Acs, Vec<Credential>)> {
        let mut v = vec![];
        if let Ok(tip) = self.carry_own_tip() {
            let creds = self
                .acs_credentials
                .iter()
                .filter_map(|h| unhex(h).ok())
                .filter_map(|b| Credential::from_body(&b).ok())
                .collect();
            v.push((tip, creds));
        }
        for r in self.relationships.values() {
            if r.state != RelState::Active || r.blocked {
                continue;
            }
            let Some(acs) = unhex(&r.peer_acs_body)
                .ok()
                .and_then(|b| Acs::from_body(&b).ok())
            else {
                continue;
            };
            if crate::crypto::hex(&acs.account_id) != r.peer_account_id {
                continue;
            }
            let creds = r
                .peer_credentials
                .iter()
                .filter_map(|h| unhex(h).ok())
                .filter_map(|b| Credential::from_body(&b).ok())
                .collect();
            v.push((acs, creds));
        }
        v
    }

    /// Prove every endpoint against the validated heads, restricted to `only` (one account) when given. Refuses an
    /// empty list, a repeated device or a repeated leaf key, and any endpoint no head proves.
    pub(super) fn group_bind(&self, eps: &[Endpoint], only: Option<&[u8]>) -> R<Vec<Bound>> {
        if eps.is_empty() {
            return err("endpoint_unbound");
        }
        let mut devs = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for e in eps {
            if !devs.insert(e.device.clone()) || !keys.insert(e.signature_key.clone()) {
                return err("endpoint_duplicate");
            }
        }
        let heads = self.group_heads();
        eps.iter()
            .map(|e| {
                heads
                    .iter()
                    .filter(|(a, _)| only.is_none_or(|o| o == a.account_id))
                    .find_map(|(a, c)| endpoint_in(a, c, e))
                    .ok_or(GroupError("endpoint_unbound"))
            })
            .collect()
    }
}
