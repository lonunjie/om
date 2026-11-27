// SPDX-License-Identifier: MPL-2.0
//! `bridges` slice 3: admission with the protected capability and quorum, export classes enforced by the host, stale
//! mapping fails closed, removal stops export, no hiding behind an account or service author.
use om_core::bridge_room::*;
use om_core::service_control::*;
use om_core::service_room_member::RoomError;
use std::collections::{BTreeMap, BTreeSet};

const B: [u8; 32] = [0x61; 32];
const EP: [u8; 16] = [0x10; 16];
fn caps(c: &[&str]) -> BTreeSet<String> {
    c.iter().map(|s| s.to_string()).collect()
}
fn head(version: u64, hash: u8, st: u64) -> ServiceState {
    ServiceState {
        service_id: B,
        version,
        prev: None,
        controllers: [(1u64, vec![1u8; 32])].into(),
        threshold: 1,
        endpoints: BTreeMap::from([(
            EP.to_vec(),
            Endpoint {
                id: EP.to_vec(),
                state: st,
                profile: 1,
                key: vec![1; 40],
                mode: 2,
                host_ref: None,
            },
        )]),
        hash: [hash; 32],
    }
}
fn mapping(direction: u64, classes: &[u8]) -> Mapping {
    Mapping {
        incarnation: [5; 16],
        namespace: "test.net".into(),
        direction,
        allowed_classes: classes.iter().copied().collect(),
        active: true,
    }
}
fn human(n: u8) -> (u64, Vec<u8>) {
    (1, vec![n; 32])
}
fn room() -> BridgeRoom {
    let mut r = BridgeRoom::new(Some(2), 300, 600);
    r.admit(
        &caps(&[ADD_BRIDGE]),
        &head(1, 1, ACTIVE),
        mapping(BOTH, &[1, 2]),
        &[human(1), human(2)],
        0,
    )
    .unwrap();
    r
}

#[test]
fn admission_needs_add_bridge_not_install_service_and_human_quorum() {
    let mut r = BridgeRoom::new(Some(2), 300, 600);
    let h = head(1, 1, ACTIVE);
    let two = [human(1), human(2)];
    assert_eq!(
        r.admit(
            &caps(&["room.install_service"]),
            &h,
            mapping(BOTH, &[1]),
            &two,
            0
        ),
        Err(BridgeRoomError::Room(RoomError::NotAuthorized))
    );
    assert_eq!(
        r.admit(
            &caps(&[ADD_BRIDGE]),
            &h,
            mapping(BOTH, &[1]),
            &[human(1), (3, B.to_vec())],
            0
        ),
        Err(BridgeRoomError::Room(RoomError::QuorumNotMet))
    );
    assert_eq!(
        r.admit(&caps(&[ADD_BRIDGE]), &h, mapping(9, &[1]), &two, 0),
        Err(BridgeRoomError::BadDirection)
    );
    assert_eq!(
        r.admit(&caps(&[ADD_BRIDGE]), &h, mapping(BOTH, &[]), &two, 0),
        Err(BridgeRoomError::EmptyClasses)
    );
    r.admit(&caps(&[ADD_BRIDGE]), &h, mapping(BOTH, &[1]), &two, 0)
        .unwrap();
    assert!(r.roster(&B).unwrap().can_read_all_content());
}

#[test]
fn export_follows_the_mapping_classes_and_direction() {
    let r = room();
    assert_eq!(r.may_export(&B, &EP, 1, 10), Ok(()));
    assert_eq!(r.may_export(&B, &EP, 9, 10), Err(Blocked::ClassNotAllowed));
    assert_eq!(r.may_export(&[0; 32], &EP, 1, 10), Err(Blocked::NoMapping));
    let mut inbound = BridgeRoom::new(None, 300, 600);
    inbound
        .admit(
            &caps(&[ADD_BRIDGE]),
            &head(1, 1, ACTIVE),
            mapping(TO_OM, &[1]),
            &[],
            0,
        )
        .unwrap();
    assert_eq!(inbound.may_export(&B, &EP, 1, 10), Err(Blocked::Direction));
}

#[test]
fn a_stale_mapping_cannot_send_until_a_fresh_head_is_observed() {
    let mut r = room();
    assert_eq!(r.may_export(&B, &EP, 1, 601), Err(Blocked::StaleMapping));
    r.observe_head(&head(2, 2, ACTIVE), 700).unwrap();
    assert_eq!(r.may_export(&B, &EP, 1, 1000), Ok(()));
    assert_eq!(
        r.observe_head(&head(1, 1, ACTIVE), 701),
        Err(BridgeRoomError::Room(RoomError::Rollback))
    );
}

#[test]
fn a_revoking_head_stops_export_and_removal_ends_the_mapping() {
    let mut r = room();
    r.observe_head(&head(2, 2, REVOKED), 50).unwrap();
    assert_eq!(r.may_export(&B, &EP, 1, 51), Err(Blocked::EndpointStopped));
    assert!(r.members().may_commit_removal(&B, &EP, false, 351).is_ok());
    assert_eq!(
        r.remove(&caps(&[]), &B),
        Err(BridgeRoomError::Room(RoomError::NotAuthorized))
    );
    let gone = r.remove(&caps(&[ADD_BRIDGE]), &B).unwrap();
    assert_eq!(gone.len(), 1);
    assert_eq!(r.may_export(&B, &EP, 1, 60), Err(Blocked::NoMapping));
}

#[test]
fn revoking_the_mapping_stops_export_at_once() {
    let mut r = room();
    assert_eq!(
        r.revoke_mapping(&caps(&[]), &B),
        Err(BridgeRoomError::Room(RoomError::NotAuthorized))
    );
    r.revoke_mapping(&caps(&[ADD_BRIDGE]), &B).unwrap();
    assert_eq!(r.may_export(&B, &EP, 1, 10), Err(Blocked::Revoked));
}

#[test]
fn a_bridge_cannot_hide_behind_an_account_or_service_author() {
    assert!(import_author_ok(KIND_BRIDGE));
    assert!(!import_author_ok(KIND_ACCOUNT) && !import_author_ok(KIND_SERVICE));
}

// ---- ADR-0270: offers and review targets ----

fn offer(m: Mapping) -> Offer {
    Offer {
        head: head(1, 1, ACTIVE),
        mapping: m,
        approvers: vec![human(1), human(2)],
        name: "Test\u{202E}Bridge\n".into(),
        external_claim: Some("claims transport encryption".into()),
    }
}

#[test]
fn offer_is_not_a_member_until_admitted_with_its_exact_target() {
    let mut r = BridgeRoom::new(Some(2), 300, 600);
    let t = r.propose(offer(mapping(BOTH, &[1, 2]))).unwrap();
    assert_eq!(r.listing(), vec![(B, Membership::Proposed)]);
    assert!(r.mapping(&B).is_none());
    assert_eq!(r.may_export(&B, &EP, 1, 0), Err(Blocked::NoMapping));
    assert_eq!(r.offer(&B).unwrap().name, "TestBridge");
    // A target for a different reading is refused and changes nothing.
    let other = review_target(&B, &mapping(TO_FOREIGN, &[1, 2]));
    assert_eq!(
        r.admit_offer(&caps(&[ADD_BRIDGE]), &B, &other, 0),
        Err(OfferError::StaleTarget)
    );
    assert_eq!(r.listing(), vec![(B, Membership::Proposed)]);
    // The right target without the protected capability is refused by the existing path.
    assert!(matches!(
        r.admit_offer(&caps(&[]), &B, &t, 0),
        Err(OfferError::Room(_))
    ));
    assert_eq!(r.listing(), vec![(B, Membership::Proposed)]);
    r.admit_offer(&caps(&[ADD_BRIDGE]), &B, &t, 0).unwrap();
    assert_eq!(r.listing(), vec![(B, Membership::Member)]);
    assert_eq!(r.current_target(&B), Some(t));
}

#[test]
fn replacing_an_offer_invalidates_the_earlier_review() {
    let mut r = BridgeRoom::new(Some(2), 300, 600);
    let old = r.propose(offer(mapping(TO_FOREIGN, &[1]))).unwrap();
    let new = r.propose(offer(mapping(BOTH, &[1, 2]))).unwrap();
    assert_ne!(old, new);
    assert_eq!(
        r.admit_offer(&caps(&[ADD_BRIDGE]), &B, &old, 0),
        Err(OfferError::StaleTarget)
    );
    assert!(r.admit_offer(&caps(&[ADD_BRIDGE]), &B, &new, 0).is_ok());
}

#[test]
fn removal_keeps_a_tombstone_and_a_new_offer_can_follow() {
    let mut r = room();
    let t = r.current_target(&B).unwrap();
    assert_eq!(
        r.remove_member(&caps(&[ADD_BRIDGE]), &B, &[0; 32]),
        Err(OfferError::StaleTarget)
    );
    assert!(r.remove_member(&caps(&[]), &B, &t).is_err());
    assert_eq!(r.listing(), vec![(B, Membership::Member)]);
    r.remove_member(&caps(&[ADD_BRIDGE]), &B, &t).unwrap();
    assert_eq!(r.listing(), vec![(B, Membership::Removed)]);
    assert_eq!(r.may_export(&B, &EP, 1, 0), Err(Blocked::NoMapping));
    let t2 = r.propose(offer(mapping(BOTH, &[1, 2]))).unwrap();
    assert_eq!(r.listing(), vec![(B, Membership::Proposed)]);
    r.admit_offer(&caps(&[ADD_BRIDGE]), &B, &t2, 0).unwrap();
}

#[test]
fn offers_are_bounded_and_validated() {
    let mut r = BridgeRoom::new(None, 300, 600);
    assert_eq!(
        r.propose(offer(mapping(9, &[1]))).err(),
        Some(OfferError::Malformed)
    );
    assert_eq!(
        r.propose(offer(mapping(BOTH, &[]))).err(),
        Some(OfferError::Malformed)
    );
    let mut room = room();
    assert_eq!(
        room.propose(offer(mapping(BOTH, &[1]))).err(),
        Some(OfferError::Malformed)
    );
    let mut long = offer(mapping(BOTH, &[1]));
    long.external_claim = Some("x".repeat(1000));
    r.propose(long).unwrap();
    assert_eq!(
        r.offer(&B).unwrap().external_claim.as_ref().unwrap().len(),
        160
    );
    assert!(r.withdraw_offer(&B));
    assert!(r.listing().is_empty());
}
