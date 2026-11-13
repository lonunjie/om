// SPDX-License-Identifier: MPL-2.0
//! Multi-device phase 5: the existing device hands the new device a `DeviceHistorySnapshot` (MULTI_DEVICE_SYNC
//! "History transfer"). Round trip on a second profile, nothing readable on the relay, events merge by union.

mod common;

use common::history_world::{World, conv_of, texts};
use om_core::api::*;

#[test]
fn the_new_device_gets_the_older_messages_in_order() {
    let mut w = World::new("hist1");
    w.talk(3);
    let before = texts(&w.alice, &w.a, &w.a_conv);
    assert_eq!(before.len(), 6);
    assert_eq!(w.link(), Some(OutcomeClass::Committed));

    let sent = w
        .transfer(false)
        .expect("the existing device announced a snapshot");
    assert_eq!(sent.role, HistoryRole::Sender);
    assert!(sent.chunks_total >= 1);
    let got = w.transfer(true).expect("the new device expected one");
    assert_eq!(got.role, HistoryRole::Receiver);
    assert_eq!(got.chunks_done, got.chunks_total);
    assert_eq!(got.chunks_ignored, 0);
    assert_eq!(got.messages_added, 6);

    let phone_conv = conv_of(&w.phone, &w.n);
    assert_eq!(
        texts(&w.phone, &w.n, &phone_conv),
        before,
        "same messages, same order"
    );

    // The conversation keeps working, and the older messages are not duplicated by what comes next.
    w.carol.dispatch(
        &w.c,
        "later",
        IntentBody::SendMessage {
            conversation_id: w.c_conv.clone(),
            draft: Draft {
                text: "after the link".into(),
            },
        },
    );
    w.settle();
    let on_phone = texts(&w.phone, &w.n, &phone_conv);
    assert_eq!(on_phone.len(), 7);
    assert_eq!(on_phone.last().unwrap(), "after the link");
}

#[test]
fn a_big_history_is_bounded_and_newest_first() {
    let mut w = World::new("hist2");
    w.talk(2);
    // Long messages force several chunks.
    for i in 0..60 {
        w.alice.dispatch(
            &w.a,
            &format!("big{i}"),
            IntentBody::SendMessage {
                conversation_id: w.a_conv.clone(),
                draft: Draft {
                    text: format!("{i:02}-{}", "x".repeat(1500)),
                },
            },
        );
        w.settle();
    }
    assert_eq!(w.link(), Some(OutcomeClass::Committed));
    let got = w.transfer(true).unwrap();
    assert!(got.chunks_total > 1, "more than one chunk");
    assert_eq!(got.chunks_done, got.chunks_total);
    let phone_conv = conv_of(&w.phone, &w.n);
    let on_phone = texts(&w.phone, &w.n, &phone_conv);
    assert!(
        on_phone.iter().any(|t| t.starts_with("59-")),
        "the newest is there"
    );
    assert_eq!(on_phone.len() as u32, got.messages_added);
}

#[test]
fn the_relay_never_holds_the_history_in_the_clear() {
    let mut w = World::new("hist3");
    w.talk(2);
    assert_eq!(w.link(), Some(OutcomeClass::Committed));
    assert!(w.transfer(true).unwrap().messages_added > 0);
    for bytes in w.net.all_deposited_bytes() {
        for needle in ["alice says", "carol says"] {
            assert!(
                !bytes
                    .windows(needle.len())
                    .any(|win| win == needle.as_bytes()),
                "no history text on a relay"
            );
        }
    }
}

#[test]
fn an_empty_history_announces_no_snapshot() {
    let mut w = World::new("hist4");
    assert_eq!(w.link(), Some(OutcomeClass::Committed));
    assert!(w.transfer(true).is_none());
    assert!(w.transfer(false).is_none());
}
