// SPDX-License-Identifier: MPL-2.0
//! The recovery approval ceremony with a surviving controller (ADR-0202 section 3, ADR-0203): a restricted device that
//! lost every device asks, over a purpose-bound exchange, for its exact request to be approved by Alice's surviving
//! controller. Two identities share one loopback network; Bob is Alice's contact.

mod common;

use common::confirmation as nonce;
use common::recovery_world::{DELAY_MS, Fresh, SCOPE, World, fresh, mono, world};
use om_core::api::*;
use om_core::crypto::{hex, unhex};
use om_core::runtime::{Runtime, settle_all};
use om_core::state::{AttemptState, EvidenceStage};

fn settle(w: &mut World, f: &mut Fresh) {
    settle_all(&mut [
        (&mut w.alice, &w.a),
        (&mut w.bob, &w.b),
        (&mut f.rt, &f.ctx),
    ]);
}

/// Fresh device with an attempt waiting.
fn asking(w: &World) -> Fresh {
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    f
}

fn offer(w: &mut World, op: &str) -> RecoveryApprovalOfferView {
    w.alice
        .dispatch(&w.a, op, IntentBody::OfferRecoveryApproval);
    w.alice.flush_net(&w.a);
    offer_view(w, op).expect("an offer")
}

fn offer_view(w: &World, op: &str) -> Option<RecoveryApprovalOfferView> {
    w.alice.query(Query::GetRecoveryApprovalOffer {
        ctx: w.a.clone(),
        operation_id: op.into(),
    })
}

fn join(w: &mut World, f: &mut Fresh, link: &str) -> Option<OutcomeClass> {
    let out = f.go(
        "join",
        IntentBody::JoinRecoveryApproval { link: link.into() },
    );
    settle(w, f);
    out
}

fn confirm(w: &mut World, request_id: &str, op: &str) -> Option<OutcomeClass> {
    let c = nonce(&mut w.alice, &w.a, "ConfirmRecovery", request_id);
    w.alice.dispatch(
        &w.a,
        op,
        IntentBody::ConfirmRecovery {
            request_id: request_id.into(),
            confirmation: c,
        },
    );
    w.alice.outcome(&w.a, op)
}

fn approval_of(f: &Fresh) -> Option<RecoveryApprovalView> {
    f.view().and_then(|v| v.approval)
}

#[test]
fn the_controller_approval_round_trip_adopts_with_no_waiting_and_revokes_the_old_device() {
    let mut w = world();
    let mut f = asking(&w);
    let o = offer(&mut w, "off");
    assert_eq!(o.state, ApprovalOfferCode::WaitingForDevice);
    let link = o.link.clone().expect("a link");
    assert!(link.starts_with("om:recover-approve/"));
    assert_eq!(join(&mut w, &mut f, &link), Some(OutcomeClass::Committed));
    // Both devices show the same six digits for the exact proposal; the controller names the request to confirm.
    let o = offer_view(&w, "off").unwrap();
    assert_eq!(o.state, ApprovalOfferCode::AwaitingConfirmation);
    let a = approval_of(&f).expect("the exchange");
    assert_eq!(a.state, ApprovalCode::WaitingForController);
    assert_eq!(a.verification_code, o.verification_code);
    assert_eq!(a.verification_code.as_ref().map(String::len), Some(6));
    // Nothing is adopted before the person confirms.
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    let request_id = o.request_id.clone().expect("the request");
    assert_eq!(
        confirm(&mut w, &request_id, "c1"),
        Some(OutcomeClass::Committed)
    );
    settle(&mut w, &mut f);
    for _ in 0..3 {
        f.rt.flush_net(&f.ctx);
        settle(&mut w, &mut f);
    }
    // No interval was run: the controller's co-signature completed it.
    assert!(f.view().is_none(), "the restricted state is gone");
    let app = f.app();
    assert_eq!(app["acsChain"].as_array().unwrap().len(), 2);
    let h = |k: &str| -> Vec<Vec<u8>> {
        app[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect()
    };
    let links = app["recoveryLinks"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    let ev: Vec<om_core::control::RecoveryLinkEvidence> = links
        .iter()
        .map(|l| {
            let g = |k: &str| unhex(l[k].as_str().unwrap()).unwrap();
            om_core::control::RecoveryLinkEvidence {
                child_hash: g("childHash").try_into().unwrap(),
                policy: g("policy"),
                next_policy: g("nextPolicy"),
                request: g("request"),
                elapsed_seconds: l["elapsedSeconds"].as_u64().unwrap(),
            }
        })
        .collect();
    let head =
        om_core::control::verify_chain_with(&h("acsChain"), &h("acsCredentials"), &ev).unwrap();
    assert_eq!(head.control.len(), 1);
    assert_eq!(head.revoked.len(), 1, "Alice's old device is revoked");
    assert_eq!(
        ev[0].elapsed_seconds, 0,
        "a controller's approval carries no verifier-local time"
    );
    assert_eq!(
        offer_view(&w, "off").unwrap().state,
        ApprovalOfferCode::Approved
    );
    // The ceremony wiped its secret; no queue message carries the recovery root or a key.
    let applog = w.alice.core.debug_app_state(&w.a).unwrap();
    assert_eq!(applog["approvalOffers"]["off"]["secret"], "");
}

#[test]
fn an_enrollment_link_is_never_an_approval_link_nor_the_reverse() {
    let mut w = world();
    let mut f = asking(&w);
    // An enrollment offer from Alice, handed to the recovery join.
    w.alice.dispatch(&w.a, "enr", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let enroll: Option<LinkOfferView> = w.alice.query(Query::GetLinkOffer {
        ctx: w.a.clone(),
        operation_id: "enr".into(),
    });
    let enroll = enroll.unwrap().link;
    assert_eq!(
        f.go("j1", IntentBody::JoinRecoveryApproval { link: enroll }),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    // And an approval link given to the enrollment entry point.
    let link = offer(&mut w, "off").link.unwrap();
    assert_eq!(
        f.go("j2", IntentBody::RequestDeviceLink { link: link.clone() }),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    // A newer version of the approval link is refused rather than read as version 1.
    let o = om_core::crypto::recovery_approval::decode_offer(&link).unwrap();
    let tampered = {
        let mut b = vec![2u8];
        b.extend_from_slice(&o.expiry_s.to_be_bytes());
        b.extend_from_slice(&o.secret);
        b.push(o.a_cap.len() as u8);
        b.extend_from_slice(&o.a_cap);
        b.push(o.b_recv.len() as u8);
        b.extend_from_slice(&o.b_recv);
        b.extend_from_slice(o.uri.as_bytes());
        format!("om:recover-approve/{}", b64url(&b))
    };
    assert_eq!(
        f.go("j3", IntentBody::JoinRecoveryApproval { link: tampered }),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unsupported
        })
    );
    assert!(approval_of(&f).is_none(), "nothing was started");
}

#[test]
fn a_stale_parent_is_answered_with_evidence_and_ends_the_request_without_approving() {
    let mut w = world();
    let mut f = asking(&w);
    enroll_phone(&mut w);
    let o = offer(&mut w, "off");
    join(&mut w, &mut f, &o.link.unwrap());
    // The controller did not ask the person anything and made no signature.
    assert_eq!(
        offer_view(&w, "off").unwrap().state,
        ApprovalOfferCode::Declined
    );
    assert!(
        w.alice.core.debug_app_state(&w.a).unwrap()["recoveryRequests"]
            .as_object()
            .is_none_or(|m| m.is_empty())
    );
    let v = f.view().expect("still restricted");
    assert_eq!(v.attempt, Some(AttemptState::Stale));
    assert!(f.app()["acsChain"].as_array().unwrap().is_empty());
}

fn enroll_phone(w: &mut World) {
    let mut phone = Runtime::new("phone", &w.net);
    let n = phone.open("p-n");
    w.alice.dispatch(&w.a, "enr", IntentBody::OfferDeviceLink);
    w.alice.flush_net(&w.a);
    let link = w
        .alice
        .query::<Option<LinkOfferView>>(Query::GetLinkOffer {
            ctx: w.a.clone(),
            operation_id: "enr".into(),
        })
        .unwrap()
        .link;
    phone.dispatch(&n, "ask", IntentBody::RequestDeviceLink { link });
    settle_all(&mut [(&mut w.alice, &w.a), (&mut phone, &n)]);
    let pending: DevicesView = w.alice.query(Query::GetDevices { ctx: w.a.clone() });
    let rid = pending.link_requests[0].request_id.clone();
    let c = nonce(&mut w.alice, &w.a, "ApproveDeviceLink", &rid);
    w.alice.dispatch(
        &w.a,
        "ok",
        IntentBody::ApproveDeviceLink {
            request_id: rid,
            confirmation: c,
        },
    );
    settle_all(&mut [(&mut w.alice, &w.a), (&mut phone, &n)]);
}

#[test]
fn approval_preserves_the_independent_checkpoint_when_the_record_tail_is_short() {
    use om_core::recovery_record::{self as rr, Plain};
    use om_core::recovery_sheet::{Parsed, Sheet, parse};

    let mut w = world();
    enroll_phone(&mut w);
    let (_, envelope) = w.net.recovery_record(&w.keys.locator).unwrap();
    let plain = rr::open_envelope(&w.keys, &SCOPE, &envelope).unwrap();
    let Plain::Record(mut record) = rr::read_plaintext(&plain).unwrap() else {
        panic!("record")
    };
    let info = record.verify().unwrap();
    assert!(info.head.generation > 1);
    record.evidence.tail.clear();
    record.evidence.credentials.retain(|bytes| {
        let credential = om_core::crypto::binding::Credential::from_body(bytes).unwrap();
        info.head
            .credentials
            .contains(&(credential.device_id, credential.hash()))
    });
    let envelope =
        rr::seal_envelope(&w.keys, &SCOPE, [6; 12], &record.to_padded_bytes().unwrap()).unwrap();
    w.net.recovery_replace(&w.keys.locator, envelope);
    let Parsed::Sheet(sheet) = parse(&w.sheet).unwrap() else {
        panic!("sheet")
    };
    let mut pin = sheet.checkpoint.clone().unwrap();
    pin.acs_hash = [9; 32];
    let different_pin =
        Sheet::full_v2(*sheet.root().unwrap(), SCOPE, sheet.port, &sheet.host, pin).unwrap();
    let mut f = fresh(&w.net);
    f.begin(&different_pin.to_compact());
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::NeedsEvidence);
    common::recovery_world::adopt_via_approval(&mut w, &mut f);
    let view = f.view().expect("unresolved checkpoint");
    assert_eq!(view.evidence, EvidenceStage::NeedsEvidence);
    assert_eq!(view.approval.unwrap().state, ApprovalCode::Failed);
}

#[test]
fn approval_does_not_transfer_elapsed_time_for_an_earlier_recovery() {
    use om_core::recovery_record::{self as rr, Plain};

    let mut w = world();
    let mut controller = asking(&w);
    controller.tick(&mono(1, 0));
    controller.tick(&mono(1, DELAY_MS));
    assert!(controller.view().is_none());
    let (_, envelope) = w.net.recovery_record(&w.keys.locator).unwrap();
    let plain = rr::open_envelope(&w.keys, &SCOPE, &envelope).unwrap();
    let Plain::Record(mut record) = rr::read_plaintext(&plain).unwrap() else {
        panic!("record")
    };
    let app = controller.app();
    let bytes = |key: &str| {
        app[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect::<Vec<_>>()
    };
    record.evidence.tail = bytes("acsChain");
    record.evidence.head = record.evidence.tail.pop().unwrap();
    record.evidence.credentials = bytes("acsCredentials");
    record.evidence.policy = unhex(app["recoveryPolicy"].as_str().unwrap()).unwrap();
    let signed = om_core::crypto::cbor::decode(&record.evidence.head).unwrap();
    let head =
        om_core::crypto::binding::Acs::from_body(signed.get(1).unwrap().b().unwrap()).unwrap();
    record.head.generation = head.generation;
    record.head.state_hash = head.hash();
    record.generation = head.generation << 20;
    record.recovery_epoch += 1;
    let envelope =
        rr::seal_envelope(&w.keys, &SCOPE, [6; 12], &record.to_padded_bytes().unwrap()).unwrap();
    w.net.recovery_replace(&w.keys.locator, envelope);
    let mut second = fresh(&w.net);
    // Use a distinct device and profile for the next recovery target.
    second.rt = Runtime::new("next recovery device", &w.net);
    second.ctx = second.rt.open("q");
    second.begin(&w.sheet);
    assert_eq!(
        second.view().unwrap().evidence,
        EvidenceStage::NeedsEvidence
    );
    w.alice = controller.rt;
    w.a = controller.ctx;
    common::recovery_world::adopt_via_approval(&mut w, &mut second);
    let view = second.view().expect("verifier-local history required");
    assert_eq!(view.evidence, EvidenceStage::NeedsEvidence);
    assert_eq!(view.approval.unwrap().state, ApprovalCode::Failed);
}

#[test]
fn an_expired_ceremony_cannot_advance_recovery_and_the_request_stays_as_it_was() {
    let mut w = world();
    let mut f = asking(&w);
    let before = f.app()["recoveryJoin"]["attempt"]["obs"]["request"].clone();
    let o = offer(&mut w, "off");
    join(&mut w, &mut f, &o.link.unwrap());
    let request_id = offer_view(&w, "off").unwrap().request_id.unwrap();
    // The person confirms in time; the reply waits on the Home, and the restricted device only looks after the end.
    assert_eq!(
        confirm(&mut w, &request_id, "c1"),
        Some(OutcomeClass::Committed)
    );
    w.alice.flush_net(&w.a);
    f.rt.tick(5 * 60 * 1000 + 1);
    settle(&mut w, &mut f);
    assert!(f.app()["acsChain"].as_array().unwrap().is_empty());
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    assert_eq!(approval_of(&f).unwrap().state, ApprovalCode::Expired);
    assert_eq!(
        f.app()["recoveryJoin"]["attempt"]["obs"]["request"],
        before,
        "the exact signed request is untouched"
    );
    // A link past its time is not even started.
    let mut w = world();
    let mut g = asking(&w);
    let stale = o_link_after_expiry(&mut w);
    g.rt.tick(6 * 60 * 1000);
    assert_eq!(
        g.go("late", IntentBody::JoinRecoveryApproval { link: stale }),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Expired
        })
    );
}

fn o_link_after_expiry(w: &mut World) -> String {
    let l = offer(w, "off2").link.unwrap();
    w.alice.tick(6 * 60 * 1000);
    l
}

#[test]
fn the_ceremony_survives_a_restart_on_both_devices() {
    let mut w = world();
    let mut f = asking(&w);
    let o = offer(&mut w, "off");
    join(&mut w, &mut f, &o.link.unwrap());
    // Both sides restart in the middle of the exchange: the proposal, the offer and the code are persisted.
    f.rt.restart();
    f.ctx = f.rt.open("q");
    w.alice.restart();
    w.a = w.alice.open("p-a");
    let o = offer_view(&w, "off").expect("the offer survived");
    assert_eq!(o.state, ApprovalOfferCode::AwaitingConfirmation);
    let request_id = o.request_id.unwrap();
    assert_eq!(
        confirm(&mut w, &request_id, "c1"),
        Some(OutcomeClass::Committed)
    );
    for _ in 0..4 {
        settle(&mut w, &mut f);
        f.rt.flush_net(&f.ctx);
    }
    assert!(f.view().is_none(), "adopted after both restarts");
}

#[test]
fn a_replayed_reply_changes_nothing() {
    let mut w = world();
    let mut f = asking(&w);
    let o = offer(&mut w, "off");
    join(&mut w, &mut f, &o.link.unwrap());
    let request_id = offer_view(&w, "off").unwrap().request_id.unwrap();
    confirm(&mut w, &request_id, "c1");
    for _ in 0..4 {
        settle(&mut w, &mut f);
        f.rt.flush_net(&f.ctx);
    }
    assert!(f.view().is_none());
    // Confirming the same request again does nothing more: Alice's device was revoked by the recovery itself.
    assert!(matches!(
        confirm(&mut w, &request_id, "c2"),
        Some(OutcomeClass::Rejected { .. })
    ));
    assert_eq!(f.app()["acsChain"].as_array().unwrap().len(), 2);
    let _ = (hex(&[]), EvidenceStage::Pinned, mono(1, DELAY_MS));
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for ch in b.chunks(3) {
        let n = (ch[0] as u32) << 16
            | (*ch.get(1).unwrap_or(&0) as u32) << 8
            | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..=ch.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// OM4-39 / ADR-0226 section 5: when the approval reply (the manifest) is lost and its chunks arrive, the restricted device
/// buffers nothing and adopts nothing; the surviving controller repeats the whole sequence, manifest first, and it completes.
#[test]
fn a_reply_whose_manifest_was_lost_is_repeated_by_the_sender_and_completes() {
    let mut w = world();
    let mut f = asking(&w);
    let o = offer(&mut w, "off");
    join(&mut w, &mut f, &o.link.unwrap());
    let request_id = offer_view(&w, "off").unwrap().request_id.unwrap();
    let before = w.net.deposit_log().len();
    assert_eq!(
        confirm(&mut w, &request_id, "c1"),
        Some(OutcomeClass::Committed)
    );
    w.alice.flush_net(&w.a);
    // the reply and its chunks go to one queue of the restricted device: the last deposit names it
    let mailbox = w.net.deposit_log().last().expect("a deposit").0.clone();
    let net = w.net.clone();
    let to_device = |from: usize| -> Vec<Vec<u8>> {
        net.deposit_log()[from..]
            .iter()
            .filter(|(m, _)| *m == mailbox)
            .map(|(_, b)| b.clone())
            .collect()
    };
    let sent = to_device(before);
    assert!(sent.len() >= 2, "a manifest and at least one chunk");
    let manifest = sent[0].clone();
    // the Home loses the manifest; the restricted device sees only the chunks
    assert!(w.net.lose_queued(&mailbox, &manifest));
    for _ in 0..3 {
        settle(&mut w, &mut f);
        f.rt.flush_net(&f.ctx);
    }
    assert!(f.app()["acsChain"].as_array().unwrap().is_empty(), "nothing adopted");
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    let join = f.app()["recoveryJoin"]["attempt"]["approval"].clone();
    assert!(join.is_object(), "the exchange is still open");
    assert!(join["descriptor"].is_null(), "no manifest yet");
    assert!(
        join["chunks"].as_array().is_none_or(|c| c.iter().all(|x| x.is_null())),
        "nothing is buffered before the manifest"
    );
    // the controller's device repeats the whole sequence after one fifth of the exchange's life
    w.alice.tick(2 * 60 * 1000);
    w.alice.flush_net(&w.a);
    for _ in 0..4 {
        settle(&mut w, &mut f);
        f.rt.flush_net(&f.ctx);
    }
    let repeats: Vec<Vec<u8>> = to_device(before).into_iter().skip(sent.len()).collect();
    assert_eq!(repeats, sent, "bit-identical, manifest first");
    assert!(f.view().is_none(), "adopted from the repeat");
    assert_eq!(f.app()["acsChain"].as_array().unwrap().len(), 2);
}
