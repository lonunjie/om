// SPDX-License-Identifier: MPL-2.0
//! Device-side prekey store + sessions, and a toy KeyMaterialProvider (`ASYNC_KEY_MATERIAL.md`
//! §2 publish/fetch/inventory). Everything persists as deterministic CBOR.
//!
//! Identity (ADR-0131 rule 1): each device has an AccountID, a DeviceID, a composite Ed25519 + ML-DSA-65
//! signing key and a PQXDH X25519 identity key, published as a DeviceCredential committed by a
//! one-device AccountControlState. Every prekey the provider serves is signed by the composite key.
//!
//! One-time semantics: the provider hands out each one-time item at most once (pop), and the device
//! deletes a one-time private key only after the initial ciphertext authenticates ([PQXDH] §3.4). A
//! handshake that used no one-time key at all goes through the exact replay ledger (ledger.rs), and
//! `receive` commits the new state (ledger included) to the vault before releasing plaintext.
//!
//! Initiator/relationship binding (ADR-0137, review F-11): every inbound 1:1 envelope requires a
//! `RelationshipContext`. The sender IK is accepted only if it is the identity key of exactly one active,
//! non-quarantined DeviceCredential committed by that relationship peer's trusted ACS head (`trust_peer`,
//! persisted like the peer heads of K7). The check runs before any secret-key operation for both new and
//! established sessions, so there is no optional/unpinned receive path.

use crate::crypto::binding::{
    self, Acs, BundleFreshness, Credential, KIND_EC_ONE_TIME, KIND_PQ_ONE_TIME, PeerDevice,
    PeerView, RelationshipContext, StaticKeys, TrustedPeer, VerifiedBundle,
};
use crate::crypto::cbor::{self, V};
use crate::crypto::ledger::LastResortLedger;
use crate::crypto::pqxdh::{self, InitHeader, PqKind, encode_ec};
use crate::crypto::rng::{CryptoRng, RngCore, arr32, arr64};
use crate::crypto::session::{MsgKeys, SessionState};
use crate::crypto::sig::DeviceSigner;
use crate::crypto::{Error, PROFILE_V1};
use std::collections::{BTreeMap, BTreeSet};

/// Fixed prototype clock values (bundle expiry is checked against `NOW`).
pub const NOW: u64 = 1_790_000_000;
pub const BUNDLE_EXPIRES: u64 = 4_102_444_800;

#[derive(Clone, PartialEq, Eq)]
pub struct SignedEc {
    pub id: u32,
    pub epoch: u64,
    pub secret: [u8; 32],
}

#[derive(Clone, PartialEq, Eq)]
pub struct SignedPq {
    pub id: u32,
    pub epoch: u64,
    pub seed: [u8; 64],
}

impl core::fmt::Debug for SignedEc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SignedEc(id={}, epoch={}, <redacted>)",
            self.id, self.epoch
        )
    }
}
impl core::fmt::Debug for SignedPq {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SignedPq(id={}, epoch={}, <redacted>)",
            self.id, self.epoch
        )
    }
}
impl Drop for SignedEc {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.secret);
    }
}
impl Drop for SignedPq {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.seed);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub dr: SessionState,
    pub initiator: bool,
    /// Alice: PQXDH prefix re-sent until Bob's first reply ([DR] §7.1).
    pub pending_init: Option<InitHeader>,
    /// EK_A of the handshake that created the session (matches repeated prefixes).
    pub init_ek: [u8; 32],
    pub pq_kind: PqKind,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Device {
    pub account_id: [u8; 32],
    pub device_id: [u8; 16],
    pub signer: DeviceSigner,
    pub ik_secret: [u8; 32],
    pub acs_generation: u64,
    pub epoch: u64,
    pub next_id: u32,
    pub spk: SignedEc,
    pub prev_spk: Option<SignedEc>,
    pub last_resort: SignedPq,
    pub prev_last_resort: Option<SignedPq>,
    pub opks: BTreeMap<u32, [u8; 32]>,
    pub pqopks: BTreeMap<u32, [u8; 64]>,
    pub ledger: LastResortLedger,
    /// Sessions keyed by EncodeEC(peer IK).
    pub sessions: BTreeMap<Vec<u8>, Session>,
    /// Relationship peers by AccountID: the trusted ACS head (hash) and view (K6/K7).
    pub peers: BTreeMap<[u8; 32], ([u8; 32], TrustedPeer)>,
    /// Durable highest-seen AsyncKeyBundle generation/core per peer AccountID+DeviceID+profile.
    pub bundle_freshness: BundleFreshness,
    /// Derived from `peers` (not persisted): every committed peer device, for IK_A lookup.
    pub peer_devices: Vec<PeerDevice>,
    /// `expires_at` (POSIX seconds) stamped into the AsyncKeyBundles this device signs. Persisted at key 18;
    /// absent in older state, which means `BUNDLE_EXPIRES`.
    pub bundle_expires_at: u64,
}

impl core::fmt::Debug for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Device(<redacted>)")
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.ik_secret.zeroize();
        for k in self.opks.values_mut() {
            k.zeroize();
        }
        for k in self.pqopks.values_mut() {
            k.zeroize();
        }
    }
}

/// What a device uploads to ONE provider pool: signed public material only.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Upload {
    pub bundle: Vec<u8>,
    pub opks: Vec<Vec<u8>>,
    pub pqopks: Vec<Vec<u8>>,
}

/// Durable device storage (DeviceVault). `commit` must be durable when it returns Ok.
pub trait Vault {
    fn commit(&mut self, device_state: &[u8]) -> Result<(), Error>;
}

/// In-memory vault with an injectable failure, for crash scenarios.
#[derive(Default)]
pub struct MemVault {
    pub stored: Option<Vec<u8>>,
    pub fail_next: bool,
}

impl Vault for MemVault {
    fn commit(&mut self, s: &[u8]) -> Result<(), Error> {
        if std::mem::take(&mut self.fail_next) {
            return Err(Error::VaultCommitFailed);
        }
        self.stored = Some(s.to_vec());
        Ok(())
    }
}

impl Device {
    pub fn new<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let account_id = arr32(rng);
        let mut device_id = [0u8; 16];
        rng.fill_bytes(&mut device_id);
        let signer = DeviceSigner::generate(rng);
        let ik_secret = arr32(rng);
        Device {
            account_id,
            device_id,
            signer,
            ik_secret,
            acs_generation: 1,
            epoch: 1,
            next_id: 3,
            spk: SignedEc {
                id: 1,
                epoch: 1,
                secret: arr32(rng),
            },
            prev_spk: None,
            last_resort: SignedPq {
                id: 2,
                epoch: 1,
                seed: arr64(rng),
            },
            prev_last_resort: None,
            opks: BTreeMap::new(),
            pqopks: BTreeMap::new(),
            ledger: LastResortLedger::default(),
            sessions: BTreeMap::new(),
            peers: BTreeMap::new(),
            bundle_freshness: BundleFreshness::default(),
            peer_devices: vec![],
            bundle_expires_at: BUNDLE_EXPIRES,
        }
    }

    /// Store a relationship peer's validated ACS head (V-002 supplies the trust decision; the prototype takes
    /// the given body as the trusted head). K7: an older generation than the stored head is refused.
    pub fn trust_peer(
        &mut self,
        acs_body: Vec<u8>,
        credential_bodies: Vec<Vec<u8>>,
    ) -> Result<(), Error> {
        let acs = Acs::from_body(&acs_body)?;
        if let Some((_, old)) = self.peers.get(&acs.account_id)
            && Acs::from_body(&old.acs_body)?.generation > acs.generation
        {
            return Err(Error::Binding(
                "peer ACS head older than the stored head (K7)",
            ));
        }
        let p = TrustedPeer {
            acs_body,
            credential_bodies,
        };
        let devs = binding::peer_devices(&p, &acs.hash())?;
        self.peers.insert(acs.account_id, (acs.hash(), p));
        self.peer_devices.retain(|d| d.1 != acs.account_id);
        self.peer_devices.extend(devs);
        self.peer_devices.sort();
        Ok(())
    }

    /// Convenience for tests: trust `other`'s one-device account as it publishes itself.
    pub fn trust_device(&mut self, other: &Device) -> Result<(), Error> {
        self.trust_peer(other.acs().body(), vec![other.credential().body()])
    }

    fn reindex_peers(&mut self) -> Result<(), Error> {
        let mut all = vec![];
        for (h, p) in self.peers.values() {
            all.extend(binding::peer_devices(p, h)?);
        }
        all.sort();
        self.peer_devices = all;
        Ok(())
    }

    pub fn ik(&self) -> [u8; 32] {
        pqxdh::x25519_pub(&self.ik_secret)
    }

    pub fn credential(&self) -> Credential {
        Credential {
            device_id: self.device_id,
            account_id: self.account_id,
            signing_pk: self.signer.public(),
            ik: self.ik(),
        }
    }

    /// One-device account: the device is the (only) control device.
    pub fn acs(&self) -> Acs {
        Acs {
            account_id: self.account_id,
            generation: self.acs_generation,
            control: BTreeSet::from([self.device_id]),
            quarantined: BTreeSet::new(),
            messaging: BTreeSet::new(),
            revoked: BTreeSet::new(),
            credentials: vec![(self.device_id, self.credential().hash())],
            previous: None,
            constitutional: (1, None),
            recovery_commitment: None,
            min_profile: PROFILE_V1,
            key_epochs: vec![],
            transparency: None,
        }
    }

    fn static_keys_for_pool(&self, pool_id: [u8; 16]) -> StaticKeys {
        StaticKeys {
            spk_id: self.spk.id,
            spk: pqxdh::x25519_pub(&self.spk.secret),
            pqspk_id: self.last_resort.id,
            pqspk: pqxdh::mlkem_pub(&self.last_resort.seed),
            epoch: self.epoch,
            pool_id,
            expires_at: self.bundle_expires_at,
        }
    }

    /// The signed-AsyncKeyBundle for the current signed-prekey generation and provider pool.
    pub fn signed_bundle_for_pool<R: RngCore + CryptoRng>(
        &self,
        pool_id: [u8; 16],
        rng: &mut R,
    ) -> Vec<u8> {
        let body = binding::bundle_body(&self.device_id, &self.static_keys_for_pool(pool_id));
        binding::sign_bundle(&self.signer, &self.device_id, body, arr32(rng))
    }

    pub fn signed_bundle<R: RngCore + CryptoRng>(&self, rng: &mut R) -> Vec<u8> {
        self.signed_bundle_for_pool([0x11; 16], rng)
    }

    fn alloc_id(&mut self) -> u32 {
        let i = self.next_id;
        self.next_id += 1;
        i
    }

    /// Generate one-time prekeys ([PQXDH] §3.2) for a named provider pool. Each item signature
    /// commits to this pool and the current signed-prekey generation (ADR-0140).
    pub fn generate_one_time_for_pool<R: RngCore + CryptoRng>(
        &mut self,
        pool_id: [u8; 16],
        n_ec: usize,
        n_pq: usize,
        rng: &mut R,
    ) -> Upload {
        let mut up = Upload {
            bundle: self.signed_bundle_for_pool(pool_id, rng),
            ..Default::default()
        };
        for _ in 0..n_ec {
            let id = self.alloc_id();
            let secret = arr32(rng);
            self.opks.insert(id, secret);
            up.opks.push(binding::sign_item(
                &self.signer,
                &self.device_id,
                self.epoch,
                &pool_id,
                id,
                KIND_EC_ONE_TIME,
                &pqxdh::x25519_pub(&secret),
                arr32(rng),
            ));
        }
        for _ in 0..n_pq {
            let id = self.alloc_id();
            let seed = arr64(rng);
            self.pqopks.insert(id, seed);
            up.pqopks.push(binding::sign_item(
                &self.signer,
                &self.device_id,
                self.epoch,
                &pool_id,
                id,
                KIND_PQ_ONE_TIME,
                &pqxdh::mlkem_pub(&seed),
                arr32(rng),
            ));
        }
        up
    }

    pub fn generate_one_time<R: RngCore + CryptoRng>(
        &mut self,
        n_ec: usize,
        n_pq: usize,
        rng: &mut R,
    ) -> Upload {
        self.generate_one_time_for_pool([0x11; 16], n_ec, n_pq, rng)
    }

    /// New signed-prekey epoch: fresh EC signed prekey and ML-KEM signed (last-resort) prekey. The
    /// previous epoch stays acceptable for a grace period ([PQXDH] §3.2) with its own ledger.
    pub fn rotate_signed_prekeys<R: RngCore + CryptoRng>(&mut self, rng: &mut R) -> Upload {
        self.epoch += 1;
        let (a, b) = (self.alloc_id(), self.alloc_id());
        let spk = SignedEc {
            id: a,
            epoch: self.epoch,
            secret: arr32(rng),
        };
        let lr = SignedPq {
            id: b,
            epoch: self.epoch,
            seed: arr64(rng),
        };
        self.prev_spk = Some(std::mem::replace(&mut self.spk, spk));
        self.prev_last_resort = Some(std::mem::replace(&mut self.last_resort, lr));
        Upload {
            bundle: self.signed_bundle(rng),
            ..Default::default()
        }
    }

    /// End of grace period: delete the previous epoch's secrets, and with them its ledger.
    pub fn expire_previous(&mut self) {
        if let Some(p) = self.prev_spk.take() {
            self.ledger.drop_epoch(p.epoch);
        }
        self.prev_last_resort = None;
    }

    pub fn peer_view<'a>(acs_body: &'a [u8], cred_body: &'a [u8]) -> PeerView<'a> {
        PeerView {
            acs_body,
            trusted_acs_hash: crate::crypto::sig::ds_hash(binding::LBL_ACS_HASH, acs_body),
            credential_body: cred_body,
            now: NOW,
        }
    }

    // ---- sending ------------------------------------------------------------------------

    /// Alice: verify the fetched material against Bob's trusted ACS head, then PQXDH + DR init.
    pub fn start_session<R: RngCore + CryptoRng>(
        &mut self,
        fetch: &[Vec<u8>],
        peer: &PeerView,
        rng: &mut R,
    ) -> Result<(VerifiedBundle, pqxdh::Transcript), Error> {
        let vb = binding::verify_fetch_with_freshness(fetch, peer, &mut self.bundle_freshness)?;
        // The trusted PeerView that authorizes initiation is also installed as relationship
        // state, so replies can only be accepted with that peer's mandatory route context.
        self.trust_peer(peer.acs_body.to_vec(), vec![peer.credential_body.to_vec()])?;
        let est = pqxdh::initiate(&self.ik_secret, &vb, rng)?;
        let dr = SessionState::init_alice(&est.sk, est.ad.clone(), &vb.spk, rng);
        let kind = if vb.pq_one_time {
            PqKind::OneTime
        } else {
            PqKind::LastResort
        };
        let sess = Session {
            dr,
            initiator: true,
            init_ek: est.header.ek_a,
            pending_init: Some(est.header),
            pq_kind: kind,
        };
        self.sessions.insert(encode_ec(&vb.ik).to_vec(), sess);
        Ok((vb, est.transcript))
    }

    /// Envelope = [sender EncodeEC(IK), PQXDH prefix or null, DR header bstr, ciphertext bstr].
    /// The AEAD envelope context is OM-CBOR [sender EncodeEC(IK), PQXDH prefix or null].
    pub fn encrypt(&mut self, peer_ik: &[u8; 32], pt: &[u8]) -> Result<(Vec<u8>, MsgKeys), Error> {
        let me = encode_ec(&self.ik()).to_vec();
        let s = self
            .sessions
            .get_mut(encode_ec(peer_ik).as_slice())
            .ok_or(Error::NoSession)?;
        let init = s
            .pending_init
            .as_ref()
            .map(|h| h.to_cbor())
            .unwrap_or(V::Null);
        let ctx = cbor::encode(&V::A(vec![V::B(me.clone()), init.clone()]));
        let e = s.dr.encrypt(pt, &ctx)?;
        Ok((
            cbor::encode(&V::A(vec![V::B(me), init, V::B(e.header), V::B(e.ct)])),
            e.keys,
        ))
    }

    // ---- receiving ----------------------------------------------------------------------

    /// Decrypt, then commit the whole new device state (ledger included) to `vault` BEFORE returning
    /// plaintext. The relationship context is mandatory; if the commit fails nothing changes and nothing is released.
    pub fn receive<R: RngCore + CryptoRng>(
        &mut self,
        relationship: &RelationshipContext,
        env: &[u8],
        vault: &mut dyn Vault,
        rng: &mut R,
    ) -> Result<([u8; 32], Vec<u8>, MsgKeys), Error> {
        let mut next = self.clone();
        let r = next.decrypt(relationship, env, rng)?;
        vault.commit(&next.to_bytes())?;
        *self = next;
        Ok(r)
    }

    /// In-memory decrypt (the caller persists). The relationship context is mandatory for every envelope,
    /// including an established session. Returns (sender IK, plaintext). On error nothing changes.
    pub fn decrypt<R: RngCore + CryptoRng>(
        &mut self,
        relationship: &RelationshipContext,
        env: &[u8],
        rng: &mut R,
    ) -> Result<([u8; 32], Vec<u8>, MsgKeys), Error> {
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
        let ctx = cbor::encode(&V::A(vec![a[0].clone(), a[1].clone()]));
        let init = if a[1].is_null() {
            None
        } else {
            Some(InitHeader::from_cbor(&a[1])?)
        };
        if let Some(h) = &init
            && h.ik_a != sender_ik
        {
            return Err(Error::Decode("envelope sender/IK_A mismatch"));
        }
        // ADR-0137: relationship identity is checked before *any* session lookup/decrypt.
        // This closes the established-session bypass as well as new-session initiation.
        binding::verify_initiator(&sender_ik, &self.peer_devices, relationship)?;
        match (self.sessions.get_mut(sender), init) {
            (Some(s), init) => {
                if let Some(h) = &init
                    && (s.initiator || h.ek_a != s.init_ek)
                {
                    return Err(Error::SessionConflict);
                }
                let (pt, keys) = s.dr.decrypt(header, ct, &ctx, rng)?;
                s.pending_init = None; // [DR] §7.1: a reply has been received
                Ok((sender_ik, pt, keys))
            }
            (None, Some(h)) => self.accept_initial(&h, header, ct, &ctx, rng),
            (None, None) => Err(Error::NoSession),
        }
    }

    /// Bob: [PQXDH] §3.4 then first DR decrypt; consume one-time keys / record the ledger only on success.
    fn accept_initial<R: RngCore + CryptoRng>(
        &mut self,
        h: &InitHeader,
        header: &[u8],
        ct: &[u8],
        ctx: &[u8],
        rng: &mut R,
    ) -> Result<([u8; 32], Vec<u8>, MsgKeys), Error> {
        let spk = [Some(&self.spk), self.prev_spk.as_ref()]
            .into_iter()
            .flatten()
            .find(|k| k.id == h.spk_id)
            .ok_or(Error::UnknownSignedPrekey)?
            .clone();
        let (pq_seed, kind) = if let Some(k) = self.pqopks.get(&h.pq_id) {
            (*k, PqKind::OneTime)
        } else if let Some(k) = [Some(&self.last_resort), self.prev_last_resort.as_ref()]
            .into_iter()
            .flatten()
            .find(|k| k.id == h.pq_id && k.epoch == spk.epoch)
        {
            (k.seed, PqKind::LastResort)
        } else {
            return Err(Error::UnknownOrConsumedPqPrekey);
        };
        let opk = match h.opk_id {
            Some(id) => Some(*self.opks.get(&id).ok_or(Error::UnknownOrConsumedEcPrekey)?),
            None => None,
        };
        // ADR-0131 rule 2: only initiations that consume NO one-time key use the ledger.
        let ledgered = kind == PqKind::LastResort && opk.is_none();
        let hid = pqxdh::handshake_id(&self.device_id, spk.epoch, h);
        if ledgered {
            self.ledger.check(spk.epoch, &hid)?;
        }
        let (sk, ad) = pqxdh::respond(&self.ik_secret, &spk.secret, opk.as_ref(), &pq_seed, h)?;
        let mut dr = SessionState::init_bob(&sk, ad, &spk.secret);
        let (pt, keys) = dr.decrypt(header, ct, ctx, rng)?;
        // Success: delete used one-time private keys (forward secrecy + single use) or record the ledger.
        if kind == PqKind::OneTime {
            self.pqopks.remove(&h.pq_id);
        }
        if let Some(id) = h.opk_id {
            self.opks.remove(&id);
        }
        if ledgered {
            self.ledger.record(spk.epoch, hid);
        }
        self.sessions.insert(
            encode_ec(&h.ik_a).to_vec(),
            Session {
                dr,
                initiator: false,
                pending_init: None,
                init_ek: h.ek_a,
                pq_kind: kind,
            },
        );
        Ok((h.ik_a, pt, keys))
    }

    // ---- persistence --------------------------------------------------------------------

    pub fn to_cbor(&self) -> V {
        let mut v = self.to_cbor_base();
        if self.bundle_expires_at != BUNDLE_EXPIRES
            && let V::M(m) = &mut v
        {
            m.push((18, V::U(self.bundle_expires_at)));
        }
        v
    }

    fn to_cbor_base(&self) -> V {
        let ec = |k: &SignedEc| {
            V::A(vec![
                V::U(k.id as u64),
                V::U(k.epoch),
                V::B(k.secret.to_vec()),
            ])
        };
        let pq = |k: &SignedPq| {
            V::A(vec![
                V::U(k.id as u64),
                V::U(k.epoch),
                V::B(k.seed.to_vec()),
            ])
        };
        V::M(vec![
            (0, V::T("om-v001-device".into())),
            (1, V::B(self.ik_secret.to_vec())),
            (2, V::U(self.next_id as u64)),
            (3, ec(&self.spk)),
            (4, self.prev_spk.as_ref().map(ec).unwrap_or(V::Null)),
            (5, pq(&self.last_resort)),
            (6, self.prev_last_resort.as_ref().map(pq).unwrap_or(V::Null)),
            (
                7,
                V::A(
                    self.opks
                        .iter()
                        .map(|(i, s)| V::A(vec![V::U(*i as u64), V::B(s.to_vec())]))
                        .collect(),
                ),
            ),
            (
                8,
                V::A(
                    self.pqopks
                        .iter()
                        .map(|(i, s)| V::A(vec![V::U(*i as u64), V::B(s.to_vec())]))
                        .collect(),
                ),
            ),
            (9, self.ledger.to_cbor()),
            (
                10,
                V::A(
                    self.sessions
                        .iter()
                        .map(|(peer, s)| {
                            V::A(vec![
                                V::B(peer.clone()),
                                s.dr.to_cbor(),
                                V::Bool(s.initiator),
                                s.pending_init
                                    .as_ref()
                                    .map(|h| h.to_cbor())
                                    .unwrap_or(V::Null),
                                V::B(s.init_ek.to_vec()),
                                V::U(s.pq_kind as u64),
                            ])
                        })
                        .collect(),
                ),
            ),
            (11, V::B(self.account_id.to_vec())),
            (12, V::B(self.device_id.to_vec())),
            (
                13,
                V::A(vec![
                    V::B(self.signer.ed_seed.to_vec()),
                    V::B(self.signer.mldsa_seed.to_vec()),
                ]),
            ),
            (14, V::U(self.acs_generation)),
            (15, V::U(self.epoch)),
            (
                16,
                V::A(
                    self.peers
                        .values()
                        .map(|(h, p)| {
                            V::A(vec![
                                V::B(h.to_vec()),
                                V::B(p.acs_body.clone()),
                                V::A(
                                    p.credential_bodies
                                        .iter()
                                        .map(|c| V::B(c.clone()))
                                        .collect(),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                17,
                V::A(
                    self.bundle_freshness
                        .floors
                        .iter()
                        .map(|((account, device, profile), (generation, core))| {
                            V::A(vec![
                                V::B(account.to_vec()),
                                V::B(device.to_vec()),
                                V::U(*profile),
                                V::U(*generation),
                                V::B(core.to_vec()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        if v.get(0)?.t()? != "om-v001-device" {
            return Err(Error::Decode("device: type tag"));
        }
        let ec = |x: &V| -> Result<SignedEc, Error> {
            let a = x.a()?;
            if a.len() != 3 {
                return Err(Error::Decode("device: signed ec"));
            }
            Ok(SignedEc {
                id: a[0].u32()?,
                epoch: a[1].u()?,
                secret: a[2].b32()?,
            })
        };
        let pq = |x: &V| -> Result<SignedPq, Error> {
            let a = x.a()?;
            if a.len() != 3 {
                return Err(Error::Decode("device: signed pq"));
            }
            Ok(SignedPq {
                id: a[0].u32()?,
                epoch: a[1].u()?,
                seed: a[2].b64()?,
            })
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
            let a = e.a()?;
            if a.len() != 2 || pqopks.insert(a[0].u32()?, a[1].b64()?).is_some() {
                return Err(Error::Decode("device: pqopk"));
            }
        }
        let mut sessions = BTreeMap::new();
        for e in v.get(10)?.a()? {
            let a = e.a()?;
            if a.len() != 6 {
                return Err(Error::Decode("device: session"));
            }
            let s = Session {
                dr: SessionState::from_cbor(&a[1])?,
                initiator: matches!(a[2], V::Bool(true)),
                pending_init: opt(&a[3]).map(InitHeader::from_cbor).transpose()?,
                init_ek: a[4].b32()?,
                pq_kind: PqKind::from_u(a[5].u()?)?,
            };
            sessions.insert(a[0].b()?.to_vec(), s);
        }
        let mut peers = BTreeMap::new();
        for e in v.opt(16).map(|x| x.a()).transpose()?.unwrap_or(&[]) {
            let a = e.a()?;
            if a.len() != 3 {
                return Err(Error::Decode("device: peer"));
            }
            let p = TrustedPeer {
                acs_body: a[1].b()?.to_vec(),
                credential_bodies: a[2]
                    .a()?
                    .iter()
                    .map(|c| c.b().map(|b| b.to_vec()))
                    .collect::<Result<_, _>>()?,
            };
            peers.insert(Acs::from_body(&p.acs_body)?.account_id, (a[0].b32()?, p));
        }
        let mut bundle_freshness = BundleFreshness::default();
        for e in v.opt(17).map(|x| x.a()).transpose()?.unwrap_or(&[]) {
            let a = e.a()?;
            if a.len() != 5 || a[1].b()?.len() != 16 {
                return Err(Error::Decode("device: bundle freshness"));
            }
            let key = (a[0].b32()?, a[1].b()?.try_into().unwrap(), a[2].u()?);
            if bundle_freshness
                .floors
                .insert(key, (a[3].u()?, a[4].b32()?))
                .is_some()
            {
                return Err(Error::Decode("device: duplicate bundle freshness"));
            }
        }
        let did = v.get(12)?.b()?;
        let sg = v.get(13)?.a()?;
        if did.len() != 16 || sg.len() != 2 {
            return Err(Error::Decode("device: identity"));
        }
        Device {
            account_id: v.get(11)?.b32()?,
            device_id: did.try_into().unwrap(),
            signer: DeviceSigner {
                ed_seed: sg[0].b32()?,
                mldsa_seed: sg[1].b32()?,
            },
            ik_secret: v.get(1)?.b32()?,
            acs_generation: v.get(14)?.u()?,
            epoch: v.get(15)?.u()?,
            next_id: v.get(2)?.u32()?,
            spk: ec(v.get(3)?)?,
            prev_spk: opt(v.get(4)?).map(ec).transpose()?,
            last_resort: pq(v.get(5)?)?,
            prev_last_resort: opt(v.get(6)?).map(pq).transpose()?,
            opks,
            pqopks,
            ledger: LastResortLedger::from_cbor(v.get(9)?)?,
            sessions,
            peers,
            bundle_freshness,
            peer_devices: vec![],
            bundle_expires_at: v
                .opt(18)
                .map(|x| x.u())
                .transpose()?
                .unwrap_or(BUNDLE_EXPIRES),
        }
        .reindexed()
    }

    fn reindexed(mut self) -> Result<Self, Error> {
        self.reindex_peers()?;
        Ok(self)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        cbor::encode(&self.to_cbor())
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, Error> {
        Self::from_cbor(&cbor::decode(b)?)
    }
}

fn opt(x: &V) -> Option<&V> {
    if x.is_null() { None } else { Some(x) }
}

/// Toy KeyMaterialProvider holding ONE disjoint pool per device (`ASYNC_KEY_MATERIAL.md` §4). It stores
/// only signed bytes it cannot alter without breaking a signature, plus the peer's ACS/credential bodies
/// that a real client would get from the account's control-state sync (V-002).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Provider {
    /// device_id -> published material
    pub devices: BTreeMap<[u8; 16], Published>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    pub bundle: Vec<u8>,
    pub opks: Vec<Vec<u8>>,
    pub pqopks: Vec<Vec<u8>>,
    pub acs_body: Vec<u8>,
    pub cred_body: Vec<u8>,
}

impl Provider {
    /// publishHandshakeMaterial: signed bundle (replacing) and one-time items (appending).
    pub fn publish(&mut self, d: &Device, up: Upload) {
        let e = self
            .devices
            .entry(d.device_id)
            .or_insert_with(|| Published {
                bundle: vec![],
                opks: vec![],
                pqopks: vec![],
                acs_body: vec![],
                cred_body: vec![],
            });
        e.bundle = up.bundle;
        e.acs_body = d.acs().body();
        e.cred_body = d.credential().body();
        e.opks.extend(up.opks);
        e.pqopks.extend(up.pqopks);
    }

    /// fetchHandshakeMaterial: [signed bundle, popped EC one-time item?, popped PQ one-time item?].
    /// With the PQ pool empty the bundle's signed ML-KEM prekey is the last-resort key ([PQXDH] §3.3).
    pub fn fetch(&mut self, device_id: &[u8; 16]) -> Result<Vec<Vec<u8>>, Error> {
        let p = self
            .devices
            .get_mut(device_id)
            .ok_or(Error::NoPrekeysPublished)?;
        let mut out = vec![p.bundle.clone()];
        if !p.opks.is_empty() {
            out.push(p.opks.remove(0));
        }
        if !p.pqopks.is_empty() {
            out.push(p.pqopks.remove(0));
        }
        Ok(out)
    }

    pub fn view(&self, device_id: &[u8; 16]) -> (Vec<u8>, Vec<u8>) {
        let p = &self.devices[device_id];
        (p.acs_body.clone(), p.cred_body.clone())
    }

    /// inventory(): counts only (the bounded state `ASYNC_KEY_MATERIAL.md` §6 allows).
    pub fn inventory(&self, device_id: &[u8; 16]) -> (usize, usize) {
        self.devices
            .get(device_id)
            .map(|p| (p.opks.len(), p.pqopks.len()))
            .unwrap_or((0, 0))
    }

    pub fn to_cbor(&self) -> V {
        let list = |v: &Vec<Vec<u8>>| V::A(v.iter().map(|b| V::B(b.clone())).collect());
        V::A(
            self.devices
                .iter()
                .map(|(d, p)| {
                    V::A(vec![
                        V::B(d.to_vec()),
                        V::B(p.bundle.clone()),
                        list(&p.opks),
                        list(&p.pqopks),
                        V::B(p.acs_body.clone()),
                        V::B(p.cred_body.clone()),
                    ])
                })
                .collect(),
        )
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let list = |x: &V| -> Result<Vec<Vec<u8>>, Error> {
            x.a()?.iter().map(|b| Ok(b.b()?.to_vec())).collect()
        };
        let mut devices = BTreeMap::new();
        for e in v.a()? {
            let a = e.a()?;
            let d = a.first().ok_or(Error::Decode("provider entry"))?.b()?;
            if a.len() != 6 || d.len() != 16 {
                return Err(Error::Decode("provider entry"));
            }
            devices.insert(
                d.try_into().unwrap(),
                Published {
                    bundle: a[1].b()?.to_vec(),
                    opks: list(&a[2])?,
                    pqopks: list(&a[3])?,
                    acs_body: a[4].b()?.to_vec(),
                    cred_body: a[5].b()?.to_vec(),
                },
            );
        }
        Ok(Provider { devices })
    }
}
