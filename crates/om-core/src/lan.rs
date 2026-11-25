// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` rendezvous state (ADR-0158 sections 1, 2 and 9): which contacts may be found on the same network, the
//! fixed K-tag advertisement, matching an observed tag, and retirement. Sans-IO: no clock, no randomness source and no
//! socket; the host passes `now` and a byte filler. The constructions are `crypto::lan_rendezvous`.
//!
//! * Off by default per contact; a contact is on only after an explicit [`LanRendezvous::enable`]. Under Hardened, Maximum
//!   and Custom (rank at least 1, as `PrivacyPolicyLevel::rank`) the whole facility is unavailable: nothing is advertised,
//!   nothing matches (ADR-0158 section 9; Custom is treated like Hardened because it is never a way to go below it).
//! * Exactly [`K`] tags are advertised per epoch, real tags plus random decoys, sorted so the order carries nothing. With
//!   more than K eligible real tags a deterministic window rotates with the epoch (the wire is unchanged).
//! * A matched tag shows possession of a rendezvous capability only. It never names an account or device and never creates
//!   or advances a relationship (ADR-0158 section 2, C-12).
//! * Blocking or ending a relationship calls [`LanRendezvous::retire`]: the contact's seeds are zeroized and removed.
//! * Rate limiting happens before any HMAC is computed.

use crate::api::PrivacyPolicyLevel;
use crate::crypto::lan_rendezvous::{self as lr, LanSeed, TAG_LEN, Tag};
use std::collections::BTreeMap;

/// Advertised tags per device per epoch (ADR-0158 section 1).
pub const K: usize = 8;

/// An opaque per-contact handle chosen by the host; it is never advertised.
pub type ContactRef = [u8; 16];

#[derive(Debug, PartialEq, Eq)]
pub enum LanError {
    /// The privacy level turns local delivery off.
    Unavailable,
    /// The contact has no rendezvous seed.
    UnknownContact,
}

/// Token bucket checked before any expensive work on an observed tag. `burst` tags may arrive at once; `per_second`
/// refills. These are local policy values, not wire constants.
pub struct RateLimit {
    burst: u32,
    per_second: u32,
    tokens: u32,
    last: u64,
}

impl RateLimit {
    pub fn new(burst: u32, per_second: u32) -> Self {
        Self {
            burst,
            per_second,
            tokens: burst,
            last: 0,
        }
    }
    fn allow(&mut self, now: u64) -> bool {
        let add = now
            .saturating_sub(self.last)
            .saturating_mul(self.per_second as u64);
        self.tokens = (self.tokens as u64 + add).min(self.burst as u64) as u32;
        self.last = self.last.max(now);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

struct Entry {
    enabled: bool,
    seeds: Vec<LanSeed>,
}

pub struct LanRendezvous {
    level: Option<PrivacyPolicyLevel>,
    contacts: BTreeMap<ContactRef, Entry>,
    limit: RateLimit,
}

fn ct_eq(a: &Tag, b: &Tag) -> bool {
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl LanRendezvous {
    pub fn new(level: Option<PrivacyPolicyLevel>, limit: RateLimit) -> Self {
        Self {
            level,
            contacts: BTreeMap::new(),
            limit,
        }
    }

    pub fn set_level(&mut self, level: Option<PrivacyPolicyLevel>) {
        self.level = level;
    }

    /// Replaces the contact set with the peers the person has turned on (each already has a seed), at the current privacy
    /// level, keeping the rate limiter's state. Used by the engine, which holds the choice and the seeds in the Device.
    pub fn replace_contacts(
        &mut self,
        level: Option<PrivacyPolicyLevel>,
        enabled: impl IntoIterator<Item = (ContactRef, LanSeed)>,
    ) {
        self.level = level;
        self.contacts.clear();
        for (c, seed) in enabled {
            self.contacts
                .entry(c)
                .or_insert(Entry {
                    enabled: true,
                    seeds: Vec::new(),
                })
                .seeds
                .push(seed);
        }
    }

    /// Whether local delivery may run at all under the current privacy level.
    pub fn available(&self) -> bool {
        PrivacyPolicyLevel::rank(self.level) == 0
    }

    /// Records a seed for a contact (a new device pair or an invite). The contact stays OFF until [`Self::enable`].
    pub fn add_seed(&mut self, contact: ContactRef, seed: LanSeed) {
        self.contacts
            .entry(contact)
            .or_insert(Entry {
                enabled: false,
                seeds: Vec::new(),
            })
            .seeds
            .push(seed);
    }

    /// The person's explicit per-contact choice ("Find each other on the same Wi-Fi").
    pub fn enable(&mut self, contact: &ContactRef) -> Result<(), LanError> {
        if !self.available() {
            return Err(LanError::Unavailable);
        }
        self.contacts
            .get_mut(contact)
            .ok_or(LanError::UnknownContact)?
            .enabled = true;
        Ok(())
    }

    pub fn disable(&mut self, contact: &ContactRef) {
        if let Some(e) = self.contacts.get_mut(contact) {
            e.enabled = false;
        }
    }

    pub fn is_enabled(&self, contact: &ContactRef) -> bool {
        self.available() && self.contacts.get(contact).is_some_and(|e| e.enabled)
    }

    /// Block or relationship end: deletes the contact's seeds now (they zeroize on drop). Returns whether any existed.
    pub fn retire(&mut self, contact: &ContactRef) -> bool {
        self.contacts.remove(contact).is_some()
    }

    /// The K tags to advertise in the epoch of `now`: real tags of enabled contacts (a rotating window when more than K),
    /// then random decoys, sorted. Empty when the facility is unavailable or no contact is enabled, so nothing is
    /// advertised that could reveal that the feature is on.
    pub fn advertised_tags(&self, now: u64, fill: &mut dyn FnMut(&mut [u8])) -> Vec<Tag> {
        if !self.available() {
            return Vec::new();
        }
        let epoch = lr::epoch_of(now);
        let mut real: Vec<Tag> = self
            .contacts
            .values()
            .filter(|e| e.enabled)
            .flat_map(|e| e.seeds.iter().map(|s| lr::tag(s, epoch)))
            .collect();
        if real.is_empty() {
            return Vec::new();
        }
        real.sort();
        real.dedup();
        let mut out: Vec<Tag> = if real.len() > K {
            let start = (epoch as usize).wrapping_mul(K) % real.len();
            (0..K).map(|i| real[(start + i) % real.len()]).collect()
        } else {
            real
        };
        while out.len() < K {
            let mut d = [0u8; TAG_LEN];
            fill(&mut d);
            out.push(d);
        }
        out.sort();
        out
    }

    /// Which enabled contact an observed tag belongs to, if any, accepting epochs e-1, e and e+1. Rate-limited first. A
    /// match is possession of a rendezvous capability and nothing more.
    pub fn match_tag(&mut self, now: u64, observed: &Tag) -> Option<ContactRef> {
        if !self.available() || !self.limit.allow(now) {
            return None;
        }
        let epochs: Vec<u64> = lr::accepted_epochs(now).collect();
        let mut found = None;
        for (c, e) in &self.contacts {
            if !e.enabled {
                continue;
            }
            for s in &e.seeds {
                for ep in &epochs {
                    if ct_eq(&lr::tag(s, *ep), observed) {
                        found = Some(*c);
                    }
                }
            }
        }
        found
    }
}
