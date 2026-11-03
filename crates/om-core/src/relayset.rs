// SPDX-License-Identifier: MPL-2.0
//! The signed, versioned, hash-chained `RelaySet` of one accepted relationship (spec/cddl/delivery.cddl;
//! MULTI_RELAY_AND_DELIVERY_RESILIENCE.md 2 and 2.1 R-211; REACHABILITY_AND_OBJECT_REPAIR.md 2; ADR-0161 section 2).
//!
//! * A record is `{0: 17, 1: 1, 2: version, ?3: previous_set_hash, 4: [1*16 relay-entry]}`, signed with role `device` under
//!   `OM-RELAYSET-SIG-v1` by one device. Its identity is `OM-RELAYSET-HASH-v1` over the canonical unsigned body.
//! * Version 1 names no parent; every later record is exactly `parent.version + 1` and names its parent's hash.
//! * A [`Lineage`] keeps every accepted record (concurrent branches are kept as availability history) and a bounded set
//!   of records whose parent has not arrived. The effective record is chosen independently of arrival order: among the
//!   records whose signer is *currently* eligible, the highest version, then the lexicographically smallest hash.
//!
//! A RelaySet is availability metadata, never identity or membership authority: nothing here decides who is a
//! contact or a device. Whether a signer is eligible is the caller's question (it needs the peer's newest ACS).

use crate::crypto::cbor::{self, V};
use crate::crypto::sig::{
    DeviceSigner, ROLE_DEVICE, digest, ds_hash, ds_sig, envelope, open_envelope, signature_entry,
};
use crate::crypto::{Error, PROFILE_V1};
use crate::route::RelayEntry;
use serde::{Deserialize, Serialize};

pub const LBL_SIG: &str = "OM-RELAYSET-SIG-v1";
pub const LBL_HASH: &str = "OM-RELAYSET-HASH-v1";
/// `RelaySet.entries`: `[1*16 relay-entry]`.
pub const MAX_ENTRIES: usize = 16;
/// Registry bound for a signed RelaySet (om-object-types.json, type 17).
pub const MAX_BYTES: usize = 32_768;
/// Resource policy, not protocol: accepted records and records waiting for a parent kept per relationship. A record
/// refused for lack of room is simply not retained; arrival order never picks a winner among those that are.
pub const MAX_NODES: usize = 128;
pub const MAX_PENDING: usize = 16;

/// One unsigned record body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub version: u64,
    pub previous: Option<[u8; 32]>,
    pub entries: Vec<RelayEntry>,
}

impl Record {
    /// `None` when the record could not be a valid `RelaySet`: version and previous hash must agree (version 1 has no
    /// parent), 1..=16 valid entries, and no capability repeated (MULTI_RELAY 3: never one capability on two routes).
    pub fn new(version: u64, previous: Option<[u8; 32]>, entries: Vec<RelayEntry>) -> Option<Self> {
        let r = Record {
            version,
            previous,
            entries,
        };
        r.is_valid().then_some(r)
    }

    pub fn is_valid(&self) -> bool {
        let shape = self.version >= 1 && (self.version == 1) == self.previous.is_none();
        let n = self.entries.len();
        (1..=MAX_ENTRIES).contains(&n)
            && shape
            && self.entries.iter().all(RelayEntry::is_valid)
            && self.entries.iter().enumerate().all(|(i, a)| {
                self.entries[i + 1..]
                    .iter()
                    .all(|b| a.capability != b.capability)
            })
    }

    pub fn body(&self) -> Vec<u8> {
        let mut m = vec![(0, V::U(17)), (1, V::U(1)), (2, V::U(self.version))];
        if let Some(p) = &self.previous {
            m.push((3, digest(p)));
        }
        m.push((4, V::A(self.entries.iter().map(RelayEntry::to_v).collect())));
        cbor::encode(&V::M(m))
    }

    /// `OM-RELAYSET-HASH-v1` over the canonical unsigned body.
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_HASH, &self.body())
    }

    pub fn from_body(body: &[u8]) -> Result<Self, Error> {
        let bad = Error::Decode("RelaySet");
        let v = cbor::decode(body)?;
        let V::M(fields) = &v else { return Err(bad) };
        if fields.iter().any(|(k, _)| !matches!(k, 0..=4)) {
            return Err(bad);
        }
        if v.get(0)?.u()? != 17 || v.get(1)?.u()? != 1 {
            return Err(bad);
        }
        let previous = match v.opt(3) {
            None => None,
            Some(d) if d.get(0)?.u()? == 1 => Some(d.get(1)?.b32()?),
            Some(_) => return Err(bad),
        };
        let entries = v
            .get(4)?
            .a()?
            .iter()
            .map(|e| RelayEntry::from_v(e).ok_or(Error::Decode("relay-entry")))
            .collect::<Result<Vec<_>, _>>()?;
        let r = Record {
            version: v.get(2)?.u()?,
            previous,
            entries,
        };
        // Canonical form: re-encoding must give the same bytes, and the record must be a valid RelaySet.
        if !r.is_valid() || r.body() != body {
            return Err(bad);
        }
        Ok(r)
    }
}

/// A `signed-RelaySet` (`{1: bstr .cbor RelaySet, 2: [signature]}`), signed by `device_id` (role `device`).
pub fn sign(signer: &DeviceSigner, device_id: &[u8; 16], rec: &Record, rnd: [u8; 32]) -> Vec<u8> {
    let body = rec.body();
    let s = signer.sign_with(
        &ds_sig(LBL_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &body),
        rnd,
    );
    envelope(
        body,
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// The device that signed `bytes`, read without verifying (to find its credential). Never trusted by itself.
pub fn claimed_signer(bytes: &[u8]) -> Option<[u8; 16]> {
    let v = cbor::decode(bytes).ok()?;
    let sigs = v.get(2).ok()?.a().ok()?;
    let [s] = sigs else { return None };
    s.get(0).ok()?.b().ok()?.try_into().ok()
}

/// Verify a signed RelaySet against the credential key the caller already trusts for `device_id`. Whether that device
/// is currently eligible (active, not quarantined) is the caller's check against the newest ACS it holds.
pub fn open(bytes: &[u8], device_id: &[u8; 16], signing_pk: &[u8]) -> Result<Record, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Decode("RelaySet size"));
    }
    let body = open_envelope(
        bytes,
        LBL_SIG,
        PROFILE_V1,
        ROLE_DEVICE,
        device_id,
        signing_pk,
    )?;
    Record::from_body(&body)
}

// ---------------------------------------------------------------------------------------------------- lineage

/// One accepted (or waiting) record: enough to validate descendants and to rebuild the effective set. The payload may
/// be compacted, but never the `(hash, version, previous)` continuity metadata (REACHABILITY 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub hash: String,
    pub version: u64,
    #[serde(default)]
    pub previous: Option<String>,
    /// Hex DeviceID of the signer. Eligibility is re-evaluated against the current ACS every time.
    pub signer: String,
    pub entries: Vec<RelayEntry>,
}

impl Node {
    pub fn from_record(rec: &Record, signer: &[u8; 16]) -> Self {
        Node {
            hash: crate::crypto::hex(&rec.hash()),
            version: rec.version,
            previous: rec.previous.as_ref().map(|p| crate::crypto::hex(p)),
            signer: crate::crypto::hex(signer),
            entries: rec.entries.clone(),
        }
    }
}

/// What offering a record to a lineage did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Newly retained (and any waiting descendants were reconsidered).
    Accepted,
    /// Already held; nothing changed.
    Duplicate,
    /// The parent has not arrived; kept and reconsidered when it does.
    Pending,
    /// Malformed continuity (version jump, genesis naming a parent) or no room: not retained.
    Rejected(&'static str),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lineage {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub pending: Vec<Node>,
}

impl Lineage {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn holds(&self, hash: &str) -> bool {
        self.nodes.iter().any(|n| n.hash == hash)
    }

    pub fn get(&self, hash: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.hash == hash)
    }

    /// Offer a verified record. Idempotent and order-independent in what it ends up holding.
    pub fn offer(&mut self, node: Node) -> Offer {
        if self.holds(&node.hash) || self.pending.iter().any(|n| n.hash == node.hash) {
            return Offer::Duplicate;
        }
        match (node.version, &node.previous) {
            (0, _) => return Offer::Rejected("version 0"),
            (1, Some(_)) => return Offer::Rejected("genesis names a parent"),
            (v, None) if v > 1 => return Offer::Rejected("successor names no parent"),
            _ => {}
        }
        let parent = node.previous.as_ref().and_then(|p| self.get(p));
        match (&node.previous, parent) {
            (Some(_), Some(p)) if p.version + 1 != node.version => Offer::Rejected("version jump"),
            (Some(_), None) => {
                if self.pending.len() >= MAX_PENDING {
                    return Offer::Rejected("too many records waiting for a parent");
                }
                self.pending.push(node);
                Offer::Pending
            }
            _ => {
                if self.nodes.len() >= MAX_NODES {
                    return Offer::Rejected("lineage full");
                }
                self.nodes.push(node);
                self.reconsider();
                Offer::Accepted
            }
        }
    }

    /// Move waiting records whose parent has arrived into the lineage (repeatedly: a chain may unlock together).
    fn reconsider(&mut self) {
        loop {
            let parent_version = |lin: &Lineage, n: &Node| {
                n.previous
                    .as_ref()
                    .and_then(|p| lin.get(p))
                    .map(|p| p.version)
            };
            // A waiting record whose parent arrived with the wrong version is a jump: drop it.
            let jumps: Vec<String> = self
                .pending
                .iter()
                .filter(|n| parent_version(self, n).is_some_and(|v| v + 1 != n.version))
                .map(|n| n.hash.clone())
                .collect();
            self.pending.retain(|n| !jumps.contains(&n.hash));
            let Some(at) = self
                .pending
                .iter()
                .position(|n| parent_version(self, n).is_some_and(|v| v + 1 == n.version))
            else {
                return;
            };
            let n = self.pending.remove(at);
            if self.nodes.len() < MAX_NODES {
                self.nodes.push(n);
            }
        }
    }

    /// The effective record: among records whose signer `eligible` accepts, highest version, then smallest hash.
    /// Independent of the order the records were offered in.
    pub fn effective(&self, eligible: impl Fn(&str) -> bool) -> Option<&Node> {
        self.nodes
            .iter()
            .filter(|n| eligible(&n.signer))
            .max_by(|a, b| a.version.cmp(&b.version).then_with(|| b.hash.cmp(&a.hash)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u8) -> RelayEntry {
        RelayEntry::new(&format!("https://h{n}.test/"), &[n; 32], 2_000_000_000).unwrap()
    }

    #[test]
    fn genesis_has_no_parent_and_successor_must() {
        assert!(Record::new(1, None, vec![entry(1)]).is_some());
        assert!(Record::new(1, Some([7; 32]), vec![entry(1)]).is_none());
        assert!(Record::new(2, None, vec![entry(1)]).is_none());
        assert!(Record::new(2, Some([7; 32]), vec![entry(1)]).is_some());
        assert!(Record::new(0, None, vec![entry(1)]).is_none());
        assert!(Record::new(1, None, vec![]).is_none());
    }

    #[test]
    fn a_capability_is_never_shared_between_two_routes() {
        let a = entry(1);
        let mut b = entry(2);
        b.capability = a.capability.clone();
        assert!(Record::new(1, None, vec![a.clone(), b]).is_none());
    }

    #[test]
    fn body_round_trips_canonically_and_hash_is_stable() {
        let r = Record::new(1, None, vec![entry(1), entry(2)]).unwrap();
        let back = Record::from_body(&r.body()).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.hash(), r.hash());
        let child = Record::new(2, Some(r.hash()), vec![entry(2)]).unwrap();
        assert_ne!(child.hash(), r.hash());
        let mut bad = r.body();
        bad.push(0);
        assert!(Record::from_body(&bad).is_err());
    }
}
