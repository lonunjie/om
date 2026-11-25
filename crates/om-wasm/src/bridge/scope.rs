// SPDX-License-Identifier: MPL-2.0
//! Typed JSON queries for the services and bridges scopes (feature `scope-engines`, engineering artifact only).
//! Stateless: each call carries what it needs and returns a decision; no keys, no host state, no authority.

use om_core::bridge_fidelity::{Carry, Op, Plan, Support};
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
use om_core::service_room_member::{RoomError, RoomServices, TriggerFilter};
use std::collections::BTreeMap;

/// Upper bound per handle kind; a host that needs more closes one first.
const MAX_HANDLES: usize = 64;

#[derive(Default)]
pub struct Handles {
    rooms: BTreeMap<String, RoomServices>,
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

fn approvers(a: &[ApproverIn]) -> Option<Vec<(u64, Vec<u8>)>> {
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
    },
    View {
        service: String,
    },
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

pub fn room_services_do(h: &mut Handles, room: &str, caps: &[String], op: RoomOp) -> Value {
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
            RoomServices::new(quorum, sequencing_timeout),
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
            match r.set_filter(&caps, &s, filter(&filter_classes)) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::ObserveHead { head: hd, now } => {
            let Some(hd) = head(&hd) else {
                return bad();
            };
            match r.observe_head(&hd, now) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::MustStop { service, endpoint } => {
            let (Some(s), Some(e)) = (hex_n::<32>(&service), hex_vec(&endpoint)) else {
                return bad();
            };
            json!({ "mustStopSending": r.must_stop_sending(&s, &e) })
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
            match r.may_commit_removal(&s, &e, is_coordinator, now) {
                Ok(()) => done(),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::RemovalCommitted { service, endpoint } => {
            let (Some(s), Some(e)) = (hex_n::<32>(&service), hex_vec(&endpoint)) else {
                return bad();
            };
            json!({ "applied": r.removal_committed(&s, &e) })
        }
        RoomOp::Remove { service } => {
            let Some(s) = hex_n::<32>(&service) else {
                return bad();
            };
            match r.remove_installation(&caps, &s) {
                Ok(gone) => json!({ "removedEndpoints": gone.len() }),
                Err(e) => refused(room_err(&e)),
            }
        }
        RoomOp::View { service } => {
            let Some(s) = hex_n::<32>(&service) else {
                return bad();
            };
            roster_view(r.entry(&s), r.held_head(&s))
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingIn {
    incarnation: String,
    namespace: String,
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
    },
    RevokeMapping {
        bridge: String,
    },
    MayExport {
        bridge: String,
        endpoint: String,
        class: u8,
        now: u64,
    },
    ImportAuthorOk {
        author_kind: u64,
    },
    View {
        bridge: String,
    },
}

pub fn bridge_room_do(h: &mut Handles, room: &str, caps: &[String], op: BridgeRoomOp) -> Value {
    use om_core::bridge_room::Blocked;
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
        BridgeRoomOp::Remove { bridge } => {
            let Some(b) = hex_n::<32>(&bridge) else {
                return bad();
            };
            match r.remove(&caps, &b) {
                Ok(gone) => json!({ "removedEndpoints": gone.len() }),
                Err(e) => refused(bridge_room_err(&e)),
            }
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
            match r.may_export(&b, &e, class, now) {
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
        BridgeRoomOp::View { bridge } => {
            let Some(b) = hex_n::<32>(&bridge) else {
                return bad();
            };
            roster_view(r.roster(&b), r.members().held_head(&b))
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
        Ok(Frame::Request { kind }) => json!({ "kind": "request", "frameKind": kind }),
    }
}
