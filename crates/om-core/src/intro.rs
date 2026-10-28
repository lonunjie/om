// SPDX-License-Identifier: MPL-2.0
//! First-contact objects built by Core (ADR-0181): the per-invite intro identity, the signed `ContactDescriptor`,
//! `IntroductionTicket`, `MessageRequestPayload` and `IntroIdentityBinding`.
//!
//! All four use the frozen W0 shapes, signature envelopes and domain-separation labels of
//! `spec/cddl/discovery.cddl`; a descriptor carries no account material, and the payload carries no AccountID.
//! The intro identity is a composite Ed25519 AND ML-DSA-65 key (the device construction) used with signature
//! role 4. Core derives it from a profile secret and the invite id, so nothing is stored per invite.

use crate::crypto::binding::{Acs, Credential};
use crate::crypto::cbor::{self, V};
use crate::crypto::kdf::hkdf_sha256;
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{
    self, DeviceSigner, ROLE_DEVICE, ROLE_INTRO_IDENTITY, digest, ds_hash, ds_sig, envelope,
    open_envelope, signature_entry,
};
use crate::crypto::{Error, PROFILE_V1};

pub const LBL_DESCRIPTOR_SIG: &str = "OM-CONTACT-DESCRIPTOR-SIG-v1";
pub const LBL_TICKET_SIG: &str = "OM-INTRO-TICKET-SIG-v1";
pub const LBL_PAYLOAD_SIG: &str = "OM-MESSAGE-REQUEST-PAYLOAD-SIG-v1";
pub const LBL_BINDING_SIG: &str = "OM-INTRO-IDENTITY-BINDING-SIG-v1";

/// Core-local labels (not wire registry labels; they never leave the profile except as the link digest).
const HKDF_SALT: &[u8] = b"om-core:intro-identity/v0";
const KEY_DIGEST_LABEL: &str = "om-core:intro-key-digest/v0";
/// Stand-in `introduction_services` entry for a Core-issued descriptor (no IntroGate adapter exists yet). An inviter
/// descriptor appends the request mailbox: `om-core:intro/v0/<32 hex>`; it is signed, so the introduction service
/// cannot redirect requests.
pub const INTRO_SERVICE_URI: &str = "om-core:intro/v0";

pub fn service_uri(request_mailbox: &[u8; 16]) -> String {
    format!(
        "{INTRO_SERVICE_URI}/{}",
        crate::crypto::hex(request_mailbox)
    )
}

pub fn parse_service_uri(uri: &str) -> Option<[u8; 16]> {
    let h = uri.strip_prefix(INTRO_SERVICE_URI)?.strip_prefix('/')?;
    crate::crypto::unhex(h).ok()?.try_into().ok()
}

pub const TYPE_DESCRIPTOR: u64 = 11;
pub const TYPE_TICKET: u64 = 15;
pub const TYPE_PAYLOAD: u64 = 20;
pub const TYPE_BINDING: u64 = 21;

/// Largest sealed request payload a Core-issued ticket allows (`payload_limit_bytes`).
pub const PAYLOAD_LIMIT_BYTES: u64 = 32_768;
/// ChaCha20-Poly1305 tag appended to the HPKE ciphertext.
pub const AEAD_TAG_BYTES: u64 = 16;
/// Whole `MessageRequest` bytes accepted from the relay before any parsing.
pub const MAX_REQUEST_BYTES: usize = 40_000;
/// The descriptor `subject_hint` bound (bytes).
const SUBJECT_HINT_MAX: usize = 64;

/// Derive the invite's intro identity: `HKDF-SHA256(salt, intro_root, invite_id)` into the two signer seeds.
pub fn derive_signer(root: &[u8; 32], invite_id: &[u8; 16]) -> DeviceSigner {
    let mut out = [0u8; 64];
    hkdf_sha256(HKDF_SALT, root, invite_id, &mut out);
    let s = DeviceSigner {
        ed_seed: out[..32].try_into().expect("32"),
        mldsa_seed: out[32..].try_into().expect("32"),
    };
    zeroize::Zeroize::zeroize(&mut out);
    s
}

/// What the invite link commits to.
pub fn key_digest(intro_public_key: &[u8]) -> [u8; 32] {
    ds_hash(KEY_DIGEST_LABEL, intro_public_key)
}

fn hint(label: &str) -> Option<String> {
    let mut out = String::new();
    for c in label.chars() {
        if out.len() + c.len_utf8() > SUBJECT_HINT_MAX {
            break;
        }
        out.push(c);
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

pub fn request_key_struct(request_public_key: &[u8]) -> V {
    V::M(vec![
        (0, V::U(PROFILE_V1)),
        (1, V::B(request_public_key.to_vec())),
    ])
}

/// `ContactDescriptor` body (type 11). `request_key` is present on an inviter descriptor only.
pub fn descriptor_body(
    descriptor_id: &[u8; 16],
    label: &str,
    intro_service: &str,
    expires_at_s: u64,
    request_key: Option<&[u8]>,
) -> Vec<u8> {
    let mut m = vec![
        (0, V::U(TYPE_DESCRIPTOR)),
        (1, V::U(1)),
        (2, V::B(descriptor_id.to_vec())),
    ];
    if let Some(h) = hint(label) {
        m.push((3, V::T(h)));
    }
    m.push((6, V::A(vec![V::U(1)])));
    m.push((7, V::A(vec![V::M(vec![(0, V::T(intro_service.into()))])])));
    m.push((12, V::U(expires_at_s)));
    m.push((13, V::M(vec![(0, V::U(1))])));
    if let Some(pk) = request_key {
        m.push((15, request_key_struct(pk)));
    }
    cbor::encode(&V::M(m))
}

/// `IntroductionTicket` body (type 15), issued by the recipient's Core: single use, small payload, no attachments
/// or calls, and an empty `admission_proof` (no IntroGate adapter exists yet; ADR-0181 section 4).
pub fn ticket_body(
    invite_id: &[u8; 16],
    service_binding: &[u8; 32],
    expires_at_s: u64,
    nonce: &[u8; 16],
    request_public_key: &[u8],
) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(TYPE_TICKET)),
        (1, V::U(1)),
        (2, V::B(invite_id.to_vec())),
        (3, V::B(service_binding.to_vec())),
        (4, V::U(expires_at_s)),
        (5, V::U(1)),
        (6, V::U(PAYLOAD_LIMIT_BYTES)),
        (7, V::Bool(false)),
        (8, V::Bool(false)),
        (9, V::B(nonce.to_vec())),
        (11, request_key_struct(request_public_key)),
    ]))
}

pub struct RouteEntry<'a> {
    pub relay_uri: &'a str,
    pub capability: &'a [u8],
    pub expires_at_s: u64,
}

/// `MessageRequestPayload` body (type 20). No AccountID, ACS, credentials or chain.
pub fn payload_body(
    ticket_id: &[u8; 16],
    request_key_digest: &[u8; 32],
    sender_intro_identity: &[u8],
    sender_descriptor: &[u8],
    reply_route: &RouteEntry<'_>,
    reply_delivery_key: V,
) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(TYPE_PAYLOAD)),
        (1, V::U(1)),
        (2, V::B(ticket_id.to_vec())),
        (3, digest(request_key_digest)),
        (4, V::B(sender_intro_identity.to_vec())),
        (5, V::B(sender_descriptor.to_vec())),
        (
            8,
            V::M(vec![
                (0, V::T(reply_route.relay_uri.into())),
                (1, V::B(reply_route.capability.to_vec())),
                (2, V::U(0)),
                (4, V::U(reply_route.expires_at_s)),
            ]),
        ),
        (9, reply_delivery_key),
    ]))
}

/// Sign `body` with an intro identity (role 4). The frozen `signature.signer` field is a 16..32 byte key id, so it
/// carries the key digest; the verifier supplies the public key (the descriptor record, or the payload's
/// `sender_intro_identity`) and checks the digest against it.
pub fn sign_intro<R: RngCore + CryptoRng>(
    signer: &DeviceSigner,
    label: &str,
    body: Vec<u8>,
    rng: &mut R,
) -> Vec<u8> {
    let pk = signer.public();
    let id = key_digest(&pk);
    let s = signer.sign(
        &ds_sig(label, PROFILE_V1, ROLE_INTRO_IDENTITY, &id, &body),
        rng,
    );
    envelope(
        body,
        vec![signature_entry(&id, PROFILE_V1, ROLE_INTRO_IDENTITY, s)],
    )
}

/// A role-4 envelope that verified: the signed body.
pub struct Opened {
    pub body: V,
    pub body_bytes: Vec<u8>,
}

/// The body of a signed envelope, NOT yet verified: used only to read the public key the payload names before
/// checking the signature against it.
pub fn peek_body(bytes: &[u8]) -> Result<V, Error> {
    cbor::decode(cbor::decode(bytes)?.get(1)?.b()?)
}

/// Verify an envelope carrying exactly one intro-identity signature, made by the key `intro_public_key` under `label`.
pub fn open_intro(bytes: &[u8], label: &str, intro_public_key: &[u8]) -> Result<Opened, Error> {
    if intro_public_key.len() != sig::PK_LEN {
        return Err(Error::Binding("intro identity key length"));
    }
    let id = key_digest(intro_public_key);
    let body_bytes = open_envelope(
        bytes,
        label,
        PROFILE_V1,
        ROLE_INTRO_IDENTITY,
        &id,
        intro_public_key,
    )?;
    let body = cbor::decode(&body_bytes)?;
    Ok(Opened { body, body_bytes })
}

/// What a verified descriptor says (the fields Core uses).
pub struct DescriptorInfo {
    pub descriptor_id: [u8; 16],
    pub label: String,
    pub intro_service: String,
    pub expires_at_s: u64,
    pub request_key: Option<Vec<u8>>,
}

pub fn read_descriptor(body: &V) -> Result<DescriptorInfo, Error> {
    if body.get(0)?.u()? != TYPE_DESCRIPTOR || body.get(1)?.u()? != 1 {
        return Err(Error::Binding("not a ContactDescriptor v1"));
    }
    let request_key = match body.opt(15) {
        None => None,
        Some(rk) => {
            if rk.get(0)?.u()? != PROFILE_V1 {
                return Err(Error::Binding("request key profile"));
            }
            Some(rk.get(1)?.b()?.to_vec())
        }
    };
    Ok(DescriptorInfo {
        descriptor_id: body
            .get(2)?
            .b()?
            .try_into()
            .map_err(|_| Error::Binding("descriptor id"))?,
        label: body.opt(3).map(|t| t.t()).transpose()?.unwrap_or("").into(),
        intro_service: body
            .get(7)?
            .a()?
            .first()
            .ok_or(Error::Binding("no introduction service"))?
            .get(0)?
            .t()?
            .into(),
        expires_at_s: body.get(12)?.u()?,
        request_key,
    })
}

pub struct TicketInfo {
    pub ticket_id: [u8; 16],
    pub service_binding: [u8; 32],
    pub expires_at_s: u64,
    pub max_uses: u64,
    pub payload_limit: u64,
    pub request_key: Option<Vec<u8>>,
}

pub fn read_ticket(body: &V) -> Result<TicketInfo, Error> {
    if body.get(0)?.u()? != TYPE_TICKET || body.get(1)?.u()? != 1 {
        return Err(Error::Binding("not an IntroductionTicket v1"));
    }
    Ok(TicketInfo {
        ticket_id: body
            .get(2)?
            .b()?
            .try_into()
            .map_err(|_| Error::Binding("ticket id"))?,
        service_binding: body
            .get(3)?
            .b()?
            .try_into()
            .map_err(|_| Error::Binding("service binding"))?,
        expires_at_s: body.get(4)?.u()?,
        max_uses: body.get(5)?.u()?,
        payload_limit: body.get(6)?.u()?,
        request_key: match body.opt(11) {
            None => None,
            Some(rk) => Some(rk.get(1)?.b()?.to_vec()),
        },
    })
}

// ---------------------------------------------------------------------------------------------- binding

/// `IntroIdentityBinding` body (type 21).
pub fn binding_body(
    account_id: &[u8; 32],
    intro_identity: &[u8],
    request_key_digest: &[u8; 32],
    acs_generation: u64,
    acs_state_hash: &[u8; 32],
    issued_at_s: u64,
) -> Vec<u8> {
    cbor::encode(&V::M(vec![
        (0, V::U(TYPE_BINDING)),
        (1, V::U(1)),
        (2, V::B(account_id.to_vec())),
        (3, V::B(intro_identity.to_vec())),
        (4, digest(request_key_digest)),
        (5, V::U(acs_generation)),
        (6, digest(acs_state_hash)),
        (7, V::U(issued_at_s)),
    ]))
}

/// Sign a binding with a device of the account (role `device`, signer = DeviceID).
pub fn sign_binding(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    body: Vec<u8>,
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_BINDING_SIG, PROFILE_V1, ROLE_DEVICE, device_id, &body),
        rnd,
    );
    envelope(
        body,
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// What the verifier already knows and requires of a binding.
pub struct BindingExpect<'a> {
    /// The intro public key the counterpart used (committed by the link, or signed into the request).
    pub intro_identity: &'a [u8],
    pub request_key_digest: &'a [u8; 32],
}

/// A binding that verified against the account's ACS head.
pub struct VerifiedBinding {
    pub device_id: [u8; 16],
    pub credential: Credential,
}

/// Verify a signed `IntroIdentityBinding` against `head` (a verified ACS head) and the credentials supplied with it:
/// signed by a device the head lists as active and not quarantined, whose credential the head commits to; the
/// account, intro identity, request-key digest, ACS generation and ACS hash are exactly the expected ones.
pub fn verify_binding(
    bytes: &[u8],
    head: &Acs,
    credentials: &[Vec<u8>],
    expect: &BindingExpect<'_>,
) -> Result<VerifiedBinding, Error> {
    let v = cbor::decode(bytes)?;
    let sigs = v.get(2)?.a()?;
    let [s] = sigs else {
        return Err(Error::Binding("exactly one binding signature"));
    };
    let device_id: [u8; 16] = s
        .get(0)?
        .b()?
        .try_into()
        .map_err(|_| Error::Binding("binding signer is not a DeviceID"))?;
    let cred = credentials
        .iter()
        .filter_map(|c| Credential::from_body(c).ok())
        .find(|c| {
            c.device_id == device_id
                && c.account_id == head.account_id
                && head
                    .credentials
                    .iter()
                    .any(|(d, h)| *d == c.device_id && *h == c.hash())
        })
        .ok_or(Error::Binding("binding device credential not committed"))?;
    if !head.device_active(&device_id) {
        return Err(Error::Binding("binding device not active"));
    }
    let body_bytes = open_envelope(
        bytes,
        LBL_BINDING_SIG,
        PROFILE_V1,
        ROLE_DEVICE,
        &device_id,
        &cred.signing_pk,
    )?;
    let b = cbor::decode(&body_bytes)?;
    let d = |k: u64| -> Result<[u8; 32], Error> {
        let dg = b.get(k)?;
        if dg.get(0)?.u()? != 1 {
            return Err(Error::Binding("binding hash algorithm"));
        }
        dg.get(1)?.b32()
    };
    if b.get(0)?.u()? != TYPE_BINDING
        || b.get(1)?.u()? != 1
        || b.get(2)?.b()? != head.account_id
        || b.get(3)?.b()? != expect.intro_identity
        || d(4)? != *expect.request_key_digest
        || b.get(5)?.u()? != head.generation
        || d(6)? != head.hash()
    {
        return Err(Error::Binding(
            "binding does not match the expected context",
        ));
    }
    Ok(VerifiedBinding {
        device_id,
        credential: cred,
    })
}

/// A fresh 16-byte value.
pub fn rand16<R: RngCore + CryptoRng>(rng: &mut R) -> [u8; 16] {
    let mut b = [0u8; 16];
    rng.fill_bytes(&mut b);
    b
}

pub fn rand32<R: RngCore + CryptoRng>(rng: &mut R) -> [u8; 32] {
    arr32(rng)
}
