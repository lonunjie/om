// SPDX-License-Identifier: MPL-2.0
//! Bridge admission and Room state (`bridges` slice 3; INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md section 6,
//! ADR-0124, ADR-0128 rules 1 and 2, ADR-0260, ADR-0264). Pure; MLS Add/Remove and signatures are the caller's.
//!
//! A bridge is always a full Room member, so it reuses the service member path ([`RoomServices`]): installation needs the
//! protected capability `room.add_bridge` (never `room.install_service`) plus human quorum, the roster says it can read
//! everything, and a head that revokes its endpoint stops it and lets any member commit removal after
//! `sequencing_timeout`. On top of that the Room keeps a mapping per bridge and the host decides every export here:
//!
//! * only classes in the mapping's `allowed_event_classes` leave, and only in a direction that exports;
//! * a mapping that is revoked, whose Room observed its bridge head longer ago than `freshness_bound`, or whose endpoint
//!   is stopped, fails closed;
//! * an imported event's author kind must be the bridge principal (3), never an account (1) or a service (2).

use crate::bridge_control::{BridgeState, human_quorum};
use crate::service_room_member::{
    INSTALL, Principal, RoomError, RoomServices, RosterEntry, TriggerFilter,
};
use std::collections::{BTreeMap, BTreeSet};

pub const ADD_BRIDGE: &str = "room.add_bridge";

/// Mapping directions (`bridge-direction`): OM to foreign, foreign to OM, both.
pub const TO_FOREIGN: u64 = 1;
pub const TO_OM: u64 = 2;
pub const BOTH: u64 = 3;

/// Author kinds on events (`Principal` key 0).
pub const KIND_ACCOUNT: u64 = 1;
pub const KIND_SERVICE: u64 = 2;
pub const KIND_BRIDGE: u64 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum BridgeRoomError {
    Room(RoomError),
    NoMapping,
    BadDirection,
    EmptyClasses,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Blocked {
    NoMapping,
    Revoked,
    /// The mapping does not export in this direction.
    Direction,
    /// The class is not in `allowed_event_classes`.
    ClassNotAllowed,
    /// The Room has not observed the bridge's control head within `freshness_bound`.
    StaleMapping,
    /// A head revoked or replaced the endpoint and its removal is pending.
    EndpointStopped,
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub incarnation: [u8; 16],
    pub namespace: String,
    pub direction: u64,
    pub allowed_classes: BTreeSet<u8>,
    pub active: bool,
}

pub struct BridgeRoom {
    members: RoomServices,
    mappings: BTreeMap<[u8; 32], Mapping>,
    endpoints: BTreeMap<[u8; 32], BTreeSet<Vec<u8>>>,
    observed_at: BTreeMap<[u8; 32], u64>,
    freshness_bound: u64,
}

fn as_install(caps: &BTreeSet<String>) -> BTreeSet<String> {
    // The member path checks one protected capability; for a bridge that is `room.add_bridge`.
    if caps.contains(ADD_BRIDGE) {
        [INSTALL.to_string()].into()
    } else {
        BTreeSet::new()
    }
}

impl BridgeRoom {
    pub fn new(quorum: Option<usize>, sequencing_timeout: u64, freshness_bound: u64) -> Self {
        Self {
            members: RoomServices::new(quorum, sequencing_timeout),
            mappings: BTreeMap::new(),
            endpoints: BTreeMap::new(),
            observed_at: BTreeMap::new(),
            freshness_bound,
        }
    }

    /// Admits a bridge as a full member and records its mapping.
    pub fn admit(
        &mut self,
        caps: &BTreeSet<String>,
        head: &BridgeState,
        mapping: Mapping,
        approvers: &[Principal],
        now: u64,
    ) -> Result<(), BridgeRoomError> {
        if !matches!(mapping.direction, TO_FOREIGN | TO_OM | BOTH) {
            return Err(BridgeRoomError::BadDirection);
        }
        if mapping.allowed_classes.is_empty() {
            return Err(BridgeRoomError::EmptyClasses);
        }
        // Neither service (2) nor bridge (3) principals, nor any endpoint, count toward a human quorum.
        let eps: BTreeSet<Vec<u8>> = head.endpoints.keys().cloned().collect();
        let approvers = human_quorum(approvers, &eps);
        self.members
            .install(
                &as_install(caps),
                head,
                mapping.incarnation.to_vec(),
                None::<TriggerFilter>,
                &approvers,
            )
            .map_err(BridgeRoomError::Room)?;
        self.endpoints.insert(
            head.service_id,
            self.members
                .entry(&head.service_id)
                .map(|e| e.endpoints.clone())
                .unwrap_or_default(),
        );
        self.observed_at.insert(head.service_id, now);
        self.mappings.insert(head.service_id, mapping);
        Ok(())
    }

    /// The Room validated a newer bridge control head.
    pub fn observe_head(&mut self, head: &BridgeState, now: u64) -> Result<(), BridgeRoomError> {
        self.members
            .observe_head(head, now)
            .map_err(BridgeRoomError::Room)?;
        self.observed_at.insert(head.service_id, now);
        Ok(())
    }

    /// Removal through Room control and MLS: the mapping ends with the membership.
    pub fn remove(
        &mut self,
        caps: &BTreeSet<String>,
        bridge: &[u8; 32],
    ) -> Result<Vec<Vec<u8>>, BridgeRoomError> {
        let gone = self
            .members
            .remove_installation(&as_install(caps), bridge)
            .map_err(BridgeRoomError::Room)?;
        self.mappings.remove(bridge);
        self.endpoints.remove(bridge);
        self.observed_at.remove(bridge);
        Ok(gone)
    }

    /// A Room control transition that revokes the mapping without removing the bridge (export stops at once).
    pub fn revoke_mapping(
        &mut self,
        caps: &BTreeSet<String>,
        bridge: &[u8; 32],
    ) -> Result<(), BridgeRoomError> {
        if !caps.contains(ADD_BRIDGE) {
            return Err(BridgeRoomError::Room(RoomError::NotAuthorized));
        }
        self.mappings
            .get_mut(bridge)
            .ok_or(BridgeRoomError::NoMapping)?
            .active = false;
        Ok(())
    }

    /// The host's export decision for one event class through `endpoint`.
    pub fn may_export(
        &self,
        bridge: &[u8; 32],
        endpoint: &[u8],
        class: u8,
        now: u64,
    ) -> Result<(), Blocked> {
        let m = self.mappings.get(bridge).ok_or(Blocked::NoMapping)?;
        if !m.active {
            return Err(Blocked::Revoked);
        }
        if !matches!(m.direction, TO_FOREIGN | BOTH) {
            return Err(Blocked::Direction);
        }
        if self.members.must_stop_sending(bridge, endpoint) {
            return Err(Blocked::EndpointStopped);
        }
        let seen = self.observed_at.get(bridge).copied().unwrap_or(0);
        if now > seen.saturating_add(self.freshness_bound) {
            return Err(Blocked::StaleMapping);
        }
        if !m.allowed_classes.contains(&class) {
            return Err(Blocked::ClassNotAllowed);
        }
        Ok(())
    }

    pub fn members(&self) -> &RoomServices {
        &self.members
    }
    pub fn members_mut(&mut self) -> &mut RoomServices {
        &mut self.members
    }
    pub fn roster(&self, bridge: &[u8; 32]) -> Option<&RosterEntry> {
        self.members.entry(bridge)
    }
}

/// An imported event is authored by a bridge principal only. Account and service kinds are refused, so a bridge cannot be
/// hidden behind an account or a service author.
pub fn import_author_ok(kind: u64) -> bool {
    kind == KIND_BRIDGE
}
