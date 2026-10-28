// SPDX-License-Identifier: MPL-2.0
//! AccountControlState authority (ACCOUNT_CONTINUITY_AND_CONTROL.md section 4.1, ADR-0127/0137/0161/0168).
//!
//! * [`authorize`] is the verifier rule: a child state is diffed against its parent, signers are counted against the
//!   parent, the strongest row among the derived deltas decides, and an unlisted delta is rejected. It is a direct
//!   port of the V-002 executable mirror (`verification/bounded/acs_authorization_table.py`) and is replayed against
//!   that mirror's committed golden vectors in `tests/acs_authorization.rs` (and, locally, against its exhaustive
//!   16,200-case enumeration). The AccountControlState implementation does not merge without that CI coverage
//!   (AGENTS.md, 21:32 checkpoint rules).
//! * [`Signed`] helpers build and verify the W0 `SignedAccountControlState`: one `controller`-role signature per
//!   signing control device over the canonical body, verified against the device credentials the state commits to.
//!
//! Chat clocks, HLC and last-write-wins never decide anything here.

use crate::crypto::binding::{Acs, Credential};
use crate::crypto::cbor;
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{self, DeviceSigner, ROLE_CONTROLLER, ds_sig};
use crate::crypto::{Error, PROFILE_V1};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// A parsed signed state: the state, its canonical body, and the devices whose controller signature verified.
pub type OpenedAcs = (Acs, Vec<u8>, BTreeSet<[u8; 16]>);

pub const LBL_ACS_SIG: &str = "OM-ACS-SIG-v1";

// ---------------------------------------------------------------------------------------------- abstract model

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Retirement {
    pub reason: String,
}

/// The authorization-relevant projection of an AccountControlState. Identifiers are opaque strings so the same code
/// runs over the V-002 vectors (`"d1"`) and real states (hex device ids).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Model {
    pub control: BTreeSet<String>,
    pub quarantined: BTreeSet<String>,
    pub messaging: BTreeSet<String>,
    pub revoked: BTreeSet<String>,
    pub recovery_policy: String,
    pub constitutional_policy: u64,
    pub min_one_to_one_security_generation: u64,
    pub key_epochs: BTreeMap<String, u64>,
    /// `device_credentials` (ACS key 13) as device -> credential digest. Absent in the V-002 abstract vectors, which
    /// do not model credentials: both sides are then empty and no credential delta exists.
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
    pub transparency: String,
    pub generation: u64,
    #[serde(default)]
    pub retirement: Option<Retirement>,
    #[serde(default)]
    pub fork_ack: Option<Vec<String>>,
    #[allow(dead_code)]
    pub previous: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Recovery {
    pub elapsed: u64,
    pub delay: u64,
    pub veto: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Ctx {
    #[serde(default)]
    pub signers: BTreeSet<String>,
    #[serde(default)]
    pub recovery: Option<Recovery>,
    #[serde(default)]
    pub trusted_heads: BTreeSet<String>,
    #[serde(default)]
    pub frozen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accept(String),
    /// Recovery acting alone: waiting for this verifier's own activation interval (R-278).
    Pending(String),
    Reject(String),
}

impl Verdict {
    pub fn class(&self) -> &'static str {
        match self {
            Verdict::Accept(_) => "accept",
            Verdict::Pending(_) => "pending",
            Verdict::Reject(_) => "reject",
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Verdict::Accept(r) | Verdict::Pending(r) | Verdict::Reject(r) => r,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    SelfOnly,
    One,
    MajorityRv,
    Majority,
    Bridge,
    Forbidden,
}

/// The machine-readable policy rows (`spec/protocol/account-control-transition-policy.json`); a test compares this
/// table with the JSON so it can never accept more than the normative relation.
pub const POLICY_ROWS: [(&str, &str); 14] = [
    ("link_messaging", "one_active_nonquarantined"),
    ("promote", "parent_majority_or_recovery_vote"),
    ("quarantine", "one_active_nonquarantined_other_than_target"),
    ("lift_quarantine", "parent_majority_active_nonquarantined"),
    ("remove_messaging", "one_active_nonquarantined"),
    ("revoke_control", "parent_majority_or_recovery_vote"),
    ("recovery_policy", "parent_majority_or_recovery_vote"),
    ("constitutional_policy", "parent_majority"),
    ("raise_min_profile", "one_active_nonquarantined"),
    ("lower_min_profile", "forbidden"),
    ("rotate_own_key", "self_active_nonquarantined"),
    ("transparency", "one_active_nonquarantined"),
    ("fork_bridge", "device_only_parent_majority_common_ancestor"),
    ("retirement", "parent_majority_or_recovery_vote"),
];

fn rule_of(class: &str) -> Rule {
    let auth = POLICY_ROWS
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, a)| *a)
        .unwrap_or("forbidden");
    match auth {
        "one_active_nonquarantined" | "one_active_nonquarantined_other_than_target" => Rule::One,
        "parent_majority_or_recovery_vote" => Rule::MajorityRv,
        "parent_majority_active_nonquarantined" | "parent_majority" => Rule::Majority,
        "self_active_nonquarantined" => Rule::SelfOnly,
        "device_only_parent_majority_common_ancestor" => Rule::Bridge,
        _ => Rule::Forbidden,
    }
}

fn majority(n: usize) -> usize {
    n / 2 + 1
}

/// Derive the delta classes of `c` against `p` (section 4.1, first paragraph).
pub fn deltas(p: &Model, c: &Model) -> Vec<(&'static str, Option<String>)> {
    let mut out: Vec<(&'static str, Option<String>)> = vec![];
    let d = |s: &String| Some(s.clone());
    for x in c.messaging.difference(&p.messaging) {
        out.push(("link_messaging", d(x)));
    }
    for x in c.control.difference(&p.control) {
        out.push(("promote", d(x)));
    }
    for x in c.quarantined.difference(&p.quarantined) {
        out.push(("quarantine", d(x)));
    }
    for x in p.quarantined.difference(&c.quarantined) {
        let class = if c.control.contains(x) {
            "lift_quarantine"
        } else {
            "revoke_control"
        };
        out.push((class, d(x)));
    }
    for x in p.messaging.difference(&c.messaging) {
        let class = if c.revoked.contains(x) {
            "remove_messaging"
        } else {
            "unlisted"
        };
        out.push((class, d(x)));
    }
    for x in p.control.difference(&c.control) {
        let class = if c.revoked.contains(x) {
            "revoke_control"
        } else {
            "unlisted"
        };
        out.push((class, d(x)));
    }
    let removed_control: BTreeSet<&String> = p.control.difference(&c.control).collect();
    let removed_msg: BTreeSet<&String> = p.messaging.difference(&c.messaging).collect();
    for x in c.revoked.difference(&p.revoked) {
        if !removed_control.contains(x) && !removed_msg.contains(x) {
            out.push(("unlisted", d(x)));
        }
    }
    if p.revoked.difference(&c.revoked).next().is_some() {
        out.push(("unlisted", Some("un-revoke".into())));
    }
    if c.recovery_policy != p.recovery_policy {
        out.push(("recovery_policy", None));
    }
    if c.constitutional_policy != p.constitutional_policy {
        out.push(("constitutional_policy", None));
    }
    if c.min_one_to_one_security_generation > p.min_one_to_one_security_generation {
        out.push(("raise_min_profile", None));
    }
    if c.min_one_to_one_security_generation < p.min_one_to_one_security_generation {
        out.push(("lower_min_profile", None));
    }
    for (dev, e) in &c.key_epochs {
        if p.key_epochs.get(dev).is_some_and(|old| old != e) {
            out.push(("rotate_own_key", d(dev)));
        }
    }
    credential_deltas(p, c, &mut out);
    if c.transparency != p.transparency {
        out.push(("transparency", None));
    }
    if c.retirement.is_some() {
        out.push(("retirement", None));
    }
    if c.fork_ack.is_some() {
        out.push(("fork_bridge", None));
    }
    out
}

/// Every change of a `device_credentials` entry is a listed delta (B1, audit 2026-10-28):
/// * an existing entry replaced by a different digest is `rotate_own_key` and needs a strictly higher
///   `control_key_epochs` value for that device (an absent epoch counts as 0), else it is unlisted;
/// * an entry removed is unlisted unless the device is revoked in the child (that removal is part of the revoke);
/// * an entry added is unlisted unless its device is added to the messaging or control set in the same step.
fn credential_deltas(p: &Model, c: &Model, out: &mut Vec<(&'static str, Option<String>)>) {
    let epoch = |m: &Model, dev: &String| m.key_epochs.get(dev).copied().unwrap_or(0);
    for (dev, digest) in &c.credentials {
        match p.credentials.get(dev) {
            Some(old) if old == digest => {}
            Some(_) if epoch(c, dev) > epoch(p, dev) => {
                out.push(("rotate_own_key", Some(dev.clone())))
            }
            Some(_) => out.push(("unlisted", Some(dev.clone()))),
            None => {
                let added = (c.messaging.contains(dev) && !p.messaging.contains(dev))
                    || (c.control.contains(dev) && !p.control.contains(dev));
                if !added {
                    out.push(("unlisted", Some(dev.clone())));
                }
            }
        }
    }
    for dev in p.credentials.keys() {
        if !c.credentials.contains_key(dev) && !c.revoked.contains(dev) {
            out.push(("unlisted", Some(dev.clone())));
        }
    }
}

fn after_invariants(c: &Model) -> Vec<&'static str> {
    let mut bad = vec![];
    if c.control.difference(&c.quarantined).next().is_none() {
        bad.push("zero voting controllers");
    }
    if c.control.intersection(&c.revoked).next().is_some() {
        bad.push("revoked device is a controller");
    }
    if c.messaging.intersection(&c.revoked).next().is_some() {
        bad.push("revoked device is a messaging device");
    }
    bad
}

/// The verifier rule. Signers are counted against the PARENT; the caller must already have verified that each
/// signer's signature is valid under that device's credential.
pub fn authorize(p: &Model, c: &Model, ctx: &Ctx) -> Verdict {
    let v = authorize_inner(p, c, ctx);
    if !matches!(v, Verdict::Reject(_)) {
        if let Some(b) = after_invariants(c).first() {
            return Verdict::Reject(format!("section 13: child would have {b}"));
        }
        let voting_after = c.control.difference(&c.quarantined).count();
        if voting_after < majority(c.control.len())
            && deltas(p, c).iter().any(|(k, _)| *k == "quarantine")
        {
            let (tag, why) = (v.class(), v.reason().to_string());
            let why = format!("{why}; child enters SAFETY_FREEZE (section 4 rule 4)");
            return if tag == "accept" {
                Verdict::Accept(why)
            } else {
                Verdict::Pending(why)
            };
        }
    }
    v
}

fn authorize_inner(p: &Model, c: &Model, ctx: &Ctx) -> Verdict {
    use Verdict::{Accept, Pending, Reject};
    let rej = |s: &str| Reject(s.to_string());
    if p.retirement.is_some() {
        return rej("parent retired: AccountRetirement is terminal");
    }
    if c.generation != p.generation + 1 {
        return rej("generation must advance by exactly one");
    }
    let ds = deltas(p, c);
    if ds.is_empty() {
        return rej("no delta");
    }
    if ds.iter().any(|(k, _)| *k == "unlisted") {
        return rej("delta not listed in section 4.1");
    }
    let rule = ds
        .iter()
        .map(|(k, _)| rule_of(k))
        .max()
        .unwrap_or(Rule::Forbidden);
    if rule == Rule::Forbidden {
        return rej(
            "lowering/non-raising minimum_crypto_profile on the one_to_one security_generation axis is forbidden",
        );
    }
    let control = &p.control;
    let quarantined = &p.quarantined;
    let voting: BTreeSet<&String> = control.difference(quarantined).collect();
    let signers = &ctx.signers;
    let maj = majority(control.len());
    let safety_frozen = !quarantined.is_empty() && voting.len() < maj;
    if safety_frozen {
        // Section 5 / ADR-0137 as amended by ADR-0168: the freeze is derived from the parent state alone; freeze
        // resolution is the sole transition that counts quarantined challenge signatures.
        if rule == Rule::Bridge {
            return rej(
                "a quarantine-induced safety freeze is not a fork freeze; fork bridge is a separate transition",
            );
        }
        if ds.iter().any(|(k, _)| {
            !matches!(
                *k,
                "lift_quarantine" | "revoke_control" | "remove_messaging"
            )
        }) {
            return rej(
                "parent is under quarantine-induced SAFETY_FREEZE; only restore/revoke freeze resolutions may resolve it",
            );
        }
        if signers.difference(control).next().is_some() {
            return rej("a freeze-resolution signer is not a controller of the parent");
        }
        if ctx.recovery.as_ref().is_some_and(|r| r.veto) {
            return rej("RecoveryVeto observed");
        }
        if ctx.recovery.is_some() && signers.is_empty() {
            return rej(
                "a recovery vote resolves a freeze only co-signed by at least one parent controller",
            );
        }
        let rv = usize::from(ctx.recovery.is_some());
        if signers.len() + rv < maj {
            return rej(
                "freeze resolution needs a parent majority (device signatures plus at most one co-signed recovery vote)",
            );
        }
        let revoked: BTreeSet<&String> = control.difference(&c.control).collect();
        let restored: BTreeSet<&String> = quarantined
            .difference(&c.quarantined)
            .filter(|x| !revoked.contains(x))
            .collect();
        let removed_msg: BTreeSet<&String> = p.messaging.difference(&c.messaging).collect();
        if restored.is_empty() && revoked.is_empty() {
            return rej("freeze resolution must restore or revoke at least one controller");
        }
        if !restored.iter().all(|x| signers.contains(*x)) {
            return rej("every restored controller must contribute its freeze-challenge signature");
        }
        if revoked.iter().any(|x| signers.contains(*x)) {
            return rej("a freeze-resolution signer cannot revoke itself");
        }
        if !removed_msg.is_subset(&revoked) {
            return rej(
                "freeze resolution may remove messaging devices only for revoked controllers",
            );
        }
        if c.quarantined.difference(quarantined).next().is_some() {
            return rej("freeze resolution may not add new quarantines");
        }
        if c.control.difference(&c.quarantined).count() < majority(c.control.len()) {
            return rej(
                "freeze resolution must leave an active, non-quarantined majority of the child controllers",
            );
        }
        let how = if rv == 1 {
            "with a co-signed recovery vote"
        } else {
            "device-only"
        };
        return Accept(format!(
            "parent majority ({how}) resolves the freeze by restoring/revoking controllers"
        ));
    }
    if ctx.frozen && rule != Rule::Bridge {
        return rej("a non-quarantine freeze is resolved only by the fork-bridge path");
    }
    if signers.iter().any(|s| !voting.contains(s)) {
        return rej("a signer is not an active, non-quarantined controller of the parent");
    }
    let n = control.len();
    // Structural sanity of specific deltas.
    for (k, d) in &ds {
        let Some(d) = d else { continue };
        if *k == "quarantine" && (!control.contains(d) || signers.contains(d)) {
            return rej("quarantine target must be another control device of the parent");
        }
        if *k == "rotate_own_key"
            && (signers.len() != 1 || !signers.contains(d) || quarantined.contains(d))
        {
            return rej("only the device itself, not quarantined, may rotate its own key entry");
        }
        if *k == "promote" && p.revoked.contains(d) {
            return rej("a revoked device id is never reused");
        }
    }
    let rec = ctx.recovery.as_ref();
    if rule == Rule::Bridge {
        let ack: BTreeSet<&String> = c.fork_ack.iter().flatten().collect();
        if ack.len() < 2 {
            return rej("fork bridge must list every observed branch head (>= 2)");
        }
        if !ctx.trusted_heads.iter().all(|h| ack.contains(h)) {
            return rej("fork_ack does not cover a head this verifier trusts");
        }
        if rec.is_some() {
            return rej("a recovery vote never counts for a fork bridge");
        }
        return if signers.len() >= maj {
            Accept("device-only majority of the common ancestor".into())
        } else {
            rej("fork bridge needs a device-only majority")
        };
    }
    if c.retirement.as_ref().is_some_and(|r| r.reason == "moved") && signers.is_empty() {
        return rej(
            "R-201: a successor pointer needs device signers under the parent state (guardians alone cannot move an account)",
        );
    }
    match rule {
        Rule::One | Rule::SelfOnly => {
            if rec.is_some() {
                return rej("a recovery vote is not a controller signature");
            }
            if signers.is_empty() {
                rej("no controller signature")
            } else {
                Accept("single active controller".into())
            }
        }
        Rule::Majority => {
            if rec.is_some() {
                return rej("this delta takes no recovery vote");
            }
            if signers.len() >= maj {
                Accept("majority".into())
            } else {
                Reject(format!("majority {maj} of N={n} required"))
            }
        }
        _ => {
            let Some(r) = rec else {
                return if signers.len() >= maj {
                    Accept("majority".into())
                } else {
                    Reject(format!("majority {maj} of N={n} required"))
                };
            };
            if r.veto {
                return rej("RecoveryVeto observed");
            }
            if !signers.is_empty() {
                return if signers.len() + 1 >= maj {
                    Accept("majority reached with a co-signed recovery vote".into())
                } else {
                    rej("recovery vote + co-signers below majority")
                };
            }
            if r.elapsed < r.delay {
                return Pending(
                    "recovery alone: waiting for this verifier's own activation interval (R-278)"
                        .into(),
                );
            }
            Accept("recovery alone after the full delay with no veto".into())
        }
    }
}

// ---------------------------------------------------------------------------------------------- real states

fn hex16(d: &[u8; 16]) -> String {
    crate::crypto::hex(d)
}

/// Project a real state onto the abstract model. `None` when it carries something this verifier cannot judge
/// (an unknown profile): such a state is never authorized.
pub fn model_of(a: &Acs) -> Option<Model> {
    // Only OM-Crypto-v1 exists in production; its security_generation.one_to_one is 1 (om-crypto-profiles.json).
    let sec_gen = (a.min_profile == PROFILE_V1).then_some(1u64)?;
    let ids = |s: &BTreeSet<[u8; 16]>| s.iter().map(hex16).collect::<BTreeSet<_>>();
    Some(Model {
        control: ids(&a.control),
        quarantined: ids(&a.quarantined),
        messaging: ids(&a.messaging),
        revoked: ids(&a.revoked),
        recovery_policy: a
            .recovery_commitment
            .map(|r| crate::crypto::hex(&r))
            .unwrap_or_default(),
        constitutional_policy: (a.constitutional.0 << 32) | a.constitutional.1.unwrap_or(0),
        min_one_to_one_security_generation: sec_gen,
        key_epochs: a.key_epochs.iter().map(|(d, e)| (hex16(d), *e)).collect(),
        credentials: a
            .credentials
            .iter()
            .map(|(d, h)| (hex16(d), crate::crypto::hex(h)))
            .collect(),
        transparency: a
            .transparency
            .map(|r| crate::crypto::hex(&r))
            .unwrap_or_default(),
        generation: a.generation,
        retirement: None,
        fork_ack: None,
        previous: a
            .previous
            .map(|r| crate::crypto::hex(&r))
            .unwrap_or_default(),
    })
}

/// Sign `body` as each of `signers` (role `controller`). One entry per distinct device.
pub fn sign_acs<R: RngCore + CryptoRng>(
    body: &[u8],
    signers: &[(&[u8; 16], &DeviceSigner)],
    rng: &mut R,
) -> Vec<u8> {
    let entries = signers
        .iter()
        .map(|(id, s)| {
            let sg = s.sign_with(
                &ds_sig(LBL_ACS_SIG, PROFILE_V1, ROLE_CONTROLLER, *id, body),
                arr32(rng),
            );
            sig::signature_entry(*id, PROFILE_V1, ROLE_CONTROLLER, sg)
        })
        .collect();
    sig::envelope(body.to_vec(), entries)
}

/// Open a `SignedAccountControlState`: returns the parsed state and the devices whose controller signature verified.
/// Duplicate, unknown or invalid signatures fail the whole object. Which signers are *sufficient* is [`authorize`]'s
/// job, evaluated against the parent.
///
/// `authority` names the state whose committed credentials the signers must hold: the PARENT for a successor
/// (a state can never vouch for the keys that sign it; ACCOUNT_CONTINUITY_AND_CONTROL.md section 4.1, B1), and
/// `None` only for a genesis, which has no parent and so is bound by its own commitments.
pub fn open_signed(
    signed: &[u8],
    credentials: &[Vec<u8>],
    authority: Option<&Acs>,
) -> Result<OpenedAcs, Error> {
    let v = cbor::decode(signed)?;
    let body = v.get(1)?.b()?.to_vec();
    let acs = Acs::from_body(&body)?;
    let creds: Vec<Credential> = credentials
        .iter()
        .map(|c| Credential::from_body(c))
        .collect::<Result<_, _>>()?;
    let mut who = BTreeSet::new();
    let entries = v.get(2)?.a()?;
    if entries.is_empty() || entries.len() > 16 {
        return Err(Error::Binding("acs: 1..16 signatures required"));
    }
    for e in entries {
        let id: [u8; 16] = e
            .get(0)?
            .b()?
            .try_into()
            .map_err(|_| Error::Binding("acs signer id"))?;
        if e.get(1)?.u()? != PROFILE_V1 || e.get(2)?.u()? != ROLE_CONTROLLER {
            return Err(Error::Binding("acs signature profile/role"));
        }
        // The signer's key must be the credential the authority state commits to (digest check), for this account.
        let committed = authority.unwrap_or(&acs);
        let cred = creds
            .iter()
            .find(|c| {
                c.device_id == id
                    && c.account_id == acs.account_id
                    && committed
                        .credentials
                        .iter()
                        .any(|(d, h)| *d == id && *h == c.hash())
            })
            .ok_or(Error::Binding("acs signer has no committed credential"))?;
        sig::verify(
            &cred.signing_pk,
            &ds_sig(LBL_ACS_SIG, PROFILE_V1, ROLE_CONTROLLER, &id, &body),
            e.get(3)?.b()?,
        )?;
        if !who.insert(id) {
            return Err(Error::Binding("acs: duplicate signer"));
        }
    }
    Ok((acs, body, who))
}

/// Verify a genesis state: generation 1, no parent, signed by every control device it names.
pub fn verify_genesis(signed: &[u8], credentials: &[Vec<u8>]) -> Result<Acs, Error> {
    let (acs, _, who) = open_signed(signed, credentials, None)?;
    if acs.generation != 1 || acs.previous.is_some() {
        return Err(Error::Binding("acs genesis: generation 1 without a parent"));
    }
    if acs.control != who {
        return Err(Error::Binding(
            "acs genesis: every control device must sign the genesis",
        ));
    }
    // The genesis names exactly the credentials it commits to, each present.
    if acs.credentials.len() != credentials.len() {
        return Err(Error::Binding("acs genesis: credential set mismatch"));
    }
    Ok(acs)
}

/// Verify `signed` as the successor of the trusted `parent`: the child must chain to the parent's hash and
/// [`authorize`] must accept it against the parent with the verified signers. Anything else (a larger generation
/// alone, a different branch, a pending recovery) is not accepted here.
pub fn verify_successor(
    parent: &Acs,
    signed: &[u8],
    credentials: &[Vec<u8>],
) -> Result<Acs, Error> {
    // Signers are checked against the PARENT's credentials, never the child's own.
    let (child, _, who) = open_signed(signed, credentials, Some(parent))?;
    if child.account_id != parent.account_id || child.previous != Some(parent.hash()) {
        return Err(Error::Binding("acs: not a descendant of the trusted head"));
    }
    let (pm, cm) = (
        model_of(parent).ok_or(Error::Binding("acs: unknown profile"))?,
        model_of(&child).ok_or(Error::Binding("acs: unknown profile"))?,
    );
    let ctx = Ctx {
        signers: who.iter().map(hex16).collect(),
        ..Ctx::default()
    };
    match authorize(&pm, &cm, &ctx) {
        Verdict::Accept(_) => Ok(child),
        _ => Err(Error::Binding(
            "acs: transition not authorized by the parent",
        )),
    }
}

/// Verify a whole chain `[genesis, child1, ...]` and return the head. Every link must descend from the previous
/// one and be authorized by its parent; the credentials must cover every signer along the way.
pub fn verify_chain(chain: &[Vec<u8>], credentials: &[Vec<u8>]) -> Result<Acs, Error> {
    let (first, rest) = chain
        .split_first()
        .ok_or(Error::Binding("acs: empty chain"))?;
    if rest.len() > 64 {
        return Err(Error::Binding("acs: chain too long"));
    }
    let mut head = verify_genesis_in(first, credentials)?;
    for s in rest {
        head = verify_successor(&head, s, credentials)?;
    }
    Ok(head)
}

/// The genesis is verified against the credentials that exist at its generation: only the ones it commits to.
fn verify_genesis_in(signed: &[u8], credentials: &[Vec<u8>]) -> Result<Acs, Error> {
    let body = Acs::from_body(cbor::decode(signed)?.get(1)?.b()?)?;
    let committed: Vec<Vec<u8>> = credentials
        .iter()
        .filter(|c| {
            Credential::from_body(c).is_ok_and(|c| {
                body.credentials
                    .iter()
                    .any(|(d, h)| *d == c.device_id && *h == c.hash())
            })
        })
        .cloned()
        .collect();
    verify_genesis(signed, &committed)
}

// ---------------------------------------------------------------------------------------------- local planning

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Quarantine a control device (reversible; never for messaging devices, which are removed instead).
    Suspend,
    /// Permanent removal: terminal for that DeviceID.
    Revoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The target is not a device of this account (or is already revoked).
    NotFound,
    /// The acting device is not an active, non-quarantined controller of the head.
    NotController,
    /// Wrong action for the target, or it would leave no voting controller / suspend itself.
    Conflict,
    /// A suspension that would put the account into a SAFETY_FREEZE; removal is offered instead (section 9).
    WouldFreeze,
    /// Authorized only with signatures from other controllers (or a recovery vote), which this device cannot gather.
    NeedsOtherSigners,
    /// Not authorized even with every voting controller signing.
    Forbidden,
}

/// Build the child state for `action` against `target` and run it through [`authorize`] with `me` as the only
/// signer. The result is the unsigned child; the caller signs it and re-verifies with [`verify_successor`].
pub fn plan(
    head: &Acs,
    me: &[u8; 16],
    action: Action,
    target: &[u8; 16],
) -> Result<Acs, PlanError> {
    if !head.control.contains(me) || head.quarantined.contains(me) {
        return Err(PlanError::NotController);
    }
    let mut child = head.clone();
    child.generation = head.generation + 1;
    child.previous = Some(head.hash());
    match action {
        Action::Suspend => {
            if head.messaging.contains(target) {
                return Err(PlanError::Conflict); // messaging devices are removed, never quarantined
            }
            if !head.control.contains(target) {
                return Err(PlanError::NotFound);
            }
            if head.quarantined.contains(target) || target == me {
                return Err(PlanError::Conflict);
            }
            child.quarantined.insert(*target);
        }
        Action::Revoke => {
            if head.revoked.contains(target)
                || !(head.control.contains(target) || head.messaging.contains(target))
            {
                return Err(PlanError::NotFound);
            }
            child.control.remove(target);
            child.quarantined.remove(target);
            child.messaging.remove(target);
            child.revoked.insert(*target);
            child.credentials.retain(|(d, _)| d != target);
            child.key_epochs.retain(|(d, _)| d != target);
        }
    }
    let (Some(pm), Some(cm)) = (model_of(head), model_of(&child)) else {
        return Err(PlanError::Forbidden);
    };
    let me_hex = hex16(me);
    let alone = Ctx {
        signers: [me_hex].into(),
        ..Ctx::default()
    };
    match authorize(&pm, &cm, &alone) {
        Verdict::Accept(why) if why.contains("SAFETY_FREEZE") => Err(PlanError::WouldFreeze),
        Verdict::Accept(_) => Ok(child),
        Verdict::Pending(_) | Verdict::Reject(_) => {
            let all = Ctx {
                signers: pm.control.difference(&pm.quarantined).cloned().collect(),
                ..Ctx::default()
            };
            match authorize(&pm, &cm, &all) {
                Verdict::Accept(_) => Err(PlanError::NeedsOtherSigners),
                _ if after_invariants(&cm).is_empty() => Err(PlanError::Forbidden),
                _ => Err(PlanError::Conflict),
            }
        }
    }
}

/// Is the head under a quarantine-induced SAFETY_FREEZE (derived from the head alone, section 5)?
pub fn safety_frozen(head: &Acs) -> bool {
    !head.quarantined.is_empty()
        && head.control.difference(&head.quarantined).count() < majority(head.control.len())
}
