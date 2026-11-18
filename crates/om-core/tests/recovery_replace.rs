// SPDX-License-Identifier: MPL-2.0
//! Replacing the recovery key (ADR-0205): a person whose account has no recovery kit (made while the Home offered no
//! recovery place, or before it did), or who wants a sheet with a newer checkpoint, makes a new key. Nothing changes until
//! the new key is confirmed saved; then the account's control state commits to the new recovery policy, the device keeps a
//! new kit, and the record is written under it. The old key stops being able to recover.

mod common;

use common::recovery_world::{Fresh, fresh, place, root_of, world_with, adopt_via_approval, mono, World};
use om_core::api::*;
use om_core::runtime::settle_all;

fn settle(w: &mut World) {
    settle_all(&mut [(&mut w.alice, &w.a), (&mut w.bob, &w.b)]);
}

fn status(w: &World) -> RecoveryStatusView {
    w.alice.query(Query::GetRecoveryStatus { ctx: w.a.clone() })
}

/// Ask for a new key; returns its operation id and what Core shows once.
fn replace(w: &mut World, op: &str) -> (String, Option<RecoveryKeyView>) {
    w.alice.dispatch(&w.a, op, IntentBody::ReplaceRecoveryKey);
    w.alice.flush_net(&w.a);
    let key = w.alice.query(Query::GetRecoveryKey {
        ctx: w.a.clone(),
        key_operation_id: op.into(),
    });
    (op.to_string(), key)
}

fn confirm_saved(w: &mut World, op: &str) -> Option<OutcomeClass> {
    w.alice.dispatch(
        &w.a,
        &format!("saved-{op}"),
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: op.into(),
        },
    );
    settle(w);
    w.alice.outcome(&w.a, &format!("saved-{op}"))
}

#[test]
fn an_account_without_a_kit_makes_a_key_and_a_fresh_device_recovers_with_it() {
    let mut w = world_with(false);
    assert!(status(&w).record.is_none(), "no kit: the record is off");
    assert!(w.net.recovery_puts().is_empty());
    // The Home now offers a place; the person asks for a new key.
    w.alice.set_recovery_place(Some(place()));
    let (op, key) = replace(&mut w, "replace-1");
    let key = key.expect("a key to show");
    assert!(key.sheet.is_some(), "the sheet names where the account is saved");
    // Nothing has changed until it is confirmed: no record, no new policy on the head.
    assert!(w.net.recovery_puts().is_empty());
    assert!(status(&w).record.is_none());

    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    settle(&mut w);
    assert_eq!(w.net.recovery_puts().len(), 1, "the record is written under the new kit");
    assert_eq!(status(&w).record, Some(RecoveryRecordCode::Ready));
    let _ = root_of(&key.groups);

    // A device that lost everything recovers from the new sheet, approved by Alice's device.
    let mut f: Fresh = fresh(&w.net);
    f.begin(&key.sheet.clone().unwrap());
    adopt_via_approval(&mut w, &mut f);
    assert!(f.view().is_none(), "the restricted state is gone: the account is back");
}

#[test]
fn abandoning_the_new_key_changes_nothing_and_a_second_ask_replaces_the_first() {
    let mut w = world_with(true);
    let puts = w.net.recovery_puts().len();
    let (_, first) = replace(&mut w, "replace-a");
    let first = first.expect("key");
    let (op2, second) = replace(&mut w, "replace-b");
    let second = second.expect("key");
    assert_ne!(first.groups, second.groups, "each ask makes a fresh key");
    // The first key can no longer be confirmed; the second can.
    assert_eq!(confirm_saved(&mut w, "replace-a"), Some(OutcomeClass::Rejected { reason: RejectReason::NotFound }));
    assert_eq!(w.net.recovery_puts().len(), puts, "nothing was written for an unconfirmed key");
    assert_eq!(confirm_saved(&mut w, &op2), Some(OutcomeClass::Committed));
    settle(&mut w);
    assert!(w.net.recovery_puts().len() > puts, "the new record is written");
}

#[test]
fn after_a_replacement_the_old_key_can_no_longer_recover_the_account() {
    let mut w = world_with(true);
    let old_sheet = w.sheet.clone();
    let (op, key) = replace(&mut w, "replace-c");
    assert!(key.is_some());
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    settle(&mut w);
    // The old sheet still finds the old record, but its request is signed by a method the account no longer commits to.
    let mut f: Fresh = fresh(&w.net);
    f.begin(&old_sheet);
    assert_eq!(f.go("req", IntentBody::RequestRecovery), Some(OutcomeClass::Committed));
    w.alice.dispatch(&w.a, "off", IntentBody::OfferRecoveryApproval);
    w.alice.flush_net(&w.a);
    let offer = |w: &World| -> RecoveryApprovalOfferView {
        w.alice
            .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer { ctx: w.a.clone(), operation_id: "off".into() })
            .expect("offer")
    };
    let link = offer(&w).link.expect("link");
    f.go("join", IntentBody::JoinRecoveryApproval { link });
    for _ in 0..4 {
        settle_all(&mut [(&mut w.alice, &w.a), (&mut w.bob, &w.b), (&mut f.rt, &f.ctx)]);
    }
    assert!(offer(&w).request_id.is_none(), "the controller shows no request to confirm: it is not signed by the account's recovery method");
    assert!(f.view().is_some(), "the old key did not bring the account back");
}

#[test]
fn saving_a_replacement_rechecks_for_an_incoming_recovery() {
    let mut w = world_with(true);
    let (op, key) = replace(&mut w, "replacement");
    assert!(key.is_some());
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(
        f.go("recover", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    let request = w.net.recovery_requests().pop().unwrap();
    w.alice
        .recovery_observe_request(&w.a, &request, &mono(1, 0));
    let before = w.alice.core.debug_app_state(&w.a).unwrap()["acsChain"].clone();
    assert_eq!(
        confirm_saved(&mut w, &op),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Conflict
        })
    );
    let after = w.alice.core.debug_app_state(&w.a).unwrap();
    assert_eq!(after["acsChain"], before);
    assert!(after["pendingRecovery"].is_null());
}
