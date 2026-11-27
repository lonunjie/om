// SPDX-License-Identifier: MPL-2.0
//! Typed JSON queries for the services and bridges scopes (feature `scope-engines`, engineering artifact only).
//! Stateless: each call carries what it needs and returns a decision; no keys, no host state, no authority.

use om_core::bridge_fidelity::{Carry, Op, Plan, Support};
use om_core::room_snapshot::{
    HeadReference, Mutation, OperationId, OperationOutcome, OperationTarget, SnapshotError,
};
use om_core::service_broker::{EgressPolicy, Flow, Installation, Refusal, Tool};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FidelityOp {
    Text {
        rich: bool,
    },
    Attachment {
        bytes: u64,
    },
    Reply {
        target_mapped: bool,
    },
    Edit {
        source_tracked: bool,
        policy_allows: bool,
    },
    Delete,
    Reaction {
        target_mapped: bool,
    },
    Topic {
        compatible_target: bool,
    },
    Poll {
        compatible_privacy: bool,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EgressPolicyIn {
    Room { allowed: bool },
    OneToOne { me: bool, peer: bool },
}

fn carry_name(c: Carry) -> &'static str {
    match c {
        Carry::Native => "native",
        Carry::PlainText => "plain_text",
        Carry::NewEditedMessage => "new_edited_message",
        Carry::UnlinkedMessage => "unlinked_message",
        Carry::Flattened => "flattened",
        Carry::ReadOnlySummary => "read_only_summary",
        Carry::NotCarried => "not_carried",
    }
}

pub fn fidelity_plan(semantics: &[u8], max_attachment_bytes: Option<u64>, op: FidelityOp) -> Value {
    let support = Support {
        semantics: semantics.iter().copied().collect(),
        max_attachment_bytes,
    };
    let op = match op {
        FidelityOp::Text { rich } => Op::Text { rich },
        FidelityOp::Attachment { bytes } => Op::Attachment { bytes },
        FidelityOp::Reply { target_mapped } => Op::Reply { target_mapped },
        FidelityOp::Edit {
            source_tracked,
            policy_allows,
        } => Op::Edit {
            source_tracked,
            policy_allows,
        },
        FidelityOp::Delete => Op::Delete,
        FidelityOp::Reaction { target_mapped } => Op::Reaction { target_mapped },
        FidelityOp::Topic { compatible_target } => Op::Topic { compatible_target },
        FidelityOp::Poll { compatible_privacy } => Op::Poll { compatible_privacy },
    };
    let p: Plan = support.translate(op);
    json!({ "carry": carry_name(p.carry), "flags": p.flags, "claimsErasure": p.claims_erasure })
}

pub fn broker_request(
    granted: &[String],
    tainted: bool,
    policy: EgressPolicyIn,
    name: &str,
) -> Value {
    let tools: Vec<Tool> = granted.iter().filter_map(|g| Tool::parse(g)).collect();
    let ins = Installation::new(tools);
    let mut flow = Flow::default();
    if tainted {
        flow.read_plaintext();
    }
    let policy = match policy {
        EgressPolicyIn::Room { allowed } => EgressPolicy::Room { allowed },
        EgressPolicyIn::OneToOne { me, peer } => EgressPolicy::OneToOne { me, peer },
    };
    match flow.request(&ins, name, policy) {
        Ok(_) => json!({ "allowed": true }),
        Err(r) => json!({
            "allowed": false,
            "refusal": match r {
                Refusal::Forbidden => "forbidden",
                Refusal::UnknownTool => "unknown_tool",
                Refusal::NotGranted => "not_granted",
                Refusal::TaintedEgress => "tainted_egress",
            }
        }),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceDisclosure {
    service: String,
    command: String,
    destination: String,
    expires_at_ms: u64,
    items: Vec<DisclosureItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisclosureItem {
    id: String,
    kind: String,
    value: String,
}

pub fn service_invocation_review(disclosure: ServiceDisclosure, now_ms: u64) -> Value {
    let mut ids = BTreeSet::new();
    let valid = hex_n::<32>(&disclosure.service).is_some()
        && !disclosure.command.is_empty()
        && !disclosure.destination.trim().is_empty()
        && disclosure
            .items
            .iter()
            .filter(|item| item.kind == "trigger")
            .count()
            == 1
        && disclosure.items.iter().all(|item| {
            !item.id.is_empty()
                && ids.insert(&item.id)
                && matches!(
                    item.kind.as_str(),
                    "trigger"
                        | "message"
                        | "attachment"
                        | "draft"
                        | "thread_or_topic"
                        | "text"
                        | "participant"
                        | "scope"
                )
        });
    let review = json!([
        disclosure.service,
        disclosure.command,
        disclosure.destination,
        disclosure.expires_at_ms,
        disclosure
            .items
            .iter()
            .map(|i| [&i.id, &i.kind, &i.value])
            .collect::<Vec<_>>()
    ]);
    let target = lan_hex(&om_core::crypto::sha256(&[review.to_string().as_bytes()]));
    let egress = if disclosure.command == "fetch_url_via_scoped_network_provider" {
        "blocked_by_capability"
    } else {
        "not_requested"
    };
    // There is no authenticated grant source for engineering Room invocations.
    let result = broker_request(
        &[],
        true,
        EgressPolicyIn::Room { allowed: false },
        &disclosure.command,
    );
    json!({
        "target": target,
        "allowed": false,
        "refusal": if !valid { json!("malformed") } else if disclosure.expires_at_ms <= now_ms {
            json!("expired")
        } else { result["refusal"].clone() },
        "flow": { "tainted": true, "egress": egress },
    })
}

// ---- Room topics -------------------------------------------------------------------------------------------------

use om_core::room_topics::{Mode, RoomTopics, Stamp, TopicError, TopicId};
use std::collections::BTreeSet;

#[derive(Deserialize)]
pub struct StampIn {
    pub clock: u64,
    pub event: String,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TopicOp {
    Create {
        id: String,
        title: String,
        at: StampIn,
    },
    Update {
        id: String,
        title: String,
        at: StampIn,
    },
    SetStatus {
        id: String,
        status: u64,
        at: StampIn,
    },
    Post {
        mode: String,
        top_level: bool,
        event: String,
        author: String,
        topic: Option<String>,
    },
    Move {
        event: String,
        topic: Option<String>,
        at: StampIn,
    },
}

fn hex_n<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}
fn stamp(s: &StampIn) -> Option<Stamp> {
    Some((s.clock, hex_n::<32>(&s.event)?))
}
fn topic_err(e: TopicError) -> &'static str {
    match e {
        TopicError::NotAuthorized => "not_authorized",
        TopicError::Duplicate => "duplicate",
        TopicError::UnknownTopic => "unknown_topic",
        TopicError::UnknownMessage => "unknown_message",
        TopicError::Malformed => "malformed",
        TopicError::TopicRequired => "topic_required",
    }
}

pub fn topics_do(t: &mut RoomTopics, caps: &[String], actor: &str, op: TopicOp) -> Value {
    let caps: BTreeSet<String> = caps.iter().cloned().collect();
    let bad = || json!({ "refused": "malformed" });
    let res = match op {
        TopicOp::Create { id, title, at } => {
            let (Some(id), Some(at)) = (hex_n::<16>(&id), stamp(&at)) else {
                return bad();
            };
            t.create(&caps, actor.as_bytes().to_vec(), id, &title, at)
        }
        TopicOp::Update { id, title, at } => {
            let (Some(id), Some(at)) = (hex_n::<16>(&id), stamp(&at)) else {
                return bad();
            };
            t.update(&caps, actor.as_bytes(), id, &title, at)
        }
        TopicOp::SetStatus { id, status, at } => {
            let (Some(id), Some(at)) = (hex_n::<16>(&id), stamp(&at)) else {
                return bad();
            };
            t.set_status(&caps, actor.as_bytes(), id, status, at)
        }
        TopicOp::Post {
            mode,
            top_level,
            event,
            author,
            topic,
        } => {
            let mode = match mode.as_str() {
                "chat" => Mode::Chat,
                "forum" => Mode::Forum,
                "announcement" => Mode::Announcement,
                _ => return bad(),
            };
            let Some(event) = hex_n::<32>(&event) else {
                return bad();
            };
            let topic: Option<TopicId> = match topic.as_deref().map(hex_n::<16>) {
                None => None,
                Some(Some(t)) => Some(t),
                Some(None) => return bad(),
            };
            t.post(mode, top_level, event, author.into_bytes(), topic)
        }
        TopicOp::Move { event, topic, at } => {
            let (Some(event), Some(at)) = (hex_n::<32>(&event), stamp(&at)) else {
                return bad();
            };
            let topic: Option<TopicId> = match topic.as_deref().map(hex_n::<16>) {
                None => None,
                Some(Some(t)) => Some(t),
                Some(None) => return bad(),
            };
            t.move_message(&caps, actor.as_bytes(), event, topic, at)
        }
    };
    match res {
        Ok(()) => json!({ "applied": true }),
        Err(e) => json!({ "refused": topic_err(e) }),
    }
}

pub fn topics_view(t: &RoomTopics, topic: Option<&str>, message: Option<&str>) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(id) = topic.and_then(hex_n::<16>) {
        out.insert(
            "topic".into(),
            match t.topic(&id) {
                Some(x) => json!({ "title": x.title, "status": x.status as u8 }),
                None => Value::Null,
            },
        );
    }
    if let Some(ev) = message.and_then(hex_n::<32>) {
        out.insert(
            "placement".into(),
            match t.placement(&ev) {
                None => Value::Null,
                Some(None) => json!({ "topic": null }),
                Some(Some(id)) => {
                    json!({ "topic": id.iter().map(|b| format!("{b:02x}")).collect::<String>() })
                }
            },
        );
    }
    Value::Object(out)
}

// ---- Room moderation ---------------------------------------------------------------------------------------------

use om_core::room_moderation::{Effect, ModError, RoomModeration};

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ModOp {
    Admit {
        who: String,
        incarnation: String,
        governance: bool,
    },
    Remove {
        target: String,
        incarnation: String,
        governance_authority: bool,
    },
    Ban {
        target: String,
        incarnation: String,
        governance_authority: bool,
    },
    Unban {
        target: String,
        incarnation: String,
    },
    Timeout {
        target: String,
        incarnation: String,
        governance_authority: bool,
    },
    Untimeout {
        target: String,
        incarnation: String,
    },
    Restrict {
        target: String,
        restriction: u64,
        incarnation: String,
        governance_authority: bool,
    },
    Lift {
        target: String,
        restriction: u64,
        incarnation: String,
    },
    Redact {
        author: String,
        event: String,
    },
}

fn mod_err(e: ModError) -> &'static str {
    match e {
        ModError::NotAuthorized => "not_authorized",
        ModError::SelfAction => "self_action",
        ModError::GovernanceProtected => "governance_protected",
        ModError::Banned => "banned",
        ModError::Duplicate => "duplicate",
        ModError::UnknownMember => "unknown_member",
    }
}

fn effect(r: Result<Effect, ModError>) -> Value {
    match r {
        Ok(Effect::Applied) => json!({ "applied": true }),
        Ok(Effect::NoOp) => json!({ "applied": false, "noop": true }),
        Err(e) => json!({ "refused": mod_err(e) }),
    }
}
fn unit(r: Result<(), ModError>) -> Value {
    match r {
        Ok(()) => json!({ "applied": true }),
        Err(e) => json!({ "refused": mod_err(e) }),
    }
}

pub fn moderation_do(m: &mut RoomModeration, caps: &[String], actor: &str, op: ModOp) -> Value {
    let caps: BTreeSet<String> = caps.iter().cloned().collect();
    let bad = || json!({ "refused": "malformed" });
    let a = actor.as_bytes();
    match op {
        ModOp::Admit {
            who,
            incarnation,
            governance,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            unit(m.admit(who.into_bytes(), i, governance))
        }
        ModOp::Remove {
            target,
            incarnation,
            governance_authority,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            effect(m.remove(&caps, a, target.as_bytes(), i, governance_authority))
        }
        ModOp::Ban {
            target,
            incarnation,
            governance_authority,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            unit(m.ban(&caps, a, target.as_bytes(), i, governance_authority))
        }
        ModOp::Unban {
            target,
            incarnation,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            effect(m.unban(&caps, a, target.as_bytes(), i))
        }
        ModOp::Timeout {
            target,
            incarnation,
            governance_authority,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            unit(m.timeout(&caps, a, target.as_bytes(), i, governance_authority))
        }
        ModOp::Untimeout {
            target,
            incarnation,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            effect(m.untimeout(&caps, a, target.as_bytes(), i))
        }
        ModOp::Restrict {
            target,
            restriction,
            incarnation,
            governance_authority,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            unit(m.restrict(
                &caps,
                a,
                target.as_bytes(),
                restriction,
                i,
                governance_authority,
            ))
        }
        ModOp::Lift {
            target,
            restriction,
            incarnation,
        } => {
            let Some(i) = hex_n::<16>(&incarnation) else {
                return bad();
            };
            effect(m.lift(&caps, a, target.as_bytes(), restriction, i))
        }
        ModOp::Redact { author, event } => {
            let Some(e) = hex_n::<32>(&event) else {
                return bad();
            };
            unit(m.redact(&caps, a, author.as_bytes(), e))
        }
    }
}

pub fn moderation_view(m: &RoomModeration, who: &str) -> Value {
    let w = who.as_bytes();
    json!({ "member": m.is_member(w), "banned": m.is_banned(w), "timedOut": m.is_timed_out(w) })
}

// ---- Space binding and LAN first contact -------------------------------------------------------------------------

use om_core::lan_first_contact::{FirstContact, State, StepError};
use om_core::space_binding::{Binding, ChildLink, IntentError, validate_delegated_intent};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingIn {
    pub space_id: String,
    pub capabilities: Vec<String>,
    pub freshness_bound: u64,
    pub accepts_subdelegation: bool,
}

fn hex_vec(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// Checks one SpaceDelegatedIntent body against a Room binding. The link is taken as official (the Room's quorum approved
/// the binding); the caller verified the envelope and the Space head signatures.
pub fn space_intent_check(
    body_hex: &str,
    b: BindingIn,
    held_head: &str,
    held_version: u64,
    held_age: u64,
    via_sub: bool,
) -> Value {
    let (Some(body), Some(space), Some(head)) = (
        hex_vec(body_hex),
        hex_n::<32>(&b.space_id),
        hex_n::<32>(held_head),
    ) else {
        return json!({ "refused": "malformed" });
    };
    let mut link = ChildLink::proposed_by_space();
    link.child_accepts();
    let binding = match Binding::new(
        &link,
        space,
        Default::default(),
        b.capabilities.into_iter().collect(),
        b.freshness_bound,
        b.accepts_subdelegation,
    ) {
        Ok(x) => x,
        Err(e) => return json!({ "refused": format!("binding_{e:?}").to_lowercase() }),
    };
    match validate_delegated_intent(&body, &binding, (head, held_version), held_age, via_sub) {
        Ok(d) => json!({
            "valid": true,
            "operation": d.operation,
            "reducing": d.reducing,
            "capabilities": d.capabilities.into_iter().collect::<Vec<_>>()
        }),
        Err(e) => json!({
            "valid": false,
            "refusal": match e {
                IntentError::Malformed => "malformed",
                IntentError::WrongSpace => "wrong_space",
                IntentError::StaleHead => "stale_head",
                IntentError::OutsideBinding => "outside_binding",
                IntentError::Protected => "protected",
                IntentError::CryptoProfile => "crypto_profile",
                IntentError::SubDelegated => "sub_delegated",
            }
        }),
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FirstContactStep {
    Reach,
    Ticket { verified: bool },
    Request { valid: bool },
    OwnBinding { verified: bool },
    PeerBinding { verified: bool },
    Lost,
    State,
}

pub fn first_contact(f: &mut FirstContact, step: FirstContactStep) -> Value {
    let r: Result<(), StepError> = match step {
        FirstContactStep::Reach => {
            f.reach();
            Ok(())
        }
        FirstContactStep::Ticket { verified } => f.ticket(verified),
        FirstContactStep::Request { valid } => f.request(valid),
        FirstContactStep::OwnBinding { verified } => f.own_binding(verified),
        FirstContactStep::PeerBinding { verified } => f.peer_binding(verified),
        FirstContactStep::Lost => {
            f.reachability_lost();
            Ok(())
        }
        FirstContactStep::State => Ok(()),
    };
    let state = match f.state() {
        State::Unknown => "unknown",
        State::Reachable => "reachable",
        State::Ticketed => "ticketed",
        State::Requested => "requested",
        State::HalfBound => "half_bound",
        State::Active => "active",
    };
    json!({
        "state": state,
        "asyncKeyFetchAllowed": f.async_key_fetch_allowed(),
        "deviceEligible": f.device_eligible(),
        "refused": match r { Ok(()) => Value::Null, Err(StepError::OutOfOrder) => json!("out_of_order"), Err(StepError::NotVerified) => json!("not_verified") }
    })
}

// ---- Handle-based room services, bridge room and LAN transfer ---------------------------------------------------------
// State lives in the host instance under a caller-chosen handle (a short string). Control heads arrive as facts the caller
// already validated (chain and signatures are the caller's, as in the Core modules); nothing here holds keys or authority.

use om_core::bridge_room::{BridgeRoom, BridgeRoomError, Mapping};
use om_core::lan_transfer::{Chunk, Receiver, Sender, TransferError};
use om_core::service_control::{Endpoint, ServiceState};
use om_core::service_offer::{Membership, Offer, OfferError, ServiceRoom};
use om_core::service_room_member::{RoomError, TriggerFilter};
use std::collections::BTreeMap;

/// Upper bound per handle kind; a host that needs more closes one first.
const MAX_HANDLES: usize = 64;

#[derive(Default)]
pub struct Handles {
    rooms: BTreeMap<String, ServiceRoom>,
    bridge_rooms: BTreeMap<String, BridgeRoom>,
    senders: BTreeMap<String, (Sender, bool)>,
    receivers: BTreeMap<String, Receiver>,
    links: BTreeMap<String, om_core::space_binding::ChildLink>,
    bindings: BTreeMap<String, om_core::space_binding::Binding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointIn {
    id: String,
    state: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadIn {
    service_id: String,
    version: u64,
    hash: String,
    endpoints: Vec<EndpointIn>,
}

#[derive(Deserialize)]
pub struct ApproverIn {
    kind: u64,
    id: String,
}

fn head(h: &HeadIn) -> Option<ServiceState> {
    let mut endpoints = BTreeMap::new();
    for e in &h.endpoints {
        let id = hex_vec(&e.id)?;
        endpoints.insert(
            id.clone(),
            Endpoint {
                id,
                state: e.state,
                profile: 0,
                key: Vec::new(),
                mode: 0,
                host_ref: None,
            },
        );
    }
    Some(ServiceState {
        service_id: hex_n::<32>(&h.service_id)?,
        version: h.version,
        prev: None,
        controllers: BTreeSet::new(),
        threshold: 0,
        endpoints,
        hash: hex_n::<32>(&h.hash)?,
    })
}

pub fn approvers(a: &[ApproverIn]) -> Option<Vec<(u64, Vec<u8>)>> {
    a.iter().map(|p| Some((p.kind, hex_vec(&p.id)?))).collect()
}

fn room_err(e: &RoomError) -> &'static str {
    match e {
        RoomError::NotAuthorized => "not_authorized",
        RoomError::QuorumNotMet => "quorum_not_met",
        RoomError::AlreadyInstalled => "already_installed",
        RoomError::NotInstalled => "not_installed",
        RoomError::Rollback => "rollback",
        RoomError::WrongService => "wrong_service",
        RoomError::NotRevoked => "not_revoked",
        RoomError::TooEarly => "too_early",
    }
}

fn bridge_room_err(e: &BridgeRoomError) -> &'static str {
    match e {
        BridgeRoomError::Room(r) => room_err(r),
        BridgeRoomError::NoMapping => "no_mapping",
        BridgeRoomError::BadDirection => "bad_direction",
        BridgeRoomError::EmptyClasses => "empty_classes",
    }
}

fn refused(r: &str) -> Value {
    json!({ "refused": r })
}
fn done() -> Value {
    json!({ "done": true })
}
fn caps_set(c: &[String]) -> BTreeSet<String> {
    c.iter().cloned().collect()
}
fn filter(f: &Option<Vec<u8>>) -> Option<TriggerFilter> {
    f.as_ref().map(|c| TriggerFilter {
        event_classes: c.iter().copied().collect(),
    })
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RoomOp {
    Open {
        quorum: Option<usize>,
        sequencing_timeout: u64,
    },
    Close,
    Install {
        head: HeadIn,
        incarnation: String,
        filter_classes: Option<Vec<u8>>,
        approvers: Vec<ApproverIn>,
    },
    SetFilter {
        service: String,
        filter_classes: Option<Vec<u8>>,
    },
    ObserveHead {
        head: HeadIn,
        now: u64,
    },
    MustStop {
        service: String,
        endpoint: String,
    },
    MayCommitRemoval {
        service: String,
        endpoint: String,
        is_coordinator: bool,
        now: u64,
    },
    RemovalCommitted {
        service: String,
        endpoint: String,
    },
    Remove {
        service: String,
        operation_id: Option<String>,
    },
    View {
        service: String,
    },
    List,
    /// An offer references the currently verified Room control head.
    Propose {
        head: HeadIn,
        incarnation: String,
        capabilities: Vec<String>,
        filter_classes: Option<Vec<u8>>,
        approvers: Vec<ApproverIn>,
        name: String,
    },
    WithdrawOffer {
        service: String,
    },
    InstallOffer {
        service: String,
        target: String,
        operation_id: Option<String>,
    },
    RemoveMember {
        service: String,
        target: String,
        operation_id: Option<String>,
    },
    Operation {
        operation_id: String,
        mutation: MutationIn,
        principal: String,
        target: Option<String>,
    },
}

impl RoomOp {
    pub fn needs_verified_authority(&self) -> bool {
        matches!(
            self,
            Self::Install { .. }
                | Self::SetFilter { .. }
                | Self::ObserveHead { .. }
                | Self::MayCommitRemoval { .. }
                | Self::RemovalCommitted { .. }
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationIn {
    AdmitOffer,
    InstallOffer,
    RemoveMember,
    Remove,
}

impl From<MutationIn> for Mutation {
    fn from(value: MutationIn) -> Self {
        match value {
            MutationIn::AdmitOffer => Self::AdmitOffer,
            MutationIn::InstallOffer => Self::InstallOffer,
            MutationIn::RemoveMember => Self::RemoveMember,
            MutationIn::Remove => Self::Remove,
        }
    }
}

fn operation_target(
    id: Option<&str>,
    mutation: Mutation,
    principal: &str,
    target: Option<&str>,
) -> Option<(Option<OperationId>, OperationTarget)> {
    Some((
        match id {
            Some(id) => Some(hex_n::<32>(id)?),
            None => None,
        },
        OperationTarget {
            mutation,
            principal: hex_n::<32>(principal)?,
            review: match target {
                Some(target) => Some(hex_n::<32>(target)?),
                None => None,
            },
        },
    ))
}

fn operation_result(outcome: OperationOutcome) -> Value {
    match outcome {
        OperationOutcome::Applied => done(),
        OperationOutcome::Removed(count) => json!({ "removedEndpoints": count }),
        OperationOutcome::Refused(reason) => refused(&reason),
    }
}

fn operation_view(outcome: Option<OperationOutcome>) -> Value {
    json!({ "result": outcome.map(operation_result) })
}

fn verified_head<'a>(
    controls: &'a [om_core::engine::RoomControlEntry],
    kind: om_core::room_control::ObjectKind,
    head: &ServiceState,
) -> Option<&'a om_core::engine::RoomControlEntry> {
    verified_head_ref(controls, kind, &HeadReference::from(head))
}

fn verified_head_ref<'a>(
    controls: &'a [om_core::engine::RoomControlEntry],
    kind: om_core::room_control::ObjectKind,
    head: &HeadReference,
) -> Option<&'a om_core::engine::RoomControlEntry> {
    controls.iter().find(|entry| {
        entry.kind == kind
            && entry.id == head.id
            && !entry.frozen
            && entry.head_version == Some(head.version)
            && entry.head_hash == Some(head.hash)
            && entry
                .endpoints
                .iter()
                .any(|(_, state, usable)| *state == om_core::service_control::ACTIVE && *usable)
            && entry.endpoints.len() == head.endpoints.len()
            && head.endpoints.windows(2).all(|pair| pair[0].0 < pair[1].0)
            && entry.endpoints.iter().all(|(id, state, usable)| {
                head.endpoints
                    .iter()
                    .any(|(ep, status)| ep == id && status == state)
                    && (*state != om_core::service_control::ACTIVE || *usable)
            })
    })
}

fn roster_view(
    e: Option<&om_core::service_room_member::RosterEntry>,
    held: Option<(u64, [u8; 32])>,
) -> Value {
    match e {
        None => json!({ "installed": false }),
        Some(e) => json!({
            "installed": true,
            "canReadAllContent": e.can_read_all_content(),
            "endpoints": e.endpoints.iter().map(|x| x.iter().map(|b| format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>(),
            "filterClasses": e.filter.as_ref().map(|f| f.event_classes.iter().copied().collect::<Vec<u8>>()),
            "admittedVersion": e.admitted_under.0,
            "heldVersion": held.map(|h| h.0),
        }),
    }
}

fn service_offer_err(e: &OfferError) -> &'static str {
    match e {
        OfferError::Malformed => "malformed",
        OfferError::TooMany => "too_many_offers",
        OfferError::StaleTarget => "stale_target",
        OfferError::Room(e) => room_err(e),
    }
}

fn service_listing(r: &ServiceRoom, service: &[u8; 32], membership: Membership) -> Value {
    let mut value = roster_view(r.members().entry(service), r.members().held_head(service));
    if let Value::Object(row) = &mut value {
        let offer = r.offer(service);
        row.insert("service".into(), json!(lan_hex(service)));
        row.insert(
            "membership".into(),
            json!(match membership {
                Membership::Proposed => "proposed",
                Membership::Installed => "installed",
                Membership::Removed => "removed",
            }),
        );
        row.insert(
            "target".into(),
            json!(r.current_target(service).map(|t| lan_hex(&t))),
        );
        row.insert("name".into(), json!(r.name(service)));
        row.insert(
            "capabilities".into(),
            json!(
                offer
                    .filter(|_| membership == Membership::Proposed)
                    .map(|o| &o.capabilities)
            ),
        );
        row.insert(
            "approversReportedAtProposal".into(),
            json!(offer.map(|o| o.approvers.len())),
        );
        if let Some(offer) = offer {
            row.insert(
                "admissionBasis".into(),
                json!({
                    "serviceId": lan_hex(service),
                    "version": offer.head.version,
                    "hash": lan_hex(&offer.head.hash),
                    "endpoints": offer.head.endpoints.values().map(|e| json!({
                        "id": lan_hex(&e.id), "state": e.state,
                    })).collect::<Vec<_>>(),
                }),
            );
            if membership == Membership::Proposed {
                row.insert(
                    "filterClasses".into(),
                    json!(offer.filter.as_ref().map(|f| &f.event_classes)),
                );
            }
        }
    }
    value
}

pub fn room_services_do(
    h: &mut Handles,
    room: &str,
    caps: &[String],
    op: RoomOp,
    controls: &[om_core::engine::RoomControlEntry],
) -> Value {
    let bad = || refused("malformed");
    if let RoomOp::Open {
        quorum,
        sequencing_timeout,
    } = op
    {
        if !h.rooms.contains_key(room) && h.rooms.len() >= MAX_HANDLES {
            return refused("too_many_handles");
        }
        h.rooms.insert(
            room.to_string(),
            ServiceRoom::new(quorum, sequencing_timeout),
        );
        return done();
    }
    if matches!(op, RoomOp::Close) {
        return json!({ "done": h.rooms.remove(room).is_some() });
    }
    let Some(r) = h.rooms.get_mut(room) else {
        return refused("unknown_handle");
    };
    let caps = caps_set(caps);
    match op {
        RoomOp::Open { .. } | RoomOp::Close => unreachable!(),
        RoomOp::Install {
            head: hd,
            incarnation,
            filter_classes,
            approvers: ap,
        } => {
            let (Some(hd), Some(inc), Some(ap)) =
                (head(&hd), hex_vec(&incarnation), approvers(&ap))
            else {
                return bad();
            };
            match r.install(&caps, &hd, inc, filter(&filter_classes), &ap) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::SetFilter {
            service,
            filter_classes,
        } => {
            let Some(s) = hex_n::<32>(&service) else {
                return bad();
            };
            match r
                .members_mut()
                .set_filter(&caps, &s, filter(&filter_classes))
            {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::ObserveHead { head: hd, now } => {
            let Some(hd) = head(&hd) else {
                return bad();
            };
            match r.members_mut().observe_head(&hd, now) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::MustStop { service, endpoint } => {
            let (Some(s), Some(e)) = (hex_n::<32>(&service), hex_vec(&endpoint)) else {
                return bad();
            };
            json!({ "mustStopSending": r.members().must_stop_sending(&s, &e) })
        }
        RoomOp::MayCommitRemoval {
            service,
            endpoint,
            is_coordinator,
            now,
        } => {
            let (Some(s), Some(e)) = (hex_n::<32>(&service), hex_vec(&endpoint)) else {
                return bad();
            };
            match r.members().may_commit_removal(&s, &e, is_coordinator, now) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::RemovalCommitted { service, endpoint } => {
            let (Some(s), Some(e)) = (hex_n::<32>(&service), hex_vec(&endpoint)) else {
                return bad();
            };
            json!({ "applied": r.members_mut().removal_committed(&s, &e) })
        }
        RoomOp::Remove {
            service,
            operation_id,
        } => {
            let Some((id, target)) =
                operation_target(operation_id.as_deref(), Mutation::Remove, &service, None)
            else {
                return bad();
            };
            operation_result(r.apply_operation(id, target, |r| {
                match r.remove(&caps, &target.principal) {
                    Ok(gone) => OperationOutcome::Removed(gone.len()),
                    Err(e) => OperationOutcome::refused(room_err(&e)),
                }
            }))
        }
        RoomOp::View { service } => {
            let Some(s) = hex_n::<32>(&service) else {
                return bad();
            };
            roster_view(r.members().entry(&s), r.members().held_head(&s))
        }
        RoomOp::List => {
            let services: Vec<_> = r
                .listing()
                .into_iter()
                .map(|(service, membership)| service_listing(r, &service, membership))
                .collect();
            json!({ "services": services })
        }
        RoomOp::Propose {
            head: hd,
            incarnation,
            capabilities,
            filter_classes,
            approvers: ap,
            name,
        } => {
            let (Some(hd), Some(incarnation), Some(approvers)) =
                (head(&hd), hex_vec(&incarnation), approvers(&ap))
            else {
                return bad();
            };
            if verified_head(controls, om_core::room_control::ObjectKind::Service, &hd).is_none() {
                return refused("unverified_head");
            }
            match r.propose(Offer {
                head: hd,
                incarnation,
                capabilities: capabilities.into_iter().collect(),
                filter: filter(&filter_classes),
                approvers,
                name,
            }) {
                Ok(target) => json!({ "target": lan_hex(&target) }),
                Err(e) => refused(service_offer_err(&e)),
            }
        }
        RoomOp::WithdrawOffer { service } => {
            let Some(s) = hex_n::<32>(&service) else {
                return bad();
            };
            json!({ "done": r.withdraw_offer(&s) })
        }
        RoomOp::InstallOffer {
            service,
            target,
            operation_id,
        } => {
            let Some((id, key)) = operation_target(
                operation_id.as_deref(),
                Mutation::InstallOffer,
                &service,
                Some(&target),
            ) else {
                return bad();
            };
            let s = key.principal;
            let t = key.review.expect("install has a review target");
            operation_result(r.apply_operation(id, key, |r| {
                if !caps.contains(om_core::service_room_member::INSTALL) {
                    return OperationOutcome::refused("not_authorized");
                }
                if r.current_target(&s) == Some(t)
                    && r.members().entry(&s).is_none()
                    && r.offer(&s).is_some_and(|offer| {
                        verified_head(
                            controls,
                            om_core::room_control::ObjectKind::Service,
                            &offer.head,
                        )
                        .is_none()
                    })
                {
                    return OperationOutcome::refused("unverified_head");
                }
                match r.install_offer(&caps, &s, &t) {
                    Ok(()) => OperationOutcome::Applied,
                    Err(e) => OperationOutcome::refused(service_offer_err(&e)),
                }
            }))
        }
        RoomOp::RemoveMember {
            service,
            target,
            operation_id,
        } => {
            let Some((id, key)) = operation_target(
                operation_id.as_deref(),
                Mutation::RemoveMember,
                &service,
                Some(&target),
            ) else {
                return bad();
            };
            operation_result(r.apply_operation(id, key, |r| {
                if !caps.contains(om_core::service_room_member::INSTALL) {
                    return OperationOutcome::refused("not_authorized");
                }
                match r.remove_installation(
                    &caps,
                    &key.principal,
                    &key.review.expect("remove member has a review target"),
                ) {
                    Ok(gone) => OperationOutcome::Removed(gone.len()),
                    Err(e) => OperationOutcome::refused(service_offer_err(&e)),
                }
            }))
        }
        RoomOp::Operation {
            operation_id,
            mutation,
            principal,
            target,
        } => {
            let Some((Some(id), key)) = operation_target(
                Some(&operation_id),
                mutation.into(),
                &principal,
                target.as_deref(),
            ) else {
                return bad();
            };
            operation_view(r.operation(id, key))
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingIn {
    incarnation: String,
    namespace: String,
    foreign_scope: String,
    direction: u64,
    allowed_classes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum BridgeRoomOp {
    Open {
        quorum: Option<usize>,
        sequencing_timeout: u64,
        freshness_bound: u64,
    },
    Close,
    Admit {
        head: HeadIn,
        mapping: MappingIn,
        approvers: Vec<ApproverIn>,
        now: u64,
    },
    ObserveHead {
        head: HeadIn,
        now: u64,
    },
    Remove {
        bridge: String,
        operation_id: Option<String>,
    },
    RevokeMapping {
        bridge: String,
    },
    MayExport {
        bridge: String,
        endpoint: String,
        class: u8,
        now: Option<u64>,
    },
    ImportAuthorOk {
        author_kind: u64,
    },
    View {
        bridge: String,
    },
    /// Every bridge the Room can show, with its membership (ADR-0270): proposed, member or removed.
    List,
    /// Places a proposed bridge in the Room. Grants nothing.
    Propose {
        head: HeadIn,
        mapping: MappingIn,
        approvers: Vec<ApproverIn>,
        name: String,
        external_claim: Option<String>,
    },
    WithdrawOffer {
        bridge: String,
    },
    /// Admits the offer the reviewer saw; the target must be the current one.
    AdmitOffer {
        bridge: String,
        target: String,
        operation_id: Option<String>,
    },
    RemoveMember {
        bridge: String,
        target: String,
        operation_id: Option<String>,
    },
    Operation {
        operation_id: String,
        mutation: MutationIn,
        principal: String,
        target: Option<String>,
    },
}

impl BridgeRoomOp {
    pub fn needs_verified_authority(&self) -> bool {
        matches!(
            self,
            Self::Admit { .. } | Self::ObserveHead { .. } | Self::RevokeMapping { .. }
        )
    }
}

fn offer_err(e: &om_core::bridge_room::OfferError) -> &'static str {
    use om_core::bridge_room::OfferError as E;
    match e {
        E::Malformed => "malformed",
        E::TooMany => "too_many_offers",
        E::StaleTarget => "stale_target",
        E::Room(r) => bridge_room_err(r),
    }
}

fn bridge_entry(
    r: &BridgeRoom,
    b: &[u8; 32],
    m: om_core::bridge_room::Membership,
    controls: &[om_core::engine::RoomControlEntry],
) -> Value {
    use om_core::bridge_room::Membership as M;
    let mut v = match m {
        M::Member => bridge_view(r, b),
        _ => json!({ "installed": false }),
    };
    if let Value::Object(o) = &mut v {
        o.insert("bridge".into(), json!(lan_hex(b)));
        o.insert(
            "membership".into(),
            json!(match m {
                M::Proposed => "proposed",
                M::Member => "member",
                M::Removed => "removed",
            }),
        );
        o.insert(
            "target".into(),
            r.current_target(b)
                .map_or(Value::Null, |t| json!(lan_hex(&t))),
        );
        let head = controls.iter().find(|entry| {
            entry.kind == om_core::room_control::ObjectKind::Bridge && entry.id == *b
        });
        o.insert(
            "headFresh".into(),
            json!(head.is_some_and(|head| {
                !head.frozen
                    && head.head_version == r.members().held_head(b).map(|h| h.0)
                    && head.head_hash == r.members().held_head(b).map(|h| h.1)
                    && r.has_runtime_observation(b)
                    && r.is_fresh(head.observed_age_ms)
            })),
        );
        if let Some(of) = r.offer(b) {
            if m == M::Proposed {
                o.insert("mapping".into(), mapping_view(Some(&of.mapping)));
            }
            o.insert("name".into(), json!(of.name));
            o.insert("externalClaim".into(), json!(of.external_claim));
            o.insert(
                "approversReportedAtProposal".into(),
                json!(of.approvers.len()),
            );
        }
    }
    v
}

fn mapping_view(m: Option<&Mapping>) -> Value {
    match m {
        None => Value::Null,
        Some(m) => json!({
            "namespace": m.namespace,
            "foreignScope": m.foreign_scope,
            "direction": m.direction,
            "allowedClasses": m.allowed_classes.iter().copied().collect::<Vec<u8>>(),
            "active": m.active,
        }),
    }
}

fn bridge_view(r: &BridgeRoom, b: &[u8; 32]) -> Value {
    let mut v = roster_view(r.roster(b), r.members().held_head(b));
    if let Value::Object(o) = &mut v {
        o.insert("mapping".into(), mapping_view(r.mapping(b)));
    }
    v
}

fn bridge_export_result(result: Result<(), om_core::bridge_room::Blocked>) -> Value {
    use om_core::bridge_room::Blocked;
    match result {
        Ok(()) => json!({ "allowed": true }),
        Err(x) => json!({ "allowed": false, "blocked": match x {
            Blocked::NoMapping => "no_mapping",
            Blocked::Revoked => "revoked",
            Blocked::Direction => "direction",
            Blocked::ClassNotAllowed => "class_not_allowed",
            Blocked::StaleMapping => "stale_mapping",
            Blocked::EndpointStopped => "endpoint_stopped",
        }}),
    }
}

pub fn bridge_room_do(
    h: &mut Handles,
    room: &str,
    caps: &[String],
    op: BridgeRoomOp,
    controls: &[om_core::engine::RoomControlEntry],
    runtime_ms: Option<u64>,
) -> Value {
    use om_core::room_control::ObjectKind;
    let bad = || refused("malformed");
    if let BridgeRoomOp::Open {
        quorum,
        sequencing_timeout,
        freshness_bound,
    } = op
    {
        if !h.bridge_rooms.contains_key(room) && h.bridge_rooms.len() >= MAX_HANDLES {
            return refused("too_many_handles");
        }
        h.bridge_rooms.insert(
            room.to_string(),
            BridgeRoom::new(quorum, sequencing_timeout, freshness_bound),
        );
        return done();
    }
    if matches!(op, BridgeRoomOp::Close) {
        return json!({ "done": h.bridge_rooms.remove(room).is_some() });
    }
    if let BridgeRoomOp::ImportAuthorOk { author_kind } = op {
        return json!({ "allowed": om_core::bridge_room::import_author_ok(author_kind) });
    }
    let Some(r) = h.bridge_rooms.get_mut(room) else {
        return refused("unknown_handle");
    };
    let caps = caps_set(caps);
    match op {
        BridgeRoomOp::Open { .. } | BridgeRoomOp::Close | BridgeRoomOp::ImportAuthorOk { .. } => {
            unreachable!()
        }
        BridgeRoomOp::Admit {
            head: hd,
            mapping,
            approvers: ap,
            now,
        } => {
            let (Some(hd), Some(inc), Some(ap)) =
                (head(&hd), hex_n::<16>(&mapping.incarnation), approvers(&ap))
            else {
                return bad();
            };
            let m = Mapping {
                incarnation: inc,
                namespace: mapping.namespace,
                foreign_scope: mapping.foreign_scope,
                direction: mapping.direction,
                allowed_classes: mapping.allowed_classes.into_iter().collect(),
                active: true,
            };
            match r.admit(&caps, &hd, m, &ap, now) {
                Ok(()) => done(),
                Err(e) => refused(bridge_room_err(&e)),
            }
        }
        BridgeRoomOp::ObserveHead { head: hd, now } => {
            let Some(hd) = head(&hd) else {
                return bad();
            };
            match r.observe_head(&hd, now) {
                Ok(()) => done(),
                Err(e) => refused(bridge_room_err(&e)),
            }
        }
        BridgeRoomOp::Remove {
            bridge,
            operation_id,
        } => {
            let Some((id, target)) =
                operation_target(operation_id.as_deref(), Mutation::Remove, &bridge, None)
            else {
                return bad();
            };
            operation_result(r.apply_operation(id, target, |r| {
                match r.remove(&caps, &target.principal) {
                    Ok(gone) => OperationOutcome::Removed(gone.len()),
                    Err(e) => OperationOutcome::refused(bridge_room_err(&e)),
                }
            }))
        }
        BridgeRoomOp::RevokeMapping { bridge } => {
            let Some(b) = hex_n::<32>(&bridge) else {
                return bad();
            };
            match r.revoke_mapping(&caps, &b) {
                Ok(()) => done(),
                Err(e) => refused(bridge_room_err(&e)),
            }
        }
        BridgeRoomOp::MayExport {
            bridge,
            endpoint,
            class,
            now,
        } => {
            let (Some(b), Some(e)) = (hex_n::<32>(&bridge), hex_vec(&endpoint)) else {
                return bad();
            };
            if let Some(tick) = runtime_ms {
                let held = r.members().held_head(&b);
                let verified = controls
                    .iter()
                    .find(|entry| entry.kind == ObjectKind::Bridge && entry.id == b);
                if let Some(entry) = verified {
                    if entry.frozen
                        || !entry
                            .endpoints
                            .iter()
                            .any(|(id, _, usable)| *id == e && *usable)
                    {
                        return json!({ "allowed": false, "blocked": "endpoint_stopped" });
                    }
                    if held != entry.head_version.zip(entry.head_hash)
                        || !r.is_fresh(entry.observed_age_ms)
                    {
                        return json!({ "allowed": false, "blocked": "stale_mapping" });
                    }
                } else {
                    return json!({ "allowed": false, "blocked": "stale_mapping" });
                }
                return bridge_export_result(r.may_export(&b, &e, class, tick));
            }
            #[cfg(test)]
            if let Some(now) = now {
                return bridge_export_result(r.may_export(&b, &e, class, now));
            }
            let _ = now;
            refused("unavailable")
        }
        BridgeRoomOp::View { bridge } => {
            let Some(b) = hex_n::<32>(&bridge) else {
                return bad();
            };
            bridge_view(r, &b)
        }
        BridgeRoomOp::List => {
            let list: Vec<Value> = r
                .listing()
                .iter()
                .map(|(b, m)| bridge_entry(r, b, *m, controls))
                .collect();
            json!({ "bridges": list })
        }
        BridgeRoomOp::Propose {
            head: hd,
            mapping,
            approvers: ap,
            name,
            external_claim,
        } => {
            let (Some(hd), Some(inc), Some(ap)) =
                (head(&hd), hex_n::<16>(&mapping.incarnation), approvers(&ap))
            else {
                return bad();
            };
            if verified_head(controls, ObjectKind::Bridge, &hd).is_none() {
                return refused("unverified_head");
            }
            let m = Mapping {
                incarnation: inc,
                namespace: mapping.namespace,
                foreign_scope: mapping.foreign_scope,
                direction: mapping.direction,
                allowed_classes: mapping.allowed_classes.into_iter().collect(),
                active: true,
            };
            match r.propose(om_core::bridge_room::Offer {
                head: hd,
                mapping: m,
                approvers: ap,
                name,
                external_claim,
            }) {
                Ok(t) => json!({ "target": lan_hex(&t) }),
                Err(e) => refused(offer_err(&e)),
            }
        }
        BridgeRoomOp::WithdrawOffer { bridge } => {
            let Some(b) = hex_n::<32>(&bridge) else {
                return bad();
            };
            json!({ "done": r.withdraw_offer(&b) })
        }
        BridgeRoomOp::AdmitOffer {
            bridge,
            target,
            operation_id,
        } => {
            let Some((id, key)) = operation_target(
                operation_id.as_deref(),
                Mutation::AdmitOffer,
                &bridge,
                Some(&target),
            ) else {
                return bad();
            };
            let b = key.principal;
            let t = key.review.expect("admit has a review target");
            operation_result(r.apply_operation(id, key, |r| {
                let Some(offer) = r
                    .offer(&b)
                    .filter(|_| r.roster(&b).is_none() && r.current_target(&b) == Some(t))
                else {
                    return OperationOutcome::refused("stale_target");
                };
                let Some(now) = verified_head(controls, ObjectKind::Bridge, &offer.head)
                    .filter(|entry| r.is_fresh(entry.observed_age_ms))
                    .and_then(|entry| {
                        runtime_ms.and_then(|tick| tick.checked_sub(entry.observed_age_ms))
                    })
                else {
                    return OperationOutcome::refused("unverified_head");
                };
                match r.admit_offer(&caps, &b, &t, now) {
                    Ok(()) => OperationOutcome::Applied,
                    Err(e) => OperationOutcome::refused(offer_err(&e)),
                }
            }))
        }
        BridgeRoomOp::RemoveMember {
            bridge,
            target,
            operation_id,
        } => {
            let Some((id, key)) = operation_target(
                operation_id.as_deref(),
                Mutation::RemoveMember,
                &bridge,
                Some(&target),
            ) else {
                return bad();
            };
            operation_result(r.apply_operation(id, key, |r| {
                if !caps.contains(om_core::bridge_room::ADD_BRIDGE) {
                    return OperationOutcome::refused("not_authorized");
                }
                match r.remove_member(
                    &caps,
                    &key.principal,
                    &key.review.expect("remove member has a review target"),
                ) {
                    Ok(gone) => OperationOutcome::Removed(gone.len()),
                    Err(e) => OperationOutcome::refused(offer_err(&e)),
                }
            }))
        }
        BridgeRoomOp::Operation {
            operation_id,
            mutation,
            principal,
            target,
        } => {
            let Some((Some(id), key)) = operation_target(
                Some(&operation_id),
                mutation.into(),
                &principal,
                target.as_deref(),
            ) else {
                return bad();
            };
            operation_view(r.operation(id, key))
        }
    }
}

pub fn snapshot_refusal(error: SnapshotError) -> Value {
    refused(match error {
        SnapshotError::Invalid => "invalid_snapshot",
        SnapshotError::Limit => "snapshot_limit",
    })
}

impl Handles {
    pub fn export_room_snapshot(
        &self,
        room: &str,
        kind: om_core::room_control::ObjectKind,
    ) -> Option<Result<Vec<u8>, SnapshotError>> {
        use om_core::room_control::ObjectKind::*;
        match kind {
            Service => self.rooms.get(room).map(ServiceRoom::export_snapshot),
            Bridge => self.bridge_rooms.get(room).map(BridgeRoom::export_snapshot),
        }
    }

    pub fn import_room_snapshot(
        &mut self,
        room: &str,
        kind: om_core::room_control::ObjectKind,
        bytes: &[u8],
        controls: &[om_core::engine::RoomControlEntry],
        now: u64,
    ) -> Value {
        use om_core::room_control::ObjectKind::*;
        let verify = |head: &HeadReference| verified_head_ref(controls, kind, head).is_some();
        match kind {
            Service => {
                if self.rooms.contains_key(room) || self.rooms.len() >= MAX_HANDLES {
                    return refused("room_already_open_or_limit");
                }
                match ServiceRoom::import_snapshot(bytes, verify, now) {
                    Ok(restored) => {
                        self.rooms.insert(room.into(), restored);
                    }
                    Err(error) => return snapshot_refusal(error),
                }
            }
            Bridge => {
                if self.bridge_rooms.contains_key(room) || self.bridge_rooms.len() >= MAX_HANDLES {
                    return refused("room_already_open_or_limit");
                }
                match BridgeRoom::import_snapshot(bytes, verify, now) {
                    Ok(restored) => {
                        self.bridge_rooms.insert(room.into(), restored);
                    }
                    Err(error) => return snapshot_refusal(error),
                }
            }
        }
        done()
    }
}

impl Handles {
    pub fn observe_room_control(
        &mut self,
        room: &str,
        kind: om_core::room_control::ObjectKind,
        accepted: &ServiceState,
        canonical: bool,
        now: u64,
    ) {
        use om_core::room_control::ObjectKind;
        match kind {
            ObjectKind::Service => {
                if let Some(r) = self.rooms.get_mut(room) {
                    let members = r.members_mut();
                    members.observe_revocations(accepted, now);
                    if canonical {
                        let _ = members.observe_head(accepted, now);
                    }
                }
            }
            ObjectKind::Bridge => {
                if let Some(r) = self.bridge_rooms.get_mut(room) {
                    r.members_mut().observe_revocations(accepted, now);
                    if canonical {
                        let _ = r.observe_head(accepted, now);
                    }
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TransferOp {
    SenderOpen {
        blob_id: String,
        digest: String,
        total: u64,
    },
    ReceiverOpen {
        total: u64,
        restored_offset: Option<u64>,
    },
    Close,
    ResumeFrom {
        receiver_offset: u64,
    },
    NextChunk,
    TransportAck {
        up_to: u64,
    },
    Receipt {
        authenticated: bool,
        blob_id: String,
        digest: String,
        length: u64,
    },
    Accept {
        offset: u64,
        len: usize,
    },
    View,
}

fn transfer_err(e: &TransferError) -> &'static str {
    match e {
        TransferError::BadAck => "bad_ack",
        TransferError::ReceiptMismatch => "receipt_mismatch",
        TransferError::ReceiptNotAuthenticated => "receipt_not_authenticated",
        TransferError::BadChunk => "bad_chunk",
        TransferError::Overrun => "overrun",
    }
}

/// `side` is "sender" or "receiver"; a handle names one object of that side.
pub fn lan_transfer_do(h: &mut Handles, side: &str, handle: &str, op: TransferOp) -> Value {
    let bad = || refused("malformed");
    let sender = side == "sender";
    if !sender && side != "receiver" {
        return bad();
    }
    match op {
        TransferOp::SenderOpen {
            blob_id,
            digest,
            total,
        } => {
            let (true, Some(b), Some(d)) = (sender, hex_n::<32>(&blob_id), hex_n::<32>(&digest))
            else {
                return bad();
            };
            if !h.senders.contains_key(handle) && h.senders.len() >= MAX_HANDLES {
                return refused("too_many_handles");
            }
            h.senders
                .insert(handle.to_string(), (Sender::new(b, d, total), false));
            done()
        }
        TransferOp::ReceiverOpen {
            total,
            restored_offset,
        } => {
            if sender {
                return bad();
            }
            if !h.receivers.contains_key(handle) && h.receivers.len() >= MAX_HANDLES {
                return refused("too_many_handles");
            }
            let r = match restored_offset {
                Some(o) => Receiver::restore(total, o),
                None => Receiver::new(total),
            };
            h.receivers.insert(handle.to_string(), r);
            done()
        }
        TransferOp::Close => {
            json!({ "done": if sender { h.senders.remove(handle).is_some() } else { h.receivers.remove(handle).is_some() } })
        }
        op if sender => {
            let Some((s, paused)) = h.senders.get_mut(handle) else {
                return refused("unknown_handle");
            };
            match op {
                TransferOp::ResumeFrom { receiver_offset } => {
                    match s.resume_from(receiver_offset) {
                        Ok(()) => done(),
                        Err(e) => refused(transfer_err(&e)),
                    }
                }
                TransferOp::NextChunk => match s.next_chunk(paused) {
                    Some(Chunk { offset, len }) => {
                        json!({ "chunk": { "offset": offset, "len": len }, "paused": *paused })
                    }
                    None => json!({ "chunk": null, "paused": *paused }),
                },
                TransferOp::TransportAck { up_to } => match s.transport_ack(up_to) {
                    Ok(()) => done(),
                    Err(e) => refused(transfer_err(&e)),
                },
                TransferOp::Receipt {
                    authenticated,
                    blob_id,
                    digest,
                    length,
                } => {
                    let (Some(b), Some(d)) = (hex_n::<32>(&blob_id), hex_n::<32>(&digest)) else {
                        return bad();
                    };
                    match s.receipt(authenticated, &b, &d, length) {
                        Ok(()) => done(),
                        Err(e) => refused(transfer_err(&e)),
                    }
                }
                TransferOp::View => {
                    json!({ "inFlight": s.in_flight(), "confirmed": s.is_confirmed(), "paused": *paused })
                }
                _ => bad(),
            }
        }
        op => {
            let Some(r) = h.receivers.get_mut(handle) else {
                return refused("unknown_handle");
            };
            match op {
                TransferOp::Accept { offset, len } => match r.accept(&Chunk { offset, len }) {
                    Ok(()) => {
                        json!({ "acknowledged": r.acknowledged(), "complete": r.is_complete() })
                    }
                    Err(e) => refused(transfer_err(&e)),
                },
                TransferOp::View => {
                    json!({ "acknowledged": r.acknowledged(), "complete": r.is_complete() })
                }
                _ => bad(),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LinkOp {
    Propose,
    ChildAccepts,
    SpaceRevokes,
    ChildRevokes,
    /// Needs an official link under the same handle; the Room's quorum approval is the caller's check.
    CreateBinding {
        space_id: String,
        governance: Vec<ApproverIn>,
        capabilities: Vec<String>,
        freshness_bound: u64,
        accepts_subdelegation: bool,
    },
    /// A Space-side unlink ends the binding for good; re-linking never restores it.
    Unlink,
    View,
}

/// One Room-to-Space link and its binding under a handle. In memory only: the Room-side binding wire is not frozen.
pub fn space_binding_do(h: &mut Handles, handle: &str, op: LinkOp) -> Value {
    use om_core::space_binding::{Binding, BindingError, ChildLink};
    if matches!(op, LinkOp::Propose) {
        if !h.links.contains_key(handle) && h.links.len() >= MAX_HANDLES {
            return refused("too_many_handles");
        }
        h.links
            .insert(handle.to_string(), ChildLink::proposed_by_space());
        h.bindings.remove(handle);
        return done();
    }
    let Some(l) = h.links.get_mut(handle) else {
        return refused("unknown_handle");
    };
    match op {
        LinkOp::Propose => unreachable!(),
        LinkOp::ChildAccepts => l.child_accepts(),
        LinkOp::SpaceRevokes => l.space_revokes(),
        LinkOp::ChildRevokes => l.child_revokes(),
        LinkOp::CreateBinding {
            space_id,
            governance,
            capabilities,
            freshness_bound,
            accepts_subdelegation,
        } => {
            let (Some(sp), Some(gov)) = (hex_n::<32>(&space_id), approvers(&governance)) else {
                return refused("malformed");
            };
            return match Binding::new(
                l,
                sp,
                gov.into_iter().collect(),
                capabilities.into_iter().collect(),
                freshness_bound,
                accepts_subdelegation,
            ) {
                Ok(b) => {
                    h.bindings.insert(handle.to_string(), b);
                    done()
                }
                Err(e) => refused(match e {
                    BindingError::LinkNotOfficial => "link_not_official",
                    BindingError::Protected => "binding_protected",
                    BindingError::EmptyCapabilities => "empty_capabilities",
                }),
            };
        }
        LinkOp::Unlink => {
            l.space_revokes();
            if let Some(b) = h.bindings.remove(handle) {
                b.space_unlinked();
            }
        }
        LinkOp::View => {}
    }
    let l = h.links[handle];
    json!({ "official": l.is_official(), "spaceSide": l.space_side, "childSide": l.child_side, "bound": h.bindings.contains_key(handle) })
}

// ---- LanFrameV1 transfer frames ---------------------------------------------------------------------------------------

use om_core::lan_frame::{Frame, encode_frame};

fn hex_out(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FrameIn {
    Begin {
        transfer_id: String,
        object_kind: u64,
        total_bytes: u64,
        sha256: String,
    },
    Chunk {
        transfer_id: String,
        offset: u64,
        bytes: String,
    },
    End {
        transfer_id: String,
    },
    Ack {
        transfer_id: String,
        offset: u64,
    },
    Error {
        id: String,
        code: u64,
    },
    Receipt {
        transfer_id: String,
        tag: String,
    },
    FetchAsyncKey {
        request_id: String,
        device_id: String,
        profile: u64,
        capability: String,
    },
    FetchBlob {
        request_id: String,
        capability: String,
        blob_id: String,
        ciphertext_digest: String,
    },
}

/// Canonical `LanFrameV1` bytes (hex) for one transfer frame; the codec refuses an out-of-range value on the way back in.
pub fn lan_frame_encode(f: FrameIn) -> Value {
    let id = |s: &str| hex_n::<16>(s);
    let frame = match f {
        FrameIn::Begin {
            transfer_id,
            object_kind,
            total_bytes,
            sha256,
        } => {
            let (Some(t), Some(d)) = (id(&transfer_id), hex_n::<32>(&sha256)) else {
                return refused("malformed");
            };
            Frame::Begin {
                transfer_id: t,
                object_kind,
                total_bytes,
                sha256: d,
            }
        }
        FrameIn::Chunk {
            transfer_id,
            offset,
            bytes,
        } => {
            let (Some(t), Some(b)) = (id(&transfer_id), hex_vec(&bytes)) else {
                return refused("malformed");
            };
            Frame::Chunk {
                transfer_id: t,
                offset,
                bytes: b,
            }
        }
        FrameIn::FetchAsyncKey {
            request_id,
            device_id,
            profile,
            capability,
        } => {
            let (Some(r), Some(d), Some(c)) =
                (id(&request_id), id(&device_id), hex_vec(&capability))
            else {
                return refused("malformed");
            };
            Frame::FetchAsyncKey {
                request_id: r,
                device_id: d,
                profile,
                capability: c,
            }
        }
        FrameIn::FetchBlob {
            request_id,
            capability,
            blob_id,
            ciphertext_digest,
        } => {
            let (Some(r), Some(c), Some(b), Some(d)) = (
                id(&request_id),
                hex_vec(&capability),
                hex_n::<32>(&blob_id),
                hex_n::<32>(&ciphertext_digest),
            ) else {
                return refused("malformed");
            };
            Frame::FetchBlob {
                request_id: r,
                capability: c,
                blob_id: b,
                ciphertext_digest: d,
            }
        }
        FrameIn::End { transfer_id } => {
            let Some(t) = id(&transfer_id) else {
                return refused("malformed");
            };
            Frame::End { transfer_id: t }
        }
        FrameIn::Ack {
            transfer_id,
            offset,
        } => {
            let Some(t) = id(&transfer_id) else {
                return refused("malformed");
            };
            Frame::Ack {
                transfer_id: t,
                offset,
            }
        }
        FrameIn::Error { id: i, code } => {
            let Some(t) = id(&i) else {
                return refused("malformed");
            };
            Frame::Error { id: t, code }
        }
        FrameIn::Receipt { transfer_id, tag } => {
            let (Some(t), Some(g)) = (id(&transfer_id), hex_n::<32>(&tag)) else {
                return refused("malformed");
            };
            Frame::Receipt {
                transfer_id: t,
                tag: g,
            }
        }
    };
    match encode_frame(&frame) {
        Some(b) if om_core::lan_frame::decode(&b).is_ok() => json!({ "frame": hex_out(&b) }),
        _ => refused("value_range"),
    }
}

/// One channel message to a typed frame, or the codec's stable refusal class.
/// The signalling kind and 16-byte session id of a seal request, or `None` when either is malformed.
pub fn lan_signal_args(
    kind: &str,
    session_id: &str,
) -> Option<(om_core::crypto::lan_signal::Kind, [u8; 16])> {
    use om_core::crypto::lan_signal::Kind;
    let kind = match kind {
        "offer" => Kind::Offer,
        "answer" => Kind::Answer,
        _ => return None,
    };
    Some((kind, hex_vec(session_id)?.try_into().ok()?))
}

pub fn lan_hex(b: &[u8]) -> String {
    hex_out(b)
}

pub fn lan_bytes(hex: &str) -> Option<Vec<u8>> {
    hex_vec(hex)
}

fn signal_refusal(e: om_core::crypto::lan_signal::SignalError) -> &'static str {
    use om_core::crypto::lan_signal::SignalError::*;
    match e {
        Malformed => "malformed",
        Epoch => "epoch",
        Replay => "replay",
        Decrypt => "decrypt",
        Payload => "payload",
        NoSeed => "no_seed",
    }
}

pub fn lan_signal_sealed(r: Result<Vec<u8>, om_core::crypto::lan_signal::SignalError>) -> Value {
    match r {
        Ok(b) => json!({ "envelope": hex_out(&b) }),
        Err(e) => refused(signal_refusal(e)),
    }
}

pub fn lan_signal_opened(
    r: Result<
        (om_core::crypto::lan_signal::Kind, [u8; 16], String),
        om_core::crypto::lan_signal::SignalError,
    >,
) -> Value {
    use om_core::crypto::lan_signal::Kind;
    match r {
        Ok((k, sid, sdp)) => json!({
            "kind": if k == Kind::Offer { "offer" } else { "answer" },
            "sessionId": hex_out(&sid),
            "sdp": sdp,
        }),
        Err(e) => refused(signal_refusal(e)),
    }
}

pub fn lan_frame_decode(bytes_hex: &str) -> Value {
    let Some(b) = hex_vec(bytes_hex) else {
        return refused("malformed");
    };
    match om_core::lan_frame::decode(&b) {
        Err(e) => refused(e),
        Ok(Frame::Begin {
            transfer_id,
            object_kind,
            total_bytes,
            sha256,
        }) => {
            json!({ "kind": "begin", "transferId": hex_out(&transfer_id), "objectKind": object_kind, "totalBytes": total_bytes, "sha256": hex_out(&sha256) })
        }
        Ok(Frame::Chunk {
            transfer_id,
            offset,
            bytes,
        }) => {
            json!({ "kind": "chunk", "transferId": hex_out(&transfer_id), "offset": offset, "bytes": hex_out(&bytes) })
        }
        Ok(Frame::End { transfer_id }) => {
            json!({ "kind": "end", "transferId": hex_out(&transfer_id) })
        }
        Ok(Frame::Ack {
            transfer_id,
            offset,
        }) => json!({ "kind": "ack", "transferId": hex_out(&transfer_id), "offset": offset }),
        Ok(Frame::Error { id, code }) => {
            json!({ "kind": "error", "id": hex_out(&id), "code": code })
        }
        Ok(Frame::Receipt { transfer_id, tag }) => {
            json!({ "kind": "receipt", "transferId": hex_out(&transfer_id), "tag": hex_out(&tag) })
        }
        Ok(Frame::FetchAsyncKey {
            request_id,
            device_id,
            profile,
            capability,
        }) => {
            json!({ "kind": "fetch_async_key", "requestId": hex_out(&request_id), "deviceId": hex_out(&device_id), "profile": profile, "capability": hex_out(&capability) })
        }
        Ok(Frame::FetchBlob {
            request_id,
            capability,
            blob_id,
            ciphertext_digest,
        }) => {
            json!({ "kind": "fetch_blob", "requestId": hex_out(&request_id), "capability": hex_out(&capability), "blobId": hex_out(&blob_id), "ciphertextDigest": hex_out(&ciphertext_digest) })
        }
    }
}

/// `service` or `bridge`.
pub fn control_kind(s: &str) -> Option<om_core::room_control::ObjectKind> {
    match s {
        "service" => Some(om_core::room_control::ObjectKind::Service),
        "bridge" => Some(om_core::room_control::ObjectKind::Bridge),
        _ => None,
    }
}

fn snake(debug: String) -> String {
    let mut out = String::new();
    for (i, c) in debug.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

pub fn control_observed(
    r: Result<om_core::service_control::ControlOutcome, om_core::engine::ObserveError>,
) -> Value {
    use om_core::engine::ObserveError;
    use om_core::service_control::ControlOutcome;
    match r {
        Ok(ControlOutcome::Accepted) => json!({ "outcome": "accepted" }),
        Ok(ControlOutcome::Duplicate) => json!({ "outcome": "duplicate" }),
        Ok(ControlOutcome::Forked) => json!({ "outcome": "forked" }),
        Err(ObserveError::Open(e)) => json!({ "refused": snake(format!("{e:?}")) }),
        Err(ObserveError::Control(e)) => json!({ "refused": snake(format!("{e:?}")) }),
    }
}

pub fn control_view(v: &[om_core::engine::RoomControlEntry]) -> Value {
    let list: Vec<Value> = v
        .iter()
        .map(|e| {
            json!({
                "kind": match e.kind {
                    om_core::room_control::ObjectKind::Service => "service",
                    om_core::room_control::ObjectKind::Bridge => "bridge",
                },
                "id": lan_hex(&e.id),
                "headVersion": e.head_version,
                "headHash": e.head_hash.map(|hash| lan_hex(&hash)),
                "frozen": e.frozen,
                "endpoints": e.endpoints.iter().map(|(id, state, usable)| json!({
                    "id": lan_hex(id), "state": state, "usable": usable,
                })).collect::<Vec<_>>(),
                "observedAgeMs": e.observed_age_ms,
                "reportedApprovers": e.reported_approvers.iter().map(|(k, id)| json!({
                    "kind": k, "id": lan_hex(id),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "entries": list })
}

#[cfg(test)]
mod reducer_fixture_tests {
    use super::*;
    use crate::bridge::Bridge;
    use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;

    fn session() -> impl FnMut(Value) -> Value {
        let mut bridge = Bridge::new(
            || InsecureTestRng::from_label(b"scope"),
            || 1_790_000_000_000,
        )
        .with_test_only_scope_reducers();
        move |v| serde_json::from_slice(&bridge.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
    }
    #[test]
    fn topics_run_through_one_bridge_instance() {
        let mut b = Bridge::new(
            || InsecureTestRng::from_label(b"scope"),
            || 1_790_000_000_000,
        )
        .with_test_only_scope_reducers();
        let mut go = |v: Value| -> Value {
            serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
        };
        let id = "01".repeat(16);
        let ev = "aa".repeat(32);
        let st = json!({"clock": 1, "event": "bb".repeat(32)});
        let r = go(
            json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"create","id":id,"title":"general","at":st}}),
        );
        assert_eq!(r["ok"]["refused"], "not_authorized");
        let r = go(
            json!({"cmd":"topics_do","caps":["room.create_topic"],"actor":"ann","op":{"kind":"create","id":id,"title":"general","at":st}}),
        );
        assert_eq!(r["ok"]["applied"], true);
        let r = go(
            json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"post","mode":"forum","topLevel":true,"event":ev,"author":"ann","topic":null}}),
        );
        assert_eq!(r["ok"]["refused"], "topic_required");
        let r = go(
            json!({"cmd":"topics_do","caps":[],"actor":"ann","op":{"kind":"post","mode":"forum","topLevel":true,"event":ev,"author":"ann","topic":id}}),
        );
        assert_eq!(r["ok"]["applied"], true);
        let r = go(
            json!({"cmd":"topics_do","caps":["room.manage_topics"],"actor":"bob","op":{"kind":"move","event":ev,"topic":null,"at":{"clock":5,"event":"cc".repeat(32)}}}),
        );
        assert_eq!(r["ok"]["applied"], true);
        let v = go(json!({"cmd":"topics_view","topic":id,"message":ev}));
        assert_eq!(v["ok"]["topic"]["title"], "general");
        assert_eq!(v["ok"]["placement"], json!({"topic": null}));
        let r = go(
            json!({"cmd":"topics_do","caps":[],"actor":"x","op":{"kind":"create","id":"zz","title":"t","at":st}}),
        );
        assert_eq!(r["ok"]["refused"], "malformed");
    }

    #[test]
    fn moderation_runs_through_the_bridge_and_stale_lifts_are_noops() {
        let mut b = Bridge::new(
            || InsecureTestRng::from_label(b"scope"),
            || 1_790_000_000_000,
        )
        .with_test_only_scope_reducers();
        let mut go = |v: Value| -> Value {
            serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
        };
        let inc = |n: u8| format!("{n:02x}").repeat(16);
        assert_eq!(
            go(
                json!({"cmd":"moderation_do","caps":[],"actor":"x","op":{"kind":"admit","who":"bob","incarnation":inc(3),"governance":false}})
            )["ok"]["applied"],
            true
        );
        let ban = |c: &str, t: &str, n: u8| json!({"cmd":"moderation_do","caps":[c],"actor":"mod","op":{"kind":"ban","target":t,"incarnation":inc(n),"governanceAuthority":false}});
        assert_eq!(
            go(ban("room.remove_member", "bob", 9))["ok"]["refused"],
            "not_authorized"
        );
        assert_eq!(go(ban("room.ban_member", "bob", 9))["ok"]["applied"], true);
        assert_eq!(
            go(json!({"cmd":"moderation_view","who":"bob"}))["ok"],
            json!({"member": false, "banned": true, "timedOut": false})
        );
        let unban = |n: u8| json!({"cmd":"moderation_do","caps":["room.ban_member"],"actor":"mod","op":{"kind":"unban","target":"bob","incarnation":inc(n)}});
        assert_eq!(
            go(unban(1))["ok"],
            json!({"applied": false, "noop": true}),
            "a stale incarnation changes nothing"
        );
        assert_eq!(
            go(json!({"cmd":"moderation_view","who":"bob"}))["ok"]["banned"],
            true
        );
        assert_eq!(go(unban(9))["ok"]["applied"], true);
    }

    #[test]
    fn first_contact_never_activates_without_both_bindings() {
        let mut b = Bridge::new(
            || InsecureTestRng::from_label(b"scope"),
            || 1_790_000_000_000,
        )
        .with_test_only_scope_reducers();
        let mut go = |v: Value| -> Value {
            serde_json::from_slice(&b.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
        };
        let step = |k: Value| json!({"cmd":"lan_first_contact","step":k});
        assert_eq!(
            go(step(json!({"kind":"ticket","verified":true})))["ok"]["refused"],
            "out_of_order"
        );
        assert_eq!(
            go(step(json!({"kind":"reach"})))["ok"]["state"],
            "reachable"
        );
        assert_eq!(
            go(step(json!({"kind":"ticket","verified":false})))["ok"]["refused"],
            "not_verified"
        );
        for k in [
            json!({"kind":"ticket","verified":true}),
            json!({"kind":"request","valid":true}),
            json!({"kind":"own_binding","verified":true}),
        ] {
            go(step(k));
        }
        let half = go(step(json!({"kind":"state"})));
        assert_eq!(
            (
                half["ok"]["state"].clone(),
                half["ok"]["asyncKeyFetchAllowed"].clone()
            ),
            (json!("half_bound"), json!(false))
        );
        let done = go(step(json!({"kind":"peer_binding","verified":true})));
        assert_eq!(
            (
                done["ok"]["state"].clone(),
                done["ok"]["deviceEligible"].clone()
            ),
            (json!("active"), json!(true))
        );
    }

    #[test]
    fn engineering_room_rejects_caller_capabilities_even_for_a_verified_room_member() {
        use om_core::api::IntentBody;
        use om_core::groups::flow::{GroupEvent, GroupIntent};
        use om_core::runtime::{LoopbackNetwork, Runtime};

        let mut runtime = Runtime::new("member", &LoopbackNetwork::default());
        let profile = runtime.open("member");
        runtime.dispatch(
            &profile,
            "identity",
            IntentBody::CreateIdentityProfile {
                label: "Member".into(),
            },
        );
        runtime.flush_net(&profile);
        let (reply, step) = runtime
            .core
            .group_do(
                &profile,
                GroupIntent::Create {
                    name: "Room".into(),
                    relationship_ids: vec![],
                },
            )
            .unwrap();
        assert!(runtime.settle(&step));
        let reply = reply.unwrap();
        let GroupEvent::Created { conversation_id } = &reply.events[0] else {
            panic!("Room not created")
        };
        let conversation = om_core::crypto::hex(conversation_id);
        let room = serde_json::to_string(&(&profile.profile_id, &conversation)).unwrap();
        let mut bridge = Bridge::new(
            || InsecureTestRng::from_label(b"scope"),
            || 1_790_000_000_000,
        )
        .with_runtime_clock(|| Some(30));
        let mut call = |v: Value| -> Value {
            serde_json::from_slice(&bridge.handle(&serde_json::to_vec(&v).unwrap())).unwrap()
        };
        assert!(
            call(json!({
                "cmd":"init","home":{"tier":"none","observers":["no_third_party"],
                "migrationAvailable":false,"clientOriginChoice":["default_origin"]}
            }))["err"]
                .is_null()
        );
        let ctx = call(json!({"cmd":"open_profile","profileId":profile.profile_id,"records":runtime.store.records(&profile.profile_id)}))["ok"].clone();
        assert!(ctx.is_object());
        let cmd =
            |engine, caps, op| json!({"cmd":engine,"room":room,"ctx":ctx,"caps":caps,"op":op});
        let open = json!({"kind":"open","quorum":null,"sequencingTimeout":10,"freshnessBound":30});
        assert_eq!(
            call(cmd("room_services_do", json!([]), open.clone()))["ok"],
            done()
        );
        assert_eq!(call(cmd("bridge_room_do", json!([]), open))["ok"], done());
        let service = "61".repeat(32);
        assert_eq!(
            call(cmd(
                "room_services_do",
                json!(["room.install_service"]),
                json!({"kind":"remove","service":service})
            ))["ok"],
            refused("not_authorized")
        );
        assert_eq!(
            call(cmd(
                "bridge_room_do",
                json!(["room.add_bridge"]),
                json!({"kind":"revoke_mapping","bridge":service})
            ))["ok"],
            refused("not_authorized")
        );
    }

    #[test]
    fn bridge_offer_requires_a_matching_verified_head_and_fresh_runtime_observation() {
        use om_core::engine::RoomControlEntry;
        use om_core::room_control::ObjectKind;
        let mut handles = Handles::default();
        let run = |handles: &mut Handles, op: Value, controls: &[RoomControlEntry], tick| {
            bridge_room_do(
                handles,
                "room",
                &[],
                serde_json::from_value(op).unwrap(),
                controls,
                Some(tick),
            )
        };
        assert_eq!(
            run(
                &mut handles,
                json!({"kind":"open","quorum":null,"sequencingTimeout":20,"freshnessBound":30}),
                &[],
                10
            ),
            done()
        );
        let offer = json!({
            "kind":"propose", "head": head(1,"01",1), "approvers": [], "name": "Bridge",
            "mapping": {"incarnation":"05".repeat(16),"namespace":"network","foreignScope":"group","direction":3,"allowedClasses":[1]},
        });
        assert_eq!(
            run(&mut handles, offer.clone(), &[], 10),
            refused("unverified_head")
        );
        let verified = RoomControlEntry {
            kind: ObjectKind::Bridge,
            id: [0x61; 32],
            head_version: Some(1),
            head_hash: Some([1; 32]),
            frozen: false,
            endpoints: vec![(vec![0x10; 16], 1, true)],
            observed_age_ms: 0,
            reported_approvers: vec![],
        };
        let proposed = run(&mut handles, offer, std::slice::from_ref(&verified), 10);
        assert!(proposed["target"].is_string());
        let admit =
            json!({"kind":"admit_offer","bridge":"61".repeat(32),"target":proposed["target"]});
        let newer = RoomControlEntry {
            head_version: Some(2),
            ..verified.clone()
        };
        assert_eq!(
            run(&mut handles, admit.clone(), &[newer], 15),
            refused("unverified_head")
        );
        let stale = RoomControlEntry {
            observed_age_ms: 31,
            ..verified.clone()
        };
        assert_eq!(
            run(&mut handles, admit.clone(), &[stale], 41),
            refused("unverified_head")
        );
        assert_eq!(
            run(&mut handles, admit, &[verified], 10),
            refused("not_authorized")
        );
    }

    #[test]
    fn room_mutation_receipts_and_snapshots_round_trip() {
        use om_core::engine::RoomControlEntry;
        use om_core::room_control::ObjectKind;

        for kind in [ObjectKind::Service, ObjectKind::Bridge] {
            let bridge = kind == ObjectKind::Bridge;
            let cap = if bridge {
                "room.add_bridge"
            } else {
                "room.install_service"
            };
            let caps = [cap.into()];
            let principal = if bridge { "bridge" } else { "service" };
            let install = if bridge {
                "admit_offer"
            } else {
                "install_offer"
            };
            let verified = RoomControlEntry {
                kind,
                id: [0x61; 32],
                head_version: Some(1),
                head_hash: Some([1; 32]),
                frozen: false,
                endpoints: vec![(vec![0x10; 16], 1, true)],
                observed_age_ms: 0,
                reported_approvers: vec![],
            };
            let other = RoomControlEntry {
                id: [0x62; 32],
                ..verified.clone()
            };
            let controls = [verified.clone(), other];
            let run = |h: &mut Handles, op: Value| {
                if bridge {
                    bridge_room_do(
                        h,
                        "room",
                        &caps,
                        serde_json::from_value(op).unwrap(),
                        &controls,
                        Some(50),
                    )
                } else {
                    room_services_do(
                        h,
                        "room",
                        &caps,
                        serde_json::from_value(op).unwrap(),
                        &controls,
                    )
                }
            };
            let mut original = Handles::default();
            assert_eq!(
                run(
                    &mut original,
                    json!({
                        "kind":"open","quorum":null,"sequencingTimeout":10,"freshnessBound":30,
                    })
                ),
                done()
            );

            let propose = |id: &str| {
                let mut facts = head(1, "01", 1);
                facts["serviceId"] = json!(id);
                json!({
                    "kind":"propose", "head":facts, "name":"Example", "approvers": [],
                    "incarnation":"05".repeat(16), "capabilities":["message.create"], "filterClasses":[1],
                    "mapping":{"incarnation":"05".repeat(16),"namespace":"test","foreignScope":"room","direction":3,"allowedClasses":[1]},
                })
            };
            let id = "61".repeat(32);
            let target = run(&mut original, propose(&id))["target"].clone();
            assert!(target.is_string());
            assert!(run(&mut original, propose(&"62".repeat(32)))["target"].is_string());
            let operation_id = "0a".repeat(32);
            let intent = json!({ "kind":install, (principal):id, "target":target, "operationId":operation_id });
            assert_eq!(run(&mut original, intent.clone()), done());
            assert_eq!(run(&mut original, intent.clone()), done());
            let mut changed = intent.clone();
            changed["target"] = json!("bb".repeat(32));
            assert_eq!(run(&mut original, changed), refused("operation_conflict"));

            let export = |h: &Handles| h.export_room_snapshot("room", kind).unwrap().unwrap();
            let snapshot = export(&original);
            let mut reopened = Handles::default();
            assert_eq!(
                reopened.import_room_snapshot("room", kind, &snapshot, &controls, 3),
                done()
            );
            assert_eq!(export(&reopened), snapshot);
            let list = run(&mut reopened, json!({"kind":"list"}));
            let rows = list[if bridge { "bridges" } else { "services" }]
                .as_array()
                .unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(
                rows[0]["membership"],
                json!(if bridge { "member" } else { "installed" })
            );
            assert_eq!(rows[1]["membership"], "proposed");
            assert_eq!(
                run(
                    &mut reopened,
                    json!({
                        "kind":"operation","operationId":operation_id,"mutation":install,"principal":id,"target":target,
                    })
                ),
                json!({ "result": done() })
            );

            let remove = json!({"kind":"remove_member",(principal):id,"target":target,"operationId":"0b".repeat(32)});
            assert_eq!(
                run(&mut reopened, remove.clone()),
                json!({"removedEndpoints":1})
            );
            assert_eq!(run(&mut reopened, remove), json!({"removedEndpoints":1}));
            assert_eq!(run(&mut reopened, intent), done());
            let list = run(&mut reopened, json!({"kind":"list"}));
            assert_eq!(
                list[if bridge { "bridges" } else { "services" }][0]["membership"],
                "removed"
            );

            // The newer current head drops the cached proposal while the independently verified member survives.
            let newer = RoomControlEntry {
                head_version: Some(2),
                ..controls[1].clone()
            };
            let mut filtered = Handles::default();
            assert_eq!(
                filtered.import_room_snapshot("room", kind, &snapshot, &[verified, newer], 3),
                done()
            );
            let list = run(&mut filtered, json!({"kind":"list"}));
            assert_eq!(
                list[if bridge { "bridges" } else { "services" }]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn room_operation_log_evicts_only_the_oldest_at_sixty_four() {
        let mut handles = Handles::default();
        let run = |h: &mut Handles, op: Value| {
            room_services_do(
                h,
                "room",
                &["room.install_service".into()],
                serde_json::from_value(op).unwrap(),
                &[],
            )
        };
        run(
            &mut handles,
            json!({"kind":"open","quorum":null,"sequencingTimeout":10}),
        );
        let service = "61".repeat(32);
        for n in 0..65 {
            assert_eq!(
                run(
                    &mut handles,
                    json!({
                        "kind":"remove","service":service,"operationId":format!("{n:064x}"),
                    })
                ),
                refused("not_installed")
            );
        }
        for (n, result) in [
            (0, Value::Null),
            (1, refused("not_installed")),
            (64, refused("not_installed")),
        ] {
            assert_eq!(
                run(
                    &mut handles,
                    json!({
                        "kind":"operation","mutation":"remove","principal":service,"operationId":format!("{n:064x}"),
                    })
                ),
                json!({"result":result})
            );
        }
    }

    #[test]
    fn accepted_revocations_reach_both_membership_reducers() {
        use om_core::bridge_room::ADD_BRIDGE;
        use om_core::room_control::ObjectKind;
        let mut handles = Handles::default();
        let initial = super::head(&serde_json::from_value(head(1, "01", 1)).unwrap()).unwrap();
        let revoked = super::head(&serde_json::from_value(head(2, "02", 3)).unwrap()).unwrap();
        let mut services = ServiceRoom::new(None, 10);
        services
            .install(
                &[om_core::service_room_member::INSTALL.into()].into(),
                &initial,
                vec![1; 16],
                None,
                &[],
            )
            .unwrap();
        handles.rooms.insert("room".into(), services);
        let mut bridges = BridgeRoom::new(None, 10, 30);
        bridges
            .admit(
                &[ADD_BRIDGE.into()].into(),
                &initial,
                Mapping {
                    incarnation: [1; 16],
                    namespace: "network".into(),
                    foreign_scope: "group".into(),
                    direction: 3,
                    allowed_classes: [1].into(),
                    active: true,
                },
                &[],
                0,
            )
            .unwrap();
        handles.bridge_rooms.insert("room".into(), bridges);

        handles.observe_room_control("room", ObjectKind::Service, &revoked, true, 20);
        handles.observe_room_control("room", ObjectKind::Bridge, &revoked, false, 20);
        for members in [
            handles.rooms["room"].members(),
            handles.bridge_rooms["room"].members(),
        ] {
            assert!(members.must_stop_sending(&initial.service_id, &[0x10; 16]));
            assert_eq!(
                members.may_commit_removal(&initial.service_id, &[0x10; 16], false, 29),
                Err(RoomError::TooEarly)
            );
            assert!(
                members
                    .may_commit_removal(&initial.service_id, &[0x10; 16], false, 30)
                    .is_ok()
            );
        }
    }

    #[test]
    fn broker_review_binds_order_service_expiry_and_trigger() {
        let review = |value| service_invocation_review(serde_json::from_value(value).unwrap(), 100);
        let sel = json!({
            "service":"61".repeat(32), "command":"fetch_url_via_scoped_network_provider",
            "destination":"endpoint", "expiresAtMs":200,
            "items":[{"id":"trigger","kind":"trigger","value":"Context"},{"id":"p","kind":"participant","value":"Member"}],
        });
        let original = review(sel.clone());
        assert_eq!(original["refusal"], "not_granted");
        let mut changed = sel.clone();
        changed["service"] = json!("62".repeat(32));
        assert_ne!(review(changed)["target"], original["target"]);
        let mut changed = sel.clone();
        changed["expiresAtMs"] = json!(199);
        assert_ne!(review(changed)["target"], original["target"]);
        let mut changed = sel.clone();
        changed["items"].as_array_mut().unwrap().swap(0, 1);
        assert_ne!(review(changed)["target"], original["target"]);
        let mut changed = sel;
        changed["items"].as_array_mut().unwrap().remove(0);
        assert_eq!(review(changed)["refusal"], "malformed");
    }

    fn head(version: u64, hash: &str, state: u64) -> Value {
        json!({"serviceId": "61".repeat(32), "version": version, "hash": hash.repeat(32),
           "endpoints": [{"id": "10".repeat(16), "state": state}]})
    }

    #[test]
    fn a_room_service_keeps_its_state_under_a_handle() {
        let mut c = session();
        let svc = "61".repeat(32);
        let ep = "10".repeat(16);
        let ok = |v: &Value| v["ok"].clone();
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"open","quorum":2,"sequencingTimeout":300}})
            )),
            json!({"done": true})
        );
        let humans = json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}]);
        let install = |caps: Value, ap: &Value| json!({"cmd":"room_services_do","room":"r1","caps":caps,"op":{"kind":"install","head":head(1,"01",1),"incarnation":"05","filterClasses":[1],"approvers":ap}});
        assert_eq!(
            ok(&c(install(json!([]), &humans))),
            json!({"refused": "not_authorized"})
        );
        let one = json!([{"kind":1,"id":"01".repeat(32)},{"kind":2,"id":"09".repeat(32)}]);
        assert_eq!(
            ok(&c(install(json!(["room.install_service"]), &one))),
            json!({"refused": "quorum_not_met"})
        );
        assert_eq!(
            ok(&c(install(json!(["room.install_service"]), &humans))),
            json!({"done": true})
        );
        let v = ok(&c(
            json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"view","service":svc}}),
        ));
        assert_eq!(
            (
                v["installed"].clone(),
                v["canReadAllContent"].clone(),
                v["filterClasses"].clone()
            ),
            (json!(true), json!(true), json!([1]))
        );
        // A head that revokes the endpoint stops it; a second Room handle is untouched; rollback is refused.
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"observe_head","head":head(2,"02",3),"now":10}})
            )),
            json!({"done": true})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"must_stop","service":svc,"endpoint":ep}})
            )),
            json!({"mustStopSending": true})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"observe_head","head":head(1,"01",1),"now":11}})
            )),
            json!({"refused": "rollback"})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"may_commit_removal","service":svc,"endpoint":ep,"isCoordinator":false,"now":20}})
            )),
            json!({"refused": "too_early"})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"may_commit_removal","service":svc,"endpoint":ep,"isCoordinator":false,"now":310}})
            )),
            json!({"done": true})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"r1","caps":[],"op":{"kind":"removal_committed","service":svc,"endpoint":ep}})
            )),
            json!({"applied": true})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"room_services_do","room":"zz","caps":[],"op":{"kind":"view","service":svc}})
            )),
            json!({"refused": "unknown_handle"})
        );
    }

    #[test]
    fn a_bridge_room_decides_every_export() {
        let mut c = session();
        let b = "61".repeat(32);
        let ep = "10".repeat(16);
        let ok = |v: &Value| v["ok"].clone();
        c(
            json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"open","quorum":2,"sequencingTimeout":300,"freshnessBound":600}}),
        );
        let mapping = json!({"incarnation":"05".repeat(16),"namespace":"test.net","foreignScope":"external group","direction":3,"allowedClasses":[1,2]});
        let admit = |caps: Value, ap: Value| json!({"cmd":"bridge_room_do","room":"b1","caps":caps,"op":{"kind":"admit","head":head(1,"01",1),"mapping":mapping,"approvers":ap,"now":0}});
        // Another bridge principal (kind 3) must not count toward the human quorum.
        let mixed = json!([{"kind":1,"id":"01".repeat(32)},{"kind":3,"id":b}]);
        assert_eq!(
            ok(&c(admit(json!(["room.add_bridge"]), mixed))),
            json!({"refused": "quorum_not_met"})
        );
        assert_eq!(
            ok(&c(admit(
                json!(["room.install_service"]),
                json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}])
            ))),
            json!({"refused": "not_authorized"})
        );
        assert_eq!(
            ok(&c(admit(
                json!(["room.add_bridge"]),
                json!([{"kind":1,"id":"01".repeat(32)},{"kind":1,"id":"02".repeat(32)}])
            ))),
            json!({"done": true})
        );
        let export = |class: u8, now: u64| json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"may_export","bridge":b,"endpoint":ep,"class":class,"now":now}});
        assert_eq!(ok(&c(export(1, 100))), json!({"allowed": true}));
        assert_eq!(ok(&c(export(9, 100)))["blocked"], "class_not_allowed");
        assert_eq!(ok(&c(export(1, 601)))["blocked"], "stale_mapping");
        assert_eq!(
            ok(&c(
                json!({"cmd":"bridge_room_do","room":"b1","caps":["room.add_bridge"],"op":{"kind":"revoke_mapping","bridge":b}})
            )),
            json!({"done": true})
        );
        assert_eq!(ok(&c(export(1, 100)))["blocked"], "revoked");
        assert_eq!(
            ok(&c(
                json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"import_author_ok","authorKind":1}})
            )),
            json!({"allowed": false})
        );
        assert_eq!(
            ok(&c(
                json!({"cmd":"bridge_room_do","room":"b1","caps":[],"op":{"kind":"import_author_ok","authorKind":3}})
            )),
            json!({"allowed": true})
        );
    }

    #[test]
    fn a_lan_transfer_confirms_only_on_an_authenticated_receipt() {
        let mut c = session();
        let ok = |v: &Value| v["ok"].clone();
        let (blob, dig) = ("aa".repeat(32), "bb".repeat(32));
        c(
            json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"sender_open","blobId":blob,"digest":dig,"total":40000}}),
        );
        c(
            json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"receiver_open","total":40000,"restoredOffset":null}}),
        );
        let mut off = 0u64;
        loop {
            let r = ok(&c(
                json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"next_chunk"}}),
            ));
            if r["chunk"].is_null() {
                break;
            }
            let len = r["chunk"]["len"].as_u64().unwrap();
            assert_eq!(r["chunk"]["offset"], off);
            let a = ok(&c(
                json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"accept","offset":off,"len":len}}),
            ));
            off += len;
            assert_eq!(a["acknowledged"], off);
        }
        assert_eq!(off, 40000);
        c(
            json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"transport_ack","upTo":40000}}),
        );
        let v = ok(&c(
            json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"view"}}),
        ));
        assert_eq!(v["confirmed"], false);
        let rc = |auth: bool, d: &str| json!({"cmd":"lan_transfer_do","side":"sender","handle":"t","op":{"kind":"receipt","authenticated":auth,"blobId":blob,"digest":d,"length":40000}});
        assert_eq!(
            ok(&c(rc(false, &dig))),
            json!({"refused": "receipt_not_authenticated"})
        );
        assert_eq!(
            ok(&c(rc(true, &"cc".repeat(32)))),
            json!({"refused": "receipt_mismatch"})
        );
        assert_eq!(ok(&c(rc(true, &dig))), json!({"done": true}));
        let bad = ok(&c(
            json!({"cmd":"lan_transfer_do","side":"receiver","handle":"t","op":{"kind":"accept","offset":1,"len":5}}),
        ));
        assert!(bad["refused"] == "bad_chunk" || bad["refused"] == "overrun");
    }

    #[test]
    fn a_space_binding_needs_an_official_link_and_dies_with_the_unlink() {
        let mut c = session();
        let ok = |v: &Value| v["ok"].clone();
        let create = |caps: Value| json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"create_binding","spaceId":"aa".repeat(32),"governance":[{"kind":1,"id":"01".repeat(32)}],"capabilities":caps,"freshnessBound":60,"acceptsSubdelegation":false}});
        assert_eq!(
            ok(&c(create(json!(["room.remove_member"])))),
            json!({"refused": "unknown_handle"})
        );
        c(json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"propose"}}));
        assert_eq!(
            ok(&c(create(json!(["room.remove_member"])))),
            json!({"refused": "link_not_official"})
        );
        let v = ok(&c(
            json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"child_accepts"}}),
        ));
        assert_eq!(v["official"], true);
        assert_eq!(
            ok(&c(create(json!(["room.manage_governance"])))),
            json!({"refused": "binding_protected"})
        );
        assert_eq!(
            ok(&c(create(json!([])))),
            json!({"refused": "empty_capabilities"})
        );
        assert_eq!(
            ok(&c(create(json!(["room.remove_member"])))),
            json!({"done": true})
        );
        let v = ok(&c(
            json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"unlink"}}),
        ));
        assert_eq!(
            (v["official"].clone(), v["bound"].clone()),
            (json!(false), json!(false))
        );
        // Re-accepting does not restore the binding.
        let v = ok(&c(
            json!({"cmd":"space_binding_do","handle":"s","op":{"kind":"child_accepts"}}),
        ));
        assert_eq!(v["bound"], false);
    }
}
