// SPDX-License-Identifier: MPL-2.0
//! Foreign identity links (`bridges` slice 5; INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md sections 4 and 5, ADR-0264).
//! A link is presentation evidence that an account controls a foreign handle. It never grants authorship and never merges
//! accounts. Pure: the controller signature and the foreign proof (challenge, OAuth, nonce, domain or bridge attestation)
//! are verified by the caller and arrive as `proof_verified`.
//!
//! * `expires_at` is required; a link outside `[not_before, expires_at)` is not shown as linked.
//! * The link chain is hash-linked with version + 1 and a previous hash; a revoked link stays revoked.
//! * The account-control basis must equal the validated control head the caller holds; anything else fails closed.
//! * Method 5 (bridge attestation) must name the attesting bridge.
//! * [`LinkTable::display`] gives a "linked" label. [`native_author`] is always `None`: a link never makes a foreign
//!   handle, or a bridge, the author of an account's event.

use crate::crypto::sig::ds_hash;
use om_codec::{Registry, Value};
use std::collections::BTreeMap;

pub const LBL_LINK_HASH: &str = "OM-FOREIGN-IDENTITY-LINK-HASH-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub account: [u8; 32],
    pub namespace: String,
    pub foreign_ref: String,
    pub method: u64,
    pub attestation: [u8; 32],
    pub basis: (u64, [u8; 32]),
    pub expires_at: u64,
    pub revoked: bool,
    pub version: u64,
    pub prev: Option<[u8; 32]>,
    pub bridge: Option<[u8; 32]>,
    pub not_before: Option<u64>,
    pub hash: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub enum LinkError {
    Malformed,
    /// The foreign proof or the controller signature was not verified.
    ProofNotVerified,
    /// The control basis is not the validated head the caller holds.
    StaleBasis,
    /// Method 5 without a bridge id.
    MissingBridge,
    BadVersion,
    UnknownPrev,
    /// A revoked link was offered as active again, or an older version arrived.
    Resurrected,
    Duplicate,
    /// `expires_at` is not after `not_before`.
    BadWindow,
}

fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(n)) = v {
        Some(*n)
    } else {
        None
    }
}
fn bytes32(v: Option<&Value>) -> Option<[u8; 32]> {
    if let Some(Value::Bytes(b)) = v {
        b.as_slice().try_into().ok()
    } else {
        None
    }
}
fn digest(v: Option<&Value>) -> Option<[u8; 32]> {
    bytes32(v?.get(1))
}

impl Link {
    pub fn from_body(body: &[u8]) -> Result<Self, LinkError> {
        let v = Registry::load_w4()
            .check_body("ForeignIdentityLink", body)
            .map_err(|_| LinkError::Malformed)?;
        let bad = || LinkError::Malformed;
        let text = |x: Option<&Value>| {
            if let Some(Value::Text(t)) = x {
                Some(t.clone())
            } else {
                None
            }
        };
        let basis = v.get(7).ok_or_else(bad)?;
        Ok(Link {
            account: bytes32(v.get(2)).ok_or_else(bad)?,
            namespace: text(v.get(3)).ok_or_else(bad)?,
            foreign_ref: text(v.get(4)).ok_or_else(bad)?,
            method: uint(v.get(5)).ok_or_else(bad)?,
            attestation: digest(v.get(6)).ok_or_else(bad)?,
            basis: (
                uint(basis.get(0)).ok_or_else(bad)?,
                digest(basis.get(1)).ok_or_else(bad)?,
            ),
            expires_at: uint(v.get(8)).ok_or_else(bad)?,
            revoked: uint(v.get(9)).ok_or_else(bad)? == 2,
            version: uint(v.get(10)).ok_or_else(bad)?,
            prev: v
                .get(11)
                .map(|d| digest(Some(d)).ok_or_else(bad))
                .transpose()?,
            bridge: v
                .get(12)
                .map(|b| bytes32(Some(b)).ok_or_else(bad))
                .transpose()?,
            not_before: v
                .get(13)
                .map(|n| uint(Some(n)).ok_or_else(bad))
                .transpose()?,
            hash: ds_hash(LBL_LINK_HASH, body),
        })
    }

    fn key(&self) -> ([u8; 32], String, String) {
        (
            self.account,
            self.namespace.clone(),
            self.foreign_ref.clone(),
        )
    }
}

/// What a client shows next to a foreign handle.
#[derive(Debug, PartialEq, Eq)]
pub enum Display {
    /// "linked to this account" presentation, with no authorship meaning.
    Linked {
        account: [u8; 32],
        method: u64,
    },
    NotLinked,
}

#[derive(Default)]
pub struct LinkTable {
    heads: BTreeMap<([u8; 32], String, String), Link>,
}

impl LinkTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts one link version. `account_head` is the validated account-control head `(version, hash)` the caller holds.
    pub fn accept(
        &mut self,
        l: Link,
        proof_verified: bool,
        account_head: (u64, [u8; 32]),
    ) -> Result<(), LinkError> {
        if !proof_verified {
            return Err(LinkError::ProofNotVerified);
        }
        if l.basis != account_head {
            return Err(LinkError::StaleBasis);
        }
        if l.method == 5 && l.bridge.is_none() {
            return Err(LinkError::MissingBridge);
        }
        if l.not_before.is_some_and(|n| l.expires_at <= n) {
            return Err(LinkError::BadWindow);
        }
        match self.heads.get(&l.key()) {
            None => {
                if l.version != 1 || l.prev.is_some() {
                    return Err(if l.prev.is_some() {
                        LinkError::UnknownPrev
                    } else {
                        LinkError::BadVersion
                    });
                }
            }
            Some(cur) => {
                if cur.hash == l.hash {
                    return Err(LinkError::Duplicate);
                }
                if cur.revoked {
                    return Err(LinkError::Resurrected);
                }
                if l.prev != Some(cur.hash) {
                    return Err(LinkError::UnknownPrev);
                }
                if l.version != cur.version + 1 {
                    return Err(LinkError::BadVersion);
                }
            }
        }
        self.heads.insert(l.key(), l);
        Ok(())
    }

    /// Display for a foreign handle at `now`. Expired, not-yet-valid or revoked links are not shown as linked.
    pub fn display(
        &self,
        account: &[u8; 32],
        namespace: &str,
        foreign_ref: &str,
        now: u64,
    ) -> Display {
        let k = (*account, namespace.to_string(), foreign_ref.to_string());
        match self.heads.get(&k) {
            Some(l)
                if !l.revoked && now < l.expires_at && l.not_before.is_none_or(|n| now >= n) =>
            {
                Display::Linked {
                    account: l.account,
                    method: l.method,
                }
            }
            _ => Display::NotLinked,
        }
    }
}

/// A valid link is presentation evidence only: it never yields a native author.
pub fn native_author(_link: &Link) -> Option<[u8; 32]> {
    None
}
