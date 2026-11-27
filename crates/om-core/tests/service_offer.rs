// SPDX-License-Identifier: MPL-2.0
use om_core::service_control::{ACTIVE, Endpoint, ServiceState};
use om_core::service_offer::{MAX_OFFERS, Membership, Offer, OfferError, ServiceRoom};
use om_core::service_room_member::{INSTALL, RoomError, TriggerFilter};
use std::collections::BTreeSet;

const S: [u8; 32] = [0x61; 32];
const EP: [u8; 16] = [0x10; 16];

fn caps() -> BTreeSet<String> {
    [INSTALL.into()].into()
}

fn offer() -> Offer {
    Offer {
        head: ServiceState {
            service_id: S,
            version: 1,
            prev: None,
            controllers: [(1, vec![1; 32])].into(),
            threshold: 1,
            endpoints: [(
                EP.to_vec(),
                Endpoint {
                    id: EP.to_vec(),
                    state: ACTIVE,
                    profile: 1,
                    key: vec![1; 40],
                    mode: 2,
                    host_ref: None,
                },
            )]
            .into(),
            hash: [1; 32],
        },
        incarnation: vec![5; 16],
        capabilities: ["message.create".into()].into(),
        filter: Some(TriggerFilter {
            event_classes: [1, 2].into(),
        }),
        approvers: vec![(1, vec![1; 32]), (1, vec![2; 32])],
        name: "Test\u{202e}Service\n".into(),
    }
}

#[test]
fn an_offer_needs_its_exact_target_and_room_authority_to_install() {
    let mut room = ServiceRoom::new(None, 300);
    let target = room.propose(offer()).unwrap();
    assert_eq!(room.listing(), vec![(S, Membership::Proposed)]);
    assert!(room.members().entry(&S).is_none());
    assert_eq!(room.offer(&S).unwrap().name, "TestService");
    assert_eq!(
        room.install_offer(&caps(), &S, &[0; 32]),
        Err(OfferError::StaleTarget)
    );
    assert_eq!(
        room.install_offer(&BTreeSet::new(), &S, &target),
        Err(OfferError::Room(RoomError::NotAuthorized))
    );
    assert_eq!(room.listing(), vec![(S, Membership::Proposed)]);

    let mut quorum_room = ServiceRoom::new(Some(2), 300);
    let quorum_target = quorum_room.propose(offer()).unwrap();
    assert_eq!(
        quorum_room.install_offer(&caps(), &S, &quorum_target),
        Err(OfferError::Room(RoomError::QuorumNotMet))
    );

    room.install_offer(&caps(), &S, &target).unwrap();
    assert_eq!(room.listing(), vec![(S, Membership::Installed)]);
    assert_eq!(room.current_target(&S), Some(target));
    assert!(room.members().entry(&S).unwrap().can_read_all_content());
}

#[test]
fn replacement_invalidates_the_old_filter_and_capability_review() {
    let mut room = ServiceRoom::new(None, 300);
    let original = offer();
    let old = room.propose(original.clone()).unwrap();
    let mut changed_head = original.clone();
    changed_head.head.version = 2;
    changed_head.head.hash = [2; 32];
    assert_eq!(room.propose(changed_head), Err(OfferError::StaleTarget));
    let mut changed = original.clone();
    changed.filter = None;
    let new_filter = room.propose(changed.clone()).unwrap();
    changed.capabilities.insert("message.edit".into());
    let new_caps = room.propose(changed).unwrap();
    assert_ne!(old, new_filter);
    assert_ne!(new_filter, new_caps);
    for stale in [old, new_filter] {
        assert_eq!(
            room.install_offer(&caps(), &S, &stale),
            Err(OfferError::StaleTarget)
        );
    }
    room.install_offer(&caps(), &S, &new_caps).unwrap();

    room.members_mut()
        .set_filter(&caps(), &S, original.filter)
        .unwrap();
    assert_eq!(
        room.remove_installation(&caps(), &S, &new_caps),
        Err(OfferError::StaleTarget)
    );
    assert_ne!(room.current_target(&S), Some(new_caps));
}

#[test]
fn removal_leaves_a_tombstone_and_a_fresh_offer_can_follow() {
    let mut room = ServiceRoom::new(None, 300);
    let target = room.propose(offer()).unwrap();
    room.install_offer(&caps(), &S, &target).unwrap();
    assert_eq!(
        room.remove_installation(&caps(), &S, &[0; 32]),
        Err(OfferError::StaleTarget)
    );
    assert_eq!(
        room.remove_installation(&BTreeSet::new(), &S, &target),
        Err(OfferError::Room(RoomError::NotAuthorized))
    );
    assert_eq!(
        room.remove_installation(&caps(), &S, &target),
        Ok(vec![EP.to_vec()])
    );
    assert_eq!(room.listing(), vec![(S, Membership::Removed)]);
    assert_eq!(room.current_target(&S), Some(target));
    assert!(room.members().entry(&S).is_none());
    assert_eq!(
        room.remove_installation(&caps(), &S, &target),
        Err(OfferError::StaleTarget)
    );

    assert_eq!(room.propose(offer()), Err(OfferError::Malformed));
    let mut next = offer();
    next.incarnation = vec![6; 16];
    let next_target = room.propose(next.clone()).unwrap();
    assert_ne!(target, next_target);
    assert!(room.withdraw_offer(&S));
    assert_eq!(room.listing(), vec![(S, Membership::Removed)]);
    assert_eq!(room.current_target(&S), Some(target));

    room.propose(next).unwrap();
    room.install_offer(&caps(), &S, &next_target).unwrap();
    assert_eq!(room.listing(), vec![(S, Membership::Installed)]);
}

#[test]
fn offers_are_bounded_and_validated() {
    let mut room = ServiceRoom::new(None, 300);
    let mut bad = offer();
    bad.incarnation.pop();
    assert_eq!(room.propose(bad), Err(OfferError::Malformed));
    let mut bad = offer();
    bad.filter.as_mut().unwrap().event_classes.clear();
    assert_eq!(room.propose(bad), Err(OfferError::Malformed));
    let mut bad = offer();
    bad.capabilities.insert("x".repeat(129));
    assert_eq!(room.propose(bad), Err(OfferError::Malformed));

    for i in 0..MAX_OFFERS {
        let mut next = offer();
        next.head.service_id = [i as u8; 32];
        next.name = "é".repeat(100);
        room.propose(next).unwrap();
    }
    assert_eq!(room.offer(&[0; 32]).unwrap().name.chars().count(), 64);
    assert_eq!(room.propose(offer()), Err(OfferError::TooMany));
    assert!(room.withdraw_offer(&[0; 32]));
    assert!(room.propose(offer()).is_ok());
    let mut replace = offer();
    replace.capabilities.clear();
    assert!(room.propose(replace).is_ok());
    assert_eq!(room.listing().len(), MAX_OFFERS);
}
