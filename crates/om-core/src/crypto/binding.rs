// SPDX-License-Identifier: MPL-2.0
//! PQXDH DeviceIdentity binding (ADR-0131 rule 1, encoding ADR-0135 §2) over the FROZEN W0 objects
//! (spec/cddl/account.cddl, keys.cddl; ADR-0119):
//!
//! * `DeviceCredential` (type 7) carries AccountID, DeviceID, the composite signing key (key 5) and the
//!   PQXDH X25519 identity key (key 6, key_agreement_public_key);
//! * `AccountControlState` (type 1) commits to the credential through `device_credentials` (key 13,
//!   {device_id, OM-DEVICE-CREDENTIAL-HASH-v1 digest}) and lists the device as active/not quarantined;
//! * `signed-AsyncKeyBundle` (type 9) deliberately omits provider-visible ACS generation/hash. The
//!   recipient validates the DeviceCredential against the relationship-pinned ACS, then verifies the
//!   bundle's signed-prekey generation and public material under OM-PREKEY-BUNDLE-SIG-v1;
//! * recipients persist a highest-seen signed-prekey generation per (peer AccountID, DeviceID,
//!   CryptoProfile), shared across provider pools. Lower generations roll back and equal generations
//!   with a different signed-prekey core are equivocation;
//! * one-time items are individually signed under OM-PREKEY-SIG-v1 over an explicit context tuple
//!   binding DeviceID, profile, bundle generation, provider pool, kind, key id and public key.
//!
//! A fetch (Home API `KeyBundleFetchResponse`, `[* bstr]`) is [signed bundle, one-time items...].
//! The verifier accepts it only against an ACS head it already trusts (V-002 supplies that head).

use crate::crypto::Error;
use crate::crypto::cbor::{self, V};
use crate::crypto::sig::{self, DeviceSigner, ROLE_DEVICE, digest, ds_hash, ds_sig};
use std::collections::{BTreeMap, BTreeSet};

pub const PROFILE_V1: u64 = 1;
pub const LBL_ACS_HASH: &str = "OM-ACS-HASH-v1";
pub const LBL_CRED_HASH: &str = "OM-DEVICE-CREDENTIAL-HASH-v1";
pub const LBL_BUNDLE_SIG: &str = "OM-PREKEY-BUNDLE-SIG-v1";
pub const LBL_ITEM_SIG: &str = "OM-PREKEY-SIG-v1";
pub const LBL_GENERATION_CORE: &str = "OM-PREKEY-GENERATION-CORE-v1";

/// prekey_kind enum (om-numeric-fields.json).
pub const KIND_EC_SIGNED: u64 = 1;
pub const KIND_PQ_SIGNED: u64 = 2;
pub const KIND_EC_ONE_TIME: u64 = 3;
pub const KIND_PQ_ONE_TIME: u64 = 4;

pub const MLKEM768_EK_LEN: usize = 1184;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub device_id: [u8; 16],
    pub account_id: [u8; 32],
    pub signing_pk: Vec<u8>,
    pub ik: [u8; 32],
}

impl Credential {
    pub fn body(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::U(7)),
            (1, V::U(1)),
            (2, V::B(self.device_id.to_vec())),
            (3, V::B(self.account_id.to_vec())),
            (4, V::U(PROFILE_V1)),
            (5, V::B(self.signing_pk.clone())),
            (6, V::B(self.ik.to_vec())),
            (7, V::U(1)),
        ]))
    }
    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        let v = cbor::decode(b)?;
        if v.get(0)?.u()? != 7 || v.get(1)?.u()? != 1 {
            return Err(Error::Binding("not a DeviceCredential v1"));
        }
        if v.get(4)?.u()? != PROFILE_V1 {
            return Err(Error::Binding("credential profile"));
        }
        let did = v.get(2)?.b()?;
        let ik = v.get(6)?.b()?;
        if did.len() != 16 || ik.len() != 32 {
            return Err(Error::Binding("credential field size"));
        }
        Ok(Credential {
            device_id: did.try_into().unwrap(),
            account_id: v.get(3)?.b32()?,
            signing_pk: v.get(5)?.b()?.to_vec(),
            ik: ik.try_into().unwrap(),
        })
    }
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_CRED_HASH, &self.body())
    }
}

/// The subset of AccountControlState (type 1) the binding check reads. Sets are encoded sorted by
/// encoded bytes (OM-CBOR set rule); for fixed-size bstr / same-shape maps that is byte order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acs {
    pub account_id: [u8; 32],
    pub generation: u64,
    pub control: BTreeSet<[u8; 16]>,
    pub quarantined: BTreeSet<[u8; 16]>,
    pub messaging: BTreeSet<[u8; 16]>,
    pub revoked: BTreeSet<[u8; 16]>,
    /// device_id -> credential digest
    pub credentials: Vec<([u8; 16], [u8; 32])>,
    /// `previous_state_hash` (key 4): absent only on the genesis state.
    pub previous: Option<[u8; 32]>,
    /// `constitutional_policy` (key 9): (kind, optional fixed threshold).
    pub constitutional: (u64, Option<u64>),
    /// `recovery_policy_commitment` (key 10).
    pub recovery_commitment: Option<[u8; 32]>,
    /// `minimum_crypto_profile` (key 11): the account-wide 1:1 floor.
    pub min_profile: u64,
    /// `control_key_epochs` (key 12): (device, epoch).
    pub key_epochs: Vec<([u8; 16], u64)>,
    /// `transparency_commitment` (key 14).
    pub transparency: Option<[u8; 32]>,
}

fn id_set(s: &BTreeSet<[u8; 16]>) -> V {
    V::A(s.iter().map(|d| V::B(d.to_vec())).collect())
}

fn read_ids(v: Option<&V>) -> Result<BTreeSet<[u8; 16]>, Error> {
    let mut out = BTreeSet::new();
    if let Some(v) = v {
        for x in v.a()? {
            let b = x.b()?;
            if b.len() != 16 || !out.insert(b.try_into().unwrap()) {
                return Err(Error::Binding("acs device set"));
            }
        }
    }
    Ok(out)
}

impl Acs {
    pub fn body(&self) -> Vec<u8> {
        let mut creds: Vec<Vec<u8>> = self
            .credentials
            .iter()
            .map(|(d, h)| cbor::encode(&V::M(vec![(0, V::B(d.to_vec())), (1, digest(h))])))
            .collect();
        creds.sort();
        let mut constitutional = vec![(0, V::U(self.constitutional.0))];
        if let Some(t) = self.constitutional.1 {
            constitutional.push((1, V::U(t)));
        }
        let mut m = vec![
            (0, V::U(1)),
            (1, V::U(1)),
            (2, V::B(self.account_id.to_vec())),
            (3, V::U(self.generation)),
            (5, id_set(&self.control)),
            (9, V::M(constitutional)),
            (11, V::U(self.min_profile)),
            (
                13,
                V::A(creds.iter().map(|c| cbor::decode(c).unwrap()).collect()),
            ),
        ];
        if !self.quarantined.is_empty() {
            m.push((6, id_set(&self.quarantined)));
        }
        if !self.messaging.is_empty() {
            m.push((7, id_set(&self.messaging)));
        }
        if !self.revoked.is_empty() {
            m.push((8, id_set(&self.revoked)));
        }
        if let Some(p) = &self.previous {
            m.push((4, digest(p)));
        }
        if let Some(r) = &self.recovery_commitment {
            m.push((10, digest(r)));
        }
        if !self.key_epochs.is_empty() {
            let mut e: Vec<Vec<u8>> = self
                .key_epochs
                .iter()
                .map(|(d, n)| cbor::encode(&V::M(vec![(0, V::B(d.to_vec())), (1, V::U(*n))])))
                .collect();
            e.sort();
            m.push((
                12,
                V::A(e.iter().map(|c| cbor::decode(c).unwrap()).collect()),
            ));
        }
        if let Some(t) = &self.transparency {
            m.push((14, digest(t)));
        }
        cbor::encode(&V::M(m))
    }
    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        let v = cbor::decode(b)?;
        if v.get(0)?.u()? != 1 || v.get(1)?.u()? != 1 {
            return Err(Error::Binding("not an AccountControlState v1"));
        }
        // Key 23 holds critical extensions (fork_ack, legacy_policy): a decoder that does not implement them rejects
        // the whole state, and so does every key this parser does not know.
        for (k, _) in v.m()? {
            if !matches!(*k, 0..=14) {
                return Err(Error::Binding(
                    "acs: unsupported field or critical extension",
                ));
            }
        }
        let constitutional = {
            let c = v.get(9)?;
            (c.get(0)?.u()?, c.opt(1).map(|t| t.u()).transpose()?)
        };
        let digest_of = |k: u64| -> Result<Option<[u8; 32]>, Error> {
            v.opt(k)
                .map(|d| {
                    if d.get(0)?.u()? != 1 {
                        return Err(Error::Binding("acs digest algorithm"));
                    }
                    d.get(1)?.b32()
                })
                .transpose()
        };
        let mut key_epochs = vec![];
        if let Some(e) = v.opt(12) {
            for x in e.a()? {
                let d = x.get(0)?.b()?;
                if d.len() != 16 {
                    return Err(Error::Binding("acs key epoch device"));
                }
                key_epochs.push((d.try_into().unwrap(), x.get(1)?.u()?));
            }
        }
        let mut credentials = vec![];
        if let Some(c) = v.opt(13) {
            for e in c.a()? {
                let d = e.get(0)?.b()?;
                let h = e.get(1)?;
                if d.len() != 16 || h.get(0)?.u()? != 1 {
                    return Err(Error::Binding("acs credential ref"));
                }
                credentials.push((d.try_into().unwrap(), h.get(1)?.b32()?));
            }
        }
        Ok(Acs {
            account_id: v.get(2)?.b32()?,
            generation: v.get(3)?.u()?,
            control: read_ids(Some(v.get(5)?))?,
            quarantined: read_ids(v.opt(6))?,
            messaging: read_ids(v.opt(7))?,
            revoked: read_ids(v.opt(8))?,
            credentials,
            previous: digest_of(4)?,
            constitutional,
            recovery_commitment: digest_of(10)?,
            min_profile: v.get(11)?.u()?,
            key_epochs,
            transparency: digest_of(14)?,
        })
    }
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_ACS_HASH, &self.body())
    }
    /// "active and non-quarantined at the referenced ACS head" (ADR-0131 rule 1).
    pub fn device_active(&self, d: &[u8; 16]) -> bool {
        (self.control.contains(d) || self.messaging.contains(d))
            && !self.quarantined.contains(d)
            && !self.revoked.contains(d)
    }
}

/// Public halves as a device publishes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticKeys {
    pub spk_id: u32,
    pub spk: [u8; 32],
    pub pqspk_id: u32,
    pub pqspk: Vec<u8>,
    /// Signed-prekey epoch (AsyncKeyBundle key 6 `generation`).
    pub epoch: u64,
    pub pool_id: [u8; 16],
    pub expires_at: u64,
}

fn item(id: u32, kind: u64, pk: &[u8]) -> V {
    V::M(vec![
        (0, V::U(id as u64)),
        (1, V::U(kind)),
        (2, V::B(pk.to_vec())),
    ])
}

pub fn bundle_body(device_id: &[u8; 16], k: &StaticKeys) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(9)),
        (1, V::U(1)),
        (2, V::B(device_id.to_vec())),
        (5, V::U(PROFILE_V1)),
        (6, V::U(k.epoch)),
        (7, item(k.spk_id, KIND_EC_SIGNED, &k.spk)),
        (8, item(k.pqspk_id, KIND_PQ_SIGNED, &k.pqspk)),
        (12, V::B(k.pool_id.to_vec())),
        (13, V::U(k.expires_at)),
    ]))
}

pub fn sign_bundle(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    body: Vec<u8>,
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_BUNDLE_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &body),
        rnd,
    );
    sig::envelope(
        body,
        vec![sig::signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

fn one_time_context(
    device_id: &[u8; 16],
    profile: u64,
    generation: u64,
    pool_id: &[u8; 16],
    id: u32,
    kind: u64,
    pk: &[u8],
) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::U(1),
        V::B(device_id.to_vec()),
        V::U(profile),
        V::U(generation),
        V::B(pool_id.to_vec()),
        V::U(kind),
        V::U(id as u64),
        V::B(pk.to_vec()),
    ]))
}

/// A one-time prekey item signed in its bundle/provider context (ADR-0140).
pub fn sign_item(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    generation: u64,
    pool_id: &[u8; 16],
    id: u32,
    kind: u64,
    pk: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    let context = one_time_context(device_id, PROFILE_V1, generation, pool_id, id, kind, pk);
    let s = signer.sign_with(
        &ds_sig(LBL_ITEM_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &context),
        rnd,
    );
    cbor::encode(&V::M(vec![
        (0, V::U(id as u64)),
        (1, V::U(kind)),
        (2, V::B(pk.to_vec())),
        (3, V::B(s)),
    ]))
}

fn generation_core_hash(b: &V) -> Result<[u8; 32], Error> {
    let core = V::A(vec![
        V::B(b.get(2)?.b()?.to_vec()),
        V::U(b.get(5)?.u()?),
        V::U(b.get(6)?.u()?),
        b.get(7)?.clone(),
        b.get(8)?.clone(),
        b.opt(11).cloned().unwrap_or(V::Null),
    ]);
    Ok(ds_hash(LBL_GENERATION_CORE, &cbor::encode(&core)))
}

/// Durable anti-rollback state. The key deliberately excludes provider_pool_id: all pools for the same
/// peer device/profile share one signed-prekey generation floor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleFreshness {
    pub floors: BTreeMap<([u8; 32], [u8; 16], u64), (u64, [u8; 32])>,
}

impl BundleFreshness {
    pub fn observe(
        &mut self,
        account: [u8; 32],
        device: [u8; 16],
        profile: u64,
        generation: u64,
        core_hash: [u8; 32],
    ) -> Result<(), Error> {
        let key = (account, device, profile);
        match self.floors.get(&key) {
            Some((highest, _)) if generation < *highest => {
                return Err(Error::PrekeyGenerationRollback);
            }
            Some((highest, old_hash)) if generation == *highest && core_hash != *old_hash => {
                return Err(Error::PrekeyGenerationEquivocation);
            }
            _ => {}
        }
        match self.floors.get(&key) {
            Some((highest, _)) if generation <= *highest => {}
            _ => {
                self.floors.insert(key, (generation, core_hash));
            }
        }
        Ok(())
    }
}

/// Output of a successful binding check: the only way to obtain PQXDH inputs for a peer device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedBundle {
    pub account_id: [u8; 32],
    pub device_id: [u8; 16],
    pub ik: [u8; 32],
    pub epoch: u64,
    pub spk_id: u32,
    pub spk: [u8; 32],
    pub pq_id: u32,
    pub pq: Vec<u8>,
    pub pq_one_time: bool,
    pub opk: Option<(u32, [u8; 32])>,
}

/// What the initiator already trusts about the peer: its ACS head (from V-002) and credential bodies.
pub struct PeerView<'a> {
    pub acs_body: &'a [u8],
    pub trusted_acs_hash: [u8; 32],
    pub credential_body: &'a [u8],
    pub now: u64,
}

pub fn verify_fetch(fetch: &[Vec<u8>], peer: &PeerView) -> Result<VerifiedBundle, Error> {
    let mut transient = BundleFreshness::default();
    verify_fetch_with_freshness(fetch, peer, &mut transient)
}

pub fn verify_fetch_with_freshness(
    fetch: &[Vec<u8>],
    peer: &PeerView,
    freshness: &mut BundleFreshness,
) -> Result<VerifiedBundle, Error> {
    let acs = Acs::from_body(peer.acs_body)?;
    if acs.hash() != peer.trusted_acs_hash {
        return Err(Error::Binding("ACS body is not the trusted head"));
    }
    let cred = Credential::from_body(peer.credential_body)?;
    if cred.account_id != acs.account_id {
        return Err(Error::Binding("credential AccountID differs from ACS"));
    }
    if !acs
        .credentials
        .iter()
        .any(|(d, h)| *d == cred.device_id && *h == cred.hash())
    {
        return Err(Error::Binding("credential digest not committed by ACS"));
    }
    if !acs.device_active(&cred.device_id) {
        return Err(Error::Binding(
            "device not active or quarantined at ACS head",
        ));
    }
    let (first, items) = fetch.split_first().ok_or(Error::NoPrekeysPublished)?;
    let body = sig::open_envelope(
        first,
        LBL_BUNDLE_SIG,
        PROFILE_V1,
        ROLE_DEVICE,
        &cred.device_id,
        &cred.signing_pk,
    )?;
    let b = cbor::decode(&body)?;
    if b.get(0)?.u()? != 9 || b.get(1)?.u()? != 1 {
        return Err(Error::Binding("not an AsyncKeyBundle v1"));
    }
    if b.get(2)?.b()? != cred.device_id {
        return Err(Error::Binding(
            "bundle subject_device_id differs from credential",
        ));
    }
    if b.opt(3).is_some() || b.opt(4).is_some() {
        return Err(Error::Binding(
            "retired provider-visible ACS fields are forbidden",
        ));
    }
    if b.get(5)?.u()? != PROFILE_V1 {
        return Err(Error::Binding("bundle profile"));
    }
    if b.get(13)?.u()? <= peer.now {
        return Err(Error::Binding("bundle expired"));
    }
    let pool = b.get(12)?.b()?;
    if pool.len() != 16 {
        return Err(Error::Binding("provider_pool_id size"));
    }
    let pool_id: [u8; 16] = pool.try_into().unwrap();
    let generation = b.get(6)?.u()?;
    let core_hash = generation_core_hash(&b)?;
    let spk = b.get(7)?;
    let pqspk = b.opt(8).ok_or(Error::MissingPqPrekey)?;
    if spk.get(1)?.u()? != KIND_EC_SIGNED || pqspk.get(1)?.u()? != KIND_PQ_SIGNED {
        return Err(Error::Binding("signed prekey kinds"));
    }
    let pq = pqspk.get(2)?.b()?.to_vec();
    if pq.len() != MLKEM768_EK_LEN {
        return Err(Error::InvalidPqKey);
    }
    let mut out = VerifiedBundle {
        account_id: cred.account_id,
        device_id: cred.device_id,
        ik: cred.ik,
        epoch: generation,
        spk_id: spk.get(0)?.u32()?,
        spk: spk.get(2)?.b32()?,
        pq_id: pqspk.get(0)?.u32()?,
        pq,
        pq_one_time: false,
        opk: None,
    };
    for raw in items {
        let v = cbor::decode(raw)?;
        let (id, kind, pk) = (v.get(0)?.u32()?, v.get(1)?.u()?, v.get(2)?.b()?.to_vec());
        if kind != KIND_EC_ONE_TIME && kind != KIND_PQ_ONE_TIME {
            return Err(Error::Binding("unexpected one-time item"));
        }
        let context = one_time_context(
            &cred.device_id,
            PROFILE_V1,
            generation,
            &pool_id,
            id,
            kind,
            &pk,
        );
        sig::verify(
            &cred.signing_pk,
            &ds_sig(
                LBL_ITEM_SIG,
                PROFILE_V1,
                ROLE_DEVICE,
                &cred.device_id,
                &context,
            ),
            v.get(3)?.b()?,
        )?;
        match kind {
            KIND_EC_ONE_TIME if out.opk.is_none() && pk.len() == 32 => {
                out.opk = Some((id, pk.try_into().unwrap()))
            }
            KIND_PQ_ONE_TIME if !out.pq_one_time && pk.len() == MLKEM768_EK_LEN => {
                out.pq_id = id;
                out.pq = pk;
                out.pq_one_time = true;
            }
            _ => return Err(Error::Binding("unexpected one-time item")),
        }
    }
    // Raise the durable floor only after the entire signed bundle and all returned one-time items
    // have verified. This happens before PQXDH private-key work in Device::start_session.
    freshness.observe(
        cred.account_id,
        cred.device_id,
        PROFILE_V1,
        generation,
        core_hash,
    )?;
    Ok(out)
}

/// Responder side of the identity binding (ADR-0136 rule 4, review F-4/F-9). A relationship peer as the
/// session manager holds it (K6/K7): the peer account's trusted ACS head and the credential bodies it commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedPeer {
    pub acs_body: Vec<u8>,
    pub credential_bodies: Vec<Vec<u8>>,
}

/// One committed device of a trusted peer: (identity key, AccountID, DeviceID, active and not quarantined).
pub type PeerDevice = ([u8; 32], [u8; 32], [u8; 16], bool);

/// Mandatory relationship-routing context for every inbound 1:1 decrypt (ADR-0137).
/// There is deliberately no `Option`/unbound form: a caller must name the peer
/// AccountID whose relationship route delivered the envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationshipContext {
    pub peer_account_id: [u8; 32],
}

impl RelationshipContext {
    pub fn new(peer_account_id: [u8; 32]) -> Self {
        Self { peer_account_id }
    }
}

/// Validate a peer view against the ACS head the caller already trusts, and list its committed devices.
/// A credential body the head does not commit (stale, rotated away, or from another account) is dropped,
/// so it can never authenticate an initiator.
pub fn peer_devices(
    p: &TrustedPeer,
    trusted_acs_hash: &[u8; 32],
) -> Result<Vec<PeerDevice>, Error> {
    let acs = Acs::from_body(&p.acs_body)?;
    if acs.hash() != *trusted_acs_hash {
        return Err(Error::Binding("peer ACS body is not the trusted head"));
    }
    let mut out = vec![];
    for b in &p.credential_bodies {
        let c = Credential::from_body(b)?;
        if c.account_id == acs.account_id
            && acs
                .credentials
                .iter()
                .any(|(d, h)| *d == c.device_id && *h == c.hash())
        {
            out.push((
                c.ik,
                c.account_id,
                c.device_id,
                acs.device_active(&c.device_id),
            ));
        }
    }
    Ok(out)
}

/// Resolve an inbound initiator IK_A to exactly one committed DeviceCredential of the
/// relationship peer named by `relationship`. Fails closed on no match, on more than
/// one match, and on a device that is not active or is quarantined at the trusted head.
/// Callers run it for *every* inbound envelope, including established sessions, before
/// any secret-key operation, one-time-key consumption or session change.
pub fn verify_initiator(
    ik_a: &[u8; 32],
    matches: &[PeerDevice],
    relationship: &RelationshipContext,
) -> Result<([u8; 32], [u8; 16]), Error> {
    let m: Vec<&PeerDevice> = matches
        .iter()
        .filter(|(ik, a, _, _)| ik == ik_a && *a == relationship.peer_account_id)
        .collect();
    match m.as_slice() {
        [] => Err(Error::Binding(
            "initiator IK_A matches no committed credential of the relationship peer",
        )),
        [(_, a, d, true)] => Ok((*a, *d)),
        [_] => Err(Error::Binding(
            "initiator device not active or quarantined at the trusted ACS head",
        )),
        _ => Err(Error::Binding(
            "initiator IK_A matches more than one credential",
        )),
    }
}
