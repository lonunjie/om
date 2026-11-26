// SPDX-License-Identifier: MPL-2.0
//! Verification of a signed service or bridge control state before a Room trusts it (ADR-0269, ADR-0262).
//!
//! A `SignedEnvelope` around a `ServiceControlState` or `BridgeControlState` carries controller-role signatures made
//! by devices. A controller named in the state is a single account (kind 1); each signing device must be an active
//! control device of that account's own control head, whose credential that head commits to. Other controller kinds
//! (organization, Space, Room governance) have no verification path in v1, so a state naming one is refused rather
//! than half trusted. The result feeds [`crate::service_control::ServiceChain::accept`], which enforces thresholds,
//! rollback, forks and endpoint rules; nothing here decides authority on its own.

use crate::bridge_control;
use crate::crypto::PROFILE_V1;
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::cbor;
use crate::crypto::sig::{self, ROLE_CONTROLLER, ds_sig};
use crate::service_control::{Principal, ServiceState};
use std::collections::BTreeSet;

pub const LBL_SERVICE_SIG: &str = "OM-SERVICE-CONTROL-SIG-v1";
pub const LBL_BRIDGE_SIG: &str = "OM-BRIDGE-CONTROL-SIG-v1";

/// The principal kind of a single account controller.
pub const KIND_ACCOUNT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Service,
    Bridge,
}

/// An account whose own control head and device credentials the verifier already trusts (ours or a contact's).
#[derive(Clone, Debug)]
pub struct ControllerAccount {
    pub head: Acs,
    pub credentials: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    Malformed,
    /// The state names a controller that is not a single account.
    UnverifiableController,
    /// A signer is not an active control device, with a committed credential, of any known controller account.
    UnknownSigner,
    BadSignature,
    DuplicateSigner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    pub state: ServiceState,
    /// Controller accounts with at least one valid signature, each once.
    pub approvers: Vec<Principal>,
}

fn controller_device<'a>(
    accounts: &'a [ControllerAccount],
    device: &[u8; 16],
) -> Option<(&'a ControllerAccount, Credential)> {
    accounts.iter().find_map(|a| {
        let h = &a.head;
        if !h.control.contains(device)
            || h.quarantined.contains(device)
            || h.revoked.contains(device)
        {
            return None;
        }
        a.credentials
            .iter()
            .filter_map(|c| Credential::from_body(c).ok())
            .find(|c| {
                c.device_id == *device
                    && c.account_id == h.account_id
                    && h.credentials
                        .iter()
                        .any(|(d, x)| d == device && *x == c.hash())
            })
            .map(|c| (a, c))
    })
}

/// Open `signed` (the bytes of a `SignedEnvelope`) for `kind`, verifying every signature.
pub fn open_signed_state(
    kind: ObjectKind,
    signed: &[u8],
    accounts: &[ControllerAccount],
) -> Result<Opened, OpenError> {
    let bad = |_| OpenError::Malformed;
    let v = cbor::decode(signed).map_err(bad)?;
    let body = v.get(1).and_then(|b| b.b()).map_err(bad)?.to_vec();
    let entries = v.get(2).and_then(|a| a.a()).map_err(bad)?;
    if entries.is_empty() || entries.len() > 16 {
        return Err(OpenError::Malformed);
    }
    let (state, label) = match kind {
        ObjectKind::Service => (ServiceState::from_body(&body), LBL_SERVICE_SIG),
        ObjectKind::Bridge => (bridge_control::state_from_body(&body), LBL_BRIDGE_SIG),
    };
    let state = state.map_err(|_| OpenError::Malformed)?;
    if state.controllers.iter().any(|c| c.0 != KIND_ACCOUNT) {
        return Err(OpenError::UnverifiableController);
    }
    let mut seen = BTreeSet::new();
    let mut approvers: BTreeSet<Principal> = BTreeSet::new();
    for e in entries {
        let signer: [u8; 16] = e
            .get(0)
            .and_then(|s| s.b())
            .map_err(bad)?
            .try_into()
            .map_err(|_| OpenError::Malformed)?;
        let profile = e.get(1).and_then(|p| p.u()).map_err(bad)?;
        let role = e.get(2).and_then(|r| r.u()).map_err(bad)?;
        if profile != PROFILE_V1 || role != ROLE_CONTROLLER {
            return Err(OpenError::Malformed);
        }
        let (account, cred) =
            controller_device(accounts, &signer).ok_or(OpenError::UnknownSigner)?;
        let signature = e.get(3).and_then(|s| s.b()).map_err(bad)?;
        sig::verify(
            &cred.signing_pk,
            &ds_sig(label, PROFILE_V1, ROLE_CONTROLLER, &signer, &body),
            signature,
        )
        .map_err(|_| OpenError::BadSignature)?;
        if !seen.insert(signer) {
            return Err(OpenError::DuplicateSigner);
        }
        approvers.insert((KIND_ACCOUNT, account.head.account_id.to_vec()));
    }
    Ok(Opened {
        state,
        approvers: approvers.into_iter().collect(),
    })
}
