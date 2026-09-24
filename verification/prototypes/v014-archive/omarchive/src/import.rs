//! Read-only / rehydrate import semantics over a read archive (OM_ARCHIVE_V1_SPEC.md §8; IMPORT_EXPORT_AND_USER_ESCAPE.md
//! §§7, 10-13, 16, 19). The importer takes live state by shared reference and returns a view: there is no code path
//! from an archive to a signing key, a device enrolment, a Room send capability or a live control-state change.
//! Feasibility prototype, not product code.

use crate::archive::Archive;
use crate::cbor::{self, Value};
use crate::testobj::ObjectVerifier;
use std::collections::BTreeMap;

/// Live authority as currently validated by the protocol (stand-in: latest accepted version per control stream).
#[derive(Clone, Debug, Default)]
pub struct LiveState {
    pub control_versions: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectStatus {
    /// Verifies; ordinary history/evidence.
    VerifiedHistory,
    /// Verifies and is control state older than live: historical evidence only.
    StaleControl { archived: u64, live: u64 },
    /// Verifies and is control state not older than live (or unknown to live state): must be re-validated through the
    /// live protocol; never applied from the archive.
    ControlNeedsLiveValidation { archived: u64 },
    /// Does not verify (bad signature, unknown signer, type mismatch): kept as opaque bytes, never native.
    Unverified,
}

pub const PROVENANCE_LEVELS: &[&str] = &["foreign_export_label", "bridge_attested", "cryptographically_linked", "user_annotation"];
const RECORD_REQUIRED: &[&str] = &["imported_record_id", "source_network_namespace", "source_export_fingerprint", "display_metadata", "content", "attachments", "provenance_level"];
const RECORD_OPTIONAL: &[&str] = &["source_conversation_ref", "source_event_ref", "source_actor_ref", "source_time_hint", "importer_notes", "link_evidence"];
/// Fields that would claim native OM authorship/identity for a foreign record.
const NATIVE_AUTHORSHIP_FIELDS: &[&str] = &["author_principal", "author_account_id", "author_endpoint_id", "event_id", "conversation_id", "signature"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedRecord {
    pub imported_record_id: String,
    pub source_network_namespace: String,
    pub claimed_level: String,
    pub effective_level: String,
    pub downgraded: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportView {
    pub objects: Vec<(String, ObjectStatus)>,
    pub records: Vec<ImportedRecord>,
    /// (record index or id, reason) - records refused but not fatal to the archive.
    pub quarantined: Vec<(String, String)>,
    pub missing_blobs: Vec<String>,
    /// Always empty: kept so tests can assert that no import path produces authority.
    pub authority_changes: Vec<String>,
}

pub fn import_readonly(a: &Archive, live: &LiveState, verifier: &dyn ObjectVerifier) -> ImportView {
    let mut v = ImportView { missing_blobs: a.report.missing_blobs.clone(), ..Default::default() };
    let mut verified_types: BTreeMap<String, String> = BTreeMap::new();
    for o in &a.objects {
        let st = match verifier.verify(&o.object_type, &o.bytes) {
            None => ObjectStatus::Unverified,
            Some(ver) => {
                verified_types.insert(o.path.clone(), ver.object_type.clone());
                if ver.object_type.contains(".control") {
                    match live.control_versions.get(&ver.stream) {
                        Some(&lv) if ver.version < lv => ObjectStatus::StaleControl { archived: ver.version, live: lv },
                        _ => ObjectStatus::ControlNeedsLiveValidation { archived: ver.version },
                    }
                } else {
                    ObjectStatus::VerifiedHistory
                }
            }
        };
        v.objects.push((o.path.clone(), st));
    }
    for r in a.records.iter().filter(|r| r.record_type == "imported_history") {
        let Ok(Value::Array(items)) = cbor::decode(&r.bytes) else {
            v.quarantined.push((r.path.clone(), "not a canonical CBOR array".into()));
            continue;
        };
        for (i, it) in items.iter().enumerate() {
            let tag = format!("{}#{i}", r.path);
            let keys = it.keys();
            if !matches!(it, Value::Map(_)) {
                v.quarantined.push((tag, "record is not a map".into()));
                continue;
            }
            if let Some(k) = keys.iter().find(|k| NATIVE_AUTHORSHIP_FIELDS.contains(k)) {
                v.quarantined.push((tag, format!("foreign record claims native authorship field {k}")));
                continue;
            }
            if let Some(k) = keys.iter().find(|k| !RECORD_REQUIRED.contains(k) && !RECORD_OPTIONAL.contains(k)) {
                v.quarantined.push((tag, format!("unknown field {k}")));
                continue;
            }
            if RECORD_REQUIRED.iter().any(|k| !keys.contains(k)) {
                v.quarantined.push((tag, "missing required field".into()));
                continue;
            }
            let level = it.get("provenance_level").and_then(|x| x.as_text()).unwrap_or("");
            if !PROVENANCE_LEVELS.contains(&level) {
                v.quarantined.push((tag, "unknown provenance_level".into()));
                continue;
            }
            // Export never upgrades provenance: a strong level needs verifiable evidence inside the archive.
            let needed = match level {
                "cryptographically_linked" => Some("test.foreign_identity_link"),
                "bridge_attested" => Some("test.bridge_attestation"),
                _ => None,
            };
            let backed = needed.is_none_or(|t| {
                it.get("link_evidence").and_then(|x| x.as_text()).and_then(|p| verified_types.get(p)).is_some_and(|vt| vt == t)
            });
            let eff = if backed { level } else { "foreign_export_label" };
            v.records.push(ImportedRecord {
                imported_record_id: it.get("imported_record_id").and_then(|x| x.as_text()).unwrap_or("").to_string(),
                source_network_namespace: it.get("source_network_namespace").and_then(|x| x.as_text()).unwrap_or("").to_string(),
                claimed_level: level.to_string(),
                effective_level: eff.to_string(),
                downgraded: !backed,
            });
        }
    }
    v
}
