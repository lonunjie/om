// SPDX-License-Identifier: MPL-2.0
//! Space control state engine (`communities_publications` slice 2, ADR-0160, ADR-0122, ADR-0128 rules 9 to 11,
//! ADR-0265, SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md "Space control-state convergence", "Governance" and
//! "Orphaned governance", SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md "Delegation ceiling"). Pure state: no clock,
//! no I/O, no signing. The caller has verified every envelope signature and passes the signing principals as `approvers`.
//!
//! * A [`SpaceChain`] holds the accepted `SpaceControlState`s of one SpaceID. A successor names its predecessor's hash,
//!   is exactly one version above it and is authorized by the PRIOR state, so a principal added by a transition can never
//!   help approve that same transition. The genesis state is approved by its own declared governance.
//! * Governance quorum (a threshold of the prior state's governance principals) may change anything. Without it a change
//!   is ordinary: governance principals, threshold and delegation policy must stay as they were, each kind of change
//!   needs an approver holding the matching `space.*` capability, and role changes obey the delegation ceiling:
//!   1. creating or editing a role needs every capability and `grantable_scope` of the result inside the ceiling,
//!   2. assigning a role needs the whole role inside the ceiling,
//!   3. a role's `grantable_scope` stays inside the grantor's own `grantable_scope` (rule 1 covers it).
//!      The ceiling is the union of `grantable_scope` over one approver's roles (no coalition of weaker approvers), and the
//!      protected `space.manage_governance` is never mintable without quorum. A member may always remove only themselves
//!      (self-leave needs membership alone).
//! * Rollback and replay are refused. A second authority-changing successor of one head freezes privileged advancement
//!   with no winner; an authority-reducing successor (ADR-0122 rule 1) is never a fork and stays applicable while frozen.
//! * Governance that nobody can reach is not repaired here: [`SpaceState::governance_reachable`] tells a client the
//!   object is administratively frozen for governance changes, while ordinary actions keep working.

use crate::crypto::sig::ds_hash;
use om_codec::cbor::encode;
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_CONTROL_HASH: &str = "OM-SPACE-CONTROL-HASH-v1";
pub type Principal = (u64, Vec<u8>);

pub const CAP_GOVERNANCE: &str = "space.manage_governance";
pub const CAP_ROLES: &str = "space.manage_roles";
pub const CAP_MEMBERS: &str = "space.manage_members";
pub const CAP_LINKS: &str = "space.manage_links";
pub const CAP_PROFILE: &str = "space.manage_profile";

#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Role {
    pub grants: BTreeSet<String>,
    pub grantable: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceState {
    pub space_id: [u8; 32],
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub hash: [u8; 32],
    /// Canonical bytes of the profile (compared, never interpreted here).
    pub profile: Vec<u8>,
    pub members: BTreeMap<Principal, BTreeSet<Vec<u8>>>,
    pub roles: BTreeMap<Vec<u8>, Role>,
    pub links: BTreeSet<(u64, Vec<u8>)>,
    pub governance: BTreeSet<Principal>,
    pub threshold: u64,
    pub delegation: BTreeSet<String>,
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
fn principal(v: &Value) -> Option<Principal> {
    Some((uint(v.get(0))?, bytes(v.get(1))?))
}
fn texts(v: Option<&Value>) -> Option<BTreeSet<String>> {
    match v {
        None => Some(BTreeSet::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                if let Value::Text(t) = x {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => None,
    }
}

impl SpaceState {
    /// Decodes a `SpaceControlState` body (the bytes inside the signed envelope) through the W3 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w3()
            .check_body("SpaceControlState", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let mut members = BTreeMap::new();
        let Some(Value::Array(ms)) = v.get(6) else {
            return Err(bad());
        };
        for m in ms {
            let roles = match m.get(1) {
                None => BTreeSet::new(),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|r| bytes(Some(r)))
                    .collect::<Option<BTreeSet<_>>>()
                    .ok_or_else(bad)?,
                _ => return Err(bad()),
            };
            if members
                .insert(principal(m.get(0).ok_or_else(bad)?).ok_or_else(bad)?, roles)
                .is_some()
            {
                return Err(bad());
            }
        }
        let mut roles = BTreeMap::new();
        if let Some(Value::Array(rs)) = v.get(7) {
            for r in rs {
                let id = bytes(r.get(0)).ok_or_else(bad)?;
                let role = Role {
                    grants: texts(r.get(2)).ok_or_else(bad)?,
                    grantable: texts(r.get(3)).ok_or_else(bad)?,
                };
                if roles.insert(id, role).is_some() {
                    return Err(bad());
                }
            }
        }
        let mut links = BTreeSet::new();
        if let Some(Value::Array(ls)) = v.get(8) {
            for l in ls {
                links.insert((
                    uint(l.get(0)).ok_or_else(bad)?,
                    bytes(l.get(1)).ok_or_else(bad)?,
                ));
            }
        }
        let gov = v.get(9).ok_or_else(bad)?;
        let Some(Value::Array(gp)) = gov.get(0) else {
            return Err(bad());
        };
        Ok(SpaceState {
            space_id: bytes(v.get(2))
                .and_then(|b| b.try_into().ok())
                .ok_or_else(bad)?,
            version: uint(v.get(3)).ok_or_else(bad)?,
            prev: match v.get(4) {
                None => None,
                Some(d) => Some(
                    bytes(d.get(1))
                        .and_then(|b| b.try_into().ok())
                        .ok_or_else(bad)?,
                ),
            },
            hash: ds_hash(LBL_CONTROL_HASH, body),
            profile: encode(v.get(5).ok_or_else(bad)?),
            members,
            roles,
            links,
            governance: gp
                .iter()
                .map(principal)
                .collect::<Option<BTreeSet<_>>>()
                .ok_or_else(bad)?,
            threshold: uint(gov.get(1)).ok_or_else(bad)?,
            delegation: texts(gov.get(2)).ok_or_else(bad)?,
        })
    }

    /// Capabilities `p` holds through its roles (governance principals hold none by being governance).
    pub fn caps_of(&self, p: &Principal) -> BTreeSet<&str> {
        self.members
            .get(p)
            .into_iter()
            .flatten()
            .filter_map(|r| self.roles.get(r))
            .flat_map(|r| r.grants.iter().map(String::as_str))
            .collect()
    }

    /// `p`'s delegation ceiling: the union of `grantable_scope` over its roles.
    pub fn ceiling_of(&self, p: &Principal) -> BTreeSet<&str> {
        self.members
            .get(p)
            .into_iter()
            .flatten()
            .filter_map(|r| self.roles.get(r))
            .flat_map(|r| r.grantable.iter().map(String::as_str))
            .collect()
    }

    /// Whether enough governance principals are reachable (`live`) to change governance. When not, the Space is
    /// administratively frozen for governance changes; ordinary actions and reductions continue.
    pub fn governance_reachable(&self, live: &BTreeSet<Principal>) -> bool {
        self.governance.intersection(live).count() as u64 >= self.threshold
    }

    /// Authority-reducing relative to `prev` (ADR-0122): the same profile, governance and delegation policy; no new
    /// member, role, link, role assignment or capability anywhere.
    pub fn reduces(&self, prev: &SpaceState) -> bool {
        self.profile == prev.profile
            && self.governance == prev.governance
            && self.threshold == prev.threshold
            && self.delegation == prev.delegation
            && self.links.is_subset(&prev.links)
            && self
                .members
                .iter()
                .all(|(p, rs)| prev.members.get(p).is_some_and(|old| rs.is_subset(old)))
            && self.roles.iter().all(|(id, r)| {
                prev.roles.get(id).is_some_and(|old| {
                    r.grants.is_subset(&old.grants) && r.grantable.is_subset(&old.grantable)
                })
            })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlError {
    WrongSpace,
    /// A genesis state when a head exists, or a non-genesis state without `prev`.
    MissingPrev,
    UnknownPrev,
    /// The version is not exactly one above its predecessor.
    BadVersion,
    /// Fewer prior governance principals approved than the prior threshold, for a change that needs quorum.
    Governance,
    /// An ordinary change without an approver holding the matching `space.*` capability.
    NotAuthorized,
    /// A role edit or assignment outside the approver's delegation ceiling (rules 1 to 3).
    Ceiling,
    /// `space.manage_governance` minted without governance quorum.
    Protected,
    /// An older or already superseded state (rollback or replay).
    Rollback,
    /// Privileged advancement is frozen by two competing authority-changing successors.
    Frozen,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Accepted,
    Duplicate,
    /// Accepted as a second authority-changing head: privileged advancement is now frozen.
    Forked,
}

pub struct SpaceChain {
    id: [u8; 32],
    known: BTreeMap<[u8; 32], SpaceState>,
    tips: BTreeSet<[u8; 32]>,
    frozen: bool,
}

fn quorum(prev: &SpaceState, approvers: &[Principal]) -> bool {
    let signed: BTreeSet<&Principal> = approvers
        .iter()
        .filter(|a| prev.governance.contains(a))
        .collect();
    signed.len() as u64 >= prev.threshold
}

/// Authorization of one non-quorum (ordinary) transition from `prev` to `next` by `approvers`.
fn authorize_ordinary(
    prev: &SpaceState,
    next: &SpaceState,
    approvers: &[Principal],
) -> Result<(), ControlError> {
    if prev.governance != next.governance
        || prev.threshold != next.threshold
        || prev.delegation != next.delegation
    {
        return Err(ControlError::Governance);
    }
    // Self-leave: the only change is removing an approver's own membership.
    let mut leaver = prev.clone();
    leaver.members.retain(|p, _| next.members.contains_key(p));
    let removed: Vec<&Principal> = prev
        .members
        .keys()
        .filter(|p| !next.members.contains_key(*p))
        .collect();
    let self_leave_only = !removed.is_empty()
        && removed.iter().all(|p| approvers.contains(p))
        && leaver.members == next.members
        && prev.roles == next.roles
        && prev.links == next.links
        && prev.profile == next.profile;
    if self_leave_only {
        return Ok(());
    }
    let holds = |cap: &str| {
        approvers
            .iter()
            .any(|a| prev.caps_of(a).contains(cap) && prev.members.contains_key(a))
    };
    let roles_changed = prev.roles != next.roles
        || next
            .members
            .iter()
            .any(|(p, rs)| match prev.members.get(p) {
                Some(old) => old != rs,
                None => !rs.is_empty(),
            });
    let members_changed = prev.members.keys().ne(next.members.keys());
    if (prev.profile != next.profile && !holds(CAP_PROFILE))
        || (prev.links != next.links && !holds(CAP_LINKS))
        || (members_changed && !holds(CAP_MEMBERS))
        || (roles_changed && !holds(CAP_ROLES))
    {
        return Err(ControlError::NotAuthorized);
    }
    if roles_changed {
        // the protected governance capability is never mintable by an ordinary role manager
        let mints_protected = next.roles.iter().any(|(id, r)| {
            prev.roles.get(id) != Some(r)
                && (r.grants.contains(CAP_GOVERNANCE) || r.grantable.contains(CAP_GOVERNANCE))
        });
        if mints_protected {
            return Err(ControlError::Protected);
        }
        // one approver whose own ceiling covers every created, edited and assigned role (no coalition of weaker ones)
        let covered = |a: &Principal| {
            if !prev.caps_of(a).contains(CAP_ROLES) {
                return false;
            }
            let ceil = prev.ceiling_of(a);
            let inside = |r: &Role| {
                r.grants.iter().all(|c| ceil.contains(c.as_str()))
                    && r.grantable.iter().all(|c| ceil.contains(c.as_str()))
            };
            let edited_ok = next
                .roles
                .iter()
                .filter(|(id, r)| prev.roles.get(*id) != Some(*r))
                .all(|(_, r)| inside(r));
            let assigned_ok = next.members.iter().all(|(p, rs)| {
                let old = prev.members.get(p);
                rs.iter()
                    .filter(|r| old.is_none_or(|o| !o.contains(*r)))
                    .all(|r| next.roles.get(r).is_some_and(inside))
            });
            edited_ok && assigned_ok
        };
        if !approvers.iter().any(covered) {
            return Err(ControlError::Ceiling);
        }
    }
    Ok(())
}

impl SpaceChain {
    pub fn new(space_id: [u8; 32]) -> Self {
        Self {
            id: space_id,
            known: BTreeMap::new(),
            tips: BTreeSet::new(),
            frozen: false,
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// The single head, or `None` before genesis or while forked.
    pub fn head(&self) -> Option<&SpaceState> {
        if self.tips.len() == 1 {
            self.known.get(self.tips.iter().next()?)
        } else {
            None
        }
    }

    pub fn accept(
        &mut self,
        st: SpaceState,
        approvers: &[Principal],
    ) -> Result<ControlOutcome, ControlError> {
        if st.space_id != self.id {
            return Err(ControlError::WrongSpace);
        }
        if self.known.contains_key(&st.hash) {
            return if self.tips.contains(&st.hash) {
                Ok(ControlOutcome::Duplicate)
            } else {
                Err(ControlError::Rollback)
            };
        }
        let Some(prev_hash) = st.prev else {
            if !self.known.is_empty() || st.version != 1 {
                return Err(ControlError::MissingPrev);
            }
            let own: BTreeSet<&Principal> = approvers
                .iter()
                .filter(|a| st.governance.contains(a))
                .collect();
            if (own.len() as u64) < st.threshold {
                return Err(ControlError::Governance);
            }
            self.tips.insert(st.hash);
            self.known.insert(st.hash, st);
            return Ok(ControlOutcome::Accepted);
        };
        let prev = self
            .known
            .get(&prev_hash)
            .ok_or(ControlError::UnknownPrev)?;
        if st.version != prev.version + 1 {
            return Err(ControlError::BadVersion);
        }
        if !quorum(prev, approvers) {
            authorize_ordinary(prev, &st, approvers)?;
        }
        let reducing = st.reduces(prev);
        let competing = !self.tips.contains(&prev_hash);
        if !reducing && (self.frozen || competing) {
            if self.frozen && !competing {
                return Err(ControlError::Frozen);
            }
            self.frozen = true;
            self.tips.insert(st.hash);
            self.known.insert(st.hash, st);
            return Ok(ControlOutcome::Forked);
        }
        if !competing {
            self.tips.remove(&prev_hash);
        }
        self.tips.insert(st.hash);
        self.known.insert(st.hash, st);
        Ok(ControlOutcome::Accepted)
    }
}
