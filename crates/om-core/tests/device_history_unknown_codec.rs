// SPDX-License-Identifier: MPL-2.0
//! A snapshot whose `(codec_id, codec_major)` the new device does not know is ignored whole (ADR-0156/0157); the link
//! still completes. Sets a process-global switch, so it is a test binary of its own.

mod common;

use common::history_world::{World, conv_of, texts};
use om_core::api::*;
use om_core::runtime::TEST_FOREIGN_HISTORY_CODEC;
use std::sync::atomic::Ordering;

#[test]
fn an_unknown_codec_is_ignored_whole_and_the_link_completes() {
    let mut w = World::new("codec");
    w.talk(2);
    TEST_FOREIGN_HISTORY_CODEC.store(true, Ordering::SeqCst);
    assert_eq!(w.link(), Some(OutcomeClass::Committed));
    TEST_FOREIGN_HISTORY_CODEC.store(false, Ordering::SeqCst);
    let got = w.transfer(true).expect("announced");
    assert_eq!(got.chunks_done, got.chunks_total);
    assert_eq!(
        got.chunks_ignored, got.chunks_total,
        "every chunk ignored whole"
    );
    assert_eq!(got.messages_added, 0);
    let phone_conv = conv_of(&w.phone, &w.n);
    assert!(
        texts(&w.phone, &w.n, &phone_conv).is_empty(),
        "nothing of it was interpreted"
    );
    let d: DevicesView = w.phone.query(Query::GetDevices { ctx: w.n.clone() });
    assert_eq!(d.devices.len(), 2, "the device is linked all the same");
}
