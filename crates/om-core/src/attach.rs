// SPDX-License-Identifier: MPL-2.0
//! Encrypted attachments (STORAGE_AND_BLOB_ARCHITECTURE, ADR-0150, spec/cddl/events.cddl `attachment-ref`).
//!
//! * The sender encrypts the plaintext on the device under a fresh random 32-byte content key (AES-256-GCM-SIV, the key
//!   and nonce expanded by HKDF from it, bound to the BlobID and the plaintext size). The key never leaves the device
//!   except inside the end-to-end encrypted `message.create` that carries the reference.
//! * `ciphertext_digest` is `DS-HASH("OM-BLOB-CIPHERTEXT-HASH-v1", ciphertext)`. A receiver recomputes it over the exact
//!   bytes it fetched *before* it decrypts, and only the plaintext of a verified blob is ever exposed.
//! * The BlobStore (a Home's spool, or any other) sees the ciphertext, its length and the time, never a name, a type or a key.
//!
//! This module is pure: no I/O, no state. The flows own persistence and the host performs the transfers.

use crate::api::RejectReason;
use crate::crypto::PROFILE_V1;
use crate::crypto::cbor::V;
use crate::crypto::kdf::hkdf_sha256;
use crate::crypto::session::{aead_open, aead_seal};
use crate::crypto::sig::{digest as digest_v, ds_hash};
use serde::{Deserialize, Serialize};

/// Registered label of `AttachmentRef.ciphertext_digest` (frozen).
pub const LBL_CIPHERTEXT: &str = "OM-BLOB-CIPHERTEXT-HASH-v1";
/// Registered label (reserved by V-001) used as the HKDF info and as the AEAD associated-data prefix.
pub const LBL_CONTENT_AD: &str = "OM-BLOB-CONTENT-AEAD-AD-v1";
pub const MAX_ATTACHMENTS: usize = 32;
/// AES-GCM-SIV appends a 16-byte tag.
pub const TAG_LEN: u64 = 16;
/// The most plaintext a V1 Core seals or opens in one piece (the whole object is held in memory; also the wire bound of the
/// Home API blob body). A BlobStore enforces its own, smaller advertised limit.
pub const HARD_MAX_PLAINTEXT: u64 = 16 * 1024 * 1024 - TAG_LEN;
const MAX_LOCATIONS: usize = 8;
const MAX_LOCATION_BYTES: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum AttachError {
    /// One object bigger than the store accepts (the advertised `max_blob_bytes`, or Core's own bound).
    TooLarge {
        max: u64,
    },
    /// The store has no room right now for this object (spool full by bytes or count). May clear; nothing was sent.
    StorageFull,
    /// No BlobStore capability has been advertised, so there is nowhere to put the object. Never falls back to inline.
    NoBlobStore,
    Empty,
    /// Not a media type, a bad name or a bad dimension.
    Invalid,
    /// Unknown blob, message or attachment index.
    NotFound,
    /// The fetched bytes are not the bytes the reference committed to (digest, length or authentication failed).
    IntegrityFailure,
    /// A crypto profile or reference form this Core does not implement.
    Unsupported,
}

impl AttachError {
    /// The typed refusal the person sees as words ("too big to send right now"); never a silent downgrade.
    pub fn reject_reason(&self) -> RejectReason {
        match self {
            AttachError::TooLarge { .. } => RejectReason::TooLarge,
            AttachError::StorageFull => RejectReason::Quota,
            AttachError::NoBlobStore | AttachError::Unsupported => RejectReason::Unsupported,
            AttachError::Empty | AttachError::Invalid => RejectReason::Invalid,
            AttachError::NotFound => RejectReason::NotFound,
            AttachError::IntegrityFailure => RejectReason::IntegrityFailure,
        }
    }
}

/// What the sender knows about a file besides its bytes. Untrusted display data on the receiving side.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub media_type: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// A decoded `attachment-ref`.
#[derive(Clone, PartialEq, Eq)]
pub struct AttachmentRef {
    pub blob_id: [u8; 32],
    pub media_type: String,
    pub plaintext_size: u64,
    pub ciphertext_digest: [u8; 32],
    pub key: [u8; 32],
    pub locations: Vec<String>,
    pub filename: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    /// The read capability (`repair_capability`, key 10: the only capability slot of the frozen ref).
    pub read_cap: Option<Vec<u8>>,
}

impl core::fmt::Debug for AttachmentRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AttachmentRef")
            .field("media_type", &self.media_type)
            .field("plaintext_size", &self.plaintext_size)
            .field("key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Is `s` a plausible `type/subtype`? The 127-byte bound is the schema's; the rest keeps control and layout characters
/// out of anything that is later rendered.
pub fn valid_media_type(s: &str) -> bool {
    let Some((t, sub)) = s.split_once('/') else {
        return false;
    };
    (1..=127).contains(&s.len())
        && !t.is_empty()
        && !sub.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\\' | b'<' | b'>'))
}

/// A file name is display text from the sender: only its last path component, no control or bidi-override characters.
pub fn clean_filename(s: &str) -> Option<String> {
    let last = s.rsplit(['/', '\\']).next().unwrap_or("");
    let out: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect();
    let out = out.trim();
    if out.is_empty() || out == "." || out == ".." {
        return None;
    }
    let mut end = out.len().min(255);
    while !out.is_char_boundary(end) {
        end -= 1;
    }
    Some(out[..end].to_string())
}

/// A locator from a peer is only ever an absolute `https://` URL without credentials. Anything else is dropped before use.
pub fn valid_location(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    s.len() <= MAX_LOCATION_BYTES
        && !authority.is_empty()
        && !authority.contains('@')
        && !authority.contains('\\')
        && s.bytes().all(|b| b.is_ascii_graphic())
}

fn content_keys(key: &[u8; 32], blob_id: &[u8; 32]) -> ([u8; 32], [u8; 12]) {
    let mut out = [0u8; 44];
    hkdf_sha256(blob_id, key, LBL_CONTENT_AD.as_bytes(), &mut out);
    let mut k = [0u8; 32];
    let mut n = [0u8; 12];
    k.copy_from_slice(&out[..32]);
    n.copy_from_slice(&out[32..]);
    (k, n)
}

fn associated_data(blob_id: &[u8; 32], plaintext_size: u64) -> Vec<u8> {
    let mut ad = LBL_CONTENT_AD.as_bytes().to_vec();
    ad.extend_from_slice(blob_id);
    ad.extend_from_slice(&plaintext_size.to_be_bytes());
    ad
}

/// `OM-BLOB-CIPHERTEXT-HASH-v1` over the exact ciphertext bytes.
pub fn ciphertext_digest(ciphertext: &[u8]) -> [u8; 32] {
    ds_hash(LBL_CIPHERTEXT, ciphertext)
}

/// Encrypt `plaintext` for `blob_id` under the random content `key`. The key is single-use (one per object), so a derived
/// nonce cannot repeat under it.
pub fn seal(key: &[u8; 32], blob_id: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
    let (k, n) = content_keys(key, blob_id);
    aead_seal(
        &k,
        &n,
        &associated_data(blob_id, plaintext.len() as u64),
        plaintext,
    )
}

impl AttachmentRef {
    /// Verify the fetched bytes against the digest and the committed length, then (and only then) decrypt. The plaintext
    /// of a blob whose digest does not match is never produced, and a wrong length after decryption is a failure too.
    pub fn open(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AttachError> {
        if self.plaintext_size > HARD_MAX_PLAINTEXT {
            return Err(AttachError::TooLarge {
                max: HARD_MAX_PLAINTEXT,
            });
        }
        if ciphertext.len() as u64 != self.plaintext_size + TAG_LEN {
            return Err(AttachError::IntegrityFailure);
        }
        let got = ciphertext_digest(ciphertext);
        // Constant-time enough for a public digest; a mismatch decides nothing secret.
        if got != self.ciphertext_digest {
            return Err(AttachError::IntegrityFailure);
        }
        let (k, n) = content_keys(&self.key, &self.blob_id);
        let pt = aead_open(
            &k,
            &n,
            &associated_data(&self.blob_id, self.plaintext_size),
            ciphertext,
        )
        .map_err(|_| AttachError::IntegrityFailure)?;
        if pt.len() as u64 != self.plaintext_size {
            return Err(AttachError::IntegrityFailure);
        }
        Ok(pt)
    }

    /// The canonical `attachment-ref` value.
    pub fn to_v(&self) -> V {
        let mut m = vec![
            (0, V::B(self.blob_id.to_vec())),
            (1, V::T(self.media_type.clone())),
            (2, V::U(self.plaintext_size)),
            (3, digest_v(&self.ciphertext_digest)),
            (
                4,
                V::M(vec![(0, V::U(PROFILE_V1)), (1, V::B(self.key.to_vec()))]),
            ),
        ];
        if !self.locations.is_empty() {
            m.push((
                5,
                V::A(self.locations.iter().map(|l| V::T(l.clone())).collect()),
            ));
        }
        if let Some(f) = &self.filename {
            m.push((6, V::T(f.clone())));
        }
        if let Some(w) = self.width {
            m.push((7, V::U(w as u64)));
        }
        if let Some(h) = self.height {
            m.push((8, V::U(h as u64)));
        }
        if let Some(d) = self.duration_ms {
            m.push((9, V::U(d)));
        }
        if let Some(c) = &self.read_cap {
            m.push((10, V::B(c.clone())));
        }
        V::M(m)
    }

    /// Strictly read an `attachment-ref` (already schema-checked by the W0 registry; this re-validates what Core relies on).
    /// Locations that are not plain `https` URLs are dropped, never used. `None` fails the whole event closed.
    pub fn from_v(v: &V) -> Option<Self> {
        let media_type = v.get(1).ok()?.t().ok()?.to_string();
        if !valid_media_type(&media_type) {
            return None;
        }
        let d = v.get(3).ok()?;
        if d.get(0).ok()?.u().ok()? != 1 {
            return None;
        }
        let enc = v.get(4).ok()?;
        if enc.get(0).ok()?.u().ok()? != PROFILE_V1 {
            return None;
        }
        let dim = |k: u64| -> Option<Option<u32>> {
            match v.opt(k) {
                None => Some(None),
                Some(x) => {
                    let n = x.u().ok()?;
                    (1..=65535).contains(&n).then_some(Some(n as u32))
                }
            }
        };
        let mut locations = vec![];
        if let Some(l) = v.opt(5) {
            let l = l.a().ok()?;
            if l.is_empty() || l.len() > MAX_LOCATIONS {
                return None;
            }
            for x in l {
                let x = x.t().ok()?;
                if valid_location(x) && !locations.iter().any(|y| y == x) {
                    locations.push(x.to_string());
                }
            }
        }
        let read_cap = match v.opt(10) {
            None => None,
            Some(c) => {
                let c = c.b().ok()?;
                (16..=64).contains(&c.len()).then(|| c.to_vec())?.into()
            }
        };
        Some(AttachmentRef {
            blob_id: v.get(0).ok()?.b32().ok()?,
            media_type,
            plaintext_size: v.get(2).ok()?.u().ok()?,
            ciphertext_digest: d.get(1).ok()?.b32().ok()?,
            key: enc.get(1).ok()?.b32().ok()?,
            locations,
            filename: match v.opt(6) {
                None => None,
                Some(f) => clean_filename(f.t().ok()?),
            },
            width: dim(7)?,
            height: dim(8)?,
            duration_ms: match v.opt(9) {
                None => None,
                Some(x) => Some(x.u().ok()?),
            },
            read_cap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(plain: &[u8]) -> (AttachmentRef, Vec<u8>) {
        let (key, blob_id) = ([7u8; 32], [9u8; 32]);
        let ct = seal(&key, &blob_id, plain);
        (
            AttachmentRef {
                blob_id,
                media_type: "audio/ogg".into(),
                plaintext_size: plain.len() as u64,
                ciphertext_digest: ciphertext_digest(&ct),
                key,
                locations: vec!["https://home.example/v1/blobs/00".into()],
                filename: Some("a.ogg".into()),
                width: None,
                height: None,
                duration_ms: Some(1500),
                read_cap: Some(vec![1; 48]),
            },
            ct,
        )
    }

    #[test]
    fn seal_open_round_trip_and_ref_is_w0_valid() {
        let (r, ct) = sample(b"voice bytes");
        assert_eq!(r.open(&ct).unwrap(), b"voice bytes");
        assert_eq!(ct.len() as u64, r.plaintext_size + TAG_LEN);
        let v = r.to_v();
        let back = AttachmentRef::from_v(
            &crate::crypto::cbor::decode(&crate::crypto::cbor::encode(&v)).unwrap(),
        )
        .unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn any_change_to_the_fetched_bytes_fails_before_decrypt() {
        let (r, ct) = sample(b"0123456789abcdef0123456789");
        for i in [0, ct.len() / 2, ct.len() - 1] {
            let mut bad = ct.clone();
            bad[i] ^= 1;
            assert_eq!(r.open(&bad), Err(AttachError::IntegrityFailure));
        }
        assert_eq!(
            r.open(&ct[..ct.len() - 1]),
            Err(AttachError::IntegrityFailure)
        );
        let mut longer = ct.clone();
        longer.push(0);
        assert_eq!(r.open(&longer), Err(AttachError::IntegrityFailure));
        // a ref whose digest is right but whose key is wrong still never yields plaintext
        let mut wrong = r.clone();
        wrong.key = [8; 32];
        assert_eq!(wrong.open(&ct), Err(AttachError::IntegrityFailure));
    }

    #[test]
    fn untrusted_fields_are_cleaned_or_dropped() {
        assert_eq!(
            clean_filename("../../etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(clean_filename("C:\\x\\y.txt").as_deref(), Some("y.txt"));
        assert_eq!(
            clean_filename("a\u{202e}b\n.txt").as_deref(),
            Some("ab.txt")
        );
        assert_eq!(clean_filename(".."), None);
        assert!(valid_location("https://home.example/b/1"));
        for bad in [
            "http://home.example/b",
            "https://user@home.example/b",
            "ftp://x",
            "https:///b",
            "https://h/ b",
            "javascript:alert(1)",
        ] {
            assert!(!valid_location(bad), "{bad}");
        }
        assert!(valid_media_type("image/png"));
        assert!(!valid_media_type("png"));
        assert!(!valid_media_type("image/png\n"));
    }
}
