// SPDX-License-Identifier: MPL-2.0
//! `GroupCommitBinding` (object 30) construction, signing and verification (ADR-0176, ADR-0173, ADR-0163).
//!
//! The binding ties one MLS Commit to one OM group-state transition: it names the parent `(state hash, state version,
//! MLS epoch)`, the intent hash, the exact Commit's hash and the resulting state hash and epoch. It is signed by the
//! committing device (role `device`) with the existing composite DeviceIdentity signer over
//! `DS-SIG("OM-GROUP-COMMIT-BINDING-SIG-v1", profile, device, device_id, body)`.
//!
//! Verification needs the committer's device public key. Where that key comes from (the account's ACS) is Core's
//! decision and is passed in; nothing here trusts a key found inside the message.

use super::{Binding, GroupError, R, digest_value, err, map};
use crate::crypto::PROFILE_V1;
use crate::crypto::sig::{
    DeviceSigner, ROLE_DEVICE, ds_hash, ds_sig, envelope, open_envelope, signature_entry,
};
use om_codec::{Value, encode};

/// `DS-SIG` label of a signed `GroupCommitBinding`.
pub const LBL_BINDING_SIG: &str = "OM-GROUP-COMMIT-BINDING-SIG-v1";
/// `DS-HASH` label over the exact MLS Commit message bytes (`mls_commit_hash`).
pub const LBL_MLS_COMMIT_HASH: &str = "OM-GROUP-MLS-COMMIT-HASH-v1";

/// `binding_kind` values.
pub const KIND_COORDINATOR_SEQUENCED: u64 = 1;
pub const KIND_DIRECT_REDUCTION: u64 = 2;
pub const KIND_RECOVERY: u64 = 3;

/// `mls_commit_hash` of a Commit.
pub fn mls_commit_hash(commit: &[u8]) -> [u8; 32] {
    ds_hash(LBL_MLS_COMMIT_HASH, commit)
}

/// Everything a binding body carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingParams {
    pub conversation_id: Vec<u8>,
    pub binding_kind: u64,
    pub reducing: bool,
    pub previous_state_hash: [u8; 32],
    pub previous_state_version: u64,
    pub previous_mls_epoch: u64,
    pub resulting_mls_epoch: u64,
    /// Absent only for a recovery binding.
    pub intent_hash: Option<[u8; 32]>,
    pub mls_commit_hash: [u8; 32],
    pub resulting_state_hash: [u8; 32],
    pub committer_device: Vec<u8>,
    /// Recovery only: the re-added devices (a set, written sorted).
    pub readded_devices: Vec<Vec<u8>>,
}

/// The canonical body of the binding. It is checked against the registry-driven codec and the three ADR-0176
/// section 3 binding rules before it is returned, so a body that comes out is a valid one.
pub fn binding_body(p: &BindingParams) -> R<Vec<u8>> {
    let mut m = vec![
        (0, Value::Uint(30)),
        (1, Value::Uint(1)),
        (2, Value::Bytes(p.conversation_id.clone())),
        (3, Value::Uint(p.binding_kind)),
        (4, Value::Bool(p.reducing)),
        (5, digest_value(&p.previous_state_hash)),
        (6, Value::Uint(p.previous_state_version)),
        (7, Value::Uint(p.previous_mls_epoch)),
        (8, Value::Uint(p.resulting_mls_epoch)),
    ];
    if let Some(h) = &p.intent_hash {
        m.push((9, digest_value(h)));
    }
    m.push((10, digest_value(&p.mls_commit_hash)));
    m.push((11, digest_value(&p.resulting_state_hash)));
    m.push((12, Value::Bytes(p.committer_device.clone())));
    if !p.readded_devices.is_empty() {
        let mut d = p.readded_devices.clone();
        d.sort();
        m.push((13, Value::Array(d.into_iter().map(Value::Bytes).collect())));
    }
    let body = encode(&map(m));
    Binding::decode(&body)?; // codec + validate_binding
    Ok(body)
}

/// The signed binding: a `signed-GroupCommitBinding` with exactly one device signature by `device_id`.
///
/// `rnd` is the FIPS 204 hedging input, drawn from Core's RNG by the caller.
pub fn sign_binding(
    signer: &DeviceSigner,
    device_id: &[u8],
    body: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_BINDING_SIG, PROFILE_V1, ROLE_DEVICE, device_id, body),
        rnd,
    );
    envelope(
        body.to_vec(),
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// A verified binding with the exact body bytes it was signed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedBinding {
    pub binding: Binding,
    pub body: Vec<u8>,
}

/// Verify a signed binding: exactly one signature, by `committer_pk` as the device named in the body, composite
/// signature valid, body strictly canonical and consistent. Failure is `bad_signature` or the codec/validator verdict.
pub fn verify_binding(signed: &[u8], committer_pk: &[u8]) -> R<VerifiedBinding> {
    // The envelope names its signer; the body must name the same device as committer.
    let claimed = claimed_signer(signed).ok_or(GroupError("malformed"))?;
    let body = open_envelope(
        signed,
        LBL_BINDING_SIG,
        PROFILE_V1,
        ROLE_DEVICE,
        &claimed,
        committer_pk,
    )
    .map_err(|_| GroupError("bad_signature"))?;
    let binding = Binding::decode(&body)?;
    if binding.committer_device != claimed {
        return err("signer_not_committer");
    }
    Ok(VerifiedBinding { binding, body })
}

/// The device that claims to have signed `bytes`, read without verifying (to find its credential). Never trusted alone.
fn claimed_signer(bytes: &[u8]) -> Option<Vec<u8>> {
    let v = crate::crypto::cbor::decode(bytes).ok()?;
    let sigs = v.get(2).ok()?.a().ok()?;
    let [s] = sigs else { return None };
    Some(s.get(0).ok()?.b().ok()?.to_vec())
}
