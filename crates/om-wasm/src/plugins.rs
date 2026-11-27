// SPDX-License-Identifier: MPL-2.0
//! JSON commands for the scope-engines plugin policy harness. The embedding host verifies signatures, grants,
//! channel binding and user consent before passing those facts here. These commands perform policy checks only.

use om_core::crypto::{hex, unhex};
use om_core::plugin_control::{
    ControlOutcome, Grant, GrantError, GrantTable, Manifest, PluginChain, PluginState,
    capability_enabled, update_decision,
};
use om_core::plugin_host::{
    Budgets, Capability, EgressFacts, HostConfig, PluginError, PluginHost, UI_MAX_NODES, UiNode,
    validate_ui_tree,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_HANDLES: usize = 64;
const MAX_CONTROL_BYTES: usize = 65_536;
const MAX_GRANT_BYTES: usize = 16_384;

use PluginError::*;

macro_rules! error_codes {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        fn error_name(error: PluginError) -> &'static str {
            match error {
                $(PluginError::$variant => $name),+
            }
        }

        fn error_registry() -> BTreeMap<&'static str, u8> {
            [$((error_name(PluginError::$variant), PluginError::$variant.code())),+]
                .into_iter()
                .collect()
        }
    };
}

error_codes! {
    MalformedRequest => "malformed_request",
    UnknownCapability => "unknown_capability",
    CapabilityNotGranted => "capability_not_granted",
    GrantExpired => "grant_expired",
    TaintedEgressDenied => "tainted_egress_denied",
    UserConfirmationRequired => "user_confirmation_required",
    BilateralConsentRequired => "bilateral_consent_required",
    OriginChannelMismatch => "origin_channel_mismatch",
    ResourceBudgetExceeded => "resource_budget_exceeded",
    HostPolicyDenied => "host_policy_denied",
    ReleaseNotAuthorized => "release_not_authorized",
    InvalidUiTree => "invalid_ui_tree",
}

struct Refusal {
    error: PluginError,
    reason: Option<String>,
}

impl From<PluginError> for Refusal {
    fn from(error: PluginError) -> Self {
        Self {
            error,
            reason: None,
        }
    }
}

impl Refusal {
    fn with_reason(error: PluginError, reason: impl std::fmt::Debug) -> Self {
        Self {
            error,
            reason: Some(format!("{reason:?}")),
        }
    }

    fn response(self) -> Value {
        let mut value = json!({ "refused": error_name(self.error), "code": self.error.code() });
        if let Some(reason) = self.reason {
            value["reason"] = json!(reason);
        }
        value
    }
}

type Reply = Result<Value, Refusal>;

fn done() -> Value {
    json!({ "done": true })
}

fn bytes(value: &str, max: usize) -> Result<Vec<u8>, PluginError> {
    if value.len() > max * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(MalformedRequest);
    }
    unhex(value).map_err(|_| MalformedRequest)
}

fn fixed<const N: usize>(value: &str) -> Result<[u8; N], PluginError> {
    bytes(value, N)?.try_into().map_err(|_| MalformedRequest)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Uint {
    Number(u64),
    Decimal(String),
}

impl Uint {
    fn get(self) -> Result<u64, PluginError> {
        match self {
            Self::Number(n) => Ok(n),
            Self::Decimal(s) if s.bytes().all(|b| b.is_ascii_digit()) => {
                s.parse().map_err(|_| MalformedRequest)
            }
            Self::Decimal(_) => Err(MalformedRequest),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CapabilityIn {
    Id(u64),
    Name(String),
}

impl CapabilityIn {
    fn get(&self) -> Result<Capability, PluginError> {
        match self {
            Self::Id(id) => Capability::ALL
                .into_iter()
                .find(|c| u64::from(c.id()) == *id),
            Self::Name(name) => Capability::parse(name),
        }
        .ok_or(UnknownCapability)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BudgetsIn {
    max_memory_bytes: Uint,
    max_fuel: Uint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EgressIn {
    policy_allows: bool,
    one_to_one: bool,
    both_consented: bool,
    one_shot_confirmed: bool,
    continuous: bool,
    export_authorized: bool,
}

impl From<EgressIn> for EgressFacts {
    fn from(facts: EgressIn) -> Self {
        Self {
            policy_allows: facts.policy_allows,
            one_to_one: facts.one_to_one,
            both_consented: facts.both_consented,
            one_shot_confirmed: facts.one_shot_confirmed,
            continuous: facts.continuous,
            export_authorized: facts.export_authorized,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeIn {
    id: String,
    kind: u8,
    parent: Option<String>,
    text: Option<String>,
    action: Option<String>,
}

impl NodeIn {
    fn get(self) -> Result<UiNode, PluginError> {
        let node = (|| {
            Ok::<_, PluginError>(UiNode {
                id: fixed(&self.id)?,
                kind: self.kind,
                parent: self.parent.as_deref().map(fixed::<16>).transpose()?,
                text: self.text,
                action: self.action,
            })
        })();
        node.map_err(|_| InvalidUiTree)
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum HostOp {
    Open {
        site_isolation: bool,
        budgets: BudgetsIn,
    },
    Close,
    Instantiate {
        imports: Vec<String>,
        grants: Vec<CapabilityIn>,
    },
    BeginInvocation,
    ReadContent {
        cap: CapabilityIn,
    },
    WritePrivate,
    Egress {
        cap: CapabilityIn,
        facts: EgressIn,
    },
    SendMessage {
        user_confirmed_exact_content: bool,
    },
    UpdateCapability {
        cap: CapabilityIn,
        fresh_release_grant: bool,
    },
    Update {
        surface: String,
        nodes: Value,
    },
    Account {
        memory_bytes: Uint,
        fuel: Uint,
    },
    ReachHostState,
    MiniAppMessage {
        origin_matches: bool,
        channel_matches: bool,
        imitates_host_prompt: bool,
    },
    Terminate,
    IsTainted,
    IsTerminated,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ChainOp {
    Genesis {
        plugin_id: String,
        anchor: String,
        body: String,
        approvers: Vec<String>,
    },
    Resume {
        anchor: String,
        verified_head: String,
    },
    Approve {
        body: String,
        approvers: Vec<String>,
    },
    UpdateDecision {
        seq: Uint,
        accepted_seq: Uint,
        pinned: Option<Uint>,
    },
    MatchesRelease {
        body: String,
    },
    View,
    ReleaseRevoked {
        seq: Uint,
    },
    Close,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum GrantOp {
    Open,
    Resume {
        verified_heads: Vec<String>,
    },
    Grant {
        body: String,
        issuer_authorized: bool,
    },
    Live {
        incarnation: String,
        now: Uint,
    },
    CapabilityEnabled {
        chain: String,
        seq: Uint,
        cap: CapabilityIn,
        scope: (Uint, String),
        now: Uint,
    },
    Close,
}

#[derive(Deserialize)]
#[serde(tag = "engine", rename_all = "snake_case", deny_unknown_fields)]
enum PluginOp {
    Registry,
    Host { handle: String, op: HostOp },
    Chain { handle: String, op: ChainOp },
    Grants { handle: String, op: GrantOp },
}

#[derive(Default)]
pub(crate) struct Handles {
    hosts: BTreeMap<String, PluginHost>,
    chains: BTreeMap<String, PluginChain>,
    grants: BTreeMap<String, GrantTable>,
}

pub(crate) fn profile_refused() -> Value {
    Refusal::from(HostPolicyDenied).response()
}

pub(crate) fn plugins_do(handles: &mut Handles, request: Value) -> Value {
    let Ok(op) = serde_json::from_value(request) else {
        return Refusal::from(MalformedRequest).response();
    };
    let result = match op {
        PluginOp::Registry => Ok(json!({
            "errorCodes": error_registry(),
            "capabilities": Capability::ALL.map(|cap| json!({ "id": cap.id(), "name": cap.name() })),
        })),
        PluginOp::Host { handle, op } => host_do(&mut handles.hosts, handle, op),
        PluginOp::Chain { handle, op } => chain_do(&mut handles.chains, handle, op),
        PluginOp::Grants { handle, op } => {
            grants_do(&mut handles.grants, &handles.chains, handle, op)
        }
    };
    result.unwrap_or_else(Refusal::response)
}

fn vacant<T>(handles: &BTreeMap<String, T>, handle: &str) -> Result<(), PluginError> {
    if handles.contains_key(handle) {
        return Err(HostPolicyDenied);
    }
    if handles.len() >= MAX_HANDLES {
        return Err(ResourceBudgetExceeded);
    }
    Ok(())
}

fn host_do(hosts: &mut BTreeMap<String, PluginHost>, handle: String, op: HostOp) -> Reply {
    if let HostOp::Open {
        site_isolation,
        budgets,
    } = op
    {
        vacant(hosts, &handle)?;
        hosts.insert(
            handle,
            PluginHost::new(HostConfig {
                site_isolation,
                budgets: Budgets {
                    max_memory_bytes: budgets.max_memory_bytes.get()?,
                    max_fuel: budgets.max_fuel.get()?,
                },
            }),
        );
        return Ok(done());
    }
    if matches!(op, HostOp::Close) {
        return Ok(json!({ "done": hosts.remove(&handle).is_some() }));
    }
    let host = hosts.get_mut(&handle).ok_or(HostPolicyDenied)?;
    match op {
        HostOp::Open { .. } | HostOp::Close => unreachable!(),
        HostOp::Instantiate { imports, grants } => {
            let imports: Vec<_> = imports.iter().map(String::as_str).collect();
            let grants = grants
                .iter()
                .map(CapabilityIn::get)
                .collect::<Result<_, _>>()?;
            host.instantiate(&imports, &grants)?;
        }
        HostOp::BeginInvocation => host.begin_invocation()?,
        HostOp::ReadContent { cap } => host.read_content(cap.get()?)?,
        HostOp::WritePrivate => host.write_private()?,
        HostOp::Egress { cap, facts } => host.egress(cap.get()?, &facts.into())?,
        HostOp::SendMessage {
            user_confirmed_exact_content,
        } => {
            host.send_message(user_confirmed_exact_content)?;
        }
        HostOp::UpdateCapability {
            cap,
            fresh_release_grant,
        } => host.update(cap.get()?, fresh_release_grant)?,
        HostOp::Update { surface, nodes } => {
            if host.is_terminated() {
                return Err(HostPolicyDenied.into());
            }
            if nodes.as_array().is_none_or(|a| a.len() > UI_MAX_NODES) {
                return Err(InvalidUiTree.into());
            }
            let nodes: Vec<NodeIn> = serde_json::from_value(nodes).map_err(|_| InvalidUiTree)?;
            let nodes = nodes
                .into_iter()
                .map(NodeIn::get)
                .collect::<Result<Vec<_>, _>>()?;
            validate_ui_tree(&surface, &nodes)?;
        }
        HostOp::Account { memory_bytes, fuel } => host.account(memory_bytes.get()?, fuel.get()?)?,
        HostOp::ReachHostState => host.reach_host_state()?,
        HostOp::MiniAppMessage {
            origin_matches,
            channel_matches,
            imitates_host_prompt,
        } => {
            host.mini_app_message(origin_matches, channel_matches, imitates_host_prompt)?;
        }
        HostOp::Terminate => host.terminate(),
        HostOp::IsTainted => return Ok(json!({ "tainted": host.is_tainted() })),
        HostOp::IsTerminated => return Ok(json!({ "terminated": host.is_terminated() })),
    }
    Ok(done())
}

fn state(body: &str) -> Result<PluginState, PluginError> {
    PluginState::from_body(&bytes(body, MAX_CONTROL_BYTES)?).map_err(|_| MalformedRequest)
}

fn grant(body: &str) -> Result<Grant, PluginError> {
    Grant::from_body(&bytes(body, MAX_GRANT_BYTES)?).map_err(|_| MalformedRequest)
}

fn approve(chain: &mut PluginChain, body: &str, approvers: &[String]) -> Reply {
    let approvers = approvers
        .iter()
        .map(|id| bytes(id, 32))
        .collect::<Result<Vec<_>, _>>()?;
    let outcome = chain
        .accept(state(body)?, &approvers)
        .map_err(|e| Refusal::with_reason(ReleaseNotAuthorized, e))?;
    Ok(json!({ "outcome": match outcome {
        ControlOutcome::Accepted => "accepted",
        ControlOutcome::Duplicate => "duplicate",
        ControlOutcome::Forked => "forked",
    } }))
}

fn chain_do(chains: &mut BTreeMap<String, PluginChain>, handle: String, op: ChainOp) -> Reply {
    match op {
        ChainOp::Genesis {
            plugin_id,
            anchor,
            body,
            approvers,
        } => {
            vacant(chains, &handle)?;
            let mut chain = PluginChain::new(fixed(&plugin_id)?, fixed(&anchor)?);
            let result = approve(&mut chain, &body, &approvers)?;
            chains.insert(handle, chain);
            return Ok(result);
        }
        ChainOp::Resume {
            anchor,
            verified_head,
        } => {
            vacant(chains, &handle)?;
            let chain = PluginChain::from_verified_head(state(&verified_head)?, fixed(&anchor)?);
            chains.insert(handle, chain);
            return Ok(done());
        }
        ChainOp::Close => return Ok(json!({ "done": chains.remove(&handle).is_some() })),
        _ => {}
    }
    let chain = chains.get_mut(&handle).ok_or(HostPolicyDenied)?;
    match op {
        ChainOp::Genesis { .. } | ChainOp::Resume { .. } | ChainOp::Close => unreachable!(),
        ChainOp::Approve { body, approvers } => approve(chain, &body, &approvers),
        ChainOp::UpdateDecision {
            seq,
            accepted_seq,
            pinned,
        } => {
            let seq = seq.get()?;
            let head = chain.head().ok_or(ReleaseNotAuthorized)?;
            let carry = update_decision(
                head,
                seq,
                accepted_seq.get()?,
                pinned.map(Uint::get).transpose()?,
                chain.release_revoked(seq),
            )
            .map_err(|e| Refusal::with_reason(ReleaseNotAuthorized, e))?;
            Ok(
                json!({ "ceiling": carry.ceiling, "disabledUntilGranted": carry.disabled_until_granted }),
            )
        }
        ChainOp::MatchesRelease { body } => {
            let manifest = Manifest::from_body(&bytes(&body, MAX_CONTROL_BYTES)?)
                .map_err(|_| MalformedRequest)?;
            if chain
                .head()
                .is_some_and(|head| manifest.matches_release(head))
                && !chain.release_revoked(manifest.seq)
            {
                Ok(done())
            } else {
                Err(ReleaseNotAuthorized.into())
            }
        }
        ChainOp::View => Ok(json!({
            "frozen": chain.is_frozen(),
            "head": chain.head().map(|head| json!({
                "version": head.version.to_string(),
                "hash": hex(&head.hash),
            })),
        })),
        ChainOp::ReleaseRevoked { seq } => {
            Ok(json!({ "revoked": chain.release_revoked(seq.get()?) }))
        }
    }
}

fn grants_do(
    tables: &mut BTreeMap<String, GrantTable>,
    chains: &BTreeMap<String, PluginChain>,
    handle: String,
    op: GrantOp,
) -> Reply {
    match op {
        GrantOp::Open => {
            vacant(tables, &handle)?;
            tables.insert(handle, GrantTable::new());
            return Ok(done());
        }
        GrantOp::Resume { verified_heads } => {
            vacant(tables, &handle)?;
            let mut table = GrantTable::new();
            let mut incarnations = BTreeSet::new();
            for body in verified_heads {
                let head = grant(&body)?;
                if !incarnations.insert(head.incarnation.clone()) {
                    return Err(MalformedRequest.into());
                }
                table.resume(head);
            }
            tables.insert(handle, table);
            return Ok(done());
        }
        GrantOp::Close => return Ok(json!({ "done": tables.remove(&handle).is_some() })),
        _ => {}
    }
    let table = tables.get_mut(&handle).ok_or(HostPolicyDenied)?;
    match op {
        GrantOp::Open | GrantOp::Resume { .. } | GrantOp::Close => unreachable!(),
        GrantOp::Grant {
            body,
            issuer_authorized,
        } => {
            table
                .accept(grant(&body)?, issuer_authorized)
                .map_err(|e| {
                    let error = match e {
                        GrantError::IssuerNotAuthorized => HostPolicyDenied,
                        GrantError::BadWindow => MalformedRequest,
                        _ => CapabilityNotGranted,
                    };
                    Refusal::with_reason(error, e)
                })?;
            Ok(done())
        }
        GrantOp::Live { incarnation, now } => {
            Ok(json!({ "live": table.live(&fixed::<16>(&incarnation)?, now.get()?) }))
        }
        GrantOp::CapabilityEnabled {
            chain,
            seq,
            cap,
            scope: (kind, id),
            now,
        } => {
            let chain = chains.get(&chain).ok_or(HostPolicyDenied)?;
            let seq = seq.get()?;
            let cap = cap.get()?.id();
            let scope = (kind.get()?, bytes(&id, 32)?);
            let now = now.get()?;
            let enabled = !chain.release_revoked(seq)
                && chain
                    .head()
                    .is_some_and(|head| capability_enabled(table, head, seq, cap, &scope, now));
            Ok(json!({ "enabled": enabled }))
        }
    }
}
