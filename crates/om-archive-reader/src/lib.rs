//! Read-only OM Portable Archive v1, PLAIN profile. Archive data is not authority.
//! All entries borrow directly from caller-owned storage; no extraction to disk.
#![forbid(unsafe_code)]

mod cbor;
mod limits;
mod manifest;
mod zip;
pub use limits::Limits;
pub mod import;
use sha2::{Digest, Sha256};

/// Stable ZIP error subclasses within the §7 `Zip(<class>)` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipError {
    Truncated,
    Structure,
    Flags,
    Compression,
    Path,
    Duplicate,
    Mode,
    Extra,
    Mismatch,
    Crc,
    SizeLimit,
    CountLimit,
    TotalSizeLimit,
}
impl ZipError {
    pub const fn class_name(self) -> &'static str {
        match self {
            Self::Truncated => "Zip(Truncated)",
            Self::Structure => "Zip(Structure)",
            Self::Flags => "Zip(Flags)",
            Self::Compression => "Zip(Compression)",
            Self::Path => "Zip(Path)",
            Self::Duplicate => "Zip(Duplicate)",
            Self::Mode => "Zip(Mode)",
            Self::Extra => "Zip(Extra)",
            Self::Mismatch => "Zip(Mismatch)",
            Self::Crc => "Zip(Crc)",
            Self::SizeLimit => "Zip(SizeLimit)",
            Self::CountLimit => "Zip(CountLimit)",
            Self::TotalSizeLimit => "Zip(TotalSizeLimit)",
        }
    }
}

/// An error exposes only a stable class, not input-derived strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveError {
    Zip(ZipError),
    NotOmArchive,
    UnsupportedVersion,
    UnexpectedLayout,
    EncryptedUnsupported,
    Cbor,
    Schema,
    UnknownCritical,
    SecretMaterial,
    IntegrityRoot,
    EntrySize,
    EntryHash,
    MissingEntry,
    UnexpectedEntry,
    ManifestSizeLimit,
}
impl ArchiveError {
    pub const fn class_name(self) -> &'static str {
        match self {
            Self::Zip(z) => z.class_name(),
            Self::NotOmArchive => "NotOmArchive",
            Self::UnsupportedVersion => "UnsupportedVersion",
            Self::UnexpectedLayout => "UnexpectedLayout",
            Self::EncryptedUnsupported => "encrypted_unsupported",
            Self::Cbor => "Cbor",
            Self::Schema => "Schema",
            Self::UnknownCritical => "UnknownCritical",
            Self::SecretMaterial => "SecretMaterial",
            Self::IntegrityRoot => "IntegrityRoot",
            Self::EntrySize => "EntrySize",
            Self::EntryHash => "EntryHash",
            Self::MissingEntry => "MissingEntry",
            Self::UnexpectedEntry => "UnexpectedEntry",
            Self::ManifestSizeLimit => "ManifestSizeLimit",
        }
    }
}
impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.class_name())
    }
}
impl std::error::Error for ArchiveError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    CanonicalObject,
    Blob,
    Record,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry<'a> {
    /// Logical path, not a filesystem path.
    pub path: String,
    pub kind: EntryKind,
    /// Unmodified canonical object / blob / record bytes; owned by the caller's input.
    pub bytes: &'a [u8],
    /// The manifest's `object_type` (canonical objects) or `record_type` (records); `None` for blobs.
    pub declared_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Archive<'a> {
    pub archive_id: [u8; 16],
    pub entries: Vec<ArchiveEntry<'a>>,
    /// Blob logical paths marked `present=false`, not a parse error.
    pub missing_blobs: Vec<String>,
}

/// Fully validates a plain OM-ZIP-v1 archive before returning any payload reference.
/// The encrypted container profile has a deliberately separate failure class.
pub fn read_archive<'a>(bytes: &'a [u8], limits: &Limits) -> Result<Archive<'a>, ArchiveError> {
    let entries = zip::parse(bytes, limits)?;
    if entries.first().map(|v| v.name) != Some("om-archive-v1/format") {
        return Err(ArchiveError::NotOmArchive);
    }
    let format = entries[0].data;
    if format != b"om-archive-v1\n" {
        return Err(if format.starts_with(b"om-archive-v") {
            ArchiveError::UnsupportedVersion
        } else {
            ArchiveError::NotOmArchive
        });
    }
    match entries.get(1).map(|e| e.name) {
        Some("om-archive-v1/header.cbor") => return Err(ArchiveError::EncryptedUnsupported),
        Some("om-archive-v1/manifest.cbor") => (),
        _ => return Err(ArchiveError::UnexpectedLayout),
    }
    let manifest_bytes = entries[1].data;
    if (manifest_bytes.len() as u64) > limits.max_manifest_size {
        return Err(ArchiveError::ManifestSizeLimit);
    }
    let manifest = manifest::parse(manifest_bytes)?;
    let actual = &entries[2..];
    let mut seen = 0usize;
    let mut output = Vec::new();
    for item in &manifest.items {
        if !item.present {
            continue;
        }
        let Some(entry) = actual.get(seen) else {
            return Err(ArchiveError::MissingEntry);
        };
        let expected = format!("om-archive-v1/{}", item.path);
        if entry.name != expected {
            // A swapped (still-present) item is misplaced, not missing.
            if actual[seen + 1..]
                .iter()
                .any(|candidate| candidate.name == expected)
            {
                return Err(ArchiveError::UnexpectedEntry);
            }
            let later = manifest
                .items
                .iter()
                .skip_while(|candidate| candidate.path != item.path)
                .skip(1)
                .any(|candidate| {
                    candidate.present
                        && entry.name.strip_prefix("om-archive-v1/") == Some(candidate.path)
                });
            return Err(if later {
                ArchiveError::MissingEntry
            } else {
                ArchiveError::UnexpectedEntry
            });
        }
        if entry.data.len() as u64 != item.size {
            return Err(ArchiveError::EntrySize);
        }
        if Sha256::digest(entry.data).as_slice() != item.hash.as_slice() {
            return Err(ArchiveError::EntryHash);
        }
        if item.kind == EntryKind::Record && item.record_type == Some("imported_history") {
            let record = cbor::decode(entry.data)?;
            if !record
                .array()
                .ok_or(ArchiveError::Schema)?
                .iter()
                .all(|node| node.map().is_some())
            {
                return Err(ArchiveError::Schema);
            }
        }
        output.push(ArchiveEntry {
            path: item.path.to_owned(),
            kind: item.kind,
            bytes: entry.data,
            declared_type: item.object_type.or(item.record_type).map(str::to_owned),
        });
        seen += 1;
    }
    if seen != actual.len() {
        return Err(ArchiveError::UnexpectedEntry);
    }
    Ok(Archive {
        archive_id: manifest.archive_id,
        entries: output,
        missing_blobs: manifest.missing_blobs,
    })
}
