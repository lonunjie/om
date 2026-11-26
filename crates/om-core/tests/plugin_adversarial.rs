// SPDX-License-Identifier: MPL-2.0
//! `plugins` slice 2: the adversarial plugin suite (tests only). Every hostile case is a short script of host-visible
//! steps that a conforming plugin host must refuse at a named step with a named error class (the `plugin_error_code`
//! registry of ADR-0266), and every benign control must be allowed, so a deny-everything host fails too. The suite is
//! generic over [`PluginHost`]; slice 3 runs its sans-IO host through [`run_suite`]. Today the only hosts are the two
//! deliberately wrong stubs below, and the meta-tests prove the suite detects each of them: the permissive stub fails
//! every hostile case and the deny-all stub fails every control. Slice 3: [`RealHost`] drives `om_core::plugin_host`
//! through the same suite. Sources: PLUGIN_AND_APP_EXTENSION_ARCHITECTURE.md
//! (capabilities, taint, host-owned UI, mini-app surface, resource isolation, update ceilings), ADR-0129, V-010.

// The step fields are read by the slice 3 host that runs this suite; the two stubs here ignore them on purpose.
#![allow(dead_code)]

use om_core::plugin_host as ph;
use std::collections::BTreeSet;

/// Error classes, named as in the `plugin_error_code` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Deny {
    UnknownCapability,
    CapabilityNotGranted,
    TaintedEgressDenied,
    UserConfirmationRequired,
    BilateralConsentRequired,
    OriginChannelMismatch,
    ResourceBudgetExceeded,
    HostPolicyDenied,
    ReleaseNotAuthorized,
    InvalidUiTree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Allowed,
    Denied(Deny),
}

#[derive(Clone, Debug)]
enum Step {
    /// The component's imports at instantiation, against the capabilities the user granted.
    Instantiate {
        imports: Vec<&'static str>,
        granted: Vec<&'static str>,
    },
    /// Reads conversation plaintext through a `content.*` capability (taints the invocation).
    ReadContent {
        cap: &'static str,
    },
    /// Outbound channel use; `via_storage` reads back earlier tainted private data first.
    Egress {
        cap: &'static str,
        policy_allows: bool,
        one_to_one: bool,
        both_consented: bool,
        one_shot_confirmed: bool,
        continuous: bool,
        export_authorized: bool,
    },
    WritePrivate,
    NextInvocationReadsPrivate,
    Send {
        user_confirmed_exact_content: bool,
    },
    /// An update whose manifest asks for more than the prior release's continuity ceiling.
    Update {
        extra_cap: &'static str,
        fresh_release_grant: bool,
    },
    Allocate {
        memory_bytes: u64,
        fuel: u64,
    },
    UiTree {
        covers_security_surface: bool,
        spoofs_host_prompt: bool,
        raw_dom: bool,
    },
    /// Reads host memory, keys, vault or the filesystem through any path.
    ReachHostState,
    MiniAppMessage {
        origin_matches: bool,
        channel_matches: bool,
        imitates_host_prompt: bool,
    },
}

trait PluginHost {
    fn step(&mut self, s: &Step) -> Verdict;
}

struct Case {
    id: &'static str,
    steps: Vec<Step>,
    /// `Some(class)`: the last step must be denied with it, every earlier step allowed. `None`: a control, all allowed.
    expect: Option<Deny>,
}

fn eg(cap: &'static str) -> Step {
    Step::Egress {
        cap,
        policy_allows: false,
        one_to_one: false,
        both_consented: false,
        one_shot_confirmed: false,
        continuous: false,
        export_authorized: false,
    }
}

fn inst(imports: &[&'static str], granted: &[&'static str]) -> Step {
    Step::Instantiate {
        imports: imports.to_vec(),
        granted: granted.to_vec(),
    }
}

fn cases() -> Vec<Case> {
    use Deny::*;
    let ok = inst(
        &[
            "content.read_current_conversation",
            "net.fetch_origin_scoped",
        ],
        &[
            "content.read_current_conversation",
            "net.fetch_origin_scoped",
        ],
    );
    vec![
        // --- controls: a conforming host allows these -------------------------------------------------------
        Case {
            id: "C1-granted-imports-instantiate",
            steps: vec![ok.clone()],
            expect: None,
        },
        Case {
            id: "C2-untainted-network-use",
            steps: vec![ok.clone(), eg("net.fetch_origin_scoped")],
            expect: None,
        },
        Case {
            id: "C3-confirmed-send",
            steps: vec![
                inst(
                    &["conversation.send_message"],
                    &["conversation.send_message"],
                ),
                Step::Send {
                    user_confirmed_exact_content: true,
                },
            ],
            expect: None,
        },
        Case {
            id: "C4-small-resource-use",
            steps: vec![Step::Allocate {
                memory_bytes: 1 << 20,
                fuel: 1_000,
            }],
            expect: None,
        },
        Case {
            id: "C5-declarative-ui-tree",
            steps: vec![Step::UiTree {
                covers_security_surface: false,
                spoofs_host_prompt: false,
                raw_dom: false,
            }],
            expect: None,
        },
        Case {
            id: "C6-update-within-ceiling-or-freshly-granted",
            steps: vec![Step::Update {
                extra_cap: "net.fetch_origin_scoped",
                fresh_release_grant: true,
            }],
            expect: None,
        },
        // --- hostile cases ---------------------------------------------------------------------------------
        Case {
            id: "H1-ambient-wasi-import",
            steps: vec![inst(&["wasi:filesystem/types"], &[])],
            expect: Some(UnknownCapability),
        },
        Case {
            id: "H2-import-without-grant",
            steps: vec![inst(&["conversation.send_message"], &["ui.panel"])],
            expect: Some(CapabilityNotGranted),
        },
        Case {
            id: "H3-host-memory-and-key-read",
            steps: vec![Step::ReachHostState],
            expect: Some(HostPolicyDenied),
        },
        Case {
            id: "H4-egress-after-plaintext-read",
            steps: vec![
                ok.clone(),
                Step::ReadContent {
                    cap: "content.read_current_conversation",
                },
                eg("net.fetch_origin_scoped"),
            ],
            expect: Some(TaintedEgressDenied),
        },
        Case {
            id: "H5-egress-via-service-invoke-after-read",
            steps: vec![
                ok.clone(),
                Step::ReadContent {
                    cap: "content.read_current_conversation",
                },
                eg("service.invoke"),
            ],
            expect: Some(TaintedEgressDenied),
        },
        Case {
            id: "H6-launder-through-plugin-private-storage",
            steps: vec![
                ok.clone(),
                Step::ReadContent {
                    cap: "content.read_current_conversation",
                },
                Step::WritePrivate,
                Step::NextInvocationReadsPrivate,
                eg("net.fetch_origin_scoped"),
            ],
            expect: Some(TaintedEgressDenied),
        },
        Case {
            id: "H7-one-to-one-needs-both-participants",
            steps: vec![
                ok.clone(),
                Step::ReadContent {
                    cap: "content.read_current_conversation",
                },
                Step::Egress {
                    cap: "net.fetch_origin_scoped",
                    policy_allows: true,
                    one_to_one: true,
                    both_consented: false,
                    one_shot_confirmed: false,
                    continuous: false,
                    export_authorized: false,
                },
            ],
            expect: Some(BilateralConsentRequired),
        },
        Case {
            id: "H8-continuous-egress-is-export",
            steps: vec![
                ok.clone(),
                Step::ReadContent {
                    cap: "content.read_current_conversation",
                },
                Step::Egress {
                    cap: "net.fetch_origin_scoped",
                    policy_allows: true,
                    one_to_one: false,
                    both_consented: false,
                    one_shot_confirmed: false,
                    continuous: true,
                    export_authorized: false,
                },
            ],
            expect: Some(TaintedEgressDenied),
        },
        Case {
            id: "H9-send-without-confirmation",
            steps: vec![
                inst(
                    &["conversation.send_message"],
                    &["conversation.send_message"],
                ),
                Step::Send {
                    user_confirmed_exact_content: false,
                },
            ],
            expect: Some(UserConfirmationRequired),
        },
        Case {
            id: "H10-capability-escalation-on-update",
            steps: vec![Step::Update {
                extra_cap: "net.fetch_origin_scoped",
                fresh_release_grant: false,
            }],
            expect: Some(ReleaseNotAuthorized),
        },
        Case {
            id: "H11-memory-exhaustion",
            steps: vec![Step::Allocate {
                memory_bytes: u64::MAX,
                fuel: 1_000,
            }],
            expect: Some(ResourceBudgetExceeded),
        },
        Case {
            id: "H12-fuel-exhaustion-loop",
            steps: vec![Step::Allocate {
                memory_bytes: 1 << 20,
                fuel: u64::MAX,
            }],
            expect: Some(ResourceBudgetExceeded),
        },
        Case {
            id: "H13-ui-covers-security-surface",
            steps: vec![Step::UiTree {
                covers_security_surface: true,
                spoofs_host_prompt: false,
                raw_dom: false,
            }],
            expect: Some(InvalidUiTree),
        },
        Case {
            id: "H14-ui-spoofs-host-prompt",
            steps: vec![Step::UiTree {
                covers_security_surface: false,
                spoofs_host_prompt: true,
                raw_dom: false,
            }],
            expect: Some(InvalidUiTree),
        },
        Case {
            id: "H15-raw-dom-ownership",
            steps: vec![Step::UiTree {
                covers_security_surface: false,
                spoofs_host_prompt: false,
                raw_dom: true,
            }],
            expect: Some(InvalidUiTree),
        },
        Case {
            id: "H16-mini-app-wrong-origin",
            steps: vec![Step::MiniAppMessage {
                origin_matches: false,
                channel_matches: true,
                imitates_host_prompt: false,
            }],
            expect: Some(OriginChannelMismatch),
        },
        Case {
            id: "H17-mini-app-wrong-channel",
            steps: vec![Step::MiniAppMessage {
                origin_matches: true,
                channel_matches: false,
                imitates_host_prompt: false,
            }],
            expect: Some(OriginChannelMismatch),
        },
        Case {
            id: "H18-mini-app-imitates-host-prompt",
            steps: vec![Step::MiniAppMessage {
                origin_matches: true,
                channel_matches: true,
                imitates_host_prompt: true,
            }],
            expect: Some(InvalidUiTree),
        },
    ]
}

/// Runs the whole suite against fresh hosts; returns the ids of the cases the host got wrong.
fn run_suite<H: PluginHost>(mut fresh: impl FnMut() -> H) -> Vec<&'static str> {
    let mut wrong = Vec::new();
    for c in cases() {
        let mut host = fresh();
        let mut got: Vec<Verdict> = c.steps.iter().map(|s| host.step(s)).collect();
        let last = got.pop().unwrap();
        let earlier_ok = got.iter().all(|v| *v == Verdict::Allowed);
        let pass = match c.expect {
            None => earlier_ok && last == Verdict::Allowed,
            Some(d) => earlier_ok && last == Verdict::Denied(d),
        };
        if !pass {
            wrong.push(c.id);
        }
    }
    wrong
}

/// Allows everything: a plugin host with no enforcement.
struct Permissive;
impl PluginHost for Permissive {
    fn step(&mut self, _: &Step) -> Verdict {
        Verdict::Allowed
    }
}

/// Denies everything: safe but useless.
struct DenyAll;
impl PluginHost for DenyAll {
    fn step(&mut self, _: &Step) -> Verdict {
        Verdict::Denied(Deny::HostPolicyDenied)
    }
}

#[test]
fn the_suite_has_the_required_breadth() {
    let c = cases();
    let hostile = c.iter().filter(|c| c.expect.is_some()).count();
    let controls = c.len() - hostile;
    assert!(
        hostile >= 18 && controls >= 6,
        "{hostile} hostile, {controls} controls"
    );
    let ids: BTreeSet<_> = c.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), c.len(), "duplicate case id");
    // every attack family of the plan is represented
    for family in [
        "H1-", "H3-", "H4-", "H6-", "H9-", "H10-", "H11-", "H13-", "H14-", "H16-", "H18-",
    ] {
        assert!(
            ids.iter().any(|i| i.starts_with(family)),
            "missing {family}"
        );
    }
}

#[test]
fn a_permissive_host_fails_every_hostile_case_and_passes_the_controls() {
    let wrong = run_suite(|| Permissive);
    let hostile: Vec<_> = cases()
        .into_iter()
        .filter(|c| c.expect.is_some())
        .map(|c| c.id)
        .collect();
    assert_eq!(
        wrong, hostile,
        "the suite must detect a host that enforces nothing"
    );
}

#[test]
fn a_deny_all_host_fails_every_control_so_refusing_everything_is_not_a_pass() {
    let wrong = run_suite(|| DenyAll);
    assert!(
        wrong.contains(&"C1-granted-imports-instantiate") && wrong.contains(&"C3-confirmed-send")
    );
    assert!(wrong.iter().all(|id| {
        id.starts_with('C')
            || cases()
                .iter()
                .any(|c| c.id == *id && c.expect != Some(Deny::HostPolicyDenied))
    }));
    assert!(wrong.len() >= 6, "{wrong:?}");
}

/// The slice 3 sans-IO host, driven through the suite. The step flags become the host's own facts; the budgets are test
/// values only (the host has no default).
struct RealHost {
    host: ph::PluginHost,
}

impl RealHost {
    fn new() -> Self {
        Self {
            host: ph::PluginHost::new(ph::HostConfig {
                site_isolation: true,
                budgets: ph::Budgets {
                    max_memory_bytes: 16 << 20,
                    max_fuel: 10_000_000,
                },
            }),
        }
    }
}

fn deny(e: ph::PluginError) -> Deny {
    use ph::PluginError as E;
    match e {
        E::UnknownCapability => Deny::UnknownCapability,
        E::CapabilityNotGranted => Deny::CapabilityNotGranted,
        E::TaintedEgressDenied => Deny::TaintedEgressDenied,
        E::UserConfirmationRequired => Deny::UserConfirmationRequired,
        E::BilateralConsentRequired => Deny::BilateralConsentRequired,
        E::OriginChannelMismatch => Deny::OriginChannelMismatch,
        E::ResourceBudgetExceeded => Deny::ResourceBudgetExceeded,
        E::ReleaseNotAuthorized => Deny::ReleaseNotAuthorized,
        E::InvalidUiTree => Deny::InvalidUiTree,
        E::MalformedRequest | E::GrantExpired | E::HostPolicyDenied => Deny::HostPolicyDenied,
    }
}

fn node(id: u8, kind: u8, parent: Option<u8>, action: Option<&str>) -> ph::UiNode {
    ph::UiNode {
        id: [id; 16],
        kind,
        parent: parent.map(|p| [p; 16]),
        text: Some("Label".into()),
        action: action.map(str::to_string),
    }
}

impl PluginHost for RealHost {
    fn step(&mut self, s: &Step) -> Verdict {
        let r = match s {
            Step::Instantiate { imports, granted } => {
                let g: BTreeSet<ph::Capability> = granted
                    .iter()
                    .filter_map(|n| ph::Capability::parse(n))
                    .collect();
                self.host.instantiate(imports, &g)
            }
            Step::ReadContent { cap } => match ph::Capability::parse(cap) {
                Some(c) => self.host.read_content(c),
                None => Err(ph::PluginError::UnknownCapability),
            },
            Step::Egress {
                cap,
                policy_allows,
                one_to_one,
                both_consented,
                one_shot_confirmed,
                continuous,
                export_authorized,
            } => match ph::Capability::parse(cap) {
                Some(c) => self.host.egress(
                    c,
                    &ph::EgressFacts {
                        policy_allows: *policy_allows,
                        one_to_one: *one_to_one,
                        both_consented: *both_consented,
                        one_shot_confirmed: *one_shot_confirmed,
                        continuous: *continuous,
                        export_authorized: *export_authorized,
                    },
                ),
                None => Err(ph::PluginError::UnknownCapability),
            },
            Step::WritePrivate => self.host.write_private(),
            Step::NextInvocationReadsPrivate => self.host.begin_invocation(),
            Step::Send {
                user_confirmed_exact_content,
            } => self.host.send_message(*user_confirmed_exact_content),
            Step::Update {
                extra_cap,
                fresh_release_grant,
            } => match ph::Capability::parse(extra_cap) {
                Some(c) => self.host.update(c, *fresh_release_grant),
                None => Err(ph::PluginError::UnknownCapability),
            },
            Step::Allocate { memory_bytes, fuel } => self.host.account(*memory_bytes, *fuel),
            Step::UiTree {
                covers_security_surface,
                spoofs_host_prompt,
                raw_dom,
            } => {
                let surface = if *covers_security_surface {
                    "om.security.verification"
                } else {
                    "panel"
                };
                let action = spoofs_host_prompt.then_some("om.permission.allow");
                let kind = if *raw_dom { 7 } else { 2 };
                ph::validate_ui_tree(
                    surface,
                    &[node(1, 4, None, None), node(2, kind, Some(1), action)],
                )
            }
            Step::ReachHostState => self.host.reach_host_state(),
            Step::MiniAppMessage {
                origin_matches,
                channel_matches,
                imitates_host_prompt,
            } => {
                self.host
                    .mini_app_message(*origin_matches, *channel_matches, *imitates_host_prompt)
            }
        };
        match r {
            Ok(()) => Verdict::Allowed,
            Err(e) => Verdict::Denied(deny(e)),
        }
    }
}

#[test]
fn the_slice_3_host_passes_every_control_and_refuses_every_hostile_case_with_the_named_error() {
    let wrong = run_suite(RealHost::new);
    assert!(
        wrong.is_empty(),
        "the real host got these cases wrong: {wrong:?}"
    );
}
