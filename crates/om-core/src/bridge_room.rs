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
use crate::room_snapshot::{
    self, HeadReference, MAX_ROOM_ROWS, OperationId, OperationOutcome, OperationTarget,
    RoomOperations, SnapshotError,
};
use crate::service_room_member::{
    INSTALL, MemberSnapshot, Principal, RemovalKey, RoomError, RoomServices, RosterEntry,
    TriggerFilter,
};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
struct SavedOffer {
    head: HeadReference,
    mapping: Mapping,
    approvers: Vec<Principal>,
    name: String,
    external_claim: Option<String>,
}

impl From<&Offer> for SavedOffer {
    fn from(offer: &Offer) -> Self {
        Self {
            head: (&offer.head).into(),
            mapping: offer.mapping.clone(),
            approvers: offer.approvers.clone(),
            name: offer.name.clone(),
            external_claim: offer.external_claim.clone(),
        }
    }
}

impl SavedOffer {
    fn into_offer(self) -> Offer {
        Offer {
            head: self.head.into_state(),
            mapping: self.mapping,
            approvers: self.approvers,
            name: self.name,
            external_claim: self.external_claim,
        }
    }
}

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
    operations: RoomOperations,
    offers: BTreeMap<[u8; 32], Offer>,
    removed: BTreeMap<[u8; 32], [u8; 32]>,
    retired_incarnations: BTreeSet<([u8; 32], [u8; 16])>,
    superseded_targets: BTreeSet<[u8; 32]>,
    admitted_offers: BTreeMap<[u8; 32], Offer>,
    members: RoomServices,
    mappings: BTreeMap<[u8; 32], Mapping>,
    observed_at: BTreeMap<[u8; 32], u64>,
    freshness_bound: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedBridgeRoom {
    policy: (Option<usize>, u64),
    freshness_bound: u64,
    offers: Vec<SavedOffer>,
    members: Vec<(MemberSnapshot, Mapping, Option<SavedOffer>)>,
    removed: Vec<([u8; 32], [u8; 32])>,
    retired_incarnations: BTreeSet<([u8; 32], [u8; 16])>,
    superseded_targets: BTreeSet<[u8; 32]>,
    committed_removals: BTreeSet<RemovalKey>,
}

impl SavedBridgeRoom {
    fn within_limits(&self) -> bool {
        self.offers.len() <= MAX_OFFERS
            && [
                self.members.len(),
                self.removed.len(),
                self.retired_incarnations.len(),
                self.superseded_targets.len(),
                self.committed_removals.len(),
            ]
            .into_iter()
            .all(|len| len <= MAX_ROOM_ROWS)
    }
}

fn valid_mapping(mapping: &Mapping) -> bool {
    matches!(mapping.direction, TO_FOREIGN | TO_OM | BOTH)
        && !mapping.allowed_classes.is_empty()
        && [&mapping.namespace, &mapping.foreign_scope]
            .iter()
            .all(|text| {
                !text.trim().is_empty() && clean_text(text, CLAIM_MAX).as_str() == text.as_str()
            })
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
    pub fn operation(&self, id: OperationId, target: OperationTarget) -> Option<OperationOutcome> {
        self.operations.lookup(id, target)
    }

    /// Record the exact terminal result alongside the reducer transition.
    pub fn apply_operation(
        &mut self,
        id: Option<OperationId>,
        target: OperationTarget,
        apply: impl FnOnce(&mut Self) -> OperationOutcome,
    ) -> OperationOutcome {
        if let Some(outcome) = id.and_then(|id| self.operation(id, target)) {
            return outcome;
        }
        let outcome = apply(self);
        if let Some(id) = id {
            self.operations.record(id, target, outcome.clone());
        }
        outcome
    }

    pub fn new(quorum: Option<usize>, sequencing_timeout: u64, freshness_bound: u64) -> Self {
        Self {
            operations: RoomOperations::default(),
            offers: BTreeMap::new(),
            removed: BTreeMap::new(),
            retired_incarnations: BTreeSet::new(),
            superseded_targets: BTreeSet::new(),
            admitted_offers: BTreeMap::new(),
            members: RoomServices::new(quorum, sequencing_timeout),
            mappings: BTreeMap::new(),
            observed_at: BTreeMap::new(),
            freshness_bound,
        }
    }

    pub fn export_snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        let snapshot = SavedBridgeRoom {
            policy: self.members.policy(),
            freshness_bound: self.freshness_bound,
            offers: self.offers.values().map(SavedOffer::from).collect(),
            members: self
                .members
                .snapshot_members()
                .into_iter()
                .filter_map(|entry| {
                    let id = entry.roster.service;
                    Some((
                        entry,
                        self.mappings.get(&id)?.clone(),
                        self.admitted_offers.get(&id).map(SavedOffer::from),
                    ))
                })
                .collect(),
            removed: self
                .removed
                .iter()
                .map(|(id, target)| (*id, *target))
                .collect(),
            retired_incarnations: self.retired_incarnations.clone(),
            superseded_targets: self.superseded_targets.clone(),
            committed_removals: self.members.committed_removals().clone(),
        };
        if !snapshot.within_limits() {
            return Err(SnapshotError::Limit);
        }
        room_snapshot::encode(snapshot, &self.operations)
    }

    /// The caller verifies the local record and supplies current independently verified Room head facts.
    pub fn import_snapshot(
        bytes: &[u8],
        verify: impl Fn(&HeadReference) -> bool,
        now: u64,
    ) -> Result<Self, SnapshotError> {
        let (snapshot, operations) = room_snapshot::decode::<SavedBridgeRoom>(bytes)?;
        if !snapshot.within_limits() {
            return Err(SnapshotError::Limit);
        }
        let mut room = Self::new(
            snapshot.policy.0,
            snapshot.policy.1,
            snapshot.freshness_bound,
        );
        room.operations = operations;
        room.removed.extend(snapshot.removed);
        room.retired_incarnations = snapshot.retired_incarnations;
        room.superseded_targets = snapshot.superseded_targets;
        room.members
            .restore_committed_removals(snapshot.committed_removals);
        for (member, mapping, saved) in snapshot.members {
            let id = member.roster.service;
            if room.mappings.contains_key(&id)
                || room.removed.contains_key(&id)
                || !valid_mapping(&mapping)
                || member.roster.incarnation.as_slice() != mapping.incarnation.as_slice()
                || room
                    .retired_incarnations
                    .contains(&(id, mapping.incarnation))
            {
                continue;
            }
            let mut active = mapping.clone();
            active.active = true;
            let admitted = saved
                .as_ref()
                .map_or_else(|| member.head.clone(), |o| o.head.clone());
            let offer = match saved {
                Some(saved)
                    if saved.head.id == id && saved.mapping == active && verify(&saved.head) =>
                {
                    let offer = saved.into_offer();
                    if room.propose(offer).is_err() {
                        continue;
                    }
                    room.offers.remove(&id)
                }
                Some(_) => continue,
                None => None,
            };
            if room.members.restore_member(member, &admitted, &verify, now) {
                room.mappings.insert(id, mapping);
                if let Some(offer) = offer {
                    room.admitted_offers.insert(id, offer);
                }
                // A saved runtime timestamp cannot make a mapping fresh after a restart.
            }
        }
        for saved in snapshot.offers {
            if verify(&saved.head) {
                let _ = room.propose(saved.into_offer());
            }
        }
        Ok(room)
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
        // A restored mapping has no live admission basis, even after a later control observation.
        if self.observed_at.contains_key(&head.service_id)
            && self.members.held_head(&head.service_id) != previous
        {
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
        if !valid_mapping(&offer.mapping) {
            return Err(OfferError::Malformed);
        }
        if !self.offers.contains_key(&id) && self.offers.len() >= MAX_OFFERS {
            return Err(OfferError::TooMany);
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

    pub fn has_runtime_observation(&self, bridge: &[u8; 32]) -> bool {
        self.observed_at.contains_key(bridge)
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
