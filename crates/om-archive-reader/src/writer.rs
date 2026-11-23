// SPDX-License-Identifier: MPL-2.0
//! Plain-mode OM-ZIP-v1 writer and selective, staged export (portability slice 3; OM_ARCHIVE_V1_SPEC.md sections 3 to 5,
//! ADR-0263). Written from the specification: the strict ZIP subset (every entry stored, flag 0x0800, fixed DOS date, ZIP64
//! only when required), the canonical-CBOR manifest and the integrity root.
//!
//! * [`write_archive`] lays out exactly one valid byte sequence for a set of payload items; it never writes a secret object
//!   type and returns [`WriteError`] instead.
//! * [`ExportJob`] selects candidates, excludes what must never leave (secret types, ViewOnce items, and disappearing items
//!   because only a secure export may carry them and no encrypted profile is adopted), stages every payload item through a
//!   [`Staging`] sink, and assembles the manifest only when each item is durably staged, so a partial export never publishes
//!   a manifest naming something that was not written. An interrupted job resumes (staging is idempotent) or is discarded.
//! * [`PortabilityReport`] states what was included, what was excluded and why, and which blobs were deliberately omitted.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The registry of object types that must never be written (`om-secret-object-types.json`, spec section 5.6).
pub const SECRET_TYPES: [&str; 8] = [
    "om.account_root.private",
    "om.device_identity.private",
    "om.device_vault",
    "om.recovery_secret",
    "om.mls.private_state",
    "om.session.private_state",
    "om.service_controller.private",
    "om.bridge_controller.private",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    SecretType(String),
    /// A segment, a stem or a path outside the section 3.3 grammar.
    BadName,
    TooManyItems,
}

// ---- canonical CBOR (text keys, definite lengths, shortest form) ----

fn head(major: u8, n: u64) -> Vec<u8> {
    let m = major << 5;
    match n {
        0..=23 => vec![m | n as u8],
        24..=0xff => vec![m | 24, n as u8],
        0x100..=0xffff => vec![m | 25, (n >> 8) as u8, n as u8],
        0x1_0000..=0xffff_ffff => {
            let mut v = vec![m | 26];
            v.extend((n as u32).to_be_bytes());
            v
        }
        _ => {
            let mut v = vec![m | 27];
            v.extend(n.to_be_bytes());
            v
        }
    }
}
fn c_uint(n: u64) -> Vec<u8> {
    head(0, n)
}
fn c_text(s: &str) -> Vec<u8> {
    [head(3, s.len() as u64), s.as_bytes().to_vec()].concat()
}
fn c_bytes(b: &[u8]) -> Vec<u8> {
    [head(2, b.len() as u64), b.to_vec()].concat()
}
fn c_array(items: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = head(4, items.len() as u64);
    for i in items {
        out.extend(i);
    }
    out
}
/// Map with keys sorted bytewise by their encoded form (RFC 8949 section 4.2.1).
fn c_map(pairs: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    let mut keyed: Vec<(Vec<u8>, Vec<u8>)> =
        pairs.into_iter().map(|(k, v)| (c_text(k), v)).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = head(5, keyed.len() as u64);
    for (k, v) in keyed {
        out.extend(k);
        out.extend(v);
    }
    out
}
const C_NULL: [u8; 1] = [0xf6];
fn c_bool(b: bool) -> Vec<u8> {
    vec![if b { 0xf5 } else { 0xf4 }]
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---- items ----

/// One payload item to write. Canonical object bytes are exact and never re-encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Object {
        object_type: String,
        bytes: Vec<u8>,
    },
    /// `bytes: None` is a deliberately omitted blob (`present=false`, a partial export).
    Blob {
        blob_id: [u8; 32],
        media_type: String,
        bytes: Option<Vec<u8>>,
        /// Size and hash of the omitted content, kept in the inventory.
        declared: Option<(u64, [u8; 32])>,
    },
    Record {
        record_type: String,
        stem: String,
        bytes: Vec<u8>,
    },
}

/// Manifest fields the exporter chooses. `export_scope` and `extensions` are canonical CBOR supplied by the caller.
#[derive(Clone, Debug)]
pub struct ManifestMeta {
    pub archive_id: [u8; 16],
    pub export_scope: Vec<u8>,
    /// `Some(None)` writes the key as null; `None` omits it.
    pub source_account_hint: Option<Option<String>>,
    pub included_conversations: Option<Vec<String>>,
    pub generated_at_hint: u64,
    pub exporter_implementation: Option<String>,
    pub provenance_profiles: Vec<String>,
}

fn valid_segment(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
        })
}

struct Payload {
    path: String,
    bytes: Vec<u8>,
}

struct Inventory {
    objects: Vec<Vec<u8>>,
    blobs: Vec<Vec<u8>>,
    records: Vec<Vec<u8>>,
    payload: Vec<Payload>,
}

fn build_inventory(items: &[Item]) -> Result<Inventory, WriteError> {
    let mut objects = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut records = BTreeMap::new();
    for it in items {
        match it {
            Item::Object { object_type, bytes } => {
                if SECRET_TYPES.contains(&object_type.as_str()) {
                    return Err(WriteError::SecretType(object_type.clone()));
                }
                let h: [u8; 32] = Sha256::digest(bytes).into();
                let path = format!("canonical/objects/{}", hex(&h));
                let inv = c_map(vec![
                    ("path", c_text(&path)),
                    ("object_type", c_text(object_type)),
                    ("size", c_uint(bytes.len() as u64)),
                    ("sha256", c_bytes(&h)),
                ]);
                objects.insert(
                    path.clone(),
                    (
                        inv,
                        Some(Payload {
                            path,
                            bytes: bytes.clone(),
                        }),
                    ),
                );
            }
            Item::Blob {
                blob_id,
                media_type,
                bytes,
                declared,
            } => {
                let path = format!("blobs/{}", hex(blob_id));
                let (size, h, payload) = match (bytes, declared) {
                    (Some(b), _) => {
                        let h: [u8; 32] = Sha256::digest(b).into();
                        (
                            b.len() as u64,
                            h,
                            Some(Payload {
                                path: path.clone(),
                                bytes: b.clone(),
                            }),
                        )
                    }
                    (None, Some((s, h))) => (*s, *h, None),
                    (None, None) => return Err(WriteError::BadName),
                };
                let inv = c_map(vec![
                    ("path", c_text(&path)),
                    ("blob_id", c_bytes(blob_id)),
                    ("media_type", c_text(media_type)),
                    ("present", c_bool(payload.is_some())),
                    ("size", c_uint(size)),
                    ("sha256", c_bytes(&h)),
                ]);
                blobs.insert(path, (inv, payload));
            }
            Item::Record {
                record_type,
                stem,
                bytes,
            } => {
                if !valid_segment(stem) {
                    return Err(WriteError::BadName);
                }
                let path = format!("records/{stem}.cbor");
                if !valid_segment(&format!("{stem}.cbor")) {
                    return Err(WriteError::BadName);
                }
                let h: [u8; 32] = Sha256::digest(bytes).into();
                let inv = c_map(vec![
                    ("path", c_text(&path)),
                    ("record_type", c_text(record_type)),
                    ("size", c_uint(bytes.len() as u64)),
                    ("sha256", c_bytes(&h)),
                ]);
                records.insert(
                    path.clone(),
                    (
                        inv,
                        Some(Payload {
                            path,
                            bytes: bytes.clone(),
                        }),
                    ),
                );
            }
        }
    }
    let mut payload = Vec::new();
    let mut take = |m: BTreeMap<String, (Vec<u8>, Option<Payload>)>| {
        let mut inv = Vec::new();
        for (_, (i, p)) in m {
            inv.push(i);
            payload.extend(p);
        }
        inv
    };
    let objects = take(objects);
    let blobs = take(blobs);
    let records = take(records);
    Ok(Inventory {
        objects,
        blobs,
        records,
        payload,
    })
}

/// Writes a complete plain archive (format entry, manifest, payload in the specified order).
pub fn write_archive(meta: &ManifestMeta, items: &[Item]) -> Result<Vec<u8>, WriteError> {
    let inv = build_inventory(items)?;
    let objects = c_array(inv.objects);
    let blobs = c_array(inv.blobs);
    let records = c_array(inv.records);
    let mut root = Sha256::new();
    root.update(b"OM-ARCHIVE-v1/integrity-root");
    root.update([0u8]);
    root.update(c_array(vec![
        c_bytes(&meta.archive_id),
        objects.clone(),
        blobs.clone(),
        records.clone(),
    ]));
    let root: [u8; 32] = root.finalize().into();
    let mut m = vec![
        ("format_version", c_text("om-archive-v1")),
        ("archive_id", c_bytes(&meta.archive_id)),
        ("export_scope", meta.export_scope.clone()),
        ("generated_at_hint", c_uint(meta.generated_at_hint)),
        (
            "provenance_profiles",
            c_array(meta.provenance_profiles.iter().map(|p| c_text(p)).collect()),
        ),
        ("encryption_profile", C_NULL.to_vec()),
        ("canonical_object_inventory", objects),
        ("blob_inventory", blobs),
        ("record_inventory", records),
        ("integrity_root", c_bytes(&root)),
        ("critical_extensions", c_array(vec![])),
        ("extensions", c_map(vec![])),
    ];
    if let Some(h) = &meta.source_account_hint {
        m.push((
            "source_account_hint",
            h.as_ref().map_or(C_NULL.to_vec(), |s| c_text(s)),
        ));
    }
    if let Some(c) = &meta.included_conversations {
        m.push((
            "included_conversations",
            c_array(c.iter().map(|s| c_text(s)).collect()),
        ));
    }
    if let Some(e) = &meta.exporter_implementation {
        m.push(("exporter_implementation", c_text(e)));
    }
    let manifest = c_map(m);
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("om-archive-v1/format".into(), b"om-archive-v1\n".to_vec()),
        ("om-archive-v1/manifest.cbor".into(), manifest),
    ];
    for p in inv.payload {
        entries.push((format!("om-archive-v1/{}", p.path), p.bytes));
    }
    Ok(zip(&entries))
}

// ---- strict ZIP ----

fn crc32(b: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(b);
    h.finalize()
}

const MAX32: u64 = 0xFFFF_FFFF;

/// Lays out the entries as the one valid byte sequence of section 3.2. ZIP64 records are used iff required.
fn zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut offsets = Vec::new();
    let mut pos: u64 = 0;
    for (n, d) in entries {
        offsets.push(pos);
        pos += 30 + n.len() as u64 + d.len() as u64;
        // a local header carries a 20-byte ZIP64 extra field when its sizes need it
        if d.len() as u64 >= MAX32 {
            pos += 20;
        }
    }
    let need64 = entries.len() >= 0xFFFF
        || offsets.iter().any(|o| *o >= MAX32)
        || entries.iter().any(|(_, d)| d.len() as u64 >= MAX32)
        || pos >= MAX32;
    let mut out: Vec<u8> = Vec::new();
    let mut real_offsets = Vec::new();
    for (n, d) in entries {
        real_offsets.push(out.len() as u64);
        let big = d.len() as u64 >= MAX32;
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend((if need64 { 45u16 } else { 20u16 }).to_le_bytes());
        out.extend(0x0800u16.to_le_bytes());
        out.extend(0u16.to_le_bytes()); // stored
        out.extend(0u16.to_le_bytes()); // time
        out.extend(0x0021u16.to_le_bytes()); // date 1980-01-01
        out.extend(crc32(d).to_le_bytes());
        let (cs, us) = if big {
            (0xFFFF_FFFFu32, 0xFFFF_FFFFu32)
        } else {
            (d.len() as u32, d.len() as u32)
        };
        out.extend(cs.to_le_bytes());
        out.extend(us.to_le_bytes());
        out.extend((n.len() as u16).to_le_bytes());
        out.extend((if big { 20u16 } else { 0u16 }).to_le_bytes());
        out.extend(n.as_bytes());
        if big {
            out.extend(1u16.to_le_bytes());
            out.extend(16u16.to_le_bytes());
            out.extend((d.len() as u64).to_le_bytes());
            out.extend((d.len() as u64).to_le_bytes());
        }
        out.extend(d);
    }
    let cd_start = out.len() as u64;
    for (i, (n, d)) in entries.iter().enumerate() {
        let size_big = d.len() as u64 >= MAX32;
        let off_big = real_offsets[i] >= MAX32;
        out.extend(0x0201_4b50u32.to_le_bytes());
        out.extend((if need64 { 0x032Du16 } else { 0x0314u16 }).to_le_bytes());
        out.extend((if need64 { 45u16 } else { 20u16 }).to_le_bytes());
        out.extend(0x0800u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0x0021u16.to_le_bytes());
        out.extend(crc32(d).to_le_bytes());
        let sz = if size_big {
            0xFFFF_FFFFu32
        } else {
            d.len() as u32
        };
        out.extend(sz.to_le_bytes());
        out.extend(sz.to_le_bytes());
        out.extend((n.len() as u16).to_le_bytes());
        let extra_len: u16 = (if size_big { 16 } else { 0 }) + (if off_big { 8 } else { 0 });
        out.extend((if extra_len > 0 { extra_len + 4 } else { 0 }).to_le_bytes());
        out.extend(0u16.to_le_bytes()); // comment
        out.extend(0u16.to_le_bytes()); // disk
        out.extend(0u16.to_le_bytes()); // internal attrs
        out.extend((0o100644u32 << 16).to_le_bytes());
        out.extend(
            (if off_big {
                0xFFFF_FFFFu32
            } else {
                real_offsets[i] as u32
            })
            .to_le_bytes(),
        );
        out.extend(n.as_bytes());
        if extra_len > 0 {
            out.extend(1u16.to_le_bytes());
            out.extend(extra_len.to_le_bytes());
            if size_big {
                out.extend((d.len() as u64).to_le_bytes());
                out.extend((d.len() as u64).to_le_bytes());
            }
            if off_big {
                out.extend(real_offsets[i].to_le_bytes());
            }
        }
    }
    let cd_size = out.len() as u64 - cd_start;
    if need64 {
        let eocd64 = out.len() as u64;
        out.extend(0x0606_4b50u32.to_le_bytes());
        out.extend(44u64.to_le_bytes());
        out.extend(0x032Du16.to_le_bytes());
        out.extend(45u16.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend((entries.len() as u64).to_le_bytes());
        out.extend((entries.len() as u64).to_le_bytes());
        out.extend(cd_size.to_le_bytes());
        out.extend(cd_start.to_le_bytes());
        out.extend(0x0706_4b50u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(eocd64.to_le_bytes());
        out.extend(1u32.to_le_bytes());
    }
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    let count = if need64 {
        0xFFFFu16
    } else {
        entries.len() as u16
    };
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(
        (if need64 {
            0xFFFF_FFFFu32
        } else {
            cd_size as u32
        })
        .to_le_bytes(),
    );
    out.extend(
        (if need64 {
            0xFFFF_FFFFu32
        } else {
            cd_start as u32
        })
        .to_le_bytes(),
    );
    out.extend(0u16.to_le_bytes());
    out
}

// ---- selective, staged export ----

/// A thing the app offers for export, with the facts selection needs.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub item: Item,
    pub conversation: Option<String>,
    pub time: Option<u64>,
    /// An item that disappears (carries an expiry).
    pub disappearing: bool,
    /// A ViewOnce item: never exported in any profile.
    pub view_once: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Selection {
    /// `None` selects every conversation (items without a conversation are always eligible).
    pub conversations: Option<BTreeSet<String>>,
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub include_blobs: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Excluded {
    SecretType,
    ViewOnce,
    /// Only a secure export may carry unexpired disappearing items; no encrypted profile is adopted.
    Disappearing,
    NotSelected,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortabilityReport {
    pub objects: usize,
    pub blobs: usize,
    pub records: usize,
    pub omitted_blobs: Vec<String>,
    pub excluded: BTreeMap<Excluded, usize>,
}

/// Where payload items are staged. A real sink writes durably; `stage` is idempotent and `is_durable` says whether an
/// item survived (so an interrupted job can resume and a crash never leaves a manifest ahead of its payload).
pub trait Staging {
    type Error;
    fn stage(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    fn is_durable(&self, path: &str) -> bool;
    fn publish(&mut self, archive: &[u8]) -> Result<(), Self::Error>;
    fn discard(&mut self);
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExportError<E> {
    Write(WriteError),
    Staging(E),
    /// An item that the manifest would name is not durably staged yet: nothing is published.
    NotDurable(String),
}

pub struct ExportJob {
    meta: ManifestMeta,
    items: Vec<Item>,
    pub report: PortabilityReport,
}

fn item_path(i: &Item) -> Option<String> {
    match i {
        Item::Object { bytes, .. } => {
            Some(format!("canonical/objects/{}", hex(&Sha256::digest(bytes))))
        }
        Item::Blob {
            blob_id,
            bytes: Some(_),
            ..
        } => Some(format!("blobs/{}", hex(blob_id))),
        Item::Blob { .. } => None,
        Item::Record { stem, .. } => Some(format!("records/{stem}.cbor")),
    }
}

impl ExportJob {
    pub fn plan(meta: ManifestMeta, candidates: Vec<Candidate>, sel: &Selection) -> ExportJob {
        let mut report = PortabilityReport::default();
        let mut items = Vec::new();
        for c in candidates {
            let secret = matches!(&c.item, Item::Object { object_type, .. } if SECRET_TYPES.contains(&object_type.as_str()));
            let reason = if secret {
                Some(Excluded::SecretType)
            } else if c.view_once {
                Some(Excluded::ViewOnce)
            } else if c.disappearing {
                Some(Excluded::Disappearing)
            } else if !selected(&c, sel) {
                Some(Excluded::NotSelected)
            } else {
                None
            };
            if let Some(r) = reason {
                *report.excluded.entry(r).or_default() += 1;
                continue;
            }
            match &c.item {
                Item::Object { .. } => report.objects += 1,
                Item::Blob { blob_id, bytes, .. } => {
                    report.blobs += 1;
                    if bytes.is_none() {
                        report.omitted_blobs.push(format!("blobs/{}", hex(blob_id)));
                    }
                }
                Item::Record { .. } => report.records += 1,
            }
            items.push(c.item);
        }
        ExportJob {
            meta,
            items,
            report,
        }
    }

    /// Stages every payload item. Safe to call again after an interruption: already durable items are skipped.
    pub fn stage_all<S: Staging>(&self, s: &mut S) -> Result<(), ExportError<S::Error>> {
        for it in &self.items {
            let Some(path) = item_path(it) else { continue };
            if s.is_durable(&path) {
                continue;
            }
            let bytes: &[u8] = match it {
                Item::Object { bytes, .. } | Item::Record { bytes, .. } => bytes,
                Item::Blob { bytes: Some(b), .. } => b,
                Item::Blob { .. } => continue,
            };
            s.stage(&path, bytes).map_err(ExportError::Staging)?;
        }
        Ok(())
    }

    /// Publishes the archive only when every payload item is durably staged; otherwise nothing is published.
    pub fn finish<S: Staging>(
        &self,
        s: &mut S,
    ) -> Result<PortabilityReport, ExportError<S::Error>> {
        for it in &self.items {
            if let Some(path) = item_path(it)
                && !s.is_durable(&path)
            {
                return Err(ExportError::NotDurable(path));
            }
        }
        let archive = write_archive(&self.meta, &self.items).map_err(ExportError::Write)?;
        s.publish(&archive).map_err(ExportError::Staging)?;
        Ok(self.report.clone())
    }
}

fn selected(c: &Candidate, sel: &Selection) -> bool {
    if matches!(c.item, Item::Blob { .. }) && !sel.include_blobs {
        return false;
    }
    if let (Some(wanted), Some(conv)) = (&sel.conversations, &c.conversation)
        && !wanted.contains(conv)
    {
        return false;
    }
    if let Some(t) = c.time
        && (sel.from.is_some_and(|f| t < f) || sel.to.is_some_and(|x| t > x))
    {
        return false;
    }
    true
}
