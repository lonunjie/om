//! om-archive-v1 writer and strict reader (OM_ARCHIVE_V1_SPEC.md §§4-8). Feasibility prototype, not product code.

use crate::cbor::{self, map, CborError, Value};
use crate::crypto::{self, Argon2Params, StreamError};
use crate::zip::{self, Limits, ZipError};
use std::collections::BTreeSet;

pub const FORMAT_VERSION: &str = "om-archive-v1";
pub const ENC_PROFILE: &str = "OMA-ENC-1-candidate";
pub const FORMAT_ENTRY: &str = "om-archive-v1/format";
pub const FORMAT_CONTENT: &[u8] = b"om-archive-v1\n";
pub const MANIFEST_ENTRY: &str = "om-archive-v1/manifest.cbor";
pub const HEADER_ENTRY: &str = "om-archive-v1/header.cbor";
pub const CHUNK_SIZE: u64 = 65536;

/// Spec §5.6: object types that are secret key material and MUST NOT appear in an ordinary archive.
pub const SECRET_OBJECT_TYPES: &[&str] = &[
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
pub enum ArchiveError {
    Zip(ZipError),
    NotOmArchive,
    UnsupportedVersion,
    UnexpectedLayout(&'static str),
    Cbor(&'static str, CborError),
    Schema(String),
    UnknownCritical(String),
    SecretMaterial(String),
    IntegrityRoot,
    EntryHash(String),
    EntrySize(String),
    MissingEntry,
    UnexpectedEntry(String),
    ManifestTooLarge,
    KdfParams,
    NoMatchingKey,
    HeaderMac,
    Decrypt(String),
}

// ---------------------------------------------------------------------------------------------------------------
// Input / output model

#[derive(Clone, Debug)]
pub struct BlobIn {
    pub blob_id: [u8; 32],
    pub media_type: String,
    pub size: u64,
    pub sha256: [u8; 32],
    /// None = deliberately omitted (partial export); the inventory still records size and digest.
    pub data: Option<Vec<u8>>,
}

impl BlobIn {
    pub fn present(blob_id: [u8; 32], media_type: &str, data: Vec<u8>) -> Self {
        BlobIn { blob_id, media_type: media_type.into(), size: data.len() as u64, sha256: crypto::sha256(&[&data]), data: Some(data) }
    }
}

#[derive(Clone, Debug)]
pub struct ArchiveInput {
    pub archive_id: [u8; 16],
    pub export_scope: Value,
    pub source_account_hint: Option<String>,
    pub included_conversations: Vec<String>,
    pub generated_at_hint: u64,
    pub exporter_implementation: String,
    pub provenance_profiles: Vec<String>,
    /// (object_type, exact canonical bytes)
    pub objects: Vec<(String, Vec<u8>)>,
    pub blobs: Vec<BlobIn>,
    /// (record_type, file name stem, CBOR bytes) e.g. ("imported_history", "imported-history", ..)
    pub records: Vec<(String, String, Vec<u8>)>,
}

#[derive(Clone, Debug)]
pub enum Recipient {
    Passphrase { passphrase: Vec<u8>, salt: [u8; 16], params: Argon2Params },
    RawKey { key: [u8; 32] },
}

#[derive(Clone, Debug)]
pub struct Encryption {
    pub archive_key: [u8; 32],
    pub recipients: Vec<Recipient>,
    pub chunk_size: u64,
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub encryption: Option<Encryption>,
    pub force_zip64: bool,
    /// Allow Argon2 parameters below the §6.2 floor (vectors only). INSECURE.
    pub insecure_test_kdf: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyMaterial {
    Passphrase(Vec<u8>),
    RawKey([u8; 32]),
}

#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub limits: Limits,
    pub max_manifest_size: u64,
    pub keys: Vec<KeyMaterial>,
    pub insecure_test_kdf: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions { limits: Limits::default(), max_manifest_size: 64 << 20, keys: vec![], insecure_test_kdf: false }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectOut {
    pub path: String,
    pub object_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobOut {
    pub path: String,
    pub blob_id: [u8; 32],
    pub media_type: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordOut {
    pub path: String,
    pub record_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadReport {
    pub encrypted: bool,
    pub missing_blobs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archive {
    pub manifest: Value,
    pub objects: Vec<ObjectOut>,
    pub blobs: Vec<BlobOut>,
    pub records: Vec<RecordOut>,
    pub report: ReadReport,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn object_path(bytes: &[u8]) -> String {
    format!("canonical/objects/{}", hex(&crypto::sha256(&[bytes])))
}
pub fn blob_path(id: &[u8; 32]) -> String {
    format!("blobs/{}", hex(id))
}
pub fn record_path(stem: &str) -> String {
    format!("records/{stem}.cbor")
}

// ---------------------------------------------------------------------------------------------------------------
// Manifest construction (§5)

fn inventories(inp: &ArchiveInput) -> (Value, Value, Value) {
    let mut objs: Vec<(String, Value)> = inp
        .objects
        .iter()
        .map(|(t, b)| {
            let p = object_path(b);
            (p.clone(), map(vec![("path", Value::Text(p)), ("object_type", Value::text(t)), ("size", Value::Uint(b.len() as u64)),
                                 ("sha256", Value::Bytes(crypto::sha256(&[b]).to_vec()))]))
        })
        .collect();
    objs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut blobs: Vec<(String, Value)> = inp
        .blobs
        .iter()
        .map(|b| {
            let p = blob_path(&b.blob_id);
            (p.clone(), map(vec![("path", Value::Text(p)), ("blob_id", Value::Bytes(b.blob_id.to_vec())), ("media_type", Value::text(&b.media_type)),
                                 ("present", Value::Bool(b.data.is_some())), ("size", Value::Uint(b.size)), ("sha256", Value::Bytes(b.sha256.to_vec()))]))
        })
        .collect();
    blobs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut recs: Vec<(String, Value)> = inp
        .records
        .iter()
        .map(|(t, stem, b)| {
            let p = record_path(stem);
            (p.clone(), map(vec![("path", Value::Text(p)), ("record_type", Value::text(t)), ("size", Value::Uint(b.len() as u64)),
                                 ("sha256", Value::Bytes(crypto::sha256(&[b]).to_vec()))]))
        })
        .collect();
    recs.sort_by(|a, b| a.0.cmp(&b.0));
    let arr = |v: Vec<(String, Value)>| Value::Array(v.into_iter().map(|x| x.1).collect());
    (arr(objs), arr(blobs), arr(recs))
}

pub fn integrity_root(archive_id: &[u8], objs: &Value, blobs: &Value, recs: &Value) -> [u8; 32] {
    let body = cbor::encode(&Value::Array(vec![Value::Bytes(archive_id.to_vec()), objs.clone(), blobs.clone(), recs.clone()]));
    crypto::sha256(&[b"OM-ARCHIVE-v1/integrity-root\0", &body])
}

fn build_manifest(inp: &ArchiveInput, encrypted: bool) -> Value {
    let (o, b, r) = inventories(inp);
    let root = integrity_root(&inp.archive_id, &o, &b, &r);
    map(vec![
        ("format_version", Value::text(FORMAT_VERSION)),
        ("archive_id", Value::Bytes(inp.archive_id.to_vec())),
        ("export_scope", inp.export_scope.clone()),
        ("source_account_hint", inp.source_account_hint.as_deref().map(Value::text).unwrap_or(Value::Null)),
        ("included_conversations", Value::Array(inp.included_conversations.iter().map(|s| Value::text(s)).collect())),
        ("generated_at_hint", Value::Uint(inp.generated_at_hint)),
        ("exporter_implementation", Value::text(&inp.exporter_implementation)),
        ("provenance_profiles", Value::Array(inp.provenance_profiles.iter().map(|s| Value::text(s)).collect())),
        ("encryption_profile", if encrypted { Value::text(ENC_PROFILE) } else { Value::Null }),
        ("canonical_object_inventory", o),
        ("blob_inventory", b),
        ("record_inventory", r),
        ("integrity_root", Value::Bytes(root.to_vec())),
        ("critical_extensions", Value::Array(vec![])),
        ("extensions", Value::Map(vec![])),
    ])
}

/// Payload entries in canonical order (§4.3): objects, present blobs, records - each sorted by path.
fn payload_order(inp: &ArchiveInput) -> Vec<(String, Vec<u8>)> {
    let mut o: Vec<(String, Vec<u8>)> = inp.objects.iter().map(|(_, b)| (object_path(b), b.clone())).collect();
    o.sort();
    let mut b: Vec<(String, Vec<u8>)> = inp.blobs.iter().filter_map(|x| x.data.as_ref().map(|d| (blob_path(&x.blob_id), d.clone()))).collect();
    b.sort();
    let mut r: Vec<(String, Vec<u8>)> = inp.records.iter().map(|(_, s, d)| (record_path(s), d.clone())).collect();
    r.sort();
    o.into_iter().chain(b).chain(r).collect()
}

fn entry_key(k: &[u8; 32], archive_id: &[u8], index: u64) -> [u8; 32] {
    crypto::hkdf(k, archive_id, &[b"OM-ARCHIVE-v1 entry".as_slice(), &index.to_be_bytes()].concat())
}
fn header_mac_key(k: &[u8; 32], archive_id: &[u8]) -> [u8; 32] {
    crypto::hkdf(k, archive_id, b"OM-ARCHIVE-v1 header-mac")
}
fn raw_kek(raw: &[u8; 32], archive_id: &[u8]) -> [u8; 32] {
    crypto::hkdf(raw, archive_id, b"OM-ARCHIVE-v1 raw-kek")
}
fn wrap_aad(archive_id: &[u8]) -> Vec<u8> {
    [b"OM-ARCHIVE-v1 wrap\0".as_slice(), archive_id].concat()
}
pub fn raw_key_id(raw: &[u8; 32]) -> [u8; 8] {
    crypto::sha256(&[b"OM-ARCHIVE-v1 raw-key-id\0", raw])[..8].try_into().unwrap()
}
pub fn encrypted_entry_name(i: u64) -> String {
    format!("om-archive-v1/e/{i:08}")
}

pub fn write(inp: &ArchiveInput, opt: &WriteOptions) -> Vec<u8> {
    let mut entries = vec![(FORMAT_ENTRY.to_string(), FORMAT_CONTENT.to_vec())];
    let payload = payload_order(inp);
    match &opt.encryption {
        None => {
            entries.push((MANIFEST_ENTRY.to_string(), cbor::encode(&build_manifest(inp, false))));
            for (p, d) in payload {
                entries.push((format!("{}{p}", zip::PREFIX), d));
            }
        }
        Some(enc) => {
            let aid = &inp.archive_id;
            let recips: Vec<Value> = enc
                .recipients
                .iter()
                .map(|r| match r {
                    Recipient::Passphrase { passphrase, salt, params } => {
                        assert!(crypto::argon2_params_acceptable(*params, opt.insecure_test_kdf), "argon2 params below floor");
                        let kek = crypto::argon2id(passphrase, salt, *params);
                        map(vec![("type", Value::text("passphrase-argon2id")), ("salt", Value::Bytes(salt.to_vec())),
                                 ("m_kib", Value::Uint(params.m_kib as u64)), ("t", Value::Uint(params.t as u64)), ("p", Value::Uint(params.p as u64)),
                                 ("wrapped_key", Value::Bytes(crypto::wrap(&kek, &wrap_aad(aid), &enc.archive_key)))])
                    }
                    Recipient::RawKey { key } => map(vec![("type", Value::text("raw-key")), ("key_id", Value::Bytes(raw_key_id(key).to_vec())),
                                                         ("wrapped_key", Value::Bytes(crypto::wrap(&raw_kek(key, aid), &wrap_aad(aid), &enc.archive_key)))]),
                })
                .collect();
            let core = vec![("format_version", Value::text(FORMAT_VERSION)), ("profile", Value::text(ENC_PROFILE)),
                            ("archive_id", Value::Bytes(aid.to_vec())), ("chunk_size", Value::Uint(enc.chunk_size)), ("recipients", Value::Array(recips))];
            let mac = crypto::hmac(&header_mac_key(&enc.archive_key, aid), &cbor::encode(&map(core.clone())));
            let mut hdr = core;
            hdr.push(("header_mac", Value::Bytes(mac.to_vec())));
            entries.push((HEADER_ENTRY.to_string(), cbor::encode(&map(hdr))));
            let cs = enc.chunk_size as usize;
            let manifest = cbor::encode(&build_manifest(inp, true));
            entries.push((encrypted_entry_name(0), crypto::stream_encrypt(&entry_key(&enc.archive_key, aid, 0), cs, &manifest)));
            for (i, (_, d)) in payload.into_iter().enumerate() {
                let idx = i as u64 + 1;
                entries.push((encrypted_entry_name(idx), crypto::stream_encrypt(&entry_key(&enc.archive_key, aid, idx), cs, &d)));
            }
        }
    }
    zip::write(&entries, opt.force_zip64)
}

// ---------------------------------------------------------------------------------------------------------------
// Reader (§7)

fn schema(msg: impl Into<String>) -> ArchiveError {
    ArchiveError::Schema(msg.into())
}

/// Exact key-set check: every required key present, no key outside required ∪ optional.
fn keys_exact(v: &Value, what: &str, required: &[&str], optional: &[&str]) -> Result<(), ArchiveError> {
    let Value::Map(_) = v else { return Err(schema(format!("{what}: not a map"))) };
    let ks: BTreeSet<&str> = v.keys().into_iter().collect();
    if ks.len() != match v { Value::Map(m) => m.len(), _ => 0 } {
        return Err(schema(format!("{what}: non-text key")));
    }
    for r in required {
        if !ks.contains(r) {
            return Err(schema(format!("{what}: missing {r}")));
        }
    }
    for k in &ks {
        if !required.contains(k) && !optional.contains(k) {
            return Err(schema(format!("{what}: unknown field {k}")));
        }
    }
    Ok(())
}

fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, ArchiveError> {
    v.get(k).and_then(|x| x.as_text()).ok_or_else(|| schema(format!("{k}: expected text")))
}
fn uint(v: &Value, k: &str) -> Result<u64, ArchiveError> {
    v.get(k).and_then(|x| x.as_u64()).ok_or_else(|| schema(format!("{k}: expected uint")))
}
fn bytes_n<const N: usize>(v: &Value, k: &str) -> Result<[u8; N], ArchiveError> {
    v.get(k).and_then(|x| x.as_bytes()).and_then(|b| b.try_into().ok()).ok_or_else(|| schema(format!("{k}: expected {N} bytes")))
}
fn arr<'a>(v: &'a Value, k: &str) -> Result<&'a [Value], ArchiveError> {
    v.get(k).and_then(|x| x.as_array()).ok_or_else(|| schema(format!("{k}: expected array")))
}
fn text_array(v: &Value, k: &str) -> Result<(), ArchiveError> {
    if arr(v, k)?.iter().all(|x| x.as_text().is_some()) { Ok(()) } else { Err(schema(format!("{k}: expected text array"))) }
}

fn check_path(p: &str, prefix: &str) -> Result<(), ArchiveError> {
    let full = format!("{}{p}", zip::PREFIX);
    if !p.starts_with(prefix) || !zip::valid_name(full.as_bytes()) {
        return Err(schema(format!("bad logical path {p}")));
    }
    Ok(())
}

struct Inv {
    path: String,
    size: u64,
    sha: [u8; 32],
}

struct Manifest {
    value: Value,
    objects: Vec<(Inv, String)>,
    blobs: Vec<(Inv, [u8; 32], String, bool)>,
    records: Vec<(Inv, String)>,
}

fn parse_manifest(bytes: &[u8], encrypted: bool, archive_id_hdr: Option<[u8; 16]>) -> Result<Manifest, ArchiveError> {
    let v = cbor::decode(bytes).map_err(|e| ArchiveError::Cbor("manifest", e))?;
    let Value::Map(_) = v else { return Err(schema("manifest: not a map")) };
    // version first: a future major version must be reported as such, not as a schema error
    match v.get("format_version").and_then(|x| x.as_text()) {
        Some(FORMAT_VERSION) => {}
        Some(_) => return Err(ArchiveError::UnsupportedVersion),
        None => return Err(schema("format_version missing")),
    }
    if let Some(ce) = v.get("critical_extensions") {
        let ce = ce.as_array().ok_or_else(|| schema("critical_extensions: expected array"))?;
        if let Some(x) = ce.first() {
            return Err(ArchiveError::UnknownCritical(x.as_text().unwrap_or("?").to_string()));
        }
    }
    keys_exact(&v, "manifest",
        &["format_version", "archive_id", "export_scope", "generated_at_hint", "provenance_profiles", "encryption_profile",
          "canonical_object_inventory", "blob_inventory", "record_inventory", "integrity_root", "critical_extensions", "extensions"],
        &["source_account_hint", "included_conversations", "exporter_implementation"])?;
    let aid: [u8; 16] = bytes_n(&v, "archive_id")?;
    if archive_id_hdr.is_some_and(|h| h != aid) {
        return Err(schema("archive_id differs from header"));
    }
    if !matches!(v.get("export_scope"), Some(Value::Map(_))) || !matches!(v.get("extensions"), Some(Value::Map(_))) {
        return Err(schema("export_scope/extensions: expected map"));
    }
    uint(&v, "generated_at_hint")?;
    text_array(&v, "provenance_profiles")?;
    if v.get("included_conversations").is_some() {
        text_array(&v, "included_conversations")?;
    }
    for k in ["source_account_hint", "exporter_implementation"] {
        if let Some(x) = v.get(k) {
            if !(x.as_text().is_some() || (k == "source_account_hint" && *x == Value::Null)) {
                return Err(schema(format!("{k}: bad type")));
            }
        }
    }
    match (v.get("encryption_profile"), encrypted) {
        (Some(Value::Null), false) => {}
        (Some(Value::Text(t)), true) if t == ENC_PROFILE => {}
        _ => return Err(schema("encryption_profile does not match container mode")),
    }
    let inv = |item: &Value, prefix: &str| -> Result<Inv, ArchiveError> {
        let path = text(item, "path")?.to_string();
        check_path(&path, prefix)?;
        Ok(Inv { path, size: uint(item, "size")?, sha: bytes_n(item, "sha256")? })
    };
    let sorted = |ps: Vec<&str>| ps.windows(2).all(|w| w[0] < w[1]);
    let mut objects = vec![];
    for it in arr(&v, "canonical_object_inventory")? {
        keys_exact(it, "object entry", &["path", "object_type", "size", "sha256"], &[])?;
        let i = inv(it, "canonical/objects/")?;
        if i.path != format!("canonical/objects/{}", hex(&i.sha)) {
            return Err(schema("object path is not its sha256"));
        }
        let t = text(it, "object_type")?.to_string();
        if SECRET_OBJECT_TYPES.contains(&t.as_str()) {
            return Err(ArchiveError::SecretMaterial(t));
        }
        objects.push((i, t));
    }
    let mut blobs = vec![];
    for it in arr(&v, "blob_inventory")? {
        keys_exact(it, "blob entry", &["path", "blob_id", "media_type", "present", "size", "sha256"], &[])?;
        let i = inv(it, "blobs/")?;
        let id: [u8; 32] = bytes_n(it, "blob_id")?;
        if i.path != blob_path(&id) {
            return Err(schema("blob path does not match blob_id"));
        }
        let present = it.get("present").and_then(|x| x.as_bool()).ok_or_else(|| schema("present: expected bool"))?;
        blobs.push((i, id, text(it, "media_type")?.to_string(), present));
    }
    let mut records = vec![];
    for it in arr(&v, "record_inventory")? {
        keys_exact(it, "record entry", &["path", "record_type", "size", "sha256"], &[])?;
        records.push((inv(it, "records/")?, text(it, "record_type")?.to_string()));
    }
    if !sorted(objects.iter().map(|x| x.0.path.as_str()).collect())
        || !sorted(blobs.iter().map(|x| x.0.path.as_str()).collect())
        || !sorted(records.iter().map(|x| x.0.path.as_str()).collect())
    {
        return Err(schema("inventory not sorted by path or duplicate path"));
    }
    let root = integrity_root(&aid, v.get("canonical_object_inventory").unwrap(), v.get("blob_inventory").unwrap(), v.get("record_inventory").unwrap());
    if bytes_n::<32>(&v, "integrity_root")? != root {
        return Err(ArchiveError::IntegrityRoot);
    }
    Ok(Manifest { value: v, objects, blobs, records })
}

struct Header {
    archive_id: [u8; 16],
    chunk_size: u64,
    key: [u8; 32],
}

fn open_header(bytes: &[u8], opt: &ReadOptions) -> Result<Header, ArchiveError> {
    let v = cbor::decode(bytes).map_err(|e| ArchiveError::Cbor("header", e))?;
    match v.get("format_version").and_then(|x| x.as_text()) {
        Some(FORMAT_VERSION) => {}
        Some(_) => return Err(ArchiveError::UnsupportedVersion),
        None => return Err(schema("header: format_version missing")),
    }
    keys_exact(&v, "header", &["format_version", "profile", "archive_id", "chunk_size", "recipients", "header_mac"], &[])?;
    if text(&v, "profile")? != ENC_PROFILE {
        return Err(ArchiveError::UnsupportedVersion);
    }
    let aid: [u8; 16] = bytes_n(&v, "archive_id")?;
    let cs = uint(&v, "chunk_size")?;
    if cs != CHUNK_SIZE {
        return Err(schema("chunk_size must be 65536"));
    }
    let mac: [u8; 32] = bytes_n(&v, "header_mac")?;
    let recips = arr(&v, "recipients")?;
    if recips.is_empty() || recips.len() > 16 {
        return Err(schema("recipients: 1..16 required"));
    }
    let aad = wrap_aad(&aid);
    let mut found: Option<[u8; 32]> = None;
    for r in recips {
        match text(r, "type")? {
            "passphrase-argon2id" => {
                keys_exact(r, "recipient", &["type", "salt", "m_kib", "t", "p", "wrapped_key"], &[])?;
                let salt: [u8; 16] = bytes_n(r, "salt")?;
                let p = |k| uint(r, k).and_then(|x| u32::try_from(x).map_err(|_| ArchiveError::KdfParams));
                let params = Argon2Params { m_kib: p("m_kib")?, t: p("t")?, p: p("p")? };
                if !crypto::argon2_params_acceptable(params, opt.insecure_test_kdf) {
                    return Err(ArchiveError::KdfParams); // checked before running the KDF: no DoS, no downgrade
                }
                let w: [u8; 48] = bytes_n(r, "wrapped_key")?;
                if found.is_none() {
                    for k in &opt.keys {
                        if let KeyMaterial::Passphrase(pw) = k {
                            if let Some(key) = crypto::unwrap(&crypto::argon2id(pw, &salt, params), &aad, &w) {
                                found = Some(key);
                                break;
                            }
                        }
                    }
                }
            }
            "raw-key" => {
                keys_exact(r, "recipient", &["type", "key_id", "wrapped_key"], &[])?;
                let kid: [u8; 8] = bytes_n(r, "key_id")?;
                let w: [u8; 48] = bytes_n(r, "wrapped_key")?;
                if found.is_none() {
                    for k in &opt.keys {
                        if let KeyMaterial::RawKey(raw) = k {
                            if raw_key_id(raw) == kid {
                                if let Some(key) = crypto::unwrap(&raw_kek(raw, &aid), &aad, &w) {
                                    found = Some(key);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            other => return Err(ArchiveError::UnknownCritical(format!("recipient type {other}"))),
        }
    }
    let key = found.ok_or(ArchiveError::NoMatchingKey)?;
    let core = match &v {
        Value::Map(m) => Value::Map(m.iter().filter(|(k, _)| k.as_text() != Some("header_mac")).cloned().collect()),
        _ => unreachable!(),
    };
    if !crypto::hmac_verify(&header_mac_key(&key, &aid), &cbor::encode(&core), &mac) {
        return Err(ArchiveError::HeaderMac);
    }
    Ok(Header { archive_id: aid, chunk_size: cs, key })
}

pub fn read(bytes: &[u8], opt: &ReadOptions) -> Result<Archive, ArchiveError> {
    let entries = zip::read(bytes, &opt.limits).map_err(ArchiveError::Zip)?;
    if entries.first().map(|e| e.name.as_str()) != Some(FORMAT_ENTRY) {
        return Err(ArchiveError::NotOmArchive);
    }
    if entries[0].data != FORMAT_CONTENT {
        return Err(if entries[0].data.starts_with(b"om-archive-v") { ArchiveError::UnsupportedVersion } else { ArchiveError::NotOmArchive });
    }
    let second = entries.get(1).ok_or(ArchiveError::UnexpectedLayout("missing manifest/header"))?;
    let encrypted = match second.name.as_str() {
        MANIFEST_ENTRY => false,
        HEADER_ENTRY => true,
        _ => return Err(ArchiveError::UnexpectedLayout("second entry must be manifest.cbor or header.cbor")),
    };
    let mut payload: Vec<Vec<u8>> = Vec::new();
    let man;
    if !encrypted {
        if second.data.len() as u64 > opt.max_manifest_size {
            return Err(ArchiveError::ManifestTooLarge);
        }
        man = parse_manifest(second.data, false, None)?;
        let expected: Vec<String> = expected_paths(&man);
        if entries.len() - 2 != expected.len() {
            return Err(if entries.len() - 2 < expected.len() { ArchiveError::MissingEntry } else { ArchiveError::UnexpectedEntry(entries.last().unwrap().name.clone()) });
        }
        for (e, p) in entries[2..].iter().zip(&expected) {
            if e.name != format!("{}{p}", zip::PREFIX) {
                return Err(ArchiveError::UnexpectedEntry(e.name.clone()));
            }
            payload.push(e.data.to_vec());
        }
    } else {
        let h = open_header(second.data, opt)?;
        let cs = h.chunk_size as usize;
        let e0 = entries.get(2).ok_or(ArchiveError::MissingEntry)?;
        if e0.name != encrypted_entry_name(0) {
            return Err(ArchiveError::UnexpectedEntry(e0.name.clone()));
        }
        if e0.data.len() as u64 > opt.max_manifest_size + (opt.max_manifest_size / CHUNK_SIZE + 1) * 16 {
            return Err(ArchiveError::ManifestTooLarge);
        }
        let mbytes = crypto::stream_decrypt(&entry_key(&h.key, &h.archive_id, 0), cs, e0.data)
            .map_err(|e| ArchiveError::Decrypt(format!("manifest: {e:?}")))?;
        man = parse_manifest(&mbytes, true, Some(h.archive_id))?;
        let expected = expected_paths(&man);
        if entries.len() - 3 != expected.len() {
            return Err(if entries.len() - 3 < expected.len() { ArchiveError::MissingEntry } else { ArchiveError::UnexpectedEntry(entries.last().unwrap().name.clone()) });
        }
        for (i, (e, p)) in entries[3..].iter().zip(&expected).enumerate() {
            let idx = i as u64 + 1;
            if e.name != encrypted_entry_name(idx) {
                return Err(ArchiveError::UnexpectedEntry(e.name.clone()));
            }
            let pt = crypto::stream_decrypt(&entry_key(&h.key, &h.archive_id, idx), cs, e.data).map_err(|err| {
                ArchiveError::Decrypt(format!("{p}: {}", match err { StreamError::Auth => "authentication failed", StreamError::Length => "bad length" }))
            })?;
            payload.push(pt);
        }
    }
    // verify every payload entry against the manifest inventory
    let mut it = payload.into_iter();
    let mut check = |inv: &Inv| -> Result<Vec<u8>, ArchiveError> {
        let d = it.next().ok_or(ArchiveError::MissingEntry)?;
        if d.len() as u64 != inv.size {
            return Err(ArchiveError::EntrySize(inv.path.clone()));
        }
        if crypto::sha256(&[&d]) != inv.sha {
            return Err(ArchiveError::EntryHash(inv.path.clone()));
        }
        Ok(d)
    };
    let mut out = Archive { manifest: man.value.clone(), objects: vec![], blobs: vec![], records: vec![], report: ReadReport { encrypted, ..Default::default() } };
    for (inv, t) in &man.objects {
        out.objects.push(ObjectOut { path: inv.path.clone(), object_type: t.clone(), bytes: check(inv)? });
    }
    for (inv, id, mt, present) in &man.blobs {
        let data = if *present { Some(check(inv)?) } else { out.report.missing_blobs.push(inv.path.clone()); None };
        out.blobs.push(BlobOut { path: inv.path.clone(), blob_id: *id, media_type: mt.clone(), size: inv.size, sha256: inv.sha, data });
    }
    for (inv, t) in &man.records {
        out.records.push(RecordOut { path: inv.path.clone(), record_type: t.clone(), bytes: check(inv)? });
    }
    Ok(out)
}

fn expected_paths(m: &Manifest) -> Vec<String> {
    m.objects.iter().map(|x| x.0.path.clone())
        .chain(m.blobs.iter().filter(|x| x.3).map(|x| x.0.path.clone()))
        .chain(m.records.iter().map(|x| x.0.path.clone()))
        .collect()
}

/// Rebuild the writer input from a read archive (round-trip / re-export).
pub fn to_input(a: &Archive) -> ArchiveInput {
    let m = &a.manifest;
    ArchiveInput {
        archive_id: m.get("archive_id").and_then(|x| x.as_bytes()).unwrap().try_into().unwrap(),
        export_scope: m.get("export_scope").unwrap().clone(),
        source_account_hint: m.get("source_account_hint").and_then(|x| x.as_text()).map(String::from),
        included_conversations: m.get("included_conversations").and_then(|x| x.as_array()).map(|a| a.iter().map(|s| s.as_text().unwrap().to_string()).collect()).unwrap_or_default(),
        generated_at_hint: m.get("generated_at_hint").and_then(|x| x.as_u64()).unwrap(),
        exporter_implementation: m.get("exporter_implementation").and_then(|x| x.as_text()).unwrap_or("").to_string(),
        provenance_profiles: m.get("provenance_profiles").and_then(|x| x.as_array()).unwrap().iter().map(|s| s.as_text().unwrap().to_string()).collect(),
        objects: a.objects.iter().map(|o| (o.object_type.clone(), o.bytes.clone())).collect(),
        blobs: a.blobs.iter().map(|b| BlobIn { blob_id: b.blob_id, media_type: b.media_type.clone(), size: b.size, sha256: b.sha256, data: b.data.clone() }).collect(),
        records: a.records.iter().map(|r| (r.record_type.clone(), r.path.trim_start_matches("records/").trim_end_matches(".cbor").to_string(), r.bytes.clone())).collect(),
    }
}
