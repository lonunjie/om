// SPDX-License-Identifier: MPL-2.0
//! Service control engine (services_bots slice 1, ADR-0262, SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md sections
//! 3, 11 and 13, ADR-0129 rules on endpoint revocation). Pure state: no clock, no I/O, no signing.
//!
//! * [`ServiceState`] is a decoded `ServiceControlState` (through the W4 registry). The hash is `ds_hash` under
//!   `OM-SERVICE-CONTROL-HASH-v1` over the body bytes.
//! * [`ServiceChain`] holds the accepted states of one ServiceID. A state is approved by a threshold of the PRIOR state's
//!   distinct controllers (genesis by its own declared controllers). The caller verifies the envelope signatures and passes
//!   the verified signer principals as `approvers`.
//! * No self-bootstrap: neither the controller set nor the approvers may name the service itself or any of its
//!   endpoints, so a service can never create or advance its own root control.
//! * Endpoint state is monotone: a revoked endpoint stays in the state as a tombstone, a quarantined endpoint cannot
//!   return to active under the same ID (a new endpoint ID is the new incarnation), and nothing active or quarantined may
//!   vanish without being revoked first. A host move ([`ServiceState::migrate_endpoint`]) is one new state that revokes the
//!   old endpoint and adds the new one, so there is never a window with neither or both.
//! * Rollback and replay are refused. A second authority-changing successor of one head freezes privileged advancement;
//!   a reducing successor (it only revokes or quarantines) is always accepted, so a fork can never stop a revocation.
//! * [`InstallationTable`] keeps `ServiceInstallation` chains by their incarnation ID. A revocation is terminal for that
//!   ID only; a later reinstall has a fresh ID, so a stale removal can never remove it.
//!
//! The caller must verify envelope signatures and the representation of approvers first. Authorship, grants and the
//! broker are later slices.

use crate::crypto::sig::ds_hash;
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_CONTROL_HASH: &str = "OM-SERVICE-CONTROL-HASH-v1";
pub const LBL_INSTALLATION_HASH: &str = "OM-SERVICE-INSTALLATION-HASH-v1";

pub type Principal = (u64, Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
}

pub const ACTIVE: u64 = 1;
pub const QUARANTINED: u64 = 2;
pub const REVOKED: u64 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub id: Vec<u8>,
    pub state: u64,
    pub profile: u64,
    pub key: Vec<u8>,
    pub mode: u64,
    pub host_ref: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceState {
    pub service_id: [u8; 32],
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub controllers: BTreeSet<Principal>,
    pub threshold: u64,
    pub endpoints: BTreeMap<Vec<u8>, Endpoint>,
    pub hash: [u8; 32],
}

fn digest32(v: Option<&Value>) -> Option<[u8; 32]> {
    match v?.get(1)? {
        Value::Bytes(b) => b.as_slice().try_into().ok(),
        _ => None,
    }
}
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

impl ServiceState {
    /// Decodes a `ServiceControlState` body (the bytes inside the signed envelope) through the W4 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("ServiceControlState", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let service_id: [u8; 32] = bytes(v.get(2))
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
                mode: uint(e.get(4)).ok_or_else(bad)?,
                host_ref: e.get(5).and_then(|h| bytes(Some(h))),
            };
            if endpoints.insert(ep.id.clone(), ep).is_some() {
                return Err(bad());
            }
        }
        Ok(ServiceState {
            service_id,
            version: uint(v.get(3)).ok_or_else(bad)?,
            prev: v
                .get(4)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            controllers,
            threshold: uint(v.get(6)).ok_or_else(bad)?,
            endpoints,
            hash: ds_hash(LBL_CONTROL_HASH, body),
        })
    }

    /// Whether `p` names the service itself or one of its endpoints or hosts (self-bootstrap, never a controller).
    fn is_self(&self, p: &Principal) -> bool {
        p.1 == self.service_id
            || self.endpoints.values().any(|e| {
                e.id == p.1 || e.key == p.1 || e.host_ref.as_deref() == Some(p.1.as_slice())
            })
    }

    /// True when this state only revokes or quarantines endpoints relative to `prev`: it adds nothing, keeps controllers
    /// and threshold, and no endpoint gains authority. A reducing state is never blocked by a fork.
    pub fn reduces(&self, prev: &ServiceState) -> bool {
        self.controllers == prev.controllers
            && self.threshold == prev.threshold
            && self.endpoints.len() == prev.endpoints.len()
            && self.endpoints.iter().all(|(id, e)| {
                prev.endpoints
                    .get(id)
                    .is_some_and(|p| e.state >= p.state && same_credential(e, p))
            })
            && self
                .endpoints
                .iter()
                .any(|(id, e)| prev.endpoints[id].state != e.state)
    }

    /// The next state of a host move: `old` becomes revoked and `new` (a fresh endpoint ID, active) is added, in one
    /// version. The caller encodes, signs and offers it like any other state; this only builds the value-level change.
    pub fn migrate_endpoint(
        &self,
        old: &[u8],
        new: Endpoint,
    ) -> Result<ServiceState, ControlError> {
        let o = self
            .endpoints
            .get(old)
            .ok_or(ControlError::UnknownEndpoint)?;
        if o.state == REVOKED {
            return Err(ControlError::EndpointResurrected);
        }
        if self.endpoints.contains_key(&new.id) || new.state != ACTIVE {
            return Err(ControlError::EndpointResurrected);
        }
        let mut next = self.clone();
        next.endpoints.get_mut(old).unwrap().state = REVOKED;
        next.endpoints.insert(new.id.clone(), new);
        next.version += 1;
        next.prev = Some(self.hash);
        Ok(next)
    }
}

fn same_credential(a: &Endpoint, b: &Endpoint) -> bool {
    a.key == b.key && a.profile == b.profile && a.mode == b.mode && a.host_ref == b.host_ref
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    WrongService,
    /// A genesis state offered when a head exists, or a non-genesis state without a `prev`.
    MissingPrev,
    /// `prev` is not an accepted state.
    UnknownPrev,
    /// The version is not exactly one above its predecessor.
    BadVersion,
    /// Fewer distinct controllers of the approving state signed than its threshold.
    Threshold,
    /// The service, one of its endpoints or its host appears as a controller or approver.
    SelfBootstrap,
    /// An older or already superseded state (rollback or replay).
    Rollback,
    /// Privileged (non-reducing) advancement is frozen by two competing authority-changing successors.
    Frozen,
    /// A revoked endpoint was dropped, reactivated or reused, or a quarantined one reopened under the same ID.
    EndpointResurrected,
    /// An active or quarantined endpoint disappeared without being revoked.
    EndpointDeleted,
    UnknownEndpoint,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Accepted,
    Duplicate,
    /// Accepted as a second authority-changing head: privileged advancement is now frozen.
    Forked,
}

pub struct ServiceChain {
    id: [u8; 32],
    known: BTreeMap<[u8; 32], ServiceState>,
    tips: BTreeSet<[u8; 32]>,
    frozen: bool,
}

impl ServiceChain {
    pub fn new(service_id: [u8; 32]) -> Self {
        Self {
            id: service_id,
            known: BTreeMap::new(),
            tips: BTreeSet::new(),
            frozen: false,
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// The single head, or `None` before genesis or while forked.
    pub fn head(&self) -> Option<&ServiceState> {
        if self.tips.len() == 1 {
            self.known.get(self.tips.iter().next()?)
        } else {
            None
        }
    }

    /// Whether any accepted state revokes `endpoint`: revocation is monotone across every branch.
    pub fn is_revoked(&self, endpoint: &[u8]) -> bool {
        self.known.values().any(|s| {
            s.endpoints
                .get(endpoint)
                .is_some_and(|e| e.state == REVOKED)
        })
    }

    /// An endpoint may act only while it is active in every tip and revoked in none.
    pub fn endpoint_usable(&self, endpoint: &[u8]) -> bool {
        !self.tips.is_empty()
            && !self.is_revoked(endpoint)
            && self.tips.iter().all(|t| {
                self.known[t]
                    .endpoints
                    .get(endpoint)
                    .is_some_and(|e| e.state == ACTIVE)
            })
    }

    pub fn accept(
        &mut self,
        st: ServiceState,
        approvers: &[Principal],
    ) -> Result<ControlOutcome, ControlError> {
        if st.service_id != self.id {
            return Err(ControlError::WrongService);
        }
        if self.known.contains_key(&st.hash) {
            return if self.tips.contains(&st.hash) {
                Ok(ControlOutcome::Duplicate)
            } else {
                Err(ControlError::Rollback)
            };
        }
        if st.controllers.iter().any(|c| st.is_self(c)) || approvers.iter().any(|a| st.is_self(a)) {
            return Err(ControlError::SelfBootstrap);
        }
        let Some(prev_hash) = st.prev else {
            if !self.known.is_empty() || st.version != 1 {
                return Err(ControlError::MissingPrev);
            }
            let signed: BTreeSet<&Principal> = approvers
                .iter()
                .filter(|a| st.controllers.contains(a))
                .collect();
            if (signed.len() as u64) < st.threshold {
                return Err(ControlError::Threshold);
            }
            self.tips.insert(st.hash);
            self.known.insert(st.hash, st);
            return Ok(ControlOutcome::Accepted);
        };
        let prev = self
            .known
            .get(&prev_hash)
            .ok_or(ControlError::UnknownPrev)?;
        if st.version != prev.version + 1 {
            return Err(ControlError::BadVersion);
        }
        let signed: BTreeSet<&Principal> = approvers
            .iter()
            .filter(|a| prev.controllers.contains(a))
            .collect();
        if (signed.len() as u64) < prev.threshold {
            return Err(ControlError::Threshold);
        }
        endpoint_rules(prev, &st)?;
        let reducing = st.reduces(prev);
        let competing = !self.tips.contains(&prev_hash);
        if !reducing && (self.frozen || competing) {
            if self.frozen && !competing {
                return Err(ControlError::Frozen);
            }
            self.frozen = true;
            self.tips.insert(st.hash);
            self.known.insert(st.hash, st);
            return Ok(ControlOutcome::Forked);
        }
        if !competing {
            self.tips.remove(&prev_hash);
        }
        self.tips.insert(st.hash);
        self.known.insert(st.hash, st);
        Ok(ControlOutcome::Accepted)
    }
}

/// Endpoint transitions are monotone between a state and its successor.
fn endpoint_rules(prev: &ServiceState, next: &ServiceState) -> Result<(), ControlError> {
    for (id, old) in &prev.endpoints {
        match next.endpoints.get(id) {
            None if old.state == REVOKED => return Err(ControlError::EndpointResurrected),
            None => return Err(ControlError::EndpointDeleted),
            Some(e) if old.state == REVOKED && e != old => {
                return Err(ControlError::EndpointResurrected);
            }
            Some(e) if old.state == QUARANTINED && e.state == ACTIVE => {
                return Err(ControlError::EndpointResurrected);
            }
            Some(e) if e.state < old.state && old.state != ACTIVE => {
                return Err(ControlError::EndpointResurrected);
            }
            _ => {}
        }
    }
    Ok(())
}

/// A decoded `ServiceInstallation`, reduced to what the table needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub incarnation: Vec<u8>,
    pub service_id: [u8; 32],
    pub scope: (u64, Vec<u8>),
    pub mode: u64,
    pub revoked: bool,
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub hash: [u8; 32],
}

impl Installation {
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("ServiceInstallation", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let scope = v.get(4).ok_or_else(bad)?;
        Ok(Installation {
            incarnation: bytes(v.get(2)).ok_or_else(bad)?,
            service_id: bytes(v.get(3))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            scope: (
                uint(scope.get(0)).ok_or_else(bad)?,
                bytes(scope.get(1)).ok_or_else(bad)?,
            ),
            mode: uint(v.get(6)).ok_or_else(bad)?,
            revoked: uint(v.get(11)).ok_or_else(bad)? == 2,
            version: uint(v.get(14)).ok_or_else(bad)?,
            prev: v
                .get(15)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            hash: ds_hash(LBL_INSTALLATION_HASH, body),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    MissingPrev,
    UnknownPrev,
    BadVersion,
    /// The incarnation's service, scope or data mode changed after genesis (an update is a new incarnation).
    Changed,
    /// A revoked incarnation was offered as active again.
    Resurrected,
    Rollback,
}

#[derive(Default)]
pub struct InstallationTable {
    by_id: BTreeMap<Vec<u8>, Vec<Installation>>,
}

impl InstallationTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts one verified installation record. Revocation is terminal for its incarnation ID only.
    pub fn accept(&mut self, ins: Installation) -> Result<(), InstallError> {
        let chain = self.by_id.entry(ins.incarnation.clone()).or_default();
        if chain.iter().any(|c| c.hash == ins.hash) {
            return Ok(());
        }
        match ins.prev {
            None => {
                if !chain.is_empty() || ins.version != 1 {
                    return Err(InstallError::MissingPrev);
                }
            }
            Some(p) => {
                let last = chain.last().ok_or(InstallError::UnknownPrev)?;
                if last.hash != p {
                    return Err(if chain.iter().any(|c| c.hash == p) {
                        InstallError::Rollback
                    } else {
                        InstallError::UnknownPrev
                    });
                }
                if ins.version != last.version + 1 {
                    return Err(InstallError::BadVersion);
                }
                if (&ins.service_id, &ins.scope, ins.mode)
                    != (&last.service_id, &last.scope, last.mode)
                {
                    return Err(InstallError::Changed);
                }
                if last.revoked && !ins.revoked {
                    return Err(InstallError::Resurrected);
                }
            }
        }
        chain.push(ins);
        Ok(())
    }

    /// The live installations of `service_id` in `scope`: latest record of each incarnation, not revoked.
    pub fn active(&self, service_id: &[u8; 32], scope: &(u64, Vec<u8>)) -> Vec<&Installation> {
        self.by_id
            .values()
            .filter_map(|c| c.last())
            .filter(|i| !i.revoked && &i.service_id == service_id && &i.scope == scope)
            .collect()
    }
}
