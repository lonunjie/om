// SPDX-License-Identifier: MPL-2.0
//! Binding of an MLS leaf to an ACS-committed `DeviceCredential` (GROUP_CHAT_ARCHITECTURE "OM
//! authentication for MLS", ADR-0189 1.4(b)(c)).
//!
//! Every MLS leaf has a signature key dedicated to MLS, generated inside the engine; the device's own key never
//! enters the engine. The leaf's BasicCredential carries the DeviceID and the device's own composite signature over
//! (DeviceID, leaf key) under the label [`LBL_LEAF_SIG`] (role `device`): the attestation. An MLS leaf is admitted, in
//! an outgoing Add, an incoming Commit or a Welcome, only when that attestation verifies under the `signing_public_key`
//! of a `DeviceCredential` that the **current validated head** of an account commits to, and that head lists the
//! device as an active control or messaging device (not revoked, not quarantined). A leaf with no attestation, a wrong
//! one, or whose key is the device's own key never binds. The heads consulted are the ones Core itself validated: its own
//! account's chain tip and the pinned head of each `Active` relationship. Nothing a peer asserts about a leaf (a
//! release's `device`/`pk`, a Commit's `added` list, a Welcome's directory) is authority; those fields are only
//! compared against what the heads prove. Fail closed: an endpoint no head proves is refused (`endpoint_unbound`).
//! A Room needs more than two accounts only once the heads of non-relationship accounts can be anchored (D37, D44).

use super::mls::Endpoint;
use super::{GroupError, R, err};
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::cbor::{self, V};
use crate::crypto::sig::{self, DeviceSigner, PK_LEN, ROLE_DEVICE, ds_sig};
use crate::crypto::unhex;
use crate::state::{AppState, RelState};
use std::collections::BTreeSet;

/// `DS-SIG` label of the MLS leaf attestation (`om-domain-separation.json`, role `device`).
pub const LBL_LEAF_SIG: &str = "OM-GROUP-MLS-LEAF-SIG-v1";

fn leaf_body(device: &[u8], leaf_key: &[u8]) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::B(device.to_vec())),
        (1, V::B(leaf_key.to_vec())),
    ]))
}

/// The device's attestation of its MLS leaf key: its composite signature (role `device`) over the DeviceID and the
/// leaf key. The result goes into the engine's credential (`GroupCrypto::set_attestation`).
pub fn attest_leaf(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    leaf_key: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    signer.sign_with(
        &ds_sig(
            LBL_LEAF_SIG,
            crate::crypto::PROFILE_V1,
            ROLE_DEVICE,
            device_id,
            &leaf_body(device_id, leaf_key),
        ),
        rnd,
    )
}

/// Whether `attestation` is the attestation by the device owning credential key `device_pk` of exactly this leaf.
/// The leaf key must not be the device's own Ed25519 key: the MLS key is dedicated, never shared.
pub fn leaf_attested(device_pk: &[u8], device: &[u8], leaf_key: &[u8], attestation: &[u8]) -> bool {
    device_pk.len() == PK_LEN
        && !device.is_empty()
        && leaf_key != &device_pk[..32]
        && sig::verify(
            device_pk,
            &ds_sig(
                LBL_LEAF_SIG,
                crate::crypto::PROFILE_V1,
                ROLE_DEVICE,
                device,
                &leaf_body(device, leaf_key),
            ),
            attestation,
        )
        .is_ok()
}

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
    leaf_attested(
        &cred.signing_pk,
        &ep.device,
        &ep.signature_key,
        &ep.attestation,
    )
    .then(|| Bound {
        device: ep.device.clone(),
        account: cred.account_id.to_vec(),
        pk: cred.signing_pk.clone(),
    })
}

impl AppState {
    /// The validated heads this device holds, one per account: its own account's chain tip, and the pinned head of
    /// each active relationship. Where two heads name one account only the newest generation counts (an older one may
    /// still list a device a newer one revoked); two different heads at the same generation are a fork and the
    /// account then has no head at all (fail closed).
    pub(super) fn group_heads(&self) -> Vec<(Acs, Vec<Credential>)> {
        let mut all: Vec<(Acs, Vec<Credential>)> = vec![];
        if let Ok(tip) = self.carry_own_tip() {
            let creds = self
                .acs_credentials
                .iter()
                .filter_map(|h| unhex(h).ok())
                .filter_map(|b| Credential::from_body(&b).ok())
                .collect();
            all.push((tip, creds));
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
            all.push((acs, creds));
        }
        // heads a Welcome carried for accounts this device has no relationship head for (verified when they arrived)
        for rh in self.group_carriage.room_heads.values() {
            let (Some(acs), creds) = (
                unhex(&rh.state).ok().and_then(|b| Acs::from_body(&b).ok()),
                rh.credentials
                    .iter()
                    .filter_map(|h| unhex(h).ok())
                    .filter_map(|b| Credential::from_body(&b).ok())
                    .collect::<Vec<_>>(),
            ) else {
                continue;
            };
            all.push((acs, creds));
        }
        let mut best: Vec<(Acs, Vec<Credential>)> = vec![];
        let mut forked: BTreeSet<[u8; 32]> = BTreeSet::new();
        for (a, c) in all {
            match best.iter_mut().find(|(b, _)| b.account_id == a.account_id) {
                None => best.push((a, c)),
                Some((b, bc)) => {
                    if a.generation > b.generation {
                        *b = a;
                        *bc = c;
                    } else if a.generation == b.generation {
                        if a.hash() == b.hash() {
                            bc.extend(c);
                        } else {
                            forked.insert(a.account_id);
                        }
                    }
                }
            }
        }
        best.retain(|(a, _)| !forked.contains(&a.account_id));
        best
    }

    /// Whether the newest validated head of `account` lists `device` (the credential whose key is `pk`) as an active
    /// control or messaging device, not revoked and not quarantined. No head for the account: not authorized.
    pub(super) fn group_device_authorized(&self, account: &[u8], device: &[u8], pk: &[u8]) -> bool {
        self.group_heads().iter().any(|(a, creds)| {
            a.account_id == account
                && creds
                    .iter()
                    .any(|c| c.device_id == device && c.signing_pk == pk && active_in(a, c))
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::rng::insecure_test_rng::InsecureTestRng;

    #[test]
    fn only_a_dedicated_key_attested_by_the_very_device_binds() {
        let mut rng = InsecureTestRng::from_label(b"leaf");
        let dev = DeviceSigner::generate(&mut rng);
        let other = DeviceSigner::generate(&mut rng);
        let (id, pk) = ([5u8; 16], dev.public());
        let leaf = [7u8; 32];
        let att = attest_leaf(&dev, &id, &leaf, [1; 32]);
        assert!(leaf_attested(&pk, &id, &leaf, &att));
        // another device's attestation, another key, another DeviceID, no attestation: all refused
        let by_other = attest_leaf(&other, &id, &leaf, [1; 32]);
        assert!(!leaf_attested(&pk, &id, &leaf, &by_other));
        assert!(!leaf_attested(&pk, &id, &[8u8; 32], &att));
        assert!(!leaf_attested(&pk, &[6u8; 16], &leaf, &att));
        assert!(!leaf_attested(&pk, &id, &leaf, &[]));
        // the device's own Ed25519 key is never accepted as the MLS leaf key, even when properly signed
        let shared = attest_leaf(&dev, &id, &pk[..32], [1; 32]);
        assert!(!leaf_attested(&pk, &id, &pk[..32], &shared));
    }
}
