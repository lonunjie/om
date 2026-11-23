// SPDX-License-Identifier: MPL-2.0
//! Automation and loop safety (`services_bots` slice 6; SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md "Automation rules" and
//! "Bot-to-bot / agent-to-agent loop safety"). Pure: the caller passes the time and the lineage it read from the event.
//!
//! * A rule fires only on event classes inside its installation's grant (trigger visibility never exceeds the grant) and
//!   ignores service-originated events unless the rule explicitly allows them.
//! * Lineage carries `hop_count`; output at or beyond the hop limit stops, so two services mentioning each other end.
//! * A rate budget per rule per window bounds output regardless of triggers.

use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_HOP_LIMIT: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lineage {
    pub root_invocation: [u8; 16],
    pub parent_invocation: Option<[u8; 16]>,
    pub origin_is_service: bool,
    pub hop_count: u32,
}

impl Lineage {
    /// The lineage a service's output carries.
    pub fn child(&self, invocation: [u8; 16]) -> Lineage {
        Lineage {
            root_invocation: self.root_invocation,
            parent_invocation: Some(invocation),
            origin_is_service: true,
            hop_count: self.hop_count + 1,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    /// The event class is outside the installation grant or the rule's trigger types.
    NotInScope,
    /// Service-originated and the rule does not opt in.
    ServiceOrigin,
    HopLimit,
    RateBudget,
}

#[derive(Debug)]
pub struct Rule {
    pub enabled: bool,
    /// Event classes the installation grant lets this service see.
    pub granted_classes: BTreeSet<u8>,
    pub trigger_types: BTreeSet<u8>,
    pub allow_service_origin: bool,
    pub hop_limit: u32,
    pub max_per_window: u32,
    pub window: u64,
    fired: BTreeMap<u64, u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RuleError {
    /// A trigger type outside the installation grant.
    ExceedsGrant,
    ZeroBudget,
}

impl Rule {
    pub fn new(
        granted_classes: BTreeSet<u8>,
        trigger_types: BTreeSet<u8>,
        allow_service_origin: bool,
        max_per_window: u32,
        window: u64,
    ) -> Result<Self, RuleError> {
        if !trigger_types.is_subset(&granted_classes) {
            return Err(RuleError::ExceedsGrant);
        }
        if max_per_window == 0 || window == 0 {
            return Err(RuleError::ZeroBudget);
        }
        Ok(Self {
            enabled: true,
            granted_classes,
            trigger_types,
            allow_service_origin,
            hop_limit: DEFAULT_HOP_LIMIT,
            max_per_window,
            window,
            fired: BTreeMap::new(),
        })
    }

    /// Decides whether the rule may act on an event of `class` with `lineage` at `now`, spending budget when it may.
    pub fn trigger(&mut self, class: u8, lineage: &Lineage, now: u64) -> Result<Lineage, Skip> {
        if !self.enabled {
            return Err(Skip::Disabled);
        }
        if !self.granted_classes.contains(&class) || !self.trigger_types.contains(&class) {
            return Err(Skip::NotInScope);
        }
        if lineage.origin_is_service && !self.allow_service_origin {
            return Err(Skip::ServiceOrigin);
        }
        if lineage.hop_count + 1 > self.hop_limit {
            return Err(Skip::HopLimit);
        }
        let bucket = now / self.window;
        self.fired.retain(|b, _| *b + 1 >= bucket);
        let n = self.fired.entry(bucket).or_insert(0);
        if *n >= self.max_per_window {
            return Err(Skip::RateBudget);
        }
        *n += 1;
        Ok(lineage.child([0; 16]))
    }
}
