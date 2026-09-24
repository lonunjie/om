// SPDX-License-Identifier: Apache-2.0
//! Device-side prekey store + sessions, and a toy KeyMaterialProvider (`ASYNC_KEY_MATERIAL.md`
//! §2 publish/fetch/inventory). Everything persists as deterministic CBOR.
//!
//! One-time semantics: the provider hands out each one-time item at most once (pop), and the
//! device deletes a one-time private key only after the initial ciphertext authenticates
//! ([PQXDH] §3.4). A second use of the same id therefore fails even if a provider misbehaves.
//! When one-time PQ keys run out the provider returns the signed last-resort PQ key
//! ([PQXDH] §3.3); there is no bundle shape without a PQ key.

use crate::cbor::{self, V};
use crate::kdf::LR_REPLAY_TAG;
use crate::pqxdh::{self, encode_ec, encode_kem, InitHeader, PqKind, PrekeyBundle};
use crate::rng::{arr32, arr64, CryptoRng, RngCore};
use crate::triple::TripleState;
use crate::{sha256, xeddsa, Error};
use std::collections::{BTreeMap, BTreeSet};

/// Bound on the last-resort replay cache (OM policy; oldest-by-value evicted deterministically).
pub const LR_REPLAY_CACHE_MAX: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEc {
    pub id: u32,
    pub secret: [u8; 32],
    pub sig: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedPq {
    pub id: u32,
    pub seed: [u8; 64],
    pub sig: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub tr: TripleState,
    pub initiator: bool,
    /// Alice: PQXDH prefix re-sent until Bob's first reply ([DR] §7.1).
    pub pending_init: Option<InitHeader>,
    /// EK_A of the handshake that created the session (matches repeated prefixes).
    pub init_ek: [u8; 32],
    pub pq_kind: PqKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub ik_secret: [u8; 32],
    pub next_id: u32,
    pub spk: SignedEc,
    pub prev_spk: Option<SignedEc>,
    pub last_resort: SignedPq,
    pub prev_last_resort: Option<SignedPq>,
    pub opks: BTreeMap<u32, [u8; 32]>,
    pub pqopks: BTreeMap<u32, SignedPq>,
    pub lr_replay: BTreeSet<[u8; 32]>,
    /// Sessions keyed by EncodeEC(peer IK).
    pub sessions: BTreeMap<Vec<u8>, Session>,
}

/// What a device uploads to a provider: public halves only.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Upload {
    pub opks: Vec<(u32, [u8; 32])>,
    pub pqopks: Vec<(u32, Vec<u8>, [u8; 64])>,
}

impl Device {
    pub fn new<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let ik_secret = arr32(rng);
        let spk = Self::make_spk(&ik_secret, 1, rng);
        let last_resort = Self::make_pq(&ik_secret, 2, rng);
        Device {
            ik_secret,
            next_id: 3,
            spk,
            prev_spk: None,
            last_resort,
            prev_last_resort: None,
            opks: BTreeMap::new(),
            pqopks: BTreeMap::new(),
            lr_replay: BTreeSet::new(),
            sessions: BTreeMap::new(),
        }
    }

    pub fn ik(&self) -> [u8; 32] {
        pqxdh::x25519_pub(&self.ik_secret)
    }

    fn make_spk<R: RngCore + CryptoRng>(ik: &[u8; 32], id: u32, rng: &mut R) -> SignedEc {
        let secret = arr32(rng);
        let z = arr64(rng);
        let sig = xeddsa::sign(ik, &encode_ec(&pqxdh::x25519_pub(&secret)), &z);
        SignedEc { id, secret, sig }
    }

    fn make_pq<R: RngCore + CryptoRng>(ik: &[u8; 32], id: u32, rng: &mut R) -> SignedPq {
        let seed = arr64(rng);
        let z = arr64(rng);
        let sig = xeddsa::sign(ik, &encode_kem(&pqxdh::mlkem_pub(&seed)), &z);
        SignedPq { id, seed, sig }
    }

    fn alloc_id(&mut self) -> u32 {
        let i = self.next_id;
        self.next_id += 1;
        i
    }

    /// Generate one-time prekeys ([PQXDH] §3.2) and return the public upload for ONE provider pool.
    pub fn generate_one_time<R: RngCore + CryptoRng>(&mut self, n_ec: usize, n_pq: usize, rng: &mut R) -> Upload {
        let mut up = Upload::default();
        for _ in 0..n_ec {
            let id = self.alloc_id();
            let s = arr32(rng);
            self.opks.insert(id, s);
            up.opks.push((id, pqxdh::x25519_pub(&s)));
        }
        for _ in 0..n_pq {
            let id = self.alloc_id();
            let k = Self::make_pq(&self.ik_secret, id, rng);
            up.pqopks.push((id, pqxdh::mlkem_pub(&k.seed), k.sig));
            self.pqopks.insert(id, k);
        }
        up
    }

    /// Rotate the signed last-resort PQ prekey; the previous one is kept for a grace period ([PQXDH] §3.2).
    pub fn rotate_last_resort<R: RngCore + CryptoRng>(&mut self, rng: &mut R) {
        let id = self.alloc_id();
        let new = Self::make_pq(&self.ik_secret, id, rng);
        self.prev_last_resort = Some(std::mem::replace(&mut self.last_resort, new));
    }

    /// End of grace period: delete the previous last-resort key ([PQXDH] §3.2 "eventually delete").
    pub fn expire_previous(&mut self) {
        self.prev_last_resort = None;
        self.prev_spk = None;
    }

    // ---- sending ------------------------------------------------------------------------

    /// Alice: PQXDH with a fetched bundle, create the Triple Ratchet session.
    pub fn start_session<R: RngCore + CryptoRng>(&mut self, bundle: &PrekeyBundle, rng: &mut R) -> Result<pqxdh::Transcript, Error> {
        let est = pqxdh::initiate(&self.ik_secret, bundle, rng)?;
        let tr = TripleState::init_alice(&est.sk, est.ad.clone(), &bundle.spk, rng)?;
        let sess = Session { tr, initiator: true, init_ek: est.header.ek_a, pending_init: Some(est.header), pq_kind: bundle.pq_kind };
        self.sessions.insert(encode_ec(&bundle.ik).to_vec(), sess);
        Ok(est.transcript)
    }

    /// Envelope = [sender EncodeEC(IK), PQXDH prefix or null, TR header bstr, ciphertext bstr].
    pub fn encrypt<R: RngCore + CryptoRng>(&mut self, peer_ik: &[u8; 32], pt: &[u8], rng: &mut R) -> Result<(Vec<u8>, crate::triple::MsgKeys), Error> {
        let me = encode_ec(&self.ik()).to_vec();
        let s = self.sessions.get_mut(encode_ec(peer_ik).as_slice()).ok_or(Error::NoSession)?;
        let e = s.tr.encrypt(pt, rng)?;
        let init = s.pending_init.as_ref().map(|h| h.to_cbor()).unwrap_or(V::Null);
        Ok((cbor::encode(&V::A(vec![V::B(me), init, V::B(e.header), V::B(e.ct)])), e.keys))
    }

    // ---- receiving ----------------------------------------------------------------------

    /// Decrypt an envelope. Returns (sender IK, plaintext). On error nothing is changed.
    pub fn decrypt<R: RngCore + CryptoRng>(&mut self, env: &[u8], rng: &mut R) -> Result<([u8; 32], Vec<u8>, crate::triple::MsgKeys), Error> {
        let v = cbor::decode(env)?;
        let a = v.a()?;
        if a.len() != 4 {
            return Err(Error::Decode("envelope arity"));
        }
        let sender = a[0].b()?;
        if sender.len() != 33 || sender[0] != pqxdh::EC_TYPE_X25519 {
            return Err(Error::Decode("envelope sender"));
        }
        let sender_ik: [u8; 32] = sender[1..].try_into().unwrap();
        let header = a[2].b()?;
        let ct = a[3].b()?;
        let init = if a[1].is_null() { None } else { Some(InitHeader::from_cbor(&a[1])?) };
        if let Some(h) = &init {
            if h.ik_a != sender_ik {
                return Err(Error::Decode("envelope sender/IK_A mismatch"));
            }
        }
        match (self.sessions.get_mut(sender), init) {
            (Some(s), init) => {
                if let Some(h) = &init {
                    if s.initiator || h.ek_a != s.init_ek {
                        return Err(Error::SessionConflict);
                    }
                }
                let (pt, keys) = s.tr.decrypt(header, ct, rng)?;
                s.pending_init = None; // [DR] §7.1: a reply has been received
                Ok((sender_ik, pt, keys))
            }
            (None, Some(h)) => self.accept_initial(&h, header, ct, rng),
            (None, None) => Err(Error::NoSession),
        }
    }

    /// Bob: [PQXDH] §3.4 then first Triple Ratchet decrypt; consume one-time keys only on success.
    fn accept_initial<R: RngCore + CryptoRng>(&mut self, h: &InitHeader, header: &[u8], ct: &[u8], rng: &mut R) -> Result<([u8; 32], Vec<u8>, crate::triple::MsgKeys), Error> {
        let spk = [Some(&self.spk), self.prev_spk.as_ref()]
            .into_iter()
            .flatten()
            .find(|k| k.id == h.spk_id)
            .ok_or(Error::UnknownSignedPrekey)?
            .clone();
        let (pq_seed, kind) = if let Some(k) = self.pqopks.get(&h.pq_id) {
            (k.seed, PqKind::OneTime)
        } else if let Some(k) = [Some(&self.last_resort), self.prev_last_resort.as_ref()].into_iter().flatten().find(|k| k.id == h.pq_id) {
            (k.seed, PqKind::LastResort)
        } else {
            return Err(Error::UnknownOrConsumedPqPrekey);
        };
        let opk = match h.opk_id {
            Some(id) => Some(*self.opks.get(&id).ok_or(Error::UnknownOrConsumedEcPrekey)?),
            None => None,
        };
        let replay_tag = sha256(&[LR_REPLAY_TAG, &h.ek_a, &h.ct]);
        if kind == PqKind::LastResort && self.lr_replay.contains(&replay_tag) {
            return Err(Error::ReplayedLastResortHandshake);
        }
        let (sk, ad) = pqxdh::respond(&self.ik_secret, &spk.secret, opk.as_ref(), &pq_seed, h)?;
        let mut tr = TripleState::init_bob(&sk, ad, &spk.secret)?;
        let (pt, keys) = tr.decrypt(header, ct, rng)?;
        // Success: delete used one-time private keys (forward secrecy + single use).
        if kind == PqKind::OneTime {
            self.pqopks.remove(&h.pq_id);
        } else {
            if self.lr_replay.len() >= LR_REPLAY_CACHE_MAX {
                let first = *self.lr_replay.iter().next().unwrap();
                self.lr_replay.remove(&first);
            }
            self.lr_replay.insert(replay_tag);
        }
        if let Some(id) = h.opk_id {
            self.opks.remove(&id);
        }
        self.sessions.insert(
            encode_ec(&h.ik_a).to_vec(),
            Session { tr, initiator: false, pending_init: None, init_ek: h.ek_a, pq_kind: kind },
        );
        Ok((h.ik_a, pt, keys))
    }

    // ---- persistence --------------------------------------------------------------------

    pub fn to_cbor(&self) -> V {
        let ec = |k: &SignedEc| V::A(vec![V::U(k.id as u64), V::B(k.secret.to_vec()), V::B(k.sig.to_vec())]);
        let pq = |k: &SignedPq| V::A(vec![V::U(k.id as u64), V::B(k.seed.to_vec()), V::B(k.sig.to_vec())]);
        V::M(vec![
            (0, V::T("om-v001-proto-device".into())),
            (1, V::B(self.ik_secret.to_vec())),
            (2, V::U(self.next_id as u64)),
            (3, ec(&self.spk)),
            (4, self.prev_spk.as_ref().map(ec).unwrap_or(V::Null)),
            (5, pq(&self.last_resort)),
            (6, self.prev_last_resort.as_ref().map(pq).unwrap_or(V::Null)),
            (7, V::A(self.opks.iter().map(|(i, s)| V::A(vec![V::U(*i as u64), V::B(s.to_vec())])).collect())),
            (8, V::A(self.pqopks.values().map(pq).collect())),
            (9, V::A(self.lr_replay.iter().map(|t| V::B(t.to_vec())).collect())),
            (
                10,
                V::A(
                    self.sessions
                        .iter()
                        .map(|(peer, s)| {
                            V::A(vec![
                                V::B(peer.clone()),
                                s.tr.to_cbor(),
                                V::Bool(s.initiator),
                                s.pending_init.as_ref().map(|h| h.to_cbor()).unwrap_or(V::Null),
                                V::B(s.init_ek.to_vec()),
                                V::U(s.pq_kind as u64),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        if v.get(0)?.t()? != "om-v001-proto-device" {
            return Err(Error::Decode("device: type tag"));
        }
        let ec = |x: &V| -> Result<SignedEc, Error> {
            let a = x.a()?;
            if a.len() != 3 {
                return Err(Error::Decode("device: signed ec"));
            }
            Ok(SignedEc { id: a[0].u32()?, secret: a[1].b32()?, sig: a[2].b64()? })
        };
        let pq = |x: &V| -> Result<SignedPq, Error> {
            let a = x.a()?;
            if a.len() != 3 {
                return Err(Error::Decode("device: signed pq"));
            }
            Ok(SignedPq { id: a[0].u32()?, seed: a[1].b64()?, sig: a[2].b64()? })
        };
        let mut opks = BTreeMap::new();
        for e in v.get(7)?.a()? {
            let a = e.a()?;
            if a.len() != 2 || opks.insert(a[0].u32()?, a[1].b32()?).is_some() {
                return Err(Error::Decode("device: opk"));
            }
        }
        let mut pqopks = BTreeMap::new();
        for e in v.get(8)?.a()? {
            let k = pq(e)?;
            if pqopks.insert(k.id, k).is_some() {
                return Err(Error::Decode("device: duplicate pqopk"));
            }
        }
        let mut lr_replay = BTreeSet::new();
        for t in v.get(9)?.a()? {
            lr_replay.insert(t.b32()?);
        }
        let mut sessions = BTreeMap::new();
        for e in v.get(10)?.a()? {
            let a = e.a()?;
            if a.len() != 6 {
                return Err(Error::Decode("device: session"));
            }
            let s = Session {
                tr: TripleState::from_cbor(&a[1])?,
                initiator: matches!(a[2], V::Bool(true)),
                pending_init: opt(&a[3]).map(InitHeader::from_cbor).transpose()?,
                init_ek: a[4].b32()?,
                pq_kind: PqKind::from_u(a[5].u()?)?,
            };
            sessions.insert(a[0].b()?.to_vec(), s);
        }
        Ok(Device {
            ik_secret: v.get(1)?.b32()?,
            next_id: v.get(2)?.u32()?,
            spk: ec(v.get(3)?)?,
            prev_spk: opt(v.get(4)?).map(ec).transpose()?,
            last_resort: pq(v.get(5)?)?,
            prev_last_resort: opt(v.get(6)?).map(pq).transpose()?,
            opks,
            pqopks,
            lr_replay,
            sessions,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        cbor::encode(&self.to_cbor())
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        Self::from_cbor(&cbor::decode(b)?)
    }
}

/// Toy KeyMaterialProvider holding ONE disjoint pool per device (`ASYNC_KEY_MATERIAL.md` §4).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Provider {
    /// device IK -> published material
    pub devices: BTreeMap<[u8; 32], Published>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    pub spk: (u32, [u8; 32], [u8; 64]),
    pub last_resort: (u32, Vec<u8>, [u8; 64]),
    pub opks: Vec<(u32, [u8; 32])>,
    pub pqopks: Vec<(u32, Vec<u8>, [u8; 64])>,
}

fn opt(x: &V) -> Option<&V> {
    if x.is_null() { None } else { Some(x) }
}

impl Provider {
    /// publishHandshakeMaterial: signed prekey + last-resort (replacing) and one-time items (appending).
    pub fn publish(&mut self, d: &Device, up: Upload) {
        let spk = (d.spk.id, pqxdh::x25519_pub(&d.spk.secret), d.spk.sig);
        let lr = (d.last_resort.id, pqxdh::mlkem_pub(&d.last_resort.seed), d.last_resort.sig);
        let e = self.devices.entry(d.ik()).or_insert_with(|| Published { spk: spk.clone(), last_resort: lr.clone(), opks: vec![], pqopks: vec![] });
        e.spk = spk;
        e.last_resort = lr;
        e.opks.extend(up.opks);
        e.pqopks.extend(up.pqopks);
    }

    /// fetchHandshakeMaterial ([PQXDH] §3.3): pop one-time items; last-resort PQ key when depleted.
    pub fn fetch(&mut self, ik: &[u8; 32]) -> Result<PrekeyBundle, Error> {
        let p = self.devices.get_mut(ik).ok_or(Error::NoPrekeysPublished)?;
        let opk = if p.opks.is_empty() { None } else { Some(p.opks.remove(0)) };
        let (pq_id, pq, pq_sig, pq_kind) = if p.pqopks.is_empty() {
            let (i, k, s) = p.last_resort.clone();
            (i, k, s, PqKind::LastResort)
        } else {
            let (i, k, s) = p.pqopks.remove(0);
            (i, k, s, PqKind::OneTime)
        };
        Ok(PrekeyBundle { ik: *ik, spk_id: p.spk.0, spk: p.spk.1, spk_sig: p.spk.2, pq_id, pq_kind, pq, pq_sig, opk })
    }

    /// inventory(): counts only (the bounded state `ASYNC_KEY_MATERIAL.md` §6 allows).
    pub fn inventory(&self, ik: &[u8; 32]) -> (usize, usize) {
        self.devices.get(ik).map(|p| (p.opks.len(), p.pqopks.len())).unwrap_or((0, 0))
    }

    pub fn to_cbor(&self) -> V {
        V::A(
            self.devices
                .iter()
                .map(|(ik, p)| {
                    V::A(vec![
                        V::B(ik.to_vec()),
                        V::A(vec![V::U(p.spk.0 as u64), V::B(p.spk.1.to_vec()), V::B(p.spk.2.to_vec())]),
                        V::A(vec![V::U(p.last_resort.0 as u64), V::B(p.last_resort.1.clone()), V::B(p.last_resort.2.to_vec())]),
                        V::A(p.opks.iter().map(|(i, k)| V::A(vec![V::U(*i as u64), V::B(k.to_vec())])).collect()),
                        V::A(p.pqopks.iter().map(|(i, k, s)| V::A(vec![V::U(*i as u64), V::B(k.clone()), V::B(s.to_vec())])).collect()),
                    ])
                })
                .collect(),
        )
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let mut devices = BTreeMap::new();
        for e in v.a()? {
            let a = e.a()?;
            if a.len() != 5 {
                return Err(Error::Decode("provider entry"));
            }
            let s = a[1].a()?;
            let l = a[2].a()?;
            if s.len() != 3 || l.len() != 3 {
                return Err(Error::Decode("provider keys"));
            }
            let mut opks = vec![];
            for o in a[3].a()? {
                let o = o.a()?;
                if o.len() != 2 {
                    return Err(Error::Decode("provider opk"));
                }
                opks.push((o[0].u32()?, o[1].b32()?));
            }
            let mut pqopks = vec![];
            for o in a[4].a()? {
                let o = o.a()?;
                if o.len() != 3 {
                    return Err(Error::Decode("provider pqopk"));
                }
                pqopks.push((o[0].u32()?, o[1].b()?.to_vec(), o[2].b64()?));
            }
            devices.insert(
                a[0].b32()?,
                Published {
                    spk: (s[0].u32()?, s[1].b32()?, s[2].b64()?),
                    last_resort: (l[0].u32()?, l[1].b()?.to_vec(), l[2].b64()?),
                    opks,
                    pqopks,
                },
            );
        }
        Ok(Provider { devices })
    }
}
