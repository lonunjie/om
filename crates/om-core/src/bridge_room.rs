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
use sha2::{Digest, Sha256};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub incarnation: [u8; 16],
    pub namespace: String,
    pub foreign_scope: String,
    pub direction: u64,
    pub allowed_classes: BTreeSet<u8>,
    pub active: bool,
}

/// A proposed bridge (ADR-0270): host-supplied observation, never authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub head: BridgeState,
    pub mapping: Mapping,
    /// Reported at proposal time (ADR-0269 item 3); never proof of a quorum.
    pub approvers: Vec<Principal>,
    pub name: String,
    pub external_claim: Option<String>,
}

/// The membership a Room shows for one bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Membership {
    Proposed,
    Member,
    Removed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OfferError {
    Malformed,
    TooMany,
    /// The target is not the current offer or member.
    StaleTarget,
    Room(BridgeRoomError),
}

pub const MAX_OFFERS: usize = 32;
const NAME_MAX: usize = 64;
const CLAIM_MAX: usize = 160;

/// Bounded display text: no control characters or bidirectional overrides, at most `max` characters.
pub fn clean_text(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .take(max)
        .collect()
}

/// The opaque review target of one exact bridge, mapping incarnation and mapping content.
pub fn review_target(bridge: &[u8; 32], m: &Mapping) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"om.bridge.review-target.v1");
    h.update(bridge);
    h.update(m.incarnation);
    h.update((m.namespace.len() as u64).to_be_bytes());
    h.update(m.namespace.as_bytes());
    h.update((m.foreign_scope.len() as u64).to_be_bytes());
    h.update(m.foreign_scope.as_bytes());
    h.update(m.direction.to_be_bytes());
    for c in &m.allowed_classes {
        h.update([*c]);
    }
    h.finalize().into()
}

pub struct BridgeRoom {
    offers: BTreeMap<[u8; 32], Offer>,
    removed: BTreeMap<[u8; 32], [u8; 32]>,
    retired_incarnations: BTreeSet<([u8; 32], [u8; 16])>,
    superseded_targets: BTreeSet<[u8; 32]>,
    admitted_offers: BTreeMap<[u8; 32], Offer>,
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
            offers: BTreeMap::new(),
            removed: BTreeMap::new(),
            retired_incarnations: BTreeSet::new(),
            superseded_targets: BTreeSet::new(),
            admitted_offers: BTreeMap::new(),
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
        if self
            .retired_incarnations
            .contains(&(head.service_id, mapping.incarnation))
        {
            return Err(BridgeRoomError::Room(RoomError::Rollback));
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
        let previous = self.members.held_head(&head.service_id);
        self.members
            .observe_head(head, now)
            .map_err(BridgeRoomError::Room)?;
        if self.members.held_head(&head.service_id) != previous {
            self.observed_at.insert(head.service_id, now);
        }
        Ok(())
    }

    /// Places a proposed bridge in the Room (ADR-0270). A new offer replaces an earlier one for the same bridge.
    pub fn propose(&mut self, mut offer: Offer) -> Result<[u8; 32], OfferError> {
        let id = offer.head.service_id;
        if self.mappings.contains_key(&id)
            || self
                .retired_incarnations
                .contains(&(id, offer.mapping.incarnation))
        {
            return Err(OfferError::Malformed);
        }
        if !matches!(offer.mapping.direction, TO_FOREIGN | TO_OM | BOTH)
            || offer.mapping.allowed_classes.is_empty()
        {
            return Err(OfferError::Malformed);
        }
        if !self.offers.contains_key(&id) && self.offers.len() >= MAX_OFFERS {
            return Err(OfferError::TooMany);
        }
        if [&offer.mapping.namespace, &offer.mapping.foreign_scope]
            .iter()
            .any(|text| {
                text.trim().is_empty() || clean_text(text, CLAIM_MAX).as_str() != text.as_str()
            })
        {
            return Err(OfferError::Malformed);
        }
        offer.name = clean_text(&offer.name, NAME_MAX);
        offer.external_claim = offer
            .external_claim
            .map(|c| clean_text(&c, CLAIM_MAX))
            .filter(|c| !c.is_empty());
        offer.mapping.active = true;
        let t = review_target(&id, &offer.mapping);
        if self.superseded_targets.contains(&t) {
            return Err(OfferError::StaleTarget);
        }
        if let Some(old) = self.offers.get(&id) {
            let old_target = review_target(&id, &old.mapping);
            if old_target == t {
                return if *old == offer {
                    Ok(t)
                } else {
                    Err(OfferError::StaleTarget)
                };
            }
            self.superseded_targets.insert(old_target);
        }
        self.offers.insert(id, offer);
        Ok(t)
    }

    pub fn withdraw_offer(&mut self, bridge: &[u8; 32]) -> bool {
        let Some(offer) = self.offers.remove(bridge) else {
            return false;
        };
        self.superseded_targets
            .insert(review_target(bridge, &offer.mapping));
        true
    }

    pub fn offer(&self, bridge: &[u8; 32]) -> Option<&Offer> {
        self.offers
            .get(bridge)
            .or_else(|| self.admitted_offers.get(bridge))
    }

    /// Every bridge the Room can show, in id order, with its membership.
    pub fn listing(&self) -> Vec<([u8; 32], Membership)> {
        let mut ids: BTreeMap<[u8; 32], Membership> = BTreeMap::new();
        for b in self.removed.keys() {
            ids.insert(*b, Membership::Removed);
        }
        for b in self.offers.keys() {
            ids.insert(*b, Membership::Proposed);
        }
        for b in self.mappings.keys() {
            ids.insert(*b, Membership::Member);
        }
        ids.into_iter().collect()
    }

    /// The target a reviewer would approve now for this bridge, whatever its membership.
    pub fn current_target(&self, bridge: &[u8; 32]) -> Option<[u8; 32]> {
        if let Some(m) = self.mappings.get(bridge) {
            return Some(review_target(bridge, m));
        }
        if let Some(o) = self.offers.get(bridge) {
            return Some(review_target(bridge, &o.mapping));
        }
        self.removed.get(bridge).copied()
    }

    /// Admits the offer named by `target`; refuses a target that is not the current offer.
    pub fn admit_offer(
        &mut self,
        caps: &BTreeSet<String>,
        bridge: &[u8; 32],
        target: &[u8; 32],
        now: u64,
    ) -> Result<(), OfferError> {
        let o = self.offers.get(bridge).ok_or(OfferError::StaleTarget)?;
        if review_target(bridge, &o.mapping) != *target {
            return Err(OfferError::StaleTarget);
        }
        let o = o.clone();
        // Reported approvers are display evidence only.
        self.admit(caps, &o.head, o.mapping.clone(), &[], now)
            .map_err(OfferError::Room)?;
        self.offers.remove(bridge);
        self.admitted_offers.insert(*bridge, o);
        self.removed.remove(bridge);
        Ok(())
    }

    /// Removes the member named by `target`; the Room keeps a tombstone with its last target.
    pub fn remove_member(
        &mut self,
        caps: &BTreeSet<String>,
        bridge: &[u8; 32],
        target: &[u8; 32],
    ) -> Result<Vec<Vec<u8>>, OfferError> {
        let m = self.mappings.get(bridge).ok_or(OfferError::StaleTarget)?;
        let t = review_target(bridge, m);
        if t != *target {
            return Err(OfferError::StaleTarget);
        }
        let gone = self.remove(caps, bridge).map_err(OfferError::Room)?;
        self.removed.insert(*bridge, t);
        Ok(gone)
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
        if let Some(mapping) = self.mappings.remove(bridge) {
            self.retired_incarnations
                .insert((*bridge, mapping.incarnation));
        }
        self.admitted_offers.remove(bridge);
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
        if !self
            .observed_at
            .get(bridge)
            .and_then(|seen| now.checked_sub(*seen))
            .is_some_and(|age| self.is_fresh(age))
        {
            return Err(Blocked::StaleMapping);
        }
        if !m.allowed_classes.contains(&class) {
            return Err(Blocked::ClassNotAllowed);
        }
        Ok(())
    }

    pub fn is_fresh(&self, age: u64) -> bool {
        age <= self.freshness_bound
    }

    pub fn members(&self) -> &RoomServices {
        &self.members
    }
    pub fn members_mut(&mut self) -> &mut RoomServices {
        &mut self.members
    }
    /// The mapping recorded when the bridge was admitted, with its current activity.
    pub fn mapping(&self, bridge: &[u8; 32]) -> Option<&Mapping> {
        self.mappings.get(bridge)
    }
    /// Every admitted bridge, in id order.
    pub fn bridge_ids(&self) -> Vec<[u8; 32]> {
        self.mappings.keys().copied().collect()
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
