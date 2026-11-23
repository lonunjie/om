// SPDX-License-Identifier: MPL-2.0
//! Bridge control and typed authorship (bridges slice 1, ADR-0264, ADR-0124, ADR-0128, ADR-0129,
//! INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md). Pure state: no clock, no I/O, no signing.
//!
//! A `BridgeControlState` has the same authority shape as a service's: a hash chain approved by a threshold of the PRIOR
//! controllers, no self-bootstrap, monotone endpoint tombstones, atomic host migration, and a fork freeze in which a
//! reducing state is always accepted. The chain logic is therefore the one in [`crate::service_control`]; this module
//! decodes a bridge state into that shape, hashed under `OM-BRIDGE-CONTROL-HASH-v1`, so the two can never drift.
//!
//! * [`BridgeState::from_body`] decodes through the W4 registry (key 14 and the bridge objects exist only there).
//! * [`validate_bridge_event`] checks that an event whose author is a BridgePrincipal (kind 3) is sent by an endpoint
//!   that is active in the bridge's control chain, carries foreign provenance (key 14), and is never also a service
//!   representation (key 13). A bridge cannot produce an event that reads as an account's or a service's.
//! * [`human_quorum`] drops bridge and service principals and every known endpoint from a human or account quorum.
//!
//! The caller verifies envelope signatures and the Room's admission (`room.add_bridge`, MLS roster) first. Provenance
//! trace and loop rules against a Room mapping are slice 2.

use crate::crypto::sig::ds_hash;
use crate::service_control::{ACTIVE, Endpoint, Principal, ServiceChain, ServiceState, WireError};
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_CONTROL_HASH: &str = "OM-BRIDGE-CONTROL-HASH-v1";

/// The chain of one BridgeID (the same engine as a service chain).
pub type BridgeChain = ServiceChain;
/// A decoded bridge control state (the service shape; `service_id` is the BridgeID, endpoint `mode` is always 0).
pub type BridgeState = ServiceState;

fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(n)) = v {
        Some(*n)
    } else {
        None
    }
}
fn bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    if let Some(Value::Bytes(b)) = v {
        Some(b.clone())
    } else {
        None
    }
}

/// Decodes a `BridgeControlState` body (the bytes inside the signed envelope).
pub fn state_from_body(body: &[u8]) -> Result<BridgeState, WireError> {
    let v = Registry::load_w4()
        .check_body("BridgeControlState", body)
        .map_err(|_| WireError::Malformed)?;
    let bad = || WireError::Malformed;
    let bridge_id: [u8; 32] = bytes(v.get(2))
        .ok_or_else(bad)?
        .try_into()
        .map_err(|_| bad())?;
    let Some(Value::Array(cs)) = v.get(5) else {
        return Err(bad());
    };
    let controllers = cs
        .iter()
        .map(|c| Some((uint(c.get(0))?, bytes(c.get(1))?)))
        .collect::<Option<BTreeSet<_>>>()
        .ok_or_else(bad)?;
    let Some(Value::Array(es)) = v.get(7) else {
        return Err(bad());
    };
    let mut endpoints = BTreeMap::new();
    for e in es {
        let ep = Endpoint {
            id: bytes(e.get(0)).ok_or_else(bad)?,
            state: uint(e.get(1)).ok_or_else(bad)?,
            profile: uint(e.get(2)).ok_or_else(bad)?,
            key: bytes(e.get(3)).ok_or_else(bad)?,
            mode: 0,
            host_ref: e.get(4).and_then(|h| bytes(Some(h))),
        };
        if endpoints.insert(ep.id.clone(), ep).is_some() {
            return Err(bad());
        }
    }
    let prev = match v.get(4) {
        None => None,
        Some(d) => match d.get(1) {
            Some(Value::Bytes(b)) => Some(b.as_slice().try_into().map_err(|_| bad())?),
            _ => return Err(bad()),
        },
    };
    Ok(ServiceState {
        service_id: bridge_id,
        version: uint(v.get(3)).ok_or_else(bad)?,
        prev,
        controllers,
        threshold: uint(v.get(6)).ok_or_else(bad)?,
        endpoints,
        hash: ds_hash(LBL_CONTROL_HASH, body),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum BridgeAuthorError {
    Malformed,
    /// The event's author is not a BridgePrincipal (kind 3).
    NotBridgeAuthor,
    /// A BridgePrincipal author without foreign provenance (key 14), or a service representation alongside it.
    ProvenanceRule,
    /// The sending endpoint is not active under the bridge's control state (unknown, quarantined or revoked), or the
    /// chain is for a different BridgeID.
    EndpointNotUsable,
}

/// A validated bridge-authored event. There is no way to build one without the provenance and endpoint checks.
#[derive(Debug, PartialEq, Eq)]
pub struct BridgeAuthorship {
    pub bridge: [u8; 32],
    pub endpoint: Vec<u8>,
}

impl BridgeAuthorship {
    /// Model fact: a bridge-authored event is never an account's or a service's, and is always shown as bridged.
    pub fn bridged(&self) -> bool {
        true
    }
}

/// Validates the authorship of one bridge-authored event. `event` is the canonical `ConversationEvent` body, `endpoint`
/// the endpoint whose key carried it (verified by the caller). Room admission and mapping checks are the caller's.
pub fn validate_bridge_event(
    event: &[u8],
    endpoint: &[u8],
    chain: &BridgeChain,
) -> Result<BridgeAuthorship, BridgeAuthorError> {
    let v = Registry::load_w4()
        .check("ConversationEvent", event, false)
        .map_err(|_| BridgeAuthorError::Malformed)?;
    let author = v.get(5).ok_or(BridgeAuthorError::Malformed)?;
    if uint(author.get(0)) != Some(3) {
        return Err(BridgeAuthorError::NotBridgeAuthor);
    }
    let bridge: [u8; 32] = bytes(author.get(1))
        .and_then(|b| b.try_into().ok())
        .ok_or(BridgeAuthorError::Malformed)?;
    if v.get(14).is_none() || v.get(13).is_some() {
        return Err(BridgeAuthorError::ProvenanceRule);
    }
    if !chain.endpoint_usable(endpoint)
        || chain.head().is_none_or(|h| h.service_id != bridge)
        || chain
            .head()
            .and_then(|h| h.endpoints.get(endpoint))
            .is_none_or(|e| e.state != ACTIVE)
    {
        return Err(BridgeAuthorError::EndpointNotUsable);
    }
    Ok(BridgeAuthorship {
        bridge,
        endpoint: endpoint.to_vec(),
    })
}

/// The principals that count toward a human or account quorum: bridge (kind 3) and service (kind 2) principals and every
/// known bridge or service endpoint never count, so a full MLS member that is a bridge cannot become a governing human.
pub fn human_quorum(approvers: &[Principal], endpoint_ids: &BTreeSet<Vec<u8>>) -> Vec<Principal> {
    approvers
        .iter()
        .filter(|(kind, id)| !matches!(kind, 2 | 3) && !endpoint_ids.contains(id))
        .cloned()
        .collect()
}
