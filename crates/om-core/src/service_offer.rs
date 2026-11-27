// SPDX-License-Identifier: MPL-2.0
//! Room-local offers (ADR-0272). The caller supplies verified control heads; an offer grants no authority.

use crate::bridge_room::clean_text;
use crate::service_control::{ACTIVE, ServiceState};
use crate::service_room_member::{Principal, RoomError, RoomServices, TriggerFilter};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_OFFERS: usize = 32;
const NAME_MAX: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub head: ServiceState,
    pub incarnation: Vec<u8>,
    pub capabilities: BTreeSet<String>,
    pub filter: Option<TriggerFilter>,
    /// Reported at proposal time (ADR-0269 item 3).
    pub approvers: Vec<Principal>,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Membership {
    Proposed,
    Installed,
    Removed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OfferError {
    Malformed,
    TooMany,
    StaleTarget,
    Room(RoomError),
}

/// Binds a review to a service, its installation incarnation and the exact filter and requested capabilities.
pub fn review_target(
    service: &[u8; 32],
    incarnation: &[u8],
    filter: Option<&TriggerFilter>,
    capabilities: &BTreeSet<String>,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"om.service.review-target.v1");
    h.update(service);
    h.update((incarnation.len() as u64).to_be_bytes());
    h.update(incarnation);
    match filter {
        None => h.update([0]),
        Some(filter) => {
            h.update([1]);
            h.update((filter.event_classes.len() as u64).to_be_bytes());
            for class in &filter.event_classes {
                h.update([*class]);
            }
        }
    }
    for capability in capabilities {
        h.update((capability.len() as u64).to_be_bytes());
        h.update(capability.as_bytes());
    }
    h.finalize().into()
}

impl Offer {
    fn target(&self) -> [u8; 32] {
        review_target(
            &self.head.service_id,
            &self.incarnation,
            self.filter.as_ref(),
            &self.capabilities,
        )
    }
}

enum Installation {
    Installed(Option<Box<Offer>>),
    Removed {
        incarnation: Vec<u8>,
        name: String,
        target: [u8; 32],
    },
}

pub struct ServiceRoom {
    members: RoomServices,
    offers: BTreeMap<[u8; 32], Offer>,
    installations: BTreeMap<[u8; 32], Installation>,
}

impl ServiceRoom {
    pub fn new(quorum: Option<usize>, sequencing_timeout: u64) -> Self {
        Self {
            members: RoomServices::new(quorum, sequencing_timeout),
            offers: BTreeMap::new(),
            installations: BTreeMap::new(),
        }
    }

    pub fn members(&self) -> &RoomServices {
        &self.members
    }

    pub fn members_mut(&mut self) -> &mut RoomServices {
        &mut self.members
    }

    /// Records a legacy install in the listing without assigning it a review target.
    pub fn install(
        &mut self,
        caps: &BTreeSet<String>,
        head: &ServiceState,
        incarnation: Vec<u8>,
        filter: Option<TriggerFilter>,
        approvers: &[Principal],
    ) -> Result<(), RoomError> {
        self.members
            .install(caps, head, incarnation, filter, approvers)?;
        self.offers.remove(&head.service_id);
        self.installations
            .insert(head.service_id, Installation::Installed(None));
        Ok(())
    }

    pub fn propose(&mut self, mut offer: Offer) -> Result<[u8; 32], OfferError> {
        let id = offer.head.service_id;
        // Reuse the W4 bounds for installation incarnations, capabilities and trigger classes.
        if self.members.entry(&id).is_some()
            || !(16..=32).contains(&offer.incarnation.len())
            || offer.capabilities.len() > 128
            || offer
                .capabilities
                .iter()
                .any(|cap| cap.is_empty() || cap.len() > 128 || clean_text(cap, 128) != *cap)
            || offer.filter.as_ref().is_some_and(|filter| {
                filter.event_classes.is_empty()
                    || filter
                        .event_classes
                        .iter()
                        .any(|class| !(1..=18).contains(class))
            })
            || !offer
                .head
                .endpoints
                .values()
                .any(|endpoint| endpoint.state == ACTIVE)
            || matches!(self.installations.get(&id), Some(Installation::Removed { incarnation, .. })
                if *incarnation == offer.incarnation)
        {
            return Err(OfferError::Malformed);
        }
        if !self.offers.contains_key(&id) && self.offers.len() >= MAX_OFFERS {
            return Err(OfferError::TooMany);
        }
        offer.name = clean_text(&offer.name, NAME_MAX);
        let target = offer.target();
        // Reusing a target is idempotent only for the same observation.
        if self
            .offers
            .get(&id)
            .is_some_and(|old| old.target() == target && *old != offer)
        {
            return Err(OfferError::StaleTarget);
        }
        self.offers.insert(id, offer);
        Ok(target)
    }

    pub fn withdraw_offer(&mut self, service: &[u8; 32]) -> bool {
        self.offers.remove(service).is_some()
    }

    pub fn listing(&self) -> Vec<([u8; 32], Membership)> {
        let mut listing: BTreeMap<_, _> = self
            .installations
            .iter()
            .map(|(id, installation)| {
                let membership = match installation {
                    Installation::Installed(_) => Membership::Installed,
                    Installation::Removed { .. } => Membership::Removed,
                };
                (*id, membership)
            })
            .collect();
        listing.extend(self.offers.keys().map(|id| (*id, Membership::Proposed)));
        listing.into_iter().collect()
    }

    pub fn offer(&self, service: &[u8; 32]) -> Option<&Offer> {
        self.offers
            .get(service)
            .or_else(|| match self.installations.get(service)? {
                Installation::Installed(offer) => offer.as_deref(),
                Installation::Removed { .. } => None,
            })
    }

    pub fn name(&self, service: &[u8; 32]) -> Option<&str> {
        if let Some(offer) = self.offer(service) {
            return Some(&offer.name);
        }
        match self.installations.get(service)? {
            Installation::Removed { name, .. } => Some(name),
            Installation::Installed(_) => None,
        }
    }

    pub fn current_target(&self, service: &[u8; 32]) -> Option<[u8; 32]> {
        if let Some(offer) = self.offers.get(service) {
            return Some(offer.target());
        }
        match self.installations.get(service)? {
            Installation::Installed(offer) => {
                let entry = self.members.entry(service)?;
                Some(review_target(
                    service,
                    &entry.incarnation,
                    entry.filter.as_ref(),
                    &offer.as_ref()?.capabilities,
                ))
            }
            Installation::Removed { target, .. } => Some(*target),
        }
    }

    pub fn install_offer(
        &mut self,
        caps: &BTreeSet<String>,
        service: &[u8; 32],
        target: &[u8; 32],
    ) -> Result<(), OfferError> {
        let offer = self.offers.get(service).ok_or(OfferError::StaleTarget)?;
        if offer.target() != *target {
            return Err(OfferError::StaleTarget);
        }
        // A Room requiring quorum needs verified approvals, which the offer does not carry.
        self.members
            .install(
                caps,
                &offer.head,
                offer.incarnation.clone(),
                offer.filter.clone(),
                &[],
            )
            .map_err(OfferError::Room)?;
        self.installations.insert(
            *service,
            Installation::Installed(self.offers.remove(service).map(Box::new)),
        );
        Ok(())
    }

    pub fn remove(
        &mut self,
        caps: &BTreeSet<String>,
        service: &[u8; 32],
    ) -> Result<Vec<Vec<u8>>, RoomError> {
        let target = self.current_target(service);
        let endpoints = self.members.remove_installation(caps, service)?;
        if let (Some(Installation::Installed(Some(offer))), Some(target)) =
            (self.installations.remove(service), target)
        {
            self.installations.insert(
                *service,
                Installation::Removed {
                    incarnation: offer.incarnation,
                    name: offer.name,
                    target,
                },
            );
        }
        Ok(endpoints)
    }

    pub fn remove_installation(
        &mut self,
        caps: &BTreeSet<String>,
        service: &[u8; 32],
        target: &[u8; 32],
    ) -> Result<Vec<Vec<u8>>, OfferError> {
        if self.members.entry(service).is_none() || self.current_target(service) != Some(*target) {
            return Err(OfferError::StaleTarget);
        }
        self.remove(caps, service).map_err(OfferError::Room)
    }
}
