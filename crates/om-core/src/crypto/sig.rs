// SPDX-License-Identifier: MPL-2.0
//! OM-Crypto-v1 DeviceIdentity composite signature: Ed25519 (RFC 8032) AND ML-DSA-65 (FIPS 204)
//! over the same DS-SIG bytes (ADR-0131 rule 1; encoding pinned by ADR-0135 §1).
//!
//! * public key (DeviceCredential key 5) = Ed25519 pk (32) || ML-DSA-65 vk (1952) = 1984 bytes;
//! * signature (Signature key 3)         = Ed25519 sig (64) || ML-DSA-65 sig (3309) = 3373 bytes;
//! * ML-DSA-65 is pure ML-DSA.Sign with an empty context string; Ed25519 is verified strictly.
//!
//! A verifier accepts only when BOTH components verify; any other length is rejected, so neither
//! half can be stripped or substituted.

use crate::crypto::Error;
use crate::crypto::cbor::{self, V};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use libcrux_ml_dsa::ml_dsa_65 as mldsa;

pub const ED_PK: usize = 32;
pub const ED_SIG: usize = 64;
pub const MLDSA_VK: usize = 1952;
pub const MLDSA_SIG: usize = 3309;
pub const PK_LEN: usize = ED_PK + MLDSA_VK;
pub const SIG_LEN: usize = ED_SIG + MLDSA_SIG;

/// Signature roles (om-crypto-profiles.json `signature_roles`).
pub const ROLE_CONTROLLER: u64 = 1;
pub const ROLE_DEVICE: u64 = 3;
pub const ROLE_INTRO_IDENTITY: u64 = 4;

/// DS-SIG(label, crypto_profile, role, signer, body) (om-domain-separation.json).
pub fn ds_sig(label: &str, profile: u64, role: u64, signer: &[u8], body: &[u8]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(label.into()),
        V::U(profile),
        V::U(role),
        V::B(signer.to_vec()),
        V::B(body.to_vec()),
    ]))
}

/// DS-HASH(label, body) with hash_alg 1 (SHA-256).
pub fn ds_hash(label: &str, body: &[u8]) -> [u8; 32] {
    crate::crypto::sha256(&[&cbor::encode(&V::A(vec![
        V::T(label.into()),
        V::B(body.to_vec()),
    ]))])
}

/// Digest struct {0: hash_alg, 1: bytes}.
pub fn digest(h: &[u8; 32]) -> V {
    V::M(vec![(0, V::U(1)), (1, V::B(h.to_vec()))])
}

/// A DeviceIdentity signing key: two 32-byte seeds (Ed25519 secret key; ML-DSA-65 KeyGen seed xi).
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSigner {
    pub ed_seed: [u8; 32],
    pub mldsa_seed: [u8; 32],
}

impl core::fmt::Debug for DeviceSigner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("DeviceSigner(<redacted>)")
    }
}

impl Drop for DeviceSigner {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.ed_seed);
        zeroize::Zeroize::zeroize(&mut self.mldsa_seed);
    }
}

impl DeviceSigner {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        DeviceSigner {
            ed_seed: arr32(rng),
            mldsa_seed: arr32(rng),
        }
    }

    pub fn public(&self) -> Vec<u8> {
        let mut pk = SigningKey::from_bytes(&self.ed_seed)
            .verifying_key()
            .to_bytes()
            .to_vec();
        pk.extend_from_slice(
            mldsa::generate_key_pair(self.mldsa_seed)
                .verification_key
                .as_ref(),
        );
        pk
    }

    /// Sign `msg` (already DS-SIG encoded). `rnd` is the FIPS 204 hedging input.
    pub fn sign_with(&self, msg: &[u8], rnd: [u8; 32]) -> Vec<u8> {
        let mut s = SigningKey::from_bytes(&self.ed_seed)
            .sign(msg)
            .to_bytes()
            .to_vec();
        let kp = mldsa::generate_key_pair(self.mldsa_seed);
        let m = mldsa::sign(&kp.signing_key, msg, b"", rnd).expect("ML-DSA-65 sign");
        s.extend_from_slice(m.as_ref());
        s
    }

    pub fn sign<R: RngCore + CryptoRng>(&self, msg: &[u8], rng: &mut R) -> Vec<u8> {
        self.sign_with(msg, arr32(rng))
    }
}

/// Which half of a composite signature failed (for named negative vectors).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigFailure {
    Length,
    Ed25519,
    MlDsa65,
}

pub fn verify_detail(pk: &[u8], msg: &[u8], sig: &[u8]) -> Result<(), SigFailure> {
    if pk.len() != PK_LEN || sig.len() != SIG_LEN {
        return Err(SigFailure::Length);
    }
    let ed_pk: [u8; ED_PK] = pk[..ED_PK].try_into().unwrap();
    let ed_sig: [u8; ED_SIG] = sig[..ED_SIG].try_into().unwrap();
    let vk = VerifyingKey::from_bytes(&ed_pk).map_err(|_| SigFailure::Ed25519)?;
    vk.verify_strict(msg, &ed25519_dalek::Signature::from_bytes(&ed_sig))
        .map_err(|_| SigFailure::Ed25519)?;
    let mvk = mldsa::MLDSA65VerificationKey::new(pk[ED_PK..].try_into().unwrap());
    let msig = mldsa::MLDSA65Signature::new(sig[ED_SIG..].try_into().unwrap());
    mldsa::verify(&mvk, msg, b"", &msig).map_err(|_| SigFailure::MlDsa65)
}

pub fn verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> Result<(), Error> {
    verify_detail(pk, msg, sig).map_err(|_| Error::BadSignature)
}

/// Signature struct {0: signer, 1: profile, 2: role, 3: composite signature}.
pub fn signature_entry(signer: &[u8], profile: u64, role: u64, sig: Vec<u8>) -> V {
    V::M(vec![
        (0, V::B(signer.to_vec())),
        (1, V::U(profile)),
        (2, V::U(role)),
        (3, V::B(sig)),
    ])
}

/// SignedEnvelope {1: body bstr, 2: [+ Signature]}.
pub fn envelope(body: Vec<u8>, sigs: Vec<V>) -> Vec<u8> {
    cbor::encode(&V::M(vec![(1, V::B(body)), (2, V::A(sigs))]))
}

/// Open a SignedEnvelope that must carry EXACTLY one signature by (signer, profile, role), verified
/// with the composite key `pk` under `label`. Returns the body bytes.
pub fn open_envelope(
    bytes: &[u8],
    label: &str,
    profile: u64,
    role: u64,
    signer: &[u8],
    pk: &[u8],
) -> Result<Vec<u8>, Error> {
    let v = cbor::decode(bytes)?;
    let body = v.get(1)?.b()?.to_vec();
    let sigs = v.get(2)?.a()?;
    if sigs.len() != 1 {
        return Err(Error::Binding(
            "exactly one device signature entry required",
        ));
    }
    let s = &sigs[0];
    if s.get(0)?.b()? != signer || s.get(1)?.u()? != profile || s.get(2)?.u()? != role {
        return Err(Error::Binding("signature signer/profile/role mismatch"));
    }
    verify(
        pk,
        &ds_sig(label, profile, role, signer, &body),
        s.get(3)?.b()?,
    )?;
    Ok(body)
}
