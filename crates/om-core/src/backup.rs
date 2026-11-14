// SPDX-License-Identifier: MPL-2.0
//! The OM chat-history backup file (`OMB-1`, ADR-0184): one profile's conversations and messages, sealed under a
//! key the host derived from a password. This is the history archive of IMPORT_EXPORT_AND_USER_ESCAPE.md sections 7-8
//! in its smallest useful form, NOT the OM Portable Archive and NOT the verification-only `OMA-ENC-1-candidate`.
//!
//! * **Never contains authority.** No account, recovery, device, MLS, session or controller private key, no delivery
//!   key, no route, no outbox: the body is built from an allow-list of display fields (`collect`), so a field added to
//!   the profile state later is NOT exported by accident.
//! * **Importing grants nothing.** A restored conversation is a read-only [`RestoredChat`]; it has no relationship,
//!   key, mailbox or outbox entry, so it can show old messages and nothing else (`apply_import`).
//! * **Sans-IO and dependency-light.** Argon2id runs in the host (the web vault already ships `@noble/hashes`); Core
//!   takes the 32-byte output as `master` and does the rest with primitives already in the workspace
//!   (HKDF-SHA-256, XChaCha20-Poly1305).
//!
//! File layout (all integers big endian; 84-byte header, then the AEAD ciphertext):
//!
//! ```text
//!  0..8   "OMBACKUP"        magic
//!  8      version = 1       any other value: UnsupportedVersion
//!  9      profile = 1       OMB-ENC-1 = Argon2id + HKDF-SHA-256 + XChaCha20-Poly1305; any other: UnsupportedProfile
//! 10..12  flags = 0         reserved, must be zero
//! 12..24  Argon2id m (KiB), t, p            u32 each
//! 24..40  salt (16 random bytes)            fresh per file
//! 40..64  XChaCha20-Poly1305 nonce (24 random bytes)  fresh per file
//! 64..80  key check (16 bytes)              HKDF output, tells a wrong password from a damaged file
//! 80..84  ciphertext length                 u32, includes the 16-byte tag
//! 84..    ciphertext                        AAD = bytes 0..84
//! ```
//!
//! Keys: `prk = HKDF-Extract(salt, master)`; `aead_key = Expand(prk, "OM-BACKUP-v1 aead key", 32)`;
//! `check = Expand(prk, "OM-BACKUP-v1 key check", 16)`.

use crate::api::DeliveryState;
use crate::crypto::cbor::{self, V};
use crate::crypto::kdf::hkdf_expand;
use crate::state::{AppState, Hex, RelState, RestoredChat, RestoredMsg};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeSet;
use zeroize::Zeroize;

pub const MAGIC: &[u8; 8] = b"OMBACKUP";
pub const FORMAT_VERSION: u8 = 1;
/// `OMB-ENC-1`: Argon2id (host) + HKDF-SHA-256 + XChaCha20-Poly1305.
pub const PROFILE_ENC_1: u8 = 1;
pub const HEADER_LEN: usize = 84;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;
const CHECK_LEN: usize = 16;
const TAG_LEN: usize = 16;

/// The largest file Core writes or reads. The host bridge passes bytes as hex in one JSON command (16 MiB input
/// limit), and history is text, so 6 MiB holds on the order of a hundred thousand messages.
pub const MAX_FILE_BYTES: usize = 6 << 20;
pub const MAX_CHATS: usize = 10_000;
pub const MAX_MESSAGES: usize = 250_000;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_NAME_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 128;

const AEAD_LABEL: &[u8] = b"OM-BACKUP-v1 aead key";
const CHECK_LABEL: &[u8] = b"OM-BACKUP-v1 key check";
const BODY_TYPE: &str = "om.history-backup";
const BODY_VERSION: u64 = 1;

// ------------------------------------------------------------------------------------------------ errors

/// Why a backup could not be written or read. Distinct codes, so the UI can say the right thing in plain words.
/// None of them leaves anything half imported: a read either returns the whole body or an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupError {
    /// Not an OM backup file (the first bytes are not the magic).
    NotABackup,
    /// A backup file from a newer (or unknown) format version.
    UnsupportedVersion(u8),
    /// A known version with an encryption profile this build does not have.
    UnsupportedProfile(u8),
    /// Reserved header bits or password-hashing settings outside what this build accepts (too weak or too costly).
    UnsupportedParameters,
    /// The file ends before the header or the ciphertext does.
    Truncated,
    /// The file has the right start but its structure is wrong (extra bytes, impossible lengths).
    Malformed,
    /// The key does not match this file: a wrong password, or the salt / password-hashing settings were altered.
    WrongPassword,
    /// The key matched, but the contents or the header were altered or damaged after the file was written.
    Tampered,
    /// Authentic, but the contents are not a valid history (bad structure, limits exceeded).
    InvalidContent,
    /// More history than one backup file or one restored profile may hold.
    TooLarge,
}

impl BackupError {
    pub fn code(self) -> &'static str {
        match self {
            BackupError::NotABackup => "not_a_backup",
            BackupError::UnsupportedVersion(_) => "unsupported_version",
            BackupError::UnsupportedProfile(_) => "unsupported_profile",
            BackupError::UnsupportedParameters => "unsupported_parameters",
            BackupError::Truncated => "truncated",
            BackupError::Malformed => "malformed",
            BackupError::WrongPassword => "wrong_password",
            BackupError::Tampered => "tampered",
            BackupError::InvalidContent => "invalid_content",
            BackupError::TooLarge => "too_large",
        }
    }
}

impl core::fmt::Display for BackupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "backup {}", self.code())
    }
}
impl std::error::Error for BackupError {}

// ------------------------------------------------------------------------------------------------ password hashing

/// Argon2id settings the host must use. The file records them, so a reader never guesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kdf {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

/// What a writer uses: the same 64 MiB, 3 passes the web vault's passphrase layer uses (`policy.standard`). Builds with
/// the test RNG (never production, R-072) use a token cost so tests and the e2e build stay fast.
pub const fn default_kdf() -> Kdf {
    if cfg!(feature = "insecure-test-rng") {
        Kdf {
            m_kib: 64,
            t: 1,
            p: 1,
        }
    } else {
        Kdf {
            m_kib: 65_536,
            t: 3,
            p: 1,
        }
    }
}

/// Floor and ceiling are checked before the host spends any time or memory (a hostile header must neither
/// downgrade the protection nor ask for gigabytes). `relaxed` is the test-build floor.
pub fn kdf_in_bounds(k: &Kdf, relaxed: bool) -> bool {
    let (min_m, min_t) = if relaxed { (8, 1) } else { (65_536, 3) };
    (1..=4).contains(&k.p)
        && k.m_kib >= min_m.max(8 * k.p)
        && k.m_kib <= 262_144
        && (min_t..=10).contains(&k.t)
}

fn bounds_ok(k: &Kdf) -> bool {
    kdf_in_bounds(k, cfg!(feature = "insecure-test-rng"))
}

// ------------------------------------------------------------------------------------------------ the sealed file

/// The settings and salt for the next file; the host derives the key with them (`Core::backup_plan`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPlan {
    pub kdf: Kdf,
    #[serde(with = "crate::host::hexs")]
    pub salt: Vec<u8>,
}

/// What the host needs before it can derive the key: everything is public header data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHeader {
    pub version: u8,
    pub profile: u8,
    pub kdf: Kdf,
    #[serde(with = "crate::host::hexs")]
    pub salt: Vec<u8>,
    pub ciphertext_len: u32,
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Parse and validate the header without the key. Order matters and is part of the format: magic, version, profile,
/// reserved bits, password-hashing bounds, then the declared and the actual length.
pub fn inspect(file: &[u8]) -> Result<BackupHeader, BackupError> {
    let magic_seen = &file[..file.len().min(MAGIC.len())];
    if !MAGIC.starts_with(magic_seen) {
        return Err(BackupError::NotABackup);
    }
    if file.len() < HEADER_LEN {
        return Err(BackupError::Truncated);
    }
    if file[8] != FORMAT_VERSION {
        return Err(BackupError::UnsupportedVersion(file[8]));
    }
    if file[9] != PROFILE_ENC_1 {
        return Err(BackupError::UnsupportedProfile(file[9]));
    }
    if file[10] != 0 || file[11] != 0 {
        return Err(BackupError::UnsupportedParameters);
    }
    let kdf = Kdf {
        m_kib: be32(&file[12..16]),
        t: be32(&file[16..20]),
        p: be32(&file[20..24]),
    };
    if !bounds_ok(&kdf) {
        return Err(BackupError::UnsupportedParameters);
    }
    let ct_len = be32(&file[80..84]);
    if (ct_len as usize) < TAG_LEN || HEADER_LEN + ct_len as usize > MAX_FILE_BYTES {
        return Err(BackupError::Malformed);
    }
    let total = HEADER_LEN + ct_len as usize;
    if file.len() < total {
        return Err(BackupError::Truncated);
    }
    if file.len() > total {
        return Err(BackupError::Malformed);
    }
    Ok(BackupHeader {
        version: file[8],
        profile: file[9],
        kdf,
        salt: file[24..40].to_vec(),
        ciphertext_len: ct_len,
    })
}

struct FileKeys {
    aead: [u8; 32],
    check: [u8; CHECK_LEN],
}

impl Drop for FileKeys {
    fn drop(&mut self) {
        self.aead.zeroize();
        self.check.zeroize();
    }
}

fn file_keys(master: &[u8; 32], salt: &[u8]) -> FileKeys {
    let (prk, _) = Hkdf::<Sha256>::extract(Some(salt), master);
    let mut prk32 = [0u8; 32];
    prk32.copy_from_slice(&prk);
    let mut k = FileKeys {
        aead: [0; 32],
        check: [0; CHECK_LEN],
    };
    hkdf_expand(&prk32, AEAD_LABEL, &mut k.aead);
    hkdf_expand(&prk32, CHECK_LABEL, &mut k.check);
    prk32.zeroize();
    k
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Seal `body` into a backup file. Deterministic in its inputs; the caller supplies fresh random `salt` and `nonce`
/// (Core draws them from its CSPRNG) and the host-derived `master`.
pub fn seal(
    body: &[u8],
    kdf: Kdf,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; NONCE_LEN],
    master: &[u8; 32],
) -> Result<Vec<u8>, BackupError> {
    if !bounds_ok(&kdf) {
        return Err(BackupError::UnsupportedParameters);
    }
    let ct_len = body.len() + TAG_LEN;
    if HEADER_LEN + ct_len > MAX_FILE_BYTES {
        return Err(BackupError::TooLarge);
    }
    let keys = file_keys(master, salt);
    let mut head = Vec::with_capacity(HEADER_LEN);
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&[FORMAT_VERSION, PROFILE_ENC_1, 0, 0]);
    for n in [kdf.m_kib, kdf.t, kdf.p] {
        head.extend_from_slice(&n.to_be_bytes());
    }
    head.extend_from_slice(salt);
    head.extend_from_slice(nonce);
    head.extend_from_slice(&keys.check);
    head.extend_from_slice(&(ct_len as u32).to_be_bytes());
    debug_assert_eq!(head.len(), HEADER_LEN);
    let cipher = XChaCha20Poly1305::new((&keys.aead).into());
    let ct = cipher
        .encrypt(
            nonce.into(),
            Payload {
                msg: body,
                aad: &head,
            },
        )
        .map_err(|_| BackupError::Malformed)?;
    head.extend_from_slice(&ct);
    Ok(head)
}

/// Authenticate and decrypt. Nothing is returned unless every byte of header and ciphertext verified.
pub fn open(file: &[u8], master: &[u8; 32]) -> Result<Vec<u8>, BackupError> {
    inspect(file)?;
    let keys = file_keys(master, &file[24..40]);
    if !ct_eq(&keys.check, &file[64..80]) {
        return Err(BackupError::WrongPassword);
    }
    let nonce: [u8; NONCE_LEN] = file[40..64]
        .try_into()
        .map_err(|_| BackupError::Malformed)?;
    let cipher = XChaCha20Poly1305::new((&keys.aead).into());
    cipher
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &file[HEADER_LEN..],
                aad: &file[..HEADER_LEN],
            },
        )
        .map_err(|_| BackupError::Tampered)
}

// ------------------------------------------------------------------------------------------------ the body

/// One message as the backup keeps it. `delivery`: 0 never left this device, 1 sent, 2 delivered, 3 refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedMsg {
    pub id: String,
    pub outgoing: bool,
    pub text: String,
    pub sent_at: u64,
    pub delivery: u8,
    pub clock_ms: u64,
    pub clock_logical: u32,
    /// SHA-256 (hex) of the canonical event that created the message, kept as evidence; empty when unknown.
    pub digest: Hex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedChat {
    pub id: String,
    pub display_name: String,
    /// The other person's AccountID as this device knew it (public; empty when unknown). A hint, never trusted.
    pub peer_account_id: Hex,
    pub created_at: u64,
    pub canon_id: Hex,
    pub messages: Vec<ArchivedMsg>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryArchive {
    /// Wall clock when the file was written: a display hint only.
    pub exported_at_ms: u64,
    /// The profile's own display name at export time (a hint for the person choosing a file).
    pub owner_label: String,
    pub chats: Vec<ArchivedChat>,
}

fn delivery_code(d: &DeliveryState) -> u8 {
    match d {
        DeliveryState::PendingNetwork | DeliveryState::PendingReconciliation => 0,
        DeliveryState::Sent => 1,
        DeliveryState::Delivered => 2,
        DeliveryState::Rejected { .. } => 3,
    }
}

/// Everything a backup keeps, chosen field by field. Deleted messages are left out (erased content is never
/// resurrected); restored chats are included, so a backup taken after a restore is complete.
pub fn collect(app: &AppState, exported_at_ms: u64) -> Result<HistoryArchive, BackupError> {
    let mut chats: Vec<ArchivedChat> = vec![];
    for r in app.relationships.values() {
        let messages: Vec<ArchivedMsg> = r
            .messages
            .iter()
            .filter(|m| !m.deleted && !m.lane)
            .map(|m| ArchivedMsg {
                id: m.id.clone(),
                outgoing: m.outgoing,
                text: m.text.clone(),
                sent_at: m.sent_at,
                delivery: if m.outgoing {
                    delivery_code(&m.delivery)
                } else {
                    2
                },
                clock_ms: m.clock_ms,
                clock_logical: m.clock_logical,
                digest: m.digest.clone(),
            })
            .collect();
        if messages.is_empty() && r.state != RelState::Active {
            continue; // a first contact that never said anything has no history
        }
        chats.push(ArchivedChat {
            id: r.id.clone(),
            display_name: r.display_name.clone(),
            peer_account_id: r.peer_account_id.clone(),
            created_at: r.created_at,
            canon_id: r.canon_id.clone().unwrap_or_default(),
            messages,
        });
    }
    for r in app.restored.values() {
        if app.relationships.contains_key(&r.id) {
            continue;
        }
        chats.push(ArchivedChat {
            id: r.id.clone(),
            display_name: r.display_name.clone(),
            peer_account_id: r.peer_account_id.clone(),
            created_at: r.created_at,
            canon_id: r.canon_id.clone().unwrap_or_default(),
            messages: r
                .messages
                .iter()
                .map(|m| ArchivedMsg {
                    id: m.id.clone(),
                    outgoing: m.outgoing,
                    text: m.text.clone(),
                    sent_at: m.sent_at,
                    delivery: m.delivery,
                    clock_ms: m.clock_ms,
                    clock_logical: m.clock_logical,
                    digest: m.digest.clone(),
                })
                .collect(),
        });
    }
    chats.sort_by(|a, b| a.id.cmp(&b.id));
    if chats.len() > MAX_CHATS
        || chats.iter().map(|c| c.messages.len()).sum::<usize>() > MAX_MESSAGES
    {
        return Err(BackupError::TooLarge);
    }
    Ok(HistoryArchive {
        exported_at_ms,
        owner_label: app.label.clone(),
        chats,
    })
}

fn opt_hex(h: &str) -> V {
    match crate::crypto::unhex(h) {
        Ok(b) if !h.is_empty() => V::B(b),
        _ => V::Null,
    }
}

/// Deterministic CBOR (the same strict subset as every other OM state object).
pub fn encode_body(a: &HistoryArchive) -> Vec<u8> {
    let chats = a
        .chats
        .iter()
        .map(|c| {
            let msgs = c
                .messages
                .iter()
                .map(|m| {
                    V::M(vec![
                        (0, V::T(m.id.clone())),
                        (1, V::Bool(m.outgoing)),
                        (2, V::T(m.text.clone())),
                        (3, V::U(m.sent_at)),
                        (4, V::U(u64::from(m.delivery))),
                        (5, V::U(m.clock_ms)),
                        (6, V::U(u64::from(m.clock_logical))),
                        (7, opt_hex(&m.digest)),
                    ])
                })
                .collect();
            V::M(vec![
                (0, V::T(c.id.clone())),
                (1, V::T(c.display_name.clone())),
                (2, opt_hex(&c.peer_account_id)),
                (3, V::U(c.created_at)),
                (4, opt_hex(&c.canon_id)),
                (5, V::A(msgs)),
            ])
        })
        .collect();
    cbor::encode(&V::M(vec![
        (0, V::T(BODY_TYPE.into())),
        (1, V::U(BODY_VERSION)),
        (2, V::U(a.exported_at_ms)),
        (3, V::T(a.owner_label.clone())),
        (4, V::A(chats)),
    ]))
}

fn plain(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}

fn hex_field(v: &V, len: usize) -> Result<Hex, BackupError> {
    match v {
        V::Null => Ok(String::new()),
        V::B(b) if b.len() == len => Ok(crate::crypto::hex(b)),
        _ => Err(BackupError::InvalidContent),
    }
}

/// Strict decode with every limit checked: unknown body versions, wrong types, duplicate ids, oversize text and
/// control characters in names or ids all fail the whole body.
pub fn decode_body(bytes: &[u8]) -> Result<HistoryArchive, BackupError> {
    let bad = |_| BackupError::InvalidContent;
    let v = cbor::decode(bytes).map_err(bad)?;
    if v.get(0).and_then(|x| x.t()).map_err(bad)? != BODY_TYPE
        || v.get(1).and_then(|x| x.u()).map_err(bad)? != BODY_VERSION
    {
        return Err(BackupError::InvalidContent);
    }
    let owner_label = v.get(3).and_then(|x| x.t()).map_err(bad)?.to_string();
    if owner_label.len() > MAX_NAME_BYTES || owner_label.chars().any(char::is_control) {
        return Err(BackupError::InvalidContent);
    }
    let raw_chats = v.get(4).and_then(|x| x.a()).map_err(bad)?;
    if raw_chats.len() > MAX_CHATS {
        return Err(BackupError::TooLarge);
    }
    let mut chats = Vec::with_capacity(raw_chats.len());
    let mut chat_ids = BTreeSet::new();
    let mut total = 0usize;
    for c in raw_chats {
        let id = c.get(0).and_then(|x| x.t()).map_err(bad)?.to_string();
        let display_name = c.get(1).and_then(|x| x.t()).map_err(bad)?.to_string();
        if !plain(&id, MAX_ID_BYTES)
            || !plain(&display_name, MAX_NAME_BYTES)
            || !chat_ids.insert(id.clone())
        {
            return Err(BackupError::InvalidContent);
        }
        let raw_msgs = c.get(5).and_then(|x| x.a()).map_err(bad)?;
        total += raw_msgs.len();
        if total > MAX_MESSAGES {
            return Err(BackupError::TooLarge);
        }
        let mut messages = Vec::with_capacity(raw_msgs.len());
        let mut msg_ids = BTreeSet::new();
        for m in raw_msgs {
            let mid = m.get(0).and_then(|x| x.t()).map_err(bad)?.to_string();
            let text = m.get(2).and_then(|x| x.t()).map_err(bad)?.to_string();
            let outgoing = match m.get(1).map_err(bad)? {
                V::Bool(b) => *b,
                _ => return Err(BackupError::InvalidContent),
            };
            let delivery = m.get(4).and_then(|x| x.u()).map_err(bad)?;
            if !plain(&mid, MAX_ID_BYTES)
                || text.len() > MAX_TEXT_BYTES
                || delivery > 3
                || !msg_ids.insert(mid.clone())
            {
                return Err(BackupError::InvalidContent);
            }
            messages.push(ArchivedMsg {
                id: mid,
                outgoing,
                text,
                sent_at: m.get(3).and_then(|x| x.u()).map_err(bad)?,
                delivery: delivery as u8,
                clock_ms: m.get(5).and_then(|x| x.u()).map_err(bad)?,
                clock_logical: m.get(6).and_then(|x| x.u32()).map_err(bad)?,
                digest: hex_field(m.get(7).map_err(bad)?, 32)?,
            });
        }
        chats.push(ArchivedChat {
            id,
            display_name,
            peer_account_id: hex_field(c.get(2).map_err(bad)?, 32)?,
            created_at: c.get(3).and_then(|x| x.u()).map_err(bad)?,
            canon_id: hex_field(c.get(4).map_err(bad)?, 32)?,
            messages,
        });
    }
    Ok(HistoryArchive {
        exported_at_ms: v.get(2).and_then(|x| x.u()).map_err(bad)?,
        owner_label,
        chats,
    })
}

// ------------------------------------------------------------------------------------------------ restoring

/// What a restore did, for the one line the person sees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// Conversations that were not on this device before.
    pub chats_added: u32,
    /// Restored conversations that gained messages from this file.
    pub chats_updated: u32,
    /// Conversations left alone because this device already has them live.
    pub chats_skipped: u32,
    pub messages_added: u32,
}

/// Merge a decoded archive into the profile as read-only [`RestoredChat`]s. Authority never moves: a chat that is
/// live on this device is left untouched, an existing restored message is never replaced, and nothing here creates a
/// relationship, key, route or outbox entry. Returns an error when the result would exceed the restored-history
/// limits; the caller works on a staged copy and discards it, so nothing is half applied.
pub fn apply_import(
    app: &mut AppState,
    archive: HistoryArchive,
    now_ms: u64,
) -> Result<ImportSummary, BackupError> {
    let mut s = ImportSummary::default();
    for c in archive.chats {
        if app.relationships.contains_key(&c.id) {
            s.chats_skipped += 1;
            continue;
        }
        let rest = app
            .restored
            .entry(c.id.clone())
            .or_insert_with(|| RestoredChat {
                id: c.id.clone(),
                display_name: c.display_name.clone(),
                peer_account_id: c.peer_account_id.clone(),
                created_at: c.created_at,
                canon_id: if c.canon_id.is_empty() {
                    None
                } else {
                    Some(c.canon_id.clone())
                },
                restored_at: now_ms,
                messages: vec![],
            });
        let fresh = rest.messages.is_empty();
        let known: BTreeSet<String> = rest.messages.iter().map(|m| m.id.clone()).collect();
        let before = rest.messages.len();
        for m in c.messages {
            if known.contains(&m.id) {
                continue;
            }
            rest.messages.push(RestoredMsg {
                id: m.id,
                outgoing: m.outgoing,
                text: m.text,
                sent_at: m.sent_at,
                delivery: m.delivery,
                clock_ms: m.clock_ms,
                clock_logical: m.clock_logical,
                digest: m.digest,
            });
        }
        if !fresh {
            // Merging into history that is already here: stable by time, so ties keep the order they were written in.
            // A new chat keeps exactly the order the file had (that was the order the original device showed).
            rest.messages.sort_by_key(|m| m.sent_at);
        }
        let added = (rest.messages.len() - before) as u32;
        s.messages_added += added;
        if fresh {
            s.chats_added += 1;
        } else if added > 0 {
            s.chats_updated += 1;
        }
    }
    if app.restored.len() > MAX_CHATS
        || app
            .restored
            .values()
            .map(|r| r.messages.len())
            .sum::<usize>()
            > MAX_MESSAGES
    {
        return Err(BackupError::TooLarge);
    }
    Ok(s)
}
