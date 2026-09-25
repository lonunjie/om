// SPDX-License-Identifier: Apache-2.0
//! PQXDH per [PQXDH] revision 3 with curve = curve25519, hash = SHA-256, pqkem = ML-KEM-768 (FIPS 203),
//! info = "OM-Crypto-v1_PQXDH_X25519_SHA-256_ML-KEM-768".
//!
//! OM deviation from [PQXDH] §3.2 (ADR-0131 rule 1, ADR-0135 §2): prekeys are not XEdDSA-signed by IK_B.
//! They are signed by the device's composite Ed25519 + ML-DSA-65 key, and IK_B itself is the
//! DeviceCredential's key_agreement_public_key committed by the AccountControlState. `initiate` accepts
//! only a `VerifiedBundle`, which exists only after binding::verify_fetch succeeded.

use crate::binding::VerifiedBundle;
use crate::cbor::{self, V};
use crate::kdf;
use crate::rng::{arr32, CryptoRng, RngCore};
use crate::sig::ds_hash;
use crate::{Error, PROFILE_V1};
use libcrux_ml_kem::mlkem768 as mlkem;
use x25519_dalek::{PublicKey, StaticSecret};

/// [PQXDH] §2.1 EncodeEC: single-byte curve id || little-endian u. 0x05 follows the Signal convention.
pub const EC_TYPE_X25519: u8 = 0x05;
pub const MLKEM768_EK_LEN: usize = 1184;
pub const MLKEM768_CT_LEN: usize = 1088;

pub fn encode_ec(pk: &[u8; 32]) -> [u8; 33] {
    let mut o = [0u8; 33];
    o[0] = EC_TYPE_X25519;
    o[1..].copy_from_slice(pk);
    o
}

pub fn x25519_pub(secret: &[u8; 32]) -> [u8; 32] {
    PublicKey::from(&StaticSecret::from(*secret)).to_bytes()
}

/// [PQXDH] §2.2 DH(PK1, PK2) = X25519.
pub fn dh(secret: &[u8; 32], public: &[u8; 32]) -> [u8; 32] {
    StaticSecret::from(*secret).diffie_hellman(&PublicKey::from(*public)).to_bytes()
}

/// ML-KEM-768 key pair from its 64-byte seed d || z (FIPS 203 ML-KEM.KeyGen_internal).
/// Devices store the seed (FIPS 203 §3.3 permits seed storage) and re-expand on use.
pub fn mlkem_keypair(seed: &[u8; 64]) -> mlkem::MlKem768KeyPair {
    mlkem::generate_key_pair(*seed)
}

pub fn mlkem_pub(seed: &[u8; 64]) -> Vec<u8> {
    mlkem_keypair(seed).public_key().as_slice().to_vec()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PqKind {
    OneTime = 1,
    LastResort = 2,
}

impl PqKind {
    pub fn from_u(n: u64) -> Result<Self, Error> {
        match n {
            1 => Ok(PqKind::OneTime),
            2 => Ok(PqKind::LastResort),
            _ => Err(Error::Decode("pq kind")),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            PqKind::OneTime => "one_time",
            PqKind::LastResort => "last_resort",
        }
    }
}

/// [PQXDH] §3.3 initial-message prefix (sent with every message until Bob replies, [DR] §7.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitHeader {
    pub ik_a: [u8; 32],
    pub ek_a: [u8; 32],
    pub ct: Vec<u8>,
    pub spk_id: u32,
    pub pq_id: u32,
    pub opk_id: Option<u32>,
}

impl InitHeader {
    pub fn to_cbor(&self) -> V {
        let mut m = vec![
            (1, V::B(self.ik_a.to_vec())),
            (2, V::B(self.ek_a.to_vec())),
            (3, V::B(self.ct.clone())),
            (4, V::U(self.spk_id as u64)),
            (5, V::U(self.pq_id as u64)),
        ];
        if let Some(o) = self.opk_id {
            m.push((6, V::U(o as u64)));
        }
        V::M(m)
    }
    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let ct = match v.opt(3) {
            Some(c) => c.b()?.to_vec(),
            None => return Err(Error::InvalidPqCiphertext),
        };
        if ct.len() != MLKEM768_CT_LEN {
            return Err(Error::InvalidPqCiphertext);
        }
        Ok(InitHeader {
            ik_a: v.get(1)?.b32()?,
            ek_a: v.get(2)?.b32()?,
            ct,
            spk_id: v.get(4)?.u32()?,
            pq_id: v.get(5)?.u32()?,
            opk_id: v.opt(6).map(|x| x.u32()).transpose()?,
        })
    }
}

/// Values exported to test vectors (never persisted).
#[derive(Clone, Debug, Default)]
pub struct Transcript {
    pub ek_secret: [u8; 32],
    pub kem_m: [u8; 32],
    pub dh: Vec<[u8; 32]>,
    pub ss: [u8; 32],
    pub sk: [u8; 32],
    pub ad: Vec<u8>,
}

pub struct Established {
    pub header: InitHeader,
    pub sk: [u8; 32],
    pub ad: Vec<u8>,
    pub transcript: Transcript,
}

/// [PQXDH] §3.3 Alice's side, from a bundle whose dual-signed binding has been verified.
pub fn initiate<R: RngCore + CryptoRng>(ik_a_secret: &[u8; 32], b: &VerifiedBundle, rng: &mut R) -> Result<Established, Error> {
    let pk = mlkem::MlKem768PublicKey::try_from(b.pq.as_slice()).map_err(|_| Error::InvalidPqKey)?;
    if !mlkem::validate_public_key(&pk) {
        return Err(Error::InvalidPqKey);
    }
    let ek_secret = arr32(rng);
    let kem_m = arr32(rng);
    // (CT, SS) = PQKEM-ENC(PQPK_B)
    let (ct, ss) = mlkem::encapsulate(&pk, kem_m);
    let ek_a = x25519_pub(&ek_secret);
    let mut dhs = vec![dh(ik_a_secret, &b.spk), dh(&ek_secret, &b.ik), dh(&ek_secret, &b.spk)];
    if let Some((_, opk)) = b.opk {
        dhs.push(dh(&ek_secret, &opk));
    }
    let mut km = Vec::with_capacity(5 * 32);
    for d in &dhs {
        km.extend_from_slice(d);
    }
    km.extend_from_slice(&ss);
    let sk = kdf::pqxdh_kdf(&km);
    zeroize::Zeroize::zeroize(&mut km);
    // AD = EncodeEC(IK_A) || EncodeEC(IK_B). ML-KEM binds H(ek) into K, so §4.12 does not require
    // appending EncodeKEM(PQPK_B).
    let mut ad = encode_ec(&x25519_pub(ik_a_secret)).to_vec();
    ad.extend_from_slice(&encode_ec(&b.ik));
    let header = InitHeader {
        ik_a: x25519_pub(ik_a_secret),
        ek_a,
        ct: ct.as_slice().to_vec(),
        spk_id: b.spk_id,
        pq_id: b.pq_id,
        opk_id: b.opk.map(|(i, _)| i),
    };
    let transcript = Transcript { ek_secret, kem_m, dh: dhs, ss, sk, ad: ad.clone() };
    Ok(Established { header, sk, ad, transcript })
}

/// ADR-0131 rule 2 / ADR-0135 §3: handshake_id = DS-HASH("OM-PQXDH-HANDSHAKE-ID-v1", OM-CBOR
/// [profile, recipient_device_id, signed_prekey_epoch, spk_id, pq_id, IK_A, EK_A, CT]).
pub fn handshake_id(recipient_device_id: &[u8; 16], epoch: u64, h: &InitHeader) -> [u8; 32] {
    let body = cbor::encode(&V::A(vec![
        V::U(PROFILE_V1),
        V::B(recipient_device_id.to_vec()),
        V::U(epoch),
        V::U(h.spk_id as u64),
        V::U(h.pq_id as u64),
        V::B(h.ik_a.to_vec()),
        V::B(h.ek_a.to_vec()),
        V::B(h.ct.clone()),
    ]));
    ds_hash(kdf::HANDSHAKE_ID, &body)
}

/// [PQXDH] §3.4 Bob's side. Key lookup/consumption is done by the caller (store.rs) so that
/// one-time keys are deleted only after the initial ciphertext decrypts.
pub fn respond(
    ik_b_secret: &[u8; 32],
    spk_secret: &[u8; 32],
    opk_secret: Option<&[u8; 32]>,
    pq_seed: &[u8; 64],
    h: &InitHeader,
) -> Result<([u8; 32], Vec<u8>), Error> {
    if h.ct.len() != MLKEM768_CT_LEN {
        return Err(Error::InvalidPqCiphertext);
    }
    let kp = mlkem_keypair(pq_seed);
    let ct = mlkem::MlKem768Ciphertext::try_from(h.ct.as_slice()).map_err(|_| Error::InvalidPqCiphertext)?;
    let ss = mlkem::decapsulate(kp.private_key(), &ct);
    let mut km = Vec::with_capacity(5 * 32);
    km.extend_from_slice(&dh(spk_secret, &h.ik_a));
    km.extend_from_slice(&dh(ik_b_secret, &h.ek_a));
    km.extend_from_slice(&dh(spk_secret, &h.ek_a));
    if let Some(o) = opk_secret {
        km.extend_from_slice(&dh(o, &h.ek_a));
    }
    km.extend_from_slice(&ss);
    let sk = kdf::pqxdh_kdf(&km);
    zeroize::Zeroize::zeroize(&mut km);
    let mut ad = encode_ec(&h.ik_a).to_vec();
    ad.extend_from_slice(&encode_ec(&x25519_pub(ik_b_secret)));
    Ok((sk, ad))
}
