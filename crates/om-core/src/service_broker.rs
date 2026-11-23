// SPDX-License-Identifier: MPL-2.0
//! Capability broker, taint and egress for services (`services_bots` slice 5;
//! SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md "Tool use and prompt-injection boundary" and "LLMs", ADR-0124 rules 1 and 2,
//! ADR-0129 rule 2). Pure policy: the host executes what the broker allows.
//!
//! * A service asks for a typed [`Tool`]. The broker runs it only when the installation was granted that tool. Text that
//!   merely looks like a tool call, or names a never-granted power, is refused ([`Refusal`]).
//! * No service ever gets keys, the vault, a shell, the filesystem, unrestricted network, account control, governance
//!   authority or the right to approve its own escalation: those have no [`Tool`] and their names are refused outright.
//! * An invocation that read plaintext is tainted. A tainted invocation has no egress unless the conversation's policy
//!   grants `plaintext_egress` (in a Room the Room policy; in a 1:1 BOTH participants).
//! * A capability added by an update waits for a human approval and never takes effect on the service's own say-so.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    CreateDraft,
    SearchSelectedLocalScope,
    FetchUrlViaScopedNetworkProvider,
    ProposeRoomAction,
}

impl Tool {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "create_draft" => Self::CreateDraft,
            "search_selected_local_scope" => Self::SearchSelectedLocalScope,
            "fetch_url_via_scoped_network_provider" => Self::FetchUrlViaScopedNetworkProvider,
            "propose_room_action" => Self::ProposeRoomAction,
            _ => return None,
        })
    }
    /// Tools that send data off the device.
    pub fn is_egress(self) -> bool {
        matches!(self, Self::FetchUrlViaScopedNetworkProvider)
    }
}

/// Powers no service receives by default; their names never map to a tool.
pub const NEVER: &[&str] = &[
    "private_key",
    "device_vault",
    "shell",
    "filesystem",
    "network_unrestricted",
    "account_control",
    "room_governance",
    "space_governance",
    "approve_own_escalation",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A power that is never granted.
    Forbidden,
    /// Not a typed tool at all (tool-looking text from a model).
    UnknownTool,
    /// A real tool the installation was not granted.
    NotGranted,
    /// Tainted invocation and the conversation policy does not allow plaintext egress.
    TaintedEgress,
}

/// `plaintext_egress` policy of the conversation the invocation read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgressPolicy {
    Room { allowed: bool },
    OneToOne { me: bool, peer: bool },
}

impl EgressPolicy {
    pub fn allows(self) -> bool {
        match self {
            Self::Room { allowed } => allowed,
            Self::OneToOne { me, peer } => me && peer,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Installation {
    granted: BTreeSet<Tool>,
    awaiting_approval: BTreeSet<Tool>,
}

impl Installation {
    pub fn new(granted: impl IntoIterator<Item = Tool>) -> Self {
        Self {
            granted: granted.into_iter().collect(),
            awaiting_approval: BTreeSet::new(),
        }
    }

    pub fn is_granted(&self, t: Tool) -> bool {
        self.granted.contains(&t)
    }

    /// An update (new version of the service) wants `wanted` tools. Dropped tools go at once; added tools wait for a
    /// human approval and are inert until then.
    pub fn update(&mut self, wanted: impl IntoIterator<Item = Tool>) {
        let wanted: BTreeSet<Tool> = wanted.into_iter().collect();
        self.granted.retain(|t| wanted.contains(t));
        self.awaiting_approval = wanted.difference(&self.granted).copied().collect();
    }

    pub fn pending(&self) -> &BTreeSet<Tool> {
        &self.awaiting_approval
    }

    /// A human approver accepts the pending growth. `by_service` is true when the approval came from a service principal
    /// or endpoint (including the requester): refused, a service never approves its own escalation.
    pub fn approve(&mut self, by_service: bool) -> Result<(), Refusal> {
        if by_service {
            return Err(Refusal::Forbidden);
        }
        let add = std::mem::take(&mut self.awaiting_approval);
        self.granted.extend(add);
        Ok(())
    }

    pub fn decline(&mut self) {
        self.awaiting_approval.clear();
    }
}

/// One running invocation and its information-flow state.
#[derive(Debug, Default)]
pub struct Flow {
    tainted: bool,
}

impl Flow {
    /// The invocation read conversation plaintext (the triggering message counts). Taint never clears.
    pub fn read_plaintext(&mut self) {
        self.tainted = true;
    }
    pub fn is_tainted(&self) -> bool {
        self.tainted
    }

    /// Mediates one tool request by name.
    pub fn request(
        &self,
        ins: &Installation,
        name: &str,
        policy: EgressPolicy,
    ) -> Result<Tool, Refusal> {
        if NEVER.contains(&name) {
            return Err(Refusal::Forbidden);
        }
        let tool = Tool::parse(name).ok_or(Refusal::UnknownTool)?;
        if !ins.is_granted(tool) {
            return Err(Refusal::NotGranted);
        }
        if tool.is_egress() && self.tainted && !policy.allows() {
            return Err(Refusal::TaintedEgress);
        }
        Ok(tool)
    }
}
