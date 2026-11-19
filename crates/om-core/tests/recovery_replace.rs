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

#[test]
fn a_replaced_inbox_still_alerts_after_restart() {
    let mut w = world_with(true);
    w.alice.recovery_tick(&w.a, &mono(1, 0));
    w.alice.flush_net(&w.a);

    let mut fresh = fresh(&w.net);
    fresh.begin(&w.sheet);
    let (op, _) = replace(&mut w, "replace-for-alerts");
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    assert_eq!(fresh.go("req", IntentBody::RequestRecovery), Some(OutcomeClass::Committed));

    w.alice.restart();
    w.a = w.alice.open("p-a");
    w.alice.tick(61_000);
    w.alice.recovery_tick(&w.a, &mono(2, 0));
    w.alice.flush_net(&w.a);
    let view = serde_json::to_value(status(&w)).unwrap();
    assert_eq!(view["earlierCodeUsed"], true);
    assert!(status(&w).requests.is_empty());

    w.alice.restart();
    w.a = w.alice.open("p-a");
    assert_eq!(serde_json::to_value(status(&w)).unwrap()["earlierCodeUsed"], true);
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
    use common::recovery_world::DELAY_MS;
    use om_core::host::{NetCall, NetResult};

    let mut w = world_with(true);
    let old_sheet = w.sheet.clone();
    let (etag, old_record) = w.net.recovery_record(&w.keys.locator).unwrap();
    let (op, key) = replace(&mut w, "replace-c");
    assert!(key.is_some());
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    settle(&mut w);
    // A fresh profile starts with previously retained bytes. Its live reconciliation uses the actual Home.
    let mut f: Fresh = fresh(&w.net);
    f.rt.dispatch(
        &f.ctx,
        "old-sheet",
        IntentBody::BeginRecovery {
            recovery_key: old_sheet,
            home: String::new(),
            place: None,
        },
    );
    let read = f.rt.core.net_ops(&f.ctx).unwrap().into_iter()
        .find(|op| matches!(op.call, NetCall::RecoveryGet { .. })).unwrap();
    let seen = f.rt.core.net_result(&f.ctx, read.id, NetResult::Many {
        items: vec![etag.into_bytes(), old_record],
    }).unwrap();
    assert!(f.rt.settle(&seen));
    let owner = w.alice.core.debug_app_state(&w.a).unwrap();
    let replacement = owner["acsChain"].as_array().unwrap().last().unwrap().as_str().unwrap();
    let signed = om_core::crypto::unhex(replacement).unwrap();
    let body = om_core::crypto::cbor::decode(&signed).unwrap();
    let replacement_body = om_core::crypto::hex(body.get(1).unwrap().b().unwrap());
    let peer_before = w.bob.core.debug_app_state(&w.b).unwrap()["relationships"].clone();
    assert!(peer_before.as_object().unwrap().values().any(|r| r["peerAcsBody"] == replacement_body));
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
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 1));
    assert!(f.view().is_some(), "the old attempt remains restricted after reconciliation");
    let peer_after = w.bob.core.debug_app_state(&w.b).unwrap()["relationships"].clone();
    for (id, before) in peer_before.as_object().unwrap() {
        assert_eq!(peer_after[id]["peerAcsBody"], before["peerAcsBody"]);
        assert!(peer_after[id]["peerRecovery"].as_array().unwrap().is_empty());
    }
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

#[test]
fn replacement_persists_its_exact_candidate_and_authorization() {
    let mut w = world_with(true);
    let (op, key) = replace(&mut w, "bound");
    assert!(key.is_some());
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    let pending = &app["pendingRecovery"];
    assert_eq!(pending["parentEpoch"], 0);
    assert_eq!(pending["controllers"].as_array().unwrap().len(), 1);
    assert_eq!(pending["requiredVotes"], 1);
    let child_hash = pending["childHash"].as_str().unwrap().to_owned();

    w.alice.restart();
    w.a = w.alice.open("p-a");
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    let child = app["acsChain"].as_array().unwrap().last().unwrap().as_str().unwrap();
    let signed = om_core::crypto::unhex(child).unwrap();
    let body = om_core::crypto::cbor::decode(&signed).unwrap();
    let child = om_core::crypto::binding::Acs::from_body(body.get(1).unwrap().b().unwrap()).unwrap();
    assert_eq!(om_core::crypto::hex(&child.hash()), child_hash);
}

#[test]
fn replacement_retires_the_old_record_only_after_the_new_write_is_acknowledged() {
    use om_core::host::{NetCall, NetResult};

    let mut w = world_with(true);
    let (op, key) = replace(&mut w, "retire");
    let key = key.unwrap();
    w.alice.dispatch(
        &w.a,
        "confirm",
        IntentBody::ConfirmRecoveryKeyStored { key_operation_id: op },
    );
    let old_locator = w.keys.locator;
    assert!(w.net.recovery_record(&old_locator).is_some());
    let outgoing = w.alice.core.net_ops(&w.a).unwrap();
    assert!(outgoing.iter().all(|op| !matches!(op.call, NetCall::RecoveryDelete { .. })));

    let ctx = w.alice.restart().remove(0);
    w.a = ctx;
    w.alice.flush_net(&w.a);
    let keys = om_core::recovery_record::RecordKeys::derive(&root_of(&key.groups), &place().scope);
    assert!(w.net.recovery_record(&keys.locator).is_some());
    let delete = w.alice.core.net_ops(&w.a).unwrap().into_iter()
        .find(|op| matches!(op.call, NetCall::RecoveryDelete { .. })).unwrap();
    assert_eq!(w.net.perform(&delete.call), NetResult::Ok);
    assert!(w.net.recovery_record(&old_locator).is_none());

    // The Home applied the deletion but its response never reached the controller.
    w.a = w.alice.restart().remove(0);
    let retry = w.alice.core.net_ops(&w.a).unwrap().into_iter()
        .find(|op| matches!(op.call, NetCall::RecoveryDelete { .. })).unwrap();
    assert_eq!(retry.id, delete.id);
    assert_eq!(retry.call, delete.call);
    settle(&mut w);
    let app = w.alice.core.debug_app_state(&w.a).unwrap();
    assert!(app.get("retiredRecoveryRecords").is_none());
}

#[test]
fn the_saved_checkpoint_can_start_a_non_genesis_record_chain() {
    use common::recovery_world::{DELAY_MS, SCOPE};
    use om_core::recovery_record::{self as rr, Plain, RecordKeys};
    use om_core::state::EvidenceStage;

    let mut w = world_with(true);
    let (op, _) = replace(&mut w, "first");
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    let (op, key) = replace(&mut w, "second");
    let key = key.unwrap();
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));

    let keys = RecordKeys::derive(&root_of(&key.groups), &SCOPE);
    let (_, envelope) = w.net.recovery_record(&keys.locator).unwrap();
    let plain = rr::open_envelope(&keys, &SCOPE, &envelope).unwrap();
    let Plain::Record(mut record) = rr::read_plaintext(&plain).unwrap() else {
        panic!("recovery record");
    };
    record.evidence.tail.remove(0);
    let plain = record.to_padded_bytes().unwrap();
    let envelope = rr::seal_envelope(&keys, &SCOPE, [55; 12], &plain).unwrap();
    w.net.recovery_replace(&keys.locator, envelope);

    let mut f = fresh(&w.net);
    f.begin(&key.sheet.unwrap());
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::Pinned);
    assert_eq!(f.go("request", IntentBody::RequestRecovery), Some(OutcomeClass::Committed));
    f.tick(&mono(2, 0));
    f.tick(&mono(2, DELAY_MS + 1));
    assert!(f.view().is_none());
    f.rt.restart();
    f.ctx = f.rt.open("q");
    let status: RecoveryStatusView = f.rt.query(Query::GetRecoveryStatus { ctx: f.ctx.clone() });
    assert_eq!(status.record, Some(RecoveryRecordCode::NeedsNewKey));
    assert!(status.recovering.is_none());
}

#[test]
fn a_signature_before_the_saved_checkpoint_does_not_decide_recovery() {
    use common::recovery_world::SCOPE;
    use om_core::crypto::cbor::{self, V};
    use om_core::recovery_record::{self as rr, Plain, RecordKeys};
    use om_core::state::EvidenceStage;

    let mut w = world_with(true);
    for _ in 0..2 {
        let op = format!("first-{}", w.net.recovery_puts().len());
        replace(&mut w, &op);
        assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));
    }
    let (op, key) = replace(&mut w, "last");
    let key = key.unwrap();
    assert_eq!(confirm_saved(&mut w, &op), Some(OutcomeClass::Committed));

    let keys = RecordKeys::derive(&root_of(&key.groups), &SCOPE);
    let (_, envelope) = w.net.recovery_record(&keys.locator).unwrap();
    let plain = rr::open_envelope(&keys, &SCOPE, &envelope).unwrap();
    let Plain::Record(mut record) = rr::read_plaintext(&plain).unwrap() else {
        panic!("recovery record");
    };
    let V::M(mut signed) = cbor::decode(&record.evidence.tail[1]).unwrap() else {
        panic!("signed state");
    };
    let (_, V::A(signatures)) = signed.iter_mut().find(|(key, _)| *key == 2).unwrap() else {
        panic!("signature list");
    };
    let V::M(signature) = &mut signatures[0] else {
        panic!("signature entry");
    };
    let (_, V::B(bytes)) = signature.iter_mut().find(|(key, _)| *key == 3).unwrap() else {
        panic!("signature bytes");
    };
    bytes[0] ^= 1;
    record.evidence.tail[1] = cbor::encode(&V::M(signed));
    let plain = record.to_padded_bytes().unwrap();
    let envelope = rr::seal_envelope(&keys, &SCOPE, [55; 12], &plain).unwrap();
    w.net.recovery_replace(&keys.locator, envelope);

    let mut f = fresh(&w.net);
    f.begin(&key.sheet.unwrap());
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::Pinned);
}
