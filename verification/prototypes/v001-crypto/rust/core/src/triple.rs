// SPDX-License-Identifier: Apache-2.0
//! Triple Ratchet per [DR] §6: EC Double Ratchet (ratchet.rs) + Sparse Post-Quantum Ratchet
//! (Signal's `spqr` crate, ML-KEM Braid SCKA) run in parallel; message key =
//! KDF_HYBRID(ec_mk, pq_mk); AEAD = AES-256-GCM-SIV (RFC 8452).
//!
//! Downgrade rule: spqr is initialised with `min_version = V1`, and a missing SPQR message key
//! is a hard error. A peer (or attacker) sending SPQR "V0" (PQ ratchet disabled) is rejected.

use crate::cbor::{self, V};
use crate::kdf::{kdf_hybrid, tr_split};
use crate::ratchet::{EcState, MAX_SKIP};
use crate::rng::{CryptoRng, RngCore};
use crate::{Error, PROFILE_PROTO};
use aes_gcm_siv::aead::{Aead, KeyInit, Payload};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TripleState {
    pub ec: EcState,
    /// Opaque spqr::SerializedState (spqr's own protobuf encoding, embedded as a bstr).
    pub pq: Vec<u8>,
    /// [DR] §7.1: PQXDH AD becomes the ratchet AD.
    pub ad: Vec<u8>,
}

/// Keys observed while processing one message (exported to vectors/benchmarks only).
#[derive(Clone, Debug)]
pub struct MsgKeys {
    pub ec_mk: [u8; 32],
    pub pq_mk: Vec<u8>,
    pub key: [u8; 32],
    pub nonce: [u8; 12],
}

pub struct Encrypted {
    pub header: Vec<u8>,
    pub ct: Vec<u8>,
    pub keys: MsgKeys,
}

fn spqr_params(direction: spqr::Direction, auth_key: &[u8]) -> spqr::Params<'_> {
    spqr::Params {
        direction,
        version: spqr::Version::V1,
        // No negotiation down to V0 (= no PQ ratchet): never silently downgrade (AGENTS.md rule 6).
        min_version: spqr::Version::V1,
        auth_key,
        chain_params: spqr::ChainParams { max_jump: MAX_SKIP as u32, max_ooo_keys: 2000 },
    }
}

/// Parsed Triple Ratchet header: [profile, dh, pn, n, scka_msg] ([DR] §6.5 composite header).
pub struct Header {
    pub dh: [u8; 32],
    pub pn: u64,
    pub n: u64,
    pub scka: Vec<u8>,
}

pub fn encode_header(dh: &[u8; 32], pn: u64, n: u64, scka: &[u8]) -> Vec<u8> {
    cbor::encode(&V::A(vec![V::U(PROFILE_PROTO), V::B(dh.to_vec()), V::U(pn), V::U(n), V::B(scka.to_vec())]))
}

pub fn parse_header(b: &[u8]) -> Result<Header, Error> {
    let v = cbor::decode(b)?;
    let a = v.a()?;
    if a.len() != 5 {
        return Err(Error::Decode("header arity"));
    }
    if a[0].u()? != PROFILE_PROTO {
        return Err(Error::Decode("header profile"));
    }
    Ok(Header { dh: a[1].b32()?, pn: a[2].u()?, n: a[3].u()?, scka: a[4].b()?.to_vec() })
}

/// spqr wire preamble (`version || varint epoch || varint index`, see spqr src/lib.rs
/// `msg_preamble`). Parsed ONLY for observability (epoch counting); not relied on for security.
pub fn spqr_epoch(msg: &[u8]) -> Option<u64> {
    if msg.len() < 2 {
        return None;
    }
    let (mut v, mut shift) = (0u64, 0u32);
    for &b in &msg[1..] {
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

impl TripleState {
    /// [DR] §6.4 RatchetInitAliceTR.
    pub fn init_alice<R: RngCore + CryptoRng>(sk: &[u8; 32], ad: Vec<u8>, bob_spk: &[u8; 32], rng: &mut R) -> Result<Self, Error> {
        let (sk_ec, sk_scka) = tr_split(sk);
        Ok(TripleState {
            ec: EcState::init_alice(&sk_ec, bob_spk, rng),
            pq: spqr::initial_state(spqr_params(spqr::Direction::A2B, &sk_scka))?,
            ad,
        })
    }

    /// [DR] §6.4 RatchetInitBobTR.
    pub fn init_bob(sk: &[u8; 32], ad: Vec<u8>, spk_secret: &[u8; 32]) -> Result<Self, Error> {
        let (sk_ec, sk_scka) = tr_split(sk);
        Ok(TripleState {
            ec: EcState::init_bob(&sk_ec, spk_secret),
            pq: spqr::initial_state(spqr_params(spqr::Direction::B2A, &sk_scka))?,
            ad,
        })
    }

    /// [DR] §6.5 TripleRatchetEncrypt. State is committed only if every step succeeds.
    pub fn encrypt<R: RngCore + CryptoRng>(&mut self, plaintext: &[u8], rng: &mut R) -> Result<Encrypted, Error> {
        let mut ec = self.ec.clone();
        let (n, ec_mk) = ec.send_key()?;
        let s = spqr::send(&self.pq, rng)?;
        let pq_mk = s.key.ok_or(Error::PqRatchetUnavailable)?;
        let (key, nonce) = kdf_hybrid(&ec_mk, &pq_mk);
        let header = encode_header(&ec.dhs_pub, ec.pn, n, &s.msg);
        let ct = aead_seal(&key, &nonce, &self.concat_ad(&header), plaintext);
        self.ec = ec;
        self.pq = s.state;
        Ok(Encrypted { header, ct, keys: MsgKeys { ec_mk, pq_mk, key, nonce } })
    }

    /// [DR] §6.6 TripleRatchetDecrypt; on any failure the state is unchanged ([DR] §3.5).
    pub fn decrypt<R: RngCore + CryptoRng>(&mut self, header: &[u8], ct: &[u8], rng: &mut R) -> Result<(Vec<u8>, MsgKeys), Error> {
        let h = parse_header(header)?;
        let mut ec = self.ec.clone();
        let ec_mk = ec.receive_key(&h.dh, h.pn, h.n, rng)?;
        let r = spqr::recv(&self.pq, &h.scka)?;
        let pq_mk = r.key.ok_or(Error::PqRatchetUnavailable)?;
        let (key, nonce) = kdf_hybrid(&ec_mk, &pq_mk);
        let pt = aead_open(&key, &nonce, &self.concat_ad(header), ct)?;
        self.ec = ec;
        self.pq = r.state;
        Ok((pt, MsgKeys { ec_mk, pq_mk, key, nonce }))
    }

    /// [DR] §3.1 CONCAT(ad, header): AD is fixed-length (2 x 33 bytes), header is self-delimiting CBOR.
    fn concat_ad(&self, header: &[u8]) -> Vec<u8> {
        let mut a = self.ad.clone();
        a.extend_from_slice(header);
        a
    }

    pub fn to_cbor(&self) -> V {
        V::M(vec![(1, self.ec.to_cbor()), (2, V::B(self.pq.clone())), (3, V::B(self.ad.clone()))])
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let ad = v.get(3)?.b()?.to_vec();
        if ad.len() != 66 {
            return Err(Error::Decode("state: AD length"));
        }
        Ok(TripleState { ec: EcState::from_cbor(v.get(1)?)?, pq: v.get(2)?.b()?.to_vec(), ad })
    }
}

pub fn aead_seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], pt: &[u8]) -> Vec<u8> {
    let c = Aes256GcmSiv::new_from_slice(key).expect("32-byte key");
    c.encrypt(&Nonce::from(*nonce), Payload { msg: pt, aad }).expect("GCM-SIV encrypt")
}

pub fn aead_open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> Result<Vec<u8>, Error> {
    let c = Aes256GcmSiv::new_from_slice(key).expect("32-byte key");
    c.decrypt(&Nonce::from(*nonce), Payload { msg: ct, aad }).map_err(|_| Error::DecryptFailed)
}
