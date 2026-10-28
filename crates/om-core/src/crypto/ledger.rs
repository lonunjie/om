// SPDX-License-Identifier: MPL-2.0
//! Exact last-resort PQXDH replay ledger (ADR-0131 rule 2).
//!
//! * one exact set of accepted `handshake_id`s per signed-prekey epoch of this device (the device is the
//!   namespace: each Device owns its own ledger);
//! * not a Bloom filter, no eviction: at `LAST_RESORT_ACCEPT_LIMIT` accepted initiations the epoch fails
//!   closed (`LastResortExhausted`) until the device publishes a fresh signed-prekey epoch;
//! * an epoch's set is dropped only together with that epoch's signed-prekey secrets (store.rs
//!   `expire_previous`), after which no initiation for that epoch can be accepted at all;
//! * durability is the caller's job: store.rs `Device::receive` commits the whole device state, ledger
//!   included, to the vault BEFORE it releases plaintext.

use crate::crypto::Error;
use crate::crypto::cbor::V;
use std::collections::{BTreeMap, BTreeSet};

pub const LAST_RESORT_ACCEPT_LIMIT: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastResortLedger {
    pub epochs: BTreeMap<u64, BTreeSet<[u8; 32]>>,
}

impl LastResortLedger {
    /// Would `id` be admissible for `epoch`? Checked before any expensive work.
    pub fn check(&self, epoch: u64, id: &[u8; 32]) -> Result<(), Error> {
        match self.epochs.get(&epoch) {
            Some(s) if s.contains(id) => Err(Error::ReplayedLastResortHandshake),
            Some(s) if s.len() >= LAST_RESORT_ACCEPT_LIMIT => Err(Error::LastResortExhausted),
            _ => Ok(()),
        }
    }

    /// Record an accepted initiation (after `check` and after the initial AEAD authenticated).
    pub fn record(&mut self, epoch: u64, id: [u8; 32]) {
        self.epochs.entry(epoch).or_default().insert(id);
    }

    pub fn count(&self, epoch: u64) -> usize {
        self.epochs.get(&epoch).map(|s| s.len()).unwrap_or(0)
    }

    pub fn drop_epoch(&mut self, epoch: u64) {
        self.epochs.remove(&epoch);
    }

    pub fn to_cbor(&self) -> V {
        V::A(
            self.epochs
                .iter()
                .map(|(e, s)| {
                    V::A(vec![
                        V::U(*e),
                        V::A(s.iter().map(|x| V::B(x.to_vec())).collect()),
                    ])
                })
                .collect(),
        )
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let mut epochs = BTreeMap::new();
        for e in v.a()? {
            let a = e.a()?;
            if a.len() != 2 {
                return Err(Error::Decode("ledger entry"));
            }
            let mut s = BTreeSet::new();
            let ids = a[1].a()?;
            if ids.len() > LAST_RESORT_ACCEPT_LIMIT {
                return Err(Error::Decode("ledger over limit"));
            }
            for x in ids {
                if !s.insert(x.b32()?) {
                    return Err(Error::Decode("ledger duplicate"));
                }
            }
            if epochs.insert(a[0].u()?, s).is_some() {
                return Err(Error::Decode("ledger duplicate epoch"));
            }
        }
        Ok(LastResortLedger { epochs })
    }
}
