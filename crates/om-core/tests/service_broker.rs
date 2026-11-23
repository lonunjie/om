// SPDX-License-Identifier: MPL-2.0
//! `services_bots` slices 5 and 6: the injection cases cannot exceed granted capabilities, a tainted invocation has no
//! egress without the policy, growth needs a human, and two services mentioning each other stop at the hop limit.
use om_core::service_automation::*;
use om_core::service_broker::*;
use std::collections::BTreeSet;

fn ins() -> Installation {
    Installation::new([Tool::CreateDraft, Tool::FetchUrlViaScopedNetworkProvider])
}
const NO: EgressPolicy = EgressPolicy::Room { allowed: false };

#[test]
fn injected_tool_text_cannot_exceed_granted_capabilities() {
    let f = Flow::default();
    assert_eq!(f.request(&ins(), "create_draft", NO), Ok(Tool::CreateDraft));
    assert_eq!(
        f.request(&ins(), "propose_room_action", NO),
        Err(Refusal::NotGranted)
    );
    assert_eq!(
        f.request(&ins(), "search_selected_local_scope", NO),
        Err(Refusal::NotGranted)
    );
    for n in NEVER {
        assert_eq!(f.request(&ins(), n, NO), Err(Refusal::Forbidden), "{n}");
    }
    for n in [
        "rm -rf /",
        "{\"tool\":\"create_draft\"}",
        "CREATE_DRAFT",
        "",
    ] {
        assert_eq!(f.request(&ins(), n, NO), Err(Refusal::UnknownTool), "{n}");
    }
}

#[test]
fn a_tainted_invocation_has_no_egress_without_the_policy() {
    let mut f = Flow::default();
    let fetch = "fetch_url_via_scoped_network_provider";
    assert_eq!(
        f.request(&ins(), fetch, NO),
        Ok(Tool::FetchUrlViaScopedNetworkProvider),
        "untainted may fetch if granted"
    );
    f.read_plaintext();
    assert!(f.is_tainted());
    assert_eq!(f.request(&ins(), fetch, NO), Err(Refusal::TaintedEgress));
    assert_eq!(
        f.request(&ins(), "create_draft", NO),
        Ok(Tool::CreateDraft),
        "non-egress tools still work"
    );
    assert!(
        f.request(&ins(), fetch, EgressPolicy::Room { allowed: true })
            .is_ok()
    );
    // in a 1:1 both participants must allow it
    assert_eq!(
        f.request(
            &ins(),
            fetch,
            EgressPolicy::OneToOne {
                me: true,
                peer: false
            }
        ),
        Err(Refusal::TaintedEgress)
    );
    assert!(
        f.request(
            &ins(),
            fetch,
            EgressPolicy::OneToOne {
                me: true,
                peer: true
            }
        )
        .is_ok()
    );
}

#[test]
fn capability_growth_after_an_update_needs_a_human_and_never_a_service() {
    let mut i = ins();
    i.update([Tool::CreateDraft, Tool::ProposeRoomAction]);
    assert!(
        !i.is_granted(Tool::FetchUrlViaScopedNetworkProvider),
        "dropped at once"
    );
    assert!(
        !i.is_granted(Tool::ProposeRoomAction),
        "added tools are inert until approved"
    );
    assert!(i.pending().contains(&Tool::ProposeRoomAction));
    assert_eq!(i.approve(true), Err(Refusal::Forbidden));
    assert!(!i.is_granted(Tool::ProposeRoomAction));
    i.approve(false).unwrap();
    assert!(i.is_granted(Tool::ProposeRoomAction) && i.pending().is_empty());
    i.update([Tool::CreateDraft, Tool::SearchSelectedLocalScope]);
    i.decline();
    assert!(!i.is_granted(Tool::SearchSelectedLocalScope));
}

fn classes(c: &[u8]) -> BTreeSet<u8> {
    c.iter().copied().collect()
}
fn human_event() -> Lineage {
    Lineage {
        root_invocation: [1; 16],
        parent_invocation: None,
        origin_is_service: false,
        hop_count: 0,
    }
}

#[test]
fn a_rule_cannot_read_beyond_its_installation_grant() {
    assert_eq!(
        Rule::new(classes(&[1]), classes(&[1, 2]), false, 5, 60).err(),
        Some(RuleError::ExceedsGrant)
    );
    assert_eq!(
        Rule::new(classes(&[1]), classes(&[1]), false, 0, 60).err(),
        Some(RuleError::ZeroBudget)
    );
    let mut r = Rule::new(classes(&[1, 2]), classes(&[1]), false, 5, 60).unwrap();
    assert_eq!(
        r.trigger(2, &human_event(), 0).err(),
        Some(Skip::NotInScope)
    );
    assert!(r.trigger(1, &human_event(), 0).is_ok());
    r.enabled = false;
    assert_eq!(r.trigger(1, &human_event(), 0).err(), Some(Skip::Disabled));
}

#[test]
fn service_originated_events_are_ignored_unless_the_rule_opts_in() {
    let from_service = human_event().child([2; 16]);
    let mut closed = Rule::new(classes(&[1]), classes(&[1]), false, 5, 60).unwrap();
    assert_eq!(
        closed.trigger(1, &from_service, 0).err(),
        Some(Skip::ServiceOrigin)
    );
    let mut open = Rule::new(classes(&[1]), classes(&[1]), true, 5, 60).unwrap();
    assert!(open.trigger(1, &from_service, 0).is_ok());
}

#[test]
fn two_services_that_mention_each_other_stop_at_the_hop_limit() {
    let mut a = Rule::new(classes(&[1]), classes(&[1]), true, 100, 60).unwrap();
    let mut b = Rule::new(classes(&[1]), classes(&[1]), true, 100, 60).unwrap();
    let mut l = human_event();
    let mut steps = 0;
    loop {
        let rule = if steps % 2 == 0 { &mut a } else { &mut b };
        match rule.trigger(1, &l, 0) {
            Ok(next) => {
                l = next;
                steps += 1;
            }
            Err(e) => {
                assert_eq!(e, Skip::HopLimit);
                break;
            }
        }
        assert!(steps <= 10, "unbounded loop");
    }
    assert_eq!(steps, DEFAULT_HOP_LIMIT as usize);
    assert_eq!(l.root_invocation, [1; 16]);
}

#[test]
fn the_rate_budget_bounds_output_per_window_and_resets() {
    let mut r = Rule::new(classes(&[1]), classes(&[1]), false, 2, 60).unwrap();
    assert!(r.trigger(1, &human_event(), 0).is_ok());
    assert!(r.trigger(1, &human_event(), 59).is_ok());
    assert_eq!(
        r.trigger(1, &human_event(), 59).err(),
        Some(Skip::RateBudget)
    );
    assert!(r.trigger(1, &human_event(), 60).is_ok());
}
