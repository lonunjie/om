// SPDX-License-Identifier: MPL-2.0
//! Room moderation state (`communities_publications` slice 5; SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md
//! "Moderation state is not role theater", DECENTRALIZED_ABUSE_MODERATION.md section 8, ADR-0122 rules 4 and 10,
//! ADR-0128 rule 12, "Immutable reduction incarnation").
//!
//! Pure reducers over already-authenticated, already-ordered control transitions; the caller supplies the actor's
//! capabilities at the transition's epoch. Rules enforced here:
//!
//! * Nobody moderates themselves; a governance principal is only touched with `governance_authority` (a governance
//!   transition, decided by the caller), never by an ordinary moderator.
//! * A ban or restriction ends only through a committed lift, never by a clock: `expires_at` on a decision is advisory
//!   and nothing here reads one.
//! * Every ban and restriction is an instance with its own immutable incarnation id. A lift or remove targeting an
//!   incarnation that is already gone is an idempotent no-op and never touches a later re-created instance with the same
//!   principal.
//! * A [`Decision`] is a record for export and appeal. Storing it changes nothing in the Room, and its public form drops
//!   reporter identity and evidence (C-16).

use std::collections::{BTreeMap, BTreeSet};

pub type Principal = Vec<u8>;
pub type Incarnation = [u8; 16];

pub const REMOVE: &str = "room.remove_member";
pub const BAN: &str = "room.ban_member";
pub const TIMEOUT: &str = "room.timeout_member";
pub const RESTRICT: &str = "room.restrict_member";
pub const REDACT: &str = "room.redact_others";

#[derive(Debug, PartialEq, Eq)]
pub enum ModError {
    NotAuthorized,
    /// The actor targeted themselves.
    SelfAction,
    /// The target is a governance principal and the transition is not a governance one.
    GovernanceProtected,
    /// The target is banned and may not be admitted.
    Banned,
    /// `ban`, `timeout` or `restrict` for an id already in use.
    Duplicate,
    UnknownMember,
}

/// What a lift or remove did.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Applied,
    /// The target instance was already gone or is a different incarnation: no state change.
    NoOp,
}

#[derive(Clone, Debug, Default)]
pub struct RoomModeration {
    members: BTreeMap<Principal, Incarnation>,
    governance: BTreeSet<Principal>,
    bans: BTreeMap<Principal, Incarnation>,
    timeouts: BTreeMap<Principal, Incarnation>,
    restrictions: BTreeMap<(Principal, u64), Incarnation>,
    redacted: BTreeSet<[u8; 32]>,
}

fn has(caps: &BTreeSet<String>, c: &str) -> bool {
    caps.contains(c)
}

impl RoomModeration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a member instance. `governance` marks a governance principal.
    pub fn admit(
        &mut self,
        who: Principal,
        incarnation: Incarnation,
        governance: bool,
    ) -> Result<(), ModError> {
        if self.bans.contains_key(&who) {
            return Err(ModError::Banned);
        }
        if governance {
            self.governance.insert(who.clone());
        }
        self.members.insert(who, incarnation);
        Ok(())
    }

    fn guard(
        &self,
        caps: &BTreeSet<String>,
        cap: &str,
        actor: &[u8],
        target: &[u8],
        governance_authority: bool,
    ) -> Result<(), ModError> {
        if !has(caps, cap) {
            return Err(ModError::NotAuthorized);
        }
        if actor == target {
            return Err(ModError::SelfAction);
        }
        if self.governance.contains(target) && !governance_authority {
            return Err(ModError::GovernanceProtected);
        }
        Ok(())
    }

    /// `membership.remove`. Names the exact member incarnation; a stale incarnation is a no-op.
    pub fn remove(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        target_incarnation: Incarnation,
        governance_authority: bool,
    ) -> Result<Effect, ModError> {
        self.guard(caps, REMOVE, actor, target, governance_authority)?;
        match self.members.get(target) {
            Some(i) if *i == target_incarnation => {
                self.members.remove(target);
                self.timeouts.remove(target);
                self.restrictions.retain(|(p, _), _| p != target);
                Ok(Effect::Applied)
            }
            _ => Ok(Effect::NoOp),
        }
    }

    /// `membership.ban`: bars admission until a committed lift and removes a present member.
    pub fn ban(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        new_incarnation: Incarnation,
        governance_authority: bool,
    ) -> Result<(), ModError> {
        self.guard(caps, BAN, actor, target, governance_authority)?;
        if self.bans.contains_key(target) {
            return Err(ModError::Duplicate);
        }
        self.bans.insert(target.to_vec(), new_incarnation);
        self.members.remove(target);
        self.timeouts.remove(target);
        self.restrictions.retain(|(p, _), _| p != target);
        Ok(())
    }

    /// `control.member.unban`, the only way a ban ends. The actor may not lift their own ban.
    pub fn unban(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        ban_incarnation: Incarnation,
    ) -> Result<Effect, ModError> {
        self.guard(caps, BAN, actor, target, true)?;
        match self.bans.get(target) {
            Some(i) if *i == ban_incarnation => {
                self.bans.remove(target);
                Ok(Effect::Applied)
            }
            _ => Ok(Effect::NoOp),
        }
    }

    /// `control.restriction.timeout`.
    pub fn timeout(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        new_incarnation: Incarnation,
        governance_authority: bool,
    ) -> Result<(), ModError> {
        self.guard(caps, TIMEOUT, actor, target, governance_authority)?;
        if !self.members.contains_key(target) {
            return Err(ModError::UnknownMember);
        }
        if self.timeouts.contains_key(target) {
            return Err(ModError::Duplicate);
        }
        self.timeouts.insert(target.to_vec(), new_incarnation);
        Ok(())
    }

    /// `control.restriction.untimeout`: the only way a timeout ends.
    pub fn untimeout(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        incarnation: Incarnation,
    ) -> Result<Effect, ModError> {
        self.guard(caps, TIMEOUT, actor, target, true)?;
        match self.timeouts.get(target) {
            Some(i) if *i == incarnation => {
                self.timeouts.remove(target);
                Ok(Effect::Applied)
            }
            _ => Ok(Effect::NoOp),
        }
    }

    /// `control.restriction.set` for a restriction kind (posting, media, ...).
    pub fn restrict(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        kind: u64,
        new_incarnation: Incarnation,
        governance_authority: bool,
    ) -> Result<(), ModError> {
        self.guard(caps, RESTRICT, actor, target, governance_authority)?;
        if !self.members.contains_key(target) {
            return Err(ModError::UnknownMember);
        }
        let k = (target.to_vec(), kind);
        if self.restrictions.contains_key(&k) {
            return Err(ModError::Duplicate);
        }
        self.restrictions.insert(k, new_incarnation);
        Ok(())
    }

    /// `control.restriction.lift`.
    pub fn lift(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        target: &[u8],
        kind: u64,
        incarnation: Incarnation,
    ) -> Result<Effect, ModError> {
        self.guard(caps, RESTRICT, actor, target, true)?;
        let k = (target.to_vec(), kind);
        match self.restrictions.get(&k) {
            Some(i) if *i == incarnation => {
                self.restrictions.remove(&k);
                Ok(Effect::Applied)
            }
            _ => Ok(Effect::NoOp),
        }
    }

    /// `moderation.redact`: suppresses another principal's message in shared state. Own messages are deleted, not redacted.
    pub fn redact(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        author: &[u8],
        event: [u8; 32],
    ) -> Result<(), ModError> {
        if !has(caps, REDACT) {
            return Err(ModError::NotAuthorized);
        }
        if actor == author {
            return Err(ModError::SelfAction);
        }
        self.redacted.insert(event);
        Ok(())
    }

    pub fn is_member(&self, p: &[u8]) -> bool {
        self.members.contains_key(p)
    }
    pub fn is_banned(&self, p: &[u8]) -> bool {
        self.bans.contains_key(p)
    }
    pub fn is_timed_out(&self, p: &[u8]) -> bool {
        self.timeouts.contains_key(p)
    }
    pub fn is_restricted(&self, p: &[u8], kind: u64) -> bool {
        self.restrictions.contains_key(&(p.to_vec(), kind))
    }
    pub fn is_redacted(&self, e: &[u8; 32]) -> bool {
        self.redacted.contains(e)
    }
    /// The current ban or timeout incarnation, for a lift to name.
    pub fn ban_incarnation(&self, p: &[u8]) -> Option<Incarnation> {
        self.bans.get(p).copied()
    }
}

/// A signed ModerationDecision, kept as a record. It carries no authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub decision_id: [u8; 16],
    pub actor: Principal,
    pub action: String,
    pub subject: Principal,
    pub target_event: Option<[u8; 32]>,
    pub reason_code: Option<String>,
    /// Advisory wall-clock hints; never read by any reducer.
    pub starts_at: Option<u64>,
    pub expires_at: Option<u64>,
    /// Private: access-controlled to the moderators, never in the public form.
    pub reporter: Option<Principal>,
    pub evidence_commitment: Option<[u8; 32]>,
}

/// What an export or appeal view shows: no reporter identity and no evidence reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicDecision {
    pub decision_id: [u8; 16],
    pub action: String,
    pub subject: Principal,
    pub target_event: Option<[u8; 32]>,
    pub reason_code: Option<String>,
}

impl Decision {
    pub fn public_view(&self) -> PublicDecision {
        PublicDecision {
            decision_id: self.decision_id,
            action: self.action.clone(),
            subject: self.subject.clone(),
            target_event: self.target_event,
            reason_code: self.reason_code.clone(),
        }
    }
}

/// The decision log: appending never touches [`RoomModeration`].
#[derive(Clone, Debug, Default)]
pub struct DecisionLog(Vec<Decision>);

impl DecisionLog {
    pub fn record(&mut self, d: Decision) {
        self.0.push(d);
    }
    pub fn all(&self) -> &[Decision] {
        &self.0
    }
}
