// SPDX-License-Identifier: MPL-2.0
//! Bounded ADR-0142 sealed-delivery HPKE feasibility path. This is verification code only.
//! It reuses the prototype's pinned draft-ietf-hpke-pq-05 implementation; it adds no KEM.

use crate::crypto::cbor::{self, V};
use crate::crypto::hpke;
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::crypto::sig::digest;
use crate::crypto::{Error, PROFILE_V1};

pub const ROUTE_LABEL: &str = "OM-SEALED-DELIVERY-ROUTE-v1";
pub const INFO_LABEL: &str = "OM-SEALED-DELIVERY-HPKE-INFO-v1";
pub const AAD_LABEL: &str = "OM-SEALED-DELIVERY-HPKE-AAD-v1";
pub const BUCKETS: [usize; 5] = [1024, 4096, 16384, 65536, 131072];
pub const TAG_LEN: usize = 16;
pub const CLOCK_SKEW: u64 = 300;

#[derive(Clone, Debug)]
pub struct OpenedDelivery {
    pub inner_envelope: Vec<u8>,
    /// Outer HPKE authenticated only the delivery key and context.
    pub outer_hpke_opened: bool,
    /// Sender authentication remains for RelationshipContext + inner OM session processing.
    pub sender_authenticated: bool,
}

pub fn route_binding(relay_uri: &str, deposit_capability: &[u8]) -> [u8; 32] {
    let route = cbor::encode(&V::A(vec![
        V::T(ROUTE_LABEL.into()),
        V::T(relay_uri.into()),
        V::B(deposit_capability.to_vec()),
    ]));
    crate::crypto::sha256(&[&route])
}

pub fn info(profile: u64, key_id: &[u8; 16], route: &[u8; 32]) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(INFO_LABEL.into()),
        V::U(profile),
        V::B(key_id.to_vec()),
        digest(route),
    ]))
}

pub fn aad(
    profile: u64,
    key_id: &[u8; 16],
    delivery_id: &[u8; 16],
    expires_at: u64,
    bucket: usize,
) -> Vec<u8> {
    cbor::encode(&V::A(vec![
        V::T(AAD_LABEL.into()),
        V::U(23),
        V::U(1),
        V::U(profile),
        V::B(key_id.to_vec()),
        V::B(delivery_id.to_vec()),
        V::U(expires_at),
        V::U(bucket as u64),
    ]))
}

fn bucket_for(n: usize) -> Result<usize, Error> {
    BUCKETS
        .iter()
        .copied()
        .find(|b| n.checked_add(4).is_some_and(|x| x <= *b))
        .ok_or(Error::SealedDelivery(
            "inner envelope exceeds largest bucket",
        ))
}

/// Deterministic seam for vectors. `enc_randomness` is the draft KEM's 64-byte random input;
/// padding is test-only input, and must have exactly bucket-4-inner.len() bytes.
pub fn seal_deterministic(
    recipient_pk: &[u8],
    key_id: &[u8; 16],
    delivery_id: &[u8; 16],
    relay_uri: &str,
    capability: &[u8],
    expires_at: u64,
    inner: &[u8],
    enc_randomness: &[u8; 64],
    padding: &[u8],
) -> Result<Vec<u8>, Error> {
    if recipient_pk.len() != hpke::NPK {
        return Err(Error::SealedDelivery("profile-1 public key length"));
    }
    let bucket = bucket_for(inner.len())?;
    if padding.len() != bucket - 4 - inner.len() {
        return Err(Error::SealedDelivery("padding length"));
    }
    let route = route_binding(relay_uri, capability);
    let info = info(PROFILE_V1, key_id, &route);
    let aad = aad(PROFILE_V1, key_id, delivery_id, expires_at, bucket);
    let mut pt = Vec::with_capacity(bucket);
    pt.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    pt.extend_from_slice(inner);
    pt.extend_from_slice(padding);
    let (ss, enc) = hpke::encaps_derand(recipient_pk, enc_randomness)?;
    let ct = hpke::key_schedule(&ss, &info).seal(&aad, &pt);
    Ok(cbor::encode(&V::M(vec![
        (0, V::U(23)),
        (1, V::U(1)),
        (2, V::U(PROFILE_V1)),
        (3, V::B(key_id.to_vec())),
        (4, V::B(delivery_id.to_vec())),
        (5, V::B(enc)),
        (6, V::B(ct)),
        (7, V::U(expires_at)),
    ])))
}

pub fn seal<R: RngCore + CryptoRng>(
    recipient_pk: &[u8],
    key_id: &[u8; 16],
    relay_uri: &str,
    capability: &[u8],
    expires_at: u64,
    inner: &[u8],
    rng: &mut R,
) -> Result<Vec<u8>, Error> {
    let bucket = bucket_for(inner.len())?;
    let mut delivery_id = [0u8; 16];
    rng.fill_bytes(&mut delivery_id);
    let mut randomness = [0u8; 64];
    rng.fill_bytes(&mut randomness);
    let mut padding = vec![0u8; bucket - 4 - inner.len()];
    rng.fill_bytes(&mut padding);
    seal_deterministic(
        recipient_pk,
        key_id,
        &delivery_id,
        relay_uri,
        capability,
        expires_at,
        inner,
        &randomness,
        &padding,
    )
}

pub fn open(
    wrapper: &[u8],
    recipient_seed: &[u8; 32],
    expected_key_id: &[u8; 16],
    relay_uri: &str,
    capability: &[u8],
    now: u64,
) -> Result<OpenedDelivery, Error> {
    let bad = |s| Error::SealedDelivery(s);
    let v = cbor::decode(wrapper)?;
    let fields = match &v {
        V::M(m) => m,
        _ => return Err(bad("wrapper must be a map")),
    };
    for (key, _) in fields {
        if !matches!(*key, 0..=7) {
            return Err(bad("unknown critical field or unsupported extension"));
        }
    }
    if v.get(0)?.u()? != 23 || v.get(1)?.u()? != 1 {
        return Err(bad("wrapper type/version"));
    }
    let profile = v.get(2)?.u()?;
    if profile != PROFILE_V1 {
        return Err(bad("unsupported profile"));
    }
    let key = v.get(3)?.b()?;
    let id = v.get(4)?.b()?;
    if key.len() != 16 || id.len() != 16 {
        return Err(bad("key/id length"));
    }
    if key != expected_key_id {
        return Err(bad("wrong delivery key"));
    }
    let expires = v.get(7)?.u()?;
    if now > expires.saturating_add(CLOCK_SKEW) {
        return Err(bad("expired"));
    }
    let enc = v.get(5)?.b()?;
    let ct = v.get(6)?.b()?;
    if enc.len() != hpke::NENC {
        return Err(bad("profile-1 encapsulation length"));
    }
    let bucket = ct
        .len()
        .checked_sub(TAG_LEN)
        .ok_or(bad("ciphertext length"))?;
    if !BUCKETS.contains(&bucket) || ct.len() != bucket + TAG_LEN {
        return Err(bad("ciphertext padding bucket"));
    }
    let route = route_binding(relay_uri, capability);
    let info = info(profile, expected_key_id, &route);
    let aad = aad(
        profile,
        expected_key_id,
        &id.try_into().unwrap(),
        expires,
        bucket,
    );
    let ss = hpke::decaps(recipient_seed, enc)?;
    let pt = hpke::key_schedule(&ss, &info).open(&aad, ct)?;
    if pt.len() != bucket || pt.len() < 4 {
        return Err(bad("plaintext bucket length"));
    }
    let n = u32::from_be_bytes(pt[..4].try_into().unwrap()) as usize;
    if n > pt.len() - 4 || bucket_for(n)? != bucket {
        return Err(bad("nonconforming plaintext bucket"));
    }
    Ok(OpenedDelivery {
        inner_envelope: pt[4..4 + n].to_vec(),
        outer_hpke_opened: true,
        sender_authenticated: false,
    })
}
