// SPDX-License-Identifier: MPL-2.0
//! `RelayEntry` and `RelaySet` (spec/cddl/delivery.cddl, MULTI_RELAY_AND_DELIVERY_RESILIENCE.md section 2): where a
//! mailbox is reachable. A sealed-delivery key is bound to exactly one entry by
//! `route_binding = SHA-256(OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability])`.
//!
//! Core does not choose or derive routes. The host's provider adapter allocates them ([`RouteOffer`]: a fresh mailbox
//! id for Core to poll, plus the `relay_uri` and deposit capability the adapter will honour for it) and Core binds its
//! delivery keys to what the adapter said. The stand-in `om-core:mailbox/v0` of pass 2 survives only as a *legacy*
//! entry that state migration writes for relationships created before routes existed.
//!
//! Built here: the per-relationship entry set with one active entry. Not built: the signed, versioned RelaySet
//! lineage (`previous_set_hash`, branch convergence, signer checks), several entries per mailbox, weights and
//! transports. A set with more than one entry is refused rather than half-used.

use crate::crypto::cbor::V;
use crate::crypto::delivery::{self, CORE_ROUTE_URI};
use crate::host::hexs;
use serde::{Deserialize, Serialize};

/// `relay-entry.relay_uri`: tstr .size (1..2048). Core additionally insists on plain printable ASCII without spaces.
pub const MAX_RELAY_URI: usize = 2048;
/// `relay-entry.capability`: bstr .size (16..64).
pub const MIN_CAPABILITY: usize = 16;
pub const MAX_CAPABILITY: usize = 64;
/// Offers the host may keep queued in Core at once.
pub const MAX_OFFERS: usize = 16;

fn uri_ok(u: &str) -> bool {
    !u.is_empty() && u.len() <= MAX_RELAY_URI && u.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// One route to a mailbox (`relay-entry`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEntry {
    pub relay_uri: String,
    /// The deposit capability a sender presents at that relay. Opaque to Core.
    #[serde(with = "hexs")]
    pub capability: Vec<u8>,
    pub priority: u8,
    /// POSIX seconds; 0 for a legacy entry whose expiry was never recorded.
    pub expires_at: u64,
}

impl RelayEntry {
    /// `None` when the entry would not be a valid `relay-entry` (or uses a URI Core will not carry).
    pub fn new(relay_uri: &str, capability: &[u8], expires_at: u64) -> Option<Self> {
        let e = RelayEntry {
            relay_uri: relay_uri.to_string(),
            capability: capability.to_vec(),
            priority: 0,
            expires_at,
        };
        e.is_valid().then_some(e)
    }

    pub fn is_valid(&self) -> bool {
        uri_ok(&self.relay_uri)
            && (MIN_CAPABILITY..=MAX_CAPABILITY).contains(&self.capability.len())
    }

    /// The pass-2 stand-in for a mailbox created before routes existed (state migration only).
    pub fn legacy(mailbox: &[u8]) -> Self {
        RelayEntry {
            relay_uri: CORE_ROUTE_URI.to_string(),
            capability: mailbox.to_vec(),
            priority: 0,
            expires_at: 0,
        }
    }

    pub fn is_legacy(&self) -> bool {
        self.relay_uri == CORE_ROUTE_URI
    }

    /// What a delivery key for this route commits to.
    pub fn binding(&self) -> [u8; 32] {
        delivery::route_binding(&self.relay_uri, &self.capability)
    }

    /// The CBOR map of `relay-entry` (keys 0, 1, 2, 4; weight, transports and failure-domain hints are not used).
    pub fn to_v(&self) -> V {
        V::M(vec![
            (0, V::T(self.relay_uri.clone())),
            (1, V::B(self.capability.clone())),
            (2, V::U(u64::from(self.priority))),
            (4, V::U(self.expires_at)),
        ])
    }

    /// Reads a `relay-entry`, accepting optional fields it does not use (3, 5, 6) but not unknown keys.
    pub fn from_v(v: &V) -> Option<Self> {
        let V::M(fields) = v else { return None };
        if fields.iter().any(|(k, _)| !matches!(k, 0..=6)) {
            return None;
        }
        let e = RelayEntry {
            relay_uri: v.get(0).ok()?.t().ok()?.to_string(),
            capability: v.get(1).ok()?.b().ok()?.to_vec(),
            priority: u8::try_from(v.get(2).ok()?.u().ok()?).ok()?,
            expires_at: v.get(4).ok()?.u().ok()?,
        };
        e.is_valid().then_some(e)
    }
}

/// The routes of one mailbox for one relationship (receiving side). One active entry in this milestone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySet {
    pub entries: Vec<RelayEntry>,
}

impl RelaySet {
    pub fn single(e: RelayEntry) -> Self {
        RelaySet { entries: vec![e] }
    }

    /// A set Core can act on: exactly one valid entry.
    pub fn primary(&self) -> Option<&RelayEntry> {
        match self.entries.as_slice() {
            [e] if e.is_valid() => Some(e),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What the host's provider adapter offers for one new mailbox: the 16-byte id Core will poll, and the route peers
/// will deposit to. Offers are single use, held in memory only, and carry no key material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteOffer {
    #[serde(with = "hexs")]
    pub mailbox: Vec<u8>,
    pub relay_uri: String,
    #[serde(with = "hexs")]
    pub capability: Vec<u8>,
}

impl RouteOffer {
    pub fn is_valid(&self) -> bool {
        self.mailbox.len() == 16
            && uri_ok(&self.relay_uri)
            && (MIN_CAPABILITY..=MAX_CAPABILITY).contains(&self.capability.len())
            && self.relay_uri != CORE_ROUTE_URI
    }
}
