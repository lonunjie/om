//! INSECURE TEST-ONLY stand-in for OM canonical signed objects. OM's real per-object CDDL/signature registries are
//! V-005 work and are not frozen; om-archive-v1 treats canonical objects as opaque exact bytes plus a declared type,
//! and delegates verification to an `ObjectVerifier`. This module supplies Ed25519-signed deterministic-CBOR test
//! objects whose keys are derived from public labels, so the archive's preservation/verification/stale-authority
//! behaviour can be exercised. Never use these keys or this envelope outside tests.

use crate::cbor::{self, map, Value};
use crate::crypto::sha256;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

pub const DOMAIN: &[u8] = b"OM-TEST-OBJECT-v0\0";

/// Deterministic signing key from a public label. INSECURE: anyone can recompute it.
pub fn insecure_test_signing_key(label: &str) -> SigningKey {
    SigningKey::from_bytes(&sha256(&[b"OM insecure test key ", label.as_bytes()]))
}

pub fn insecure_test_object(signer: &str, object_type: &str, stream: &str, version: u64, body: Value) -> Vec<u8> {
    let sk = insecure_test_signing_key(signer);
    let unsigned = vec![
        ("t", Value::text(object_type)),
        ("stream", Value::text(stream)),
        ("v", Value::Uint(version)),
        ("body", body),
        ("pk", Value::Bytes(sk.verifying_key().to_bytes().to_vec())),
    ];
    let msg = [DOMAIN, &cbor::encode(&map(unsigned.clone()))].concat();
    let sig = sk.sign(&msg).to_bytes().to_vec();
    let mut all = unsigned;
    all.push(("sig", Value::Bytes(sig)));
    cbor::encode(&map(all))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub object_type: String,
    pub stream: String,
    pub version: u64,
    pub signer_pk: [u8; 32],
    pub body: Value,
}

/// Verification hook an importer supplies. The archive never decides object validity itself.
pub trait ObjectVerifier {
    fn verify(&self, declared_type: &str, bytes: &[u8]) -> Option<Verified>;
}

/// Test verifier: strict canonical CBOR, declared type must match, Ed25519 signature must verify, and the signer
/// must be one of the trusted test keys for that stream (stand-in for live authority evaluation).
pub struct InsecureTestVerifier {
    pub trusted: Vec<[u8; 32]>,
}

impl ObjectVerifier for InsecureTestVerifier {
    fn verify(&self, declared_type: &str, bytes: &[u8]) -> Option<Verified> {
        let v = cbor::decode(bytes).ok()?;
        if v.keys() != ["t", "v", "pk", "sig", "body", "stream"] {
            return None; // exact key set in canonical order
        }
        let t = v.get("t")?.as_text()?;
        if t != declared_type {
            return None;
        }
        let pk: [u8; 32] = v.get("pk")?.as_bytes()?.try_into().ok()?;
        if !self.trusted.contains(&pk) {
            return None;
        }
        let sig = Signature::from_slice(v.get("sig")?.as_bytes()?).ok()?;
        let unsigned = match &v {
            Value::Map(m) => Value::Map(m.iter().filter(|(k, _)| k.as_text() != Some("sig")).cloned().collect()),
            _ => return None,
        };
        let msg = [DOMAIN, &cbor::encode(&unsigned)].concat();
        VerifyingKey::from_bytes(&pk).ok()?.verify(&msg, &sig).ok()?;
        Some(Verified {
            object_type: t.to_string(),
            stream: v.get("stream")?.as_text()?.to_string(),
            version: v.get("v")?.as_u64()?,
            signer_pk: pk,
            body: v.get("body")?.clone(),
        })
    }
}
