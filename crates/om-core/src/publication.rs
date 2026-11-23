// SPDX-License-Identifier: MPL-2.0
//! Publication validator and subscriber feed state (`communities_publications`, ADR-0128 rule 13, ADR-0122 rules 1 and 6,
//! SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md "Publication control and signing-key continuity", cases PUB-01 to PUB-14).
//! Sans-IO and pure: it owns no clock, no network and no signature code.
//!
//! Precondition: the caller has already verified the signed envelope of every object it hands in (the signature under the
//! key the control state authorizes) and, for control transitions, which governance principals signed it (`approvers`).
//! What is decided here is validity under the chain and control rules, never by clock:
//!
//! * Events are chained per signing key by `seq` and `prev_post_hash`. A duplicate is idempotent; two different events for
//!   one `(key, seq)` are an equivocation conflict that is recorded, never a silent overwrite. A gap is held, not accepted.
//! * A revocation records the key's last valid `seq`. An event beyond it is invalid whatever its `clock` or the control
//!   state it names; one at or below it stays valid. `clock` is carried for display ordering and never read.
//! * Control states are hash-chained. A successor must name the prior accepted state and satisfy that state's governance;
//!   an older head is never accepted after a newer one (no rollback); two authority-changing successors of one head freeze
//!   privileged advancement with no winner, while authority-reducing transitions stay applicable.
//! * A tombstone is an ordinary valid event. The post view says retracted, never absent.
//! * The feed keeps no follower roster; `following` is local preference. A discussion `ConversationID` is data only: nothing
//!   here reads Room membership and no Room state grants publishing authority.

use crate::crypto::sig::ds_hash;
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_EVENT_HASH: &str = "OM-PUBLICATION-EVENT-HASH-v1";
pub const LBL_CONTROL_HASH: &str = "OM-PUBLICATION-CONTROL-HASH-v1";

pub type Principal = (u64, Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    Current,
    Revoked { last_valid_seq: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyState {
    pub key_ref: Vec<u8>,
    pub status: KeyStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlState {
    pub publication_id: [u8; 32],
    pub version: u64,
    /// Hash of the prior accepted state (`digest` key 4); `None` only for the genesis state.
    pub prev: Option<[u8; 32]>,
    /// `DS-HASH(OM-PUBLICATION-CONTROL-HASH-v1, body)`: what events name as `control_state_hash`.
    pub hash: [u8; 32],
    pub publisher: Vec<Principal>,
    pub keys: Vec<KeyState>,
    pub discussion: Option<Vec<u8>>,
    pub governance: Vec<Principal>,
    pub threshold: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Post,
    Edit,
    Tombstone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub publication_id: [u8; 32],
    pub post_id: Vec<u8>,
    /// Display ordering only. Never read by validation.
    pub clock: u64,
    pub kind: Kind,
    pub signing_key_ref: Vec<u8>,
    pub seq: u64,
    pub prev_post_hash: Option<[u8; 32]>,
    pub control_state_hash: [u8; 32],
    /// `DS-HASH(OM-PUBLICATION-EVENT-HASH-v1, body)`: what the next event of this key names as `prev_post_hash`.
    pub hash: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
}

fn digest32(v: Option<&Value>) -> Option<[u8; 32]> {
    match v?.get(1)? {
        Value::Bytes(b) => b.as_slice().try_into().ok(),
        _ => None,
    }
}
fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(n)) = v {
        Some(*n)
    } else {
        None
    }
}
fn bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    if let Some(Value::Bytes(b)) = v {
        Some(b.clone())
    } else {
        None
    }
}
fn principals(v: Option<&Value>) -> Option<Vec<Principal>> {
    let Some(Value::Array(a)) = v else {
        return None;
    };
    a.iter()
        .map(|p| Some((uint(p.get(0))?, bytes(p.get(1))?)))
        .collect()
}

impl ControlState {
    /// Decodes a `PublicationControlState` body (the bytes inside the signed envelope) through the W3 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w3()
            .check_body("PublicationControlState", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let Some(Value::Array(ks)) = v.get(7) else {
            return Err(bad());
        };
        let keys = ks
            .iter()
            .map(|k| {
                Some(KeyState {
                    key_ref: bytes(k.get(0))?,
                    status: match uint(k.get(1))? {
                        1 => KeyStatus::Current,
                        _ => KeyStatus::Revoked {
                            last_valid_seq: uint(k.get(2))?,
                        },
                    },
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(bad)?;
        let gov = v.get(9).ok_or_else(bad)?;
        Ok(ControlState {
            publication_id: bytes(v.get(2))
                .and_then(|b| b.try_into().ok())
                .ok_or_else(bad)?,
            version: uint(v.get(3)).ok_or_else(bad)?,
            prev: v
                .get(4)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            hash: ds_hash(LBL_CONTROL_HASH, body),
            publisher: principals(v.get(6)).ok_or_else(bad)?,
            keys,
            discussion: v
                .get(8)
                .map(|c| bytes(Some(c)).ok_or_else(bad))
                .transpose()?,
            governance: principals(gov.get(0)).ok_or_else(bad)?,
            threshold: uint(gov.get(1)).ok_or_else(bad)?,
        })
    }

    fn key(&self, r: &[u8]) -> Option<&KeyState> {
        self.keys.iter().find(|k| k.key_ref == r)
    }

    /// Authority-reducing relative to `prev` (ADR-0122): no new key, no revoked key back to current, no added publisher or
    /// governance principal, an unchanged or higher threshold.
    pub fn reduces(&self, prev: &ControlState) -> bool {
        self.keys.iter().all(|k| {
            prev.key(&k.key_ref).is_some_and(|p| {
                !(matches!(p.status, KeyStatus::Revoked { .. }) && k.status == KeyStatus::Current)
            })
        }) && self.publisher.iter().all(|p| prev.publisher.contains(p))
            && self.governance.iter().all(|g| prev.governance.contains(g))
            && self.threshold >= prev.threshold
    }
}

impl Event {
    /// Decodes a `PublicationEvent` body through the W3 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w3()
            .check_body("PublicationEvent", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        Ok(Event {
            publication_id: bytes(v.get(2))
                .and_then(|b| b.try_into().ok())
                .ok_or_else(bad)?,
            post_id: bytes(v.get(3)).ok_or_else(bad)?,
            clock: 0, // present on the wire for display; deliberately not carried into validation
            kind: match uint(v.get(5)) {
                Some(1) => Kind::Post,
                Some(2) => Kind::Edit,
                Some(3) => Kind::Tombstone,
                _ => return Err(bad()),
            },
            signing_key_ref: bytes(v.get(7)).ok_or_else(bad)?,
            seq: uint(v.get(8)).ok_or_else(bad)?,
            prev_post_hash: v
                .get(9)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            control_state_hash: digest32(v.get(10)).ok_or_else(bad)?,
            hash: ds_hash(LBL_EVENT_HASH, body),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlError {
    WrongPublication,
    /// A non-genesis state without a `prev`, or a genesis state when a head exists (PUB-08).
    MissingPrev,
    /// `prev` is not an accepted state (PUB-08).
    UnknownPrev,
    /// Fewer distinct governance principals of the prior state approved than its threshold (PUB-08).
    Governance,
    /// An older, already superseded state was offered (PUB-05, PUB-06).
    Rollback,
    /// Privileged advancement is frozen by two authority-changing successors (PUB-07).
    Frozen,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Accepted,
    /// Already held (idempotent).
    Duplicate,
    /// Accepted as a second authority-changing head: privileged advancement is now frozen (PUB-07).
    Forked,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EventError {
    WrongPublication,
    /// The event names a control state this feed has not accepted: fetch it first.
    UnknownControl,
    /// The signing key is not authorized by the control state the event names (PUB-09).
    KeyNotAuthorized,
    /// The key was revoked and this `seq` is beyond its last valid one (PUB-02, PUB-03).
    RevokedBeyondLastValid,
    /// `seq`/`prev_post_hash` do not continue the key's chain (PUB-01).
    BadChain,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EventOutcome {
    Accepted,
    /// Same event again (PUB-12).
    Duplicate,
    /// A different event for an already used `(key, seq)`: recorded as a conflict, nothing overwritten (PUB-12).
    Equivocation,
    /// `seq` is ahead of the chain: kept until the missing events arrive.
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostView {
    Live(Kind),
    /// "Publisher retracted this": the post existed and was withdrawn (PUB-11). No erasure promise.
    Retracted,
}

pub struct Feed {
    id: [u8; 32],
    known: BTreeMap<[u8; 32], ControlState>,
    tips: BTreeSet<[u8; 32]>,
    frozen: bool,
    chains: BTreeMap<Vec<u8>, BTreeMap<u64, [u8; 32]>>,
    held: Vec<Event>,
    posts: BTreeMap<Vec<u8>, PostView>,
    conflicts: Vec<([u8; 32], [u8; 32])>,
    /// Local preference only. Never sent to the publisher or anyone else (PUB-13).
    pub following: bool,
}

impl Feed {
    pub fn new(publication_id: [u8; 32]) -> Self {
        Feed {
            id: publication_id,
            known: BTreeMap::new(),
            tips: BTreeSet::new(),
            frozen: false,
            chains: BTreeMap::new(),
            held: Vec::new(),
            posts: BTreeMap::new(),
            conflicts: Vec::new(),
            following: false,
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// The highest accepted head (greatest `version`), unless the feed is frozen on competing heads.
    pub fn head(&self) -> Option<&ControlState> {
        if self.frozen {
            return None;
        }
        self.tips
            .iter()
            .filter_map(|h| self.known.get(h))
            .max_by_key(|c| c.version)
    }

    pub fn conflicts(&self) -> &[([u8; 32], [u8; 32])] {
        &self.conflicts
    }

    pub fn post(&self, post_id: &[u8]) -> Option<PostView> {
        self.posts.get(post_id).copied()
    }

    /// Accepts a control state. `approvers` are the principals whose signatures on it were verified.
    pub fn accept_control(
        &mut self,
        cs: ControlState,
        approvers: &[Principal],
    ) -> Result<ControlOutcome, ControlError> {
        if cs.publication_id != self.id {
            return Err(ControlError::WrongPublication);
        }
        if self.known.contains_key(&cs.hash) {
            // The current head again is idempotent; any older state is a rollback offer.
            return if self.tips.contains(&cs.hash) {
                Ok(ControlOutcome::Duplicate)
            } else {
                Err(ControlError::Rollback)
            };
        }
        let Some(prev_hash) = cs.prev else {
            if !self.known.is_empty() {
                return Err(ControlError::MissingPrev);
            }
            self.tips.insert(cs.hash);
            self.known.insert(cs.hash, cs);
            return Ok(ControlOutcome::Accepted);
        };
        let prev = self
            .known
            .get(&prev_hash)
            .ok_or(ControlError::UnknownPrev)?;
        let signed: BTreeSet<&Principal> = approvers
            .iter()
            .filter(|a| prev.governance.contains(a))
            .collect();
        if (signed.len() as u64) < prev.threshold {
            return Err(ControlError::Governance);
        }
        let reducing = cs.reduces(prev);
        let competing = !self.tips.contains(&prev_hash);
        if !reducing && (self.frozen || competing) {
            if self.frozen && !competing {
                return Err(ControlError::Frozen);
            }
            // A second authority-changing successor of one head: no winner by arrival, so privileged advancement freezes.
            self.frozen = true;
            self.tips.insert(cs.hash);
            self.known.insert(cs.hash, cs);
            return Ok(ControlOutcome::Forked);
        }
        // Normal advancement, or a reducing transition (never a fork; applicable while frozen).
        if !competing {
            self.tips.remove(&prev_hash);
        }
        self.tips.insert(cs.hash);
        self.known.insert(cs.hash, cs);
        Ok(ControlOutcome::Accepted)
    }

    /// Whether any accepted control state records `key` as revoked at a last valid `seq` below `seq`.
    fn revoked_beyond(&self, key: &[u8], seq: u64) -> bool {
        self.known.values().any(|c| {
            matches!(c.key(key), Some(KeyState { status: KeyStatus::Revoked { last_valid_seq }, .. }) if seq > *last_valid_seq)
        })
    }

    pub fn accept_event(&mut self, ev: Event) -> Result<EventOutcome, EventError> {
        if ev.publication_id != self.id {
            return Err(EventError::WrongPublication);
        }
        let named = self
            .known
            .get(&ev.control_state_hash)
            .ok_or(EventError::UnknownControl)?;
        let Some(state) = named.key(&ev.signing_key_ref) else {
            return Err(EventError::KeyNotAuthorized);
        };
        // The named state may be older than a later revocation: the highest knowledge decides, and the clock never does.
        if self.revoked_beyond(&ev.signing_key_ref, ev.seq) {
            return Err(EventError::RevokedBeyondLastValid);
        }
        if let KeyStatus::Revoked { last_valid_seq } = state.status
            && ev.seq > last_valid_seq
        {
            return Err(EventError::RevokedBeyondLastValid);
        }
        let chain = self.chains.entry(ev.signing_key_ref.clone()).or_default();
        if let Some(known) = chain.get(&ev.seq) {
            return if *known == ev.hash {
                Ok(EventOutcome::Duplicate)
            } else {
                self.conflicts.push((*known, ev.hash));
                Ok(EventOutcome::Equivocation)
            };
        }
        let next = chain.keys().next_back().map_or(0, |s| s + 1);
        if ev.seq > next {
            self.held.push(ev);
            return Ok(EventOutcome::Held);
        }
        // ev.seq < next would have been found above, so ev.seq == next here.
        let want_prev = if ev.seq == 0 {
            None
        } else {
            chain.get(&(ev.seq - 1)).copied()
        };
        if ev.prev_post_hash != want_prev {
            return Err(EventError::BadChain);
        }
        chain.insert(ev.seq, ev.hash);
        self.posts.insert(
            ev.post_id.clone(),
            match (ev.kind, self.posts.get(&ev.post_id)) {
                (Kind::Tombstone, _) | (_, Some(PostView::Retracted)) => PostView::Retracted,
                (k, _) => PostView::Live(k),
            },
        );
        // A held event may now continue the chain.
        let key = ev.signing_key_ref.clone();
        if let Some(i) = self
            .held
            .iter()
            .position(|h| h.signing_key_ref == key && h.seq == ev.seq + 1)
        {
            let h = self.held.remove(i);
            let _ = self.accept_event(h);
        }
        Ok(EventOutcome::Accepted)
    }
}
