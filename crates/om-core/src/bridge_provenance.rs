// SPDX-License-Identifier: MPL-2.0
//! Bridge provenance and loop engine (bridges slice 2, ADR-0264, ADR-0267, ADR-0124 rules 2 to 5,
//! INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md section 13, V-013). Pure state: no clock, no I/O, no signing.
//!
//! [`RoomProvenance`] decides whether one bridge-authored import is materialized in one Room:
//!
//! * The author must be the bridge of an active inbound Room mapping ([`ImportMapping`]) whose namespace and foreign
//!   scope are the event's, and whose conversation is the event's.
//! * The trace must end with the author, name no bridge twice, and have `hop_count` below `hop_limit`
//!   ([`crate::bridge_control`]'s endpoint check is separate; the wire already refuses malformed traces).
//! * The origin fingerprint is `HMAC-SHA256(room_secret, canonical ["OM-INTEROP-FP-MAC-v1", origin_namespace,
//!   stable_origin_ref])`. When the observed leg is the origin leg the engine recomputes it and refuses a mismatch; it
//!   never trusts a received fingerprint it can check. When the origin leg is further away it cannot, so the event must
//!   carry the `origin_unverified` fidelity flag and is imported without becoming a dedup key.
//! * Dedup is per Room across bridges and only on a verified fingerprint, never on text, name or time.
//!
//! The Room secret is an input (ADR-0267): this module never derives, stores beyond the Room object, or emits it.
//! Admission, mapping authenticity, MLS roster and export bounds are the caller's and later slices'.

use crate::crypto::kdf::hmac_sha256;
use om_codec::cbor::encode;
use om_codec::{Registry, Value};
use std::collections::BTreeSet;

pub const LBL_FINGERPRINT: &str = "OM-INTEROP-FP-MAC-v1";
/// Default hop limit an originating bridge writes (ADR-0267).
pub const DEFAULT_HOP_LIMIT: u64 = 3;
const FLAG_ORIGIN_UNVERIFIED: u64 = 9;

/// `HMAC-SHA256(room_secret, canonical ["OM-INTEROP-FP-MAC-v1", origin_network_namespace, stable_origin_event_ref])`.
/// A public Room identifier never substitutes for the secret; the BridgeID is not an input.
pub fn origin_fingerprint(room_secret: &[u8; 32], namespace: &str, origin_ref: &str) -> [u8; 32] {
    let msg = encode(&Value::Array(vec![
        Value::Text(LBL_FINGERPRINT.into()),
        Value::Text(namespace.into()),
        Value::Text(origin_ref.into()),
    ]));
    hmac_sha256(room_secret, &msg)
}

/// The authenticated inbound Room mapping an import arrives under (the caller verified its signatures and head).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportMapping {
    pub bridge: [u8; 32],
    pub room: Vec<u8>,
    pub namespace: String,
    pub foreign_scope: String,
    /// 1 om_to_foreign, 2 foreign_to_om, 3 bidirectional.
    pub direction: u64,
    pub active: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImportRefusal {
    Malformed,
    /// The event is not bridge-authored with provenance, or the author is not the mapping's bridge.
    WrongAuthor,
    /// The mapping is revoked, not inbound, for another Room, or another namespace or foreign scope.
    MappingMismatch,
    /// The trace does not end with the author bridge.
    TraceNotAuthor,
    /// A bridge appears twice in the traversed path (loop or reflection).
    Loop,
    /// `hop_count` has reached `hop_limit`.
    HopLimitExhausted,
    /// The observed leg is the origin and the received fingerprint is not the recomputed one.
    FingerprintMismatch,
    /// Origin cannot be verified here and the event does not carry `origin_unverified`.
    MissingUnverifiedFlag,
    /// The same verified origin event is already materialized in this Room (through any bridge).
    Duplicate,
}

/// An accepted import. `verified` is whether the fingerprint was recomputed here; only a verified one is a dedup key.
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub fingerprint: [u8; 32],
    pub verified: bool,
}

pub struct RoomProvenance {
    secret: [u8; 32],
    seen: BTreeSet<[u8; 32]>,
}

fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(n)) = v {
        Some(*n)
    } else {
        None
    }
}
fn bytes(v: Option<&Value>) -> Option<&[u8]> {
    if let Some(Value::Bytes(b)) = v {
        Some(b)
    } else {
        None
    }
}
fn text(v: Option<&Value>) -> Option<&str> {
    if let Some(Value::Text(t)) = v {
        Some(t)
    } else {
        None
    }
}

impl RoomProvenance {
    pub fn new(room_secret: [u8; 32]) -> Self {
        Self {
            secret: room_secret,
            seen: BTreeSet::new(),
        }
    }

    /// Whether a verified origin fingerprint is already materialized in this Room.
    pub fn is_materialized(&self, fp: &[u8; 32]) -> bool {
        self.seen.contains(fp)
    }

    /// Checks one bridge-authored import (canonical `ConversationEvent` body) against its mapping without changing state.
    pub fn check_import(
        &self,
        event: &[u8],
        mapping: &ImportMapping,
    ) -> Result<Verdict, ImportRefusal> {
        let v = Registry::load_w4()
            .check("ConversationEvent", event, false)
            .map_err(|_| ImportRefusal::Malformed)?;
        let author = v.get(5).ok_or(ImportRefusal::Malformed)?;
        let prov = v.get(14).ok_or(ImportRefusal::WrongAuthor)?;
        if uint(author.get(0)) != Some(3) || bytes(author.get(1)) != Some(&mapping.bridge[..]) {
            return Err(ImportRefusal::WrongAuthor);
        }
        if !mapping.active
            || !matches!(mapping.direction, 2 | 3)
            || bytes(v.get(4)) != Some(mapping.room.as_slice())
            || text(prov.get(0)) != Some(mapping.namespace.as_str())
            || text(prov.get(1)) != Some(mapping.foreign_scope.as_str())
        {
            return Err(ImportRefusal::MappingMismatch);
        }
        let trace = prov.get(9).ok_or(ImportRefusal::Malformed)?;
        let Some(Value::Array(path)) = trace.get(2) else {
            return Err(ImportRefusal::Malformed);
        };
        if path.last().and_then(|p| bytes(Some(p))) != Some(&mapping.bridge[..]) {
            return Err(ImportRefusal::TraceNotAuthor);
        }
        let distinct: BTreeSet<&[u8]> = path.iter().filter_map(|p| bytes(Some(p))).collect();
        if distinct.len() != path.len() {
            return Err(ImportRefusal::Loop);
        }
        let (hops, limit) = (uint(trace.get(3)), uint(trace.get(4)));
        if hops.is_none() || limit.is_none() || hops >= limit {
            return Err(ImportRefusal::HopLimitExhausted);
        }
        let received: [u8; 32] = bytes(prov.get(8))
            .and_then(|b| b.try_into().ok())
            .ok_or(ImportRefusal::Malformed)?;
        let origin_ns = text(trace.get(0)).ok_or(ImportRefusal::Malformed)?;
        let verified = origin_ns == mapping.namespace;
        if verified {
            let event_ref = text(prov.get(2)).ok_or(ImportRefusal::Malformed)?;
            if origin_fingerprint(&self.secret, origin_ns, event_ref) != received {
                return Err(ImportRefusal::FingerprintMismatch);
            }
            if self.seen.contains(&received) {
                return Err(ImportRefusal::Duplicate);
            }
        } else {
            let flagged = matches!(prov.get(7), Some(Value::Array(f))
                if f.contains(&Value::Uint(FLAG_ORIGIN_UNVERIFIED)));
            if !flagged {
                return Err(ImportRefusal::MissingUnverifiedFlag);
            }
        }
        Ok(Verdict {
            fingerprint: received,
            verified,
        })
    }

    /// Records an accepted import after it is durably materialized; only a verified fingerprint becomes a dedup key.
    pub fn commit(&mut self, verdict: &Verdict) {
        if verdict.verified {
            self.seen.insert(verdict.fingerprint);
        }
    }
}
