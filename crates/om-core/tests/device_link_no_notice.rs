// SPDX-License-Identifier: MPL-2.0
//! A ceremony that finishes without the enrollment notice fails (D24, plan phase-4 gate). This test sets a
//! process-global switch, so it is a test binary of its own.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::runtime::{LoopbackNetwork, Runtime, TEST_OMIT_NOTICE, settle_all};
use std::sync::atomic::Ordering;

#[test]
fn an_approval_without_the_notice_is_refused_by_the_new_device() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice-nn", &net);
    let mut phone = Runtime::new("phone-nn", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let n = phone.open("p-n");
    alice.dispatch(&a, "offer", IntentBody::OfferDeviceLink);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
            ctx: a.clone(),
            operation_id: "offer".into(),
        })
        .unwrap()
        .link;
    phone.dispatch(&n, "ask", IntentBody::RequestDeviceLink { link });
    settle_all(&mut [(&mut alice, &a), (&mut phone, &n)]);
    let pending: DevicesView = alice.query(Query::GetDevices { ctx: a.clone() });
    let request_id = pending.link_requests[0].request_id.clone();

    TEST_OMIT_NOTICE.store(true, Ordering::SeqCst);
    let confirmation = nonce(&mut alice, &a, "ApproveDeviceLink", &request_id);
    alice.dispatch(
        &a,
        "ok",
        IntentBody::ApproveDeviceLink {
            request_id,
            confirmation,
        },
    );
    TEST_OMIT_NOTICE.store(false, Ordering::SeqCst);
    settle_all(&mut [(&mut alice, &a), (&mut phone, &n)]);

    let asked: LinkRequestView = phone
        .query::<Option<LinkRequestView>>(Query::GetLinkRequest {
            ctx: n.clone(),
            operation_id: "ask".into(),
        })
        .unwrap();
    assert_eq!(
        asked.state,
        LinkRequestState::Rejected,
        "no notice, no ceremony"
    );
    let devices: DevicesView = phone.query(Query::GetDevices { ctx: n.clone() });
    assert!(devices.devices.is_empty(), "the phone adopted no account");
    let st = phone.core.debug_app_state(&n).unwrap();
    assert!(st["acsChain"].as_array().unwrap().is_empty());
    assert_eq!(st["keysPublished"], false);
    assert_eq!(
        phone.net.key_inventory().len(),
        1,
        "only the existing device published keys"
    );
}
