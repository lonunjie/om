//! The capability broker: the single enforcement point between service output and host authority.
//! Rules cite SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md (SP) and SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md (SC).

use crate::model::ToolRequest;
use crate::world::{DataAccess, Effect, Installation, Invocation, World};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(&'static str),
    /// Queued for explicit user/governance approval; never auto-approved by the broker.
    NeedsApproval,
}

/// Typed host tools and the capability each needs (SP "Tool use and prompt-injection boundary").
fn required_capability(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "create_draft" => "draft.create",
        "send_message" => "message.send",
        "read_context" => "context.read",
        "search_local" => "local.search",
        "fetch_url" => "network.fetch",
        "propose_room_action" => "room.propose_action",
        "request_capability" => "",
        _ => return None,
    })
}

/// Things SP says no service receives by default and that OM exposes no host tool for at all.
const NEVER_TOOLS: &[&str] = &[
    "export_keys", "read_device_vault", "shell_exec", "read_file", "write_file", "account_control", "approve_escalation",
    "grant_capability", "manage_members", "add_bridge", "change_governance", "install_service", "send_as_account",
];

#[derive(Default)]
pub struct Broker {
    calls: BTreeMap<u64, u32>,
    sends_per_root: BTreeMap<(String, u64), u32>,
    sent_texts: BTreeSet<(String, u64, String)>,
    tainted: BTreeSet<u64>,
    pub log: Vec<(u64, String, String)>, // (invocation, tool, decision)
    /// Fault injection for the harness: disable loop/budget enforcement to show what the broker prevents.
    pub insecure_disable_loop_limits: bool,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.contains('@') || host.contains(':') || host.is_empty() {
        return None; // userinfo / explicit ports are refused rather than parsed ambiguously
    }
    Some(host)
}

impl Broker {
    pub fn start(&mut self, inv: &Invocation, world: &mut World) {
        if !inv.bundle.is_empty() {
            self.tainted.insert(inv.invocation_id); // SP "plaintext-read vs network egress": disclosed plaintext taints
        }
        world.invocations.push((inv.invocation_id, inv.root_invocation_id, inv.hop_count, inv.service_id.clone(), !inv.bundle.is_empty()));
    }

    /// Automation trigger rule (SP "Bot-to-bot / agent-to-agent loop safety"; SC section 14).
    pub fn should_trigger(&self, inst: &Installation, author_is_service: bool, author: &str, parent_hop: u32, room: &str, text: &str) -> bool {
        let mentioned = text.contains(&format!("@{}", inst.service_id));
        let in_scope = inst.scope_rooms.iter().any(|r| r == room);
        let not_self = author != inst.service_id;
        let service_ok = !author_is_service || inst.respond_to_services;
        let hop_ok = self.insecure_disable_loop_limits || parent_hop < inst.hop_limit;
        mentioned && in_scope && not_self && service_ok && hop_ok
    }

    pub fn decide(&mut self, inst: &Installation, inv: &Invocation, req: &ToolRequest, world: &mut World) -> (Decision, Option<String>) {
        let d = self.decide_inner(inst, inv, req, world);
        self.log.push((inv.invocation_id, req.tool.clone(), format!("{:?}", d.0)));
        d
    }

    fn decide_inner(&mut self, inst: &Installation, inv: &Invocation, req: &ToolRequest, world: &mut World) -> (Decision, Option<String>) {
        let id = inv.invocation_id;
        let calls = self.calls.entry(id).or_insert(0);
        *calls += 1;
        if *calls > inst.max_calls_per_invocation && !self.insecure_disable_loop_limits {
            return (Decision::Deny("invocation_call_budget"), None);
        }
        if NEVER_TOOLS.contains(&req.tool.as_str()) {
            return (Decision::Deny("no_such_host_authority"), None);
        }
        let Some(cap) = required_capability(&req.tool) else { return (Decision::Deny("unknown_tool"), None) };
        if req.tool == "request_capability" {
            // SC section 13: capability expansion requires approval; the broker never grants.
            world.effects.push((id, Effect::CapabilityRequest { service: inst.service_id.clone(), capability: s(&req.args, "capability").unwrap_or("").into() }));
            return (Decision::NeedsApproval, None);
        }
        if !inst.granted_capabilities.contains(cap) {
            return (Decision::Deny("capability_not_granted"), None);
        }
        let room_ok = |r: &str| inst.scope_rooms.iter().any(|x| x == r);
        match req.tool.as_str() {
            "create_draft" => {
                let room = s(&req.args, "room").map(String::from);
                if room.as_deref().is_some_and(|r| !room_ok(r)) {
                    return (Decision::Deny("room_out_of_scope"), None);
                }
                world.effects.push((id, Effect::Draft { service: inst.service_id.clone(), room, text: s(&req.args, "text").unwrap_or("").into() }));
                (Decision::Allow, None)
            }
            "read_context" => {
                let item = s(&req.args, "item").unwrap_or("");
                // Selected-context mode: only items in the disclosure bundle. Full-member: its Room's messages too.
                let text = inv.bundle.iter().find(|(i, _)| i == item).map(|x| x.1.clone()).or_else(|| {
                    (inst.data_access_mode == DataAccess::FullRoomMember)
                        .then(|| {
                            let n: usize = item.strip_prefix("room:")?.parse().ok()?;
                            inv.room.as_ref().and_then(|r| world.rooms.get(r)).and_then(|m| m.get(n)).map(|x| x.1.clone())
                        })
                        .flatten()
                });
                match text {
                    None => (Decision::Deny("context_not_disclosed"), None),
                    Some(t) => {
                        self.tainted.insert(id);
                        world.effects.push((id, Effect::ReadContext { service: inst.service_id.clone(), item: item.into() }));
                        (Decision::Allow, Some(t))
                    }
                }
            }
            "search_local" => {
                if inst.data_access_mode != DataAccess::LocalPersonal {
                    return (Decision::Deny("local_index_not_granted"), None);
                }
                self.tainted.insert(id);
                (Decision::Allow, Some(world.local_index.join("\n")))
            }
            "fetch_url" => {
                let url = s(&req.args, "url").unwrap_or("");
                let Some(host) = host_of(url) else { return (Decision::Deny("url_not_https_or_ambiguous"), None) };
                if !inst.network_allowlist.iter().any(|h| h == host) {
                    return (Decision::Deny("host_not_allowlisted"), None);
                }
                if self.tainted.contains(&id) && !inst.plaintext_egress {
                    return (Decision::Deny("plaintext_egress_not_granted"), None);
                }
                world.effects.push((id, Effect::Fetch { service: inst.service_id.clone(), url: url.into() }));
                (Decision::Allow, Some(world.web.get(url).cloned().unwrap_or_default()))
            }
            "propose_room_action" => {
                let room = s(&req.args, "room").unwrap_or("");
                if !room_ok(room) {
                    return (Decision::Deny("room_out_of_scope"), None);
                }
                world.effects.push((id, Effect::ProposedRoomAction { service: inst.service_id.clone(), room: room.into(), action: s(&req.args, "action").unwrap_or("").into() }));
                (Decision::NeedsApproval, None) // a proposal, never an executed governance action
            }
            "send_message" => {
                let room = s(&req.args, "room").or(inv.room.as_deref()).unwrap_or("");
                let text = s(&req.args, "text").unwrap_or("").to_string();
                if !room_ok(room) {
                    return (Decision::Deny("room_out_of_scope"), None);
                }
                if s(&req.args, "author").is_some_and(|a| a != inst.service_id) {
                    return (Decision::Deny("cannot_author_as_other_principal"), None); // SP invariant 8
                }
                let via = match s(&req.args, "on_behalf_of") {
                    None => None,
                    Some(acct) => match &inst.representation {
                        Some(g) if g.represented_account == acct && g.rooms.iter().any(|r| r == room) && g.allowed_actions.iter().any(|a| a == "send_message") => Some(acct.to_string()),
                        _ => return (Decision::Deny("no_representation_grant"), None),
                    },
                };
                if !self.insecure_disable_loop_limits {
                    if inv.hop_count > inst.hop_limit {
                        return (Decision::Deny("hop_limit"), None);
                    }
                    let n = self.sends_per_root.entry((inst.service_id.clone(), inv.root_invocation_id)).or_insert(0);
                    if *n >= inst.max_sends_per_root {
                        return (Decision::Deny("send_budget_per_root"), None);
                    }
                    if !self.sent_texts.insert((inst.service_id.clone(), inv.root_invocation_id, text.clone())) {
                        return (Decision::Deny("repeated_output_loop"), None);
                    }
                    *n += 1;
                }
                world.rooms.entry(room.into()).or_default().push((inst.service_id.clone(), text.clone()));
                world.effects.push((id, Effect::Message { author_kind: "service".into(), author: inst.service_id.clone(), via_account: via, room: room.into(), text,
                                                          root: inv.root_invocation_id, hop: inv.hop_count }));
                (Decision::Allow, None)
            }
            _ => (Decision::Deny("unknown_tool"), None),
        }
    }
}
