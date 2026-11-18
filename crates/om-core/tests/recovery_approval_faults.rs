// SPDX-License-Identifier: MPL-2.0
//! A surviving controller that misbehaves cannot move the restricted device (ADR-0203): a changed target, another
//! request, a bad reply tag, a missing chunk and a conflicting duplicate chunk each leave the attempt exactly as it was.
//! The test switch is process-global, so this is a test binary of its own.

mod common;

use common::confirmation as nonce;
use common::recovery_world::{Fresh, World, fresh, world};
use om_core::api::*;
use om_core::runtime::{TEST_APPROVAL_FAULT, settle_all};
use om_core::state::AttemptState;
use std::sync::atomic::Ordering;

fn settle(w: &mut World, f: &mut Fresh) {
    settle_all(&mut [
        (&mut w.alice, &w.a),
        (&mut w.bob, &w.b),
        (&mut f.rt, &f.ctx),
    ]);
}

fn run(fault: u8) -> (World, Fresh) {
    let mut w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    w.alice
        .dispatch(&w.a, "off", IntentBody::OfferRecoveryApproval);
    w.alice.flush_net(&w.a);
    let link = w
        .alice
        .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
            ctx: w.a.clone(),
            operation_id: "off".into(),
        })
        .unwrap()
        .link
        .unwrap();
    f.go("join", IntentBody::JoinRecoveryApproval { link });
    settle(&mut w, &mut f);
    let rid = w
        .alice
        .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
            ctx: w.a.clone(),
            operation_id: "off".into(),
        })
        .unwrap()
        .request_id
        .unwrap();
    let c = nonce(&mut w.alice, &w.a, "ConfirmRecovery", &rid);
    TEST_APPROVAL_FAULT.store(fault, Ordering::SeqCst);
    w.alice.dispatch(
        &w.a,
        "c",
        IntentBody::ConfirmRecovery {
            request_id: rid,
            confirmation: c,
        },
    );
    w.alice.flush_net(&w.a);
    TEST_APPROVAL_FAULT.store(0, Ordering::SeqCst);
    for _ in 0..4 {
        settle(&mut w, &mut f);
        f.rt.flush_net(&f.ctx);
    }
    (w, f)
}

fn untouched(f: &Fresh) {
    assert_eq!(
        f.view().expect("still restricted").attempt,
        Some(AttemptState::Waiting)
    );
    assert!(
        f.app()["acsChain"].as_array().unwrap().is_empty(),
        "nothing adopted"
    );
}

#[test]
fn a_misbehaving_controller_moves_nothing() {
    let want = |f: &Fresh| f.view().unwrap().approval.map(|a| a.state);
    // Target substituted, other request digest, bad tag: the reply does not bind this exchange.
    for fault in [1u8, 2, 5] {
        let (_w, f) = run(fault);
        untouched(&f);
        assert_eq!(want(&f), Some(ApprovalCode::Failed), "fault {fault}");
    }
    // A missing chunk: never assembled, never applied, still waiting for it.
    let (_w, f) = run(3);
    untouched(&f);
    assert_eq!(want(&f), Some(ApprovalCode::Receiving));
    // A conflicting duplicate chunk aborts the ceremony.
    let (_w, f) = run(4);
    untouched(&f);
    assert_eq!(want(&f), Some(ApprovalCode::Failed));
}
