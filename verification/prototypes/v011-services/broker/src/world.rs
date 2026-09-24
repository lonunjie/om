//! Host-side state: installations, invocation lineage and the append-only effect log.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataAccess {
    SelectedContext,
    LocalPersonal,
    ScopedSubscription,
    FullRoomMember,
}

/// ServiceInstallation (spec/protocol/service-installation.logical.schema.json) plus broker policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installation {
    pub service_id: String,
    pub scope_rooms: Vec<String>,
    pub granted_capabilities: BTreeSet<String>,
    pub data_access_mode: DataAccess,
    #[serde(default)]
    pub network_allowlist: Vec<String>,
    /// Separately granted disclosure capability: may plaintext this invocation read leave via the network?
    #[serde(default)]
    pub plaintext_egress: bool,
    /// Automation trigger policy: react to service-originated events (default false).
    #[serde(default)]
    pub respond_to_services: bool,
    pub hop_limit: u32,
    pub max_calls_per_invocation: u32,
    pub max_sends_per_root: u32,
    /// RepresentationGrant: (represented account, rooms, allowed actions).
    #[serde(default)]
    pub representation: Option<Representation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Representation {
    pub represented_account: String,
    pub rooms: Vec<String>,
    pub allowed_actions: Vec<String>,
}

/// Invocation lineage (the "Bot-to-bot loop safety" frame).
#[derive(Clone, Debug)]
pub struct Invocation {
    pub invocation_id: u64,
    pub root_invocation_id: u64,
    pub origin_principal: String,
    pub hop_count: u32,
    pub service_id: String,
    /// ContextDisclosureBundle items: (item id, text).
    pub bundle: Vec<(String, String)>,
    pub room: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    Draft { service: String, room: Option<String>, text: String },
    Message { author_kind: String, author: String, via_account: Option<String>, room: String, text: String, root: u64, hop: u32 },
    Fetch { service: String, url: String },
    ReadContext { service: String, item: String },
    ProposedRoomAction { service: String, room: String, action: String },
    CapabilityRequest { service: String, capability: String },
}

#[derive(Default, Debug)]
pub struct World {
    /// (invocation id, effect)
    pub effects: Vec<(u64, Effect)>,
    /// every invocation started: (id, root, hop, service, bundle non-empty)
    pub invocations: Vec<(u64, u64, u32, String, bool)>,
    pub rooms: BTreeMap<String, Vec<(String, String)>>, // room -> (author, text)
    /// Simulated web: URL -> response body (may contain further injections).
    pub web: BTreeMap<String, String>,
    pub local_index: Vec<String>,
}
