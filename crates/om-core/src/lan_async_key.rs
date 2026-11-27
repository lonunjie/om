// SPDX-License-Identifier: MPL-2.0
//! Device-local, sans-IO `fetch_async_key` responder (ADR-0279). Use one instance per local device.
//! The table is volatile: a restart drops every grant. The caller supplies the requester's current authenticated
//! relationship state and a distinct [`ContactRef`] per relationship, independent of the LAN frame. The caller
//! applies lifecycle events with [`Responder::retire`] even when no request arrives.

use crate::crypto::{PROFILE_V1, cbor, sha256};
use crate::direct::ErrorCode;
use crate::flows::registry;
use crate::lan::ContactRef;
use crate::lan_frame::Frame;
use om_codec::cbor::Value;
use zeroize::Zeroize;

pub const MAX_ISSUED: usize = 64;
const CAPABILITY_LENGTH: std::ops::RangeInclusive<usize> = 16..=64;

#[derive(Debug, PartialEq, Eq)]
pub enum IssueError {
    InvalidCapability,
    Expired,
    ClockRollback,
    AlreadyIssued,
    Full,
}

/// The bundle and sealing authority selected for this particular authenticated relationship and requester.
pub struct FetchGrant<'a> {
    pub relationship: ContactRef,
    /// The unchanged signed AsyncKeyBundle bytes from local key publication.
    pub signed_bundle: &'a [u8],
    /// Seals the exact bundle bytes to the authenticated requester. `None` means sealing failed.
    /// A caller without a requester-bound sealer must supply no grant.
    pub seal_to_requester: &'a mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
}

pub enum RelationshipState<'a> {
    Active {
        relationship: ContactRef,
        /// Present only while the relationship grants this requester async-key fetch authority.
        fetch_grant: Option<FetchGrant<'a>>,
    },
    Inactive,
    Blocked(ContactRef),
    Ended(ContactRef),
    Retired(ContactRef),
}

/// A sealed object to carry back to the requester, or the existing LAN error frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Bundle {
        request_id: [u8; 16],
        sealed_bundle: Vec<u8>,
    },
    Error(Frame),
}

struct Issued {
    digest: [u8; 32],
    relationship: ContactRef,
    expires_at: u64,
    used: bool,
}

impl Drop for Issued {
    fn drop(&mut self) {
        self.digest.zeroize();
    }
}

pub struct Responder {
    device_id: [u8; 16],
    // In issuance order, so the first expired entry is the oldest issued one.
    issued: Vec<Issued>,
    last_now: u64,
}

fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b)
        .fold(0u8, |difference, (x, y)| difference | (x ^ y))
        == 0
}

impl Responder {
    pub fn new(device_id: [u8; 16]) -> Self {
        Self {
            device_id,
            issued: Vec::new(),
            last_now: 0,
        }
    }

    /// Records a fresh random capability for an already-authorized relationship before it is sent to the peer.
    /// `now` and `expires_at` are local POSIX seconds. A backwards clock fails closed. Live entries, including consumed
    /// entries, keep their slots until expiry or retirement. A full table reclaims its oldest expired entry first.
    pub fn issue(
        &mut self,
        relationship: ContactRef,
        capability: &[u8],
        now: u64,
        expires_at: u64,
    ) -> Result<(), IssueError> {
        if !self.observe_time(now) {
            return Err(IssueError::ClockRollback);
        }
        if !CAPABILITY_LENGTH.contains(&capability.len()) {
            return Err(IssueError::InvalidCapability);
        }
        if expires_at <= now {
            return Err(IssueError::Expired);
        }
        let digest = sha256(&[capability]);
        if self.find(&digest).is_some() {
            return Err(IssueError::AlreadyIssued);
        }
        if self.issued.len() == MAX_ISSUED {
            let expired = self
                .issued
                .iter()
                .position(|entry| entry.expires_at <= now)
                .ok_or(IssueError::Full)?;
            self.issued.remove(expired);
        }
        self.issued.push(Issued {
            digest,
            relationship,
            expires_at,
            used: false,
        });
        Ok(())
    }

    /// Discards all capabilities for a blocked, ended, or retired relationship.
    pub fn retire(&mut self, relationship: &ContactRef) {
        self.issued
            .retain(|entry| &entry.relationship != relationship);
    }

    fn observe_time(&mut self, now: u64) -> bool {
        if now < self.last_now {
            return false;
        }
        self.last_now = now;
        true
    }

    fn find(&self, digest: &[u8; 32]) -> Option<usize> {
        let mut found = None;
        for (index, entry) in self.issued.iter().enumerate() {
            if ct_eq(&entry.digest, digest) {
                found = Some(index);
            }
        }
        found
    }

    /// Handles a decoded LAN fetch. Other frame kinds have no reply from this responder. All denied fetches echo
    /// the request id with the same `Unavailable` code, including failures to seal an authorized reply.
    pub fn handle(
        &mut self,
        frame: &Frame,
        now: u64,
        relationship_state: RelationshipState<'_>,
    ) -> Option<Reply> {
        let grant = match relationship_state {
            RelationshipState::Active {
                relationship,
                fetch_grant,
            } => fetch_grant.filter(|grant| grant.relationship == relationship),
            RelationshipState::Inactive => None,
            RelationshipState::Blocked(relationship)
            | RelationshipState::Ended(relationship)
            | RelationshipState::Retired(relationship) => {
                self.retire(&relationship);
                None
            }
        };
        let Frame::FetchAsyncKey {
            request_id,
            device_id,
            profile,
            capability,
        } = frame
        else {
            return None;
        };
        let clock_ok = self.observe_time(now);
        let sealed_bundle = grant.and_then(|grant| {
            if !clock_ok
                || *device_id != self.device_id
                || *profile != PROFILE_V1
                || !CAPABILITY_LENGTH.contains(&capability.len())
            {
                return None;
            }
            let index = self.find(&sha256(&[capability]))?;
            let entry = &mut self.issued[index];
            if entry.used || entry.expires_at <= now || entry.relationship != grant.relationship {
                return None;
            }
            if !bundle_matches(grant.signed_bundle, device_id, *profile, now).unwrap_or(false) {
                return None;
            }
            entry.used = true;
            (grant.seal_to_requester)(grant.signed_bundle).filter(|bytes| !bytes.is_empty())
        });
        Some(match sealed_bundle {
            Some(sealed_bundle) => Reply::Bundle {
                request_id: *request_id,
                sealed_bundle,
            },
            None => Reply::Error(Frame::Error {
                id: *request_id,
                code: ErrorCode::Unavailable as u64,
            }),
        })
    }
}

// Check the target and expiry in the locally signed bundle. The requester still verifies the signature and freshness
// against its pinned relationship state using the ordinary AsyncKeyBundle rules.
fn bundle_matches(
    signed_bundle: &[u8],
    device_id: &[u8; 16],
    profile: u64,
    now: u64,
) -> Result<bool, crate::crypto::Error> {
    let envelope = registry()
        .check("AsyncKeyBundle", signed_bundle, false)
        .map_err(crate::crypto::Error::Decode)?;
    let Some(Value::Bytes(body)) = envelope.get(1) else {
        return Ok(false);
    };
    let bundle = cbor::decode(body)?;
    Ok(bundle.get(2)?.b()? == device_id
        && bundle.get(5)?.u()? == profile
        && bundle.get(13)?.u()? > now)
}
