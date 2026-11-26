// SPDX-License-Identifier: MPL-2.0
//! Plugin packages and updates (`plugins` slice 4, ADR-0266 rules 2 to 5, PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md
//! "Plugin package and publisher continuity" and "Permission upgrades never ride silently with code updates").
//! Pure state: no clock, no I/O, no signing. The caller verifies the envelope signatures and passes the verified signer
//! ids as `approvers`; time is passed in.
//!
//! * [`PluginState`] is a decoded `PluginControlState` (W4 registry); its hash is `ds_hash` under
//!   `OM-PLUGIN-CONTROL-HASH-v1`. [`PluginChain`] holds the accepted states of one PluginID. Genesis is accepted only if
//!   its hash is the externally pinned trust anchor; a successor is approved by a threshold of the PRIOR state's active
//!   publisher keys. Rollback, replay, forks, skipped versions and key removal are refused.
//! * Releases are immutable and append by exactly one sequence; a release can go `active -> revoked`, never back, and
//!   stays in the state as a tombstone. Publisher keys likewise: revoked keys stay and never reactivate.
//! * The continuity ceiling never grows by itself: a release's ceiling is a subset of what it requests and of the prior
//!   release's ceiling. A capability a release requests beyond that is declared but stays disabled until a device grant
//!   bound to that release's id exists ([`capability_enabled`]).
//! * [`update_decision`] gives rollback protection: an accepted sequence is never undone unless the user pinned that
//!   exact sequence locally, and a revoked release is refused either way.
//! * [`Manifest`] decodes a `PluginPackageManifest` and [`Manifest::matches_release`] ties it to the release record whose
//!   id is its canonical digest; a manifest the control state does not list is not authorized.
//! * [`GrantTable`] keeps `PluginCapabilityGrant` chains by incarnation: revocation is terminal for that incarnation,
//!   new authority is a new incarnation, and expiry reduces only.

use crate::crypto::sig::ds_hash;
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_CONTROL_HASH: &str = "OM-PLUGIN-CONTROL-HASH-v1";
pub const LBL_MANIFEST_HASH: &str = "OM-PLUGIN-MANIFEST-HASH-v1";
pub const LBL_GRANT_HASH: &str = "OM-PLUGIN-GRANT-HASH-v1";

pub const ACTIVE: u64 = 1;
pub const REVOKED: u64 = 2;

pub type Caps = BTreeSet<u8>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
}

fn bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    if let Some(Value::Bytes(b)) = v {
        Some(b.clone())
    } else {
        None
    }
}
fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(n)) = v {
        Some(*n)
    } else {
        None
    }
}
fn digest32(v: Option<&Value>) -> Option<[u8; 32]> {
    match v?.get(1)? {
        Value::Bytes(b) => b.as_slice().try_into().ok(),
        _ => None,
    }
}
fn text(v: Option<&Value>) -> Option<String> {
    if let Some(Value::Text(t)) = v {
        Some(t.clone())
    } else {
        None
    }
}
fn caps(v: Option<&Value>) -> Option<Caps> {
    let Some(Value::Array(a)) = v else {
        return None;
    };
    a.iter().map(|c| uint(Some(c)).map(|n| n as u8)).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherKey {
    pub signer: Vec<u8>,
    pub profile: u64,
    pub key: Vec<u8>,
    pub state: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub seq: u64,
    pub id: [u8; 32],
    pub manifest_hash: [u8; 32],
    pub requested: Caps,
    pub ceiling: Caps,
    pub state: u64,
    pub label: String,
    pub prev: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginState {
    pub plugin_id: [u8; 32],
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub keys: BTreeMap<Vec<u8>, PublisherKey>,
    pub threshold: u64,
    pub releases: BTreeMap<u64, Release>,
    pub hash: [u8; 32],
}

impl PluginState {
    /// Decodes a `PluginControlState` body (the bytes inside the signed envelope) through the W4 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("PluginControlState", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let Some(Value::Array(ks)) = v.get(5) else {
            return Err(bad());
        };
        let mut keys = BTreeMap::new();
        for k in ks {
            let key = PublisherKey {
                signer: bytes(k.get(0)).ok_or_else(bad)?,
                profile: uint(k.get(1)).ok_or_else(bad)?,
                key: bytes(k.get(2)).ok_or_else(bad)?,
                state: uint(k.get(3)).ok_or_else(bad)?,
            };
            if keys.insert(key.signer.clone(), key).is_some() {
                return Err(bad());
            }
        }
        let Some(Value::Array(rs)) = v.get(7) else {
            return Err(bad());
        };
        let mut releases = BTreeMap::new();
        for r in rs {
            let rel = Release {
                seq: uint(r.get(0)).ok_or_else(bad)?,
                id: digest32(r.get(1)).ok_or_else(bad)?,
                manifest_hash: digest32(r.get(2)).ok_or_else(bad)?,
                requested: caps(r.get(3)).ok_or_else(bad)?,
                ceiling: caps(r.get(4)).ok_or_else(bad)?,
                state: uint(r.get(5)).ok_or_else(bad)?,
                label: text(r.get(6)).ok_or_else(bad)?,
                prev: r
                    .get(7)
                    .map(|d| digest32(Some(d)).ok_or_else(bad))
                    .transpose()?,
            };
            if releases.insert(rel.seq, rel).is_some() {
                return Err(bad());
            }
        }
        Ok(PluginState {
            plugin_id: bytes(v.get(2))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            version: uint(v.get(3)).ok_or_else(bad)?,
            prev: v
                .get(4)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            keys,
            threshold: uint(v.get(6)).ok_or_else(bad)?,
            releases,
            hash: ds_hash(LBL_CONTROL_HASH, body),
        })
    }

    fn live_keys(&self) -> usize {
        self.keys.values().filter(|k| k.state == ACTIVE).count()
    }

    /// Whether the successor only revokes (no new key, no new release, nothing else changes): such a successor is always
    /// accepted, so a fork can never stop a revocation.
    fn reduces_to(&self, next: &PluginState) -> bool {
        next.releases.len() == self.releases.len()
            && next.keys.len() == self.keys.len()
            && next.threshold == self.threshold
            && next.keys.iter().all(|(s, k)| {
                self.keys
                    .get(s)
                    .is_some_and(|o| o.state <= k.state && same_key(o, k))
            })
            && next.releases.iter().all(|(q, r)| {
                self.releases
                    .get(q)
                    .is_some_and(|o| o.state <= r.state && same_release(o, r))
            })
    }
}

fn same_key(a: &PublisherKey, b: &PublisherKey) -> bool {
    a.signer == b.signer && a.profile == b.profile && a.key == b.key
}
fn same_release(a: &Release, b: &Release) -> bool {
    a.seq == b.seq
        && a.id == b.id
        && a.manifest_hash == b.manifest_hash
        && a.requested == b.requested
        && a.ceiling == b.ceiling
        && a.label == b.label
        && a.prev == b.prev
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlError {
    WrongPlugin,
    /// Genesis whose hash is not the pinned anchor, or a non-genesis state without a `prev`.
    MissingPrev,
    UnknownPrev,
    /// The version is not exactly one above its predecessor.
    BadVersion,
    Threshold,
    /// An older or already superseded state (rollback or replay).
    Rollback,
    /// A privileged successor while a fork already froze privileged advancement.
    Frozen,
    KeyRemoved,
    KeyResurrected,
    ThresholdAboveLiveKeys,
    ReleaseRemoved,
    ReleaseMutated,
    ReleaseResurrected,
    /// A new release that does not follow the last by exactly one, or whose predecessor link is wrong.
    ReleaseGap,
    /// A release id that is not its manifest hash.
    ReleaseIdMismatch,
    /// A continuity ceiling that is not inside the request or that grows beyond the prior release's.
    CapabilityEscalation,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Accepted,
    Duplicate,
    /// Accepted as a second authority-changing head: privileged advancement is now frozen.
    Forked,
}

pub struct PluginChain {
    id: [u8; 32],
    anchor: [u8; 32],
    known: BTreeMap<[u8; 32], PluginState>,
    tips: BTreeSet<[u8; 32]>,
    frozen: bool,
}

impl PluginChain {
    /// `anchor` is the hash of the genesis state the user or distribution pinned out of band.
    pub fn new(plugin_id: [u8; 32], anchor: [u8; 32]) -> Self {
        Self {
            id: plugin_id,
            anchor,
            known: BTreeMap::new(),
            tips: BTreeSet::new(),
            frozen: false,
        }
    }

    /// Resumes from a previously verified and persisted head.
    pub fn from_verified_head(head: PluginState, anchor: [u8; 32]) -> Self {
        let mut c = Self::new(head.plugin_id, anchor);
        c.tips.insert(head.hash);
        c.known.insert(head.hash, head);
        c
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// The single head, or `None` before genesis or while forked.
    pub fn head(&self) -> Option<&PluginState> {
        if self.tips.len() == 1 {
            self.known.get(self.tips.iter().next()?)
        } else {
            None
        }
    }

    /// A release is revoked once any accepted state revokes it, on any branch.
    pub fn release_revoked(&self, seq: u64) -> bool {
        self.known
            .values()
            .any(|s| s.releases.get(&seq).is_some_and(|r| r.state == REVOKED))
    }

    pub fn accept(
        &mut self,
        st: PluginState,
        approvers: &[Vec<u8>],
    ) -> Result<ControlOutcome, ControlError> {
        if st.plugin_id != self.id {
            return Err(ControlError::WrongPlugin);
        }
        if self.known.contains_key(&st.hash) {
            return if self.tips.contains(&st.hash) {
                Ok(ControlOutcome::Duplicate)
            } else {
                Err(ControlError::Rollback)
            };
        }
        if st.threshold == 0 || st.threshold as usize > st.live_keys() {
            return Err(ControlError::ThresholdAboveLiveKeys);
        }
        let Some(prev_hash) = st.prev else {
            if !self.known.is_empty() || st.version != 1 || st.hash != self.anchor {
                return Err(ControlError::MissingPrev);
            }
            let signed = signers(&st, approvers);
            if (signed as u64) < st.threshold {
                return Err(ControlError::Threshold);
            }
            release_rules(None, &st)?;
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
        if (signers(prev, approvers) as u64) < prev.threshold {
            return Err(ControlError::Threshold);
        }
        key_rules(prev, &st)?;
        release_rules(Some(prev), &st)?;
        let reducing = prev.reduces_to(&st);
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

/// Distinct approvers that are ACTIVE publisher keys of `state`.
fn signers(state: &PluginState, approvers: &[Vec<u8>]) -> usize {
    approvers
        .iter()
        .filter(|a| state.keys.get(*a).is_some_and(|k| k.state == ACTIVE))
        .collect::<BTreeSet<_>>()
        .len()
}

fn key_rules(prev: &PluginState, next: &PluginState) -> Result<(), ControlError> {
    for (id, old) in &prev.keys {
        match next.keys.get(id) {
            None => return Err(ControlError::KeyRemoved),
            Some(k) if !same_key(old, k) => return Err(ControlError::KeyRemoved),
            Some(k) if old.state == REVOKED && k.state != REVOKED => {
                return Err(ControlError::KeyResurrected);
            }
            _ => {}
        }
    }
    Ok(())
}

fn release_rules(prev: Option<&PluginState>, next: &PluginState) -> Result<(), ControlError> {
    let mut last: Option<&Release> = None;
    let mut expect = 1;
    if let Some(p) = prev {
        for (seq, old) in &p.releases {
            let new = next.releases.get(seq).ok_or(ControlError::ReleaseRemoved)?;
            if !same_release(old, new) {
                return Err(ControlError::ReleaseMutated);
            }
            if old.state == REVOKED && new.state != REVOKED {
                return Err(ControlError::ReleaseResurrected);
            }
        }
        expect = p.releases.keys().next_back().copied().unwrap_or(0) + 1;
        last = p.releases.values().next_back();
    }
    for (seq, rel) in next.releases.range(expect..) {
        if *seq != expect {
            return Err(ControlError::ReleaseGap);
        }
        if rel.id != rel.manifest_hash {
            return Err(ControlError::ReleaseIdMismatch);
        }
        if rel.prev != last.map(|l| l.id) {
            return Err(ControlError::ReleaseGap);
        }
        if !rel.ceiling.is_subset(&rel.requested)
            || last.is_some_and(|l| !rel.ceiling.is_subset(&l.ceiling))
        {
            return Err(ControlError::CapabilityEscalation);
        }
        last = Some(rel);
        expect += 1;
    }
    // Sequences start at 1 and have no gaps in the whole state.
    if next
        .releases
        .keys()
        .enumerate()
        .any(|(i, s)| *s != i as u64 + 1)
    {
        return Err(ControlError::ReleaseGap);
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum UpdateError {
    UnknownRelease,
    Revoked,
    /// An older sequence than the one already accepted, and not pinned.
    Rollback,
}

/// What an accepted update carries forward.
#[derive(Debug, PartialEq, Eq)]
pub struct Carry {
    /// The capabilities that continue under the existing grants.
    pub ceiling: Caps,
    /// Requested beyond the ceiling: disabled until a grant bound to this release exists.
    pub disabled_until_granted: Caps,
}

/// Decides whether release `seq` may run. `accepted_seq` is the highest sequence this device has accepted; `pinned` is
/// the sequence the user explicitly pinned (a deliberate local rollback), if any.
pub fn update_decision(
    head: &PluginState,
    seq: u64,
    accepted_seq: u64,
    pinned: Option<u64>,
    revoked_anywhere: bool,
) -> Result<Carry, UpdateError> {
    let rel = head.releases.get(&seq).ok_or(UpdateError::UnknownRelease)?;
    if rel.state == REVOKED || revoked_anywhere {
        return Err(UpdateError::Revoked);
    }
    if seq < accepted_seq && pinned != Some(seq) {
        return Err(UpdateError::Rollback);
    }
    Ok(Carry {
        ceiling: rel.ceiling.clone(),
        disabled_until_granted: rel.requested.difference(&rel.ceiling).copied().collect(),
    })
}

/// A decoded `PluginPackageManifest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub plugin_id: [u8; 32],
    pub seq: u64,
    pub prev: Option<[u8; 32]>,
    pub requested: Caps,
    pub hash: [u8; 32],
}

impl Manifest {
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("PluginPackageManifest", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        Ok(Manifest {
            plugin_id: bytes(v.get(2))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            seq: uint(v.get(3)).ok_or_else(bad)?,
            prev: v
                .get(5)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            requested: caps(v.get(9)).ok_or_else(bad)?,
            hash: ds_hash(LBL_MANIFEST_HASH, body),
        })
    }

    /// The manifest is authorized only if the accepted control state lists the release whose id is this manifest's hash,
    /// with the same sequence, request set and predecessor link.
    pub fn matches_release(&self, head: &PluginState) -> bool {
        self.plugin_id == head.plugin_id
            && head.releases.get(&self.seq).is_some_and(|r| {
                r.id == self.hash
                    && r.manifest_hash == self.hash
                    && r.requested == self.requested
                    && r.prev == self.prev
            })
    }
}

/// A decoded `PluginCapabilityGrant`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub install_id: Vec<u8>,
    pub plugin_id: [u8; 32],
    pub cap: u8,
    pub scope: (u64, Vec<u8>),
    pub issued_at: u64,
    pub expires_at: u64,
    pub device: Vec<u8>,
    pub release_id: [u8; 32],
    pub state: u64,
    pub incarnation: Vec<u8>,
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub hash: [u8; 32],
}

impl Grant {
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("PluginCapabilityGrant", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let scope = v.get(5).ok_or_else(bad)?;
        Ok(Grant {
            install_id: bytes(v.get(2)).ok_or_else(bad)?,
            plugin_id: bytes(v.get(3))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            cap: uint(v.get(4)).ok_or_else(bad)? as u8,
            scope: (
                uint(scope.get(0)).ok_or_else(bad)?,
                bytes(scope.get(1)).ok_or_else(bad)?,
            ),
            issued_at: uint(v.get(6)).ok_or_else(bad)?,
            expires_at: uint(v.get(7)).ok_or_else(bad)?,
            device: bytes(v.get(8)).ok_or_else(bad)?,
            release_id: digest32(v.get(9)).ok_or_else(bad)?,
            state: uint(v.get(10)).ok_or_else(bad)?,
            incarnation: bytes(v.get(11)).ok_or_else(bad)?,
            version: uint(v.get(12)).ok_or_else(bad)?,
            prev: v
                .get(13)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            hash: ds_hash(LBL_GRANT_HASH, body),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrantError {
    /// The signer is not an active device of the account that may issue grants (checked by the caller, reported here).
    IssuerNotAuthorized,
    /// Genesis with a version other than 1 or a `prev`, or a successor without one.
    MissingPrev,
    /// Older than or equal to the head's version with a different hash.
    Rollback,
    /// A different successor of an already-succeeded head.
    Fork,
    /// A revoked incarnation offered again, or a successor that changes anything but `active -> revoked`.
    Resurrection,
    BadWindow,
}

#[derive(Default)]
pub struct GrantTable {
    heads: BTreeMap<Vec<u8>, Grant>,
}

impl GrantTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resumes an incarnation from a previously verified and persisted head.
    pub fn resume(&mut self, head: Grant) {
        self.heads.insert(head.incarnation.clone(), head);
    }

    pub fn accept(&mut self, g: Grant, issuer_authorized: bool) -> Result<(), GrantError> {
        if !issuer_authorized {
            return Err(GrantError::IssuerNotAuthorized);
        }
        if g.expires_at <= g.issued_at {
            return Err(GrantError::BadWindow);
        }
        match self.heads.get(&g.incarnation) {
            None => {
                if g.version != 1 || g.prev.is_some() {
                    return Err(GrantError::MissingPrev);
                }
            }
            Some(head) => {
                if g.hash == head.hash {
                    return Ok(());
                }
                if head.state == REVOKED {
                    return Err(GrantError::Resurrection);
                }
                if g.version < head.version {
                    return Err(GrantError::Rollback);
                }
                if g.version == head.version {
                    return Err(GrantError::Fork);
                }
                if g.version != head.version + 1 || g.prev != Some(head.hash) {
                    return Err(GrantError::Rollback);
                }
                let same = Grant {
                    state: head.state,
                    version: head.version,
                    prev: head.prev,
                    hash: head.hash,
                    ..g.clone()
                };
                if same != *head || g.state < head.state {
                    return Err(GrantError::Resurrection);
                }
            }
        }
        self.heads.insert(g.incarnation.clone(), g);
        Ok(())
    }

    /// A live grant: its incarnation head is active and `now` is inside its window.
    pub fn live(&self, incarnation: &[u8], now: u64) -> bool {
        self.heads
            .get(incarnation)
            .is_some_and(|g| g.state == ACTIVE && g.issued_at <= now && now < g.expires_at)
    }

    fn live_grants(&self, now: u64) -> impl Iterator<Item = &Grant> {
        self.heads
            .values()
            .filter(move |g| g.state == ACTIVE && g.issued_at <= now && now < g.expires_at)
    }
}

/// Whether `cap` is enabled for release `seq` in `scope`: it needs a live grant. Inside the release's continuity ceiling
/// any live grant for the capability and scope carries forward; beyond it, only a grant approved for this exact release.
pub fn capability_enabled(
    grants: &GrantTable,
    head: &PluginState,
    seq: u64,
    cap: u8,
    scope: &(u64, Vec<u8>),
    now: u64,
) -> bool {
    let Some(rel) = head.releases.get(&seq) else {
        return false;
    };
    if rel.state != ACTIVE || !rel.requested.contains(&cap) {
        return false;
    }
    let inside = rel.ceiling.contains(&cap);
    grants.live_grants(now).any(|g| {
        g.plugin_id == head.plugin_id
            && g.cap == cap
            && g.scope == *scope
            && (inside || g.release_id == rel.id)
    })
}
