// SPDX-License-Identifier: MPL-2.0
//! The RecoveryRecord (ADR-0199, `spec/cddl/recovery-record.cddl`): the encrypted, advisory record a RecoveryStore holds
//! so that a device which has only the RecoveryRoot can build a recovery request and a successor.
//!
//! * Keys: every key and capability is `HKDF-SHA-256(no salt, IKM = RecoveryRoot, info = ASCII(label) ||
//!   recovery_scope_id, L = 32)` (ADR-0199 section 3). The locator is `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`.
//! * Envelope (type 35, unsigned): the crypto profile's AEAD (AES-256-GCM-SIV) under the record key, a fresh random
//!   12-byte nonce carried in the envelope, and the associated data `OM-CBOR [label, scope, locator, 1]`.
//! * Plaintext (type 36, or the redirect type 37): padded inside the AEAD with key 22 so that the encoded plaintext is
//!   exactly one of four buckets. A plaintext within 3 bytes of a bucket, or one for which no pad length reaches the
//!   bucket exactly (a bstr head changes size at 24, 256 and 65,536 bytes), uses the next bucket (departure D144).
//! * Reader (section 4): size, version, profile; open; decode; then the cross-field checks of [`RecoveryRecord::verify`].
//!   The result stays advisory: it authorizes nothing and every carried state still verifies against the chain rules.
//! * [`RecoveryKit`] (type 38) is device-private (class `om.recovery_secret`): never a backup, never sent to a Home.
//!
//! The AEAD composition is OM's own over standard primitives and has had no independent review.

use crate::crypto::Error;
use crate::crypto::PROFILE_V1;
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::cbor::{self, V};
use crate::crypto::kdf::hkdf_sha256;
use crate::crypto::session::{aead_open, aead_seal};
use crate::crypto::sig::{digest, ds_hash};
use crate::recovery::RecoveryPolicy;
use std::collections::BTreeSet;

pub const LBL_READ_CAP: &str = "OM-RECOVERY-RECORD-READ-CAP-v1";
pub const LBL_WRITE_CAP: &str = "OM-RECOVERY-RECORD-WRITE-CAP-v1";
pub const LBL_RECORD_KEY: &str = "OM-RECOVERY-RECORD-KEY-v1";
pub const LBL_LOCATOR: &str = "OM-RECOVERY-RECORD-LOCATOR-v1";
pub const LBL_SNAPSHOT_KEY: &str = "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1";
pub const LBL_RECORD_AD: &str = "OM-RECOVERY-RECORD-AEAD-AD-v1";
pub const LBL_SNAPSHOT_AD: &str = "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-AEAD-AD-v1";

pub const TYPE_ENVELOPE: u64 = 35;
pub const TYPE_RECORD: u64 = 36;
pub const TYPE_REDIRECT: u64 = 37;
pub const TYPE_KIT: u64 = 38;

/// The four plaintext sizes (ADR-0199 section 3): the Home learns one of four sizes, never the number of contacts.
pub const BUCKETS: [usize; 4] = [16_384, 65_536, 262_144, 1_048_000];
pub const TAG_LEN: usize = 16;
pub const MAX_ENVELOPE_BYTES: usize = 1_048_576;
pub const MAX_PLAINTEXT_BYTES: usize = 1_048_000;
pub const MAX_KIT_BYTES: usize = 4096;
pub const MAX_STATES: usize = 9;
pub const MAX_CREDENTIALS: usize = 320;
pub const MAX_CREDENTIAL_BYTES_TOTAL: usize = 700_000;
pub const MAX_RELATIONSHIPS: usize = 256;
pub const MAX_PROVIDER_HOMES: usize = 4;
/// `refresh` occupies the low 20 bits of `record_generation`.
pub const REFRESH_BITS: u32 = 20;
pub const MAX_ACS_GENERATION: u64 = (1u64 << (64 - REFRESH_BITS)) - 1;

fn bad(s: &'static str) -> Error {
    Error::Decode(s)
}

// ------------------------------------------------------------------------------------------------- generation

/// `record_generation = (acs_generation << 20) | refresh` (ADR-0199 section 2). `refresh` is below 2^20 and the
/// result is at least 1.
pub fn record_generation(acs_generation: u64, refresh: u32) -> Result<u64, Error> {
    if refresh >= (1 << REFRESH_BITS) {
        return Err(bad("record generation: refresh out of range"));
    }
    if acs_generation > MAX_ACS_GENERATION {
        return Err(bad("record generation: acs generation out of range"));
    }
    let g = (acs_generation << REFRESH_BITS) | refresh as u64;
    if g == 0 {
        return Err(bad("record generation: zero"));
    }
    Ok(g)
}

pub fn generation_acs(record_generation: u64) -> u64 {
    record_generation >> REFRESH_BITS
}

pub fn generation_refresh(record_generation: u64) -> u32 {
    (record_generation & ((1 << REFRESH_BITS) - 1)) as u32
}

// ------------------------------------------------------------------------------------------------------- keys

fn derive(root: &[u8; 32], label: &str, scope: &[u8; 32]) -> [u8; 32] {
    let mut info = label.as_bytes().to_vec();
    info.extend_from_slice(scope);
    let mut out = [0u8; 32];
    hkdf_sha256(&[], root, &info, &mut out);
    out
}

/// The keys and capabilities one RecoveryRoot yields for one RecoveryStore scope. Wiped on drop, never printed.
pub struct RecordKeys {
    pub read_cap: [u8; 32],
    pub write_cap: [u8; 32],
    pub record_key: [u8; 32],
    /// `OM-RECOVERY-RELATIONSHIP-SNAPSHOT-KEY-v1`; the normal profile does not use it for v1 records, the label stays derivable.
    pub snapshot_key: [u8; 32],
    /// 32-byte digest value of `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`; not secret.
    pub locator: [u8; 32],
}

impl core::fmt::Debug for RecordKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RecordKeys(<redacted>)")
    }
}

impl Drop for RecordKeys {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.read_cap);
        zeroize::Zeroize::zeroize(&mut self.write_cap);
        zeroize::Zeroize::zeroize(&mut self.record_key);
        zeroize::Zeroize::zeroize(&mut self.snapshot_key);
    }
}

pub fn locator_of(read_cap: &[u8; 32]) -> [u8; 32] {
    ds_hash(LBL_LOCATOR, read_cap)
}

impl RecordKeys {
    pub fn derive(root: &[u8; 32], scope: &[u8; 32]) -> RecordKeys {
        let read_cap = derive(root, LBL_READ_CAP, scope);
        RecordKeys {
            read_cap,
            write_cap: derive(root, LBL_WRITE_CAP, scope),
            record_key: derive(root, LBL_RECORD_KEY, scope),
            snapshot_key: derive(root, LBL_SNAPSHOT_KEY, scope),
            locator: locator_of(&read_cap),
        }
    }

    /// The keys a [`RecoveryKit`] holds (a kit has no root).
    pub fn from_kit(k: &RecoveryKit) -> RecordKeys {
        RecordKeys {
            read_cap: k.read_cap,
            write_cap: k.write_cap,
            record_key: k.record_key,
            snapshot_key: k.snapshot_key.unwrap_or([0; 32]),
            locator: locator_of(&k.read_cap),
        }
    }
}

/// The frozen associated data `OM-CBOR [OM-RECOVERY-RECORD-AEAD-AD-v1, recovery_scope_id, opaque_locator, 1]`.
pub fn associated_data(scope: &[u8; 32], locator: &[u8; 32]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(LBL_RECORD_AD.into()),
        V::B(scope.to_vec()),
        V::B(locator.to_vec()),
        V::U(1),
    ]))
}

/// `OM-CBOR [OM-RECOVERY-RELATIONSHIP-SNAPSHOT-AEAD-AD-v1, recovery_scope_id, record_generation, 1]`.
pub fn snapshot_associated_data(scope: &[u8; 32], record_generation: u64) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(LBL_SNAPSHOT_AD.into()),
        V::B(scope.to_vec()),
        V::U(record_generation),
        V::U(1),
    ]))
}

// ----------------------------------------------------------------------------------------------------- types

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcsHeadRef {
    pub generation: u64,
    pub state_hash: [u8; 32],
}

impl AcsHeadRef {
    fn to_v(&self) -> V {
        V::M(vec![
            (0, V::U(self.generation)),
            (1, digest(&self.state_hash)),
        ])
    }
    fn from_v(v: &V) -> Result<Self, Error> {
        Ok(AcsHeadRef {
            generation: v.get(0)?.u()?,
            state_hash: digest_value(v.get(1)?)?,
        })
    }
}

fn digest_value(v: &V) -> Result<[u8; 32], Error> {
    if v.get(0)?.u()? != 1 {
        return Err(bad("record: digest algorithm"));
    }
    v.get(1)?.b32()
}

/// The evidence a fresh device needs (section 1): signed states as the bytes of their `SignedEnvelope`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcsEvidence {
    pub head: Vec<u8>,
    /// Up to 8 states before the head, oldest first.
    pub tail: Vec<Vec<u8>>,
    /// Canonical `DeviceCredential` bodies the head and the tail reference; no duplicates, none unreferenced.
    pub credentials: Vec<Vec<u8>>,
    /// The canonical `RecoveryPolicy` body the head's `recovery_policy_commitment` names.
    pub policy: Vec<u8>,
}

/// Where a Home keeps recovery records: what the host's provider adapter reports (host, port, `recovery_scope_id`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryPlace {
    pub host: String,
    /// `None` = 443.
    pub port: Option<u16>,
    pub scope: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderHome {
    pub host: String,
    pub port: Option<u16>,
    pub scope: Option<[u8; 32]>,
    pub relay_set: Option<Vec<u8>>,
    pub credential: Option<Vec<u8>>,
}

impl ProviderHome {
    pub fn new(host: &str, port: Option<u16>, scope: Option<[u8; 32]>) -> Self {
        ProviderHome {
            host: host.into(),
            port,
            scope,
            relay_set: None,
            credential: None,
        }
    }
    fn to_v(&self) -> V {
        let mut m = vec![(0, V::T(self.host.clone()))];
        if let Some(p) = self.port {
            m.push((1, V::U(p as u64)));
        }
        if let Some(s) = &self.scope {
            m.push((2, V::B(s.to_vec())));
        }
        if let Some(r) = &self.relay_set {
            m.push((3, V::B(r.clone())));
        }
        if let Some(c) = &self.credential {
            m.push((4, V::B(c.clone())));
        }
        V::M(m)
    }
    fn from_v(v: &V) -> Result<Self, Error> {
        Ok(ProviderHome {
            host: v.get(0)?.t()?.to_string(),
            port: v
                .opt(1)
                .map(|p| {
                    p.u()
                        .and_then(|n| u16::try_from(n).map_err(|_| bad("record: port")))
                })
                .transpose()?,
            scope: v.opt(2).map(|s| s.b32()).transpose()?,
            relay_set: v.opt(3).map(|b| b.b().map(<[u8]>::to_vec)).transpose()?,
            credential: v.opt(4).map(|b| b.b().map(<[u8]>::to_vec)).transpose()?,
        })
    }
}

/// One relationship the recovered device reaches again. `routes` (relay entries) and the delivery keys are kept as
/// their validated CBOR values: they are revalidated before any use (ADR-0199 section 7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub conversation_id: [u8; 32],
    pub peer_account_id: [u8; 32],
    pub label: Option<String>,
    pub routes: Vec<V>,
    pub targets: Vec<([u8; 16], V)>,
    pub key_fetch_cap: Option<Vec<u8>>,
    pub peer_head: Option<AcsHeadRef>,
}

impl SnapshotEntry {
    fn to_v(&self) -> V {
        let mut m = vec![
            (0, V::B(self.conversation_id.to_vec())),
            (1, V::B(self.peer_account_id.to_vec())),
            (3, V::A(self.routes.clone())),
            (
                4,
                V::A(
                    self.targets
                        .iter()
                        .map(|(d, k)| V::M(vec![(0, V::B(d.to_vec())), (1, k.clone())]))
                        .collect(),
                ),
            ),
        ];
        if let Some(l) = &self.label {
            m.push((2, V::T(l.clone())));
        }
        if let Some(c) = &self.key_fetch_cap {
            m.push((5, V::B(c.clone())));
        }
        if let Some(h) = &self.peer_head {
            m.push((6, h.to_v()));
        }
        V::M(m)
    }
    fn from_v(v: &V) -> Result<Self, Error> {
        let mut targets = vec![];
        for t in v.get(4)?.a()? {
            let d: [u8; 16] = t
                .get(0)?
                .b()?
                .try_into()
                .map_err(|_| bad("record: device id"))?;
            targets.push((d, t.get(1)?.clone()));
        }
        Ok(SnapshotEntry {
            conversation_id: v.get(0)?.b32()?,
            peer_account_id: v.get(1)?.b32()?,
            label: v.opt(2).map(|l| l.t().map(str::to_string)).transpose()?,
            routes: v.get(3)?.a()?.to_vec(),
            targets,
            key_fetch_cap: v.opt(5).map(|b| b.b().map(<[u8]>::to_vec)).transpose()?,
            peer_head: v.opt(6).map(AcsHeadRef::from_v).transpose()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RelationshipSnapshot {
    pub entries: Vec<SnapshotEntry>,
    pub truncated: bool,
}

impl RelationshipSnapshot {
    fn to_v(&self) -> V {
        let mut m = vec![(0, V::A(self.entries.iter().map(|e| e.to_v()).collect()))];
        if self.truncated {
            m.push((1, V::Bool(true)));
        }
        V::M(m)
    }
    fn from_v(v: &V) -> Result<Self, Error> {
        Ok(RelationshipSnapshot {
            entries: v
                .get(0)?
                .a()?
                .iter()
                .map(SnapshotEntry::from_v)
                .collect::<Result<_, _>>()?,
            truncated: v.opt(1).is_some(),
        })
    }
    /// The canonical bytes of the snapshot (the plaintext of the Maximum Privacy second layer).
    pub fn to_bytes(&self) -> Vec<u8> {
        cbor::encode(&self.to_v())
    }
    /// Strict decode through the registry (`RelationshipSnapshot`).
    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        crate::flows::registry()
            .check_struct("RelationshipSnapshot", b, MAX_PLAINTEXT_BYTES)
            .map_err(|_| bad("relationship snapshot: invalid"))?;
        Self::from_v(&cbor::decode(b)?)
    }
}

/// Normal profile: the snapshot in the clear inside the record (key 7). Maximum Privacy: `nonce || ciphertext` (key 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relationships {
    Plain(RelationshipSnapshot),
    Sealed(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRecord {
    pub generation: u64,
    pub recovery_epoch: u64,
    pub head: AcsHeadRef,
    pub evidence: AcsEvidence,
    pub providers: Vec<ProviderHome>,
    pub relationships: Relationships,
    /// Key 23, preserved and never read.
    pub extensions: Option<V>,
}

/// The second plaintext kind (ADR-0200 section 6): left at the old locator of a Home the owner moved away from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRedirect {
    pub generation: u64,
    pub new_place: ProviderHome,
    pub extensions: Option<V>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plain {
    Record(Box<RecoveryRecord>),
    Redirect(RecoveryRedirect),
}

// ----------------------------------------------------------------------------------------------- encode / pad

fn bstr_item_len(l: usize) -> usize {
    1 + match l {
        0..=23 => 0,
        24..=255 => 1,
        256..=65_535 => 2,
        _ => 4,
    }
}

/// The pad length that makes `unpadded_len + (key 22 item)` equal `bucket`, if one exists.
pub fn pad_length(unpadded_len: usize, bucket: usize) -> Option<usize> {
    // item = key byte (1) + bstr head + payload; the payload length is `l`.
    let d = bucket.checked_sub(unpadded_len)?;
    // Within 3 bytes of a bucket uses the next one (ADR-0199 section 3).
    if d < 3 {
        return None;
    }
    [1usize, 2, 3, 5].into_iter().find_map(|hdr| {
        let l = d.checked_sub(1 + hdr)?;
        (bstr_item_len(l) == hdr).then_some(l)
    })
}

/// The smallest bucket that a plaintext of `unpadded_len` bytes reaches exactly with a key 22 pad.
pub fn bucket_for(unpadded_len: usize) -> Option<usize> {
    BUCKETS
        .into_iter()
        .find(|b| pad_length(unpadded_len, *b).is_some())
}

/// Encode `pairs` plus a key 22 pad so the result is exactly one bucket. `None` if it does not fit the largest.
fn encode_padded(mut pairs: Vec<(u64, V)>) -> Result<Vec<u8>, Error> {
    let base = cbor::encode(&V::M(pairs.clone())).len();
    for bucket in BUCKETS {
        if let Some(l) = pad_length(base, bucket) {
            pairs.push((22, V::B(vec![0u8; l])));
            let out = cbor::encode(&V::M(pairs));
            debug_assert_eq!(out.len(), bucket);
            return Ok(out);
        }
    }
    Err(bad("record: too large"))
}

impl RecoveryRecord {
    fn pairs(&self) -> Vec<(u64, V)> {
        let ev = &self.evidence;
        let mut m = vec![
            (0, V::U(TYPE_RECORD)),
            (1, V::U(1)),
            (2, V::U(self.generation)),
            (3, V::U(self.recovery_epoch)),
            (4, self.head.to_v()),
            (
                5,
                V::M(vec![
                    (0, V::B(ev.head.clone())),
                    (1, V::A(ev.tail.iter().map(|b| V::B(b.clone())).collect())),
                    (
                        2,
                        V::A(ev.credentials.iter().map(|b| V::B(b.clone())).collect()),
                    ),
                    (3, V::B(ev.policy.clone())),
                ]),
            ),
            (
                6,
                V::M(vec![(
                    0,
                    V::A(self.providers.iter().map(ProviderHome::to_v).collect()),
                )]),
            ),
        ];
        match &self.relationships {
            Relationships::Plain(s) => m.push((7, s.to_v())),
            Relationships::Sealed(b) => m.push((8, V::B(b.clone()))),
        }
        if let Some(e) = &self.extensions {
            m.push((23, e.clone()));
        }
        m
    }

    /// The padded plaintext: exactly one of [`BUCKETS`] bytes.
    pub fn to_padded_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_padded(self.pairs())
    }

    /// Strict decode of a plaintext (registry check, then the typed read). Cross-field rules are [`Self::verify`].
    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        if b.len() > MAX_PLAINTEXT_BYTES {
            return Err(bad("record: plaintext too large"));
        }
        crate::flows::registry()
            .check("RecoveryRecord", b, false)
            .map_err(|_| bad("record: invalid"))?;
        let v = cbor::decode(b)?;
        let ev = v.get(5)?;
        let strs = |x: &V| -> Result<Vec<Vec<u8>>, Error> {
            x.a()?.iter().map(|e| e.b().map(<[u8]>::to_vec)).collect()
        };
        let providers = v
            .get(6)?
            .get(0)?
            .a()?
            .iter()
            .map(ProviderHome::from_v)
            .collect::<Result<Vec<_>, _>>()?;
        let relationships = match (v.opt(7), v.opt(8)) {
            (Some(s), None) => Relationships::Plain(RelationshipSnapshot::from_v(s)?),
            (None, Some(c)) => Relationships::Sealed(c.b()?.to_vec()),
            _ => return Err(Error::Binding("record: exactly one of keys 7 and 8")),
        };
        Ok(RecoveryRecord {
            generation: v.get(2)?.u()?,
            recovery_epoch: v.get(3)?.u()?,
            head: AcsHeadRef::from_v(v.get(4)?)?,
            evidence: AcsEvidence {
                head: ev.get(0)?.b()?.to_vec(),
                tail: strs(ev.get(1)?)?,
                credentials: strs(ev.get(2)?)?,
                policy: ev.get(3)?.b()?.to_vec(),
            },
            providers,
            relationships,
            extensions: v.opt(23).cloned(),
        })
    }
}

impl RecoveryRedirect {
    pub fn to_padded_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut m = vec![
            (0, V::U(TYPE_REDIRECT)),
            (1, V::U(1)),
            (2, V::U(self.generation)),
            (3, self.new_place.to_v()),
        ];
        if let Some(e) = &self.extensions {
            m.push((23, e.clone()));
        }
        encode_padded(m)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        crate::flows::registry()
            .check("RecoveryRedirect", b, false)
            .map_err(|_| bad("redirect: invalid"))?;
        let v = cbor::decode(b)?;
        let place = ProviderHome::from_v(v.get(3)?)?;
        // The new place's host and scope are required (registry note: a semantic rule).
        if place.scope.is_none() {
            return Err(Error::Binding("redirect: new place needs a scope"));
        }
        Ok(RecoveryRedirect {
            generation: v.get(2)?.u()?,
            new_place: place,
            extensions: v.opt(23).cloned(),
        })
    }
}

// ------------------------------------------------------------------------------------------- seal / open

fn envelope_bytes(profile: u64, nonce: &[u8; 12], ct: Vec<u8>) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(TYPE_ENVELOPE)),
        (1, V::U(1)),
        (2, V::U(profile)),
        (3, V::B(nonce.to_vec())),
        (4, V::B(ct)),
    ]))
}

/// Seal a padded plaintext (one bucket long) into an envelope. The caller supplies a fresh random `nonce` and seals
/// **once** per staged write; retries resend these exact bytes (ADR-0199 section 6, rule 5).
pub fn seal_envelope(
    keys: &RecordKeys,
    scope: &[u8; 32],
    nonce: [u8; 12],
    padded_plaintext: &[u8],
) -> Result<Vec<u8>, Error> {
    if !BUCKETS.contains(&padded_plaintext.len()) {
        return Err(bad("record: plaintext is not a bucket"));
    }
    let ad = associated_data(scope, &keys.locator);
    let ct = aead_seal(&keys.record_key, &nonce, &ad, padded_plaintext);
    Ok(envelope_bytes(PROFILE_V1, &nonce, ct))
}

/// Open an envelope to its plaintext bytes. Every failure is "can't read it", never a partial read.
pub fn open_envelope(
    keys: &RecordKeys,
    scope: &[u8; 32],
    envelope: &[u8],
) -> Result<Vec<u8>, Error> {
    if envelope.len() > MAX_ENVELOPE_BYTES {
        return Err(bad("record: envelope too large"));
    }
    crate::flows::registry()
        .check("RecoveryRecordEnvelope", envelope, false)
        .map_err(|_| bad("record: envelope invalid"))?;
    let v = cbor::decode(envelope)?;
    if v.get(2)?.u()? != PROFILE_V1 {
        return Err(bad("record: unknown crypto profile"));
    }
    let nonce: [u8; 12] = v
        .get(3)?
        .b()?
        .try_into()
        .map_err(|_| bad("record: nonce"))?;
    let ct = v.get(4)?.b()?;
    if !BUCKETS.contains(&(ct.len().saturating_sub(TAG_LEN))) {
        return Err(bad("record: ciphertext is not a bucket"));
    }
    let ad = associated_data(scope, &keys.locator);
    aead_open(&keys.record_key, &nonce, &ad, ct)
}

/// Decode an opened plaintext by its type: 36 a record, 37 a redirect, anything else is refused.
pub fn read_plaintext(plain: &[u8]) -> Result<Plain, Error> {
    if !BUCKETS.contains(&plain.len()) {
        return Err(bad("record: plaintext is not a bucket"));
    }
    let v = cbor::decode(plain)?;
    match v.get(0)?.u()? {
        TYPE_RECORD => {
            let r = RecoveryRecord::from_bytes(plain)?;
            r.verify()?;
            Ok(Plain::Record(Box::new(r)))
        }
        TYPE_REDIRECT => Ok(Plain::Redirect(RecoveryRedirect::from_bytes(plain)?)),
        _ => Err(bad("record: unknown plaintext type")),
    }
}

/// Open and decode in one step.
pub fn open(keys: &RecordKeys, scope: &[u8; 32], envelope: &[u8]) -> Result<Plain, Error> {
    read_plaintext(&open_envelope(keys, scope, envelope)?)
}

/// Maximum Privacy second layer: `nonce (12) || AEAD ciphertext` of the snapshot under an independent key.
pub fn seal_snapshot_layer(
    key: &[u8; 32],
    scope: &[u8; 32],
    record_generation: u64,
    nonce: [u8; 12],
    snapshot: &RelationshipSnapshot,
) -> Vec<u8> {
    let ad = snapshot_associated_data(scope, record_generation);
    let mut out = nonce.to_vec();
    out.extend(aead_seal(key, &nonce, &ad, &snapshot.to_bytes()));
    out
}

pub fn open_snapshot_layer(
    key: &[u8; 32],
    scope: &[u8; 32],
    record_generation: u64,
    layer: &[u8],
) -> Result<RelationshipSnapshot, Error> {
    if layer.len() < 12 + TAG_LEN {
        return Err(bad("snapshot layer: short"));
    }
    let nonce: [u8; 12] = layer[..12].try_into().expect("12");
    let ad = snapshot_associated_data(scope, record_generation);
    RelationshipSnapshot::from_bytes(&aead_open(key, &nonce, &ad, &layer[12..])?)
}

// --------------------------------------------------------------------------------------- reader checks (s.4)

/// What the cross-field checks parsed, so a caller does not parse the evidence twice.
#[derive(Debug)]
pub struct Verified {
    pub head: Acs,
    /// Tail states then the head, oldest first (decoded bodies).
    pub states: Vec<Acs>,
    pub policy: RecoveryPolicy,
    pub credentials: Vec<Credential>,
}

pub(crate) fn signed_body(signed: &[u8]) -> Result<Vec<u8>, Error> {
    Ok(cbor::decode(signed)?.get(1)?.b()?.to_vec())
}

impl RecoveryRecord {
    /// The cross-field rules of ADR-0199 section 4 (4): head hash and generation against key 4; the tail links by
    /// `previous_state_hash` and its last state is the head's predecessor; one account; the credential set is exactly
    /// the one the states reference and each body hashes to its reference; the policy hashes to the head's commitment;
    /// `record_generation >> 20` is the head's generation. Signatures are not opened here: each carried state still
    /// verifies against the chain rules when it is used (section 1), and this result is advisory.
    pub fn verify(&self) -> Result<Verified, Error> {
        let ev = &self.evidence;
        if ev.tail.len() > MAX_STATES - 1 {
            return Err(Error::Binding("record: tail longer than 8"));
        }
        let mut states = Vec::new();
        let mut hashes = Vec::new();
        for s in ev.tail.iter().chain(std::iter::once(&ev.head)) {
            let acs = Acs::from_body(&signed_body(s)?)?;
            hashes.push(acs.hash());
            states.push(acs);
        }
        let head = states.last().expect("head").clone();
        if head.hash() != self.head.state_hash || head.generation != self.head.generation {
            return Err(Error::Binding("record: head does not match key 4"));
        }
        if generation_acs(self.generation) != head.generation {
            return Err(Error::Binding(
                "record: generation shift does not match the head",
            ));
        }
        if self.generation == 0 {
            return Err(Error::Binding("record: generation zero"));
        }
        // Every state is for one account and each links to the one before it (the last tail state is the head's predecessor).
        for (i, s) in states.iter().enumerate() {
            if s.account_id != head.account_id {
                return Err(Error::Binding("record: states of another account"));
            }
            if i > 0 && s.previous != Some(hashes[i - 1]) {
                return Err(Error::Binding("record: tail does not link to the head"));
            }
        }
        // Credentials: exactly the references of the states, each body hashing to its reference, no duplicates.
        let referenced: BTreeSet<[u8; 32]> = states
            .iter()
            .flat_map(|s| s.credentials.iter().map(|(_, h)| *h))
            .collect();
        if ev.credentials.len() > MAX_CREDENTIALS
            || ev.credentials.iter().map(Vec::len).sum::<usize>() > MAX_CREDENTIAL_BYTES_TOTAL
        {
            return Err(Error::Binding("record: credential bounds"));
        }
        let mut seen = BTreeSet::new();
        let mut credentials = Vec::new();
        for body in &ev.credentials {
            let c = Credential::from_body(body)?;
            if c.body() != *body {
                return Err(Error::Binding("record: credential not canonical"));
            }
            let h = c.hash();
            if !referenced.contains(&h) || !seen.insert(h) {
                return Err(Error::Binding("record: credential extra or duplicated"));
            }
            credentials.push(c);
        }
        if seen != referenced {
            return Err(Error::Binding("record: credential missing"));
        }
        // The policy body hashes to the head's commitment.
        let policy = RecoveryPolicy::from_body(&ev.policy)?;
        if head.recovery_commitment != Some(policy.hash()) {
            return Err(Error::Binding("record: policy does not match the head"));
        }
        if self.providers.is_empty() || self.providers.len() > MAX_PROVIDER_HOMES {
            return Err(Error::Binding("record: provider homes 1..4"));
        }
        if let Relationships::Plain(s) = &self.relationships
            && s.entries.len() > MAX_RELATIONSHIPS
        {
            return Err(Error::Binding("record: too many relationships"));
        }
        Ok(Verified {
            head,
            states,
            policy,
            credentials,
        })
    }
}

/// ADR-0141 freshness against a durable high-water mark `(generation, plaintext digest)`: a lower generation is
/// stale, the same generation with different authenticated plaintext is equivocation (fail closed).
#[derive(Debug, PartialEq, Eq)]
pub enum Freshness {
    Newer,
    Same,
    Stale,
    Equivocation,
}

pub fn freshness(mark: Option<(u64, [u8; 32])>, generation: u64, plaintext: &[u8]) -> Freshness {
    let h = plaintext_digest(plaintext);
    match mark {
        None => Freshness::Newer,
        Some((g, _)) if generation > g => Freshness::Newer,
        Some((g, _)) if generation < g => Freshness::Stale,
        Some((_, d)) if d == h => Freshness::Same,
        Some(_) => Freshness::Equivocation,
    }
}

/// The local digest a high-water mark stores for a plaintext: plain SHA-256, a local comparison value that is never
/// sent anywhere and is not a wire construction.
pub fn plaintext_digest(plaintext: &[u8]) -> [u8; 32] {
    crate::crypto::sha256(&[plaintext])
}

// -------------------------------------------------------------------------------------------- RecoveryKit

/// `RecoveryKit` (type 38): device-private, class `om.recovery_secret`. Holds no recovery signing key: it lets a
/// controller device write and read the record, never start a recovery. Wiped on drop, never printed, never in a
/// backup, never sent to a Home (ADR-0199 section 5).
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryKit {
    pub scope: [u8; 32],
    pub host: String,
    pub port: Option<u16>,
    pub read_cap: [u8; 32],
    pub write_cap: [u8; 32],
    pub record_key: [u8; 32],
    /// Random, not derived from the RecoveryRoot (ADR-0201 section 3).
    pub inbox_cap: [u8; 32],
    pub snapshot_key: Option<[u8; 32]>,
    pub epoch: u64,
}

impl core::fmt::Debug for RecoveryKit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RecoveryKit(<redacted>)")
    }
}

impl Drop for RecoveryKit {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.read_cap);
        zeroize::Zeroize::zeroize(&mut self.write_cap);
        zeroize::Zeroize::zeroize(&mut self.record_key);
        zeroize::Zeroize::zeroize(&mut self.inbox_cap);
        if let Some(k) = &mut self.snapshot_key {
            zeroize::Zeroize::zeroize(k);
        }
    }
}

impl RecoveryKit {
    /// Made on the device that generates the RecoveryRoot, while the root is in memory.
    pub fn make(
        root: &[u8; 32],
        scope: [u8; 32],
        host: &str,
        port: Option<u16>,
        epoch: u64,
        inbox_cap: [u8; 32],
        with_snapshot_key: bool,
    ) -> Self {
        let k = RecordKeys::derive(root, &scope);
        RecoveryKit {
            scope,
            host: host.into(),
            port,
            read_cap: k.read_cap,
            write_cap: k.write_cap,
            record_key: k.record_key,
            inbox_cap,
            snapshot_key: with_snapshot_key.then_some(k.snapshot_key),
            epoch,
        }
    }

    pub fn locator(&self) -> [u8; 32] {
        locator_of(&self.read_cap)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut m = vec![
            (0, V::U(TYPE_KIT)),
            (1, V::U(1)),
            (2, V::B(self.scope.to_vec())),
            (3, V::T(self.host.clone())),
            (5, V::B(self.read_cap.to_vec())),
            (6, V::B(self.write_cap.to_vec())),
            (7, V::B(self.record_key.to_vec())),
            (8, V::B(self.inbox_cap.to_vec())),
            (10, V::U(self.epoch)),
        ];
        if let Some(p) = self.port {
            m.push((4, V::U(p as u64)));
        }
        if let Some(s) = &self.snapshot_key {
            m.push((9, V::B(s.to_vec())));
        }
        cbor::encode(&V::M(m))
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        if b.len() > MAX_KIT_BYTES {
            return Err(bad("kit: too large"));
        }
        crate::flows::registry()
            .check("RecoveryKit", b, false)
            .map_err(|_| bad("kit: invalid"))?;
        let v = cbor::decode(b)?;
        Ok(RecoveryKit {
            scope: v.get(2)?.b32()?,
            host: v.get(3)?.t()?.to_string(),
            port: v
                .opt(4)
                .map(|p| {
                    p.u()
                        .and_then(|n| u16::try_from(n).map_err(|_| bad("kit: port")))
                })
                .transpose()?,
            read_cap: v.get(5)?.b32()?,
            write_cap: v.get(6)?.b32()?,
            record_key: v.get(7)?.b32()?,
            inbox_cap: v.get(8)?.b32()?,
            snapshot_key: v.opt(9).map(|s| s.b32()).transpose()?,
            epoch: v.get(10)?.u()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_arithmetic() {
        assert_eq!(record_generation(3, 0).unwrap(), 3 << 20);
        assert_eq!(record_generation(3, 7).unwrap(), (3 << 20) | 7);
        assert_eq!(generation_acs((3 << 20) | 7), 3);
        assert_eq!(generation_refresh((3 << 20) | 7), 7);
        assert!(record_generation(0, 0).is_err(), "zero is not a generation");
        assert!(record_generation(1, 1 << 20).is_err());
        assert!(record_generation(MAX_ACS_GENERATION + 1, 0).is_err());
        assert!(record_generation(MAX_ACS_GENERATION, (1 << 20) - 1).is_ok());
        // Two writers of one head never share a slot: author slot 0, steward slots >= 1.
        assert!(record_generation(5, 0).unwrap() < record_generation(5, 1).unwrap());
        assert!(record_generation(5, (1 << 20) - 1).unwrap() < record_generation(6, 0).unwrap());
    }

    #[test]
    fn pad_lengths_hit_buckets_exactly_or_move_up() {
        for bucket in BUCKETS {
            let mut ok = 0;
            for base in (bucket - 600)..=bucket {
                match pad_length(base, bucket) {
                    Some(l) => {
                        ok += 1;
                        assert_eq!(base + 1 + bstr_item_len(l) + l, bucket, "base {base}");
                    }
                    None => assert!(
                        bucket - base < 3 || [26, 259, 65_538].contains(&(bucket - base)),
                        "unexpected gap at distance {}",
                        bucket - base
                    ),
                }
            }
            assert!(ok > 590);
        }
        assert!(
            pad_length(16_382, 16_384).is_none(),
            "within 3 bytes moves up"
        );
        assert!(pad_length(16_381, 16_384).is_some());
    }

    #[test]
    fn keys_are_label_and_scope_separated() {
        let root = [9u8; 32];
        let a = RecordKeys::derive(&root, &[1; 32]);
        let b = RecordKeys::derive(&root, &[2; 32]);
        assert_ne!(a.read_cap, b.read_cap);
        assert_ne!(a.read_cap, a.write_cap);
        assert_ne!(a.record_key, a.snapshot_key);
        assert_eq!(a.locator, locator_of(&a.read_cap));
        assert_eq!(format!("{a:?}"), "RecordKeys(<redacted>)");
    }
}
