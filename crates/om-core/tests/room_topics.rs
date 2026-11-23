// SPDX-License-Identifier: MPL-2.0
//! `communities_publications` slice 4: authorization follows the kind-to-capability table; moving a message keeps its
//! EventID and author; forum and announcement modes come from capabilities; follow and mute stay local.
use om_core::room_topics::*;
use std::collections::BTreeSet;

fn caps(c: &[&str]) -> BTreeSet<String> {
    c.iter().map(|s| s.to_string()).collect()
}
const T1: TopicId = [1; 16];
const T2: TopicId = [2; 16];
const ALICE: &[u8] = b"alice";
const BOB: &[u8] = b"bob";
fn st(c: u64, e: u8) -> Stamp {
    (c, [e; 32])
}
fn with_topics() -> RoomTopics {
    let mut r = RoomTopics::new();
    r.create(
        &caps(&[CREATE_TOPIC]),
        ALICE.to_vec(),
        T1,
        "general",
        st(1, 1),
    )
    .unwrap();
    r.create(&caps(&[CREATE_TOPIC]), ALICE.to_vec(), T2, "help", st(2, 2))
        .unwrap();
    r
}

#[test]
fn create_needs_its_capability_and_a_new_valid_id() {
    let mut r = RoomTopics::new();
    assert_eq!(
        r.create(&caps(&[]), ALICE.to_vec(), T1, "x", st(1, 1)),
        Err(TopicError::NotAuthorized)
    );
    assert_eq!(
        r.create(&caps(&[CREATE_TOPIC]), ALICE.to_vec(), T1, "", st(1, 1)),
        Err(TopicError::Malformed)
    );
    let long = "x".repeat(257);
    assert_eq!(
        r.create(&caps(&[CREATE_TOPIC]), ALICE.to_vec(), T1, &long, st(1, 1)),
        Err(TopicError::Malformed)
    );
    r.create(&caps(&[CREATE_TOPIC]), ALICE.to_vec(), T1, "ok", st(1, 1))
        .unwrap();
    assert_eq!(
        r.create(
            &caps(&[CREATE_TOPIC]),
            ALICE.to_vec(),
            T1,
            "again",
            st(2, 2)
        ),
        Err(TopicError::Duplicate)
    );
}

#[test]
fn update_and_status_need_the_creator_or_manage_topics() {
    let mut r = with_topics();
    assert_eq!(
        r.update(&caps(&[]), BOB, T1, "mine", st(5, 5)),
        Err(TopicError::NotAuthorized)
    );
    assert_eq!(
        r.set_status(&caps(&[CREATE_TOPIC]), BOB, T1, 2, st(5, 5)),
        Err(TopicError::NotAuthorized)
    );
    r.update(&caps(&[]), ALICE, T1, "renamed", st(5, 5))
        .unwrap();
    r.set_status(&caps(&[MANAGE_TOPICS]), BOB, T1, 2, st(6, 6))
        .unwrap();
    let t = r.topic(&T1).unwrap();
    assert_eq!(t.title, "renamed");
    assert_eq!(t.status, Status::Resolved);
    assert_eq!(
        r.set_status(&caps(&[MANAGE_TOPICS]), BOB, T1, 4, st(7, 7)),
        Err(TopicError::Malformed)
    );
    assert_eq!(
        r.set_status(&caps(&[MANAGE_TOPICS]), BOB, [9; 16], 1, st(7, 7)),
        Err(TopicError::UnknownTopic)
    );
}

#[test]
fn mutable_fields_converge_regardless_of_arrival_order() {
    let mut a = with_topics();
    let mut b = with_topics();
    let m = caps(&[MANAGE_TOPICS]);
    a.update(&m, BOB, T1, "first", st(10, 1)).unwrap();
    a.update(&m, BOB, T1, "second", st(11, 2)).unwrap();
    b.update(&m, BOB, T1, "second", st(11, 2)).unwrap();
    b.update(&m, BOB, T1, "first", st(10, 1)).unwrap();
    assert_eq!(a.topic(&T1).unwrap().title, "second");
    assert_eq!(b.topic(&T1).unwrap().title, "second");
}

#[test]
fn moving_a_message_keeps_its_event_id_and_author() {
    let mut r = with_topics();
    let ev = [0x77; 32];
    r.post(Mode::Chat, true, ev, ALICE.to_vec(), Some(T1))
        .unwrap();
    // the author, still holding room.post, may move it; someone else needs room.manage_topics
    assert_eq!(
        r.move_message(&caps(&[POST]), BOB, ev, Some(T2), st(3, 3)),
        Err(TopicError::NotAuthorized)
    );
    assert_eq!(
        r.move_message(&caps(&[]), ALICE, ev, Some(T2), st(3, 3)),
        Err(TopicError::NotAuthorized)
    );
    r.move_message(&caps(&[POST]), ALICE, ev, Some(T2), st(3, 3))
        .unwrap();
    assert_eq!(r.placement(&ev), Some(Some(T2)));
    r.move_message(&caps(&[MANAGE_TOPICS]), BOB, ev, None, st(4, 4))
        .unwrap();
    assert_eq!(r.placement(&ev), Some(None));
    assert_eq!(r.author_of(&ev), Some(ALICE));
    // an older move arriving late does not override a newer one
    r.move_message(&caps(&[POST]), ALICE, ev, Some(T1), st(3, 9))
        .unwrap();
    assert_eq!(r.placement(&ev), Some(None));
    assert_eq!(
        r.move_message(&caps(&[POST]), ALICE, ev, Some([9; 16]), st(9, 9)),
        Err(TopicError::UnknownTopic)
    );
    assert_eq!(
        r.move_message(&caps(&[POST]), ALICE, [0; 32], None, st(9, 9)),
        Err(TopicError::UnknownMessage)
    );
}

#[test]
fn forum_requires_a_topic_on_a_top_level_post_only() {
    let mut r = with_topics();
    assert_eq!(
        r.post(Mode::Forum, true, [1; 32], ALICE.to_vec(), None),
        Err(TopicError::TopicRequired)
    );
    r.post(Mode::Forum, true, [1; 32], ALICE.to_vec(), Some(T1))
        .unwrap();
    r.post(Mode::Forum, false, [2; 32], BOB.to_vec(), None)
        .unwrap();
    r.post(Mode::Chat, true, [3; 32], BOB.to_vec(), None)
        .unwrap();
    assert_eq!(
        r.post(Mode::Chat, true, [4; 32], BOB.to_vec(), Some([9; 16])),
        Err(TopicError::UnknownTopic)
    );
}

#[test]
fn announcement_is_a_capability_fact_not_a_label() {
    assert!(announcement_is_effective(&caps(&["room.react"])));
    assert!(!announcement_is_effective(&caps(&[POST, "room.react"])));
}

#[test]
fn follow_and_mute_are_local_and_exclusive() {
    let mut a = Attention::default();
    a.follow(T1);
    assert!(a.is_followed(&T1));
    a.mute(T1);
    assert!(a.is_muted(&T1) && !a.is_followed(&T1));
    a.clear(&T1);
    assert!(!a.is_muted(&T1) && !a.is_followed(&T1));
}
