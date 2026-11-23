// SPDX-License-Identifier: MPL-2.0
//! Bilateral child links and Space authority in a Room (`communities_publications` slice 3, ADR-0122 rules 3 and 4,
//! ADR-0128 rule 11, SPACES_ROOMS_COMMUNITIES_AND_PUBLICATION.md "Child links are affiliation, not possession",
//! "Space-managed rooms" and "SpaceDelegatedIntent"). Pure state: no clock source, no I/O, no signing.
//!
//! * [`ChildLink`]: an official link exists only while BOTH the Space side and the child side record it; either side
//!   revokes and it ends. A one-sided reference is never official.
//! * [`Binding`]: what a Room accepts from one Space, modelled as an in-memory value because the Room-side wire
//!   (`space_authority_binding`, reserved in `group.cddl`) is not frozen. It names the governance whose authorship it
//!   accepts, the exact Room capabilities delegated, the `freshness_bound` and whether sub-delegated Space authority is
//!   accepted. A Space-side unlink ends it (a reduction); a new binding needs the Room's own approval and is never
//!   restored by re-linking ([`Binding::space_unlinked`] consumes it).
//! * [`validate_delegated_intent`]: any Room member may commit a `SpaceDelegatedIntent`; the Room's check is this one,
//!   never the committer's roles. It must name the binding's Space and a Space head the member holds that is fresh
//!   enough, ask only for capabilities the binding delegates, never a protected one, and never a CryptoProfile change.
//!   A reducing intent is liveness-protected by the caller (any member commits after `sequencing_timeout`).
//!
//! The caller verified the envelope signature of the intent and of the carried Space head.

use om_codec::{Registry, Value};
use std::collections::BTreeSet;

/// Capabilities the design marks protected (`om-room-capabilities.json`): never delegated by a binding unless the Room
/// grants them under its own governance, which this slice does not model.
pub const PROTECTED: &[&str] = &[
    "room.install_service",
    "room.add_bridge",
    "room.manage_security_policy",
    "room.manage_governance",
    "space.manage_governance",
    "room.set_plaintext_egress",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChildLink {
    pub space_side: bool,
    pub child_side: bool,
}

impl ChildLink {
    pub fn proposed_by_space() -> Self {
        Self {
            space_side: true,
            child_side: false,
        }
    }
    pub fn child_accepts(&mut self) {
        self.child_side = true;
    }
    pub fn space_revokes(&mut self) {
        self.space_side = false;
    }
    pub fn child_revokes(&mut self) {
        self.child_side = false;
    }
    /// Official only while both sides record it.
    pub fn is_official(&self) -> bool {
        self.space_side && self.child_side
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub space_id: [u8; 32],
    /// The Space governance (by its control-head hash at binding time is not needed; the principals) whose authorship the
    /// Room accepts.
    pub accepted_governance: BTreeSet<(u64, Vec<u8>)>,
    pub capabilities: BTreeSet<String>,
    /// Seconds a held Space head may lag behind `now` for admission checks.
    pub freshness_bound: u64,
    pub accepts_subdelegation: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BindingError {
    /// The link is not official (one-sided), so no binding may exist.
    LinkNotOfficial,
    /// The binding lists a protected capability.
    Protected,
    EmptyCapabilities,
}

impl Binding {
    /// Creates a binding over an official link. The Room's quorum approved it (the caller's check).
    pub fn new(
        link: &ChildLink,
        space_id: [u8; 32],
        accepted_governance: BTreeSet<(u64, Vec<u8>)>,
        capabilities: BTreeSet<String>,
        freshness_bound: u64,
        accepts_subdelegation: bool,
    ) -> Result<Self, BindingError> {
        if !link.is_official() {
            return Err(BindingError::LinkNotOfficial);
        }
        if capabilities.is_empty() {
            return Err(BindingError::EmptyCapabilities);
        }
        if capabilities.iter().any(|c| PROTECTED.contains(&c.as_str())) {
            return Err(BindingError::Protected);
        }
        Ok(Self {
            space_id,
            accepted_governance,
            capabilities,
            freshness_bound,
            accepts_subdelegation,
        })
    }

    /// A Space-side unlink ends the binding. The value is consumed: re-linking never restores it, a new binding needs the
    /// Room's approval again.
    pub fn space_unlinked(self) {}
}

#[derive(Debug, PartialEq, Eq)]
pub enum IntentError {
    Malformed,
    /// The intent names another Space.
    WrongSpace,
    /// The Space head it was validated against is not the one the member holds, or the member's head is older than
    /// `freshness_bound`: admission and role effects fail closed.
    StaleHead,
    /// It asks for a capability the binding does not delegate.
    OutsideBinding,
    /// It asks for a protected capability.
    Protected,
    /// A CryptoProfile change was requested: never delegable (ADR-0128 rule 8).
    CryptoProfile,
    /// The evidence was authored under a Space's own sub-delegation and the binding does not accept that.
    SubDelegated,
}

/// A validated delegated intent.
#[derive(Debug, PartialEq, Eq)]
pub struct Delegated {
    pub operation: String,
    pub reducing: bool,
    pub capabilities: BTreeSet<String>,
}

/// Validates one `SpaceDelegatedIntent` body for a Room under `binding`. `held_head` is the freshest Space head hash and
/// version the member holds and `held_head_age` its age in seconds; `evidence_via_subdelegation` is whether the signed
/// Space evidence was authored under that Space's delegation to another Space (the caller derives it from the evidence).
pub fn validate_delegated_intent(
    body: &[u8],
    binding: &Binding,
    held_head: ([u8; 32], u64),
    held_head_age: u64,
    evidence_via_subdelegation: bool,
) -> Result<Delegated, IntentError> {
    let v = Registry::load_w3()
        .check_body("SpaceDelegatedIntent", body)
        .map_err(|_| IntentError::Malformed)?;
    let bad = || IntentError::Malformed;
    let Some(Value::Bytes(space)) = v.get(2) else {
        return Err(bad());
    };
    if space.as_slice() != binding.space_id {
        return Err(IntentError::WrongSpace);
    }
    let hash = match v.get(3).and_then(|d| d.get(1)) {
        Some(Value::Bytes(b)) => b.as_slice(),
        _ => return Err(bad()),
    };
    let version = match v.get(4) {
        Some(Value::Uint(n)) => *n,
        _ => return Err(bad()),
    };
    if hash != held_head.0 || version != held_head.1 || held_head_age > binding.freshness_bound {
        return Err(IntentError::StaleHead);
    }
    let operation = match v.get(6) {
        Some(Value::Text(t)) => t.clone(),
        _ => return Err(bad()),
    };
    if operation.contains("crypto_profile") || operation.contains("cryptoprofile") {
        return Err(IntentError::CryptoProfile);
    }
    let reducing = matches!(v.get(7), Some(Value::Bool(true)));
    let capabilities: BTreeSet<String> = match v.get(12) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| {
                if let Value::Text(t) = x {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => BTreeSet::new(),
    };
    if capabilities.iter().any(|c| PROTECTED.contains(&c.as_str())) {
        return Err(IntentError::Protected);
    }
    if !capabilities.is_subset(&binding.capabilities) {
        return Err(IntentError::OutsideBinding);
    }
    if evidence_via_subdelegation && !binding.accepts_subdelegation {
        return Err(IntentError::SubDelegated);
    }
    Ok(Delegated {
        operation,
        reducing,
        capabilities,
    })
}
