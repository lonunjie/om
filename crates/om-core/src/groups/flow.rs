// SPDX-License-Identifier: MPL-2.0
//! Ordinary groups, slice 3 (ADR-0177): the invite, accept, add, leave, remove and group-text flow in Core.
//!
//! Sans-IO. The host calls [`AppState::group_do`] with a typed [`GroupIntent`], feeds what arrived to
//! [`AppState::group_receive`] and sends every [`Outbound`] in the reply; [`AppState::group_query`] answers typed
//! queries. Nothing here touches the network, a clock or an RNG other than through [`GroupCtx`].
//!
//! What this module decides (groups plan, "Slice 3 build status"):
//!
//! * **Consent first** (ADR-0128 rule 5, GROUP_CHAT_ARCHITECTURE "Adding a person"): an invitation travels over an
//!   existing 1:1 relationship and only an `Active` one will do (`AcceptedUnbound` is refused: the peer's account
//!   is not yet bound, INVITES section 5.1). Only after the invitee accepts and releases KeyPackages is the Add
//!   committed (through the ADR-0163 journal). A Welcome that no accepted invitation of ours precedes is dropped.
//! * **D-g6** the invitation needs `room.invite` (registry operation `control.member.invite`); the Add itself is
//!   the `member.admit` intent and keeps the registry's `room.manage_admission`. Both must hold: the conservative
//!   reading of a disagreement the text does not settle.
//! * **D-g13** the signed `GroupControlIntent` travels with its Commit in one [`GroupWire::Commit`]; the receiver
//!   derives the acting account from its own device directory, never from the sender's claim.
//! * No pairwise-fan-out group mode (G1): a group text message is one engine ciphertext
//!   ([`GroupCrypto::encrypt_application`]) wrapped per recipient device by delivery.
//!
//! The wire payloads of [`GroupWire`] are Core-internal JSON and are NOT a frozen wire: which 1:1 object type
//! carries them is an open design question (plan, "Slice 3 build status").

use super::binding::KIND_COORDINATOR_SEQUENCED;
use super::journal::{CommitPlan, ReceiveOutcome};
use super::mls::{CryptoEnv, GroupCrypto, MlsError};
use super::room::{self, Change, Notice, Setting};
use super::{GenesisParams, GroupError, GroupState, Intent, Principal, R, apply, err, genesis, op};
use crate::crypto::rng::{RngCore, fill};
use crate::crypto::sig::DeviceSigner;
use crate::crypto::{hex, unhex};
use crate::state::{AppState, RelState};
use om_codec::{Value, encode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Live invitations a person may hold at once (Incoming, not yet answered): a bound, not a design number.
const MAX_OPEN_INCOMING: usize = 16;
/// Announced Room-setting changes kept per Room.
const MAX_NOTICES: usize = 64;

fn mls<T>(r: Result<T, MlsError>) -> R<T> {
    r.map_err(|e| GroupError(e.0))
}
fn h(x: &[u8]) -> String {
    hex(x)
}
fn un(x: &str) -> R<Vec<u8>> {
    unhex(x).map_err(|_| GroupError("record_corrupt"))
}

// ---- records (AppState, all serde-default additive) ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteDir {
    Outgoing,
    Incoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteState {
    Offered,
    Accepted,
    Declined,
    /// Outgoing: the Add Commit is prepared and not yet accepted by ordering.
    Admitting,
    /// Outgoing: added. Incoming: joined.
    Done,
}

/// One device's release for a group: its KeyPackage and the device public key its signatures verify under.
/// (Real engines carry the ACS-bound credential in the KeyPackage; verifying it against the ACS head is a gap.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRelease {
    pub device: String,
    pub key_package: String,
    pub pk: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInviteRec {
    pub id: String,
    pub conversation_id: String,
    pub group_name: String,
    pub dir: InviteDir,
    pub state: InviteState,
    pub relationship_id: String,
    /// The other person's account (hex): the invitee on an outgoing record, the inviter on an incoming one.
    pub peer_account: String,
    #[serde(default)]
    pub releases: Vec<DeviceRelease>,
    /// Outgoing: the journal intent of the Add.
    #[serde(default)]
    pub intent_id: Option<String>,
    pub created_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupDeviceRec {
    pub account: String,
    pub pk: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMsg {
    pub id: String,
    pub sender_account: String,
    pub sender_device: String,
    pub epoch: u64,
    pub text: String,
    pub at_ms: u64,
    pub outgoing: bool,
}

// ---- typed intents, queries, events ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupIntent {
    Create {
        name: String,
        relationship_ids: Vec<String>,
    },
    /// Create a Room from a protection profile (1 Recommended, 2 Hardened, 3 Maximum Privacy). `Create` is
    /// Recommended: one-tap join, anyone can add people, no history for new members, disappearing messages off.
    CreateWithProtection {
        name: String,
        relationship_ids: Vec<String>,
        protection: u64,
    },
    /// Change one Room setting, or move the whole Room to a protection profile (one Commit per field that differs, one
    /// after another). Tightening needs only `room.manage_security_policy` (or the admission or role capability the
    /// field needs); anything that makes the Room less strict is refused with `loosening_needs_confirmation` unless
    /// `confirmed` is true, and every member is told what changed and who changed it (ADR-0189 section 3).
    RoomSetting {
        conversation_id: Vec<u8>,
        setting: Setting,
        confirmed: bool,
    },
    Invite {
        conversation_id: Vec<u8>,
        relationship_id: String,
    },
    Accept {
        invite_id: String,
        /// Releases of this person's other devices (own-device sync delivers them).
        siblings: Vec<DeviceRelease>,
    },
    Decline {
        invite_id: String,
    },
    Leave {
        conversation_id: Vec<u8>,
    },
    Remove {
        conversation_id: Vec<u8>,
        account: Vec<u8>,
    },
    SendText {
        conversation_id: Vec<u8>,
        text: String,
    },
    /// The host reports that the prepared Commit was published to its group.
    CommitPublished {
        intent_id: String,
    },
    /// The ordering profile accepted the candidate (ADR-0163 ACCEPTED).
    CommitAccepted {
        intent_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupQuery {
    Groups,
    /// The announced changes of a Room's settings, oldest first.
    Notices {
        conversation_id: Vec<u8>,
    },
    Group {
        conversation_id: Vec<u8>,
    },
    Invites,
    Messages {
        conversation_id: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub conversation_id: String,
    pub name: String,
    pub members: usize,
    pub invited: usize,
    pub epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberView {
    pub account: String,
    pub roles: Vec<String>,
    /// `member` or `invited` (an invited person is shown, but is not in the group).
    pub status: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDetail {
    pub summary: GroupSummary,
    pub people: Vec<MemberView>,
    pub can_send: bool,
    pub protection: ProtectionView,
}

/// The Room's current protection fields in codes (the client words them; `custom` shows "Custom").
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionView {
    pub profile: u64,
    pub custom: bool,
    pub join_rule: u64,
    pub new_device_policy: u64,
    pub history_policy: u64,
    pub members_may_invite: bool,
    pub disappearing_timer: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupView {
    Groups(Vec<GroupSummary>),
    Notices(Vec<Notice>),
    Group(Box<GroupDetail>),
    Invites(Vec<GroupInviteRec>),
    Messages(Vec<GroupMsg>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent {
    Created {
        conversation_id: Vec<u8>,
    },
    InviteSent {
        invite_id: String,
    },
    InviteReceived {
        invite_id: String,
    },
    InviteAccepted {
        invite_id: String,
    },
    InviteDeclined {
        invite_id: String,
    },
    /// A Commit was prepared (journal INTENT_DURABLE to CANDIDATE_PREPARED); publish it, then report it.
    CommitPrepared {
        intent_id: String,
    },
    /// The Add could not be prepared yet (for example `not_authorized`); the invitation stays accepted.
    AdmitBlocked {
        invite_id: String,
        reason: &'static str,
    },
    MemberAdded {
        conversation_id: Vec<u8>,
        account: Vec<u8>,
    },
    MemberRemoved {
        conversation_id: Vec<u8>,
        account: Vec<u8>,
    },
    Joined {
        conversation_id: Vec<u8>,
    },
    /// This person is no longer in the group (left, or removed).
    NoLongerMember {
        conversation_id: Vec<u8>,
    },
    MessageSent {
        conversation_id: Vec<u8>,
        id: String,
    },
    MessageReceived {
        conversation_id: Vec<u8>,
        id: String,
        sender_account: Vec<u8>,
    },
    /// Something arrived that nothing of ours precedes or authorizes; nothing changed.
    Dropped(&'static str),
    /// A frame names a device of an account whose head this device lacks or holds stale: it is kept (bounded,
    /// expiring) and retried when a head arrives. Nothing was accepted (report D37).
    Held(&'static str),
    /// A Room setting changed (also on this device): who, and what, with each loosening marked. Never silent.
    PolicyChanged {
        conversation_id: Vec<u8>,
        by_account: Vec<u8>,
        changes: Vec<Change>,
    },
    /// A queued Room setting could not be carried out (for example the person lost the capability meanwhile).
    RoomSettingBlocked {
        conversation_id: Vec<u8>,
        reason: &'static str,
    },
    /// This device's own Commit lost a same-parent race, or its parent moved (GROUP_COMMIT section 3.2 rule 3): the
    /// branch is `SUPERSEDED`, nothing of it was merged, and a still-applicable intent is revalidated by the caller.
    CommitSuperseded {
        intent_id: String,
    },
}

// ---- wire (Core-internal, not frozen) ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum GroupWire {
    /// Over the 1:1: names the Room and the inviter; reveals the invitee to nobody.
    Invite {
        invite_id: String,
        conversation_id: String,
        name: String,
    },
    /// Over the 1:1: consent, and the KeyPackages released for that Room only.
    Accept {
        invite_id: String,
        releases: Vec<DeviceRelease>,
    },
    Decline {
        invite_id: String,
    },
    /// Over the 1:1, after the Add was accepted: the Welcome, the head state and the device directory.
    Welcome {
        invite_id: String,
        welcome: String,
        state: String,
        directory: Vec<(String, String, String)>,
    },
    /// To the group's devices: the Commit, its signed binding and the intent it carries (D-g13).
    Commit {
        conversation_id: String,
        committer_device: String,
        /// The device that authored the intent (the committer for admit/remove; the leaver for a leave). The
        /// committer's signed binding vouches for it; a signed intent envelope would replace the voucher (gap).
        actor_device: String,
        signed_binding: String,
        commit: String,
        intent: String,
        /// `(device, account, pk)` of devices this Commit adds.
        added: Vec<(String, String, String)>,
        /// The acting device's `signed-GroupControlIntent` (type 29) the frame carries (ADR-0189 1.4(a)); absent in
        /// the flow-level tests that drive the flow without carriage.
        #[serde(default)]
        signed_intent: Option<String>,
    },
    /// To a member device that can commit a reduction for the sender: a self-leave (never self-committed).
    LeaveRequest {
        conversation_id: String,
        intent: String,
        /// The leaver's `signed-GroupControlIntent`.
        #[serde(default)]
        signed: Option<String>,
    },
    /// To a roster device: this device's fresh per-Room route after a removal (ADR-0189 1.3). Handled by the
    /// carriage layer, never by the flow.
    RouteUpdate {
        conversation_id: String,
        routes: Vec<String>,
    },
    /// One MLS application ciphertext; delivery wraps it once per recipient device.
    App {
        conversation_id: String,
        ciphertext: String,
    },
}

impl GroupWire {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("wire serializes")
    }
    pub fn from_bytes(b: &[u8]) -> R<Self> {
        serde_json::from_slice(b).map_err(|_| GroupError("malformed_wire"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Over the 1:1 relationship (the host fans out to the person's devices).
    Relationship(String),
    /// To one member device through its own delivery capability (per-device envelope, fresh outer wrapper).
    Device { account: Vec<u8>, device: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbound {
    pub to: Target,
    pub wire: GroupWire,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupReply {
    pub events: Vec<GroupEvent>,
    pub out: Vec<Outbound>,
}

/// Who a wire arrived from, as far as the transport authenticated it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sender {
    Relationship(String),
    Device(Vec<u8>),
}

/// Everything a flow step may use besides `AppState`.
pub struct GroupCtx<'a> {
    pub engine: &'a mut dyn GroupCrypto,
    pub rng: &'a mut dyn RngCore,
    pub now_ms: u64,
    pub signer: &'a DeviceSigner,
    pub my_account: Vec<u8>,
}

macro_rules! env {
    ($c:expr) => {
        &mut CryptoEnv {
            rng: &mut *$c.rng,
            now_ms: $c.now_ms,
        }
    };
}

// ---- intent bodies -----------------------------------------------------------------------------------------------

fn m(e: Vec<(u64, Value)>) -> Value {
    Value::Map(e.into_iter().map(|(k, v)| (Value::Uint(k), v)).collect())
}

fn p_val(p: &Principal) -> Value {
    m(vec![
        (0, Value::Uint(p.kind)),
        (1, Value::Bytes(p.id.clone())),
    ])
}

fn intent_body(
    parent: &GroupState,
    id: &[u8],
    operation: u64,
    auth_kind: u64,
    args: Value,
) -> Vec<u8> {
    let reducing = matches!(operation, op::MEMBER_REMOVE | op::MEMBER_LEAVE);
    encode(&m(vec![
        (0, Value::Uint(29)),
        (1, Value::Uint(1)),
        (2, Value::Bytes(parent.conversation_id.clone())),
        (
            3,
            m(vec![
                (0, Value::Uint(1)),
                (1, Value::Bytes(parent.hash().to_vec())),
            ]),
        ),
        (4, Value::Uint(parent.state_version)),
        (5, Value::Bytes(id.to_vec())),
        (6, Value::Uint(operation)),
        (7, Value::Bool(reducing)),
        (8, args),
        (9, m(vec![(0, Value::Uint(auth_kind))])),
    ]))
}

fn target_args(who: &Principal, incarnation: &[u8]) -> Value {
    m(vec![
        (0, p_val(who)),
        (1, Value::Bytes(incarnation.to_vec())),
    ])
}

fn rand16(rng: &mut dyn RngCore) -> Vec<u8> {
    fill(rng, 16)
}

// ---- the flow ----------------------------------------------------------------------------------------------------

impl AppState {
    fn group_state(&self, conv: &[u8]) -> R<GroupState> {
        self.group(conv)?.ok_or(GroupError("unknown_group"))
    }

    fn group_dir(&self, conv: &[u8]) -> BTreeMap<String, GroupDeviceRec> {
        self.group_devices
            .get(&h(conv))
            .cloned()
            .unwrap_or_default()
    }

    /// Devices (hex ids) the directory holds for an account in a group.
    fn group_devices_of(&self, conv: &[u8], account: &[u8]) -> Vec<Vec<u8>> {
        self.group_dir(conv)
            .iter()
            .filter(|(_, r)| r.account == h(account))
            .filter_map(|(d, _)| unhex(d).ok())
            .collect()
    }

    fn group_register(&mut self, conv: &[u8], device: &[u8], account: &[u8], pk: &[u8]) {
        self.group_devices.entry(h(conv)).or_default().insert(
            h(device),
            GroupDeviceRec {
                account: h(account),
                pk: h(pk),
            },
        );
    }

    /// A relationship we may carry a group invitation over: `Active` only (ACCEPTED_UNBOUND is not enough, because
    /// the peer's account is not bound yet), not blocked. Returns the peer's account.
    fn group_active_peer(&self, relationship_id: &str) -> R<Vec<u8>> {
        let r = self
            .relationships
            .get(relationship_id)
            .ok_or(GroupError("unknown_relationship"))?;
        if r.state != RelState::Active || r.blocked {
            return err("relationship_not_active");
        }
        un(&r.peer_account_id).map_err(|_| GroupError("relationship_not_active"))
    }

    fn group_commit_wires(
        &self,
        conv: &[u8],
        my_device: &[u8],
        intent_id: &str,
        actor_device: &[u8],
        added: Vec<(String, String, String)>,
    ) -> R<Vec<Outbound>> {
        let e = self
            .group_journal
            .get(intent_id)
            .ok_or(GroupError("unknown_intent"))?;
        let c = e
            .candidate
            .as_ref()
            .ok_or(GroupError("bad_journal_state"))?;
        let wire = GroupWire::Commit {
            conversation_id: h(conv),
            committer_device: h(my_device),
            actor_device: h(actor_device),
            signed_binding: c.signed_binding.clone(),
            commit: c.commit.clone(),
            intent: e.intent_body.clone(),
            added,
            signed_intent: e.signed_intent.clone(),
        };
        Ok(self
            .group_dir(conv)
            .iter()
            .filter(|(d, _)| **d != h(my_device))
            .filter_map(|(d, r)| {
                Some(Outbound {
                    to: Target::Device {
                        account: unhex(&r.account).ok()?,
                        device: unhex(d).ok()?,
                    },
                    wire: wire.clone(),
                })
            })
            .collect())
    }

    /// Begin and prepare one intent; returns the journal id.
    fn group_prepare_intent(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        body: &[u8],
        plan: &CommitPlan,
        actor: &[u8],
        signed: Option<Vec<u8>>,
    ) -> R<String> {
        let e = self.group_journal_begin(body, actor)?;
        // The intent travels as the signed envelope of the acting device (ADR-0189 1.4(a)): this device signs its own
        // (admit, remove); a leave is forwarded as the leaver signed it.
        let signed = match signed {
            Some(s) => s,
            None => {
                let rnd = crate::crypto::rng::arr32(&mut *ctx.rng);
                super::carriage::sign_intent(
                    ctx.signer,
                    ctx.engine
                        .device_id()
                        .try_into()
                        .map_err(|_| GroupError("record_corrupt"))?,
                    body,
                    rnd,
                )
            }
        };
        if let Some(j) = self.group_journal.get_mut(&e.intent_id) {
            j.signed_intent = Some(h(&signed));
        }
        let ok = self.group_journal_prepare(
            &e.intent_id,
            plan,
            KIND_COORDINATOR_SEQUENCED,
            &mut *ctx.engine,
            env!(ctx),
            ctx.signer,
        );
        match ok {
            Ok(_) => Ok(e.intent_id),
            Err(x) => {
                // never leave an unpreparable intent open (it would block the group with `journal_busy`)
                let _ =
                    self.group_journal_abandon(&e.intent_id, &mut *ctx.engine, "prepare_failed");
                Err(x)
            }
        }
    }

    /// Execute one typed intent.
    pub fn group_do(&mut self, ctx: &mut GroupCtx<'_>, intent: GroupIntent) -> R<GroupReply> {
        let mut rep = GroupReply::default();
        match intent {
            GroupIntent::Create {
                name,
                relationship_ids,
            } => self.group_create(ctx, &name, &relationship_ids, room::RECOMMENDED, &mut rep)?,
            GroupIntent::CreateWithProtection {
                name,
                relationship_ids,
                protection,
            } => self.group_create(ctx, &name, &relationship_ids, protection, &mut rep)?,
            GroupIntent::RoomSetting {
                conversation_id,
                setting,
                confirmed,
            } => self.group_room_setting(ctx, &conversation_id, setting, confirmed, &mut rep)?,
            GroupIntent::Invite {
                conversation_id,
                relationship_id,
            } => self.group_invite(ctx, &conversation_id, &relationship_id, &mut rep)?,
            GroupIntent::Accept {
                invite_id,
                siblings,
            } => self.group_accept(ctx, &invite_id, siblings, &mut rep)?,
            GroupIntent::Decline { invite_id } => self.group_decline(&invite_id, &mut rep)?,
            GroupIntent::Leave { conversation_id } => {
                self.group_leave(ctx, &conversation_id, &mut rep)?
            }
            GroupIntent::Remove {
                conversation_id,
                account,
            } => self.group_remove(ctx, &conversation_id, &account, &mut rep)?,
            GroupIntent::SendText {
                conversation_id,
                text,
            } => self.group_send_text(ctx, &conversation_id, &text, &mut rep)?,
            GroupIntent::CommitPublished { intent_id } => {
                self.group_journal_published(&intent_id)?
            }
            GroupIntent::CommitAccepted { intent_id } => {
                self.group_commit_accepted(ctx, &intent_id, &mut rep)?
            }
        }
        Ok(rep)
    }

    fn group_create(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        name: &str,
        rels: &[String],
        protection: u64,
        rep: &mut GroupReply,
    ) -> R<()> {
        room::bundle(protection)?; // reserved values are refused before anything is created
        // Every chosen person must be eligible before anything is created.
        let mut accounts = BTreeSet::new();
        for r in rels {
            let a = self.group_active_peer(r)?;
            if a == ctx.my_account || !accounts.insert(a) {
                return err("duplicate_person");
            }
        }
        if accounts.len() + 1 > super::PRODUCT_MEMBER_CAP {
            return err("group_full");
        }
        let conv = fill(ctx.rng, 32);
        let gid = fill(ctx.rng, 24);
        let g = genesis(&GenesisParams {
            conversation_id: conv.clone(),
            creator: Principal::account(ctx.my_account.clone()),
            creator_incarnation: rand16(ctx.rng),
            creator_device: ctx.engine.device_id().to_vec(),
            admin_role_id: rand16(ctx.rng),
            member_role_id: rand16(ctx.rng),
            name: name.to_string(),
            protection,
        })?;
        mls(ctx.engine.create_group(env!(ctx), &gid, &conv))?;
        self.group_install_genesis(&g)?;
        self.group_bind_mls_id(&conv, &gid)?;
        let (dev, acct, pk) = (
            ctx.engine.device_id().to_vec(),
            ctx.my_account.clone(),
            ctx.signer.public(),
        );
        self.group_register(&conv, &dev, &acct, &pk);
        self.group_crypto_save(&*ctx.engine);
        rep.events.push(GroupEvent::Created {
            conversation_id: conv.clone(),
        });
        for r in rels {
            self.group_invite(ctx, &conv, r, rep)?;
        }
        Ok(())
    }

    fn group_invite(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        rel: &str,
        rep: &mut GroupReply,
    ) -> R<()> {
        let g = self.group_state(conv)?;
        let me = Principal::account(ctx.my_account.clone());
        // D-g6: inviting needs `room.invite` (registry operation control.member.invite).
        if !g.capabilities_of(&me).contains("room.invite") {
            return err("not_authorized");
        }
        let peer = self.group_active_peer(rel)?;
        if g.member(&Principal::account(peer.clone())).is_some() {
            return err("already_member");
        }
        let live: Vec<&GroupInviteRec> = self
            .group_invites
            .values()
            .filter(|i| {
                i.dir == InviteDir::Outgoing
                    && i.conversation_id == h(conv)
                    && matches!(
                        i.state,
                        InviteState::Offered | InviteState::Accepted | InviteState::Admitting
                    )
            })
            .collect();
        if live.iter().any(|i| i.peer_account == h(&peer)) {
            return err("already_invited");
        }
        if g.membership.len() + live.len() + 1 > super::PRODUCT_MEMBER_CAP {
            return err("group_full");
        }
        let id = h(&rand16(ctx.rng));
        self.group_invites.insert(
            id.clone(),
            GroupInviteRec {
                id: id.clone(),
                conversation_id: h(conv),
                group_name: g.profile.name.clone(),
                dir: InviteDir::Outgoing,
                state: InviteState::Offered,
                relationship_id: rel.to_string(),
                peer_account: h(&peer),
                releases: vec![],
                intent_id: None,
                created_ms: ctx.now_ms,
            },
        );
        rep.events.push(GroupEvent::InviteSent {
            invite_id: id.clone(),
        });
        rep.out.push(Outbound {
            to: Target::Relationship(rel.to_string()),
            wire: GroupWire::Invite {
                invite_id: id,
                conversation_id: h(conv),
                name: g.profile.name,
            },
        });
        Ok(())
    }

    fn group_accept(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        invite_id: &str,
        siblings: Vec<DeviceRelease>,
        rep: &mut GroupReply,
    ) -> R<()> {
        let inv = self
            .group_invites
            .get(invite_id)
            .cloned()
            .ok_or(GroupError("unknown_invite"))?;
        if inv.dir != InviteDir::Incoming || inv.state != InviteState::Offered {
            return err("bad_invite_state");
        }
        // The relationship must still be Active and still be the inviter.
        if self.group_active_peer(&inv.relationship_id)? != un(&inv.peer_account)? {
            return err("relationship_not_active");
        }
        let kp = mls(ctx.engine.key_package(env!(ctx)))?;
        let mut releases = vec![DeviceRelease {
            device: h(ctx.engine.device_id()),
            key_package: h(&kp.bytes),
            pk: h(&ctx.signer.public()),
        }];
        releases.extend(siblings);
        self.group_crypto_save(&*ctx.engine);
        let i = self.group_invites.get_mut(invite_id).expect("checked");
        i.state = InviteState::Accepted;
        i.releases = releases.clone();
        rep.events.push(GroupEvent::InviteAccepted {
            invite_id: invite_id.to_string(),
        });
        rep.out.push(Outbound {
            to: Target::Relationship(inv.relationship_id),
            wire: GroupWire::Accept {
                invite_id: invite_id.to_string(),
                releases,
            },
        });
        Ok(())
    }

    fn group_decline(&mut self, invite_id: &str, rep: &mut GroupReply) -> R<()> {
        let i = self
            .group_invites
            .get_mut(invite_id)
            .ok_or(GroupError("unknown_invite"))?;
        if i.dir != InviteDir::Incoming || i.state != InviteState::Offered {
            return err("bad_invite_state");
        }
        i.state = InviteState::Declined;
        let rel = i.relationship_id.clone();
        rep.events.push(GroupEvent::InviteDeclined {
            invite_id: invite_id.to_string(),
        });
        rep.out.push(Outbound {
            to: Target::Relationship(rel),
            wire: GroupWire::Decline {
                invite_id: invite_id.to_string(),
            },
        });
        Ok(())
    }

    /// The Add for an accepted invitation (`member.admit`, then an MLS Add of the released devices).
    fn group_admit(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        invite_id: &str,
        rep: &mut GroupReply,
    ) -> R<()> {
        let inv = self
            .group_invites
            .get(invite_id)
            .cloned()
            .ok_or(GroupError("unknown_invite"))?;
        let conv = un(&inv.conversation_id)?;
        let g = self.group_state(&conv)?;
        let member_role = g
            .role_definitions
            .iter()
            .find(|r| r.display_name == "Member")
            .map(|r| r.id.clone())
            .ok_or(GroupError("no_member_role"))?;
        let who = Principal::account(un(&inv.peer_account)?);
        let args = m(vec![
            (0, p_val(&who)),
            (1, Value::Bytes(rand16(ctx.rng))),
            (2, Value::Array(vec![Value::Bytes(member_role)])),
            (3, Value::Uint(1)),
        ]);
        let body = intent_body(&g, &rand16(ctx.rng), op::MEMBER_ADMIT, 1, args);
        let kps = inv
            .releases
            .iter()
            .map(|r| un(&r.key_package))
            .collect::<R<Vec<_>>>()?;
        let id = self.group_prepare_intent(
            ctx,
            &body,
            &CommitPlan::Add(kps),
            &ctx.my_account.clone(),
            None,
        )?;
        let added = inv
            .releases
            .iter()
            .map(|r| (r.device.clone(), inv.peer_account.clone(), r.pk.clone()))
            .collect();
        let my_dev = ctx.engine.device_id().to_vec();
        rep.out
            .extend(self.group_commit_wires(&conv, &my_dev, &id, &my_dev, added)?);
        let i = self.group_invites.get_mut(invite_id).expect("checked");
        i.state = InviteState::Admitting;
        i.intent_id = Some(id.clone());
        rep.events
            .push(GroupEvent::CommitPrepared { intent_id: id });
        Ok(())
    }

    fn group_leave(&mut self, ctx: &mut GroupCtx<'_>, conv: &[u8], rep: &mut GroupReply) -> R<()> {
        let g = self.group_state(conv)?;
        let me = Principal::account(ctx.my_account.clone());
        let inc = g
            .member(&me)
            .ok_or(GroupError("not_a_member"))?
            .incarnation
            .clone();
        let body = intent_body(
            &g,
            &rand16(ctx.rng),
            op::MEMBER_LEAVE,
            3,
            target_args(&me, &inc),
        );
        // D-g14: nobody commits their own removal; the coordinator (first of the succession) sequences it.
        let dir = self.group_dir(conv);
        let my_dev = h(ctx.engine.device_id());
        let to = g
            .coordinator_succession
            .iter()
            .map(|d| h(d))
            .find(|d| *d != my_dev && dir.contains_key(d))
            .or_else(|| dir.keys().find(|d| **d != my_dev).cloned())
            .ok_or(GroupError("no_committer"))?;
        let account = unhex(&dir[&to].account).map_err(|_| GroupError("record_corrupt"))?;
        self.group_leaving.insert(h(conv));
        rep.out.push(Outbound {
            to: Target::Device {
                account,
                device: un(&to)?,
            },
            wire: GroupWire::LeaveRequest {
                conversation_id: h(conv),
                intent: h(&body),
                signed: {
                    let rnd = crate::crypto::rng::arr32(&mut *ctx.rng);
                    let dev: [u8; 16] = ctx
                        .engine
                        .device_id()
                        .try_into()
                        .map_err(|_| GroupError("record_corrupt"))?;
                    Some(h(&super::carriage::sign_intent(
                        ctx.signer, &dev, &body, rnd,
                    )))
                },
            },
        });
        Ok(())
    }

    fn group_remove(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        account: &[u8],
        rep: &mut GroupReply,
    ) -> R<()> {
        let g = self.group_state(conv)?;
        let who = Principal::account(account.to_vec());
        let inc = g
            .member(&who)
            .ok_or(GroupError("not_a_member"))?
            .incarnation
            .clone();
        let body = intent_body(
            &g,
            &rand16(ctx.rng),
            op::MEMBER_REMOVE,
            1,
            target_args(&who, &inc),
        );
        {
            let (me, dev) = (ctx.my_account.clone(), ctx.engine.device_id().to_vec());
            self.group_reduce(ctx, conv, &body, account, &me, &dev, None, rep)
        }
    }

    /// Prepare a reduction (remove or leave) of `account`'s devices and fan the Commit out to everyone, the removed
    /// devices included (they must learn it).
    #[allow(clippy::too_many_arguments)]
    fn group_reduce(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        body: &[u8],
        account: &[u8],
        actor: &[u8],
        actor_device: &[u8],
        signed: Option<Vec<u8>>,
        rep: &mut GroupReply,
    ) -> R<()> {
        let devs = self.group_devices_of(conv, account);
        if devs.is_empty() {
            return err("unknown_member_devices");
        }
        let id = self.group_prepare_intent(ctx, body, &CommitPlan::Remove(devs), actor, signed)?;
        let my_dev = ctx.engine.device_id().to_vec();
        rep.out
            .extend(self.group_commit_wires(conv, &my_dev, &id, actor_device, vec![])?);
        rep.events
            .push(GroupEvent::CommitPrepared { intent_id: id });
        Ok(())
    }

    fn group_commit_accepted(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        intent_id: &str,
        rep: &mut GroupReply,
    ) -> R<()> {
        let e = self
            .group_journal
            .get(intent_id)
            .cloned()
            .ok_or(GroupError("unknown_intent"))?;
        let conv = un(&e.conversation_id)?;
        let before = self.group_state(&conv)?;
        let after = self.group_journal_accept(intent_id, &mut *ctx.engine)?;
        let actor = un(&e.actor_account)?;
        self.group_after_change(&conv, &before, &after, &actor, ctx.now_ms, rep);
        // An accepted Add releases the Welcome over the 1:1 (the head state and the directory travel with it).
        let inv = self
            .group_invites
            .values()
            .find(|i| i.intent_id.as_deref() == Some(intent_id))
            .cloned();
        if let Some(inv) = inv {
            let cand = e
                .candidate
                .as_ref()
                .ok_or(GroupError("bad_journal_state"))?;
            let welcome = cand.welcome.clone().ok_or(GroupError("record_corrupt"))?;
            for r in &inv.releases {
                self.group_register(&conv, &un(&r.device)?, &un(&inv.peer_account)?, &un(&r.pk)?);
            }
            let directory = self
                .group_dir(&conv)
                .into_iter()
                .map(|(d, r)| (d, r.account, r.pk))
                .collect();
            self.group_invites.get_mut(&inv.id).expect("found").state = InviteState::Done;
            rep.out.push(Outbound {
                to: Target::Relationship(inv.relationship_id),
                wire: GroupWire::Welcome {
                    invite_id: inv.id,
                    welcome,
                    state: h(&after.encode()),
                    directory,
                },
            });
        }
        // a queued Room setting goes on with its next Commit now that the journal is free
        if let Err(x) = self.group_room_advance(ctx, &conv, rep) {
            rep.events.push(GroupEvent::RoomSettingBlocked {
                conversation_id: conv.clone(),
                reason: x.0,
            });
        }
        self.group_admit_waiting(ctx, &conv, rep);
        Ok(())
    }

    /// An accepted invitation whose Add could not be prepared because another Commit was open (`journal_busy`) is
    /// admitted now that the journal is free: one Commit at a time, each accepted at once (ADR-0189 section 2).
    fn group_admit_waiting(&mut self, ctx: &mut GroupCtx<'_>, conv: &[u8], rep: &mut GroupReply) {
        let key = h(conv);
        if self
            .group_journal
            .values()
            .any(|e| e.conversation_id == key && !e.state.is_terminal())
        {
            return;
        }
        let waiting = self
            .group_invites
            .values()
            .find(|i| {
                i.dir == InviteDir::Outgoing
                    && i.state == InviteState::Accepted
                    && i.conversation_id == key
                    && i.intent_id.is_none()
            })
            .map(|i| i.id.clone());
        if let Some(id) = waiting
            && let Err(x) = self.group_admit(ctx, &id, rep)
        {
            rep.events.push(GroupEvent::AdmitBlocked {
                invite_id: id,
                reason: x.0,
            });
        }
    }

    /// The next step of a Room setting, from the state `g`: the single operation and its arguments, or `None` when `g`
    /// already satisfies the setting.
    fn room_next_op(g: &GroupState, s: &Setting) -> R<Option<(u64, Value)>> {
        let policy_args = |a: Vec<(u64, Value)>| (op::SECURITY_POLICY_SET, m(a));
        let role = |want: bool| -> R<(u64, Value)> {
            let mut r = room::member_role(g)
                .cloned()
                .ok_or(GroupError("no_member_role"))?;
            r.grants.retain(|c| c != "room.invite");
            if want {
                r.grants.push("room.invite".into());
            }
            Ok((op::ROLE_DEFINE, m(vec![(0, r.value())])))
        };
        let p = &g.policy;
        Ok(match s {
            Setting::Protection(x) => {
                let b = room::bundle(*x)?;
                if p.join_rule != b.join_rule {
                    Some((op::ADMISSION_SET, m(vec![(0, Value::Uint(b.join_rule))])))
                } else {
                    let mut a = vec![];
                    if p.new_device_policy != b.new_device_policy {
                        a.push((0, Value::Uint(b.new_device_policy)));
                    }
                    if p.history_policy != b.history_policy {
                        a.push((1, Value::Uint(b.history_policy)));
                    }
                    if p.security_profile != b.security_profile {
                        a.push((3, Value::Uint(b.security_profile)));
                    }
                    if !a.is_empty() {
                        Some(policy_args(a))
                    } else if room::members_may_invite(g) != b.members_may_invite {
                        Some(role(b.members_may_invite)?)
                    } else {
                        None
                    }
                }
            }
            Setting::JoinRule(v) => {
                (p.join_rule != *v).then(|| (op::ADMISSION_SET, m(vec![(0, Value::Uint(*v))])))
            }
            Setting::NewDevicePolicy(v) => {
                (p.new_device_policy != *v).then(|| policy_args(vec![(0, Value::Uint(*v))]))
            }
            Setting::HistoryPolicy(v) => {
                if *v == 2 {
                    return err("history_window_required");
                }
                (p.history_policy != *v).then(|| policy_args(vec![(1, Value::Uint(*v))]))
            }
            Setting::MembersMayInvite(x) => (room::members_may_invite(g) != *x)
                .then(|| role(*x))
                .transpose()?,
            Setting::DisappearingTimer(t) => (p.disappearing_timer != *t).then(|| {
                (
                    op::DISAPPEARING_TIMER_SET,
                    m(vec![(0, Value::Uint(t.unwrap_or(0)))]),
                )
            }),
        })
    }

    /// Change a Room setting (see [`GroupIntent::RoomSetting`]). The whole sequence is simulated against the state
    /// first: anything that would loosen the Room is refused without confirmation, and nothing is started unless every
    /// step would be authorized.
    fn group_room_setting(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        setting: Setting,
        confirmed: bool,
        rep: &mut GroupReply,
    ) -> R<()> {
        let g = self.group_state(conv)?;
        let me = Principal::account(ctx.my_account.clone());
        if g.member(&me).is_none() {
            return err("not_a_member");
        }
        let mut cur = g.clone();
        for _ in 0..6 {
            let Some((operation, args)) = Self::room_next_op(&cur, &setting)? else {
                break;
            };
            let body = intent_body(&cur, &[0u8; 16], operation, 1, args);
            cur = apply(&cur, &Intent::decode(&body)?, &me, cur.mls_epoch_ref + 1)?;
        }
        if Self::room_next_op(&cur, &setting)?.is_some() {
            return err("setting_not_reachable");
        }
        let loosens = room::diff(&g, &cur).iter().any(|c| c.loosened);
        if loosens && !confirmed {
            return Err(room::needs_confirmation());
        }
        // D39: Core collects no governance quorum yet, so a loosening under a quorum is refused, never made alone.
        if loosens && g.governance.approval_threshold > 1 {
            return err("quorum_not_supported");
        }
        self.group_room_queue
            .entry(h(conv))
            .or_default()
            .push(setting);
        self.group_room_advance(ctx, conv, rep)
    }

    /// Prepare the next Commit of the queued Room settings of `conv`, if the journal is free. A setting the Room
    /// already satisfies leaves the queue.
    fn group_room_advance(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        rep: &mut GroupReply,
    ) -> R<()> {
        let key = h(conv);
        let Some(queue) = self.group_room_queue.get(&key).cloned() else {
            return Ok(());
        };
        if self
            .group_journal
            .values()
            .any(|e| e.conversation_id == key && !e.state.is_terminal())
        {
            return Ok(());
        }
        let g = self.group_state(conv)?;
        let mut keep = vec![];
        let mut next = None;
        for s in queue {
            match Self::room_next_op(&g, &s)? {
                Some(o) if next.is_none() => {
                    next = Some(o);
                    keep.push(s);
                }
                Some(_) => keep.push(s),
                None => {}
            }
        }
        if keep.is_empty() {
            self.group_room_queue.remove(&key);
        } else {
            self.group_room_queue.insert(key, keep);
        }
        let Some((operation, args)) = next else {
            return Ok(());
        };
        let body = intent_body(&g, &rand16(ctx.rng), operation, 1, args);
        let me = ctx.my_account.clone();
        let id = self.group_prepare_intent(ctx, &body, &CommitPlan::Update, &me, None)?;
        let my_dev = ctx.engine.device_id().to_vec();
        rep.out
            .extend(self.group_commit_wires(conv, &my_dev, &id, &my_dev, vec![])?);
        rep.events
            .push(GroupEvent::CommitPrepared { intent_id: id });
        Ok(())
    }

    /// Events and directory upkeep after a state change the canonical head went through.
    fn group_after_change(
        &mut self,
        conv: &[u8],
        before: &GroupState,
        after: &GroupState,
        by: &[u8],
        now_ms: u64,
        rep: &mut GroupReply,
    ) {
        // Every change of a Room setting is announced to every member and kept as a Room notice: never silent.
        let changes = room::diff(before, after);
        if !changes.is_empty() {
            let log = self.group_notices.entry(h(conv)).or_default();
            log.push(Notice {
                at_ms: now_ms,
                by_account: h(by),
                changes: changes.clone(),
            });
            let over = log.len().saturating_sub(MAX_NOTICES);
            log.drain(..over);
            rep.events.push(GroupEvent::PolicyChanged {
                conversation_id: conv.to_vec(),
                by_account: by.to_vec(),
                changes,
            });
        }
        let acc = |g: &GroupState| -> BTreeSet<Vec<u8>> {
            g.membership
                .iter()
                .map(|x| x.principal.id.clone())
                .collect()
        };
        let (b, a) = (acc(before), acc(after));
        for x in a.difference(&b) {
            rep.events.push(GroupEvent::MemberAdded {
                conversation_id: conv.to_vec(),
                account: x.clone(),
            });
        }
        for x in b.difference(&a) {
            if let Some(d) = self.group_devices.get_mut(&h(conv)) {
                d.retain(|_, r| r.account != h(x));
            }
            rep.events.push(GroupEvent::MemberRemoved {
                conversation_id: conv.to_vec(),
                account: x.clone(),
            });
        }
    }

    fn group_send_text(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        text: &str,
        rep: &mut GroupReply,
    ) -> R<()> {
        if text.is_empty() || text.len() > 8192 {
            return err("bad_text");
        }
        let g = self.group_state(conv)?;
        if self.group_leaving.contains(&h(conv)) {
            return err("leaving");
        }
        if !self.group_may_send(conv) {
            return err("commit_in_flight");
        }
        let me = Principal::account(ctx.my_account.clone());
        if !g.capabilities_of(&me).contains("room.post") {
            return err("not_authorized");
        }
        let gid = self.group_mls_id(conv)?;
        let id = h(&rand16(ctx.rng));
        let plain =
            serde_json::to_vec(&serde_json::json!({"k":"text","id":id,"text":text})).expect("json");
        let ct = mls(ctx.engine.encrypt_application(env!(ctx), &gid, &plain))?;
        let epoch = mls(ctx.engine.epoch(&gid))?;
        self.group_crypto_save(&*ctx.engine);
        let my_dev = ctx.engine.device_id().to_vec();
        self.group_msgs.entry(h(conv)).or_default().push(GroupMsg {
            id: id.clone(),
            sender_account: h(&ctx.my_account),
            sender_device: h(&my_dev),
            epoch,
            text: text.to_string(),
            at_ms: ctx.now_ms,
            outgoing: true,
        });
        let wire = GroupWire::App {
            conversation_id: h(conv),
            ciphertext: h(&ct),
        };
        for (d, r) in self.group_dir(conv) {
            if d != h(&my_dev) {
                rep.out.push(Outbound {
                    to: Target::Device {
                        account: un(&r.account)?,
                        device: un(&d)?,
                    },
                    wire: wire.clone(),
                });
            }
        }
        rep.events.push(GroupEvent::MessageSent {
            conversation_id: conv.to_vec(),
            id,
        });
        Ok(())
    }

    /// Handle something that arrived. Unsolicited or unauthorized material is dropped with a [`GroupEvent::Dropped`]
    /// and changes nothing; malformed or failing material is an error.
    pub fn group_receive(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        from: Sender,
        wire: GroupWire,
    ) -> R<GroupReply> {
        let mut rep = GroupReply::default();
        match (wire, from) {
            (
                GroupWire::Invite {
                    invite_id,
                    conversation_id,
                    name,
                },
                Sender::Relationship(rel),
            ) => {
                let peer = self.group_active_peer(&rel)?;
                let open = self
                    .group_invites
                    .values()
                    .filter(|i| i.dir == InviteDir::Incoming && i.state == InviteState::Offered)
                    .count();
                if open >= MAX_OPEN_INCOMING || self.group_invites.contains_key(&invite_id) {
                    rep.events.push(GroupEvent::Dropped("invite_refused"));
                    return Ok(rep);
                }
                un(&conversation_id)?;
                self.group_invites.insert(
                    invite_id.clone(),
                    GroupInviteRec {
                        id: invite_id.clone(),
                        conversation_id,
                        group_name: name.chars().take(80).collect(),
                        dir: InviteDir::Incoming,
                        state: InviteState::Offered,
                        relationship_id: rel,
                        peer_account: h(&peer),
                        releases: vec![],
                        intent_id: None,
                        created_ms: ctx.now_ms,
                    },
                );
                rep.events.push(GroupEvent::InviteReceived { invite_id });
            }
            (
                GroupWire::Accept {
                    invite_id,
                    releases,
                },
                Sender::Relationship(rel),
            ) => {
                let ok = self.group_invites.get(&invite_id).is_some_and(|i| {
                    i.dir == InviteDir::Outgoing
                        && i.state == InviteState::Offered
                        && i.relationship_id == rel
                });
                if !ok || releases.is_empty() || releases.len() > 8 {
                    rep.events.push(GroupEvent::Dropped("unsolicited_accept"));
                    return Ok(rep);
                }
                let i = self.group_invites.get_mut(&invite_id).expect("checked");
                i.state = InviteState::Accepted;
                i.releases = releases;
                rep.events.push(GroupEvent::InviteAccepted {
                    invite_id: invite_id.clone(),
                });
                // D-g6: the Add needs `room.manage_admission` as well; if it is refused the invitation stays accepted.
                if let Err(x) = self.group_admit(ctx, &invite_id, &mut rep) {
                    rep.events.push(GroupEvent::AdmitBlocked {
                        invite_id,
                        reason: x.0,
                    });
                }
            }
            (GroupWire::Decline { invite_id }, Sender::Relationship(rel)) => {
                match self.group_invites.get_mut(&invite_id) {
                    Some(i)
                        if i.dir == InviteDir::Outgoing
                            && i.state == InviteState::Offered
                            && i.relationship_id == rel =>
                    {
                        i.state = InviteState::Declined;
                        rep.events.push(GroupEvent::InviteDeclined { invite_id });
                    }
                    _ => rep.events.push(GroupEvent::Dropped("unsolicited_decline")),
                }
            }
            (
                GroupWire::Welcome {
                    invite_id,
                    welcome,
                    state,
                    directory,
                },
                Sender::Relationship(rel),
            ) => self.group_welcome(
                ctx, &rel, &invite_id, &welcome, &state, &directory, &mut rep,
            )?,
            (
                GroupWire::Commit {
                    conversation_id,
                    committer_device,
                    actor_device,
                    signed_binding,
                    commit,
                    intent,
                    added,
                    ..
                },
                Sender::Device(_),
            ) => self.group_commit_in(
                ctx,
                &un(&conversation_id)?,
                &un(&committer_device)?,
                &un(&actor_device)?,
                &un(&signed_binding)?,
                &un(&commit)?,
                &un(&intent)?,
                &added,
                &mut rep,
            )?,
            (
                GroupWire::LeaveRequest {
                    conversation_id,
                    intent,
                    signed,
                },
                Sender::Device(dev),
            ) => {
                let conv = un(&conversation_id)?;
                let body = un(&intent)?;
                let Some(rec) = self.group_dir(&conv).get(&h(&dev)).cloned() else {
                    rep.events.push(GroupEvent::Dropped("unknown_sender"));
                    return Ok(rep);
                };
                let account = un(&rec.account)?;
                let i = Intent::decode(&body)?;
                // The request must be the sender's own leave.
                let own = i.operation == op::MEMBER_LEAVE
                    && i.args.get(0).and_then(|p| p.get(1)).and_then(|b| match b {
                        Value::Bytes(x) => Some(x.clone()),
                        _ => None,
                    }) == Some(account.clone());
                if !own {
                    rep.events.push(GroupEvent::Dropped("not_own_leave"));
                    return Ok(rep);
                }
                let signed = signed.as_deref().map(un).transpose()?;
                self.group_reduce(
                    ctx, &conv, &body, &account, &account, &dev, signed, &mut rep,
                )?;
            }
            (
                GroupWire::App {
                    conversation_id,
                    ciphertext,
                },
                Sender::Device(_),
            ) => self.group_app_in(ctx, &un(&conversation_id)?, &un(&ciphertext)?, &mut rep)?,
            _ => rep.events.push(GroupEvent::Dropped("wrong_channel")),
        }
        Ok(rep)
    }

    #[allow(clippy::too_many_arguments)]
    fn group_welcome(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        rel: &str,
        invite_id: &str,
        welcome: &str,
        state: &str,
        directory: &[(String, String, String)],
        rep: &mut GroupReply,
    ) -> R<()> {
        // A client drops a Welcome that no accepted invitation of its own precedes.
        let inv = self.group_invites.get(invite_id).cloned();
        let Some(inv) = inv.filter(|i| {
            i.dir == InviteDir::Incoming
                && i.state == InviteState::Accepted
                && i.relationship_id == rel
        }) else {
            rep.events.push(GroupEvent::Dropped("unsolicited_welcome"));
            return Ok(());
        };
        let st = GroupState::decode(&un(state)?)?;
        // The invitation no longer carries the ConversationID (ADR-0189 1.1): the Welcome's state names it.
        if !inv.conversation_id.is_empty() && h(&st.conversation_id) != inv.conversation_id {
            return err("wrong_conversation");
        }
        let me = Principal::account(ctx.my_account.clone());
        if st.member(&me).is_none()
            || st
                .member(&Principal::account(un(&inv.peer_account)?))
                .is_none()
        {
            return err("welcome_state_mismatch");
        }
        let (gid, info) = mls(ctx.engine.process_welcome(env!(ctx), &un(welcome)?))?;
        if info.epoch != st.mls_epoch_ref {
            // leave nothing half-installed: the engine joined, so drop its state again via a fresh save only on success
            return err("welcome_state_mismatch");
        }
        self.groups
            .insert(h(&st.conversation_id), super::GroupRec::of(&st));
        self.group_bind_mls_id(&st.conversation_id, &gid)?;
        for (d, a, pk) in directory {
            self.group_register(&st.conversation_id, &un(d)?, &un(a)?, &un(pk)?);
        }
        self.group_crypto_save(&*ctx.engine);
        let done = self.group_invites.get_mut(invite_id).expect("found");
        done.state = InviteState::Done;
        done.conversation_id = h(&st.conversation_id);
        rep.events.push(GroupEvent::Joined {
            conversation_id: st.conversation_id,
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn group_commit_in(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        committer: &[u8],
        actor_device: &[u8],
        signed: &[u8],
        commit: &[u8],
        intent: &[u8],
        added: &[(String, String, String)],
        rep: &mut GroupReply,
    ) -> R<()> {
        // D-g13: the committer is looked up in our own directory (its key verifies the binding); the acting account
        // is what our directory says the named actor device belongs to, never an account the sender asserts.
        let dir = self.group_dir(conv);
        let (Some(rec), Some(act)) = (dir.get(&h(committer)), dir.get(&h(actor_device))) else {
            rep.events.push(GroupEvent::Dropped("unknown_committer"));
            return Ok(());
        };
        let (account, pk) = (un(&act.account)?, un(&rec.pk)?);
        let before = self.group_state(conv)?;
        let out = self.group_receive_commit(
            signed,
            &pk,
            commit,
            intent,
            &account,
            &mut *ctx.engine,
            env!(ctx),
        )?;
        match out {
            ReceiveOutcome::Applied {
                still_member,
                superseded,
                ..
            } => {
                if let Some(intent_id) = superseded {
                    rep.events.push(GroupEvent::CommitSuperseded { intent_id });
                }
                if !still_member {
                    self.groups.remove(&h(conv));
                    self.group_devices.remove(&h(conv));
                    self.group_leaving.remove(&h(conv));
                    rep.events.push(GroupEvent::NoLongerMember {
                        conversation_id: conv.to_vec(),
                    });
                    return Ok(());
                }
                let after = self.group_state(conv)?;
                for (d, a, k) in added {
                    // only an account the Commit's own state change added may gain devices
                    let a = un(a)?;
                    if before.member(&Principal::account(a.clone())).is_none()
                        && after.member(&Principal::account(a.clone())).is_some()
                    {
                        self.group_register(conv, &un(d)?, &a, &un(k)?);
                    }
                }
                self.group_after_change(conv, &before, &after, &account, ctx.now_ms, rep);
            }
            ReceiveOutcome::Duplicate | ReceiveOutcome::OwnCandidate(_) | ReceiveOutcome::Loses => {
            }
        }
        Ok(())
    }

    fn group_app_in(
        &mut self,
        ctx: &mut GroupCtx<'_>,
        conv: &[u8],
        ct: &[u8],
        rep: &mut GroupReply,
    ) -> R<()> {
        let g = self.group_state(conv)?;
        let gid = self.group_mls_id(conv)?;
        let d = mls(ctx.engine.decrypt_application(&gid, ct))?;
        let Some(rec) = self.group_dir(conv).get(&h(&d.sender)).cloned() else {
            rep.events.push(GroupEvent::Dropped("unknown_sender"));
            return Ok(());
        };
        let account = un(&rec.account)?;
        // Authorized at the sending epoch: the head is that epoch (this engine keeps no older window yet).
        if !g
            .capabilities_of(&Principal::account(account.clone()))
            .contains("room.post")
        {
            rep.events
                .push(GroupEvent::Dropped("sender_not_authorized"));
            return Ok(());
        }
        let j: serde_json::Value =
            serde_json::from_slice(&d.plaintext).map_err(|_| GroupError("malformed_event"))?;
        let (Some("text"), Some(id), Some(text)) = (
            j["k"].as_str(),
            j["id"].as_str().map(str::to_string),
            j["text"].as_str().map(str::to_string),
        ) else {
            return err("malformed_event");
        };
        let log = self.group_msgs.entry(h(conv)).or_default();
        if log.iter().any(|x| x.id == id) {
            return Ok(());
        }
        log.push(GroupMsg {
            id: id.clone(),
            sender_account: rec.account,
            sender_device: h(&d.sender),
            epoch: d.epoch,
            text,
            at_ms: ctx.now_ms,
            outgoing: false,
        });
        rep.events.push(GroupEvent::MessageReceived {
            conversation_id: conv.to_vec(),
            id,
            sender_account: account,
        });
        Ok(())
    }

    /// Answer a typed query.
    pub fn group_query(&self, q: GroupQuery) -> R<GroupView> {
        let invited = |conv: &str| {
            self.group_invites
                .values()
                .filter(|i| {
                    i.dir == InviteDir::Outgoing
                        && i.conversation_id == conv
                        && matches!(
                            i.state,
                            InviteState::Offered | InviteState::Accepted | InviteState::Admitting
                        )
                })
                .collect::<Vec<_>>()
        };
        let summary = |g: &GroupState| GroupSummary {
            conversation_id: h(&g.conversation_id),
            name: g.profile.name.clone(),
            members: g.membership.len(),
            invited: invited(&h(&g.conversation_id)).len(),
            epoch: g.mls_epoch_ref,
        };
        match q {
            GroupQuery::Groups => {
                let mut v = Vec::new();
                for r in self.groups.values() {
                    v.push(summary(&r.open()?));
                }
                Ok(GroupView::Groups(v))
            }
            GroupQuery::Notices { conversation_id } => Ok(GroupView::Notices(
                self.group_notices
                    .get(&h(&conversation_id))
                    .cloned()
                    .unwrap_or_default(),
            )),
            GroupQuery::Group { conversation_id } => {
                let g = self.group_state(&conversation_id)?;
                let mut people: Vec<MemberView> = g
                    .membership
                    .iter()
                    .map(|mem| MemberView {
                        account: h(&mem.principal.id),
                        roles: mem
                            .role_ids
                            .iter()
                            .filter_map(|r| g.role_definitions.iter().find(|d| &d.id == r))
                            .map(|d| d.display_name.clone())
                            .collect(),
                        status: "member",
                    })
                    .collect();
                people.extend(
                    invited(&h(&conversation_id))
                        .into_iter()
                        .map(|i| MemberView {
                            account: i.peer_account.clone(),
                            roles: vec![],
                            status: "invited",
                        }),
                );
                Ok(GroupView::Group(Box::new(GroupDetail {
                    summary: summary(&g),
                    people,
                    can_send: self.group_may_send(&conversation_id)
                        && !self.group_leaving.contains(&h(&conversation_id)),
                    protection: ProtectionView {
                        profile: g.policy.security_profile,
                        custom: room::custom(&g),
                        join_rule: g.policy.join_rule,
                        new_device_policy: g.policy.new_device_policy,
                        history_policy: g.policy.history_policy,
                        members_may_invite: room::members_may_invite(&g),
                        disappearing_timer: g.policy.disappearing_timer,
                    },
                })))
            }
            GroupQuery::Invites => Ok(GroupView::Invites(
                self.group_invites
                    .values()
                    .filter(|i| i.dir == InviteDir::Incoming && i.state == InviteState::Offered)
                    .cloned()
                    .collect(),
            )),
            GroupQuery::Messages { conversation_id } => Ok(GroupView::Messages(
                self.group_msgs
                    .get(&h(&conversation_id))
                    .cloned()
                    .unwrap_or_default(),
            )),
        }
    }
}
