// SPDX-License-Identifier: Apache-2.0
//! PQXDH DeviceIdentity binding (ADR-0131 rule 1, encoding ADR-0135 §2) over the FROZEN W0 objects
//! (spec/cddl/account.cddl, keys.cddl; ADR-0119):
//!
//! * `DeviceCredential` (type 7) carries AccountID, DeviceID, the composite signing key (key 5) and the
//!   PQXDH X25519 identity key (key 6, key_agreement_public_key);
//! * `AccountControlState` (type 1) commits to the credential through `device_credentials` (key 13,
//!   {device_id, OM-DEVICE-CREDENTIAL-HASH-v1 digest}) and lists the device as active/not quarantined;
//! * `signed-AsyncKeyBundle` (type 9) commits to acs_generation/acs_state_hash, the signed EC prekey
//!   (key 7), the ML-KEM-768 signed prekey (key 8) and the signed-prekey epoch (key 6, generation), and
//!   is signed by the device's composite key under OM-PREKEY-BUNDLE-SIG-v1;
//! * one-time items travel as individually signed `prekey-item`s (key 3 = composite signature under
//!   OM-PREKEY-SIG-v1 over the item without key 3), so a provider can hand each out once.
//!
//! A fetch (Home API `KeyBundleFetchResponse`, `[* bstr]`) is [signed bundle, one-time items...].
//! The verifier accepts it only against an ACS head it already trusts (V-002 supplies that head).

use crate::cbor::{self, V};
use crate::sig::{self, ds_hash, ds_sig, digest, DeviceSigner, ROLE_DEVICE};
use crate::Error;
use std::collections::BTreeSet;

pub const PROFILE_V1: u64 = 1;
pub const LBL_ACS_HASH: &str = "OM-ACS-HASH-v1";
pub const LBL_CRED_HASH: &str = "OM-DEVICE-CREDENTIAL-HASH-v1";
pub const LBL_BUNDLE_SIG: &str = "OM-PREKEY-BUNDLE-SIG-v1";
pub const LBL_ITEM_SIG: &str = "OM-PREKEY-SIG-v1";

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
            (0, V::U(7)), (1, V::U(1)),
            (2, V::B(self.device_id.to_vec())), (3, V::B(self.account_id.to_vec())), (4, V::U(PROFILE_V1)),
            (5, V::B(self.signing_pk.clone())), (6, V::B(self.ik.to_vec())), (7, V::U(1)),
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
        let mut m = vec![
            (0, V::U(1)), (1, V::U(1)),
            (2, V::B(self.account_id.to_vec())), (3, V::U(self.generation)),
            (5, id_set(&self.control)),
            (9, V::M(vec![(0, V::U(1))])),
            (11, V::U(PROFILE_V1)),
            (13, V::A(creds.iter().map(|c| cbor::decode(c).unwrap()).collect())),
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
        cbor::encode(&V::M(m))
    }
    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        let v = cbor::decode(b)?;
        if v.get(0)?.u()? != 1 || v.get(1)?.u()? != 1 {
            return Err(Error::Binding("not an AccountControlState v1"));
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
        })
    }
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_ACS_HASH, &self.body())
    }
    /// "active and non-quarantined at the referenced ACS head" (ADR-0131 rule 1).
    pub fn device_active(&self, d: &[u8; 16]) -> bool {
        (self.control.contains(d) || self.messaging.contains(d)) && !self.quarantined.contains(d) && !self.revoked.contains(d)
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
    V::M(vec![(0, V::U(id as u64)), (1, V::U(kind)), (2, V::B(pk.to_vec()))])
}

pub fn bundle_body(device_id: &[u8; 16], acs: &Acs, k: &StaticKeys) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(9)), (1, V::U(1)),
        (2, V::B(device_id.to_vec())), (3, V::U(acs.generation)), (4, digest(&acs.hash())),
        (5, V::U(PROFILE_V1)), (6, V::U(k.epoch)),
        (7, item(k.spk_id, KIND_EC_SIGNED, &k.spk)),
        (8, item(k.pqspk_id, KIND_PQ_SIGNED, &k.pqspk)),
        (12, V::B(k.pool_id.to_vec())), (13, V::U(k.expires_at)),
    ]))
}

pub fn sign_bundle(signer: &DeviceSigner, device_id: &[u8; 16], body: Vec<u8>, rnd: [u8; 32]) -> Vec<u8> {
    let s = signer.sign_with(&ds_sig(LBL_BUNDLE_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &body), rnd);
    sig::envelope(body, vec![sig::signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)])
}

/// A one-time prekey item signed on its own: {0: id, 1: kind, 2: pk, 3: composite signature}.
pub fn sign_item(signer: &DeviceSigner, device_id: &[u8; 16], id: u32, kind: u64, pk: &[u8], rnd: [u8; 32]) -> Vec<u8> {
    let unsigned = cbor::encode(&item(id, kind, pk));
    let s = signer.sign_with(&ds_sig(LBL_ITEM_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &unsigned), rnd);
    cbor::encode(&V::M(vec![(0, V::U(id as u64)), (1, V::U(kind)), (2, V::B(pk.to_vec())), (3, V::B(s))]))
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
    let acs = Acs::from_body(peer.acs_body)?;
    if acs.hash() != peer.trusted_acs_hash {
        return Err(Error::Binding("ACS body is not the trusted head"));
    }
    let cred = Credential::from_body(peer.credential_body)?;
    if cred.account_id != acs.account_id {
        return Err(Error::Binding("credential AccountID differs from ACS"));
    }
    if !acs.credentials.iter().any(|(d, h)| *d == cred.device_id && *h == cred.hash()) {
        return Err(Error::Binding("credential digest not committed by ACS"));
    }
    if !acs.device_active(&cred.device_id) {
        return Err(Error::Binding("device not active or quarantined at ACS head"));
    }
    let (first, items) = fetch.split_first().ok_or(Error::NoPrekeysPublished)?;
    let body = sig::open_envelope(first, LBL_BUNDLE_SIG, PROFILE_V1, ROLE_DEVICE, &cred.device_id, &cred.signing_pk)?;
    let b = cbor::decode(&body)?;
    if b.get(0)?.u()? != 9 || b.get(1)?.u()? != 1 {
        return Err(Error::Binding("not an AsyncKeyBundle v1"));
    }
    if b.get(2)?.b()? != cred.device_id {
        return Err(Error::Binding("bundle subject_device_id differs from credential"));
    }
    if b.get(3)?.u()? != acs.generation || b.get(4)?.get(1)?.b32()? != acs.hash() {
        return Err(Error::Binding("bundle not bound to the trusted ACS head"));
    }
    if b.get(5)?.u()? != PROFILE_V1 {
        return Err(Error::Binding("bundle profile"));
    }
    if b.get(13)?.u()? <= peer.now {
        return Err(Error::Binding("bundle expired"));
    }
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
        epoch: b.get(6)?.u()?,
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
        let unsigned = cbor::encode(&item(id, kind, &pk));
        sig::verify(&cred.signing_pk, &ds_sig(LBL_ITEM_SIG, PROFILE_V1, ROLE_DEVICE, &cred.device_id, &unsigned), v.get(3)?.b()?)?;
        match kind {
            KIND_EC_ONE_TIME if out.opk.is_none() && pk.len() == 32 => out.opk = Some((id, pk.try_into().unwrap())),
            KIND_PQ_ONE_TIME if !out.pq_one_time && pk.len() == MLKEM768_EK_LEN => {
                out.pq_id = id;
                out.pq = pk;
                out.pq_one_time = true;
            }
            _ => return Err(Error::Binding("unexpected one-time item")),
        }
    }
    Ok(out)
}
