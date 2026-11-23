// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 5: the V-024 offline-first-contact cases on product code. Every combination of the five
//! evidence inputs is exercised in the model's order; nothing short of both bindings reaches Active.
use om_core::lan_first_contact::*;

/// Applies the evidence the way the model does: each stage needs its predecessor and a verified input.
fn run(
    has_invite: bool,
    intro_signed: bool,
    request_valid: bool,
    own: bool,
    peer: bool,
) -> FirstContact {
    let mut f = FirstContact::new();
    if has_invite {
        f.reach();
    }
    if f.ticket(intro_signed).is_ok() && f.request(request_valid).is_ok() {
        let _ = f.own_binding(own);
        let _ = f.peer_binding(peer);
    }
    f
}

#[test]
fn all_thirty_two_combinations_follow_the_model() {
    for bits in 0u8..32 {
        let b = |i: u8| bits & (1 << i) != 0;
        let (invite, signed, valid, own, peer) = (b(0), b(1), b(2), b(3), b(4));
        let f = run(invite, signed, valid, own, peer);
        let ticket = invite && signed;
        let request = ticket && valid;
        let active = request && own && peer;
        assert_eq!(f.state() == State::Active, active, "{bits:05b}");
        assert_eq!(f.async_key_fetch_allowed(), active);
        assert_eq!(f.device_eligible(), active);
        if invite && !active {
            assert!(!f.async_key_fetch_allowed() && !f.device_eligible());
        }
    }
}

#[test]
fn reachability_alone_is_never_a_relationship() {
    let mut f = FirstContact::new();
    assert_eq!(f.state(), State::Unknown);
    f.reach();
    assert_eq!(f.state(), State::Reachable);
    assert!(!f.async_key_fetch_allowed() && !f.device_eligible());
}

#[test]
fn an_unsigned_local_ticket_is_refused() {
    let mut f = FirstContact::new();
    f.reach();
    assert_eq!(f.ticket(false), Err(StepError::NotVerified));
    assert_eq!(f.state(), State::Reachable);
}

#[test]
fn stages_cannot_be_skipped() {
    let mut f = FirstContact::new();
    assert_eq!(f.ticket(true), Err(StepError::OutOfOrder));
    f.reach();
    assert_eq!(f.request(true), Err(StepError::OutOfOrder));
    assert_eq!(f.own_binding(true), Err(StepError::OutOfOrder));
    f.ticket(true).unwrap();
    assert_eq!(f.peer_binding(true), Err(StepError::OutOfOrder));
}

#[test]
fn both_reciprocal_bindings_are_needed_and_async_fetch_waits_for_them() {
    let mut f = FirstContact::new();
    f.reach();
    f.ticket(true).unwrap();
    f.request(true).unwrap();
    assert_eq!(f.state(), State::Requested);
    assert!(!f.async_key_fetch_allowed());
    f.own_binding(true).unwrap();
    assert_eq!(f.state(), State::HalfBound);
    assert!(
        !f.async_key_fetch_allowed(),
        "skipping the peer's binding is caught"
    );
    assert_eq!(f.peer_binding(false), Err(StepError::NotVerified));
    f.peer_binding(true).unwrap();
    assert_eq!(f.state(), State::Active);
    assert!(f.async_key_fetch_allowed() && f.device_eligible());
}

#[test]
fn losing_reachability_restarts_an_attempt_but_not_an_active_relationship() {
    let mut f = run(true, true, true, true, false);
    f.reachability_lost();
    assert_eq!(f.state(), State::Unknown);
    let mut a = run(true, true, true, true, true);
    a.reachability_lost();
    assert_eq!(a.state(), State::Active);
}
