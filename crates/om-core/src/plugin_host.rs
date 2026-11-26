// SPDX-License-Identifier: MPL-2.0
//! Plugin host core (`plugins` slice 3, sans-IO). Pure policy for the Component-Model plugin host of ADR-0259 and
//! ADR-0266: it decides, from facts the host supplies, whether a plugin step is allowed. It runs no plugin code, opens
//! no channel and performs no I/O; the runtime that owns the sandbox (a native process or a PWA frame on a separate
//! site) calls it and enforces its answers. Sources: PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md (capabilities, plaintext
//! taint, host-owned UI, mini-app isolation, update ceilings, resource isolation), ADR-0129, ADR-0124 rules 1 and 2,
//! ADR-0266 rules 4, 6 to 9 and 11, and the `plugin_capability_id` and `plugin_error_code` registries.
//!
//! Rules enforced here:
//! * Imports at instantiation are the whole authority: an import that is not one of the 17 registered capabilities is
//!   refused ([`PluginError::UnknownCapability`], so no ambient WASI), and a registered one needs a grant
//!   ([`PluginError::CapabilityNotGranted`]). Where site isolation is unavailable a plugin with any `content.*` import
//!   is refused outright; there is no same-site fallback.
//! * A `content.*` read taints the invocation. Writing to `storage.plugin_private` while tainted taints the stored data,
//!   and the next invocation that reads it starts tainted, so private storage cannot launder plaintext.
//! * A tainted invocation has no egress ([`Capability::NetFetchOriginScoped`], [`Capability::ServiceInvoke`]) unless the
//!   conversation policy allows it (a Room's policy; a 1:1 needs both participants), or a one-shot, user-confirmed
//!   selected-context disclosure applies. Continuous egress is export and needs the separate export authorization,
//!   and in a 1:1 also both participants' consent.
//! * `conversation.send_message` needs a host-confirmed prompt over the exact content for every send.
//! * An update may add capabilities only as disabled; they need a fresh grant for the new release.
//! * Memory and fuel above the host's configured budgets end the plugin; the host terminates it without touching OM state.
//! * UI trees are bounded and flat, use only the six declarative node kinds, and may not name a surface or action in the
//!   host's reserved `om.` namespace, so a plugin can neither cover reserved security chrome nor stand in for a host prompt.
//! * A mini-app message is accepted only on the exact origin and channel the host bound; a message that imitates a host
//!   prompt is refused.
//!
//! Constructions under delegation, not audited text: the reserved `om.` namespace for UI surface and action ids, and the
//! host-configured budgets (no default exists here). Both are recorded for an ADR.

use std::collections::BTreeSet;

/// The 17 registered plugin capabilities (`plugin_capability_id`, ADR-0266).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    UiComposerAction = 1,
    UiMessageAction = 2,
    UiPanel = 3,
    UiSettingsPage = 4,
    ContentReadSelected = 5,
    ContentReadCurrentConversation = 6,
    ContentSearchLocalScope = 7,
    ContentReadAttachmentSelected = 8,
    ComposeInsert = 9,
    ComposeCreateDraft = 10,
    ConversationSendMessage = 11,
    ConversationSetReaction = 12,
    ConversationOpen = 13,
    NetFetchOriginScoped = 14,
    StoragePluginPrivate = 15,
    NotificationsPluginLocal = 16,
    ServiceInvoke = 17,
}

impl Capability {
    pub const ALL: [Capability; 17] = [
        Self::UiComposerAction,
        Self::UiMessageAction,
        Self::UiPanel,
        Self::UiSettingsPage,
        Self::ContentReadSelected,
        Self::ContentReadCurrentConversation,
        Self::ContentSearchLocalScope,
        Self::ContentReadAttachmentSelected,
        Self::ComposeInsert,
        Self::ComposeCreateDraft,
        Self::ConversationSendMessage,
        Self::ConversationSetReaction,
        Self::ConversationOpen,
        Self::NetFetchOriginScoped,
        Self::StoragePluginPrivate,
        Self::NotificationsPluginLocal,
        Self::ServiceInvoke,
    ];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::UiComposerAction => "ui.composer_action",
            Self::UiMessageAction => "ui.message_action",
            Self::UiPanel => "ui.panel",
            Self::UiSettingsPage => "ui.settings_page",
            Self::ContentReadSelected => "content.read_selected",
            Self::ContentReadCurrentConversation => "content.read_current_conversation",
            Self::ContentSearchLocalScope => "content.search_local_scope",
            Self::ContentReadAttachmentSelected => "content.read_attachment_selected",
            Self::ComposeInsert => "compose.insert",
            Self::ComposeCreateDraft => "compose.create_draft",
            Self::ConversationSendMessage => "conversation.send_message",
            Self::ConversationSetReaction => "conversation.set_reaction",
            Self::ConversationOpen => "conversation.open",
            Self::NetFetchOriginScoped => "net.fetch_origin_scoped",
            Self::StoragePluginPrivate => "storage.plugin_private",
            Self::NotificationsPluginLocal => "notifications.plugin_local",
            Self::ServiceInvoke => "service.invoke",
        }
    }

    /// Parses a registered capability name. Anything else (including every WASI interface) is not a capability.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Maps a Component Model import name of the `om:plugin` world (for example `om:plugin/ui-panel@0.1.0`) to its
    /// capability. Any other import, including every WASI interface, is not a capability.
    pub fn from_wit_import(import: &str) -> Option<Self> {
        let rest = import.strip_prefix("om:plugin/")?;
        let iface = rest.strip_suffix("@0.1.0")?;
        let (family, tail) = iface.split_once('-')?;
        Self::parse(&format!("{family}.{}", tail.replace('-', "_")))
    }

    /// Capabilities that read conversation plaintext and so taint the invocation.
    pub fn is_content(self) -> bool {
        matches!(
            self,
            Self::ContentReadSelected
                | Self::ContentReadCurrentConversation
                | Self::ContentSearchLocalScope
                | Self::ContentReadAttachmentSelected
        )
    }

    /// Outbound channels: the taint rule applies to them.
    pub fn is_egress(self) -> bool {
        matches!(self, Self::NetFetchOriginScoped | Self::ServiceInvoke)
    }
}

/// Refusal classes, named and numbered as the `plugin_error_code` registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginError {
    MalformedRequest = 1,
    UnknownCapability = 2,
    CapabilityNotGranted = 3,
    GrantExpired = 4,
    TaintedEgressDenied = 5,
    UserConfirmationRequired = 6,
    BilateralConsentRequired = 7,
    OriginChannelMismatch = 8,
    ResourceBudgetExceeded = 9,
    HostPolicyDenied = 10,
    ReleaseNotAuthorized = 11,
    InvalidUiTree = 12,
}

impl PluginError {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// Resource ceilings the embedding host configures. There is deliberately no `Default`: no budget is invented here.
#[derive(Clone, Copy, Debug)]
pub struct Budgets {
    pub max_memory_bytes: u64,
    pub max_fuel: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct HostConfig {
    /// True only where the platform gives the plugin its own process or site (PWA: a different registrable domain).
    pub site_isolation: bool,
    pub budgets: Budgets,
}

/// Host-authored facts about one outbound use. The plugin never supplies these (ADR-0266 rule 6 and 9).
#[derive(Clone, Copy, Debug)]
pub struct EgressFacts {
    /// The conversation's effective `plaintext_egress` policy allows it (a Room's policy, or the intersection of both
    /// 1:1 participants' current policies).
    pub policy_allows: bool,
    pub one_to_one: bool,
    /// Both participants' current signed consents were verified (1:1 only).
    pub both_consented: bool,
    /// The user chose the content, saw the destination and confirmed this one action.
    pub one_shot_confirmed: bool,
    pub continuous: bool,
    /// The separate Bridge section 7 export authorization was verified.
    pub export_authorized: bool,
}

/// Registered declarative node kinds (`plugin_ui_node_kind`).
pub const UI_NODE_KINDS: std::ops::RangeInclusive<u8> = 1..=6;
pub const UI_MAX_NODES: usize = 128;
pub const UI_MAX_DEPTH: usize = 8;
pub const UI_MAX_TEXT_BYTES: usize = 1024;
pub const UI_MAX_ACTION_BYTES: usize = 128;
/// Names under this prefix belong to the host and are never plugin-owned.
pub const RESERVED_PREFIX: &str = "om.";

#[derive(Clone, Debug)]
pub struct UiNode {
    pub id: [u8; 16],
    pub kind: u8,
    pub parent: Option<[u8; 16]>,
    pub text: Option<String>,
    pub action: Option<String>,
}

/// Validates a decoded `PluginUiTreeUpdate` body.
pub fn validate_ui_tree(surface: &str, nodes: &[UiNode]) -> Result<(), PluginError> {
    use PluginError::InvalidUiTree as Bad;
    if surface.is_empty() || surface.len() > 128 || surface.starts_with(RESERVED_PREFIX) {
        return Err(Bad);
    }
    if nodes.is_empty() || nodes.len() > UI_MAX_NODES {
        return Err(Bad);
    }
    let mut ids = BTreeSet::new();
    for n in nodes {
        if !UI_NODE_KINDS.contains(&n.kind) || !ids.insert(n.id) {
            return Err(Bad);
        }
        if n.text.as_ref().is_some_and(|t| t.len() > UI_MAX_TEXT_BYTES) {
            return Err(Bad);
        }
        if let Some(a) = &n.action
            && (a.is_empty() || a.len() > UI_MAX_ACTION_BYTES || a.starts_with(RESERVED_PREFIX))
        {
            return Err(Bad);
        }
    }
    if nodes.iter().filter(|n| n.parent.is_none()).count() != 1 {
        return Err(Bad);
    }
    for n in nodes {
        // Every parent exists, and following parents reaches the root within the depth limit (no cycle).
        let mut depth = 1;
        let mut cur = n;
        while let Some(p) = cur.parent {
            cur = nodes.iter().find(|m| m.id == p).ok_or(Bad)?;
            depth += 1;
            if depth > UI_MAX_DEPTH {
                return Err(Bad);
            }
        }
    }
    Ok(())
}

/// One installed plugin's host-side state. Create a fresh value per installation; call [`PluginHost::terminate`] when
/// the plugin crashes or is stopped.
#[derive(Debug)]
pub struct PluginHost {
    cfg: HostConfig,
    granted: BTreeSet<Capability>,
    /// The non-expanding carry-forward set of the current release.
    ceiling: BTreeSet<Capability>,
    tainted: bool,
    private_tainted: bool,
    terminated: bool,
}

impl PluginHost {
    pub fn new(cfg: HostConfig) -> Self {
        Self {
            cfg,
            granted: BTreeSet::new(),
            ceiling: BTreeSet::new(),
            tainted: false,
            private_tainted: false,
            terminated: false,
        }
    }

    pub fn is_tainted(&self) -> bool {
        self.tainted
    }

    pub fn is_terminated(&self) -> bool {
        self.terminated
    }

    /// Ends the plugin. Nothing it held reaches canonical OM state; later calls are refused.
    pub fn terminate(&mut self) {
        self.terminated = true;
        self.granted.clear();
        self.tainted = false;
    }

    fn live(&self) -> Result<(), PluginError> {
        if self.terminated {
            Err(PluginError::HostPolicyDenied)
        } else {
            Ok(())
        }
    }

    /// Instantiation: the component's imports against the grants the user gave this installation.
    pub fn instantiate(
        &mut self,
        imports: &[&str],
        grants: &BTreeSet<Capability>,
    ) -> Result<(), PluginError> {
        self.live()?;
        let mut caps = Vec::with_capacity(imports.len());
        for name in imports {
            caps.push(Capability::parse(name).ok_or(PluginError::UnknownCapability)?);
        }
        if !self.cfg.site_isolation && caps.iter().any(|c| c.is_content()) {
            return Err(PluginError::HostPolicyDenied);
        }
        if caps.iter().any(|c| !grants.contains(c)) {
            return Err(PluginError::CapabilityNotGranted);
        }
        self.granted = caps.into_iter().collect();
        self.ceiling.extend(self.granted.iter().copied());
        Ok(())
    }

    /// A new invocation starts. It is tainted if the plugin's private storage holds tainted data.
    pub fn begin_invocation(&mut self) -> Result<(), PluginError> {
        self.live()?;
        self.tainted = self.private_tainted;
        Ok(())
    }

    /// The plugin reads conversation plaintext through `cap`. Taint never clears within an invocation.
    pub fn read_content(&mut self, cap: Capability) -> Result<(), PluginError> {
        self.live()?;
        if !cap.is_content() {
            return Err(PluginError::HostPolicyDenied);
        }
        if !self.granted.contains(&cap) {
            return Err(PluginError::CapabilityNotGranted);
        }
        self.tainted = true;
        Ok(())
    }

    /// The plugin stores data in its private storage. Data written while tainted stays tainted. The import gate at
    /// instantiation already decides whether the plugin can reach this call at all.
    pub fn write_private(&mut self) -> Result<(), PluginError> {
        self.live()?;
        if self.tainted {
            self.private_tainted = true;
        }
        Ok(())
    }

    /// One outbound use through `cap`.
    pub fn egress(&mut self, cap: Capability, facts: &EgressFacts) -> Result<(), PluginError> {
        self.live()?;
        if !cap.is_egress() {
            return Err(PluginError::HostPolicyDenied);
        }
        if self.tainted {
            if facts.continuous {
                // Continuous plaintext egress is export: the plugin grant and the conversation policy never authorize it.
                if !facts.export_authorized {
                    return Err(PluginError::TaintedEgressDenied);
                }
                if facts.one_to_one && !facts.both_consented {
                    return Err(PluginError::BilateralConsentRequired);
                }
            } else if !facts.one_shot_confirmed {
                if !facts.policy_allows {
                    return Err(PluginError::TaintedEgressDenied);
                }
                if facts.one_to_one && !facts.both_consented {
                    return Err(PluginError::BilateralConsentRequired);
                }
            }
        }
        if !self.granted.contains(&cap) {
            return Err(PluginError::CapabilityNotGranted);
        }
        Ok(())
    }

    /// `conversation.send_message`: the host showed the exact content and destination and the user confirmed this send.
    pub fn send_message(&mut self, user_confirmed_exact_content: bool) -> Result<(), PluginError> {
        self.live()?;
        if !self.granted.contains(&Capability::ConversationSendMessage) {
            return Err(PluginError::CapabilityNotGranted);
        }
        if !user_confirmed_exact_content {
            return Err(PluginError::UserConfirmationRequired);
        }
        Ok(())
    }

    /// An update asks for `cap`. Inside the continuity ceiling it carries forward; beyond it, only a fresh grant for the
    /// new release enables it. A manifest never enables a capability on its own.
    pub fn update(
        &mut self,
        cap: Capability,
        fresh_release_grant: bool,
    ) -> Result<(), PluginError> {
        self.live()?;
        if self.ceiling.contains(&cap) {
            return Ok(());
        }
        if !fresh_release_grant {
            return Err(PluginError::ReleaseNotAuthorized);
        }
        self.ceiling.insert(cap);
        Ok(())
    }

    /// The runtime reports the plugin's memory and fuel use. Beyond the configured budget the plugin is terminated.
    pub fn account(&mut self, memory_bytes: u64, fuel: u64) -> Result<(), PluginError> {
        self.live()?;
        let b = self.cfg.budgets;
        if memory_bytes > b.max_memory_bytes || fuel > b.max_fuel {
            self.terminate();
            return Err(PluginError::ResourceBudgetExceeded);
        }
        Ok(())
    }

    /// A plugin tries to reach host memory, keys, the vault or the filesystem by any path: there is no such capability.
    pub fn reach_host_state(&self) -> Result<(), PluginError> {
        Err(PluginError::HostPolicyDenied)
    }

    /// A message from a mini-app or plugin frame. `origin_matches` and `channel_matches` are the host's own comparison
    /// with the origin and `MessageChannel` it bound at creation; `imitates_host_prompt` is the host's verdict that the
    /// content imitates a reserved prompt.
    pub fn mini_app_message(
        &self,
        origin_matches: bool,
        channel_matches: bool,
        imitates_host_prompt: bool,
    ) -> Result<(), PluginError> {
        self.live()?;
        if !origin_matches || !channel_matches {
            return Err(PluginError::OriginChannelMismatch);
        }
        if imitates_host_prompt {
            return Err(PluginError::InvalidUiTree);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(isolated: bool) -> HostConfig {
        HostConfig {
            site_isolation: isolated,
            budgets: Budgets {
                max_memory_bytes: 8 << 20,
                max_fuel: 1_000_000,
            },
        }
    }

    fn grants(caps: &[Capability]) -> BTreeSet<Capability> {
        caps.iter().copied().collect()
    }

    fn node(id: u8, kind: u8, parent: Option<u8>) -> UiNode {
        UiNode {
            id: [id; 16],
            kind,
            parent: parent.map(|p| [p; 16]),
            text: None,
            action: None,
        }
    }

    #[test]
    fn registry_ids_and_names_round_trip() {
        for (i, c) in Capability::ALL.iter().enumerate() {
            assert_eq!(c.id() as usize, i + 1);
            assert_eq!(Capability::parse(c.name()), Some(*c));
        }
        assert_eq!(Capability::parse("wasi:filesystem/types"), None);
        // every capability is reachable through exactly its WIT interface name, and nothing else is
        for c in Capability::ALL {
            let wit = format!(
                "om:plugin/{}@0.1.0",
                c.name().replacen('.', "-", 1).replace('_', "-")
            );
            assert_eq!(Capability::from_wit_import(&wit), Some(c), "{wit}");
        }
        for bad in [
            "wasi:filesystem/types@0.2.0",
            "om:plugin/ui-panel@0.2.0",
            "om:plugin/unknown-thing@0.1.0",
            "om:plugin/ui-panel",
            "other:plugin/ui-panel@0.1.0",
        ] {
            assert_eq!(Capability::from_wit_import(bad), None, "{bad}");
        }
        assert_eq!(PluginError::InvalidUiTree.code(), 12);
        assert_eq!(PluginError::UnknownCapability.code(), 2);
    }

    #[test]
    fn content_plugins_are_refused_where_isolation_is_unavailable() {
        let g = grants(&[Capability::ContentReadSelected, Capability::UiPanel]);
        let mut h = PluginHost::new(cfg(false));
        assert_eq!(
            h.instantiate(&["content.read_selected"], &g),
            Err(PluginError::HostPolicyDenied)
        );
        // a plugin with no content capability still runs without isolation
        assert_eq!(h.instantiate(&["ui.panel"], &g), Ok(()));
        let mut iso = PluginHost::new(cfg(true));
        assert_eq!(iso.instantiate(&["content.read_selected"], &g), Ok(()));
    }

    #[test]
    fn taint_survives_private_storage_into_the_next_invocation() {
        let g = grants(&[
            Capability::ContentReadSelected,
            Capability::NetFetchOriginScoped,
            Capability::StoragePluginPrivate,
        ]);
        let mut h = PluginHost::new(cfg(true));
        h.instantiate(
            &[
                "content.read_selected",
                "net.fetch_origin_scoped",
                "storage.plugin_private",
            ],
            &g,
        )
        .unwrap();
        let none = EgressFacts {
            policy_allows: false,
            one_to_one: false,
            both_consented: false,
            one_shot_confirmed: false,
            continuous: false,
            export_authorized: false,
        };
        h.begin_invocation().unwrap();
        assert!(!h.is_tainted());
        h.read_content(Capability::ContentReadSelected).unwrap();
        h.write_private().unwrap();
        h.begin_invocation().unwrap();
        assert!(h.is_tainted(), "the next invocation inherits the taint");
        assert_eq!(
            h.egress(Capability::NetFetchOriginScoped, &none),
            Err(PluginError::TaintedEgressDenied)
        );
    }

    #[test]
    fn a_room_policy_or_a_one_shot_confirmation_allows_tainted_egress_but_a_one_to_one_needs_both()
    {
        let g = grants(&[
            Capability::ContentReadSelected,
            Capability::NetFetchOriginScoped,
        ]);
        let mut h = PluginHost::new(cfg(true));
        h.instantiate(&["content.read_selected", "net.fetch_origin_scoped"], &g)
            .unwrap();
        h.read_content(Capability::ContentReadSelected).unwrap();
        let mut f = EgressFacts {
            policy_allows: true,
            one_to_one: false,
            both_consented: false,
            one_shot_confirmed: false,
            continuous: false,
            export_authorized: false,
        };
        assert_eq!(h.egress(Capability::NetFetchOriginScoped, &f), Ok(()));
        f.one_to_one = true;
        assert_eq!(
            h.egress(Capability::NetFetchOriginScoped, &f),
            Err(PluginError::BilateralConsentRequired)
        );
        f.both_consented = true;
        assert_eq!(h.egress(Capability::NetFetchOriginScoped, &f), Ok(()));
        // continuous is export: the Room policy is not enough, and a 1:1 export still needs both consents
        f.continuous = true;
        f.export_authorized = false;
        assert_eq!(
            h.egress(Capability::NetFetchOriginScoped, &f),
            Err(PluginError::TaintedEgressDenied)
        );
        f.export_authorized = true;
        f.both_consented = false;
        assert_eq!(
            h.egress(Capability::NetFetchOriginScoped, &f),
            Err(PluginError::BilateralConsentRequired)
        );
        // a one-shot confirmed disclosure without any policy is allowed (manual sharing)
        let manual = EgressFacts {
            policy_allows: false,
            one_to_one: true,
            both_consented: false,
            one_shot_confirmed: true,
            continuous: false,
            export_authorized: false,
        };
        assert_eq!(h.egress(Capability::NetFetchOriginScoped, &manual), Ok(()));
    }

    #[test]
    fn exceeding_a_budget_terminates_the_plugin_and_later_calls_are_refused() {
        let mut h = PluginHost::new(cfg(true));
        assert_eq!(h.account(1 << 20, 1_000), Ok(()));
        assert_eq!(
            h.account(u64::MAX, 1),
            Err(PluginError::ResourceBudgetExceeded)
        );
        assert!(h.is_terminated());
        assert_eq!(h.begin_invocation(), Err(PluginError::HostPolicyDenied));
        assert_eq!(h.send_message(true), Err(PluginError::HostPolicyDenied));
    }

    #[test]
    fn updates_enable_new_capabilities_only_with_a_fresh_grant() {
        let g = grants(&[Capability::UiPanel]);
        let mut h = PluginHost::new(cfg(true));
        h.instantiate(&["ui.panel"], &g).unwrap();
        assert_eq!(h.update(Capability::UiPanel, false), Ok(()));
        assert_eq!(
            h.update(Capability::NetFetchOriginScoped, false),
            Err(PluginError::ReleaseNotAuthorized)
        );
        assert_eq!(h.update(Capability::NetFetchOriginScoped, true), Ok(()));
    }

    #[test]
    fn ui_trees_are_bounded_flat_declarative_and_leave_the_reserved_namespace_alone() {
        let ok = vec![node(1, 4, None), node(2, 1, Some(1)), node(3, 2, Some(1))];
        assert_eq!(validate_ui_tree("panel", &ok), Ok(()));
        // reserved surface and action names
        assert_eq!(
            validate_ui_tree("om.security.verification", &ok),
            Err(PluginError::InvalidUiTree)
        );
        let mut spoof = ok.clone();
        spoof[2].action = Some("om.permission.allow".into());
        assert_eq!(
            validate_ui_tree("panel", &spoof),
            Err(PluginError::InvalidUiTree)
        );
        // unknown node kind (no raw DOM), duplicate id, two roots, missing parent, cycle, too deep, too many
        for bad in [
            vec![node(1, 7, None)],
            vec![node(1, 4, None), node(1, 1, Some(1))],
            vec![node(1, 4, None), node(2, 4, None)],
            vec![node(1, 4, None), node(2, 1, Some(9))],
            vec![node(1, 4, None), node(2, 1, Some(3)), node(3, 1, Some(2))],
            (1..=9u8)
                .map(|i| node(i, 4, if i == 1 { None } else { Some(i - 1) }))
                .collect::<Vec<_>>(),
        ] {
            assert_eq!(
                validate_ui_tree("panel", &bad),
                Err(PluginError::InvalidUiTree)
            );
        }
        let many: Vec<UiNode> = (0..129u16)
            .map(|i| UiNode {
                id: {
                    let mut id = [0u8; 16];
                    id[..2].copy_from_slice(&i.to_be_bytes());
                    id
                },
                kind: 1,
                parent: if i == 0 { None } else { Some([0u8; 16]) },
                text: None,
                action: None,
            })
            .collect();
        assert_eq!(
            validate_ui_tree("panel", &many),
            Err(PluginError::InvalidUiTree)
        );
    }
}
