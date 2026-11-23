// SPDX-License-Identifier: MPL-2.0
//! Typed service authorship (services_bots slice 2, ADR-0262, SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md
//! sections 9, 10 and 15, ADR-0129). Pure checks, no clock and no I/O; the caller verifies every signature and supplies
//! the verified signer facts.
//!
//! * A service-authored `ConversationEvent` names `ServicePrincipal` as author (key 5), is sent by an endpoint that is
//!   usable under the current `ServiceChain`, and the service has a live installation in the Room. The result is always
//!   labelled automated: [`Authorship::automated`] is a model fact the UI reads and cannot omit.
//! * Key 13 (representation) adds an account on whose behalf the service acts. It needs a live, scoped
//!   `RepresentationGrant` whose hash is the one named, whose service is the author, whose represented account matches,
//!   and whose actions include the event's action. The author never becomes the account (rule 7 and 8).
//! * A grant is issued or expanded only by an active account control device named in the grant; a messaging-only device
//!   may revoke but never issue. A grant never carries account control, recovery, enrolment, key export or
//!   redelegation, and a grant that reads the counterparty's messages carries the counterparty-notice evidence.
//!   A grant stops validating when its issuing control device is quarantined (it dies with its basis) or is revoked.
//! * Service principals and endpoints never count toward a human or account quorum ([`human_quorum`]).

use crate::crypto::sig::ds_hash;
use crate::service_control::{InstallationTable, Principal, ServiceChain};
use om_codec::{Registry, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const LBL_GRANT_HASH: &str = "OM-REPRESENTATION-GRANT-HASH-v1";

/// Actions a representation may never carry.
pub const FORBIDDEN_ACTIONS: [&str; 5] = [
    "account.control",
    "account.recovery",
    "device.enroll",
    "key.export",
    "representation.redelegate",
];
pub const READ_COUNTERPARTY: &str = "message.read_other_participant";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    Malformed,
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
fn digest32(v: Option<&Value>) -> Option<[u8; 32]> {
    match v?.get(1)? {
        Value::Bytes(b) => b.as_slice().try_into().ok(),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub incarnation: Vec<u8>,
    pub account: [u8; 32],
    pub service: [u8; 32],
    pub scopes: BTreeSet<(u64, Vec<u8>)>,
    pub actions: BTreeSet<String>,
    pub issuer_device: Vec<u8>,
    pub not_before: Option<u64>,
    pub expires_at: Option<u64>,
    pub revoked: bool,
    pub has_notice: bool,
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub hash: [u8; 32],
}

impl Grant {
    /// Decodes a `RepresentationGrant` body through the W4 registry.
    pub fn from_body(body: &[u8]) -> Result<Self, WireError> {
        let v = Registry::load_w4()
            .check_body("RepresentationGrant", body)
            .map_err(|_| WireError::Malformed)?;
        let bad = || WireError::Malformed;
        let Some(Value::Array(sc)) = v.get(5) else {
            return Err(bad());
        };
        let Some(Value::Array(ac)) = v.get(6) else {
            return Err(bad());
        };
        Ok(Grant {
            incarnation: bytes(v.get(2)).ok_or_else(bad)?,
            account: bytes(v.get(3))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            service: bytes(v.get(4))
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
            scopes: sc
                .iter()
                .map(|s| Some((uint(s.get(0))?, bytes(s.get(1))?)))
                .collect::<Option<_>>()
                .ok_or_else(bad)?,
            actions: ac
                .iter()
                .map(|a| match a {
                    Value::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect::<Option<_>>()
                .ok_or_else(bad)?,
            issuer_device: bytes(v.get(8)).ok_or_else(bad)?,
            not_before: uint(v.get(10)),
            expires_at: uint(v.get(11)),
            revoked: uint(v.get(12)).ok_or_else(bad)? == 2,
            has_notice: v.get(13).is_some(),
            version: uint(v.get(14)).ok_or_else(bad)?,
            prev: v
                .get(15)
                .map(|d| digest32(Some(d)).ok_or_else(bad))
                .transpose()?,
            hash: ds_hash(LBL_GRANT_HASH, body),
        })
    }
}

/// What the caller verified about the signer of a grant record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignerRole {
    /// An active account control device (controller role).
    ControlDevice,
    /// A messaging-only device (device role).
    MessagingDevice,
    Other,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrantError {
    /// Issued or expanded by something other than an active control device, or not by the device the grant names.
    Issuer,
    /// A forbidden action (account control, recovery, enrolment, key export, redelegation).
    ForbiddenAction,
    /// Reads the counterparty's messages without the counterparty-notice evidence.
    NoCounterpartyNotice,
    /// not_before is not before expires_at.
    BadWindow,
    MissingPrev,
    UnknownPrev,
    BadVersion,
    /// The service, account or incarnation changed within one grant chain, or a later version widened a revoked one.
    Changed,
    /// A revoked grant offered as active again.
    Resurrected,
    /// The revocation was not signed by a control or messaging device of the account.
    RevokeSigner,
}

#[derive(Default)]
pub struct GrantTable {
    by_id: BTreeMap<Vec<u8>, Vec<Grant>>,
    by_hash: BTreeMap<[u8; 32], (Vec<u8>, usize)>,
    quarantined: BTreeSet<Vec<u8>>,
}

impl GrantTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts one verified grant record. `signer` is the device that signed it: `(device id, role)`.
    pub fn accept(&mut self, g: Grant, signer: (&[u8], SignerRole)) -> Result<(), GrantError> {
        if self.by_hash.contains_key(&g.hash) {
            return Ok(());
        }
        if g.actions
            .iter()
            .any(|a| FORBIDDEN_ACTIONS.contains(&a.as_str()))
        {
            return Err(GrantError::ForbiddenAction);
        }
        if g.actions.contains(READ_COUNTERPARTY) && !g.has_notice {
            return Err(GrantError::NoCounterpartyNotice);
        }
        if let (Some(a), Some(b)) = (g.not_before, g.expires_at)
            && a >= b
        {
            return Err(GrantError::BadWindow);
        }
        let chain = self.by_id.entry(g.incarnation.clone()).or_default();
        match g.prev {
            None => {
                if !chain.is_empty() || g.version != 1 {
                    return Err(GrantError::MissingPrev);
                }
            }
            Some(p) => {
                let last = chain.last().ok_or(GrantError::UnknownPrev)?;
                if last.hash != p {
                    return Err(GrantError::UnknownPrev);
                }
                if g.version != last.version + 1 {
                    return Err(GrantError::BadVersion);
                }
                if (g.account, g.service) != (last.account, last.service) {
                    return Err(GrantError::Changed);
                }
                if last.revoked && !g.revoked {
                    return Err(GrantError::Resurrected);
                }
            }
        }
        if g.revoked {
            // revocation may come from a control device or a messaging device
            if !matches!(
                signer.1,
                SignerRole::ControlDevice | SignerRole::MessagingDevice
            ) {
                return Err(GrantError::RevokeSigner);
            }
        } else if signer.1 != SignerRole::ControlDevice || signer.0 != g.issuer_device.as_slice() {
            return Err(GrantError::Issuer);
        }
        let idx = chain.len();
        self.by_hash.insert(g.hash, (g.incarnation.clone(), idx));
        chain.push(g);
        Ok(())
    }

    /// The issuing control device was quarantined or removed: every grant it issued stops validating.
    pub fn device_quarantined(&mut self, device: &[u8]) {
        self.quarantined.insert(device.to_vec());
    }

    /// The grant named by `hash`, if it is the latest record of its incarnation and still live at `now`.
    pub fn live(&self, hash: &[u8; 32], now: u64) -> Option<&Grant> {
        let (id, idx) = self.by_hash.get(hash)?;
        let chain = self.by_id.get(id)?;
        let g = &chain[*idx];
        let latest = chain.last()?;
        (latest.hash == g.hash
            && !g.revoked
            && !self.quarantined.contains(&g.issuer_device)
            && g.not_before.is_none_or(|t| now >= t)
            && g.expires_at.is_none_or(|t| now < t))
        .then_some(g)
    }
}

/// How a service event reads to everyone: always automated, optionally on behalf of an account. Never account-authored.
#[derive(Debug, PartialEq, Eq)]
pub struct Authorship {
    pub service: [u8; 32],
    pub endpoint: Vec<u8>,
    pub on_behalf_of: Option<[u8; 32]>,
}

/// What a client shows for a service-authored event. It always names the service and says automation; a represented
/// event additionally names the account it acted for, but the account is never shown as the author.
#[derive(Debug, PartialEq, Eq)]
pub struct ViaDisplay {
    pub author_service: [u8; 32],
    pub automated: bool,
    pub via_account: Option<[u8; 32]>,
}

impl Authorship {
    /// Model fact: a service-authored event is always labelled as automation. There is no way to build one without it.
    pub fn automated(&self) -> bool {
        true
    }
    /// The display data a UI renders ("service X, automated, on behalf of account Y").
    pub fn via(&self) -> ViaDisplay {
        ViaDisplay {
            author_service: self.service,
            automated: self.automated(),
            via_account: self.on_behalf_of,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthorError {
    Malformed,
    /// The author is not a ServicePrincipal (key 13 only on service-authored events).
    NotServiceAuthor,
    /// The sending endpoint is not active under the service's control state (unknown, quarantined or revoked).
    EndpointNotUsable,
    /// No live installation of this service in the Room.
    NotInstalled,
    /// Key 13 names no live grant, or the grant is for another service or account.
    NoGrant,
    /// The grant does not cover this Room or this action.
    OutOfScope,
}

/// The action name a representation grant must allow for an event kind (`ConversationEvent` key 8).
fn action_of(kind: u64) -> Option<&'static str> {
    Some(match kind {
        1 => "message.send",
        2 => "message.edit",
        3 => "message.delete",
        4 => "reaction.set",
        5 => "receipt.update",
        6 => "pin.set",
        7 => "moderation.redact",
        8 => "poll.create",
        9 => "poll.vote",
        10 => "poll.close",
        11 => "topic.create",
        12 => "topic.update",
        13 => "topic.status.set",
        14 => "message.topic.set",
        _ => return None,
    })
}

/// Validates the authorship of one service-authored event. `event` is the canonical `ConversationEvent` body, `endpoint`
/// the endpoint whose key signed or carried it (verified by the caller), `room` the Room scope it is delivered into.
pub fn validate_service_event(
    event: &[u8],
    endpoint: &[u8],
    room: &(u64, Vec<u8>),
    now: u64,
    chain: &ServiceChain,
    installs: &InstallationTable,
    grants: &GrantTable,
) -> Result<Authorship, AuthorError> {
    let v = Registry::load_w4()
        .check("ConversationEvent", event, false)
        .map_err(|_| AuthorError::Malformed)?;
    let author = v.get(5).ok_or(AuthorError::Malformed)?;
    if uint(author.get(0)) != Some(2) {
        return Err(AuthorError::NotServiceAuthor);
    }
    let service: [u8; 32] = bytes(author.get(1))
        .and_then(|b| b.try_into().ok())
        .ok_or(AuthorError::Malformed)?;
    if !chain.endpoint_usable(endpoint) || chain.head().is_none_or(|h| h.service_id != service) {
        return Err(AuthorError::EndpointNotUsable);
    }
    if installs.active(&service, room).is_empty() {
        return Err(AuthorError::NotInstalled);
    }
    let on_behalf_of = match v.get(13) {
        None => None,
        Some(rep) => {
            let account: [u8; 32] = bytes(rep.get(0))
                .and_then(|b| b.try_into().ok())
                .ok_or(AuthorError::Malformed)?;
            let hash = digest32(rep.get(1)).ok_or(AuthorError::Malformed)?;
            let g = grants.live(&hash, now).ok_or(AuthorError::NoGrant)?;
            if g.service != service || g.account != account {
                return Err(AuthorError::NoGrant);
            }
            let action = uint(v.get(8)).and_then(action_of);
            if !g.scopes.contains(room) || action.is_none_or(|a| !g.actions.contains(a)) {
                return Err(AuthorError::OutOfScope);
            }
            Some(account)
        }
    };
    Ok(Authorship {
        service,
        endpoint: endpoint.to_vec(),
        on_behalf_of,
    })
}

/// The principals that count toward a human or account quorum: service principals and any principal that is an endpoint
/// of a service never count (principal type 2 is a service; `endpoint_ids` are the endpoints known in any service chain).
pub fn human_quorum(approvers: &[Principal], endpoint_ids: &BTreeSet<Vec<u8>>) -> Vec<Principal> {
    approvers
        .iter()
        .filter(|(kind, id)| *kind != 2 && !endpoint_ids.contains(id))
        .cloned()
        .collect()
}
