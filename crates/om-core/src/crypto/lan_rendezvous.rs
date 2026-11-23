// SPDX-License-Identifier: MPL-2.0
//! `LanDeliveryProfile-v1` rendezvous constructions (ADR-0158 sections 1 and 2). Pure functions over bytes: no state, no
//! clock, no I/O. The state machine that uses them is `lan.rs`; the independent vectors are
//! `verification/vectors/lan-rendezvous-vectors.json` (made by `verification/prototypes/lan-rendezvous/`, replayed by
//! `tests/lan_rendezvous_vectors.rs`). Labels are registered in `spec/registries/om-domain-separation.json`.

use super::cbor::{self, V};
use super::kdf::{hkdf_sha256, hmac_sha256};
use zeroize::Zeroize;

pub const LBL_PAIR_SEED: &str = "OM-LAN-PAIR-SEED-v1";
pub const LBL_INVITE_SEED: &str = "OM-LAN-INVITE-SEED-v1";
pub const LBL_TAG: &str = "OM-LAN-RENDEZVOUS-TAG-v1";

/// Rendezvous epoch length (ADR-0158 section 1).
pub const EPOCH_SECONDS: u64 = 900;
/// Rendezvous tag length.
pub const TAG_LEN: usize = 16;
pub type Tag = [u8; TAG_LEN];

/// The 32-byte rendezvous seed of one device pair or one invite. Relationship-private state: never advertised, never
/// uploaded, destroyed with the relationship (ADR-0158 section 2). It has no `Debug`, `Clone` or serialization.
pub struct LanSeed([u8; 32]);

impl Drop for LanSeed {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl LanSeed {
    /// For tests and vector replay only; production code gets a seed from [`pair_seed`] or [`invite_seed`].
    pub fn from_bytes_for_test(b: [u8; 32]) -> Self {
        Self(b)
    }
    pub fn expose_for_test(&self) -> [u8; 32] {
        self.0
    }
    /// The seed bytes as HKDF input for sibling derivations (`lan_signal`). Callers must zeroize the copy.
    pub(crate) fn expose_for_derivation(&self) -> [u8; 32] {
        self.0
    }
    fn from_okm(mut okm: [u8; 32]) -> Self {
        let s = Self(okm);
        okm.zeroize();
        s
    }
}

/// `HKDF-SHA256(SK, salt = "", info = OM-CBOR [OM-LAN-PAIR-SEED-v1, crypto_profile, initiator_device_id,
/// responder_device_id], 32)`. `sk` is the purpose-separated export of the PQXDH secret at the session establishment
/// that created the pair's current ratchet; a ratchet key is never an input.
pub fn pair_seed(
    sk: &[u8; 32],
    crypto_profile: u64,
    initiator_device_id: &[u8; 16],
    responder_device_id: &[u8; 16],
) -> LanSeed {
    let info = cbor::encode(&V::A(vec![
        V::T(LBL_PAIR_SEED.into()),
        V::U(crypto_profile),
        V::B(initiator_device_id.to_vec()),
        V::B(responder_device_id.to_vec()),
    ]));
    let mut o = [0u8; 32];
    hkdf_sha256(&[], sk, &info, &mut o);
    LanSeed::from_okm(o)
}

/// `HKDF-SHA256(invite_secret, salt = "", info = OM-CBOR [OM-LAN-INVITE-SEED-v1, invite_id], 32)`.
pub fn invite_seed(invite_secret: &[u8; 32], invite_id: &[u8; 16]) -> LanSeed {
    let info = cbor::encode(&V::A(vec![
        V::T(LBL_INVITE_SEED.into()),
        V::B(invite_id.to_vec()),
    ]));
    let mut o = [0u8; 32];
    hkdf_sha256(&[], invite_secret, &info, &mut o);
    LanSeed::from_okm(o)
}

/// `e = floor(posix_seconds / 900)`. Wall time is liveness metadata only, never ordering or authority.
pub fn epoch_of(posix_seconds: u64) -> u64 {
    posix_seconds / EPOCH_SECONDS
}

/// The epochs a receiver accepts at `posix_seconds`: `e-1`, `e`, `e+1` (clock skew only). Epoch 0 has no predecessor.
pub fn accepted_epochs(posix_seconds: u64) -> impl Iterator<Item = u64> {
    let e = epoch_of(posix_seconds);
    [e.checked_sub(1), Some(e), e.checked_add(1)]
        .into_iter()
        .flatten()
}

/// `Trunc16(HMAC-SHA256(seed, OM-CBOR [OM-LAN-RENDEZVOUS-TAG-v1, e]))`.
pub fn tag(seed: &LanSeed, epoch: u64) -> Tag {
    let m = hmac_sha256(
        &seed.0,
        &cbor::encode(&V::A(vec![V::T(LBL_TAG.into()), V::U(epoch)])),
    );
    let mut t = [0u8; TAG_LEN];
    t.copy_from_slice(&m[..TAG_LEN]);
    t
}
