// SPDX-License-Identifier: MPL-2.0
//! Elliptic-curve Double Ratchet key schedule per [DR] §3 (used as the `ec_state` half of the
//! Triple Ratchet, [DR] §6.2). Only produces message keys; encryption happens in triple.rs.

use crate::crypto::Error;
use crate::crypto::cbor::{V, get_opt_b32, opt_b32};
use crate::crypto::kdf::{kdf_ck, kdf_rk};
use crate::crypto::pqxdh::{dh, x25519_pub};
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use std::collections::BTreeMap;

/// [DR] §3.1 MAX_SKIP (per chain).
pub const MAX_SKIP: u64 = 1000;
/// [DR] §8.4 per-session cap on stored skipped keys.
pub const MAX_SKIPPED_TOTAL: usize = 2000;

#[derive(Clone, PartialEq, Eq)]
pub struct EcState {
    pub dhs_secret: [u8; 32],
    pub dhs_pub: [u8; 32],
    pub dhr: Option<[u8; 32]>,
    pub rk: [u8; 32],
    pub cks: Option<[u8; 32]>,
    pub ckr: Option<[u8; 32]>,
    pub ns: u64,
    pub nr: u64,
    pub pn: u64,
    /// MKSKIPPED, ordered for deterministic serialization.
    pub skipped: BTreeMap<([u8; 32], u64), [u8; 32]>,
}

impl core::fmt::Debug for EcState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("EcState(<redacted>)")
    }
}

impl Drop for EcState {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.dhs_secret.zeroize();
        self.rk.zeroize();
        self.cks.zeroize();
        self.ckr.zeroize();
        for mk in self.skipped.values_mut() {
            mk.zeroize();
        }
    }
}

impl EcState {
    /// [DR] §3.3 RatchetInitAlice.
    pub fn init_alice<R: RngCore + CryptoRng>(
        sk: &[u8; 32],
        bob_pub: &[u8; 32],
        rng: &mut R,
    ) -> Self {
        let dhs_secret = arr32(rng);
        let (rk, cks) = kdf_rk(sk, &dh(&dhs_secret, bob_pub));
        EcState {
            dhs_pub: x25519_pub(&dhs_secret),
            dhs_secret,
            dhr: Some(*bob_pub),
            rk,
            cks: Some(cks),
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            skipped: BTreeMap::new(),
        }
    }

    /// [DR] §3.3 RatchetInitBob; [DR] §7.1 Bob's signed prekey is his initial ratchet key pair.
    pub fn init_bob(sk: &[u8; 32], spk_secret: &[u8; 32]) -> Self {
        EcState {
            dhs_pub: x25519_pub(spk_secret),
            dhs_secret: *spk_secret,
            dhr: None,
            rk: *sk,
            cks: None,
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            skipped: BTreeMap::new(),
        }
    }

    /// [DR] §3.4 RatchetSendKey. Returns (Ns, mk); header is (DHs, PN, Ns).
    pub fn send_key(&mut self) -> Result<(u64, [u8; 32]), Error> {
        let ck = self.cks.ok_or(Error::NoChain)?;
        let (ck2, mk) = kdf_ck(&ck);
        self.cks = Some(ck2);
        let n = self.ns;
        self.ns += 1;
        Ok((n, mk))
    }

    /// [DR] §3.5 RatchetReceiveKey. Callers operate on a clone and commit only on AEAD success.
    pub fn receive_key<R: RngCore + CryptoRng>(
        &mut self,
        dh_pub: &[u8; 32],
        pn: u64,
        n: u64,
        rng: &mut R,
    ) -> Result<[u8; 32], Error> {
        if let Some(mk) = self.skipped.remove(&(*dh_pub, n)) {
            return Ok(mk);
        }
        if Some(*dh_pub) != self.dhr {
            self.skip_message_keys(pn)?;
            self.dh_ratchet(dh_pub, rng);
        }
        self.skip_message_keys(n)?;
        let ck = self.ckr.ok_or(Error::NoChain)?;
        let (ck2, mk) = kdf_ck(&ck);
        self.ckr = Some(ck2);
        self.nr += 1;
        Ok(mk)
    }

    fn skip_message_keys(&mut self, until: u64) -> Result<(), Error> {
        if self.nr + MAX_SKIP < until {
            return Err(Error::TooManySkipped);
        }
        if let (Some(mut ck), Some(dhr)) = (self.ckr, self.dhr) {
            while self.nr < until {
                if self.skipped.len() >= MAX_SKIPPED_TOTAL {
                    return Err(Error::TooManySkipped);
                }
                let (ck2, mk) = kdf_ck(&ck);
                ck = ck2;
                self.skipped.insert((dhr, self.nr), mk);
                self.nr += 1;
            }
            self.ckr = Some(ck);
        }
        Ok(())
    }

    /// [DR] §3.5 DHRatchet.
    fn dh_ratchet<R: RngCore + CryptoRng>(&mut self, dh_pub: &[u8; 32], rng: &mut R) {
        self.pn = self.ns;
        self.ns = 0;
        self.nr = 0;
        self.dhr = Some(*dh_pub);
        let (rk, ckr) = kdf_rk(&self.rk, &dh(&self.dhs_secret, dh_pub));
        self.rk = rk;
        self.ckr = Some(ckr);
        self.dhs_secret = arr32(rng);
        self.dhs_pub = x25519_pub(&self.dhs_secret);
        let (rk, cks) = kdf_rk(&self.rk, &dh(&self.dhs_secret, dh_pub));
        self.rk = rk;
        self.cks = Some(cks);
    }

    pub fn to_cbor(&self) -> V {
        V::M(vec![
            (1, V::B(self.dhs_secret.to_vec())),
            (2, opt_b32(self.dhr)),
            (3, V::B(self.rk.to_vec())),
            (4, opt_b32(self.cks)),
            (5, opt_b32(self.ckr)),
            (6, V::U(self.ns)),
            (7, V::U(self.nr)),
            (8, V::U(self.pn)),
            (
                9,
                V::A(
                    self.skipped
                        .iter()
                        .map(|((d, n), mk)| {
                            V::A(vec![V::B(d.to_vec()), V::U(*n), V::B(mk.to_vec())])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_cbor(v: &V) -> Result<Self, Error> {
        let dhs_secret = v.get(1)?.b32()?;
        let mut skipped = BTreeMap::new();
        let sk = v.get(9)?.a()?;
        if sk.len() > MAX_SKIPPED_TOTAL {
            return Err(Error::Decode("state: too many skipped keys"));
        }
        for e in sk {
            let e = e.a()?;
            if e.len() != 3 {
                return Err(Error::Decode("state: skipped entry"));
            }
            if skipped
                .insert((e[0].b32()?, e[1].u()?), e[2].b32()?)
                .is_some()
            {
                return Err(Error::Decode("state: duplicate skipped key"));
            }
        }
        Ok(EcState {
            dhs_pub: x25519_pub(&dhs_secret),
            dhs_secret,
            dhr: get_opt_b32(v.get(2)?)?,
            rk: v.get(3)?.b32()?,
            cks: get_opt_b32(v.get(4)?)?,
            ckr: get_opt_b32(v.get(5)?)?,
            ns: v.get(6)?.u()?,
            nr: v.get(7)?.u()?,
            pn: v.get(8)?.u()?,
            skipped,
        })
    }
}
