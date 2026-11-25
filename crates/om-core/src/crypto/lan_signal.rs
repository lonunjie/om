// SPDX-License-Identifier: MPL-2.0
//! `LanSignalEnvelopeV1` seal and open (ADR-0158 section 3). The WebRTC SDP is never sent in clear: it is encrypted under
//! a key derived from the rendezvous seed with the profile's existing AES-256-GCM-SIV primitive. No new primitive.
//!
//! `SignalKey = HKDF-SHA256(LanSeed, salt = "", info = OM-CBOR [OM-LAN-SIGNAL-KEY-v1, session_id, epoch], 32)`;
//! associated data = `OM-CBOR [OM-LAN-SIGNAL-AAD-v1, version, kind, session_id, epoch]`; the nonce is 12 fresh random
//! bytes chosen by the caller. [`SignalGuard`] refuses a repeated `(session_id, epoch, nonce)` on both seal and open, so
//! nonce reuse under one SignalKey and replay of a captured envelope are both rejected.
//! Independent vectors: `verification/vectors/lan-rendezvous-vectors.json` (`signal`).

use super::cbor::{self, V};
use super::kdf::hkdf_sha256;
use super::lan_rendezvous::{self as lr, LanSeed};
use super::session::{aead_open, aead_seal};
use om_codec::{Registry, Value};
use std::collections::BTreeSet;
use std::sync::OnceLock;
use zeroize::Zeroize;

pub const LBL_KEY: &str = "OM-LAN-SIGNAL-KEY-v1";
pub const LBL_AAD: &str = "OM-LAN-SIGNAL-AAD-v1";
/// Largest SDP: the envelope ciphertext is at most 12,304 bytes and carries a 16-byte tag (`lan.cddl`).
pub const MAX_SDP: usize = 12_288;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Offer = 1,
    Answer = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SignalError {
    /// The bytes are not a canonical `LanSignalEnvelopeV1`.
    Malformed,
    /// The epoch is outside the accepted window e-1, e, e+1.
    Epoch,
    /// The same `(session_id, epoch, nonce)` was already used or seen.
    Replay,
    /// Authentication failed (wrong seed, tampered bytes or tampered header).
    Decrypt,
    /// The plaintext is not UTF-8, or the SDP is empty or over [`MAX_SDP`].
    Payload,
    /// No LAN seed is held for this peer.
    NoSeed,
}

/// What an opened envelope yields: kind, session id and SDP.
pub type Opened = (Kind, [u8; 16], String);

pub fn signal_key(seed: &LanSeed, session_id: &[u8; 16], epoch: u64) -> [u8; 32] {
    let info = cbor::encode(&V::A(vec![
        V::T(LBL_KEY.into()),
        V::B(session_id.to_vec()),
        V::U(epoch),
    ]));
    let mut k = [0u8; 32];
    let mut ikm = seed.expose_for_derivation();
    hkdf_sha256(&[], &ikm, &info, &mut k);
    ikm.zeroize();
    k
}

pub fn signal_aad(kind: Kind, session_id: &[u8; 16], epoch: u64) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(LBL_AAD.into()),
        V::U(1),
        V::U(kind as u64),
        V::B(session_id.to_vec()),
        V::U(epoch),
    ]))
}

/// Remembers `(session_id, epoch, nonce)` triples inside the accepted window.
#[derive(Default)]
pub struct SignalGuard {
    seen: BTreeSet<([u8; 16], u64, [u8; 12])>,
}

impl SignalGuard {
    fn record(
        &mut self,
        now: u64,
        sid: [u8; 16],
        epoch: u64,
        nonce: [u8; 12],
    ) -> Result<(), SignalError> {
        let e = lr::epoch_of(now);
        // Forget anything older than the window so the set stays bounded by one window of traffic.
        self.seen.retain(|(_, ep, _)| *ep + 1 >= e);
        if self.seen.insert((sid, epoch, nonce)) {
            Ok(())
        } else {
            Err(SignalError::Replay)
        }
    }
}

/// Builds a canonical `LanSignalEnvelopeV1` for `sdp` in the epoch of `now`.
pub fn seal(
    seed: &LanSeed,
    guard: &mut SignalGuard,
    now: u64,
    kind: Kind,
    session_id: [u8; 16],
    nonce: [u8; 12],
    sdp: &str,
) -> Result<Vec<u8>, SignalError> {
    if sdp.is_empty() || sdp.len() > MAX_SDP {
        return Err(SignalError::Payload);
    }
    let epoch = lr::epoch_of(now);
    guard.record(now, session_id, epoch, nonce)?;
    let mut key = signal_key(seed, &session_id, epoch);
    let ct = aead_seal(
        &key,
        &nonce,
        &signal_aad(kind, &session_id, epoch),
        sdp.as_bytes(),
    );
    key.zeroize();
    Ok(cbor::encode(&V::M(vec![
        (0, V::U(1)),
        (1, V::U(kind as u64)),
        (2, V::B(session_id.to_vec())),
        (3, V::U(epoch)),
        (4, V::B(nonce.to_vec())),
        (5, V::B(ct)),
    ])))
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::load_w1)
}

fn field(m: &[(Value, Value)], k: u64) -> Option<&Value> {
    m.iter().find(|(x, _)| *x == Value::Uint(k)).map(|(_, v)| v)
}

/// Opens an envelope with the candidate `seed`: strict canonical decode, epoch window, replay check, then AEAD. The
/// replay triple is recorded only after authentication succeeds, so a forged envelope cannot burn a legitimate one.
pub fn open(
    seed: &LanSeed,
    guard: &mut SignalGuard,
    now: u64,
    bytes: &[u8],
) -> Result<(Kind, [u8; 16], String), SignalError> {
    let Ok(Value::Map(m)) = registry().check_struct("LanSignalEnvelopeV1", bytes, 131_068) else {
        return Err(SignalError::Malformed);
    };
    let (
        Some(Value::Uint(kind)),
        Some(Value::Bytes(sid)),
        Some(Value::Uint(epoch)),
        Some(Value::Bytes(nonce)),
        Some(Value::Bytes(ct)),
    ) = (
        field(&m, 1),
        field(&m, 2),
        field(&m, 3),
        field(&m, 4),
        field(&m, 5),
    )
    else {
        return Err(SignalError::Malformed);
    };
    let kind = match kind {
        1 => Kind::Offer,
        2 => Kind::Answer,
        _ => return Err(SignalError::Malformed),
    };
    let (sid, nonce): ([u8; 16], [u8; 12]) = (
        sid.as_slice()
            .try_into()
            .map_err(|_| SignalError::Malformed)?,
        nonce
            .as_slice()
            .try_into()
            .map_err(|_| SignalError::Malformed)?,
    );
    if !lr::accepted_epochs(now).any(|e| e == *epoch) {
        return Err(SignalError::Epoch);
    }
    let mut key = signal_key(seed, &sid, *epoch);
    let pt = aead_open(&key, &nonce, &signal_aad(kind, &sid, *epoch), ct);
    key.zeroize();
    let pt = pt.map_err(|_| SignalError::Decrypt)?;
    let sdp = String::from_utf8(pt).map_err(|_| SignalError::Payload)?;
    if sdp.is_empty() || sdp.len() > MAX_SDP {
        return Err(SignalError::Payload);
    }
    guard.record(now, sid, *epoch, nonce)?;
    Ok((kind, sid, sdp))
}
