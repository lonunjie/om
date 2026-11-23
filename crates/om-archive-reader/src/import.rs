// SPDX-License-Identifier: MPL-2.0
//! Read-only import semantics (portability slice 2; OM_ARCHIVE_V1_SPEC.md section 8, ADR-0263). An archive is data and
//! evidence, never authority: [`import_readonly`] takes live state and an object verifier by shared reference and returns a
//! view. The view has no field that could carry a key, a device enrolment, a send right or a live-state change, so there is
//! no code path from an archive to authority.
//!
//! * Canonical objects stay opaque exact bytes. One the verifier rejects is `Unverified` and never native history. A
//!   verified control-state object older than the live version is `StaleControl` (historical evidence, never applied); one
//!   newer than live, or unknown to it, is `ControlNeedsLiveValidation` and is never applied from here either.
//! * Imported-history records never become OM events. A record that claims native authorship (`author_principal`,
//!   `author_account_id`, `author_endpoint_id`, `event_id`, `conversation_id`, `signature`), carries an unknown field or an
//!   unknown provenance level, or lacks a required field is quarantined. `cryptographically_linked` needs `link_evidence`
//!   naming an archive object that the verifier reports as a foreign-identity link, and `bridge_attested` one reported as a
//!   bridge attestation; without that evidence the record is downgraded to `foreign_export_label`. Export never upgrades.
//! * Disappearing items: an importer that cannot enforce expiry skips them; this module sees no expiry data and leaves them
//!   to the caller's object verifier, which must report them `Unverified`.

use crate::cbor::{self, Node};
use crate::{Archive, EntryKind};
use std::collections::BTreeMap;

pub const PROVENANCE_LEVELS: [&str; 4] = [
    "foreign_export_label",
    "bridge_attested",
    "cryptographically_linked",
    "user_annotation",
];
const REQUIRED: [&str; 7] = [
    "imported_record_id",
    "source_network_namespace",
    "source_export_fingerprint",
    "display_metadata",
    "content",
    "attachments",
    "provenance_level",
];
const OPTIONAL: [&str; 6] = [
    "source_conversation_ref",
    "source_event_ref",
    "source_actor_ref",
    "source_time_hint",
    "importer_notes",
    "link_evidence",
];
/// Fields that would claim native OM authorship or identity for a foreign record.
const NATIVE_CLAIMS: [&str; 6] = [
    "author_principal",
    "author_account_id",
    "author_endpoint_id",
    "event_id",
    "conversation_id",
    "signature",
];

/// What a verifier says about an archived canonical object it accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    /// Set for control-state objects: `(stream, version)`.
    pub control: Option<(String, u64)>,
    pub evidence: Option<Evidence>,
}

/// The kind of provenance evidence a verified object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    ForeignIdentityLink,
    BridgeAttestation,
}

/// The importer's own object verifier. It verifies with live protocol rules; the archive contributes only bytes.
pub trait ObjectVerifier {
    fn verify(&self, declared_type: &str, bytes: &[u8]) -> Option<Verified>;
}

/// Live control versions as currently validated by the protocol, per stream.
pub trait LiveControl {
    fn live_version(&self, stream: &str) -> Option<u64>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectStatus {
    VerifiedHistory,
    StaleControl { archived: u64, live: u64 },
    ControlNeedsLiveValidation { archived: u64 },
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Quarantine {
    NotAMap,
    NativeAuthorshipClaim(String),
    UnknownField(String),
    MissingField(&'static str),
    UnknownProvenance,
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedRecord {
    pub imported_record_id: String,
    pub source_network_namespace: String,
    pub claimed_level: String,
    pub effective_level: String,
    pub downgraded: bool,
}

/// The whole result. Deliberately has no field for keys, devices, rights or live-state changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportView {
    pub objects: Vec<(String, ObjectStatus)>,
    pub records: Vec<ImportedRecord>,
    /// `(record path, index, reason)`: refused records, not fatal to the archive.
    pub quarantined: Vec<(String, usize, Quarantine)>,
    pub missing_blobs: Vec<String>,
}

fn text_of<'a>(n: &Node<'a>, key: &str) -> Option<&'a str> {
    n.get(key).and_then(|v| v.text())
}

pub fn import_readonly(
    archive: &Archive<'_>,
    live: &dyn LiveControl,
    verifier: &dyn ObjectVerifier,
) -> ImportView {
    let mut view = ImportView {
        missing_blobs: archive.missing_blobs.clone(),
        ..Default::default()
    };
    let mut evidence: BTreeMap<&str, Evidence> = BTreeMap::new();
    for e in archive
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::CanonicalObject)
    {
        let status = match verifier.verify(e.declared_type.as_deref().unwrap_or(""), e.bytes) {
            None => ObjectStatus::Unverified,
            Some(v) => {
                if let Some(ev) = v.evidence {
                    evidence.insert(e.path.as_str(), ev);
                }
                match v.control {
                    Some((stream, version)) => match live.live_version(&stream) {
                        Some(lv) if version < lv => ObjectStatus::StaleControl {
                            archived: version,
                            live: lv,
                        },
                        _ => ObjectStatus::ControlNeedsLiveValidation { archived: version },
                    },
                    None => ObjectStatus::VerifiedHistory,
                }
            }
        };
        view.objects.push((e.path.clone(), status));
    }
    for e in archive.entries.iter().filter(|e| {
        e.kind == EntryKind::Record && e.declared_type.as_deref() == Some("imported_history")
    }) {
        let Ok(list) = cbor::decode(e.bytes) else {
            view.quarantined
                .push((e.path.clone(), 0, Quarantine::Malformed));
            continue;
        };
        let Some(items) = list.array() else {
            view.quarantined
                .push((e.path.clone(), 0, Quarantine::Malformed));
            continue;
        };
        for (i, item) in items.iter().enumerate() {
            match classify(item, &evidence) {
                Ok(rec) => view.records.push(rec),
                Err(q) => view.quarantined.push((e.path.clone(), i, q)),
            }
        }
    }
    view
}

fn classify(
    item: &Node<'_>,
    evidence: &BTreeMap<&str, Evidence>,
) -> Result<ImportedRecord, Quarantine> {
    let map = item.map().ok_or(Quarantine::NotAMap)?;
    let mut keys = Vec::with_capacity(map.len());
    for (k, _) in map {
        keys.push(k.text().ok_or(Quarantine::Malformed)?);
    }
    if let Some(k) = keys.iter().find(|k| NATIVE_CLAIMS.contains(k)) {
        return Err(Quarantine::NativeAuthorshipClaim((*k).to_owned()));
    }
    if let Some(k) = keys
        .iter()
        .find(|k| !REQUIRED.contains(k) && !OPTIONAL.contains(k))
    {
        return Err(Quarantine::UnknownField((*k).to_owned()));
    }
    if let Some(k) = REQUIRED.iter().find(|k| !keys.contains(k)) {
        return Err(Quarantine::MissingField(k));
    }
    let level = text_of(item, "provenance_level").ok_or(Quarantine::Malformed)?;
    if !PROVENANCE_LEVELS.contains(&level) {
        return Err(Quarantine::UnknownProvenance);
    }
    let needed = match level {
        "cryptographically_linked" => Some(Evidence::ForeignIdentityLink),
        "bridge_attested" => Some(Evidence::BridgeAttestation),
        _ => None,
    };
    let backed = needed.is_none_or(|need| {
        text_of(item, "link_evidence")
            .and_then(|p| evidence.get(p))
            .is_some_and(|got| *got == need)
    });
    Ok(ImportedRecord {
        imported_record_id: text_of(item, "imported_record_id")
            .ok_or(Quarantine::Malformed)?
            .to_owned(),
        source_network_namespace: text_of(item, "source_network_namespace")
            .ok_or(Quarantine::Malformed)?
            .to_owned(),
        claimed_level: level.to_owned(),
        effective_level: if backed {
            level
        } else {
            "foreign_export_label"
        }
        .to_owned(),
        downgraded: !backed,
    })
}
