// SPDX-License-Identifier: MPL-2.0
//! OM-Crypto-v1 1:1 session: Signal Double Ratchet [DR] §3 (ratchet.rs key schedule) with the payload
//! AEAD pinned by ADR-0131 rule 3 and ADR-0135 §4. No SPQR (ADR-0118: SPQR is OM-Crypto-v2), and no
//! header encryption (ADR-0131 rule 4): the header below is visible to the routing layer.
//!
//!   header      = OM-CBOR [profile = 1, dh: bstr .size 32, pn: uint, n: uint]
//!   header_hash = DS-HASH("OM-DR-HEADER-HASH-v1", header)
//!   K, N        = kdf::dr_aead_key_nonce(MK, header_hash)
//!   AD          = OM-CBOR ["OM-1TO1-EVENT-AEAD-AD-v1", profile, session_ad, header_hash, envelope_context]
//!   ciphertext  = AES-256-GCM-SIV(K, N, plaintext, AD)
//!
//! `session_ad` is the PQXDH AD (EncodeEC(IK_A) || EncodeEC(IK_B)); `envelope_context` is the immutable
//! transport context of the message (here: the envelope prefix, see store.rs).

use crate::crypto::cbor::{self, V};
use crate::crypto::kdf::{DR_HEADER_HASH, EVENT_AEAD_AD, dr_aead_key_nonce};
use crate::crypto::ratchet::EcState;
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::crypto::sig::ds_hash;
use crate::crypto::{Error, PROFILE_V1};
use aes_gcm_siv::aead::{Aead, KeyInit, Payload};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionState {
    pub ec: EcState,
    /// PQXDH AD, 66 bytes.
    pub ad: Vec<u8>,
}

/// Keys observed while processing one message (exported to vectors/benchmarks only).
#[derive(Clone, Debug)]
pub struct MsgKeys {
    pub mk: [u8; 32],
    pub header_hash: [u8; 32],
    pub key: [u8; 32],
    pub nonce: [u8; 12],
    pub aad: Vec<u8>,
}

pub struct Encrypted {
    pub header: Vec<u8>,
    pub ct: Vec<u8>,
    pub keys: MsgKeys,
}

pub struct Header {
    pub dh: [u8; 32],
    pub pn: u64,
    pub n: u64,
}

pub fn encode_header(dh: &[u8; 32], pn: u64, n: u64) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::U(PROFILE_V1),
        V::B(dh.to_vec()),
        V::U(pn),
        V::U(n),
    ]))
}

pub fn parse_header(b: &[u8]) -> Result<Header, Error> {
    let v = cbor::decode(b)?;
    let a = v.a()?;
    if a.len() != 4 {
        return Err(Error::Decode("header arity"));
    }
    if a[0].u()? != PROFILE_V1 {
        return Err(Error::Decode("header profile"));
    }
    Ok(Header {
        dh: a[1].b32()?,
        pn: a[2].u()?,
        n: a[3].u()?,
    })
}

pub fn aad(session_ad: &[u8], header_hash: &[u8; 32], ctx: &[u8]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(EVENT_AEAD_AD.into()),
        V::U(PROFILE_V1),
        V::B(session_ad.to_vec()),
        V::B(header_hash.to_vec()),
        V::B(ctx.to_vec()),
    ]))
}

fn derive(mk: [u8; 32], header: &[u8], session_ad: &[u8], ctx: &[u8]) -> MsgKeys {
    let header_hash = ds_hash(DR_HEADER_HASH, header);
    let (key, nonce) = dr_aead_key_nonce(&mk, &header_hash);
    MsgKeys {
        mk,
        header_hash,
        key,
        nonce,
        aad: aad(session_ad, &header_hash, ctx),
    }
}

impl SessionState {
    /// [DR] §3.3 RatchetInitAlice; PQXDH SK is the initial root key, Bob's signed prekey his ratchet key.
    pub fn init_alice<R: RngCore + CryptoRng>(
        sk: &[u8; 32],
        ad: Vec<u8>,
        bob_spk: &[u8; 32],
        rng: &mut R,
    ) -> Self {
        SessionState {
            ec: EcState::init_alice(sk, bob_spk, rng),
            ad,
        }
    }

    /// [DR] §3.3 RatchetInitBob.
    pub fn init_bob(sk: &[u8; 32], ad: Vec<u8>, spk_secret: &[u8; 32]) -> Self {
        SessionState {
            ec: EcState::init_bob(sk, spk_secret),
            ad,
        }
    }

    /// State is committed only if every step succeeds.
    pub fn encrypt(&mut self, plaintext: &[u8], ctx: &[u8]) -> Result<Encrypted, Error> {
        let mut ec = self.ec.clone();
        let (n, mk) = ec.send_key()?;
        let header = encode_header(&ec.dhs_pub, ec.pn, n);
        let keys = derive(mk, &header, &self.ad, ctx);
        let ct = aead_seal(&keys.key, &keys.nonce, &keys.aad, plaintext);
        self.ec = ec;
        Ok(Encrypted { header, ct, keys })
    }

    /// On any failure the state is unchanged ([DR] §3.5).
    pub fn decrypt<R: RngCore + CryptoRng>(
        &mut self,
        header: &[u8],
        ct: &[u8],
        ctx: &[u8],
        rng: &mut R,
    ) -> Result<(Vec<u8>, MsgKeys), Error> {
        let h = parse_header(header)?;
        let mut ec = self.ec.clone();
        let mk = ec.receive_key(&h.dh, h.pn, h.n, rng)?;
        let keys = derive(mk, header, &self.ad, ctx);
        let pt = aead_open(&keys.key, &keys.nonce, &keys.aad, ct)?;
        self.ec = ec;
        Ok((pt, keys))
    }

    pub fn to_cbor(&self) -> V {
        V::M(vec![(1, self.ec.to_cbor()), (3, V::B(self.ad.clone()))])
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let ad = v.get(3)?.b()?.to_vec();
        if ad.len() != 66 {
            return Err(Error::Decode("state: AD length"));
        }
        Ok(SessionState {
            ec: EcState::from_cbor(v.get(1)?)?,
            ad,
        })
    }
}

pub fn aead_seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], pt: &[u8]) -> Vec<u8> {
    let c = Aes256GcmSiv::new_from_slice(key).expect("32-byte key");
    c.encrypt(&Nonce::from(*nonce), Payload { msg: pt, aad })
        .expect("GCM-SIV encrypt")
}

pub fn aead_open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ct: &[u8],
) -> Result<Vec<u8>, Error> {
    let c = Aes256GcmSiv::new_from_slice(key).expect("32-byte key");
    c.decrypt(&Nonce::from(*nonce), Payload { msg: ct, aad })
        .map_err(|_| Error::DecryptFailed)
}
