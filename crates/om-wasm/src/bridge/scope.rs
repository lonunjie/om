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
