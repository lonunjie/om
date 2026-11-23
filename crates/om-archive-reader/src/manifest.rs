//! Manifest validation and the canonical integrity-root commitment.
use crate::cbor::{self, Node, Value};
use crate::zip::valid_name;
use crate::{ArchiveError, EntryKind};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub(crate) struct InventoryItem<'a> {
    pub(crate) path: &'a str,
    pub(crate) size: u64,
    pub(crate) hash: [u8; 32],
    pub(crate) kind: EntryKind,
    pub(crate) record_type: Option<&'a str>,
    pub(crate) object_type: Option<&'a str>,
    pub(crate) present: bool,
}

#[derive(Debug)]
pub(crate) struct Manifest<'a> {
    pub(crate) archive_id: [u8; 16],
    pub(crate) items: Vec<InventoryItem<'a>>,
    pub(crate) missing_blobs: Vec<String>,
}

const REQUIRED: &[&str] = &[
    "format_version",
    "archive_id",
    "export_scope",
    "generated_at_hint",
    "provenance_profiles",
    "encryption_profile",
    "canonical_object_inventory",
    "blob_inventory",
    "record_inventory",
    "integrity_root",
    "critical_extensions",
    "extensions",
];
const OPTIONAL: &[&str] = &[
    "source_account_hint",
    "included_conversations",
    "exporter_implementation",
];
const SECRETS: &[&str] = &[
    "om.account_root.private",
    "om.device_identity.private",
    "om.device_vault",
    "om.recovery_secret",
    "om.mls.private_state",
    "om.session.private_state",
    "om.service_controller.private",
    "om.bridge_controller.private",
];
fn req<'a, 'b>(v: &'b Node<'a>, key: &str) -> Result<&'b Node<'a>, ArchiveError> {
    v.get(key).ok_or(ArchiveError::Schema)
}
fn text<'a>(v: &Node<'a>) -> Result<&'a str, ArchiveError> {
    v.text().ok_or(ArchiveError::Schema)
}
fn unsigned(v: &Node<'_>) -> Result<u64, ArchiveError> {
    v.uint().ok_or(ArchiveError::Schema)
}
fn fixed<const N: usize>(v: &Node<'_>) -> Result<[u8; N], ArchiveError> {
    let src = v.bytes().ok_or(ArchiveError::Schema)?;
    if src.len() != N {
        return Err(ArchiveError::Schema);
    }
    let mut dst = [0; N];
    dst.copy_from_slice(src);
    Ok(dst)
}
fn array_text(v: &Node<'_>) -> Result<(), ArchiveError> {
    for item in v.array().ok_or(ArchiveError::Schema)? {
        text(item)?;
    }
    Ok(())
}
fn exact(v: &Node<'_>, required: &[&str], optional: &[&str]) -> Result<(), ArchiveError> {
    let map = v.map().ok_or(ArchiveError::Schema)?;
    if map.len() < required.len() || map.len() > required.len() + optional.len() {
        return Err(ArchiveError::Schema);
    }
    for (key, _) in map {
        let k = text(key)?;
        if !required.contains(&k) && !optional.contains(&k) {
            return Err(ArchiveError::Schema);
        }
    }
    for key in required {
        req(v, key)?;
    }
    Ok(())
}
fn hex32(bytes: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &b in bytes {
        s.push(char::from(DIGITS[usize::from(b >> 4)]));
        s.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    s
}
fn inventory<'a>(node: &Node<'a>, kind: EntryKind) -> Result<Vec<InventoryItem<'a>>, ArchiveError> {
    let elements = node.array().ok_or(ArchiveError::Schema)?;
    let mut out = Vec::new();
    let mut previous: Option<&str> = None;
    for el in elements {
        let required: &[&str] = match kind {
            EntryKind::CanonicalObject => &["path", "object_type", "size", "sha256"],
            EntryKind::Blob => &["path", "blob_id", "media_type", "present", "size", "sha256"],
            EntryKind::Record => &["path", "record_type", "size", "sha256"],
        };
        exact(el, required, &[])?;
        let path = text(req(el, "path")?)?;
        if let Some(prev) = previous
            && path <= prev
        {
            return Err(ArchiveError::Schema);
        }
        previous = Some(path);
        // Strict §3.3 grammar, even for deliberately omitted blobs.
        let full = format!("om-archive-v1/{path}");
        if !valid_name(&full) {
            return Err(ArchiveError::Schema);
        }
        let size = unsigned(req(el, "size")?)?;
        let hash = fixed::<32>(req(el, "sha256")?)?;
        let mut present = true;
        let mut record_type = None;
        let mut object_type_out = None;
        match kind {
            EntryKind::CanonicalObject => {
                let object_type = text(req(el, "object_type")?)?;
                object_type_out = Some(object_type);
                if SECRETS.contains(&object_type) {
                    return Err(ArchiveError::SecretMaterial);
                }
                if path != format!("canonical/objects/{}", hex32(&hash)) {
                    return Err(ArchiveError::Schema);
                }
            }
            EntryKind::Blob => {
                let id = fixed::<32>(req(el, "blob_id")?)?;
                text(req(el, "media_type")?)?;
                present = match &req(el, "present")?.value {
                    Value::Bool(v) => *v,
                    _ => return Err(ArchiveError::Schema),
                };
                if path != format!("blobs/{}", hex32(&id)) {
                    return Err(ArchiveError::Schema);
                }
            }
            EntryKind::Record => {
                record_type = Some(text(req(el, "record_type")?)?);
                let stem = path
                    .strip_prefix("records/")
                    .and_then(|s| s.strip_suffix(".cbor"))
                    .ok_or(ArchiveError::Schema)?;
                if stem.is_empty() || stem.contains('/') {
                    return Err(ArchiveError::Schema);
                }
            }
        }
        out.push(InventoryItem {
            path,
            size,
            hash,
            kind,
            record_type,
            object_type: object_type_out,
            present,
        });
    }
    Ok(out)
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Manifest<'_>, ArchiveError> {
    let v = cbor::decode(bytes)?;
    // §5: format version is checked first (after syntactically valid CBOR).
    if text(req(&v, "format_version")?)? != "om-archive-v1" {
        return Err(ArchiveError::UnsupportedVersion);
    }
    exact(&v, REQUIRED, OPTIONAL)?;
    let archive_id = fixed::<16>(req(&v, "archive_id")?)?;
    req(&v, "export_scope")?.map().ok_or(ArchiveError::Schema)?;
    unsigned(req(&v, "generated_at_hint")?)?;
    array_text(req(&v, "provenance_profiles")?)?;
    if !matches!(&req(&v, "encryption_profile")?.value, Value::Null) {
        return Err(ArchiveError::Schema);
    }
    req(&v, "extensions")?.map().ok_or(ArchiveError::Schema)?;
    if let Some(s) = v.get("source_account_hint")
        && !matches!(&s.value, Value::Null | Value::Text(_))
    {
        return Err(ArchiveError::Schema);
    }
    if let Some(s) = v.get("included_conversations") {
        array_text(s)?;
    }
    if let Some(s) = v.get("exporter_implementation") {
        text(s)?;
    }
    let critical = req(&v, "critical_extensions")?;
    array_text(critical)?;
    if !critical.array().ok_or(ArchiveError::Schema)?.is_empty() {
        return Err(ArchiveError::UnknownCritical);
    }
    let canonical = req(&v, "canonical_object_inventory")?;
    let blobs = req(&v, "blob_inventory")?;
    let records = req(&v, "record_inventory")?;
    let mut items = inventory(canonical, EntryKind::CanonicalObject)?;
    let blobs_found = inventory(blobs, EntryKind::Blob)?;
    let mut missing_blobs = Vec::new();
    for blob in &blobs_found {
        if !blob.present {
            missing_blobs.push(blob.path.to_owned());
        }
    }
    items.extend(blobs_found);
    items.extend(inventory(records, EntryKind::Record)?);

    let mut digest = Sha256::new();
    digest.update(b"OM-ARCHIVE-v1/integrity-root\0");
    // canonical([archive_id, canonical_inventory, blob_inventory, record_inventory])
    digest.update([0x84, 0x50]);
    digest.update(archive_id);
    digest.update(canonical.encoded);
    digest.update(blobs.encoded);
    digest.update(records.encoded);
    let expected = fixed::<32>(req(&v, "integrity_root")?)?;
    if digest.finalize().as_slice() != expected.as_slice() {
        return Err(ArchiveError::IntegrityRoot);
    }
    Ok(Manifest {
        archive_id,
        items,
        missing_blobs,
    })
}
