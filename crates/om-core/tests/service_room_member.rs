// SPDX-License-Identifier: MPL-2.0
//! `services_bots` slice 4: full-member installation, quorum, visible read-all roster, endpoint-revocation carriage with
//! the `sequencing_timeout` bound, rollback rejection and monotone removal.
use om_core::service_control::*;
use om_core::service_room_member::*;
use std::collections::{BTreeMap, BTreeSet};

const SVC: [u8; 32] = [0x41; 32];
fn caps(c: &[&str]) -> BTreeSet<String> {
    c.iter().map(|s| s.to_string()).collect()
}
fn head(version: u64, hash: u8, eps: &[(u8, u64)]) -> ServiceState {
    ServiceState {
        service_id: SVC,
        version,
        prev: None,
        controllers: [(1u64, vec![1u8; 32])].into(),
        threshold: 1,
        endpoints: eps
            .iter()
            .map(|(id, st)| {
                (
                    vec![*id; 16],
                    Endpoint {
                        id: vec![*id; 16],
                        state: *st,
                        profile: 1,
                        key: vec![*id; 40],
                        mode: 2,
                        host_ref: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        hash: [hash; 32],
    }
}
fn human(n: u8) -> (u64, Vec<u8>) {
    (1, vec![n; 32])
}
fn installed(quorum: Option<usize>) -> RoomServices {
    let mut r = RoomServices::new(quorum, 300);
    r.install(
        &caps(&[INSTALL]),
        &head(1, 1, &[(0x10, ACTIVE), (0x11, ACTIVE)]),
        vec![0x81; 16],
        None,
        &[human(1), human(2)],
    )
    .unwrap();
    r
}

#[test]
fn install_needs_the_capability_and_human_quorum() {
    let h = head(1, 1, &[(0x10, ACTIVE)]);
    let mut r = RoomServices::new(Some(2), 300);
    assert_eq!(
        r.install(&caps(&[]), &h, vec![1; 16], None, &[human(1), human(2)]),
        Err(RoomError::NotAuthorized)
    );
    // a service principal, one of its endpoints, and a repeated human do not make a quorum
    let fake = [
        (2u64, SVC.to_vec()),
        (1, vec![0x10; 16]),
        human(1),
        human(1),
    ];
    assert_eq!(
        r.install(&caps(&[INSTALL]), &h, vec![1; 16], None, &fake),
        Err(RoomError::QuorumNotMet)
    );
    r.install(
        &caps(&[INSTALL]),
        &h,
        vec![1; 16],
        None,
        &[human(1), human(2)],
    )
    .unwrap();
    assert_eq!(
        r.install(
            &caps(&[INSTALL]),
            &h,
            vec![2; 16],
            None,
            &[human(1), human(2)]
        ),
        Err(RoomError::AlreadyInstalled)
    );
}

#[test]
fn the_roster_always_says_the_service_can_read_everything_even_with_a_filter() {
    let mut r = installed(None);
    let f = TriggerFilter {
        event_classes: [1u8].into(),
    };
    r.set_filter(&caps(&[INSTALL]), &SVC, Some(f)).unwrap();
    assert_eq!(
        r.set_filter(&caps(&[]), &SVC, None),
        Err(RoomError::NotAuthorized)
    );
    let e = r.entry(&SVC).unwrap();
    assert!(e.filter.is_some() && e.can_read_all_content());
    assert_eq!(e.endpoints.len(), 2);
    assert_eq!(e.admitted_under, (1, [1; 32]));
}

#[test]
fn a_revoking_head_stops_sending_and_any_member_commits_after_the_timeout() {
    let mut r = installed(None);
    let ep = vec![0x10u8; 16];
    r.observe_head(&head(2, 2, &[(0x10, REVOKED), (0x11, ACTIVE)]), 1000)
        .unwrap();
    assert!(r.must_stop_sending(&SVC, &ep) && !r.must_stop_sending(&SVC, &[0x11; 16]));
    assert_eq!(
        r.may_commit_removal(&SVC, &ep, false, 1299),
        Err(RoomError::TooEarly)
    );
    assert_eq!(
        r.may_commit_removal(&SVC, &ep, true, 1001),
        Ok(()),
        "the coordinator may commit at once"
    );
    assert_eq!(r.may_commit_removal(&SVC, &ep, false, 1300), Ok(()));
    assert_eq!(
        r.may_commit_removal(&SVC, &[0x11; 16], false, 9999),
        Err(RoomError::NotRevoked)
    );
    assert!(r.removal_committed(&SVC, &ep));
    assert!(
        !r.removal_committed(&SVC, &ep),
        "a competing identical removal applies once"
    );
    assert!(!r.must_stop_sending(&SVC, &ep));
    assert!(!r.entry(&SVC).unwrap().endpoints.contains(&ep));
}

#[test]
fn a_replaced_endpoint_is_treated_like_a_revoked_one() {
    let mut r = installed(None);
    r.observe_head(&head(2, 2, &[(0x11, ACTIVE), (0x12, ACTIVE)]), 50)
        .unwrap();
    assert!(r.must_stop_sending(&SVC, &[0x10; 16]));
}

#[test]
fn rollback_is_rejected_the_highest_head_wins_and_removal_survives_a_later_head() {
    let mut r = installed(None);
    r.observe_head(&head(3, 3, &[(0x10, REVOKED), (0x11, ACTIVE)]), 10)
        .unwrap();
    assert_eq!(
        r.observe_head(&head(2, 2, &[(0x10, ACTIVE), (0x11, ACTIVE)]), 11),
        Err(RoomError::Rollback)
    );
    assert_eq!(r.held_head(&SVC), Some((3, [3; 32])));
    // the same head again is idempotent; a different hash at the same version is a fork-style rollback
    r.observe_head(&head(3, 3, &[(0x10, REVOKED), (0x11, ACTIVE)]), 12)
        .unwrap();
    assert_eq!(
        r.observe_head(&head(3, 9, &[(0x10, ACTIVE)]), 12),
        Err(RoomError::Rollback)
    );
    // a later head does not cancel the pending removal
    r.observe_head(&head(4, 4, &[(0x10, REVOKED), (0x11, ACTIVE)]), 20)
        .unwrap();
    assert!(r.must_stop_sending(&SVC, &[0x10; 16]));
    assert_eq!(
        r.observe_head(
            &{
                let mut h = head(1, 1, &[]);
                h.service_id = [0x99; 32];
                h
            },
            1
        ),
        Err(RoomError::NotInstalled)
    );
}

#[test]
fn removing_the_installation_removes_every_endpoint_and_needs_the_capability() {
    let mut r = installed(None);
    assert_eq!(
        r.remove_installation(&caps(&[]), &SVC),
        Err(RoomError::NotAuthorized)
    );
    let gone = r.remove_installation(&caps(&[INSTALL]), &SVC).unwrap();
    assert_eq!(gone.len(), 2);
    assert!(r.entry(&SVC).is_none());
    assert_eq!(
        r.remove_installation(&caps(&[INSTALL]), &SVC),
        Err(RoomError::NotInstalled)
    );
}
