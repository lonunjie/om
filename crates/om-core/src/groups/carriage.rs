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
use crate::route::{DepositRevocation, QueueRetirement, RelayEntry};
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
pub const KIND_REJOIN_REQUEST: u64 = 18;
pub const CAP_REJOIN: &str = "om.group.rejoin.v1";
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
/// Verified inbound lane events a device holds until the host pumps them (ADR-0189 section 1.1; report D71).
///
/// A device that has not loaded the group module holds an Invite until the person opens groups, which may be weeks. An Invite is
/// small (a name of at most 256 characters and two ids, well under 1 KiB as stored), and the base module keeps no Accept (it knows
/// no outgoing invitation), so 256 events are about a quarter of a megabyte at most: enough for a person invited by many contacts
/// while away, and not a growing store. The earlier bound of 64 dropped silently; a drop is now counted and shown (see
/// [`LaneOverflow`]).
pub const MAX_LANE_IN: usize = 256;
/// How many held lane events one contact may have (report D71). A contact that floods invitations fills its own share and then
/// overflows, instead of crowding out everyone else (AGENT_ARCHITECTURE_RULES: limits are enforced at the recipient).
pub const MAX_LANE_PER_RELATIONSHIP: usize = 32;

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
    account_head_ref(&body)
}

/// Parse a claimed head for routing and deferral. Only `open_account_head` authenticates it.
pub(super) fn account_head_ref(body: &[u8]) -> R<([u8; 32], u64, [u8; 32])> {
    if registry().check_body("AccountControlHead", body).is_err() {
        return drop_("malformed");
    }
    let v = c(cbor::decode(body))?;
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

/// `group-account-head`: the signed head of one account a Welcome names, with the state body it commits to and the
/// credentials that state lists. The receiver verifies it itself (see `carry_room_head`); nothing here is authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountHead {
    pub account: [u8; 32],
    /// `signed-AccountControlHead` (type 2, role `controller`).
    pub head: Vec<u8>,
    /// The canonical body of the `AccountControlState` the head names.
    pub state: Vec<u8>,
    /// Bodies of the `DeviceCredential`s the state lists.
    pub credentials: Vec<Vec<u8>>,
    /// Signed ACS lineage from genesis through `state`, bounded by the account verifier.
    pub chain: Vec<Vec<u8>>,
}

fn head_v(h: &AccountHead) -> V {
    let mut fields = vec![
        (0, V::B(h.account.to_vec())),
        (1, V::B(h.head.clone())),
        (2, V::B(h.state.clone())),
        (3, V::A(h.credentials.iter().cloned().map(V::B).collect())),
    ];
    if !h.chain.is_empty() {
        fields.push((4, V::A(h.chain.iter().cloned().map(V::B).collect())));
    }
    V::M(fields)
}

fn account_head(v: &V) -> R<AccountHead> {
    Ok(AccountHead {
        account: c(c(v.get(0))?.b32())?,
        head: c(c(v.get(1))?.b())?.to_vec(),
        state: c(c(v.get(2))?.b())?.to_vec(),
        credentials: c(c(v.get(3))?.a())?
            .iter()
            .map(|x| Ok(c(x.b())?.to_vec()))
            .collect::<R<_>>()?,
        chain: match v.opt(4) {
            None => vec![],
            Some(chain) => c(chain.a())?
                .iter()
                .map(|x| Ok(c(x.b())?.to_vec()))
                .collect::<R<_>>()?,
        },
    })
}

/// A decoded, strictly checked frame body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameBody {
    Commit {
        intent: Vec<u8>,
        binding: Vec<u8>,
        commit: Vec<u8>,
        added: Vec<DeviceEvidence>,
        /// The signed head of each account the added devices belong to (D93); empty when the committer holds none.
        heads: Vec<AccountHead>,
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
        /// The signed head of every account the Welcome names (ruling on D44); may be empty for a two-account Room.
        heads: Vec<AccountHead>,
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
            heads,
        } => {
            let mut m = vec![
                (0, V::B(intent.clone())),
                (1, V::B(binding.clone())),
                (2, V::B(commit.clone())),
            ];
            if !added.is_empty() {
                m.push((3, V::A(added.iter().map(ev_v).collect())));
            }
            if !heads.is_empty() {
                m.push((4, V::A(heads.iter().map(head_v).collect())));
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
            heads,
        } => {
            let mut m = vec![
                (0, V::B(invite_id.to_vec())),
                (1, V::B(welcome.clone())),
                (2, V::B(state.clone())),
                (3, V::A(routes.iter().map(ev_v).collect())),
            ];
            if !heads.is_empty() {
                m.push((4, V::A(heads.iter().map(head_v).collect())));
            }
            cbor::encode(&V::M(m))
        }
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
            heads: match b.opt(4) {
                None => vec![],
                Some(a) => c(a.a())?.iter().map(account_head).collect::<R<_>>()?,
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
            heads: match b.opt(4) {
                None => vec![],
                Some(a) => c(a.a())?.iter().map(account_head).collect::<R<_>>()?,
            },
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
    /// Home-issued, resource-scoped grant ID. Absent on pre-ADR-0214 persisted routes.
    #[serde(default)]
    pub grant_id: Option<String>,
    /// Zero once the grant has been revoked or expired during deferred queue retirement.
    #[serde(default)]
    pub grant_expires_at: u64,
}

impl MyRoute {
    pub fn receiving(&self, now_s: u64) -> bool {
        self.expires_at > now_s
            && self
                .retired_at
                .is_none_or(|t| now_s < t.saturating_add(ROUTE_OVERLAP_S))
    }

    fn revocation(&self) -> Option<DepositRevocation> {
        Some(DepositRevocation {
            relay_uri: self.entries.first()?.relay_uri.clone(),
            mailbox: self.mailbox.clone(),
            id: self.grant_id.clone()?,
        })
    }
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
    /// When this device received it (local wall clock, seconds; display only, never an authority). `0` for an event held
    /// before this field existed.
    #[serde(default)]
    pub received_s: u64,
}

/// A verified group Invitation this device holds before the group module is loaded (report D70, D71).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldInvitation {
    pub relationship_id: String,
    pub invite_id: String,
    pub group_name: String,
    /// Local receipt time in seconds; `0` when unknown (held by an older build).
    pub received_s: u64,
}

/// Lane events that could not be kept because the queue was full, since the person last acknowledged it (report D71).
/// Nothing is ever dropped without being counted here. `invites` are Invites (the person can ask to be invited again);
/// `replies` are Accepts or Declines (the inviter can invite again).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneOverflow {
    pub invites: u32,
    pub replies: u32,
}

impl LaneOverflow {
    pub fn is_empty(&self) -> bool {
        self.invites == 0 && self.replies == 0
    }
}

/// May one more lane event of `relationship` be held? `Err("lane_full")` when either bound is reached.
pub fn lane_has_room(lane: &[LaneIn], relationship: &str) -> R<()> {
    if lane.len() >= MAX_LANE_IN
        || lane
            .iter()
            .filter(|x| x.relationship_id == relationship)
            .count()
            >= MAX_LANE_PER_RELATIONSHIP
    {
        return err("lane_full");
    }
    Ok(())
}

/// Wrappers held per profile while a member's account head is missing or stale (ADR-0144 precedent: bounded).
pub const MAX_PENDING: usize = 8;
/// A held wrapper is dropped after this long without the head arriving (ADR-0144 `HELD_TTL`: a week).
pub const PENDING_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Frames waiting in the Core-owned outbox (a bound, not a wire limit).
pub const MAX_OUTBOX: usize = 256;
/// Invite, Accept and Decline the group flow produced, waiting for the 1:1 lane to take them (ADR-0189 1.1). A bound.
pub const MAX_LANE_OUT: usize = 64;
/// A lane item not taken by the 1:1 queue after this long is dropped and counted (same horizon as a held frame).
pub const LANE_OUT_TTL_S: u64 = 7 * 24 * 60 * 60;
/// Refresh a queued advertisement after the overlap; distribution debt survives this frame lifetime.
pub const ROUTE_UPDATE_TTL_S: u64 = ROUTE_OVERLAP_S;

/// A Welcome or Commit waiting for an account head or an earlier Room transition. Kept as the exact sealed wrapper
/// and checked again when the prerequisite may have arrived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFrame {
    /// The sealed wrapper (hex), exactly as it arrived at the route.
    pub wrapper: String,
    pub since_s: u64,
    #[serde(default)]
    pub room: String,
    #[serde(default)]
    pub before_welcome: bool,
    /// Relationships whose head refresh may supply what is missing (the existing K7 request is sent for each).
    pub refresh: Vec<String>,
    /// Accounts (hex) that still need a verified head. A new relationship can provide a refresh path.
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
    /// When it was queued (seconds); 0 for an item stored before this field existed (never expired by age).
    #[serde(default)]
    pub since_s: u64,
}

/// An Invite, Accept or Decline the flow produced, kept durably (in the same commit as the step that made it) until the
/// 1:1 lane queue takes it (report D58 closed). Taking it queues the hidden 1:1 message and removes this entry in one
/// transaction, so a crash repeats nothing and loses nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneOut {
    pub id: u64,
    pub relationship: String,
    pub wire: super::flow::GroupWire,
    pub since_s: u64,
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

/// A current per-Room route still owed to one destination, independent of an in-flight frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDebt {
    pub room: String,
    pub to: OutTarget,
}

/// Dedicated queues share this budget between local routes and durable retirement debt.
pub const MAX_QUEUE_RETIRE_DEBT: usize = 128;
/// A retire that was handed to the host is not handed out again before this backoff, doubling per attempt (policy, not wire).
pub const QUEUE_RETIRE_BACKOFF_S: u64 = 60;
pub const QUEUE_RETIRE_BACKOFF_MAX_S: u64 = 6 * 3600;
/// Most retirements handed to the host in one call.
pub const QUEUE_RETIRE_BATCH: usize = 8;

/// A dedicated queue whose routes are gone and whose Home should drop it (ADR-0229 section 3). Durable: survives restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRetireDebt {
    pub relay_uri: String,
    pub mailbox: String,
    /// Random 128-bit request id (hex), assigned the first time the retire is handed out and then reused by every retry.
    #[serde(default)]
    pub request_id: String,
    pub created_s: u64,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub next_try_s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetiredGrant {
    revoke: DepositRevocation,
    expires_at: u64,
}

/// Persisted in `AppState.group_carriage` (additive, serde default).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCarriageRec {
    /// `room` -> this device's routes, newest last. A room is `invite:<invite id hex>` until the Welcome, then the
    /// ConversationID hex.
    #[serde(default)]
    pub mine: BTreeMap<String, Vec<MyRoute>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired_grants: Vec<RetiredGrant>,
    /// Dedicated queues to retire at their Home once the overlap and every grant debt on them are over (ADR-0229 section 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_retire: Vec<QueueRetireDebt>,
    /// The host stages one mint at a time and keeps its journal until this claim is durable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_grant_claim: Option<DepositRevocation>,
    /// `room` -> device (hex) -> its verified route.
    #[serde(default)]
    pub peers: BTreeMap<String, BTreeMap<String, PeerRoute>>,
    /// Verified inbound relationship-lane events, oldest first, not yet pumped.
    #[serde(default)]
    pub lane_in: Vec<LaneIn>,
    /// Lane events dropped because the queue was full, until the person acknowledges (report D71).
    #[serde(default)]
    pub lane_overflow: LaneOverflow,
    /// Rooms where a member or device was removed: every remaining device owes a fresh route (ADR-0189 1.3).
    #[serde(default)]
    pub replace_owed: Vec<String>,
    /// Distribution obligations, cleared only when the host reports the corresponding current frame deposited.
    #[serde(default)]
    pub route_debt: Vec<RouteDebt>,
    /// The newest controller-signed `AccountControlHead` of this account that a control device gave this device (hex).
    /// A messaging-only device cannot sign one, so it keeps the one it was handed with the enrollment notice and
    /// presents it in an Accept (ADR-0189 1.4(b); report D36). Verified against the own chain before use.
    #[serde(default)]
    pub head_cache: Option<String>,
    #[serde(default)]
    pub head_sync_asked_ms: u64,
    /// The signed head of each account this device holds no relationship head for, as a Welcome or Commit carried it and
    /// this device verified it, per `(Room, AccountID)` (ADR-0229 section 1; key `"{room}/{account hex}"`, see
    /// [`room_head_key`]). Used only to prove that account's leaves in that Room; a pin never answers for another Room.
    #[serde(default)]
    pub room_heads: BTreeMap<String, RoomHead>,
    /// The signed head each peer's Accept carried (account hex -> hex), kept so a later Welcome can name that account
    /// with evidence. Used only while it still names the head pinned on the relationship.
    #[serde(default)]
    pub peer_heads: BTreeMap<String, String>,
    /// Verified ordinary ACS lineages retained from contact binding and head refresh, for Room release evidence.
    #[serde(default)]
    pub peer_chains: BTreeMap<String, PeerChain>,
    /// Frames held while an account proof or Room transition is missing (ADR-0212).
    #[serde(default)]
    pub pending: Vec<PendingFrame>,
    /// Frames this Core owns until the host reports them deposited, oldest first.
    #[serde(default)]
    pub outbox: Vec<OutItem>,
    #[serde(default)]
    pub next_out: u64,
    /// Invite, Accept and Decline not yet taken by the 1:1 lane, oldest first (bounded by [`MAX_LANE_OUT`]).
    #[serde(default)]
    pub lane_out: Vec<LaneOut>,
    /// Lane items dropped for good (a refusal that cannot change, or older than [`LANE_OUT_TTL_S`]): never silent, so the
    /// host can tell the person.
    #[serde(default)]
    pub lane_out_lost: u32,
    #[serde(default)]
    pub rejoin: super::rejoin::RejoinState,
}

/// The key of a Room-only pin: the Room and the account (hex), so a pin answers for that Room only (ADR-0229 section 1).
pub fn room_head_key(room: &str, account: &str) -> String {
    format!("{room}/{account}")
}

/// A verified head of an account the device has no relationship head for: the state body and its credential bodies.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomHead {
    pub state: String,
    pub credentials: Vec<String>,
    /// A later carried chain showed a different state at the pinned generation (ADR-0229 section 1 item 3): the account is
    /// pending in this Room and the pin is not used.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub conflict: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerChain {
    pub states: Vec<String>,
    pub credentials: Vec<String>,
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
        if let Some(grant) = r.revocation() {
            self.last_grant_claim = Some(grant);
        }
        self.mine.entry(room.to_string()).or_default().push(r);
    }

    pub fn owns_grant(&self, grant: &DepositRevocation) -> bool {
        self.last_grant_claim.as_ref() == Some(grant)
            || self
                .mine
                .values()
                .flatten()
                .any(|r| r.revocation().as_ref() == Some(grant))
            || self.retired_grants.iter().any(|debt| &debt.revoke == grant)
    }

    pub fn retire_unclaimed(&mut self, grant: DepositRevocation, expires_at: u64) {
        if !self.owns_grant(&grant) {
            self.last_grant_claim = Some(grant.clone());
            self.retired_grants.push(RetiredGrant {
                revoke: grant,
                expires_at,
            });
        }
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

    /// A device leaving or losing membership no longer needs any of its routes.
    pub fn retire_room(&mut self, room: &str, now_s: u64) {
        if let Some(routes) = self.mine.get_mut(room) {
            for route in routes {
                route.retired_at.get_or_insert(now_s);
            }
        }
        self.replace_owed.retain(|r| r != room);
    }

    /// A retired route stops receiving at the boundary, but its grant metadata survives until DELETE
    /// succeeds or the Home's expiry (including clock skew). Legacy routes have no DELETE target.
    pub fn expire_retired(&mut self, now_s: u64) -> Vec<DepositRevocation> {
        self.prune_retired(now_s);
        self.retired_grants
            .iter()
            .map(|grant| grant.revoke.clone())
            .collect()
    }

    /// Reserve retirement capacity before admitting another dedicated route.
    pub(super) fn can_mint_queue(&self) -> bool {
        let minted = self
            .mine
            .values()
            .flatten()
            .filter(|r| r.grant_id.is_some())
            .count();
        self.queue_retire.len() + minted < MAX_QUEUE_RETIRE_DEBT
    }

    pub(super) fn prune_retired(&mut self, now_s: u64) {
        let grants = &mut self.retired_grants;
        let queues = &mut self.queue_retire;
        grants.retain(|grant| now_s <= grant.expires_at.saturating_add(300));
        for routes in self.mine.values_mut() {
            routes.retain_mut(|r| {
                if r.receiving(now_s) {
                    return true;
                }
                if now_s > r.grant_expires_at.saturating_add(300) {
                    r.grant_expires_at = 0;
                }
                if r.grant_expires_at != 0
                    && let Some(revoke) = r.revocation()
                    && !grants.iter().any(|grant| grant.revoke == revoke)
                {
                    grants.push(RetiredGrant {
                        revoke,
                        expires_at: r.grant_expires_at,
                    });
                }
                if Self::note_queue_retire(queues, r, now_s) {
                    return false;
                }
                // A profile written before capacity was reserved can still have routes over budget.
                // Keep their retirement metadata without reopening the receive route after a clock change.
                r.expires_at = 0;
                r.seed = String::new().into();
                true
            });
        }
        self.mine.retain(|_, routes| !routes.is_empty());
    }

    /// Return true when a route can leave the table. A dedicated queue must already have retirement debt or get a slot.
    fn note_queue_retire(queues: &mut Vec<QueueRetireDebt>, r: &MyRoute, now_s: u64) -> bool {
        let (Some(entry), Some(_)) = (r.entries.first(), r.grant_id.as_ref()) else {
            return true;
        };
        if queues
            .iter()
            .any(|d| d.relay_uri == entry.relay_uri && d.mailbox == r.mailbox)
        {
            return true;
        }
        if queues.len() >= MAX_QUEUE_RETIRE_DEBT {
            return false;
        }
        queues.push(QueueRetireDebt {
            relay_uri: entry.relay_uri.clone(),
            mailbox: r.mailbox.clone(),
            request_id: String::new(),
            created_s: now_s,
            attempts: 0,
            next_try_s: 0,
        });
        true
    }

    /// Retirements due now: the overlap is over, no grant revocation on that queue is outstanding (completed or expired),
    /// and the backoff has passed. The request id is assigned here (random, durable with this commit) and reused by every
    /// retry. Each call counts an attempt, so a lost reply is retried after the backoff, after a restart too.
    pub fn due_queue_retires<Rg: RngCore>(
        &mut self,
        now_s: u64,
        rng: &mut Rg,
    ) -> Vec<QueueRetirement> {
        self.prune_retired(now_s);
        let busy = |relay: &str, mailbox: &str, rec: &GroupCarriageRec| {
            rec.retired_grants
                .iter()
                .any(|g| g.revoke.relay_uri == relay && g.revoke.mailbox == mailbox)
                || rec.mine.values().flatten().any(|r| {
                    r.mailbox == mailbox
                        && r.receiving(now_s)
                        && r.entries.first().is_some_and(|e| e.relay_uri == relay)
                })
        };
        let ready: Vec<usize> = (0..self.queue_retire.len())
            .filter(|&i| {
                let d = &self.queue_retire[i];
                d.next_try_s <= now_s && !busy(&d.relay_uri, &d.mailbox, self)
            })
            .take(QUEUE_RETIRE_BATCH)
            .collect();
        let mut out = vec![];
        for i in ready {
            let d = &mut self.queue_retire[i];
            if d.request_id.is_empty() {
                let mut id = [0u8; 16];
                rng.fill_bytes(&mut id);
                d.request_id = hex(&id);
            }
            d.attempts = d.attempts.saturating_add(1);
            let wait = QUEUE_RETIRE_BACKOFF_S
                .saturating_mul(1u64 << d.attempts.saturating_sub(1).min(16))
                .min(QUEUE_RETIRE_BACKOFF_MAX_S);
            d.next_try_s = now_s.saturating_add(wait);
            out.push(QueueRetirement {
                relay_uri: d.relay_uri.clone(),
                mailbox: d.mailbox.clone(),
                request_id: d.request_id.clone(),
            });
        }
        out
    }

    /// The Home acknowledged the retire (or says the queue is already gone or tombstoned): the debt is complete.
    pub fn queue_retired(&mut self, done: &QueueRetirement) {
        self.queue_retire.retain(|d| {
            !(!d.request_id.is_empty()
                && d.relay_uri == done.relay_uri
                && d.mailbox == done.mailbox
                && d.request_id == done.request_id)
        });
    }

    pub fn revoked(&mut self, done: &DepositRevocation, now_s: u64) {
        self.retired_grants.retain(|grant| &grant.revoke != done);
        for r in self.mine.values_mut().flatten() {
            if !r.receiving(now_s) && r.revocation().as_ref() == Some(done) {
                r.grant_expires_at = 0;
            }
        }
        self.prune_retired(now_s);
    }

    /// The usable Room-only pin for `account` (hex) in `room`.
    pub fn room_head(&self, room: &str, account: &str) -> Option<&RoomHead> {
        self.room_heads
            .get(&room_head_key(room, account))
            .filter(|rh| !rh.conflict)
    }

    /// The usable Room-only pins of `room`.
    pub fn room_heads_of<'a>(&'a self, room: &str) -> impl Iterator<Item = &'a RoomHead> + 'a {
        let prefix = format!("{room}/");
        self.room_heads
            .iter()
            .filter(move |(k, rh)| k.starts_with(&prefix) && !rh.conflict)
            .map(|(_, rh)| rh)
    }

    /// Move a room's records to a new room key (the invite room becomes the Room once the Welcome gives its id).
    pub fn rekey(&mut self, from: &str, to: &str) {
        let prefix = format!("{from}/");
        let moved: Vec<String> = self
            .room_heads
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        for k in moved {
            if let Some(rh) = self.room_heads.remove(&k) {
                self.room_heads
                    .entry(format!("{to}/{}", &k[prefix.len()..]))
                    .or_insert(rh);
            }
        }
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
    RejoinRequest {
        account: [u8; 32],
        room_hint: String,
    },
}

impl LaneBody {
    pub fn kind(&self) -> u64 {
        match self {
            LaneBody::Invite { .. } => KIND_INVITE,
            LaneBody::Accept { .. } => KIND_ACCEPT,
            LaneBody::Decline { .. } => KIND_DECLINE,
            LaneBody::RejoinRequest { .. } => KIND_REJOIN_REQUEST,
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
            LaneBody::RejoinRequest { account, room_hint } => V::M(vec![
                (0, V::B(account.to_vec())),
                (1, V::T(room_hint.clone())),
            ]),
        }
    }
}

fn lane_capability(kind: u64) -> &'static str {
    if kind == KIND_REJOIN_REQUEST {
        CAP_REJOIN
    } else {
        CAP_GROUP
    }
}

/// The canonical `ConversationEvent` of a relationship-lane body.
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
    .encode_full(
        None,
        Some(V::A(vec![V::T(lane_capability(body.kind()).to_string())])),
    );
    if bytes.len() > MAX_ACCEPT_BYTES {
        return drop_("accept_too_large");
    }
    parse_lane_event(&bytes)?;
    Ok(bytes)
}

/// Is this inner payload a group lane event at all (kind 15 to 18)? Read without validating.
pub fn is_lane_event(bytes: &[u8]) -> bool {
    cbor::decode(bytes)
        .ok()
        .and_then(|v| v.get(8).ok()?.u().ok())
        .is_some_and(|k| (KIND_INVITE..=KIND_REJOIN_REQUEST).contains(&k))
}

/// Check the W1 registry, size, kind version and the exact per-kind capability, then decode the body.
pub fn parse_lane_event(bytes: &[u8]) -> R<(events::Event, LaneBody)> {
    if bytes.len() > MAX_ACCEPT_BYTES {
        return drop_("accept_too_large");
    }
    if registry().check("ConversationEvent", bytes, false).is_err() {
        return drop_("event_invalid");
    }
    let v = c(cbor::decode(bytes))?;
    let kind = c(c(v.get(8))?.u())?;
    let ev = events::parse_group_lane(bytes, lane_capability(kind))
        .ok_or(GroupError("capability_missing"))?;
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
        KIND_REJOIN_REQUEST => LaneBody::RejoinRequest {
            account: c(c(ev.body.get(0))?.b32())?,
            room_hint: c(c(ev.body.get(1))?.t())?.to_string(),
        },
        _ => return drop_("event_invalid"),
    };
    Ok((ev, body))
}

#[cfg(test)]
mod lane_bound_tests {
    use super::*;
    use crate::groups::flow::GroupWire;

    fn held(rel: &str, n: usize) -> Vec<LaneIn> {
        (0..n)
            .map(|i| LaneIn {
                relationship_id: rel.into(),
                event_id: format!("e-{rel}-{i}"),
                wire: GroupWire::Decline {
                    invite_id: "00".into(),
                },
                received_s: 0,
            })
            .collect()
    }

    #[test]
    fn one_contact_fills_only_its_own_share() {
        let lane = held("a", MAX_LANE_PER_RELATIONSHIP);
        assert_eq!(lane_has_room(&lane, "a"), err("lane_full"));
        assert!(
            lane_has_room(&lane, "b").is_ok(),
            "others are not crowded out"
        );
    }

    #[test]
    fn the_whole_queue_is_bounded() {
        let mut lane = vec![];
        for r in 0..(MAX_LANE_IN / MAX_LANE_PER_RELATIONSHIP) {
            lane.extend(held(&format!("r{r}"), MAX_LANE_PER_RELATIONSHIP));
        }
        assert_eq!(lane.len(), MAX_LANE_IN);
        assert_eq!(lane_has_room(&lane, "new"), err("lane_full"));
        lane.pop();
        assert!(lane_has_room(&lane, "new").is_ok());
    }

    #[test]
    fn old_state_without_the_new_fields_still_loads() {
        let l: LaneIn = serde_json::from_str(
            r#"{"relationshipId":"r","eventId":"e","wire":{"t":"decline","invite_id":"00"}}"#,
        )
        .unwrap();
        assert_eq!(l.received_s, 0);
        let rec: GroupCarriageRec = serde_json::from_str("{}").unwrap();
        assert!(rec.lane_overflow.is_empty());
    }
}

#[cfg(test)]
mod queue_retire_tests {
    use super::*;
    use crate::crypto::rng::insecure_test_rng::InsecureTestRng;

    fn route(n: usize, retired_at: Option<u64>, grant: bool) -> MyRoute {
        MyRoute {
            key_id: format!("k{n}"),
            seed: SecretHex::from(String::new()),
            mailbox: format!("{n:032x}"),
            entries: vec![RelayEntry {
                relay_uri: "https://home.example/".into(),
                capability: vec![1; 32],
                priority: 0,
                expires_at: 1_000_000,
            }],
            record: String::new(),
            expires_at: 1_000_000,
            minted_at: 0,
            retired_at,
            grant_id: grant.then(|| format!("{n:032x}")),
            grant_expires_at: 1_000_000,
        }
    }

    #[test]
    fn a_full_debt_table_keeps_existing_debts_and_defers_retired_routes() {
        let mut rec = GroupCarriageRec::default();
        for n in 0..MAX_QUEUE_RETIRE_DEBT {
            assert!(GroupCarriageRec::note_queue_retire(
                &mut rec.queue_retire,
                &route(n, Some(0), true),
                10
            ));
        }
        assert!(!rec.can_mint_queue());
        let now = ROUTE_OVERLAP_S + 1;
        let deferred = route(MAX_QUEUE_RETIRE_DEBT, Some(0), true);
        let revoke = deferred.revocation().unwrap();
        let mailbox = deferred.mailbox.clone();
        rec.add_mine("room", deferred);
        let before = rec.queue_retire.clone();
        assert_eq!(rec.expire_retired(now), vec![revoke.clone()]);
        assert!(rec.queue_retire == before);
        assert!(!rec.mine["room"][0].receiving(0));
        rec.revoked(&revoke, now);

        // The deferred route and acknowledged grant revocation survive a restart.
        let mut rec: GroupCarriageRec =
            serde_json::from_slice(&serde_json::to_vec(&rec).unwrap()).unwrap();
        assert!(rec.expire_retired(now).is_empty());
        let mut rng = InsecureTestRng::from_label(b"full queue-retire");
        let due = rec.due_queue_retires(now, &mut rng);
        rec.queue_retired(&due[0]);
        rec.prune_retired(now);
        assert!(!rec.mine.contains_key("room"));
        assert_eq!(rec.queue_retire.len(), MAX_QUEUE_RETIRE_DEBT);
        assert_eq!(rec.queue_retire.last().unwrap().mailbox, mailbox);
        assert!(rec.expire_retired(now).is_empty());
        assert!(!rec.can_mint_queue());
        rec.queue_retired(&due[1]);
        assert!(rec.can_mint_queue());
        rec.add_mine("next", route(MAX_QUEUE_RETIRE_DEBT + 1, None, true));
        assert!(!rec.can_mint_queue());
    }

    #[test]
    fn a_legacy_route_without_a_grant_owes_no_retire_and_a_repeat_adds_nothing() {
        let mut rec = GroupCarriageRec::default();
        let mut note =
            |r: MyRoute| GroupCarriageRec::note_queue_retire(&mut rec.queue_retire, &r, 10);
        assert!(note(route(1, Some(0), false)));
        assert!(note(route(2, Some(0), true)));
        assert!(note(route(2, Some(0), true)));
        assert_eq!(rec.queue_retire.len(), 1);
        assert_eq!(rec.queue_retire[0].mailbox, route(2, Some(0), true).mailbox);
    }

    #[test]
    fn expired_grants_release_the_retire_and_it_retries_until_acknowledged() {
        let mut rec = GroupCarriageRec::default();
        let mut rng = InsecureTestRng::from_label(b"queue-retire");
        rec.add_mine("room", route(3, Some(0), true));
        // overlap over, grant not expired: the grant debt holds the retire back
        let now = ROUTE_OVERLAP_S + 1;
        assert!(rec.due_queue_retires(now, &mut rng).is_empty());
        assert_eq!(rec.expire_retired(now).len(), 1);
        // The grant expires at 1_000_000 (+300 skew), allowing the queue retire.
        let later = 1_000_400;
        let due = rec.due_queue_retires(later, &mut rng);
        assert_eq!(due.len(), 1);
        let mut rec: GroupCarriageRec =
            serde_json::from_slice(&serde_json::to_vec(&rec).unwrap()).unwrap();
        let future = later + 416 * 86_400;
        assert_eq!(rec.due_queue_retires(future, &mut rng), due);
        assert!(rec.due_queue_retires(future + 1, &mut rng).is_empty());
        rec.queue_retired(&due[0]);
        assert!(rec.queue_retire.is_empty());
    }
}
