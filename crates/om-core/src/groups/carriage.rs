// SPDX-License-Identifier: MPL-2.0
//! Group carriage, Package 2a (ADR-0189 sections 1.1 to 1.3): per-Room routes, `GroupTransportFrame`s over sealed
//! delivery, and the relationship-lane events (kinds 15 to 17).
//!
//! Sans-IO and pure except for [`GroupCarriageRec`], the persisted record of this device's per-Room routes and of the
//! verified routes of other devices. Nothing here touches a network: the host deposits what [`seal_frame`] returns,
//! and hands what it received to [`open_frame`].
//!
//! Rulings applied (`docs/reports/2026-11-14-groups-carriage-status.md`, "Operator-delegated decisions"):
//!
//! * Q1: the codec checks only the outer bstr of a frame. [`decode_frame`] decodes the inner `FrameBody*` strictly
//!   (canonical CBOR, unknown fields rejected, the struct that matches the `group_frame_kind`) and a frame that fails is
//!   dropped before any MLS or group state is touched.
//! * Q2: an Accept is a `ConversationEvent`, bounded by that object's 65,536 bytes.
//! * Q3: kinds 15 to 17 without `["om.group.v1"]` are rejected ([`parse_lane_event`]).
//! * Q4: an empty KeyPackage, Commit or Welcome is wire-valid but is never accepted here.
//!
//! A drop is a [`GroupError`] naming the reason; the caller changes no state when it gets one.

use super::{GroupError, R, err};
use crate::crypto::cbor::{self, V};
use crate::crypto::delivery::{self, KeyMaterial};
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::crypto::sig::{
    DeviceSigner, ROLE_CONTROLLER, ROLE_DEVICE, digest, ds_sig, envelope, open_envelope,
    signature_entry,
};
use crate::crypto::{PROFILE_V1, hex};
use crate::events::{self, Hlc};
use crate::route::RelayEntry;
use crate::state::SecretHex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// `DS-SIG` label of a signed `GroupRouteRecord` (object type 32, role `device`).
pub const LBL_ROUTE_SIG: &str = "OM-GROUP-ROUTE-SIG-v1";
/// `DS-SIG` label of a signed `DeviceCredential` (object type 7); a device may sign its own with role `device`.
pub const LBL_CREDENTIAL_SIG: &str = "OM-DEVICE-CREDENTIAL-SIG-v1";
/// `DS-SIG` label of a signed `GroupControlState` (object type 28, role `device`).
pub const LBL_STATE_SIG: &str = "OM-GROUP-CONTROL-SIG-v1";
/// The capability that kinds 15 to 17 require (ADR-0189 section 1.1).
pub const CAP_GROUP: &str = "om.group.v1";

pub const KIND_INVITE: u64 = 15;
pub const KIND_ACCEPT: u64 = 16;
pub const KIND_DECLINE: u64 = 17;
/// An Accept is a `ConversationEvent` (Q2 ruling): the object's registered limit.
pub const MAX_ACCEPT_BYTES: usize = 65_536;
/// `GroupTransportFrame.body` upper bound (`bstr .size (1..131000)`).
pub const MAX_FRAME_BODY: usize = 131_000;
/// Routes a device keeps per Room while an older one overlaps (policy, not wire; see the report, open question).
pub const MAX_MINE_PER_ROOM: usize = 4;
/// How long a replaced route keeps receiving before its deposit capability is revoked (ADR-0189 1.3 says "a bounded
/// overlap" and gives no number). 3600 s is the v1 default, accepted by the operator's delegation on 2026-11-14
/// (see the Package 2a rulings in the carriage status report); policy, not wire.
pub const ROUTE_OVERLAP_S: u64 = 3600;
/// Lifetime of a route's delivery key (a policy constant like `DELIVERY_KEY_TTL_S`; the record carries the expiry).
/// 30 days is the v1 default, accepted by the operator's delegation on 2026-11-14.
pub const ROUTE_TTL_S: u64 = 30 * 24 * 60 * 60;
/// Wrapper lifetime of a sealed frame (the 1:1 default, `WRAPPER_TTL_S`).
pub const FRAME_WRAPPER_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Verified inbound lane events a device holds until the host pumps them.
pub const MAX_LANE_IN: usize = 64;

pub const FK_COMMIT: u64 = 1;
pub const FK_LEAVE_REQUEST: u64 = 2;
pub const FK_APP: u64 = 3;
pub const FK_WELCOME: u64 = 4;
pub const FK_ADMIT_REQUEST: u64 = 5;
pub const FK_ROUTE_UPDATE: u64 = 6;

fn registry() -> &'static om_codec::Registry {
    static R: OnceLock<om_codec::Registry> = OnceLock::new();
    R.get_or_init(om_codec::Registry::load_w1)
}

fn drop_<T>(why: &'static str) -> R<T> {
    err(why)
}

fn c<T>(r: Result<T, crate::crypto::Error>) -> R<T> {
    r.map_err(|_| GroupError("malformed"))
}

// ------------------------------------------------------------------------------------------------ signed pieces

/// A `signed-DeviceCredential` the device signs itself (role `device`, label `OM-DEVICE-CREDENTIAL-SIG-v1`).
/// `body` is [`crate::crypto::binding::Credential::body`]. The envelope is evidence only: a receiver trusts a
/// credential because its body is one the account's validated head commits to, never because it is self-signed.
pub fn sign_device_credential(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    body: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_CREDENTIAL_SIG, PROFILE_V1, ROLE_DEVICE, device_id, body),
        rnd,
    );
    envelope(
        body.to_vec(),
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// A `signed-GroupControlState` over the canonical state body, one device signature (role `device`).
pub fn sign_state(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    body: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_STATE_SIG, PROFILE_V1, ROLE_DEVICE, device_id, body),
        rnd,
    );
    envelope(
        body.to_vec(),
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// `DS-SIG` label of a signed `AccountControlHead` (object type 2, role `controller`).
pub const LBL_HEAD_SIG: &str = "OM-ACH-SIG-v1";

/// A `signed-AccountControlHead` for `account_id` at `generation` over the control state hash `state_hash`: one
/// signature by a control device (role `controller`). Only a device in the account's active control set may sign one.
pub fn sign_account_head(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    account_id: &[u8; 32],
    generation: u64,
    state_hash: &[u8; 32],
    rnd: [u8; 32],
) -> Vec<u8> {
    let body = cbor::encode(&V::M(vec![
        (0, V::U(2)),
        (1, V::U(1)),
        (2, V::B(account_id.to_vec())),
        (3, V::U(generation)),
        (4, digest(state_hash)),
    ]));
    let s = signer.sign_with(
        &ds_sig(LBL_HEAD_SIG, PROFILE_V1, ROLE_CONTROLLER, device_id, &body),
        rnd,
    );
    envelope(
        body,
        vec![signature_entry(device_id, PROFILE_V1, ROLE_CONTROLLER, s)],
    )
}

/// Verify a `signed-AccountControlHead` against the control device key the caller resolves. Returns
/// `(account_id, generation, state_hash)`.
pub fn open_account_head(
    signed: &[u8],
    pk_of: impl Fn(&[u8; 16]) -> Option<Vec<u8>>,
) -> R<([u8; 32], u64, [u8; 32])> {
    let who = envelope_signer(signed).ok_or(GroupError("malformed"))?;
    let pk = pk_of(&who).ok_or(GroupError("head_signer_unknown"))?;
    let body = open_envelope(signed, LBL_HEAD_SIG, PROFILE_V1, ROLE_CONTROLLER, &who, &pk)
        .map_err(|_| GroupError("bad_signature"))?;
    let v = c(cbor::decode(&body))?;
    let d = c(v.get(4))?;
    Ok((
        c(c(v.get(2))?.b32())?,
        c(c(v.get(3))?.u())?,
        c(c(d.get(1))?.b32())?,
    ))
}

/// The device that claims to have signed a one-signature envelope (to look up its credential; never trusted alone).
pub fn envelope_signer(bytes: &[u8]) -> Option<[u8; 16]> {
    let v = cbor::decode(bytes).ok()?;
    let [s] = v.get(2).ok()?.a().ok()? else {
        return None;
    };
    s.get(0).ok()?.b().ok()?.try_into().ok()
}

/// Open a signed state: role `device`, signed by a device whose public key the caller resolves with `pk_of`.
/// Returns the signer and the canonical body bytes.
pub fn open_state(
    signed: &[u8],
    pk_of: impl Fn(&[u8; 16]) -> Option<Vec<u8>>,
) -> R<([u8; 16], Vec<u8>)> {
    let who = envelope_signer(signed).ok_or(GroupError("malformed"))?;
    let pk = pk_of(&who).ok_or(GroupError("state_signer_unknown"))?;
    let body = open_envelope(signed, LBL_STATE_SIG, PROFILE_V1, ROLE_DEVICE, &who, &pk)
        .map_err(|_| GroupError("bad_signature"))?;
    Ok((who, body))
}

/// `DS-SIG` label of a signed `GroupControlIntent` (object type 29, role `device`, ADR-0176).
pub const LBL_INTENT_SIG: &str = "OM-GROUP-INTENT-SIG-v1";

/// A `signed-GroupControlIntent`: the canonical intent body signed by the acting device (role `device`). This is the
/// envelope a Commit frame and a LeaveRequest frame carry (ADR-0189 1.4(a)); the flow's bare intent body is what it
/// wraps.
pub fn sign_intent(
    signer: &DeviceSigner,
    device_id: &[u8; 16],
    body: &[u8],
    rnd: [u8; 32],
) -> Vec<u8> {
    let s = signer.sign_with(
        &ds_sig(LBL_INTENT_SIG, PROFILE_V1, ROLE_DEVICE, device_id, body),
        rnd,
    );
    envelope(
        body.to_vec(),
        vec![signature_entry(device_id, PROFILE_V1, ROLE_DEVICE, s)],
    )
}

/// Verify a `signed-GroupControlIntent` against the key the caller holds for the device that signed it. Returns the
/// signer and the intent body.
pub fn open_intent(
    signed: &[u8],
    pk_of: impl Fn(&[u8; 16]) -> Option<Vec<u8>>,
) -> R<([u8; 16], Vec<u8>)> {
    let who = envelope_signer(signed).ok_or(GroupError("malformed"))?;
    let pk = pk_of(&who).ok_or(GroupError("intent_signer_unknown"))?;
    let body = open_envelope(signed, LBL_INTENT_SIG, PROFILE_V1, ROLE_DEVICE, &who, &pk)
        .map_err(|_| GroupError("bad_signature"))?;
    Ok((who, body))
}

/// The body of a `signed-*` envelope, without verifying its signature (the caller verifies or compares the body).
pub fn envelope_body(signed: &[u8]) -> R<Vec<u8>> {
    let v = c(cbor::decode(signed))?;
    Ok(c(c(v.get(1))?.b())?.to_vec())
}

// ------------------------------------------------------------------------------------------------ route record

/// One `GroupRouteRecord` (type 32): where one device of a Room receives, with a key fresh for that Room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteRecord {
    pub device_id: [u8; 16],
    pub delivery: KeyMaterial,
    pub relays: Vec<RelayEntry>,
}

impl RouteRecord {
    pub fn body(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::U(32)),
            (1, V::U(1)),
            (2, V::B(self.device_id.to_vec())),
            (3, self.delivery.to_v()),
            (4, V::A(self.relays.iter().map(RelayEntry::to_v).collect())),
        ]))
    }

    /// The signed record: one signature, role `device`, over `DS-SIG("OM-GROUP-ROUTE-SIG-v1", ...)`. Checked against
    /// the registry before it is returned, so a record that comes out is a valid one.
    pub fn sign(&self, signer: &DeviceSigner, rnd: [u8; 32]) -> R<Vec<u8>> {
        let body = self.body();
        let s = signer.sign_with(
            &ds_sig(
                LBL_ROUTE_SIG,
                PROFILE_V1,
                ROLE_DEVICE,
                &self.device_id,
                &body,
            ),
            rnd,
        );
        let signed = envelope(
            body,
            vec![signature_entry(&self.device_id, PROFILE_V1, ROLE_DEVICE, s)],
        );
        registry()
            .check("GroupRouteRecord", &signed, false)
            .map_err(|_| GroupError("malformed"))?;
        Ok(signed)
    }

    /// Verify a received signed record. Drops (the caller changes nothing): `malformed`, `route_bad_signature` (also
    /// the wrong role: only role `device` verifies), `route_for_another_device` (the record names a device other than
    /// `expected_device`), `route_binding_mismatch`, `route_stale` (the key or a relay entry has expired at `now_s`).
    pub fn open(
        signed: &[u8],
        expected_device: &[u8],
        signing_pk: &[u8],
        now_s: u64,
    ) -> R<RouteRecord> {
        if registry().check("GroupRouteRecord", signed, false).is_err() {
            return drop_("malformed");
        }
        let body = open_envelope(
            signed,
            LBL_ROUTE_SIG,
            PROFILE_V1,
            ROLE_DEVICE,
            expected_device,
            signing_pk,
        )
        .map_err(|_| GroupError("route_bad_signature"))?;
        let v = c(cbor::decode(&body))?;
        let device_id: [u8; 16] = c(c(v.get(2))?.b())?
            .try_into()
            .map_err(|_| GroupError("malformed"))?;
        if device_id != expected_device {
            return drop_("route_for_another_device");
        }
        let delivery = c(KeyMaterial::from_v(c(v.get(3))?))?;
        let relays: Vec<RelayEntry> = c(c(v.get(4))?.a())?
            .iter()
            .map(RelayEntry::from_v)
            .collect::<Option<_>>()
            .ok_or(GroupError("malformed"))?;
        let first = relays.first().ok_or(GroupError("malformed"))?;
        if delivery.route_binding != first.binding() {
            return drop_("route_binding_mismatch");
        }
        if now_s > delivery.expires_at.saturating_add(delivery::CLOCK_SKEW)
            || relays.iter().all(|r| {
                r.expires_at != 0 && now_s > r.expires_at.saturating_add(delivery::CLOCK_SKEW)
            })
        {
            return drop_("route_stale");
        }
        Ok(RouteRecord {
            device_id,
            delivery,
            relays,
        })
    }
}

// ------------------------------------------------------------------------------------------------ frames

/// `group-release`: one device's release for one Room (an Accept or an admit request carries up to 16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub device: [u8; 16],
    pub key_package: Vec<u8>,
    /// `signed-DeviceCredential` (exact bytes).
    pub credential: Vec<u8>,
    /// `signed-GroupRouteRecord` (exact bytes).
    pub route: Vec<u8>,
}

/// `group-release-evidence` and `group-member-route`: the same three fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEvidence {
    pub device: [u8; 16],
    pub credential: Vec<u8>,
    pub route: Vec<u8>,
}

/// A decoded, strictly checked frame body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameBody {
    Commit {
        intent: Vec<u8>,
        binding: Vec<u8>,
        commit: Vec<u8>,
        added: Vec<DeviceEvidence>,
    },
    LeaveRequest {
        intent: Vec<u8>,
    },
    App(Vec<u8>),
    Welcome {
        invite_id: [u8; 16],
        welcome: Vec<u8>,
        state: Vec<u8>,
        routes: Vec<DeviceEvidence>,
    },
    AdmitRequest {
        invite_id: [u8; 16],
        releases: Vec<Release>,
        head: Vec<u8>,
    },
    RouteUpdate {
        routes: Vec<Vec<u8>>,
    },
}

impl FrameBody {
    pub fn kind(&self) -> u64 {
        match self {
            FrameBody::Commit { .. } => FK_COMMIT,
            FrameBody::LeaveRequest { .. } => FK_LEAVE_REQUEST,
            FrameBody::App(_) => FK_APP,
            FrameBody::Welcome { .. } => FK_WELCOME,
            FrameBody::AdmitRequest { .. } => FK_ADMIT_REQUEST,
            FrameBody::RouteUpdate { .. } => FK_ROUTE_UPDATE,
        }
    }
}

fn ev_v(e: &DeviceEvidence) -> V {
    V::M(vec![
        (0, V::B(e.device.to_vec())),
        (1, V::B(e.credential.clone())),
        (2, V::B(e.route.clone())),
    ])
}

fn release_v(r: &Release) -> V {
    V::M(vec![
        (0, V::B(r.device.to_vec())),
        (1, V::B(r.key_package.clone())),
        (2, V::B(r.credential.clone())),
        (3, V::B(r.route.clone())),
    ])
}

/// The canonical bytes of a `GroupTransportFrame` (type 33) carrying `body`. Refuses a body the receiver would drop
/// (empty MLS part, size) so a sender never produces a frame its own decoder rejects.
pub fn encode_frame(body: &FrameBody) -> R<Vec<u8>> {
    let inner: Vec<u8> = match body {
        FrameBody::Commit {
            intent,
            binding,
            commit,
            added,
        } => {
            let mut m = vec![
                (0, V::B(intent.clone())),
                (1, V::B(binding.clone())),
                (2, V::B(commit.clone())),
            ];
            if !added.is_empty() {
                m.push((3, V::A(added.iter().map(ev_v).collect())));
            }
            cbor::encode(&V::M(m))
        }
        FrameBody::LeaveRequest { intent } => cbor::encode(&V::M(vec![(0, V::B(intent.clone()))])),
        FrameBody::App(ct) => ct.clone(),
        FrameBody::Welcome {
            invite_id,
            welcome,
            state,
            routes,
        } => cbor::encode(&V::M(vec![
            (0, V::B(invite_id.to_vec())),
            (1, V::B(welcome.clone())),
            (2, V::B(state.clone())),
            (3, V::A(routes.iter().map(ev_v).collect())),
        ])),
        FrameBody::AdmitRequest {
            invite_id,
            releases,
            head,
        } => cbor::encode(&V::M(vec![
            (0, V::B(invite_id.to_vec())),
            (1, V::A(releases.iter().map(release_v).collect())),
            (2, V::B(head.clone())),
        ])),
        FrameBody::RouteUpdate { routes } => cbor::encode(&V::M(vec![(
            0,
            V::A(routes.iter().map(|r| V::B(r.clone())).collect()),
        )])),
    };
    if inner.is_empty() || inner.len() > MAX_FRAME_BODY {
        return drop_("room_too_large");
    }
    let frame = cbor::encode(&V::M(vec![
        (0, V::U(33)),
        (1, V::U(1)),
        (2, V::U(body.kind())),
        (3, V::B(inner)),
    ]));
    // A sealed frame must fit the largest bucket (131,068 bytes of inner; ADR-0189 1.6).
    if frame.len() + 4 > *delivery::BUCKETS.last().expect("buckets") {
        return drop_("room_too_large");
    }
    // The sender checks its own work with the receiver's decoder.
    decode_frame(&frame)?;
    Ok(frame)
}

fn bytes16(v: &V) -> R<[u8; 16]> {
    c(c(v.b())?
        .try_into()
        .map_err(|_| crate::crypto::Error::Binding("len")))
}

fn evidence(v: &V) -> R<DeviceEvidence> {
    Ok(DeviceEvidence {
        device: bytes16(c(v.get(0))?)?,
        credential: c(c(v.get(1))?.b())?.to_vec(),
        route: c(c(v.get(2))?.b())?.to_vec(),
    })
}

fn nonempty(b: &[u8]) -> R<Vec<u8>> {
    if b.is_empty() {
        return drop_("empty_mls_object");
    }
    Ok(b.to_vec())
}

/// Decode a received frame: the outer object through the registry codec, then the inner `FrameBody*` strictly
/// (Q1 ruling). Any failure is a drop; nothing has touched MLS or group state at this point.
pub fn decode_frame(bytes: &[u8]) -> R<FrameBody> {
    if registry()
        .check("GroupTransportFrame", bytes, false)
        .is_err()
    {
        return drop_("frame_invalid");
    }
    let v = c(cbor::decode(bytes))?;
    let kind = c(c(v.get(2))?.u())?;
    let inner = c(c(v.get(3))?.b())?;
    let name = match kind {
        FK_COMMIT => "FrameBodyCommit",
        FK_LEAVE_REQUEST => "FrameBodyLeaveRequest",
        FK_APP => {
            return Ok(FrameBody::App(nonempty(inner)?));
        }
        FK_WELCOME => "FrameBodyWelcome",
        FK_ADMIT_REQUEST => "FrameBodyAdmitRequest",
        FK_ROUTE_UPDATE => "FrameBodyRouteUpdate",
        _ => return drop_("frame_invalid"),
    };
    // Unknown fields, duplicate or unsorted keys, non-minimal integers and a body that is another kind's struct all
    // fail here as `frame_body_invalid`.
    if registry()
        .check_struct(name, inner, MAX_FRAME_BODY)
        .is_err()
    {
        return drop_("frame_body_invalid");
    }
    let b = c(cbor::decode(inner))?;
    let arr = |k: u64| -> R<Vec<V>> { Ok(c(c(b.get(k))?.a())?.to_vec()) };
    Ok(match kind {
        FK_COMMIT => FrameBody::Commit {
            intent: c(c(b.get(0))?.b())?.to_vec(),
            binding: c(c(b.get(1))?.b())?.to_vec(),
            commit: nonempty(c(c(b.get(2))?.b())?)?,
            added: match b.opt(3) {
                None => vec![],
                Some(a) => c(a.a())?.iter().map(evidence).collect::<R<_>>()?,
            },
        },
        FK_LEAVE_REQUEST => FrameBody::LeaveRequest {
            intent: c(c(b.get(0))?.b())?.to_vec(),
        },
        FK_WELCOME => FrameBody::Welcome {
            invite_id: bytes16(c(b.get(0))?)?,
            welcome: nonempty(c(c(b.get(1))?.b())?)?,
            state: c(c(b.get(2))?.b())?.to_vec(),
            routes: arr(3)?.iter().map(evidence).collect::<R<_>>()?,
        },
        FK_ADMIT_REQUEST => FrameBody::AdmitRequest {
            invite_id: bytes16(c(b.get(0))?)?,
            releases: arr(1)?.iter().map(release).collect::<R<_>>()?,
            head: c(c(b.get(2))?.b())?.to_vec(),
        },
        FK_ROUTE_UPDATE => FrameBody::RouteUpdate {
            routes: arr(0)?
                .iter()
                .map(|x| Ok(c(x.b())?.to_vec()))
                .collect::<R<_>>()?,
        },
        _ => unreachable!("matched above"),
    })
}

fn release(v: &V) -> R<Release> {
    Ok(Release {
        device: bytes16(c(v.get(0))?)?,
        key_package: nonempty(c(c(v.get(1))?.b())?)?,
        credential: c(c(v.get(2))?.b())?.to_vec(),
        route: c(c(v.get(3))?.b())?.to_vec(),
    })
}

// ------------------------------------------------------------------------------------------------ seal and open

/// Seal an encoded frame (see [`encode_frame`]) to one device's per-Room route, in the `SealedDeliveryEnvelope`
/// of ADR-0142 unchanged (same buckets and padding). Returns the wrapper and the relay entry the host deposits to.
/// A route that is missing a usable entry or has expired is `route_stale`: the caller queues and refreshes, and never
/// falls back (ADR-0142 section 9).
pub fn seal_frame<Rg: RngCore + CryptoRng>(
    frame: &[u8],
    to: &RouteRecord,
    now_s: u64,
    rng: &mut Rg,
) -> R<(Vec<u8>, RelayEntry)> {
    if now_s > to.delivery.expires_at {
        return drop_("route_stale");
    }
    let entry = to
        .relays
        .iter()
        .find(|r| {
            r.binding() == to.delivery.route_binding && (r.expires_at == 0 || r.expires_at > now_s)
        })
        .ok_or(GroupError("route_stale"))?
        .clone();
    let expires = (now_s + FRAME_WRAPPER_TTL_S).min(to.delivery.expires_at);
    let wrapper = delivery::seal(
        &to.delivery.public_key,
        &to.delivery.key_id,
        &entry.relay_uri,
        &entry.capability,
        expires,
        frame,
        rng,
    )
    .map_err(|_| GroupError("room_too_large"))?;
    Ok((wrapper, entry))
}

/// One sealed frame for one destination device: the host deposits `wrapper` at `entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    pub device: Vec<u8>,
    pub wrapper: Vec<u8>,
    pub entry: RelayEntry,
}

/// The key id a wrapper names (to find which of this device's routes it is for), read without opening it.
pub fn wrapper_key_id(wrapper: &[u8]) -> Option<[u8; 16]> {
    let v = cbor::decode(wrapper).ok()?;
    v.get(3).ok()?.b().ok()?.try_into().ok()
}

// ------------------------------------------------------------------------------------------------ stored state

/// A route this device minted for one Room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyRoute {
    pub key_id: String,
    pub seed: SecretHex,
    /// The mailbox this device polls for the route (the first entry's).
    pub mailbox: String,
    pub entries: Vec<RelayEntry>,
    /// The signed `GroupRouteRecord` exactly as released.
    pub record: String,
    pub expires_at: u64,
    pub minted_at: u64,
    /// Set when a newer route replaced this one: it keeps receiving until `retired_at + ROUTE_OVERLAP_S`.
    #[serde(default)]
    pub retired_at: Option<u64>,
}

/// A verified route of another device of a Room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRoute {
    pub record: String,
    /// The signed credential the route was verified against (kept as evidence for Welcomes and route updates).
    pub credential: String,
    pub expires_at: u64,
}

/// What the invite lane has queued for the host to pump (verified; nothing here has touched group state).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneIn {
    pub relationship_id: String,
    pub event_id: String,
    pub wire: super::flow::GroupWire,
}

/// Wrappers held per profile while a member's account head is missing or stale (ADR-0144 precedent: bounded).
pub const MAX_PENDING: usize = 8;
/// A held wrapper is dropped after this long without the head arriving (ADR-0144 `HELD_TTL`: a week).
pub const PENDING_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Frames waiting in the Core-owned outbox (a bound, not a wire limit).
pub const MAX_OUTBOX: usize = 256;

/// A Welcome or a Commit with added devices that could not be verified yet because the head of an account it names is
/// missing or stale (report D37). Kept as the opaque sealed wrapper, so nothing is trusted or decoded twice; retried
/// when a head arrives; never accepted unverified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFrame {
    /// The sealed wrapper (hex), exactly as it arrived at the route.
    pub wrapper: String,
    pub since_s: u64,
    /// Relationships whose head refresh may supply what is missing (the existing K7 request is sent for each).
    pub refresh: Vec<String>,
    /// Accounts (hex) with no relationship to this device: nothing in the design says where to anchor their head
    /// (report, question Q5). Such a frame stays held until it expires.
    pub unanchored: Vec<String>,
}

/// Where an outbox frame goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutTarget {
    Relationship(String),
    Device { account: String, device: String },
}

/// One frame the Core owns until the host deposits it (G10 condition 1: the Commit and every destination's frames are
/// durable here, in order, before the Commit is accepted).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutItem {
    pub id: u64,
    /// The journal intent that produced it, if any.
    pub intent_id: Option<String>,
    pub to: OutTarget,
    pub wire: super::flow::GroupWire,
}

impl OutItem {
    pub fn to_outbound(&self) -> Option<super::flow::Outbound> {
        use super::flow::{Outbound, Target};
        let to = match &self.to {
            OutTarget::Relationship(r) => Target::Relationship(r.clone()),
            OutTarget::Device { account, device } => Target::Device {
                account: crate::crypto::unhex(account).ok()?,
                device: crate::crypto::unhex(device).ok()?,
            },
        };
        Some(Outbound {
            to,
            wire: self.wire.clone(),
        })
    }
}

/// Persisted in `AppState.group_carriage` (additive, serde default).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCarriageRec {
    /// `room` -> this device's routes, newest last. A room is `invite:<invite id hex>` until the Welcome, then the
    /// ConversationID hex.
    #[serde(default)]
    pub mine: BTreeMap<String, Vec<MyRoute>>,
    /// `room` -> device (hex) -> its verified route.
    #[serde(default)]
    pub peers: BTreeMap<String, BTreeMap<String, PeerRoute>>,
    /// Verified inbound relationship-lane events, oldest first, not yet pumped.
    #[serde(default)]
    pub lane_in: Vec<LaneIn>,
    /// Rooms where a member or device was removed: every remaining device owes a fresh route (ADR-0189 1.3).
    #[serde(default)]
    pub replace_owed: Vec<String>,
    /// The newest controller-signed `AccountControlHead` of this account that a control device gave this device (hex).
    /// A messaging-only device cannot sign one, so it keeps the one it was handed with the enrollment notice and
    /// presents it in an Accept (ADR-0189 1.4(b); report D36). Verified against the own chain before use.
    #[serde(default)]
    pub head_cache: Option<String>,
    /// Frames held until an account head arrives (D37).
    #[serde(default)]
    pub pending: Vec<PendingFrame>,
    /// Frames this Core owns until the host reports them deposited, oldest first.
    #[serde(default)]
    pub outbox: Vec<OutItem>,
    #[serde(default)]
    pub next_out: u64,
}

impl GroupCarriageRec {
    pub fn mine_by_key(&self, key_id: &[u8; 16]) -> Option<(&str, &MyRoute)> {
        let k = hex(key_id);
        self.mine
            .iter()
            .find_map(|(room, v)| v.iter().find(|r| r.key_id == k).map(|r| (room.as_str(), r)))
    }

    /// Record a minted route; older routes of the room stay (overlapping) until [`Self::retire_and_expire`].
    pub fn add_mine(&mut self, room: &str, r: MyRoute) {
        let v = self.mine.entry(room.to_string()).or_default();
        v.push(r);
        let over = v.len().saturating_sub(MAX_MINE_PER_ROOM);
        v.drain(..over);
    }

    /// Route replacement: mark every older route of `room` retired at `now_s` (after a removal, ADR-0189 1.3).
    pub fn retire_older(&mut self, room: &str, now_s: u64) {
        if let Some(v) = self.mine.get_mut(room) {
            let n = v.len();
            for r in v.iter_mut().take(n.saturating_sub(1)) {
                r.retired_at.get_or_insert(now_s);
            }
        }
        self.replace_owed.retain(|x| x != room);
    }

    /// Drop retired routes whose overlap has ended and return their entries: the host revokes each deposit
    /// capability (`DELETE /v1/relay/deposit-capabilities/{id}`), after which a removed member's copy stops working.
    pub fn expire_retired(&mut self, now_s: u64) -> Vec<RelayEntry> {
        let mut revoke = vec![];
        for v in self.mine.values_mut() {
            v.retain(|r| match r.retired_at {
                Some(t) if now_s >= t.saturating_add(ROUTE_OVERLAP_S) => {
                    revoke.extend(r.entries.iter().cloned());
                    false
                }
                _ => true,
            });
        }
        self.mine.retain(|_, v| !v.is_empty());
        revoke
    }

    /// Move a room's records to a new room key (the invite room becomes the Room once the Welcome gives its id).
    pub fn rekey(&mut self, from: &str, to: &str) {
        if let Some(v) = self.mine.remove(from) {
            self.mine.entry(to.to_string()).or_default().extend(v);
        }
        if let Some(v) = self.peers.remove(from) {
            self.peers.entry(to.to_string()).or_default().extend(v);
        }
    }

    pub fn peer(&self, room: &str, device: &[u8]) -> Option<&PeerRoute> {
        self.peers.get(room)?.get(&hex(device))
    }

    /// Learn one device's route. The caller verified it with [`RouteRecord::open`] against that device's credential
    /// and checked the device is in the roster. A record that does not outlive the stored one is `route_not_newer`
    /// (an old, still unexpired record must not roll a route back).
    pub fn learn_peer(
        &mut self,
        room: &str,
        device: &[u8],
        rec: &RouteRecord,
        record: &[u8],
        credential: &[u8],
    ) -> R<()> {
        let slot = self.peers.entry(room.to_string()).or_default();
        if let Some(old) = slot.get(&hex(device))
            && rec.delivery.expires_at <= old.expires_at
            && old.record != hex(record)
        {
            return drop_("route_not_newer");
        }
        slot.insert(
            hex(device),
            PeerRoute {
                record: hex(record),
                credential: hex(credential),
                expires_at: rec.delivery.expires_at,
            },
        );
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------ lane events

/// The body of kinds 15 to 17.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneBody {
    Invite {
        invite_id: [u8; 16],
        room_name: String,
        member_count: Option<u64>,
    },
    Accept {
        invite_id: [u8; 16],
        releases: Vec<Release>,
        head: Vec<u8>,
    },
    Decline {
        invite_id: [u8; 16],
    },
}

impl LaneBody {
    pub fn kind(&self) -> u64 {
        match self {
            LaneBody::Invite { .. } => KIND_INVITE,
            LaneBody::Accept { .. } => KIND_ACCEPT,
            LaneBody::Decline { .. } => KIND_DECLINE,
        }
    }
    pub fn invite_id(&self) -> [u8; 16] {
        match self {
            LaneBody::Invite { invite_id, .. }
            | LaneBody::Accept { invite_id, .. }
            | LaneBody::Decline { invite_id } => *invite_id,
        }
    }
    fn to_v(&self) -> V {
        match self {
            LaneBody::Invite {
                invite_id,
                room_name,
                member_count,
            } => {
                let mut m = vec![(0, V::B(invite_id.to_vec())), (1, V::T(room_name.clone()))];
                if let Some(n) = member_count {
                    m.push((2, V::U(*n)));
                }
                V::M(m)
            }
            LaneBody::Accept {
                invite_id,
                releases,
                head,
            } => V::M(vec![
                (0, V::B(invite_id.to_vec())),
                (1, V::A(releases.iter().map(release_v).collect())),
                (2, V::B(head.clone())),
            ]),
            LaneBody::Decline { invite_id } => V::M(vec![(0, V::B(invite_id.to_vec()))]),
        }
    }
}

/// The canonical `ConversationEvent` of a lane body: kind 15, 16 or 17 with `required_capabilities = ["om.group.v1"]`.
/// Checked against the W1 registry (which bounds the body and, for an Accept, the whole event at 65,536 bytes).
#[allow(clippy::too_many_arguments)]
pub fn lane_event(
    event_id: [u8; 16],
    conversation_id: [u8; 32],
    author_account: [u8; 32],
    author_device: [u8; 16],
    clock: Hlc,
    body: &LaneBody,
) -> R<Vec<u8>> {
    let bytes = events::Header {
        event_id,
        conversation_id,
        author_account,
        author_device,
        clock,
        kind: body.kind(),
        body: body.to_v(),
    }
    .encode_full(None, Some(V::A(vec![V::T(CAP_GROUP.to_string())])));
    if bytes.len() > MAX_ACCEPT_BYTES {
        return drop_("accept_too_large");
    }
    parse_lane_event(&bytes)?;
    Ok(bytes)
}

/// Is this inner payload a group lane event at all (kind 15 to 17)? Read without validating.
pub fn is_lane_event(bytes: &[u8]) -> bool {
    cbor::decode(bytes)
        .ok()
        .and_then(|v| v.get(8).ok()?.u().ok())
        .is_some_and(|k| (KIND_INVITE..=KIND_DECLINE).contains(&k))
}

/// Strictly parse a lane event: W1 registry check (kinds 15 to 17 are valid only to a W1 decoder), the Accept size
/// limit of 65,536 bytes, the required capability `om.group.v1` (Q3: without it the event is rejected), and the typed
/// body. Returns the generic event (authority checks are the caller's) and the typed body.
pub fn parse_lane_event(bytes: &[u8]) -> R<(events::Event, LaneBody)> {
    if bytes.len() > MAX_ACCEPT_BYTES {
        return drop_("accept_too_large");
    }
    if registry().check("ConversationEvent", bytes, false).is_err() {
        return drop_("event_invalid");
    }
    // `parse_group_lane` accepts exactly `["om.group.v1"]` when key 10 is present; kinds 15 to 17 must carry it.
    let v = c(cbor::decode(bytes))?;
    if v.opt(10).is_none() {
        return drop_("capability_missing");
    }
    let ev = events::parse_group_lane(bytes).ok_or(GroupError("capability_missing"))?;
    let body = match ev.kind {
        KIND_INVITE => LaneBody::Invite {
            invite_id: bytes16(c(ev.body.get(0))?)?,
            room_name: c(c(ev.body.get(1))?.t())?.to_string(),
            member_count: match ev.body.opt(2) {
                None => None,
                Some(n) => Some(c(n.u())?),
            },
        },
        KIND_ACCEPT => LaneBody::Accept {
            invite_id: bytes16(c(ev.body.get(0))?)?,
            releases: c(c(ev.body.get(1))?.a())?
                .iter()
                .map(release)
                .collect::<R<_>>()?,
            head: c(c(ev.body.get(2))?.b())?.to_vec(),
        },
        KIND_DECLINE => LaneBody::Decline {
            invite_id: bytes16(c(ev.body.get(0))?)?,
        },
        _ => return drop_("event_invalid"),
    };
    Ok((ev, body))
}
