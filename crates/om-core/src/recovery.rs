// SPDX-License-Identifier: MPL-2.0
//! Recovery primitives (RECOVERY_AND_BACKUP_POLICY.md sections 10.2-10.4, ACCOUNT_CONTINUITY_AND_CONTROL.md section 4.1,
//! `spec/cddl/account.cddl`): the RecoveryRoot and the recovery signing key derived from it, the W0 `RecoveryPolicy`,
//! `RecoveryRequest` and `RecoveryVeto` objects (strict bounded decode, canonical re-encode check), their signatures
//! and the request digest a veto names.
//!
//! * The RecoveryRoot is 32 random bytes shown once and never stored by Core after display. The recovery signing key
//!   pair (Ed25519 + ML-DSA-65, the same composite as every durable signature) is derived from it by
//!   `OM-RECOVERY-AUTH-KEY-SEED-v1` (gap G1, operator-approved 2026-11-13): HKDF-SHA256 from the root, info = label ||
//!   recovery epoch (uint, big-endian 8 bytes), 64 bytes = 32-byte Ed25519 seed || 32-byte ML-DSA-65 seed. The policy
//!   method `commitment` is the composite public key. This derivation is new and has had no independent review.
//! * `recovery epoch` is the policy's `rotation_epoch` (absent counts as 0); a request and a veto are bound to it.
//! * The policy commits through `AccountControlState.recovery_policy_commitment` (key 10) =
//!   `DS-HASH(OM-RECOVERY-POLICY-HASH-v1, policy body)`; the policy carries no commitment of its own.
//!
//! Whether a recovery successor is authorized is [`crate::control::verify_recovery_successor`]'s job; this module only
//! builds and checks the objects.

use crate::crypto::Error;
use crate::crypto::PROFILE_V1;
use crate::crypto::cbor::{self, V};
use crate::crypto::kdf::hkdf_sha256;
use crate::crypto::rng::{CryptoRng, RngCore, arr32};
use crate::crypto::sig::{
    self, DeviceSigner, PK_LEN, ROLE_CONTROLLER, ROLE_RECOVERY, digest, ds_hash, ds_sig,
};
use std::collections::BTreeSet;

pub const LBL_AUTH_KEY_SEED: &str = "OM-RECOVERY-AUTH-KEY-SEED-v1";
pub const LBL_POLICY_HASH: &str = "OM-RECOVERY-POLICY-HASH-v1";
pub const LBL_REQUEST_SIG: &str = "OM-RECOVERY-REQUEST-SIG-v1";
pub const LBL_REQUEST_HASH: &str = "OM-RECOVERY-REQUEST-HASH-v1";
pub const LBL_VETO_SIG: &str = "OM-RECOVERY-VETO-SIG-v1";

/// `recovery_state` (om-numeric-fields.json).
pub const STATE_ON: u64 = 1;
pub const STATE_PENDING: u64 = 2;
pub const STATE_OFF_EXPLICIT: u64 = 3;
/// `recovery_method_kind`: the recovery key (a RecoveryRoot shown once).
pub const METHOD_RECOVERY_KEY: u64 = 2;
/// `recover_target`.
pub const TARGET_IDENTITY: u64 = 1;
pub const TARGET_HISTORY: u64 = 2;
pub const TARGET_PROVIDER_METADATA: u64 = 3;
pub const TARGET_RELATIONSHIPS: u64 = 4;

/// Default activation delay (Recommended profile, 72 h); the CDDL minimum is 24 h.
pub const DEFAULT_DELAY_SECONDS: u64 = 72 * 60 * 60;
pub const MIN_DELAY_SECONDS: u64 = 86_400;
pub const MAX_DELAY_SECONDS: u64 = 31_536_000;

const TYPE_POLICY: u64 = 3;
const TYPE_REQUEST: u64 = 4;
const TYPE_VETO: u64 = 5;

/// Hard input bounds, checked before any parsing: 8 methods of at most 2592 key bytes, one composite signature.
pub const MAX_POLICY_BYTES: usize = 24 * 1024;
pub const MAX_REQUEST_BYTES: usize = 512;
pub const MAX_VETO_BYTES: usize = 256;
const MAX_SIGNED_BYTES: usize = 8 * 1024;

// ------------------------------------------------------------------------------------------------ RecoveryRoot

/// The 32-byte RecoveryRoot. Wiped on drop, never printed, never cloned.
pub struct RecoveryRoot([u8; 32]);

impl core::fmt::Debug for RecoveryRoot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RecoveryRoot(<redacted>)")
    }
}

impl Drop for RecoveryRoot {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl RecoveryRoot {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        RecoveryRoot(arr32(rng))
    }

    pub fn from_bytes(b: [u8; 32]) -> Self {
        RecoveryRoot(b)
    }

    /// The raw root, for the display-once transcription only.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    /// The recovery signing key of `epoch` (OM-RECOVERY-AUTH-KEY-SEED-v1, gap G1).
    pub fn auth_signer(&self, epoch: u64) -> DeviceSigner {
        let mut info = LBL_AUTH_KEY_SEED.as_bytes().to_vec();
        info.extend_from_slice(&epoch.to_be_bytes());
        let mut out = [0u8; 64];
        hkdf_sha256(&[], &self.0, &info, &mut out);
        let s = DeviceSigner {
            ed_seed: out[..32].try_into().expect("32"),
            mldsa_seed: out[32..].try_into().expect("32"),
        };
        zeroize::Zeroize::zeroize(&mut out);
        s
    }

    /// The composite public key the policy method commits to for `epoch`.
    pub fn auth_public_key(&self, epoch: u64) -> Vec<u8> {
        self.auth_signer(epoch).public()
    }
}

// ------------------------------------------------------------------------------------------------ RecoveryPolicy

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryMethod {
    pub kind: u64,
    pub commitment: Vec<u8>,
    pub guardian_threshold: Option<u64>,
    pub guardian_count: Option<u64>,
}

/// `RecoveryPolicy` (type 3). `authorization_rule` (vote_weight 1, immediate_requires_controller_cosign) and
/// `activation_policy.veto_by` (any live controller) have one frozen value each and are not fields here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryPolicy {
    pub state: u64,
    pub methods: Vec<RecoveryMethod>,
    pub recovers: BTreeSet<u64>,
    pub delay_seconds: u64,
    pub rotate_custody_on_veto: bool,
    pub notify_contacts_on_completion: bool,
    pub rotation_epoch: Option<u64>,
}

impl RecoveryPolicy {
    /// The default policy of a new identity (state on, recovery key, 72 h delay) for `epoch`.
    pub fn default_for(auth_public_key: Vec<u8>, epoch: u64) -> Self {
        RecoveryPolicy {
            state: STATE_ON,
            methods: vec![RecoveryMethod {
                kind: METHOD_RECOVERY_KEY,
                commitment: auth_public_key,
                guardian_threshold: None,
                guardian_count: None,
            }],
            recovers: BTreeSet::from([
                TARGET_IDENTITY,
                TARGET_PROVIDER_METADATA,
                TARGET_RELATIONSHIPS,
            ]),
            delay_seconds: DEFAULT_DELAY_SECONDS,
            rotate_custody_on_veto: false,
            notify_contacts_on_completion: false,
            rotation_epoch: Some(epoch),
        }
    }

    /// The recovery epoch this policy is at (`rotation_epoch`, absent = 0).
    pub fn epoch(&self) -> u64 {
        self.rotation_epoch.unwrap_or(0)
    }

    /// The composite public key of the single `recovery_key` method, if there is exactly one.
    pub fn recovery_key(&self) -> Option<&[u8]> {
        let mut it = self
            .methods
            .iter()
            .filter(|m| m.kind == METHOD_RECOVERY_KEY && m.commitment.len() == PK_LEN);
        let first = it.next()?;
        it.next().is_none().then_some(first.commitment.as_slice())
    }

    pub fn body(&self) -> Vec<u8> {
        let methods = self
            .methods
            .iter()
            .map(|m| {
                let mut f = vec![(0, V::U(m.kind)), (1, V::B(m.commitment.clone()))];
                if let Some(t) = m.guardian_threshold {
                    f.push((2, V::U(t)));
                }
                if let Some(c) = m.guardian_count {
                    f.push((3, V::U(c)));
                }
                V::M(f)
            })
            .collect();
        let mut act = vec![(0, V::U(self.delay_seconds)), (1, V::U(1))];
        if self.rotate_custody_on_veto {
            act.push((2, V::Bool(true)));
        }
        if self.notify_contacts_on_completion {
            act.push((3, V::Bool(true)));
        }
        let mut m = vec![
            (0, V::U(TYPE_POLICY)),
            (1, V::U(1)),
            (2, V::U(self.state)),
            (3, V::A(methods)),
            (4, V::M(vec![(0, V::U(1)), (1, V::Bool(true))])),
            (5, V::A(self.recovers.iter().map(|t| V::U(*t)).collect())),
            (6, V::M(act)),
        ];
        if let Some(e) = self.rotation_epoch {
            m.push((7, V::U(e)));
        }
        cbor::encode(&V::M(m))
    }

    /// The value `AccountControlState.recovery_policy_commitment` holds.
    pub fn hash(&self) -> [u8; 32] {
        ds_hash(LBL_POLICY_HASH, &self.body())
    }

    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        if b.len() > MAX_POLICY_BYTES {
            return Err(Error::Decode("recovery policy: too large"));
        }
        let v = cbor::decode(b)?;
        check_head(&v, TYPE_POLICY, &[2, 3, 4, 5, 6, 7])?;
        let state = v.get(2)?.u()?;
        if !(STATE_ON..=STATE_OFF_EXPLICIT).contains(&state) {
            return Err(Error::Binding("recovery policy: state"));
        }
        let ms = v.get(3)?.a()?;
        if ms.len() > 8 {
            return Err(Error::Binding("recovery policy: too many methods"));
        }
        let mut methods = vec![];
        for m in ms {
            for (k, _) in m.m()? {
                if !matches!(*k, 0..=3) {
                    return Err(Error::Binding("recovery method: unknown field"));
                }
            }
            let kind = m.get(0)?.u()?;
            let commitment = m.get(1)?.b()?.to_vec();
            if !(1..=6).contains(&kind) || !(32..=2592).contains(&commitment.len()) {
                return Err(Error::Binding("recovery method: kind or commitment size"));
            }
            let small = |k: u64| -> Result<Option<u64>, Error> {
                m.opt(k)
                    .map(|x| {
                        let n = x.u()?;
                        if (1..=255).contains(&n) {
                            Ok(n)
                        } else {
                            Err(Error::Binding("recovery method: guardian bound"))
                        }
                    })
                    .transpose()
            };
            methods.push(RecoveryMethod {
                kind,
                commitment,
                guardian_threshold: small(2)?,
                guardian_count: small(3)?,
            });
        }
        let rule = v.get(4)?;
        if rule.m()?.len() != 2 || rule.get(0)?.u()? != 1 || !is_true(rule.get(1)?)? {
            return Err(Error::Binding("recovery policy: authorization rule"));
        }
        let rs = v.get(5)?.a()?;
        if rs.is_empty() || rs.len() > 4 {
            return Err(Error::Binding("recovery policy: recovers size"));
        }
        let mut recovers = BTreeSet::new();
        for r in rs {
            let t = r.u()?;
            if !(1..=4).contains(&t) || !recovers.insert(t) {
                return Err(Error::Binding("recovery policy: recovers value"));
            }
        }
        let act = v.get(6)?;
        for (k, _) in act.m()? {
            if !matches!(*k, 0..=3) {
                return Err(Error::Binding("recovery activation: unknown field"));
            }
        }
        let delay_seconds = act.get(0)?.u()?;
        if !(MIN_DELAY_SECONDS..=MAX_DELAY_SECONDS).contains(&delay_seconds)
            || act.get(1)?.u()? != 1
        {
            return Err(Error::Binding("recovery activation: delay or veto_by"));
        }
        let flag = |k: u64| -> Result<bool, Error> {
            act.opt(k)
                .map(is_true)
                .transpose()
                .map(|f| f.unwrap_or(false))
        };
        let p = RecoveryPolicy {
            state,
            methods,
            recovers,
            delay_seconds,
            rotate_custody_on_veto: flag(2)?,
            notify_contacts_on_completion: flag(3)?,
            rotation_epoch: v.opt(7).map(|e| e.u()).transpose()?,
        };
        // The decoder already rejects non-shortest heads and unsorted maps; this also pins array/set order.
        if p.body() != b {
            return Err(Error::Decode("recovery policy: not canonical"));
        }
        Ok(p)
    }
}

fn is_true(v: &V) -> Result<bool, Error> {
    match v {
        V::Bool(true) => Ok(true),
        _ => Err(Error::Binding("recovery: flag must be true")),
    }
}

/// Type/version header, and no key outside `allowed` (key 23 extensions are not implemented: reject, like the ACS).
fn check_head(v: &V, ty: u64, allowed: &[u64]) -> Result<(), Error> {
    if v.get(0)?.u()? != ty || v.get(1)?.u()? != 1 {
        return Err(Error::Binding("recovery: wrong object type or version"));
    }
    for (k, _) in v.m()? {
        if *k > 1 && !allowed.contains(k) {
            return Err(Error::Binding("recovery: unsupported field or extension"));
        }
    }
    Ok(())
}

fn digest_of(v: &V) -> Result<[u8; 32], Error> {
    if v.get(0)?.u()? != 1 {
        return Err(Error::Binding("recovery: digest algorithm"));
    }
    v.get(1)?.b32()
}

// ------------------------------------------------------------------------------------------ RecoveryRequest

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequest {
    pub account_id: [u8; 32],
    pub recovery_epoch: u64,
    pub new_device_id: [u8; 16],
    /// `OM-DEVICE-CREDENTIAL-HASH-v1` of the target device's DeviceCredential.
    pub new_device_credential: [u8; 32],
    pub parent_state_hash: [u8; 32],
    pub request_nonce: [u8; 16],
}

impl RecoveryRequest {
    pub fn body(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::U(TYPE_REQUEST)),
            (1, V::U(1)),
            (2, V::B(self.account_id.to_vec())),
            (3, V::U(self.recovery_epoch)),
            (4, V::B(self.new_device_id.to_vec())),
            (5, digest(&self.new_device_credential)),
            (6, digest(&self.parent_state_hash)),
            (7, V::B(self.request_nonce.to_vec())),
        ]))
    }

    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        if b.len() > MAX_REQUEST_BYTES {
            return Err(Error::Decode("recovery request: too large"));
        }
        let v = cbor::decode(b)?;
        check_head(&v, TYPE_REQUEST, &[2, 3, 4, 5, 6, 7])?;
        let fixed = |k: u64| -> Result<[u8; 16], Error> {
            v.get(k)?
                .b()?
                .try_into()
                .map_err(|_| Error::Binding("recovery request: id size"))
        };
        let r = RecoveryRequest {
            account_id: v.get(2)?.b32()?,
            recovery_epoch: v.get(3)?.u()?,
            new_device_id: fixed(4)?,
            new_device_credential: digest_of(v.get(5)?)?,
            parent_state_hash: digest_of(v.get(6)?)?,
            request_nonce: fixed(7)?,
        };
        if r.body() != b {
            return Err(Error::Decode("recovery request: not canonical"));
        }
        Ok(r)
    }

    /// `OM-RECOVERY-REQUEST-HASH-v1` of the body: what a `RecoveryVeto` names.
    pub fn digest(&self) -> [u8; 32] {
        ds_hash(LBL_REQUEST_HASH, &self.body())
    }
}

/// Sign a request with the recovery key (role `recovery`; the signer field is the AccountID).
pub fn sign_request<R: RngCore + CryptoRng>(
    req: &RecoveryRequest,
    signer: &DeviceSigner,
    rng: &mut R,
) -> Vec<u8> {
    let body = req.body();
    let s = signer.sign_with(
        &ds_sig(
            LBL_REQUEST_SIG,
            PROFILE_V1,
            ROLE_RECOVERY,
            &req.account_id,
            &body,
        ),
        arr32(rng),
    );
    sig::envelope(
        body,
        vec![sig::signature_entry(
            &req.account_id,
            PROFILE_V1,
            ROLE_RECOVERY,
            s,
        )],
    )
}

fn body_of(signed: &[u8], max: usize) -> Result<Vec<u8>, Error> {
    if signed.len() > max.max(MAX_SIGNED_BYTES) {
        return Err(Error::Decode("recovery: signed object too large"));
    }
    Ok(cbor::decode(signed)?.get(1)?.b()?.to_vec())
}

/// Open a `signed-RecoveryRequest`: exactly one signature, role `recovery`, valid under `recovery_key` (the policy's
/// method key). Returns the request; whether it fits the parent state is the verifier's job.
pub fn open_request(signed: &[u8], recovery_key: &[u8]) -> Result<RecoveryRequest, Error> {
    let req = RecoveryRequest::from_body(&body_of(signed, MAX_SIGNED_BYTES)?)?;
    sig::open_envelope(
        signed,
        LBL_REQUEST_SIG,
        PROFILE_V1,
        ROLE_RECOVERY,
        &req.account_id,
        recovery_key,
    )?;
    Ok(req)
}

// -------------------------------------------------------------------------------------------- RecoveryVeto

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryVeto {
    pub account_id: [u8; 32],
    pub recovery_epoch: u64,
    pub request_digest: [u8; 32],
}

impl RecoveryVeto {
    pub fn body(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::U(TYPE_VETO)),
            (1, V::U(1)),
            (2, V::B(self.account_id.to_vec())),
            (3, V::U(self.recovery_epoch)),
            (4, digest(&self.request_digest)),
        ]))
    }

    pub fn from_body(b: &[u8]) -> Result<Self, Error> {
        if b.len() > MAX_VETO_BYTES {
            return Err(Error::Decode("recovery veto: too large"));
        }
        let v = cbor::decode(b)?;
        check_head(&v, TYPE_VETO, &[2, 3, 4])?;
        let r = RecoveryVeto {
            account_id: v.get(2)?.b32()?,
            recovery_epoch: v.get(3)?.u()?,
            request_digest: digest_of(v.get(4)?)?,
        };
        if r.body() != b {
            return Err(Error::Decode("recovery veto: not canonical"));
        }
        Ok(r)
    }
}

/// Sign a veto as controller `device_id` (role `controller`).
pub fn sign_veto<R: RngCore + CryptoRng>(
    veto: &RecoveryVeto,
    device_id: &[u8; 16],
    signer: &DeviceSigner,
    rng: &mut R,
) -> Vec<u8> {
    let body = veto.body();
    let s = signer.sign_with(
        &ds_sig(LBL_VETO_SIG, PROFILE_V1, ROLE_CONTROLLER, device_id, &body),
        arr32(rng),
    );
    sig::envelope(
        body,
        vec![sig::signature_entry(
            device_id,
            PROFILE_V1,
            ROLE_CONTROLLER,
            s,
        )],
    )
}

/// The controller a `signed-RecoveryVeto` claims to be signed by (not yet verified).
pub fn veto_signer(signed: &[u8]) -> Result<[u8; 16], Error> {
    if signed.len() > MAX_SIGNED_BYTES {
        return Err(Error::Decode("recovery veto: signed object too large"));
    }
    let v = cbor::decode(signed)?;
    let sigs = v.get(2)?.a()?;
    if sigs.len() != 1 {
        return Err(Error::Binding("recovery veto: exactly one signature"));
    }
    sigs[0]
        .get(0)?
        .b()?
        .try_into()
        .map_err(|_| Error::Binding("recovery veto: signer id"))
}

/// Open a `signed-RecoveryVeto` signed by controller `device_id` whose credential key is `controller_key`.
pub fn open_veto(
    signed: &[u8],
    device_id: &[u8; 16],
    controller_key: &[u8],
) -> Result<RecoveryVeto, Error> {
    let veto = RecoveryVeto::from_body(&body_of(signed, MAX_SIGNED_BYTES)?)?;
    sig::open_envelope(
        signed,
        LBL_VETO_SIG,
        PROFILE_V1,
        ROLE_CONTROLLER,
        device_id,
        controller_key,
    )?;
    Ok(veto)
}

// ------------------------------------------------------------------------------------------ successor carrier

/// What the recovering device hands a verifier alongside a `RecoveryRequest` to complete it: the recovery-signed
/// successor `AccountControlState`, the rotated policy body it commits to, and the new device's `DeviceCredential`
/// body. No design document or CDDL names a carrier for these (the request holds only the credential's digest), so
/// this is Core's own in-memory shape until the Home work (Slice 4) decides; it is not a wire object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccessorBundle {
    pub child: Vec<u8>,
    pub next_policy: Vec<u8>,
    pub credential: Vec<u8>,
}

// ----------------------------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::rng::insecure_test_rng::InsecureTestRng;

    fn root() -> RecoveryRoot {
        RecoveryRoot::from_bytes([7u8; 32])
    }

    fn request() -> RecoveryRequest {
        RecoveryRequest {
            account_id: [1; 32],
            recovery_epoch: 0,
            new_device_id: [2; 16],
            new_device_credential: [3; 32],
            parent_state_hash: [4; 32],
            request_nonce: [5; 16],
        }
    }

    #[test]
    fn root_is_redacted_and_the_derivation_is_domain_and_epoch_separated() {
        let r = root();
        assert_eq!(format!("{r:?}"), "RecoveryRoot(<redacted>)");
        let (a0, a1) = (r.auth_public_key(0), r.auth_public_key(1));
        assert_eq!(a0.len(), PK_LEN);
        assert_ne!(a0, a1, "a new epoch is a new key pair");
        assert_eq!(a0, root().auth_public_key(0), "deterministic");
        assert_ne!(
            a0,
            RecoveryRoot::from_bytes([8u8; 32]).auth_public_key(0),
            "a different root"
        );
        // The seeds are exactly HKDF-SHA256(root, info = label || epoch_be8), 32 || 32 (G1).
        let mut info = LBL_AUTH_KEY_SEED.as_bytes().to_vec();
        info.extend_from_slice(&0u64.to_be_bytes());
        let mut out = [0u8; 64];
        hkdf_sha256(&[], &[7u8; 32], &info, &mut out);
        let s = r.auth_signer(0);
        assert_eq!(
            (&s.ed_seed[..], &s.mldsa_seed[..]),
            (&out[..32], &out[32..])
        );
        // And it is not the device key derivation or any other label's output.
        let mut other = [0u8; 64];
        hkdf_sha256(&[], &[7u8; 32], b"OM-RECOVERY-RECORD-KEY-v1", &mut other);
        assert_ne!(&out[..], &other[..]);
    }

    #[test]
    fn default_policy_golden_shape_and_hash() {
        let r = root();
        let p = RecoveryPolicy::default_for(r.auth_public_key(0), 0);
        let body = p.body();
        // Head of the canonical body: {0:3, 1:1, 2:1, 3:[{0:2, 1:h'<1984>'}], ...}
        assert_eq!(
            &body[..8],
            &[0xa8, 0x00, 0x03, 0x01, 0x01, 0x02, 0x01, 0x03]
        );
        assert_eq!(RecoveryPolicy::from_body(&body).unwrap(), p);
        assert_eq!(p.hash(), ds_hash(LBL_POLICY_HASH, &body));
        assert_eq!(p.recovery_key(), Some(&p.methods[0].commitment[..]));
        assert_eq!(p.delay_seconds, 259_200);
        assert_eq!(
            p.recovers,
            BTreeSet::from([
                TARGET_IDENTITY,
                TARGET_PROVIDER_METADATA,
                TARGET_RELATIONSHIPS
            ])
        );
        // A different epoch or key is a different commitment.
        assert_ne!(
            p.hash(),
            RecoveryPolicy::default_for(r.auth_public_key(1), 1).hash()
        );
        let reg = om_codec::Registry::load();
        assert!(reg.check_body("RecoveryPolicy", &body).is_ok());
    }

    #[test]
    fn policy_decode_is_strict() {
        let p = RecoveryPolicy::default_for(root().auth_public_key(0), 0);
        let good = p.body();
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(
            RecoveryPolicy::from_body(&trailing).is_err(),
            "trailing bytes"
        );
        assert!(RecoveryPolicy::from_body(&good[..good.len() - 1]).is_err());
        assert!(RecoveryPolicy::from_body(&vec![0u8; MAX_POLICY_BYTES + 1]).is_err());
        let bad = |f: &dyn Fn(&mut RecoveryPolicy)| {
            let mut q = p.clone();
            f(&mut q);
            RecoveryPolicy::from_body(&q.body()).is_err()
        };
        assert!(bad(&|q| q.delay_seconds = 86_399), "below the 24 h floor");
        assert!(bad(&|q| q.delay_seconds = MAX_DELAY_SECONDS + 1));
        assert!(bad(&|q| q.state = 4));
        assert!(bad(&|q| q.methods[0].commitment = vec![0; 31]));
        assert!(bad(&|q| q.methods[0].commitment = vec![0; 2593]));
        assert!(bad(&|q| q.methods[0].kind = 7));
        assert!(bad(&|q| q.recovers.clear()));
        assert!(bad(&|q| {
            q.recovers.insert(5);
        }));
        assert!(bad(&|q| q.methods = vec![q.methods[0].clone(); 9]));
        // Non-canonical set order (descending recovers) is rejected by the re-encode check.
        let v = cbor::decode(&good).unwrap();
        let V::M(mut m) = v else { panic!() };
        m[5].1 = V::A(vec![V::U(4), V::U(3), V::U(1)]);
        let unsorted = {
            // hand-encode: the encoder keeps array order, only maps are sorted
            cbor::encode(&V::M(m))
        };
        assert!(RecoveryPolicy::from_body(&unsorted).is_err(), "set order");
        // An unknown key (including key 23 extensions) is refused.
        let V::M(mut m) = cbor::decode(&good).unwrap() else {
            panic!()
        };
        m.push((23, V::TM(vec![])));
        assert!(RecoveryPolicy::from_body(&cbor::encode(&V::M(m))).is_err());
        // A self-commitment (reserved key 8) is refused: the policy carries no commitment.
        let V::M(mut m) = cbor::decode(&good).unwrap() else {
            panic!()
        };
        m.push((8, digest(&[0; 32])));
        assert!(RecoveryPolicy::from_body(&cbor::encode(&V::M(m))).is_err());
        // Wrong object type.
        assert!(RecoveryPolicy::from_body(&request().body()).is_err());
    }

    #[test]
    fn request_roundtrip_signature_and_digest() {
        let mut rng = InsecureTestRng::from_label(b"recovery-request");
        let r = root();
        let (signer, pk) = (r.auth_signer(0), r.auth_public_key(0));
        let req = request();
        let signed = sign_request(&req, &signer, &mut rng);
        assert_eq!(open_request(&signed, &pk).unwrap(), req);
        assert_eq!(req.digest(), ds_hash(LBL_REQUEST_HASH, &req.body()));
        let reg = om_codec::Registry::load();
        assert!(reg.check_body("RecoveryRequest", &req.body()).is_ok());
        // Any field change changes the digest a veto names.
        let mut other = req.clone();
        other.recovery_epoch = 1;
        assert_ne!(other.digest(), req.digest());
        // Wrong key, wrong epoch key.
        assert!(open_request(&signed, &r.auth_public_key(1)).is_err());
        assert!(
            open_request(
                &signed,
                &RecoveryRoot::from_bytes([9; 32]).auth_public_key(0)
            )
            .is_err()
        );
    }

    #[test]
    fn request_rejects_wrong_role_label_trailing_and_noncanonical() {
        let mut rng = InsecureTestRng::from_label(b"recovery-request-neg");
        let r = root();
        let (signer, pk) = (r.auth_signer(0), r.auth_public_key(0));
        let req = request();
        let body = req.body();
        let mut sign_as = |label: &str, role: u64, signer_id: &[u8]| {
            let s = signer.sign_with(
                &ds_sig(label, PROFILE_V1, role, signer_id, &body),
                arr32(&mut rng),
            );
            sig::envelope(
                body.clone(),
                vec![sig::signature_entry(signer_id, PROFILE_V1, role, s)],
            )
        };
        assert!(
            open_request(
                &sign_as(LBL_REQUEST_SIG, ROLE_RECOVERY, &req.account_id),
                &pk
            )
            .is_ok()
        );
        // Role controller (1) instead of recovery (2): the entry does not match.
        assert!(
            open_request(
                &sign_as(LBL_REQUEST_SIG, ROLE_CONTROLLER, &req.account_id),
                &pk
            )
            .is_err()
        );
        // Signature made under a sibling label (the veto label) does not verify as a request.
        assert!(open_request(&sign_as(LBL_VETO_SIG, ROLE_RECOVERY, &req.account_id), &pk).is_err());
        assert!(
            open_request(
                &sign_as("OM-ACS-SIG-v1", ROLE_RECOVERY, &req.account_id),
                &pk
            )
            .is_err()
        );
        // A different signer id than the AccountID in the body.
        assert!(open_request(&sign_as(LBL_REQUEST_SIG, ROLE_RECOVERY, &[9; 32]), &pk).is_err());
        // Trailing bytes after the envelope; trailing bytes inside the body.
        let mut signed = sign_as(LBL_REQUEST_SIG, ROLE_RECOVERY, &req.account_id);
        signed.push(0);
        assert!(open_request(&signed, &pk).is_err());
        let mut long_body = body.clone();
        long_body.push(0);
        assert!(RecoveryRequest::from_body(&long_body).is_err());
        // Non-canonical: a non-shortest integer head for the epoch (0 as 0x1800).
        let mut nc = body.clone();
        let pos = nc.windows(2).position(|w| w == [0x03, 0x00]).unwrap();
        nc.splice(pos + 1..pos + 2, [0x18, 0x00]);
        assert!(RecoveryRequest::from_body(&nc).is_err());
        // Two signatures are refused, and so is an oversize object.
        let dup = sig::envelope(
            body.clone(),
            vec![
                sig::signature_entry(&req.account_id, PROFILE_V1, ROLE_RECOVERY, vec![0; 3373]),
                sig::signature_entry(&req.account_id, PROFILE_V1, ROLE_RECOVERY, vec![0; 3373]),
            ],
        );
        assert!(open_request(&dup, &pk).is_err());
        assert!(open_request(&vec![0u8; 64 * 1024], &pk).is_err());
    }

    #[test]
    fn veto_roundtrip_and_negatives() {
        let mut rng = InsecureTestRng::from_label(b"recovery-veto");
        let dev = DeviceSigner::generate(&mut rng);
        let id = [6u8; 16];
        let req = request();
        let veto = RecoveryVeto {
            account_id: req.account_id,
            recovery_epoch: 0,
            request_digest: req.digest(),
        };
        let signed = sign_veto(&veto, &id, &dev, &mut rng);
        assert_eq!(veto_signer(&signed).unwrap(), id);
        assert_eq!(open_veto(&signed, &id, &dev.public()).unwrap(), veto);
        let reg = om_codec::Registry::load();
        assert!(reg.check_body("RecoveryVeto", &veto.body()).is_ok());
        // Another controller's key, another device id, a recovery-role signature all fail.
        let other = DeviceSigner::generate(&mut rng);
        assert!(open_veto(&signed, &id, &other.public()).is_err());
        assert!(open_veto(&signed, &[7u8; 16], &dev.public()).is_err());
        let body = veto.body();
        let s = dev.sign_with(
            &ds_sig(LBL_VETO_SIG, PROFILE_V1, ROLE_RECOVERY, &id, &body),
            arr32(&mut rng),
        );
        let wrong_role = sig::envelope(
            body.clone(),
            vec![sig::signature_entry(&id, PROFILE_V1, ROLE_RECOVERY, s)],
        );
        assert!(open_veto(&wrong_role, &id, &dev.public()).is_err());
        // Signed under the request label: not a veto.
        let s = dev.sign_with(
            &ds_sig(LBL_REQUEST_SIG, PROFILE_V1, ROLE_CONTROLLER, &id, &body),
            arr32(&mut rng),
        );
        let wrong_label = sig::envelope(
            body.clone(),
            vec![sig::signature_entry(&id, PROFILE_V1, ROLE_CONTROLLER, s)],
        );
        assert!(open_veto(&wrong_label, &id, &dev.public()).is_err());
        let mut trailing = signed.clone();
        trailing.push(0);
        assert!(open_veto(&trailing, &id, &dev.public()).is_err());
        let mut long_body = body;
        long_body.push(0);
        assert!(RecoveryVeto::from_body(&long_body).is_err());
        assert!(veto_signer(&[0xa0]).is_err());
    }
}
