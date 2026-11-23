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
