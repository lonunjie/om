// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups, slice 1 (scope `groups`, ADR-0177): the typed `GroupControlState` model.
//!
//! What is here: typed state (object 28), intent (29), commit binding (30) and the structure of a history bundle
//! (31); the ADR-0176 section 3 cross-field validators with the frozen verdict names; the state hash chain
//! (`OM-GROUP-CONTROL-HASH-v1`); genesis creation; a pure reducer for `member.admit`, `member.remove`,
//! `member.leave`, `profile.set`, `role.define` and `role.assign`; and persistence of the head state in
//! [`crate::state::AppState`]. Nothing is sent: there is no MLS, no messaging and no delivery here, and signature
//! envelopes (a device signs each object, role `device`) arrive with the MLS slice, which owns the signer.
//!
//! Wire decoding goes through the registry-driven strict codec (`om_codec::Registry::check_body`) before any typed
//! parse, so a malformed body never reaches the validators. Capability ids and the operation-to-capability map are
//! read from `spec/registries/om-room-capabilities.json` (ADR-0128 rule 9), never hard-coded.
//!
//! Every operation not listed above is refused by the reducer with `unsupported_operation`; its validators exist.

use crate::crypto::sig::ds_hash;
use om_codec::{Value, encode};
use serde::{Deserialize, Serialize};
use serde_json::Value as J;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub const LBL_STATE_HASH: &str = "OM-GROUP-CONTROL-HASH-v1";
pub const LBL_INTENT_HASH: &str = "OM-GROUP-INTENT-HASH-v1";
pub const LBL_BINDING_HASH: &str = "OM-GROUP-COMMIT-BINDING-HASH-v1";

/// Product cap on members for the reference client (G7, ruling 2026-11-13). A policy constant, not wire: the wire
/// allows 512 (ADR-0176 F-27). Raising it needs no protocol change.
pub const PRODUCT_MEMBER_CAP: usize = 32;
const WIRE_MEMBER_CAP: usize = 512;

pub const PRINCIPAL_ACCOUNT: u64 = 1;

/// Operation ids (`group-op-id`, key == registry `control.<name>`).
pub mod op {
    pub const MEMBER_ADMIT: u64 = 1;
    pub const MEMBER_REMOVE: u64 = 2;
    pub const MEMBER_LEAVE: u64 = 3;
    pub const DEVICE_ADD: u64 = 4;
    pub const DEVICE_REMOVE_REVOKED: u64 = 5;
    pub const PROFILE_SET: u64 = 6;
    pub const DISAPPEARING_TIMER_SET: u64 = 7;
    pub const ROLE_DEFINE: u64 = 8;
    pub const ROLE_ASSIGN: u64 = 9;
    pub const INVITE_LINK_ISSUE: u64 = 10;
    pub const INVITE_LINK_REVOKE: u64 = 11;
    pub const INVITE_LINK_REDEEM: u64 = 12;
    pub const ADMISSION_SET: u64 = 13;
    pub const SECURITY_POLICY_SET: u64 = 14;
    pub const GOVERNANCE_SET: u64 = 15;
    pub const COORDINATOR_REPLACE: u64 = 16;

    pub const NAMES: [&str; 16] = [
        "member.admit",
        "member.remove",
        "member.leave",
        "device.add",
        "device.remove_revoked",
        "profile.set",
        "disappearing_timer.set",
        "role.define",
        "role.assign",
        "invite_link.issue",
        "invite_link.revoke",
        "invite_link.redeem",
        "admission.set",
        "security_policy.set",
        "governance.set",
        "coordinator.replace",
    ];
}

/// A group-layer error: the stable verdict name of the frozen vectors where one exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupError(pub &'static str);

impl core::fmt::Display for GroupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "group: {}", self.0)
    }
}
impl std::error::Error for GroupError {}

pub mod binding;
pub mod carriage;
pub mod carry;
pub mod flow;
pub mod journal;
pub mod mls;
pub mod room;
pub mod store;

#[cfg(any(test, feature = "test-loopback"))]
pub mod fake;

/// The OpenMLS engine (ADR-0187, a proposed delegated draft, not accepted). Off unless `groups-openmls` is on.
#[cfg(feature = "groups-openmls")]
pub mod openmls;

type R<T> = Result<T, GroupError>;
const BAD: GroupError = GroupError("malformed");

fn err<T>(why: &'static str) -> R<T> {
    Err(GroupError(why))
}

// ------------------------------------------------------------------ Value helpers

fn req(v: &Value, k: u64) -> R<&Value> {
    v.get(k).ok_or(BAD)
}
fn u(v: &Value) -> R<u64> {
    match v {
        Value::Uint(n) => Ok(*n),
        _ => Err(BAD),
    }
}
fn b(v: &Value) -> R<Vec<u8>> {
    match v {
        Value::Bytes(x) => Ok(x.clone()),
        _ => Err(BAD),
    }
}
fn t(v: &Value) -> R<String> {
    match v {
        Value::Text(x) => Ok(x.clone()),
        _ => Err(BAD),
    }
}
fn bl(v: &Value) -> R<bool> {
    match v {
        Value::Bool(x) => Ok(*x),
        _ => Err(BAD),
    }
}
fn a(v: &Value) -> R<&[Value]> {
    match v {
        Value::Array(x) => Ok(x),
        _ => Err(BAD),
    }
}
fn map(entries: Vec<(u64, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(k, v)| (Value::Uint(k), v))
            .collect(),
    )
}
fn opt<T>(v: &Value, k: u64, f: impl Fn(&Value) -> R<T>) -> R<Option<T>> {
    v.get(k).map(f).transpose()
}
fn bytes_set(v: &Value) -> R<Vec<Vec<u8>>> {
    a(v)?.iter().map(b).collect()
}
fn bytes_arr(x: &[Vec<u8>]) -> Value {
    Value::Array(x.iter().map(|d| Value::Bytes(d.clone())).collect())
}
fn text_set(v: &Value) -> R<Vec<String>> {
    a(v)?.iter().map(t).collect()
}
fn unique<T: Ord + Clone>(xs: &[T]) -> bool {
    xs.iter().cloned().collect::<BTreeSet<_>>().len() == xs.len()
}
/// A "set" array as stored. The codec requires sorted elements on the wire; [`GroupState::normalize`] sorts what the
/// reducer and genesis build, and a decoded state keeps the order it arrived in, so its hash stays the wire hash.
fn set_sorted(items: Vec<Value>) -> Value {
    Value::Array(items)
}

fn digest_of(v: &Value) -> R<Vec<u8>> {
    if u(req(v, 0)?)? != 1 {
        return err("digest_alg");
    }
    let h = b(req(v, 1)?)?;
    if h.len() != 32 {
        return err("digest_len");
    }
    Ok(h)
}
fn digest_value(h: &[u8]) -> Value {
    // the struct {0: hash_alg (1 = SHA-256), 1: bytes}
    map(vec![(0, Value::Uint(1)), (1, Value::Bytes(h.to_vec()))])
}

fn registry() -> &'static om_codec::Registry {
    crate::flows::registry()
}

// ------------------------------------------------------------------ capability registry

struct CapRegistry {
    protected: BTreeSet<String>,
    /// Capability ids of scope `room` and wave W1 that are not reserved (what a W1 creator may hold).
    room_w1: Vec<String>,
    /// operation key -> required entries (`room.x` ids and `basis: x` markers)
    requires: BTreeMap<String, Vec<String>>,
}

fn caps() -> &'static CapRegistry {
    static C: OnceLock<CapRegistry> = OnceLock::new();
    C.get_or_init(|| {
        let j: J = serde_json::from_str(include_str!(
            "../../../spec/registries/om-room-capabilities.json"
        ))
        .expect("om-room-capabilities.json parses");
        let mut protected = BTreeSet::new();
        let mut room_w1 = Vec::new();
        for c in j["capabilities"].as_array().expect("capabilities") {
            let id = c["id"].as_str().expect("id").to_string();
            if c["protected"].as_bool() == Some(true) {
                protected.insert(id.clone());
            }
            if c["scope"] == "room" && c["wave"] == "W1" && c.get("status").is_none() {
                room_w1.push(id);
            }
        }
        let mut requires = BTreeMap::new();
        for o in j["operations"].as_array().expect("operations") {
            let r = o["requires"]
                .as_array()
                .expect("requires")
                .iter()
                .map(|x| x.as_str().expect("requires entry").to_string())
                .collect();
            requires.insert(o["key"].as_str().expect("key").to_string(), r);
        }
        CapRegistry {
            protected,
            room_w1,
            requires,
        }
    })
}

// ------------------------------------------------------------------ typed state

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Principal {
    pub kind: u64,
    pub id: Vec<u8>,
}

impl Principal {
    pub fn account(id: Vec<u8>) -> Self {
        Principal {
            kind: PRINCIPAL_ACCOUNT,
            id,
        }
    }
    fn parse(v: &Value) -> R<Self> {
        Ok(Principal {
            kind: u(req(v, 0)?)?,
            id: b(req(v, 1)?)?,
        })
    }
    fn value(&self) -> Value {
        map(vec![
            (0, Value::Uint(self.kind)),
            (1, Value::Bytes(self.id.clone())),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub avatar_ref: Option<String>,
    pub description: Option<String>,
}

impl Profile {
    fn parse(v: &Value) -> R<Self> {
        Ok(Profile {
            name: t(req(v, 0)?)?,
            avatar_ref: opt(v, 1, t)?,
            description: opt(v, 2, t)?,
        })
    }
    fn value(&self) -> Value {
        let mut m = vec![(0, Value::Text(self.name.clone()))];
        if let Some(x) = &self.avatar_ref {
            m.push((1, Value::Text(x.clone())));
        }
        if let Some(x) = &self.description {
            m.push((2, Value::Text(x.clone())));
        }
        map(m)
    }
    fn valid(&self) -> bool {
        (1..=256).contains(&self.name.len())
            && self
                .avatar_ref
                .as_ref()
                .is_none_or(|x| (1..=2048).contains(&x.len()))
            && self
                .description
                .as_ref()
                .is_none_or(|x| (1..=4096).contains(&x.len()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub principal: Principal,
    /// Immutable membership incarnation (R-279): a removed and re-added person gets a new one.
    pub incarnation: Vec<u8>,
    pub role_ids: Vec<Vec<u8>>,
    /// 1 account_devices, 2 service_endpoints.
    pub endpoint_policy: u64,
}

impl Member {
    fn parse(v: &Value) -> R<Self> {
        Ok(Member {
            principal: Principal::parse(req(v, 0)?)?,
            incarnation: b(req(v, 1)?)?,
            role_ids: v.get(2).map(bytes_set).transpose()?.unwrap_or_default(),
            endpoint_policy: u(req(v, 3)?)?,
        })
    }
    fn value(&self) -> Value {
        let mut m = vec![
            (0, self.principal.value()),
            (1, Value::Bytes(self.incarnation.clone())),
            (3, Value::Uint(self.endpoint_policy)),
        ];
        if !self.role_ids.is_empty() {
            m.push((
                2,
                set_sorted(
                    self.role_ids
                        .iter()
                        .map(|r| Value::Bytes(r.clone()))
                        .collect(),
                ),
            ));
        }
        map(m)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleDef {
    pub id: Vec<u8>,
    pub display_name: String,
    pub grants: Vec<String>,
    pub grantable_scope: Option<Vec<String>>,
}

impl RoleDef {
    fn parse(v: &Value) -> R<Self> {
        Ok(RoleDef {
            id: b(req(v, 0)?)?,
            display_name: t(req(v, 1)?)?,
            grants: text_set(req(v, 2)?)?,
            grantable_scope: v.get(3).map(text_set).transpose()?,
        })
    }
    pub(crate) fn value(&self) -> Value {
        // a CDDL set is ordered by its encoded bytes (shorter text first); a grant appended later must land in place
        let texts = |x: &[String]| {
            let mut v: Vec<Value> = x.iter().map(|s| Value::Text(s.clone())).collect();
            v.sort_by_key(encode);
            set_sorted(v)
        };
        let mut m = vec![
            (0, Value::Bytes(self.id.clone())),
            (1, Value::Text(self.display_name.clone())),
            (2, texts(&self.grants)),
        ];
        if let Some(g) = &self.grantable_scope {
            m.push((3, texts(g)));
        }
        map(m)
    }
    fn well_formed(&self) -> bool {
        self.id.len() == 16
            && (1..=128).contains(&self.display_name.len())
            && (1..=64).contains(&self.grants.len())
            && unique(&self.grants)
            && self
                .grantable_scope
                .as_ref()
                .is_none_or(|g| (1..=64).contains(&g.len()) && unique(g))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Governance {
    pub principals: Vec<Principal>,
    pub approval_threshold: u64,
    pub protected_operations: Option<Vec<u64>>,
}

impl Governance {
    fn parse(v: &Value) -> R<Self> {
        Ok(Governance {
            principals: a(req(v, 0)?)?
                .iter()
                .map(Principal::parse)
                .collect::<R<_>>()?,
            approval_threshold: u(req(v, 1)?)?,
            protected_operations: v
                .get(2)
                .map(|x| a(x)?.iter().map(u).collect::<R<Vec<_>>>())
                .transpose()?,
        })
    }
    fn value(&self) -> Value {
        let mut m = vec![
            (
                0,
                set_sorted(self.principals.iter().map(Principal::value).collect()),
            ),
            (1, Value::Uint(self.approval_threshold)),
        ];
        if let Some(p) = &self.protected_operations {
            m.push((2, set_sorted(p.iter().map(|n| Value::Uint(*n)).collect())));
        }
        map(m)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// 1 invite_only, 2 admission by approval, 3 open.
    pub join_rule: u64,
    pub new_device_policy: u64,
    /// 1 since_join, 2 recent_window, 3 full_shared_history.
    pub history_policy: u64,
    pub history_window_seconds: Option<u64>,
    pub disappearing_timer: Option<u64>,
    pub security_profile: u64,
    pub sequencing_timeout: u64,
    pub past_epoch_window: u64,
}

impl Policy {
    fn parse(v: &Value) -> R<Self> {
        Ok(Policy {
            join_rule: u(req(v, 0)?)?,
            new_device_policy: u(req(v, 1)?)?,
            history_policy: u(req(v, 2)?)?,
            history_window_seconds: opt(v, 3, u)?,
            disappearing_timer: opt(v, 4, u)?,
            security_profile: u(req(v, 5)?)?,
            sequencing_timeout: u(req(v, 6)?)?,
            past_epoch_window: u(req(v, 7)?)?,
        })
    }
    fn value(&self) -> Value {
        let mut m = vec![
            (0, Value::Uint(self.join_rule)),
            (1, Value::Uint(self.new_device_policy)),
            (2, Value::Uint(self.history_policy)),
            (5, Value::Uint(self.security_profile)),
            (6, Value::Uint(self.sequencing_timeout)),
            (7, Value::Uint(self.past_epoch_window)),
        ];
        if let Some(x) = self.history_window_seconds {
            m.push((3, Value::Uint(x)));
        }
        if let Some(x) = self.disappearing_timer {
            m.push((4, Value::Uint(x)));
        }
        map(m)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    pub join_ref: Vec<u8>,
    pub issuer: Principal,
    pub issuer_incarnation: Vec<u8>,
    pub authorization_basis: u64,
    pub expiry: u64,
    pub max_uses: Option<u64>,
    pub uses: u64,
}

impl Invite {
    fn parse(v: &Value) -> R<Self> {
        Ok(Invite {
            join_ref: b(req(v, 0)?)?,
            issuer: Principal::parse(req(v, 1)?)?,
            issuer_incarnation: b(req(v, 2)?)?,
            authorization_basis: u(req(v, 3)?)?,
            expiry: u(req(v, 4)?)?,
            max_uses: opt(v, 5, u)?,
            uses: u(req(v, 6)?)?,
        })
    }
    fn value(&self) -> Value {
        let mut m = vec![
            (0, Value::Bytes(self.join_ref.clone())),
            (1, self.issuer.value()),
            (2, Value::Bytes(self.issuer_incarnation.clone())),
            (3, Value::Uint(self.authorization_basis)),
            (4, Value::Uint(self.expiry)),
            (6, Value::Uint(self.uses)),
        ];
        if let Some(x) = self.max_uses {
            m.push((5, Value::Uint(x)));
        }
        map(m)
    }
}

/// `GroupControlState` (object 28): the hash-chained group authority state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupState {
    pub conversation_id: Vec<u8>,
    pub state_version: u64,
    /// SHA-256 of the previous state (absent only at genesis).
    pub previous_state_hash: Option<Vec<u8>>,
    pub profile: Profile,
    pub membership: Vec<Member>,
    pub role_definitions: Vec<RoleDef>,
    pub governance: Governance,
    pub policy: Policy,
    /// Ordered, not a set: the coordinator succession order (ADR-0128 rule 1).
    pub coordinator_succession: Vec<Vec<u8>>,
    pub invites: Vec<Invite>,
    /// MLS epoch of the Commit that installed this version.
    pub mls_epoch_ref: u64,
    /// Key 23, carried unchanged (critical extensions are the codec's concern).
    pub extensions: Option<Value>,
}

impl GroupState {
    /// Decode a state body: strict codec and CDDL first, then the typed parse, then the section 3 rules.
    pub fn decode(body: &[u8]) -> R<Self> {
        registry()
            .check_body("GroupControlState", body)
            .map_err(GroupError)?;
        let v = om_codec::decode(body, om_codec::Limits::OM, 131_072).map_err(GroupError)?;
        let s = Self::from_value(&v)?;
        validate_state(&s)?;
        Ok(s)
    }

    /// Typed parse of an already-checked body value (no semantic validation).
    pub fn from_value(v: &Value) -> R<Self> {
        if u(req(v, 0)?)? != 28 {
            return err("wrong_type");
        }
        Ok(GroupState {
            conversation_id: b(req(v, 2)?)?,
            state_version: u(req(v, 3)?)?,
            previous_state_hash: opt(v, 4, digest_of)?,
            profile: Profile::parse(req(v, 5)?)?,
            membership: a(req(v, 6)?)?.iter().map(Member::parse).collect::<R<_>>()?,
            role_definitions: v
                .get(7)
                .map(|x| a(x)?.iter().map(RoleDef::parse).collect::<R<Vec<_>>>())
                .transpose()?
                .unwrap_or_default(),
            governance: Governance::parse(req(v, 8)?)?,
            policy: Policy::parse(req(v, 9)?)?,
            coordinator_succession: bytes_set(req(v, 10)?)?,
            invites: v
                .get(11)
                .map(|x| a(x)?.iter().map(Invite::parse).collect::<R<Vec<_>>>())
                .transpose()?
                .unwrap_or_default(),
            mls_epoch_ref: u(req(v, 12)?)?,
            extensions: v.get(23).cloned(),
        })
    }

    pub fn to_value(&self) -> Value {
        let mut m = vec![
            (0, Value::Uint(28)),
            (1, Value::Uint(1)),
            (2, Value::Bytes(self.conversation_id.clone())),
            (3, Value::Uint(self.state_version)),
            (5, self.profile.value()),
            (
                6,
                set_sorted(self.membership.iter().map(Member::value).collect()),
            ),
            (8, self.governance.value()),
            (9, self.policy.value()),
            (10, bytes_arr(&self.coordinator_succession)),
            (12, Value::Uint(self.mls_epoch_ref)),
        ];
        if let Some(h) = &self.previous_state_hash {
            m.push((4, digest_value(h)));
        }
        if !self.role_definitions.is_empty() {
            m.push((
                7,
                set_sorted(self.role_definitions.iter().map(RoleDef::value).collect()),
            ));
        }
        if !self.invites.is_empty() {
            m.push((
                11,
                set_sorted(self.invites.iter().map(Invite::value).collect()),
            ));
        }
        if let Some(x) = &self.extensions {
            m.push((23, x.clone()));
        }
        map(m)
    }

    /// The canonical body bytes. Sets are written sorted, so equal states encode equally.
    pub fn encode(&self) -> Vec<u8> {
        encode(&self.to_value())
    }

    /// The state hash `DS-HASH("OM-GROUP-CONTROL-HASH-v1", body)`: what `previous_state_hash` of the successor and
    /// `GroupCommitBinding.resulting_state_hash` carry.
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_STATE_HASH, &self.encode())
    }

    /// Put every "set" field in canonical order (ascending by encoded element), as the wire requires.
    pub fn normalize(&mut self) {
        for m in &mut self.membership {
            m.role_ids.sort();
        }
        self.membership.sort_by_cached_key(|m| encode(&m.value()));
        self.role_definitions.iter_mut().for_each(|r| {
            r.grants
                .sort_by_cached_key(|g| encode(&Value::Text(g.clone())));
            if let Some(g) = &mut r.grantable_scope {
                g.sort_by_cached_key(|g| encode(&Value::Text(g.clone())));
            }
        });
        self.role_definitions
            .sort_by_cached_key(|r| encode(&r.value()));
        self.governance
            .principals
            .sort_by_cached_key(|p| encode(&p.value()));
        if let Some(p) = &mut self.governance.protected_operations {
            p.sort();
        }
        self.invites.sort_by_cached_key(|i| encode(&i.value()));
    }

    pub fn member(&self, p: &Principal) -> Option<&Member> {
        self.membership.iter().find(|m| &m.principal == p)
    }

    /// The capabilities a member holds at this state: the union of the grants of its roles (ADR-0128 rule 9).
    pub fn capabilities_of(&self, p: &Principal) -> BTreeSet<String> {
        let Some(m) = self.member(p) else {
            return BTreeSet::new();
        };
        let mut out = BTreeSet::new();
        for r in &m.role_ids {
            if let Some(d) = self.role_definitions.iter().find(|d| &d.id == r) {
                out.extend(d.grants.iter().cloned());
            }
        }
        out
    }

    fn is_governance(&self, p: &Principal) -> bool {
        self.governance.principals.contains(p)
    }

    /// The most the actor may put in a role or assign (ADR-0128 rule 10): the union over its roles of
    /// `grantable_scope` (or `grants` when the role has no scope). Protected capabilities are never delegated by
    /// `room.manage_roles` alone: they stay in the ceiling only for a governance principal of a 1-of-1 governance
    /// (decision recorded in the groups plan; a quorum path needs the governance slice).
    pub fn ceiling_of(&self, p: &Principal) -> BTreeSet<String> {
        let Some(m) = self.member(p) else {
            return BTreeSet::new();
        };
        let mut out = BTreeSet::new();
        for r in &m.role_ids {
            if let Some(d) = self.role_definitions.iter().find(|d| &d.id == r) {
                out.extend(
                    d.grantable_scope
                        .as_ref()
                        .unwrap_or(&d.grants)
                        .iter()
                        .cloned(),
                );
            }
        }
        let may_protected = self.is_governance(p) && self.governance.approval_threshold <= 1;
        if !may_protected {
            out.retain(|c| !caps().protected.contains(c));
        }
        out
    }
}

// ------------------------------------------------------------------ state validator (ADR-0176 section 3)

/// The twelve `GroupControlState` rules, first broken rule wins. Verdict names are the frozen vectors'.
pub fn validate_state(s: &GroupState) -> R<()> {
    if (s.state_version == 1) != s.previous_state_hash.is_none() {
        return err("version_previous_mismatch");
    }
    if s.membership.len() > WIRE_MEMBER_CAP {
        return err("membership_too_large");
    }
    let principals: Vec<_> = s.membership.iter().map(|m| m.principal.clone()).collect();
    if !unique(&principals) {
        return err("membership_principal_duplicate");
    }
    let incs: Vec<_> = s.membership.iter().map(|m| m.incarnation.clone()).collect();
    if !unique(&incs) {
        return err("membership_incarnation_duplicate");
    }
    let defs: Vec<_> = s.role_definitions.iter().map(|r| r.id.clone()).collect();
    if !unique(&defs) {
        return err("role_definition_duplicate");
    }
    if s.membership
        .iter()
        .any(|m| m.role_ids.iter().any(|r| !defs.contains(r)))
    {
        return err("role_unknown");
    }
    if s.membership
        .iter()
        .any(|m| (m.principal.kind == PRINCIPAL_ACCOUNT) != (m.endpoint_policy == 1))
    {
        return err("endpoint_policy_kind_mismatch");
    }
    if !unique(&s.coordinator_succession) {
        return err("succession_duplicate");
    }
    if (s.policy.history_policy == 2) != s.policy.history_window_seconds.is_some() {
        return err("history_window_mismatch");
    }
    let refs: Vec<_> = s.invites.iter().map(|i| i.join_ref.clone()).collect();
    if !unique(&refs) {
        return err("invite_join_ref_duplicate");
    }
    for i in &s.invites {
        if i.max_uses.is_some_and(|m| i.uses > m) {
            return err("invite_uses_exceed");
        }
        if s.member(&i.issuer).map(|m| &m.incarnation) != Some(&i.issuer_incarnation) {
            return err("invite_issuer_not_member");
        }
    }
    if s.governance
        .principals
        .iter()
        .any(|p| s.member(p).is_none())
    {
        return err("governance_principal_not_member");
    }
    if s.governance.approval_threshold > s.governance.principals.len() as u64 {
        return err("governance_threshold_exceeds");
    }
    Ok(())
}

// ------------------------------------------------------------------ intent

/// `GroupControlIntent` (object 29). `args` stays a decoded value; the typed accessors below read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub conversation_id: Vec<u8>,
    pub previous_state_hash: Vec<u8>,
    pub previous_state_version: u64,
    pub intent_id: Vec<u8>,
    pub operation: u64,
    pub reducing: bool,
    pub args: Value,
    /// 1 capability_at_parent, 2 space_delegated, 3 membership_basis, 4 account_control_evidence, 5 invite_link.
    pub authorization_kind: u64,
    pub acs_head_hash: Option<Vec<u8>>,
    pub has_space_intent: bool,
    pub join_ref: Option<Vec<u8>>,
}

impl Intent {
    pub fn decode(body: &[u8]) -> R<Self> {
        registry()
            .check_body("GroupControlIntent", body)
            .map_err(GroupError)?;
        let v = om_codec::decode(body, om_codec::Limits::OM, 131_072).map_err(GroupError)?;
        let i = Self::from_value(&v)?;
        validate_intent(&i)?;
        Ok(i)
    }

    pub fn from_value(v: &Value) -> R<Self> {
        if u(req(v, 0)?)? != 29 {
            return err("wrong_type");
        }
        let z = req(v, 9)?;
        Ok(Intent {
            conversation_id: b(req(v, 2)?)?,
            previous_state_hash: digest_of(req(v, 3)?)?,
            previous_state_version: u(req(v, 4)?)?,
            intent_id: b(req(v, 5)?)?,
            operation: u(req(v, 6)?)?,
            reducing: bl(req(v, 7)?)?,
            args: req(v, 8)?.clone(),
            authorization_kind: u(req(z, 0)?)?,
            acs_head_hash: opt(z, 1, digest_of)?,
            has_space_intent: z.get(2).is_some(),
            join_ref: opt(z, 3, b)?,
        })
    }

    pub fn op_name(&self) -> Option<&'static str> {
        op::NAMES
            .get((self.operation as usize).wrapping_sub(1))
            .copied()
    }

    /// `DS-HASH("OM-GROUP-INTENT-HASH-v1", body)` over the canonical body: the binding's `intent_hash`.
    /// (Signed-envelope handling is the MLS slice's; this hashes the body the envelope carries.)
    pub fn hash_of_body(body: &[u8]) -> [u8; 32] {
        ds_hash(LBL_INTENT_HASH, body)
    }
}

/// The nine `GroupControlIntent` rules. Rule order and verdict names follow the frozen checkers.
pub fn validate_intent(i: &Intent) -> R<()> {
    let o = i.operation;
    let always = [2, 3, 5, 11, 16];
    let never = [1, 4, 6, 7, 10, 12];
    // reducing flag: fixed for eleven operations, a claim the receiver recomputes for the other five (T-161)
    if (always.contains(&o) && !i.reducing) || (never.contains(&o) && i.reducing) {
        return err("reducing_flag_mismatch");
    }
    let kind = i.authorization_kind;
    let allowed: &[u64] = match o {
        1 | 2 | 8 | 9 => &[1, 2],
        3 | 16 => &[3],
        4 => &[1, 4],
        5 => &[4],
        11 => &[1, 3],
        12 => &[5],
        6 | 7 | 10 | 13 | 14 | 15 => &[1],
        _ => return err("unknown_operation"),
    };
    if !allowed.contains(&kind) {
        return err("authorization_kind_not_allowed");
    }
    if o == op::MEMBER_ADMIT && i.args.get(4).is_some() && kind != 1 {
        return err("authorization_kind_not_allowed");
    }
    let need_head = kind == 4 || o == op::DEVICE_ADD || o == op::DEVICE_REMOVE_REVOKED;
    if i.acs_head_hash.is_some() != need_head
        || i.has_space_intent != (kind == 2)
        || i.join_ref.is_some() != (kind == 5)
    {
        return err("authorization_field_mismatch");
    }
    let args = &i.args;
    let arg = |k: u64| args.get(k);
    if o == op::INVITE_LINK_REDEEM
        && i.join_ref.as_deref() != arg(0).and_then(|v| b(v).ok()).as_deref()
    {
        return err("join_ref_mismatch");
    }
    let empty = matches!(args, Value::Map(m) if m.is_empty());
    if o == op::SECURITY_POLICY_SET {
        if empty {
            return err("security_policy_empty");
        }
        if arg(2).is_some() && arg(1).and_then(|v| u(v).ok()) != Some(2) {
            return err("history_window_mismatch");
        }
    }
    if o == op::GOVERNANCE_SET {
        if empty {
            return err("governance_set_empty");
        }
        if let Some(s) = arg(1)
            && !unique(&bytes_set(s)?)
        {
            return err("succession_duplicate");
        }
    }
    if o == op::COORDINATOR_REPLACE && b(req(args, 0)?)? == b(req(args, 1)?)? {
        return err("coordinator_same_device");
    }
    if o == op::INVITE_LINK_ISSUE && u(req(req(args, 0)?, 6)?)? != 0 {
        return err("invite_uses_nonzero");
    }
    Ok(())
}

// ------------------------------------------------------------------ binding

/// `GroupCommitBinding` (object 30): the fields the ADR-0173 ordering key and the section 3 rules read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub conversation_id: Vec<u8>,
    /// 1 coordinator_sequenced, 2 direct_reduction, 3 recovery.
    pub binding_kind: u64,
    pub reducing: bool,
    pub previous_state_hash: Vec<u8>,
    pub previous_state_version: u64,
    pub previous_mls_epoch: u64,
    pub resulting_mls_epoch: u64,
    pub intent_hash: Option<Vec<u8>>,
    pub mls_commit_hash: Vec<u8>,
    pub resulting_state_hash: Vec<u8>,
    pub committer_device: Vec<u8>,
    pub has_readded: bool,
}

impl Binding {
    pub fn decode(body: &[u8]) -> R<Self> {
        registry()
            .check_body("GroupCommitBinding", body)
            .map_err(GroupError)?;
        let v = om_codec::decode(body, om_codec::Limits::OM, 131_072).map_err(GroupError)?;
        let x = Self::from_value(&v)?;
        validate_binding(&x)?;
        Ok(x)
    }

    pub fn from_value(v: &Value) -> R<Self> {
        if u(req(v, 0)?)? != 30 {
            return err("wrong_type");
        }
        Ok(Binding {
            conversation_id: b(req(v, 2)?)?,
            binding_kind: u(req(v, 3)?)?,
            reducing: bl(req(v, 4)?)?,
            previous_state_hash: digest_of(req(v, 5)?)?,
            previous_state_version: u(req(v, 6)?)?,
            previous_mls_epoch: u(req(v, 7)?)?,
            resulting_mls_epoch: u(req(v, 8)?)?,
            intent_hash: opt(v, 9, digest_of)?,
            mls_commit_hash: digest_of(req(v, 10)?)?,
            resulting_state_hash: digest_of(req(v, 11)?)?,
            committer_device: b(req(v, 12)?)?,
            has_readded: v.get(13).is_some(),
        })
    }

    /// The ADR-0173 rule 1 same-parent ordering key, computable from the binding alone: a reduction sorts before a
    /// non-reduction, a coordinator-sequenced Commit before a direct one, then committer DeviceID, then the binding
    /// hash. The minimum is canonical. (Used by the ordering slice; here so the key is defined once.)
    pub fn ordering_key(&self, binding_body: &[u8]) -> (bool, bool, Vec<u8>, [u8; 32]) {
        (
            !self.reducing,
            self.binding_kind != 1,
            self.committer_device.clone(),
            ds_hash(LBL_BINDING_HASH, binding_body),
        )
    }
}

/// The three `GroupCommitBinding` rules.
pub fn validate_binding(x: &Binding) -> R<()> {
    if x.binding_kind == 3 {
        if x.intent_hash.is_some() {
            return err("recovery_has_intent");
        }
        if x.previous_state_hash != x.resulting_state_hash {
            return err("recovery_state_changed");
        }
        if x.reducing {
            return err("recovery_reducing");
        }
    } else {
        if x.intent_hash.is_none() {
            return err("intent_hash_missing");
        }
        if x.previous_state_hash == x.resulting_state_hash {
            return err("state_unchanged");
        }
        if x.binding_kind == 2 && !x.reducing {
            return err("direct_not_reducing");
        }
        if x.has_readded {
            return err("readded_outside_recovery");
        }
    }
    if x.resulting_mls_epoch != x.previous_mls_epoch.wrapping_add(1) {
        return err("epoch_step");
    }
    Ok(())
}

// ------------------------------------------------------------------ history bundle structure

/// The `GroupHistoryBundle` structure rules (not the admission gates, which need Room and ACS context: a later
/// slice). `v` is the body value.
pub fn validate_bundle_structure(v: &Value) -> R<()> {
    let author = Principal::parse(req(v, 4)?)?;
    let recipient = Principal::parse(req(v, 6)?)?;
    if author.kind != PRINCIPAL_ACCOUNT {
        return err("author_not_account");
    }
    if recipient.kind != PRINCIPAL_ACCOUNT {
        return err("recipient_not_account");
    }
    if author == recipient {
        return err("recipient_is_author");
    }
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for it in a(req(v, 11)?)? {
        let id = b(req(it, 0)?)?;
        if seen.contains(&id) {
            return err("item_id_duplicate");
        }
        if it.get(4).is_none() && it.get(5).is_none() {
            return err("item_empty");
        }
        if let Some(r) = it.get(6)
            && !seen.contains(&b(r)?)
        {
            return err("reply_to_unknown_or_forward");
        }
        seen.push(id);
    }
    Ok(())
}

// ------------------------------------------------------------------ genesis

/// Everything genesis needs from outside (sans-IO: ids come from the caller's RNG).
pub struct GenesisParams {
    pub conversation_id: Vec<u8>,
    pub creator: Principal,
    pub creator_incarnation: Vec<u8>,
    pub creator_device: Vec<u8>,
    pub admin_role_id: Vec<u8>,
    pub member_role_id: Vec<u8>,
    pub name: String,
    /// The protection profile the Room starts from (ADR-0189 section 3): 1 Recommended, 2 Hardened, 3 Maximum
    /// Privacy. Applied at creation only.
    pub protection: u64,
}

/// The consumer default (SCOPED_PERMISSIONS: 1-of-1 governance held by the creator; Admin and Member are role
/// definitions, the familiar labels being presentation). `mls_epoch_ref` is 0: the creator's own MLS group.
/// Admin holds every W1 room capability; Member holds posting, replying, reacting, uploading and pinning and
/// cannot invite, so adding people is an admin act until a role says otherwise.
pub fn genesis(p: &GenesisParams) -> R<GroupState> {
    let reg = caps();
    let bundle = room::bundle(p.protection)?;
    let mut member_caps = vec![
        "room.post",
        "room.reply",
        "room.react",
        "room.upload",
        "room.pin",
    ];
    // Recommended: anyone in the group can add people (ADR-0189 section 3, "Who can add people").
    if bundle.members_may_invite {
        member_caps.push("room.invite");
    }
    let s = GroupState {
        conversation_id: p.conversation_id.clone(),
        state_version: 1,
        previous_state_hash: None,
        profile: Profile {
            name: p.name.clone(),
            avatar_ref: None,
            description: None,
        },
        membership: vec![Member {
            principal: p.creator.clone(),
            incarnation: p.creator_incarnation.clone(),
            role_ids: vec![p.admin_role_id.clone()],
            endpoint_policy: 1,
        }],
        role_definitions: vec![
            RoleDef {
                id: p.admin_role_id.clone(),
                display_name: "Admin".into(),
                grants: reg.room_w1.clone(),
                grantable_scope: None,
            },
            RoleDef {
                id: p.member_role_id.clone(),
                display_name: "Member".into(),
                grants: member_caps.iter().map(|c| c.to_string()).collect(),
                grantable_scope: None,
            },
        ],
        governance: Governance {
            principals: vec![p.creator.clone()],
            approval_threshold: 1,
            protected_operations: None,
        },
        policy: Policy {
            join_rule: bundle.join_rule,
            new_device_policy: bundle.new_device_policy,
            history_policy: bundle.history_policy,
            history_window_seconds: None,
            disappearing_timer: None,
            security_profile: bundle.security_profile,
            sequencing_timeout: 300,
            past_epoch_window: 2,
        },
        coordinator_succession: vec![p.creator_device.clone()],
        invites: Vec::new(),
        mls_epoch_ref: 0,
        extensions: None,
    };
    let mut s = s;
    s.normalize();
    if !s.profile.valid() || s.conversation_id.len() != 32 || p.admin_role_id == p.member_role_id {
        return err("genesis_params");
    }
    validate_state(&s)?;
    // the codec must accept exactly what we emit
    registry()
        .check_body("GroupControlState", &s.encode())
        .map_err(GroupError)?;
    Ok(s)
}

// ------------------------------------------------------------------ reducer

/// Does `actor` hold what the registry says `control.<op>` needs, at `parent`? `requires` entries are capability ids
/// and `basis: x` markers; with a basis marker the entries are alternatives ("either one suffices").
fn authorized_by_capability(parent: &GroupState, actor: &Principal, op_name: &str) -> bool {
    let Some(req) = caps().requires.get(&format!("control.{op_name}")) else {
        return false;
    };
    let held = parent.capabilities_of(actor);
    let capreq: Vec<_> = req.iter().filter(|r| !r.starts_with("basis:")).collect();
    if capreq.is_empty() {
        return false;
    }
    if capreq.len() < req.len() {
        capreq.iter().any(|c| held.contains(*c))
    } else {
        capreq.iter().all(|c| held.contains(*c))
    }
}

fn roles_within_ceiling(parent: &GroupState, actor: &Principal, role_ids: &[Vec<u8>]) -> R<()> {
    let ceiling = parent.ceiling_of(actor);
    for r in role_ids {
        let d = parent
            .role_definitions
            .iter()
            .find(|d| &d.id == r)
            .ok_or(GroupError("role_unknown"))?;
        if !d.grants.iter().all(|c| ceiling.contains(c))
            || !d
                .grantable_scope
                .iter()
                .flatten()
                .all(|c| ceiling.contains(c))
        {
            return err("role_exceeds_ceiling");
        }
    }
    Ok(())
}

/// Remove a member and everything that hangs on its incarnation: its invites (ADR-0128 rule 6), and its seat in
/// governance. Refuses what would leave the group without members or without a governance principal.
fn drop_member(next: &mut GroupState, who: &Principal) -> R<()> {
    next.membership.retain(|m| &m.principal != who);
    if next.membership.is_empty() {
        return err("last_member");
    }
    next.invites.retain(|i| &i.issuer != who);
    next.governance.principals.retain(|p| p != who);
    if next.governance.principals.is_empty() {
        return err("governance_would_empty");
    }
    next.governance.approval_threshold = next
        .governance
        .approval_threshold
        .min(next.governance.principals.len() as u64);
    Ok(())
}

fn target_of(args: &Value) -> R<(Principal, Vec<u8>)> {
    Ok((Principal::parse(req(args, 0)?)?, b(req(args, 1)?)?))
}

/// Apply one intent to `parent` as `actor` (the account of the verified device signature) and install it at
/// `new_epoch` (the epoch of the Commit that carries it: the parent epoch plus one). Pure: the result is a new
/// state whose `previous_state_hash` is the parent's hash, or an error and no change.
///
/// Authorization is evaluated at the parent state (ADR-0121 rule 1, ADR-0173 rule 1). Operations outside this
/// slice, and authorization kinds that need evidence this slice does not hold (Space delegation, ACS evidence,
/// invite links), are refused, never approximated.
pub fn apply(
    parent: &GroupState,
    intent: &Intent,
    actor: &Principal,
    new_epoch: u64,
) -> R<GroupState> {
    validate_state(parent)?;
    validate_intent(intent)?;
    if intent.conversation_id != parent.conversation_id {
        return err("wrong_conversation");
    }
    if intent.previous_state_hash.as_slice() != parent.hash().as_slice()
        || intent.previous_state_version != parent.state_version
    {
        return err("stale_parent");
    }
    if new_epoch != parent.mls_epoch_ref.wrapping_add(1) {
        return err("epoch_step");
    }
    let name = intent.op_name().ok_or(GroupError("unknown_operation"))?;
    let mut next = parent.clone();
    next.state_version = parent.state_version + 1;
    next.previous_state_hash = Some(parent.hash().to_vec());
    next.mls_epoch_ref = new_epoch;
    let args = &intent.args;
    let capability = intent.authorization_kind == 1;

    match intent.operation {
        op::MEMBER_ADMIT => {
            if !capability || !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            // A join_ref names an invite link; counting its use is undecided (plan, slice 3): refused for now.
            if args.get(4).is_some() {
                return err("unsupported_operation");
            }
            let (who, inc) = target_of(args)?;
            let roles = args.get(2).map(bytes_set).transpose()?.unwrap_or_default();
            if parent.member(&who).is_some() {
                return err("already_member");
            }
            if parent.membership.len() >= PRODUCT_MEMBER_CAP {
                return err("group_full");
            }
            roles_within_ceiling(parent, actor, &roles)?;
            next.membership.push(Member {
                principal: who,
                incarnation: inc,
                role_ids: roles,
                endpoint_policy: u(req(args, 3)?)?,
            });
        }
        op::MEMBER_REMOVE => {
            if !capability || !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            let (who, inc) = target_of(args)?;
            // exact incarnation (ABA protection, R-279)
            if parent.member(&who).map(|m| &m.incarnation) != Some(&inc) {
                return err("target_not_member");
            }
            drop_member(&mut next, &who)?;
        }
        op::MEMBER_LEAVE => {
            let (who, inc) = target_of(args)?;
            if &who != actor || parent.member(&who).map(|m| &m.incarnation) != Some(&inc) {
                return err("not_authorized");
            }
            drop_member(&mut next, &who)?;
        }
        op::PROFILE_SET => {
            if !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            let p = Profile::parse(req(args, 0)?)?;
            if !p.valid() {
                return err("profile_invalid");
            }
            next.profile = p;
        }
        op::ROLE_DEFINE => {
            if !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            let d = RoleDef::parse(req(args, 0)?)?;
            if !d.well_formed() {
                return err("role_invalid");
            }
            // a role edit is a grant: every capability and grantable scope must lie inside the actor's ceiling
            let ceiling = parent.ceiling_of(actor);
            if !d.grants.iter().all(|c| ceiling.contains(c))
                || !d
                    .grantable_scope
                    .iter()
                    .flatten()
                    .all(|c| ceiling.contains(c))
            {
                return err("role_exceeds_ceiling");
            }
            let at = next.role_definitions.iter().position(|r| r.id == d.id);
            match at {
                Some(ix) => next.role_definitions[ix] = d,
                None if next.role_definitions.len() >= 128 => return err("too_many_roles"),
                None => next.role_definitions.push(d),
            }
        }
        op::ROLE_ASSIGN => {
            if !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            let (who, inc) = target_of(args)?;
            let roles = bytes_set(req(args, 2)?)?;
            if roles.len() > 64 || !unique(&roles) {
                return err("role_invalid");
            }
            if parent.member(&who).map(|m| &m.incarnation) != Some(&inc) {
                return err("target_not_member");
            }
            // assigning needs the whole role inside the assigner's ceiling
            roles_within_ceiling(parent, actor, &roles)?;
            if let Some(m) = next.membership.iter_mut().find(|m| m.principal == who) {
                m.role_ids = roles;
            }
        }
        op::ADMISSION_SET => {
            if !capability || !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            let rule = u(req(args, 0)?)?;
            if !(1..=3).contains(&rule) {
                return err("join_rule_invalid");
            }
            next.policy.join_rule = rule;
        }
        op::SECURITY_POLICY_SET => {
            if !capability || !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            if let Some(v) = opt(args, 0, u)? {
                next.policy.new_device_policy = v;
            }
            if let Some(v) = opt(args, 1, u)? {
                next.policy.history_policy = v;
                if v != 2 {
                    next.policy.history_window_seconds = None;
                }
            }
            if let Some(v) = opt(args, 2, u)? {
                next.policy.history_window_seconds = Some(v);
            }
            if let Some(v) = opt(args, 3, u)? {
                // 4 to 65535 are reserved (ADR-0189 section 3): a receiver does not guess what they mean
                if !(1..=3).contains(&v) {
                    return err("security_profile_reserved");
                }
                next.policy.security_profile = v;
            }
        }
        op::DISAPPEARING_TIMER_SET => {
            if !capability || !authorized_by_capability(parent, actor, name) {
                return err("not_authorized");
            }
            // 0 turns it off (the CDDL says `uint`; absent in the state means off)
            next.policy.disappearing_timer = match u(req(args, 0)?)? {
                0 => None,
                n => Some(n),
            };
        }
        _ => return err("unsupported_operation"),
    }
    next.normalize();
    validate_state(&next)?;
    Ok(next)
}

// ------------------------------------------------------------------ persistence

/// The head of one group as persisted in `AppState.groups` (additive: absent in older records).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRec {
    /// The canonical `GroupControlState` body (hex).
    pub head: String,
    /// The head's state hash (hex), kept beside it so a corrupted record is noticed on load.
    pub head_hash: String,
}

impl GroupRec {
    pub fn of(s: &GroupState) -> Self {
        GroupRec {
            head: crate::crypto::hex(&s.encode()),
            head_hash: crate::crypto::hex(&s.hash()),
        }
    }

    /// Re-open the head, checking that it still decodes, validates and hashes to what was stored.
    pub fn open(&self) -> R<GroupState> {
        let body = crate::crypto::unhex(&self.head).map_err(|_| GroupError("record_corrupt"))?;
        let s = GroupState::decode(&body)?;
        if crate::crypto::hex(&s.hash()) != self.head_hash {
            return err("record_corrupt");
        }
        Ok(s)
    }
}

impl crate::state::AppState {
    /// The group with this ConversationID, if any.
    pub fn group(&self, conversation_id: &[u8]) -> R<Option<GroupState>> {
        self.groups
            .get(&crate::crypto::hex(conversation_id))
            .map(GroupRec::open)
            .transpose()
    }

    /// Install a new group from its genesis state. Refuses an existing ConversationID.
    pub fn group_install_genesis(&mut self, s: &GroupState) -> R<()> {
        validate_state(s)?;
        if s.state_version != 1 || s.mls_epoch_ref != 0 {
            return err("not_genesis");
        }
        let key = crate::crypto::hex(&s.conversation_id);
        if self.groups.contains_key(&key) {
            return err("group_exists");
        }
        self.groups.insert(key, GroupRec::of(s));
        Ok(())
    }

    /// Apply an intent to the stored head and persist the result. The stored head is the parent: an intent made
    /// against any other state is refused (`stale_parent`) and nothing changes.
    pub fn group_apply(
        &mut self,
        intent: &Intent,
        actor: &Principal,
        new_epoch: u64,
    ) -> R<GroupState> {
        let key = crate::crypto::hex(&intent.conversation_id);
        let head = self
            .groups
            .get(&key)
            .ok_or(GroupError("unknown_group"))?
            .open()?;
        let next = apply(&head, intent, actor, new_epoch)?;
        self.groups.insert(key, GroupRec::of(&next));
        Ok(next)
    }
}
