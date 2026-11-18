// SPDX-License-Identifier: MPL-2.0
//! The recovery approval exchange (ADR-0203 sections 2 and 3): a purpose-bound ceremony between a restricted owner device
//! and a surviving controller, built on the ADR-0192 mechanism with its own labels, version and plaintext types so that
//! neither ceremony can be read as the other. Pure functions and codecs over bytes; the flow is `flows/recovery_approval.rs`
//! and the independent vectors are `verification/vectors/owner-recovery-vectors.json.gz`.
//!
//! Every message is strict: it decodes, then re-encodes to exactly the bytes received, within its bounds; anything else is
//! refused whole. Nothing here carries the RecoveryRoot, a session or history key, or any time credit.

use super::cbor::{self, V};
use super::kdf::{hkdf_sha256, hmac_sha256};
use super::link_ceremony::CeremonyKeys;
use super::sig::ds_hash;
use crate::recovery::SuccessorBundle;

pub const LBL_KDF: &str = "OM-RECOVERY-APPROVAL-KDF-v1";
pub const LBL_AD: &str = "OM-RECOVERY-APPROVAL-AEAD-AD-v1";
pub const LBL_MAC: &str = "OM-RECOVERY-APPROVAL-MAC-v1";
pub const LBL_MAILBOX: &str = "OM-RECOVERY-APPROVAL-MAILBOX-HASH-v1";
pub const LBL_EVIDENCE: &str = "OM-RECOVERY-APPROVAL-EVIDENCE-HASH-v1";

pub const OFFER_PREFIX: &str = "om:recover-approve/";
pub const OFFER_VERSION: u8 = 1;
pub const MAX_OFFER_TOKEN: usize = 200;
pub const ROLE_REQ: u8 = 1;
pub const ROLE_RESP: u8 = 2;
/// Evidence is cut into consecutive slices of this many bytes (the last one shorter).
pub const CHUNK_BYTES: usize = 50_000;
pub const MAX_CHUNKS: usize = 64;
pub const MAX_EVIDENCE_BYTES: usize = 3_200_000;
pub const MAX_STATES: usize = 65;
pub const MAX_CREDS: usize = 320;
pub const MAX_DOSSIERS: usize = 4;
pub const MAX_VETOES: usize = 16;
pub const MAX_PROPOSAL_BYTES: usize = 98_976;

pub const TYPE_PROPOSAL: u64 = 40;
pub const TYPE_REPLY: u64 = 41;
pub const TYPE_CHUNK: u64 = 42;
pub const TYPE_EVIDENCE: u64 = 43;

// ------------------------------------------------------------------------------------------------ construction

/// `CTX = OM-CBOR ["recovery_approval", 1, expiry_s, home_uri, A_cap, B_recv]`.
pub fn context(expiry_s: u64, home_uri: &[u8], a_cap: &[u8], b_recv: &[u8]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T("recovery_approval".into()),
        V::U(1),
        V::U(expiry_s),
        V::B(home_uri.to_vec()),
        V::B(a_cap.to_vec()),
        V::B(b_recv.to_vec()),
    ]))
}

/// The key schedule of ADR-0192 with this ceremony's salt and info (`CBOR [KDF, purpose]`).
pub fn keys(
    expiry_s: u64,
    home_uri: &[u8],
    secret: &[u8],
    a_cap: &[u8],
    b_recv: &[u8],
) -> CeremonyKeys {
    keys_in_context(&context(expiry_s, home_uri, a_cap, b_recv), secret)
}

pub(crate) fn keys_in_context(context: &[u8], secret: &[u8]) -> CeremonyKeys {
    let id = ds_hash(LBL_KDF, context);
    let derive = |purpose: &str| {
        let mut out = [0u8; 32];
        let info = cbor::encode(&V::A(vec![V::T(LBL_KDF.into()), V::T(purpose.into())]));
        hkdf_sha256(&id, secret, &info, &mut out);
        out
    };
    CeremonyKeys {
        id,
        req: derive("req"),
        resp: derive("resp"),
        code: derive("code"),
        bind: derive("bind"),
    }
}

/// `AD = CBOR [AD, id, role]`, role 1 restricted device to controller, role 2 the reverse.
pub fn aead_ad(id: &[u8; 32], role: u8) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(LBL_AD.into()),
        V::B(id.to_vec()),
        V::U(u64::from(role)),
    ]))
}

fn mac_in(role: u64, body: &[u8]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(LBL_MAC.into()),
        V::U(role),
        V::B(body.to_vec()),
    ]))
}

/// The six digits both devices show: `HMAC(K_code, MACIN(1, proposal_bytes))` as ADR-0192 derives them. It covers the
/// whole exact proposal.
pub fn code_digits(k_code: &[u8; 32], proposal: &[u8]) -> String {
    let m = hmac_sha256(k_code, &mac_in(1, proposal));
    super::link_ceremony::code_digits(&m)
}

/// `tag = HMAC(K_bind, MACIN(2, CBOR [id, SHA-256(proposal), descriptor_bytes]))`.
pub fn reply_tag(k_bind: &[u8; 32], id: &[u8; 32], proposal: &[u8], descriptor: &[u8]) -> [u8; 32] {
    let body = cbor::encode(&V::A(vec![
        V::B(id.to_vec()),
        V::B(super::sha256(&[proposal]).to_vec()),
        V::B(descriptor.to_vec()),
    ]));
    hmac_sha256(k_bind, &mac_in(2, &body))
}

/// The local polling id of the response queue: the first 16 bytes of `DS-HASH(MAILBOX, B_recv)`.
pub fn mailbox(b_recv: &[u8]) -> [u8; 16] {
    let h = ds_hash(LBL_MAILBOX, b_recv);
    let mut o = [0u8; 16];
    o.copy_from_slice(&h[..16]);
    o
}

pub fn evidence_digest(evidence: &[u8]) -> [u8; 32] {
    ds_hash(LBL_EVIDENCE, evidence)
}

// ------------------------------------------------------------------------------------------------ offer link

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub expiry_s: u32,
    pub secret: [u8; 16],
    pub a_cap: Vec<u8>,
    pub b_recv: Vec<u8>,
    pub uri: String,
}

/// Why an offer string was not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferError {
    /// Not an `om:recover-approve/` string at all (an enrollment link, or anything else): never reinterpreted.
    Prefix,
    /// An unsupported version byte.
    Version,
    Malformed,
}

pub fn encode_offer(o: &Offer) -> Option<String> {
    let ok = |c: &[u8]| (16..=64).contains(&c.len());
    if !ok(&o.a_cap) || !ok(&o.b_recv) || !uri_ok(&o.uri) {
        return None;
    }
    let mut v = vec![OFFER_VERSION];
    v.extend_from_slice(&o.expiry_s.to_be_bytes());
    v.extend_from_slice(&o.secret);
    v.push(o.a_cap.len() as u8);
    v.extend_from_slice(&o.a_cap);
    v.push(o.b_recv.len() as u8);
    v.extend_from_slice(&o.b_recv);
    v.extend_from_slice(o.uri.as_bytes());
    let s = format!("{OFFER_PREFIX}{}", crate::flows::b64url(&v));
    (s.len() - OFFER_PREFIX.len() <= MAX_OFFER_TOKEN).then_some(s)
}

fn uri_ok(u: &str) -> bool {
    !u.is_empty() && u.len() <= 2048 && u.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

pub fn decode_offer(link: &str) -> Result<Offer, OfferError> {
    let token = link
        .trim()
        .strip_prefix(OFFER_PREFIX)
        .ok_or(OfferError::Prefix)?;
    if token.len() > MAX_OFFER_TOKEN {
        return Err(OfferError::Malformed);
    }
    let b = crate::flows::unb64url(token).ok_or(OfferError::Malformed)?;
    if b.first() != Some(&OFFER_VERSION) {
        return Err(if b.is_empty() {
            OfferError::Malformed
        } else {
            OfferError::Version
        });
    }
    let bad = || OfferError::Malformed;
    let expiry_s = u32::from_be_bytes(b.get(1..5).ok_or_else(bad)?.try_into().map_err(|_| bad())?);
    let secret: [u8; 16] = b
        .get(5..21)
        .ok_or_else(bad)?
        .try_into()
        .map_err(|_| bad())?;
    let mut at = 21;
    let take = |at: &mut usize| -> Result<Vec<u8>, OfferError> {
        let n = *b.get(*at).ok_or_else(bad)? as usize;
        let v = b.get(*at + 1..*at + 1 + n).ok_or_else(bad)?.to_vec();
        *at += 1 + n;
        if (16..=64).contains(&v.len()) {
            Ok(v)
        } else {
            Err(bad())
        }
    };
    let a_cap = take(&mut at)?;
    let b_recv = take(&mut at)?;
    let uri = core::str::from_utf8(&b[at..])
        .map_err(|_| bad())?
        .to_string();
    if !uri_ok(&uri) {
        return Err(bad());
    }
    let o = Offer {
        expiry_s,
        secret,
        a_cap,
        b_recv,
        uri,
    };
    (encode_offer(&o).as_deref() == Some(link.trim()))
        .then_some(o)
        .ok_or(OfferError::Malformed)
}

// ---------------------------------------------------------------------------------------------------- messages

fn head(ty: u64) -> Vec<(u64, V)> {
    vec![(0, V::U(ty)), (1, V::U(1))]
}

fn strict(b: &[u8], max: usize) -> Option<V> {
    if b.len() > max {
        return None;
    }
    cbor::decode(b).ok()
}

fn bundle_v(s: &SuccessorBundle) -> V {
    V::M(vec![
        (0, V::B(s.child.clone())),
        (1, V::B(s.next_policy.clone())),
        (2, V::B(s.credential.clone())),
    ])
}

fn bundle_of(v: &V) -> Option<SuccessorBundle> {
    Some(SuccessorBundle {
        child: v.get(0).ok()?.b().ok()?.to_vec(),
        next_policy: v.get(1).ok()?.b().ok()?.to_vec(),
        credential: v.get(2).ok()?.b().ok()?.to_vec(),
    })
}

/// The first message: the exact signed request and the recovery-signed successor, each as original bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub request: Vec<u8>,
    pub bundle: SuccessorBundle,
}

impl Proposal {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut m = head(TYPE_PROPOSAL);
        m.push((2, V::B(self.request.clone())));
        m.push((3, bundle_v(&self.bundle)));
        cbor::encode(&V::M(m))
    }
    pub fn from_bytes(b: &[u8]) -> Option<Proposal> {
        let v = strict(b, MAX_PROPOSAL_BYTES)?;
        if v.get(0).ok()?.u().ok()? != TYPE_PROPOSAL || v.get(1).ok()?.u().ok()? != 1 {
            return None;
        }
        let p = Proposal {
            request: v.get(2).ok()?.b().ok()?.to_vec(),
            bundle: bundle_of(v.get(3).ok()?)?,
        };
        (p.to_bytes() == b).then_some(p)
    }
}

/// What the reply announces: the exact request, parent, target, approving controller, proposed child and the evidence
/// that follows in chunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub request_digest: [u8; 32],
    pub parent: [u8; 32],
    pub target: [u8; 32],
    pub controller: [u8; 16],
    pub child: [u8; 32],
    pub evidence_digest: [u8; 32],
    pub evidence_len: u64,
    pub chunks: u64,
}

impl Descriptor {
    pub fn to_bytes(&self) -> Vec<u8> {
        cbor::encode(&self.to_v())
    }
    fn to_v(&self) -> V {
        V::M(vec![
            (0, V::B(self.request_digest.to_vec())),
            (1, V::B(self.parent.to_vec())),
            (2, V::B(self.target.to_vec())),
            (3, V::B(self.controller.to_vec())),
            (4, V::B(self.child.to_vec())),
            (5, V::B(self.evidence_digest.to_vec())),
            (6, V::U(self.evidence_len)),
            (7, V::U(self.chunks)),
        ])
    }
    pub fn from_bytes(b: &[u8]) -> Option<Descriptor> {
        let v = strict(b, 1024)?;
        let d = Descriptor {
            request_digest: v.get(0).ok()?.b32().ok()?,
            parent: v.get(1).ok()?.b32().ok()?,
            target: v.get(2).ok()?.b32().ok()?,
            controller: v.get(3).ok()?.b().ok()?.try_into().ok()?,
            child: v.get(4).ok()?.b32().ok()?,
            evidence_digest: v.get(5).ok()?.b32().ok()?,
            evidence_len: v.get(6).ok()?.u().ok()?,
            chunks: v.get(7).ok()?.u().ok()?,
        };
        let sound = (1..=MAX_EVIDENCE_BYTES as u64).contains(&d.evidence_len)
            && (1..=MAX_CHUNKS as u64).contains(&d.chunks)
            && d.chunks == d.evidence_len.div_ceil(CHUNK_BYTES as u64);
        (sound && d.to_bytes() == b).then_some(d)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub descriptor: Descriptor,
    pub tag: [u8; 32],
}

impl Reply {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut m = head(TYPE_REPLY);
        m.push((2, self.descriptor.to_v()));
        m.push((3, V::B(self.tag.to_vec())));
        cbor::encode(&V::M(m))
    }
    /// The descriptor travels as its own canonical bytes inside the reply (the tag covers exactly those bytes).
    pub fn from_bytes(b: &[u8]) -> Option<(Reply, Vec<u8>)> {
        let v = strict(b, 1_024)?;
        if v.get(0).ok()?.u().ok()? != TYPE_REPLY || v.get(1).ok()?.u().ok()? != 1 {
            return None;
        }
        let raw = cbor::encode(v.get(2).ok()?);
        let r = Reply {
            descriptor: Descriptor::from_bytes(&raw)?,
            tag: v.get(3).ok()?.b32().ok()?,
        };
        (r.to_bytes() == b).then_some((r, raw))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub digest: [u8; 32],
    pub index: u64,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut m = head(TYPE_CHUNK);
        m.push((2, V::B(self.digest.to_vec())));
        m.push((3, V::U(self.index)));
        m.push((4, V::B(self.data.clone())));
        cbor::encode(&V::M(m))
    }
    pub fn from_bytes(b: &[u8]) -> Option<Chunk> {
        let v = strict(b, CHUNK_BYTES + 128)?;
        if v.get(0).ok()?.u().ok()? != TYPE_CHUNK || v.get(1).ok()?.u().ok()? != 1 {
            return None;
        }
        let c = Chunk {
            digest: v.get(2).ok()?.b32().ok()?,
            index: v.get(3).ok()?.u().ok()?,
            data: v.get(4).ok()?.b().ok()?.to_vec(),
        };
        let sound =
            c.index < MAX_CHUNKS as u64 && !c.data.is_empty() && c.data.len() <= CHUNK_BYTES;
        (sound && c.to_bytes() == b).then_some(c)
    }
}

/// One dossier entry: a recovery link in the chain, with its committed policies and its exact signed request. No time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dossier {
    pub child_hash: [u8; 32],
    pub policy: Vec<u8>,
    pub next_policy: Vec<u8>,
    pub request: Vec<u8>,
}

/// The evidence the reply carries, in chunks: signed states through the controller's verified head, the referenced
/// credential bodies, dossiers, observed vetoes and, when approved, the co-signed successor.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Evidence {
    pub states: Vec<Vec<u8>>,
    pub creds: Vec<Vec<u8>>,
    pub dossiers: Vec<Dossier>,
    pub vetoes: Vec<Vec<u8>>,
    pub successor: Option<SuccessorBundle>,
}

impl Evidence {
    pub fn to_bytes(&self) -> Vec<u8> {
        let list = |l: &[Vec<u8>]| V::A(l.iter().cloned().map(V::B).collect());
        let mut m = head(TYPE_EVIDENCE);
        m.push((2, list(&self.states)));
        m.push((3, list(&self.creds)));
        m.push((
            4,
            V::A(
                self.dossiers
                    .iter()
                    .map(|d| {
                        V::M(vec![
                            (
                                0,
                                V::M(vec![(0, V::U(1)), (1, V::B(d.child_hash.to_vec()))]),
                            ),
                            (1, V::B(d.policy.clone())),
                            (2, V::B(d.next_policy.clone())),
                            (3, V::B(d.request.clone())),
                        ])
                    })
                    .collect(),
            ),
        ));
        m.push((5, list(&self.vetoes)));
        if let Some(s) = &self.successor {
            m.push((6, bundle_v(s)));
        }
        cbor::encode(&V::M(m))
    }

    pub fn from_bytes(b: &[u8]) -> Option<Evidence> {
        let v = strict(b, MAX_EVIDENCE_BYTES)?;
        if v.get(0).ok()?.u().ok()? != TYPE_EVIDENCE || v.get(1).ok()?.u().ok()? != 1 {
            return None;
        }
        let list = |k: u64, max: usize, item: usize| -> Option<Vec<Vec<u8>>> {
            let a = v.get(k).ok()?.a().ok()?;
            if a.len() > max {
                return None;
            }
            a.iter()
                .map(|x| x.b().ok().filter(|b| b.len() <= item).map(<[u8]>::to_vec))
                .collect()
        };
        let dossiers: Vec<Dossier> = {
            let a = v.get(4).ok()?.a().ok()?;
            if a.len() > MAX_DOSSIERS {
                return None;
            }
            a.iter()
                .map(|e| {
                    let d = e.get(0).ok()?;
                    if d.get(0).ok()?.u().ok()? != 1 {
                        return None;
                    }
                    Some(Dossier {
                        child_hash: d.get(1).ok()?.b32().ok()?,
                        policy: e
                            .get(1)
                            .ok()?
                            .b()
                            .ok()
                            .filter(|b| b.len() <= 24_576)?
                            .to_vec(),
                        next_policy: e
                            .get(2)
                            .ok()?
                            .b()
                            .ok()
                            .filter(|b| b.len() <= 24_576)?
                            .to_vec(),
                        request: e
                            .get(3)
                            .ok()?
                            .b()
                            .ok()
                            .filter(|b| b.len() <= 16_384)?
                            .to_vec(),
                    })
                })
                .collect::<Option<_>>()?
        };
        let e = Evidence {
            states: list(2, MAX_STATES, 65_536).filter(|l| !l.is_empty())?,
            creds: list(3, MAX_CREDS, 32_768).filter(|l| !l.is_empty())?,
            dossiers,
            vetoes: list(5, MAX_VETOES, 16_384)?,
            successor: match v.opt(6) {
                None => None,
                Some(x) => Some(bundle_of(x)?),
            },
        };
        let sound = v.opt(6).is_none() || e.successor.is_some();
        (sound && e.to_bytes() == b).then_some(e)
    }
}

// ---------------------------------------------------------------------------------------------------- chunking

/// Consecutive slices of `bytes`, `size` each and only the last shorter.
pub fn split(bytes: &[u8], size: usize) -> Vec<Vec<u8>> {
    bytes.chunks(size).map(<[u8]>::to_vec).collect()
}

/// Why chunks could not be assembled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkError {
    /// Another digest, an index out of range, or a size that does not fit the announced evidence.
    Rejected,
    /// The same index with different bytes: the ceremony is aborted.
    Conflict,
}

/// Collects the chunks of one announced evidence. An early chunk (before its manifest) is the caller's to hold or drop;
/// arrival order may differ from index order; an identical duplicate is a no-op.
#[derive(Clone, Debug)]
pub struct Assembler {
    pub descriptor: Descriptor,
    size: usize,
    got: Vec<Option<Vec<u8>>>,
}

impl Assembler {
    pub fn new(descriptor: Descriptor) -> Assembler {
        Assembler::with_size(descriptor, CHUNK_BYTES)
    }

    /// As `new` with another slice size (tests of the logic only; the wire value is [`CHUNK_BYTES`]).
    pub fn with_size(descriptor: Descriptor, size: usize) -> Assembler {
        let n = descriptor.chunks as usize;
        Assembler {
            descriptor,
            size,
            got: vec![None; n],
        }
    }

    pub fn add(&mut self, c: &Chunk) -> Result<(), ChunkError> {
        let d = &self.descriptor;
        if c.digest != d.evidence_digest || c.index >= d.chunks {
            return Err(ChunkError::Rejected);
        }
        let last = c.index + 1 == d.chunks;
        let want = if last {
            d.evidence_len as usize - self.size * (d.chunks as usize - 1)
        } else {
            self.size
        };
        if c.data.len() != want {
            return Err(ChunkError::Rejected);
        }
        match &self.got[c.index as usize] {
            Some(x) if *x == c.data => Ok(()),
            Some(_) => Err(ChunkError::Conflict),
            None => {
                self.got[c.index as usize] = Some(c.data.clone());
                Ok(())
            }
        }
    }

    pub fn indexes(&self) -> Vec<u64> {
        self.got
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|_| i as u64))
            .collect()
    }

    /// The whole evidence once every index is present, its length and digest match the manifest.
    pub fn assemble(&self) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(self.descriptor.evidence_len as usize);
        for c in &self.got {
            out.extend_from_slice(c.as_ref()?);
        }
        (out.len() as u64 == self.descriptor.evidence_len
            && evidence_digest(&out) == self.descriptor.evidence_digest)
            .then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(len: usize, size: usize, digest: [u8; 32]) -> Descriptor {
        Descriptor {
            request_digest: [1; 32],
            parent: [2; 32],
            target: [3; 32],
            controller: [4; 16],
            child: [5; 32],
            evidence_digest: digest,
            evidence_len: len as u64,
            chunks: len.div_ceil(size) as u64,
        }
    }

    fn assembler(data: &[u8], size: usize) -> (Assembler, Vec<Chunk>) {
        let digest = evidence_digest(data);
        let mut d = desc(data.len(), size, digest);
        // the wire count uses the real slice size; these tests use a small one
        d.chunks = data.len().div_ceil(size) as u64;
        let chunks = split(data, size)
            .into_iter()
            .enumerate()
            .map(|(i, data)| Chunk {
                digest,
                index: i as u64,
                data,
            })
            .collect();
        (Assembler::with_size(d, size), chunks)
    }

    #[test]
    fn missing_chunks_never_assemble_and_a_late_one_completes_it_in_any_order() {
        let data: Vec<u8> = (0..1000u32).map(|x| x as u8).collect();
        let (mut a, c) = assembler(&data, 300);
        assert_eq!(c.len(), 4);
        a.add(&c[2]).unwrap();
        a.add(&c[0]).unwrap();
        a.add(&c[3]).unwrap();
        assert_eq!(a.assemble(), None, "chunk 1 is missing");
        assert_eq!(a.indexes(), vec![0, 2, 3]);
        a.add(&c[2]).unwrap(); // an identical duplicate is a no-op
        a.add(&c[1]).unwrap(); // the late one
        assert_eq!(a.assemble().unwrap(), data);
    }

    #[test]
    fn a_conflicting_duplicate_wrong_digest_or_size_is_refused() {
        let data = vec![7u8; 700];
        let (mut a, c) = assembler(&data, 300);
        a.add(&c[0]).unwrap();
        let mut other = c[0].clone();
        other.data[0] ^= 1;
        assert_eq!(a.add(&other), Err(ChunkError::Conflict));
        let mut wrong = c[1].clone();
        wrong.digest = [9; 32];
        assert_eq!(a.add(&wrong), Err(ChunkError::Rejected));
        let mut short = c[1].clone();
        short.data.pop();
        assert_eq!(a.add(&short), Err(ChunkError::Rejected));
        let mut far = c[1].clone();
        far.index = 5;
        assert_eq!(a.add(&far), Err(ChunkError::Rejected));
        // a substituted chunk with the right size changes the digest, so assembly fails
        let mut b2 = assembler(&data, 300).0;
        for ch in &c {
            b2.add(ch).unwrap();
        }
        let mut forged = c[1].clone();
        forged.data[3] ^= 1;
        let mut b3 = assembler(&data, 300).0;
        b3.add(&c[0]).unwrap();
        b3.add(&forged).unwrap();
        b3.add(&c[2]).unwrap();
        assert_eq!(b3.assemble(), None);
        assert!(b2.assemble().is_some());
    }

    #[test]
    fn messages_are_strict() {
        let p = Proposal {
            request: vec![1; 40],
            bundle: SuccessorBundle {
                child: vec![2; 30],
                next_policy: vec![3; 30],
                credential: vec![4; 30],
            },
        };
        assert_eq!(Proposal::from_bytes(&p.to_bytes()), Some(p.clone()));
        let mut extra = p.to_bytes();
        extra.push(0);
        assert_eq!(Proposal::from_bytes(&extra), None);
        let mut crossed = p.to_bytes();
        crossed[3] = 41; // the type byte of another message
        assert_eq!(Proposal::from_bytes(&crossed), None);
        let d = desc(100_000, CHUNK_BYTES, [8; 32]);
        let r = Reply {
            descriptor: d.clone(),
            tag: [6; 32],
        };
        assert_eq!(Reply::from_bytes(&r.to_bytes()).map(|x| x.0), Some(r));
        let mut bad = d;
        bad.chunks = 3;
        assert_eq!(Descriptor::from_bytes(&bad.to_bytes()), None);
    }
}
