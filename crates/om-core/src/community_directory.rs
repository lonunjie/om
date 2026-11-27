// SPDX-License-Identifier: MPL-2.0
//! Profile-local observations for the engineering host (ADR-0273). These records confer no authority.

use crate::bridge_room::clean_text;
use crate::crypto::sig::ds_hash;
use om_codec::{Value, cbor::encode};
use std::collections::BTreeMap;

pub const MAX_ENTRIES: usize = 512;
pub const MAX_OFFERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Community = 1,
    Room = 2,
    Publication = 3,
}

pub type Place = (Kind, [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uses {
    OneTime = 1,
    Limited = 2,
    Reusable = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terms {
    pub incarnation: [u8; 16],
    pub approval_required: bool,
    pub uses: Option<Uses>,
    pub expires_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub target: [u8; 32],
    pub terms: Terms,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub description: Option<String>,
    pub offer: Option<Offer>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Limit,
    Malformed,
    UnknownPlace,
    StaleTarget,
}

#[derive(Default)]
pub struct Directory(BTreeMap<Place, Entry>);

impl Directory {
    pub fn observe(
        &mut self,
        place: Place,
        name: &str,
        description: Option<&str>,
    ) -> Result<(), Error> {
        let name = clean_text(name, 64);
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::Malformed);
        }
        if self.0.len() >= MAX_ENTRIES && !self.0.contains_key(&place) {
            return Err(Error::Limit);
        }
        let description = description
            .map(|s| clean_text(s, 160))
            .filter(|s| !s.is_empty());
        let entry = self.0.entry(place).or_insert_with(|| Entry {
            name: String::new(),
            description: None,
            offer: None,
        });
        if entry.name != name || entry.description != description {
            // Display text is part of the reviewed observation. A rename needs a fresh offer.
            entry.offer = None;
        }
        entry.name = name;
        entry.description = description;
        Ok(())
    }

    pub fn observe_offer(&mut self, place: Place, terms: Terms) -> Result<[u8; 32], Error> {
        if place.0 == Kind::Publication {
            return Err(Error::Malformed);
        }
        let replacing = self
            .0
            .get(&place)
            .ok_or(Error::UnknownPlace)?
            .offer
            .is_some();
        if !replacing && self.0.values().filter(|e| e.offer.is_some()).count() >= MAX_OFFERS {
            return Err(Error::Limit);
        }
        let entry = self.0.get_mut(&place).ok_or(Error::UnknownPlace)?;
        let target = ds_hash(
            "om.community.admission-review.v1",
            &encode(&Value::Array(vec![
                Value::Uint(place.0 as u64),
                Value::Bytes(place.1.to_vec()),
                Value::Text(entry.name.clone()),
                entry.description.clone().map_or(Value::Null, Value::Text),
                Value::Bytes(terms.incarnation.to_vec()),
                Value::Bool(terms.approval_required),
                terms.uses.map_or(Value::Null, |u| Value::Uint(u as u64)),
                terms.expires_at_ms.map_or(Value::Null, Value::Uint),
            ])),
        );
        entry.offer = Some(Offer { target, terms });
        Ok(target)
    }

    pub fn withdraw_offer(&mut self, place: Place, target: [u8; 32]) -> Result<(), Error> {
        let entry = self.0.get_mut(&place).ok_or(Error::UnknownPlace)?;
        if !entry.offer.as_ref().is_some_and(|o| o.target == target) {
            return Err(Error::StaleTarget);
        }
        entry.offer = None;
        Ok(())
    }

    pub fn withdraw(&mut self, place: Place) -> bool {
        self.0.remove(&place).is_some()
    }

    pub fn list(&self) -> impl Iterator<Item = (&Place, &Entry)> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_names_and_limits_entries() {
        let mut directory = Directory::default();
        let place = |i: usize| {
            let mut id = [0; 32];
            id[..8].copy_from_slice(&(i as u64).to_be_bytes());
            (Kind::Room, id)
        };
        for i in 0..MAX_ENTRIES {
            directory
                .observe(place(i), "R\u{202e}\n", Some(&"d".repeat(161)))
                .unwrap();
        }
        let (_, entry) = directory.list().next().unwrap();
        assert_eq!(entry.name, "R");
        assert_eq!(entry.description.as_ref().unwrap().len(), 160);
        assert_eq!(
            directory.observe(place(MAX_ENTRIES), "Overflow", None),
            Err(Error::Limit)
        );
        assert!(directory.withdraw(place(0)));
        assert!(directory.observe(place(MAX_ENTRIES), "Next", None).is_ok());
    }

    #[test]
    fn an_offer_is_bound_to_its_place_and_every_reviewed_term() {
        let mut directory = Directory::default();
        let place = (Kind::Community, [1; 32]);
        let room = (Kind::Room, [1; 32]);
        let terms = Terms {
            incarnation: [3; 16],
            approval_required: true,
            uses: Some(Uses::Limited),
            expires_at_ms: Some(123),
        };
        for key in [place, room] {
            directory.observe(key, "Workshop", None).unwrap();
        }
        let first = directory.observe_offer(place, terms.clone()).unwrap();
        assert_eq!(
            directory.observe_offer(place, terms.clone()).unwrap(),
            first
        );
        assert_ne!(directory.observe_offer(room, terms.clone()).unwrap(), first);
        let next = directory
            .observe_offer(
                place,
                Terms {
                    expires_at_ms: Some(124),
                    ..terms
                },
            )
            .unwrap();
        assert_eq!(
            directory.withdraw_offer(place, first),
            Err(Error::StaleTarget)
        );
        directory.observe(place, "Renamed", None).unwrap();
        assert_eq!(
            directory.withdraw_offer(place, next),
            Err(Error::StaleTarget)
        );
    }
}
