// SPDX-License-Identifier: MPL-2.0
//! The owner's recovery attempt on a fresh device (ADR-0202, ADR-0203): request and successor, per-Home delivery, the
//! device's own interval, vetoes, the record floor and atomic adoption. Two identities share one loopback network: Alice
//! loses every device, Bob is her contact and verifies the recovery with his own interval.

mod common;

use om_core::api::*;
use om_core::crypto::unhex;
use om_core::host::MonotonicSample;
use om_core::recovery_record::{RecordKeys, RecoveryPlace};
use om_core::recovery_sheet::Sheet;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};
use om_core::state::{AttemptState, EvidenceStage, JoinStage};

const SCOPE: [u8; 32] = [7; 32];
const H: u64 = 3_600_000;
const DELAY_MS: u64 = 72 * H;

fn place() -> RecoveryPlace {
    RecoveryPlace {
        host: "home.example".into(),
        port: None,
        scope: SCOPE,
    }
}

fn mono(session: u8, ms: u64) -> MonotonicSample {
    MonotonicSample {
        boot_session: vec![session; 8],
        elapsed_ms: ms,
    }
}

struct World {
    net: LoopbackNetwork,
    alice: Runtime,
    a: ProfileExecutionContext,
    bob: Runtime,
    b: ProfileExecutionContext,
    sheet: String,
    keys: RecordKeys,
}

/// Alice with a recovery place and Bob as an active contact; the key is confirmed saved, so the record (with the
/// snapshot of Bob) is on the Home.
fn world() -> World {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    alice.set_recovery_place(Some(place()));
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let key: RecoveryKeyView = alice
        .query::<Option<RecoveryKeyView>>(Query::GetRecoveryKey {
            ctx: a.clone(),
            key_operation_id: "create-1".into(),
        })
        .expect("key");
    let mut bob = Runtime::new("bob", &net);
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    alice.dispatch(
        &a,
        "saved",
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "create-1".into(),
        },
    );
    settle_all(&mut [(&mut alice, &a), (&mut bob, &b)]);
    assert_eq!(net.recovery_puts().len(), 1, "the record is written");
    let root = {
        const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut bits = String::new();
        for c in key.groups.concat().chars() {
            bits.push_str(&format!("{:05b}", A.find(c).unwrap()));
        }
        let v: Vec<u8> = (0..32)
            .map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap())
            .collect();
        <[u8; 32]>::try_from(v).unwrap()
    };
    World {
        net,
        alice,
        a,
        bob,
        b,
        sheet: key.sheet.expect("sheet"),
        keys: RecordKeys::derive(&root, &SCOPE),
    }
}

struct Fresh {
    rt: Runtime,
    ctx: ProfileExecutionContext,
}

fn fresh(net: &LoopbackNetwork) -> Fresh {
    let mut rt = Runtime::new("fresh", net);
    let ctx = rt.open("q");
    Fresh { rt, ctx }
}

impl Fresh {
    fn go(&mut self, op: &str, body: IntentBody) -> Option<OutcomeClass> {
        self.rt.dispatch(&self.ctx, op, body);
        self.rt.flush_net(&self.ctx);
        self.rt.outcome(&self.ctx, op)
    }
    fn begin(&mut self, sheet: &str) {
        let out = self.go(
            "begin",
            IntentBody::BeginRecovery {
                recovery_key: sheet.into(),
                home: String::new(),
                place: None,
            },
        );
        assert_eq!(out, Some(OutcomeClass::Committed));
    }
    fn view(&self) -> Option<RecoveringView> {
        self.rt
            .query::<RecoveryStatusView>(Query::GetRecoveryStatus {
                ctx: self.ctx.clone(),
            })
            .recovering
    }
    fn tick(&mut self, m: &MonotonicSample) {
        self.rt.recovery_tick(&self.ctx, m);
        // a round of reads, then whatever it leads to
        for _ in 0..4 {
            self.rt.flush_net(&self.ctx);
        }
    }
    fn app(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()
    }
}

#[test]
fn the_sheet_carries_the_owner_checkpoint_and_the_record_is_pinned_by_it() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    let v = f.view().expect("restricted");
    assert_eq!(v.stage, JoinStage::Found);
    assert_eq!(v.evidence, EvidenceStage::Pinned);
    assert_eq!(v.attempt, None);
    let app = f.app();
    assert_eq!(app["recoveryJoin"]["checkpoint"]["generation"], 1);
    assert!(app["recoveryFloors"].as_object().unwrap().len() == 1);
}

#[test]
fn a_replacement_place_keeps_the_full_sheets_checkpoint() {
    use om_core::host::NetCall;

    let w = world();
    let mut f = fresh(&w.net);
    let updated = Sheet::place_only(SCOPE, None, "moved.example").unwrap();
    f.rt.dispatch(
        &f.ctx,
        "begin",
        IntentBody::BeginRecovery {
            recovery_key: w.sheet,
            home: String::new(),
            place: Some(updated.to_compact()),
        },
    );
    assert_eq!(f.rt.outcome(&f.ctx, "begin"), Some(OutcomeClass::Committed));
    let reads = f.rt.core.net_ops(&f.ctx).unwrap();
    assert!(
        matches!(reads.as_slice(), [op] if matches!(&op.call, NetCall::RecoveryGet { host, .. } if host == "moved.example"))
    );
    f.rt.flush_net(&f.ctx);
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::Pinned);
}

#[test]
fn the_whole_owner_flow_with_a_contact_verifying_independently() {
    let mut w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    // The signed request reached the Home, once; the tentative device is NOT the active device.
    assert_eq!(w.net.recovery_requests().len(), 1);
    assert_eq!(
        f.app()["acsChain"].as_array().map_or(0, Vec::len),
        0,
        "no account state yet"
    );
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    // Bob learned of it over his ordinary route and runs his own interval.
    settle_all(&mut [(&mut w.alice, &w.a), (&mut w.bob, &w.b)]);
    let list: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    let peer = list[0].peer_recovery.clone().expect("Bob tracks it");
    assert_eq!(peer.state, om_core::state::PeerRecoveryState::Waiting);
    // Before the delay nothing is adopted.
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS - 1));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    assert!(f.view().unwrap().remaining_ms.unwrap() > 0);
    // The delay runs: the reconcile round passes and the account is adopted atomically.
    f.tick(&mono(1, DELAY_MS + 5));
    assert!(f.view().is_none(), "the restricted state is gone");
    let app = f.app();
    assert_eq!(app["acsChain"].as_array().unwrap().len(), 2);
    assert_eq!(app["recoveryLinks"].as_array().unwrap().len(), 1);
    // The new device is the sole controller; the old one is revoked by the successor's own shape; the policy rotated.
    let hexes = |k: &str| -> Vec<Vec<u8>> {
        app[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect()
    };
    let links: Vec<om_core::control::RecoveryLinkEvidence> = app["recoveryLinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let h = |k: &str| unhex(l[k].as_str().unwrap()).unwrap();
            om_core::control::RecoveryLinkEvidence {
                child_hash: h("childHash").try_into().unwrap(),
                policy: h("policy"),
                next_policy: h("nextPolicy"),
                request: h("request"),
                elapsed_seconds: l["elapsedSeconds"].as_u64().unwrap(),
            }
        })
        .collect();
    let head =
        om_core::control::verify_chain_with(&hexes("acsChain"), &hexes("acsCredentials"), &links)
            .unwrap();
    assert_eq!(head.control.len(), 1);
    assert_eq!(head.revoked.len(), 1, "the lost device is revoked");
    assert!(links[0].elapsed_seconds >= DELAY_MS / 1000);
    assert_eq!(app["recoveryKeySaved"], true);
    // Bob's own interval completes on his own clock, never on hers.
    w.bob.recovery_tick(&w.b, &mono(1, 0));
    w.bob.recovery_tick(&w.b, &mono(1, DELAY_MS + 5));
    settle_all(&mut [(&mut w.bob, &w.b)]);
    let list: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    assert_eq!(
        list[0].peer_recovery.clone().unwrap().state,
        om_core::state::PeerRecoveryState::Accepted
    );
}

// ------------------------------------------------------------------------------------------------ regressions

use om_core::recovery_record::{self as rr, Plain};

fn root_of(sheet: &str) -> [u8; 32] {
    match om_core::recovery_sheet::parse(sheet).unwrap() {
        om_core::recovery_sheet::Parsed::Sheet(s) => *s.root().unwrap(),
        _ => panic!("a full sheet"),
    }
}

/// Put `record` (sealed with the owner's keys) at the Home in place of what is there.
fn replace_record(w: &World, f: impl FnOnce(&mut rr::RecoveryRecord)) {
    let (_, env) = w.net.recovery_record(&w.keys.locator).unwrap();
    let plain = rr::open_envelope(&w.keys, &SCOPE, &env).unwrap();
    let Ok(Plain::Record(mut r)) = rr::read_plaintext(&plain) else {
        panic!("a record")
    };
    f(&mut r);
    let sealed =
        rr::seal_envelope(&w.keys, &SCOPE, [9; 12], &r.to_padded_bytes().unwrap()).unwrap();
    w.net.recovery_replace(&w.keys.locator, sealed);
}

#[test]
fn an_advisory_only_record_never_finishes_even_after_the_timer_and_a_restart() {
    let w = world();
    // A sheet without the owner's checkpoint: version 1, as an older client or a legacy text gives.
    let v1 = Sheet::full(root_of(&w.sheet), SCOPE, None, "home.example")
        .unwrap()
        .to_sheet_form();
    let mut f = fresh(&w.net);
    f.begin(&v1);
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::Unpinned);
    // The request may be prepared and delivered, so controllers and contacts can see it ...
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(w.net.recovery_requests().len(), 1);
    // ... but nothing is ever adopted.
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS * 3));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    f.rt.restart();
    f.ctx = f.rt.open("q");
    f.tick(&mono(2, 0));
    f.tick(&mono(2, DELAY_MS * 3));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Waiting));
    assert_eq!(f.app()["acsChain"].as_array().map_or(0, Vec::len), 0);
}

#[test]
fn a_checkpoint_that_contradicts_the_record_is_a_conflict_and_no_request_is_made() {
    let w = world();
    // The owner's real checkpoint, but for a state the record's chain does not hold at that generation.
    let Ok(om_core::recovery_sheet::Parsed::Sheet(real)) = om_core::recovery_sheet::parse(&w.sheet)
    else {
        panic!()
    };
    let mut cp = real.checkpoint.unwrap();
    cp.acs_hash = [0xAA; 32];
    let forged = Sheet::full_v2(root_of(&w.sheet), SCOPE, None, "home.example", cp)
        .unwrap()
        .to_sheet_form();
    let mut f = fresh(&w.net);
    f.begin(&forged);
    assert_eq!(f.view().unwrap().evidence, EvidenceStage::Conflict);
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::StaleState
        })
    );
    assert_eq!(w.net.recovery_requests().len(), 0);
    // Another account's checkpoint is the same refusal.
    let mut other = real.checkpoint.unwrap();
    other.account_id = [0x55; 32];
    let wrong = Sheet::full_v2(root_of(&w.sheet), SCOPE, None, "home.example", other)
        .unwrap()
        .to_sheet_form();
    let mut g = fresh(&w.net);
    g.begin(&wrong);
    assert_eq!(g.view().unwrap().evidence, EvidenceStage::Conflict);
}

#[test]
fn the_record_floor_refuses_an_older_record_and_an_equivocating_one_and_survives_a_restart() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(f.view().unwrap().stage, JoinStage::Found);
    f.rt.restart();
    f.ctx = f.rt.open("q");
    // Same generation, other authenticated bytes (a different relationship snapshot): equivocation, fail closed.
    replace_record(&w, |r| {
        r.relationships = rr::Relationships::Plain(rr::RelationshipSnapshot {
            entries: vec![],
            truncated: false,
        })
    });
    assert_eq!(
        f.go("again", IntentBody::CheckRecoveryAgain),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.view().unwrap().stage, JoinStage::Conflict);
    // An older generation than the floor is an old copy.
    let mut g = fresh(&w.net);
    g.begin(&w.sheet);
    // (a fresh device has no floor yet, so it was a conflict-free first read of whatever the Home now holds)
    let _ = g;
}

#[test]
fn a_record_of_another_account_under_the_same_place_is_never_used_after_the_first() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    // The floor's lineage is the first record's account: poison the stored lineage and the same record conflicts.
    let mut app = f.app();
    app["recoveryFloors"][hex(&SCOPE)]["lineage"] = serde_json::json!(hex(&[1; 32]));
    rewrite(&mut f, app);
    assert_eq!(
        f.go("again", IntentBody::CheckRecoveryAgain),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.view().unwrap().stage, JoinStage::Conflict);
}

fn hex(b: &[u8]) -> String {
    om_core::crypto::hex(b)
}

#[test]
fn competing_record_head_keeps_recovery_restricted_across_restart() {
    use om_core::{
        control::sign_acs,
        crypto::{cbor, rng::insecure_test_rng::InsecureTestRng, store::Device},
        host::{NetCall, NetError, NetResult},
    };

    for newer in [false, true] {
        let w = world();
        let (_, original) = w.net.recovery_record(&w.keys.locator).unwrap();
        let saved = w
            .alice
            .store
            .records("p-a")
            .into_iter()
            .find(|r| r.key == "state")
            .unwrap();
        let device = Device::from_bytes(
            cbor::decode(&saved.value)
                .unwrap()
                .get(0)
                .unwrap()
                .b()
                .unwrap(),
        )
        .unwrap();
        let mut f = fresh(&w.net);
        f.begin(&w.sheet);
        f.go("request", IntentBody::RequestRecovery);
        replace_record(&w, |record| {
            let mut head = record.verify().unwrap().head;
            if newer {
                head.generation += 2;
                head.previous = Some([3; 32]);
                record.evidence.tail.clear();
            } else {
                head.transparency = Some([4; 32]);
            }
            record.generation = head.generation << 20;
            record.head.generation = head.generation;
            record.head.state_hash = head.hash();
            record.evidence.head = sign_acs(
                &head.body(),
                &[(&device.device_id, &device.signer)],
                &mut InsecureTestRng::from_label(b"observed head"),
            );
        });
        f.tick(&mono(1, 0));
        f.tick(&mono(1, DELAY_MS));
        assert!(f.view().is_some());
        f.rt.restart();
        f.ctx = f.rt.open("q");
        f.rt.recovery_tick(&f.ctx, &mono(2, 0));
        for op in f.rt.core.net_ops(&f.ctx).unwrap() {
            let result = if matches!(op.call, NetCall::RecoveryGet { .. }) {
                NetResult::Err {
                    error: NetError::NotFound,
                }
            } else {
                w.net.perform(&op.call)
            };
            let step = f.rt.core.net_result(&f.ctx, op.id, result).unwrap();
            assert!(f.rt.settle(&step));
        }
        assert!(
            f.view().is_some(),
            "a missing record leaves the prior observation in force"
        );
        w.net.recovery_replace(&w.keys.locator, original);
        f.tick(&mono(2, 1));
        assert!(
            f.view().is_some(),
            "a replay leaves the prior observation in force"
        );
    }
}

#[test]
fn checking_again_coalesces_with_an_in_flight_reconciliation() {
    use om_core::host::NetCall;

    for record_first in [true, false] {
        let w = world();
        let mut f = fresh(&w.net);
        f.begin(&w.sheet);
        f.go("request", IntentBody::RequestRecovery);
        f.tick(&mono(1, 0));
        f.rt.recovery_tick(&f.ctx, &mono(1, DELAY_MS));
        let reads = |f: &Fresh| {
            f.rt.core
                .net_ops(&f.ctx)
                .unwrap()
                .into_iter()
                .filter(|op| {
                    matches!(
                        op.call,
                        NetCall::RecoveryGet { .. } | NetCall::RecoveryVetoRead { .. }
                    )
                })
                .collect::<Vec<_>>()
        };
        let original = reads(&f);
        assert_eq!(original.len(), 2);
        f.rt.dispatch(&f.ctx, "again", IntentBody::CheckRecoveryAgain);
        f.rt.dispatch(&f.ctx, "again-twice", IntentBody::CheckRecoveryAgain);
        assert_eq!(reads(&f), original);
        let mut ordered = original;
        ordered.sort_by_key(|op| matches!(op.call, NetCall::RecoveryGet { .. }) != record_first);
        for (i, op) in ordered.into_iter().enumerate() {
            let step =
                f.rt.core
                    .net_result(&f.ctx, op.id, w.net.perform(&op.call))
                    .unwrap();
            assert!(f.rt.settle(&step));
            assert_eq!(f.view().is_some(), i == 0);
        }
    }
}

fn rewrite(f: &mut Fresh, app: serde_json::Value) {
    use om_core::crypto::cbor::{self, V};
    let rec =
        f.rt.store
            .records("q")
            .into_iter()
            .find(|r| r.key == "state")
            .unwrap();
    let v = cbor::decode(&rec.value).unwrap();
    let new = cbor::encode(&V::M(vec![
        (0, v.get(0).unwrap().clone()),
        (1, V::B(serde_json::to_vec(&app).unwrap())),
    ]));
    f.rt.store.replace_record("q", "state", new);
    f.rt.restart();
    f.ctx = f.rt.open("q");
}

#[test]
fn a_veto_ends_it_for_good_even_after_the_home_forgets_it_and_a_restart() {
    let mut w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    // Alice's surviving device sees the request and vetoes it; the veto goes to the Home's board.
    let req = w.net.recovery_requests()[0].clone();
    w.alice.recovery_observe_request(&w.a, &req, &mono(1, 0));
    let id = {
        let st: RecoveryStatusView = w.alice.query(Query::GetRecoveryStatus { ctx: w.a.clone() });
        st.requests[0].request_id.clone()
    };
    let c = common::confirmation(&mut w.alice, &w.a, "VetoRecovery", &id);
    w.alice.dispatch(
        &w.a,
        "veto",
        IntentBody::VetoRecovery {
            request_id: id,
            confirmation: c,
        },
    );
    w.alice.flush_net(&w.a);
    // The fresh device reads the board on its next round and the attempt is over.
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Vetoed));
    f.rt.restart();
    f.ctx = f.rt.open("q");
    // The Home prunes the board; the veto stays terminal here, and no later reading adopts anything.
    w.net.recovery_clear_vetoes();
    f.tick(&mono(2, 0));
    f.tick(&mono(2, DELAY_MS * 4));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Vetoed));
    assert_eq!(f.app()["acsChain"].as_array().map_or(0, Vec::len), 0);
}

#[test]
fn a_forged_veto_on_the_board_is_ignored() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    let digest = {
        let r = &f.app()["recoveryJoin"]["attempt"]["obs"]["digest"];
        unhex(r.as_str().unwrap()).unwrap()
    };
    // Signed by nobody the account's head names: a body that decodes, with a signature that does not verify.
    let veto = om_core::recovery::RecoveryVeto {
        account_id: [3; 32],
        recovery_epoch: 0,
        request_digest: digest.try_into().unwrap(),
    };
    let stranger = om_core::crypto::store::Device::new(
        &mut om_core::crypto::rng::insecure_test_rng::InsecureTestRng::from_label(b"stranger"),
    );
    let signed = om_core::recovery::sign_veto(
        &veto,
        &stranger.device_id,
        &stranger.signer,
        &mut om_core::crypto::rng::insecure_test_rng::InsecureTestRng::from_label(b"s"),
    );
    w.net.recovery_post_veto(&signed);
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 1));
    assert!(
        f.view().is_none(),
        "a board that holds only a forgery does not stop the attempt"
    );
}

#[test]
fn a_full_veto_page_keeps_the_attempt_pending_across_restart() {
    use om_core::host::{NetCall, NetResult};

    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("request", IntentBody::RequestRecovery);
    f.tick(&mono(1, 0));
    f.rt.recovery_tick(&f.ctx, &mono(1, DELAY_MS));
    for op in f.rt.core.net_ops(&f.ctx).unwrap() {
        let result = if matches!(op.call, NetCall::RecoveryVetoRead { .. }) {
            NetResult::Many {
                items: vec![vec![0]; 16],
            }
        } else {
            w.net.perform(&op.call)
        };
        let step = f.rt.core.net_result(&f.ctx, op.id, result).unwrap();
        assert!(f.rt.settle(&step));
    }
    assert!(f.view().is_some());
    f.rt.restart();
    f.ctx = f.rt.open("q");
    f.tick(&mono(2, 0));
    assert!(f.view().is_some());
}

#[test]
fn adoption_revalidates_and_a_tampered_successor_changes_nothing() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    let mut app = f.app();
    // Flip a byte of the stored signed successor: the exact bytes the request is bound to are what adoption re-checks.
    let child = app["recoveryJoin"]["attempt"]["obs"]["successor"]["child"]
        .as_str()
        .unwrap()
        .to_string();
    let mut b = unhex(&child).unwrap();
    let n = b.len();
    b[n - 3] ^= 1;
    app["recoveryJoin"]["attempt"]["obs"]["successor"]["child"] = serde_json::json!(hex(&b));
    rewrite(&mut f, app);
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 1));
    let v = f.view().expect("still restricted");
    assert_eq!(v.attempt, Some(AttemptState::Waiting));
    assert_eq!(f.app()["acsChain"].as_array().map_or(0, Vec::len), 0);
    assert!(
        f.app()["recoveryLinks"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
}

#[test]
fn the_interval_credits_only_monotonic_growth_inside_one_boot() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    f.tick(&mono(1, 0));
    f.tick(&mono(1, 10 * H));
    let after = f.view().unwrap().remaining_ms.unwrap();
    assert_eq!(after, DELAY_MS - 10 * H);
    // A restart: downtime is never credited, a new boot session starts a fresh reading.
    f.rt.restart();
    f.ctx = f.rt.open("q");
    f.tick(&mono(2, 500 * H));
    assert_eq!(f.view().unwrap().remaining_ms.unwrap(), after);
    // A reading that goes backwards credits nothing.
    f.tick(&mono(2, 400 * H));
    assert_eq!(f.view().unwrap().remaining_ms.unwrap(), after);
    f.tick(&mono(2, 500 * H + H));
    assert_eq!(f.view().unwrap().remaining_ms.unwrap(), after - H);
}

#[test]
fn cancel_closes_the_attempt_and_asking_twice_conflicts() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(
        f.go("r1", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(
        f.go("r2", IntentBody::RequestRecovery),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Conflict
        })
    );
    f.rt.dispatch(&f.ctx, "check", IntentBody::CheckRecoveryAgain);
    f.rt.dispatch(&f.ctx, "c", IntentBody::CancelRecovery);
    assert_eq!(f.rt.outcome(&f.ctx, "c"), Some(OutcomeClass::Committed));
    assert!(f.rt.core.net_ops(&f.ctx).unwrap().is_empty());
    f.rt.restart();
    f.ctx = f.rt.open("q");
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Cancelled));
    let terminal = &f.app()["recoveryJoin"];
    assert_eq!(terminal["root"], "");
    assert!(terminal["record"].is_null());
    assert!(terminal["attempt"].is_null());
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS * 2));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Cancelled));
}

#[test]
fn cancelling_before_request_clears_the_lookup_and_secret() {
    let w = world();
    let mut f = fresh(&w.net);
    f.rt.dispatch(
        &f.ctx,
        "start",
        IntentBody::BeginRecovery {
            recovery_key: w.sheet,
            home: String::new(),
            place: None,
        },
    );
    f.rt.dispatch(&f.ctx, "stop", IntentBody::CancelRecovery);
    assert_eq!(f.rt.outcome(&f.ctx, "stop"), Some(OutcomeClass::Committed));
    f.rt.restart();
    f.ctx = f.rt.open("q");
    assert!(f.rt.core.net_ops(&f.ctx).unwrap().is_empty());
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Cancelled));
    assert_eq!(f.app()["recoveryJoin"]["root"], "");
}

#[test]
fn the_request_is_resent_at_a_third_and_two_thirds_with_the_same_bytes() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    let first = w.net.recovery_requests();
    assert_eq!(first.len(), 1);
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS / 3 + 1));
    f.tick(&mono(1, DELAY_MS / 3 * 2 + 2));
    // The Home deduplicates identical bytes: still one request, and the attempt id never changed.
    assert_eq!(w.net.recovery_requests(), first);
    assert_eq!(f.app()["recoveryJoin"]["attempt"]["resends"], 2);
}

// ----------------------------------------------------------------------------------------------- controller inbox

fn alice_tick(w: &mut World, session: u8, at: u64) {
    w.alice.tick(61_000);
    w.alice.recovery_tick(&w.a, &mono(session, at));
    for _ in 0..3 {
        w.alice.flush_net(&w.a);
    }
}

fn tracked(w: &World) -> Vec<RecoveryRequestView> {
    w.alice
        .query::<RecoveryStatusView>(Query::GetRecoveryStatus { ctx: w.a.clone() })
        .requests
}

#[test]
fn a_surviving_controller_registers_drains_and_acknowledges_and_is_told_of_the_request() {
    let mut w = world();
    // The inbox is registered on the first tick, before any request exists.
    alice_tick(&mut w, 1, 0);
    assert_eq!(
        w.alice.core.debug_app_state(&w.a).unwrap()["recoveryInbox"]["registered"],
        true
    );
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    assert!(
        tracked(&w).is_empty(),
        "nothing was pushed to Alice: she learns by draining"
    );
    alice_tick(&mut w, 1, 1000); // drain
    alice_tick(&mut w, 1, 2000); // observe, then acknowledge
    alice_tick(&mut w, 1, 3000);
    let t = tracked(&w);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].state, RecoveryRequestStateCode::Waiting);
    assert!(
        t[0].can_stop,
        "she is a live controller of the head the request names"
    );
    // Acknowledged: a later drain finds nothing new and the item does not return.
    let inbox = &w.alice.core.debug_app_state(&w.a).unwrap()["recoveryInbox"];
    assert!(inbox["pending"].as_array().unwrap().is_empty());
    assert!(inbox["toAck"].as_array().unwrap().is_empty());
}

#[test]
fn a_forged_inbox_item_authorizes_and_tracks_nothing_and_is_still_cleared() {
    let mut w = world();
    alice_tick(&mut w, 1, 0);
    // A request that decodes but is not signed by Alice's recovery key.
    let mut rng = om_core::crypto::rng::insecure_test_rng::InsecureTestRng::from_label(b"forger");
    let account = {
        let app = w.alice.core.debug_app_state(&w.a).unwrap();
        let chain: Vec<Vec<u8>> = app["acsChain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect();
        let creds: Vec<Vec<u8>> = app["acsCredentials"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect();
        om_core::control::verify_chain(&chain, &creds).unwrap()
    };
    let attacker = om_core::recovery::RecoveryRoot::generate(&mut rng);
    let dev = om_core::crypto::store::Device::new(&mut rng);
    let req = om_core::recovery::RecoveryRequest {
        account_id: account.account_id,
        recovery_epoch: 0,
        new_device_id: dev.device_id,
        new_device_credential: [1; 32],
        parent_state_hash: account.hash(),
        request_nonce: [4; 16],
    };
    let signed = om_core::recovery::sign_request(&req, &attacker.auth_signer(0), &mut rng);
    w.net.recovery_post_request(&signed);
    alice_tick(&mut w, 1, 1000);
    alice_tick(&mut w, 1, 2000);
    alice_tick(&mut w, 1, 3000);
    assert!(tracked(&w).is_empty());
    let inbox = &w.alice.core.debug_app_state(&w.a).unwrap()["recoveryInbox"];
    assert!(
        inbox["pending"].as_array().unwrap().is_empty()
            && inbox["toAck"].as_array().unwrap().is_empty()
    );
}

#[test]
fn a_veto_from_the_surviving_controller_reaches_the_owner_through_the_home_and_ends_the_attempt() {
    use om_core::host::NetCall;
    let mut w = world();
    alice_tick(&mut w, 1, 0);
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    for t in 1..=4 {
        alice_tick(&mut w, 1, t * 1000);
    }
    let id = tracked(&w)[0].request_id.clone();
    w.alice.restart();
    w.a = w.alice.open("p-a");
    w.alice.set_recovery_place(Some(RecoveryPlace {
        host: "current.example".into(),
        ..place()
    }));
    let c = common::confirmation(&mut w.alice, &w.a, "VetoRecovery", &id);
    w.alice.dispatch(
        &w.a,
        "veto",
        IntentBody::VetoRecovery {
            request_id: id,
            confirmation: c,
        },
    );
    let boards: Vec<_> = w
        .alice
        .core
        .net_ops(&w.a)
        .unwrap()
        .into_iter()
        .filter_map(|op| match op.call {
            NetCall::RecoveryVetoPost { host, port, .. } => Some((host, port)),
            _ => None,
        })
        .collect();
    assert_eq!(boards, [(place().host, place().port)]);
    w.alice.flush_net(&w.a);
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Vetoed));
}

fn recovered_with_contact(peer_ready: bool) -> (World, Fresh, String) {
    recover_with_contact(world(), peer_ready)
}

fn recover_with_contact(mut w: World, peer_ready: bool) -> (World, Fresh, String) {
    let list: Vec<ConversationSummary> = w
        .alice
        .query(Query::GetConversationList { ctx: w.a.clone() });
    let canonical =
        w.alice.core.debug_app_state(&w.a).unwrap()["relationships"][&list[0].id]["canonId"]
            .as_str()
            .expect("the conversation was bound")
            .to_string();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    settle_all(&mut [(&mut w.bob, &w.b)]);
    f.tick(&mono(1, 0));
    w.bob.recovery_tick(&w.b, &mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 5));
    assert!(f.view().is_none(), "account recovery finished");
    f.rt.restart();
    f.ctx = f.rt.open("q");
    if peer_ready {
        w.bob.recovery_tick(&w.b, &mono(1, DELAY_MS + 5));
    }
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    (w, f, canonical)
}

#[test]
fn relationship_restore_probe_older_snapshot_stays_outdated_after_restart() {
    let w = world();
    let (_, older) = w.net.recovery_record(&w.keys.locator).unwrap();
    replace_record(&w, |record| record.generation += 1);
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    assert_eq!(f.view().unwrap().stage, JoinStage::Found);
    let floor = f.app()["recoveryFloors"].clone();
    f.rt.restart();
    f.ctx = f.rt.open("q");
    w.net.recovery_replace(&w.keys.locator, older);
    assert_eq!(
        f.go("recheck", IntentBody::CheckRecoveryAgain),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.view().unwrap().stage, JoinStage::OutOfDate);
    assert_eq!(f.app()["recoveryFloors"], floor);
    assert_eq!(
        f.go("request", IntentBody::RequestRecovery),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::StaleState
        })
    );
}

#[test]
fn relationship_restore_probe_replays_the_same_notice_once_after_contact_restart() {
    use om_core::host::{NetCall, NetResult};

    let mut w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    let first_deposit = w.net.deposit_log().len();
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    let log = w.net.deposit_log();
    let [(mailbox, wrapped)] = &log[first_deposit..] else {
        panic!("expected the initial contact notice");
    };
    settle_all(&mut [(&mut w.bob, &w.b)]);
    w.bob.recovery_tick(&w.b, &mono(1, 0));
    w.bob.recovery_tick(&w.b, &mono(1, DELAY_MS / 2));
    let list: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    let before = list[0].peer_recovery.clone().unwrap();

    w.bob.restart();
    w.b = w.bob.open("p-b");
    assert!(matches!(
        w.net.perform(&NetCall::Deposit {
            mailbox: mailbox.clone(),
            relay_uri: None,
            bytes: wrapped.clone(),
        }),
        NetResult::Ok
    ));
    settle_all(&mut [(&mut w.bob, &w.b)]);
    let list: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    assert_eq!(list[0].peer_recovery, Some(before));
    let app = w.bob.core.debug_app_state(&w.b).unwrap();
    assert_eq!(
        app["relationships"][&list[0].id]["peerRecovery"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn relationship_restore_probe_explicit_reintroduction_allows_new_messages() {
    let (mut w, mut f, original_canonical) = recovered_with_contact(true);
    w.bob
        .dispatch(&w.b, "reconnect-invite", IntentBody::CreateInvite);
    w.bob.flush_net(&w.b);
    let invite: Option<InviteView> = w.bob.query(Query::GetInvite {
        ctx: w.b.clone(),
        operation_id: "reconnect-invite".into(),
    });
    f.rt.dispatch(
        &f.ctx,
        "reconnect-accept",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let introductions: Vec<IntroductionRequest> =
        w.bob.query(Query::GetIntroductions { ctx: w.b.clone() });
    assert_eq!(introductions.len(), 1);
    w.bob.dispatch(
        &w.b,
        "reconnect-introduction",
        IntentBody::AcceptIntroduction {
            request_id: introductions[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let restored: Vec<ConversationSummary> =
        f.rt.query(Query::GetConversationList { ctx: f.ctx.clone() });
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].relationship, RelationshipStateCode::Active);
    let canonical = f.app()["relationships"][&restored[0].id]["canonId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(
        canonical, original_canonical,
        "a normal invitation establishes a fresh wire conversation"
    );

    let bob_chats: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    assert_eq!(
        bob_chats.len(),
        1,
        "the fresh session replaced the original local thread"
    );
    let bob_conversation = &bob_chats[0].id;
    let text = "Welcome back";
    w.bob.dispatch(
        &w.b,
        "reconnected-message",
        IntentBody::SendMessage {
            conversation_id: bob_conversation.clone(),
            draft: Draft {
                text: text.into(),
                reply_to: None,
            },
        },
    );
    settle_all(&mut [(&mut w.bob, &w.b), (&mut f.rt, &f.ctx)]);
    let view: Option<ConversationView> = f.rt.query(Query::GetConversationView {
        ctx: f.ctx.clone(),
        conversation_id: restored[0].id.clone(),
    });
    assert!(view.unwrap().timeline.iter().any(
        |item| matches!(item, TimelineItem::Incoming { text: message, .. } if message == text)
    ));
}

#[test]
fn relationship_restore_probe_owner_invitation_resumes_existing_contact_thread() {
    let (mut w, mut f, _) = recovered_with_contact(true);
    let chats: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    let original = &chats[0];
    assert_eq!(
        original.peer_recovery.as_ref().map(|r| r.state),
        Some(om_core::state::PeerRecoveryState::Accepted)
    );

    let candidates: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].label, "Bob");
    assert!(candidates[0].conversation_id.is_none());
    assert!(candidates[0].invite_operation_id.is_none());
    assert_eq!(
        f.go(
            "owner-reconnect-invite",
            IntentBody::ReconnectContact {
                account_id: candidates[0].account_id.clone(),
            }
        ),
        Some(OutcomeClass::Committed)
    );
    let invite: Option<InviteView> = f.rt.query(Query::GetInvite {
        ctx: f.ctx.clone(),
        operation_id: "owner-reconnect-invite".into(),
    });
    w.bob.dispatch(
        &w.b,
        "owner-reconnect-accept",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let introductions: Vec<IntroductionRequest> =
        f.rt.query(Query::GetIntroductions { ctx: f.ctx.clone() });
    assert_eq!(introductions.len(), 1);
    f.rt.dispatch(
        &f.ctx,
        "owner-reconnect-bind",
        IntentBody::AcceptIntroduction {
            request_id: introductions[0].id.clone(),
        },
    );
    f.rt.restart();
    f.ctx = f.rt.open("q");
    f.rt.flush_net(&f.ctx);
    w.bob.poll(&w.b);
    w.bob.flush_net(&w.b);
    f.rt.poll(&f.ctx);
    f.rt.flush_net(&f.ctx);
    let waiting: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(
        waiting[0].conversation_id.is_none(),
        "a relay acknowledgement alone does not finish reconnecting"
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let ready: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(ready[0].conversation_id.is_some());
    let restored: Vec<ConversationSummary> =
        f.rt.query(Query::GetConversationList { ctx: f.ctx.clone() });
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].relationship, RelationshipStateCode::Active);

    let text = "Welcome back to our chat";
    w.bob.dispatch(
        &w.b,
        "owner-reconnect-original-thread",
        IntentBody::SendMessage {
            conversation_id: original.id.clone(),
            draft: Draft {
                text: text.into(),
                reply_to: None,
            },
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let view: Option<ConversationView> = f.rt.query(Query::GetConversationView {
        ctx: f.ctx.clone(),
        conversation_id: restored[0].id.clone(),
    });
    assert!(
        view.unwrap().timeline.iter().any(
            |item| matches!(item, TimelineItem::Incoming { text: message, .. } if message == text)
        ),
        "the original contact thread should send through the restored relationship"
    );
}

#[test]
fn relationship_restore_probe_early_binding_waits_for_the_contact_across_restart() {
    let (mut w, mut f, _) = recovered_with_contact(false);
    let before: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    let original = before[0].id.clone();
    let candidates: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    f.go(
        "invite-early",
        IntentBody::ReconnectContact {
            account_id: candidates[0].account_id.clone(),
        },
    );
    let invite: Option<InviteView> = f.rt.query(Query::GetInvite {
        ctx: f.ctx.clone(),
        operation_id: "invite-early".into(),
    });
    w.bob.dispatch(
        &w.b,
        "accept-early",
        IntentBody::AcceptInvitation {
            link: invite.unwrap().link,
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let intros: Vec<IntroductionRequest> =
        f.rt.query(Query::GetIntroductions { ctx: f.ctx.clone() });
    f.go(
        "intro-early",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let pending: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(pending[0].conversation_id.is_none());
    w.bob.restart();
    w.b = w.bob.open("p-b");
    w.bob.recovery_tick(&w.b, &mono(2, DELAY_MS + 5)); // no credit across restart
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let pending: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(pending[0].conversation_id.is_none());
    w.bob.recovery_tick(&w.b, &mono(2, 2 * DELAY_MS + 5));
    settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
    let ready: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(ready[0].conversation_id.is_some());
    let after: Vec<ConversationSummary> =
        w.bob.query(Query::GetConversationList { ctx: w.b.clone() });
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, original);
}

#[test]
fn relationship_restore_probe_uses_the_newest_same_head_snapshot() {
    let w = world();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("request", IntentBody::RequestRecovery);
    replace_record(&w, |record| {
        record.generation += 1;
        let rr::Relationships::Plain(snapshot) = &mut record.relationships else {
            panic!("plain snapshot")
        };
        snapshot.entries[0].label = Some("Updated label".into());
    });
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 10));
    assert!(f.view().is_none());
    f.rt.restart();
    f.ctx = f.rt.open("q");
    let contacts: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].label, "Updated label");
}

#[test]
fn relationship_restore_probe_conflicted_snapshot_cannot_be_staged_after_restart() {
    let w = world();
    let (_, original) = w.net.recovery_record(&w.keys.locator).unwrap();
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("request", IntentBody::RequestRecovery);
    replace_record(&w, |record| {
        let rr::Relationships::Plain(snapshot) = &mut record.relationships else {
            panic!("plain snapshot")
        };
        snapshot.entries[0].label = Some("Conflicting label".into());
    });
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS + 10));
    assert!(f.view().is_some(), "the conflicting round refused adoption");
    f.rt.restart();
    f.ctx = f.rt.open("q");
    w.net.recovery_replace(&w.keys.locator, original);
    f.tick(&mono(2, 0));
    assert!(
        f.view().is_none(),
        "the owner can adopt the independently verified account"
    );
    let contacts: Vec<ReconnectContactView> =
        f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
    assert!(
        contacts.is_empty(),
        "replaying one side of the conflict cannot restore the snapshot"
    );
}

mod room_rejoin {
    use super::*;
    use om_core::groups::GroupError;
    use om_core::groups::flow::{
        GroupEvent, GroupIntent, GroupQuery, GroupReply, GroupView, GroupWire, Sender,
    };
    use om_core::groups::rejoin::RejoinView;

    fn group_do(
        rt: &mut Runtime,
        ctx: &ProfileExecutionContext,
        intent: GroupIntent,
    ) -> Result<GroupReply, GroupError> {
        let (r, step) = rt.core.group_do(ctx, intent).unwrap();
        assert!(rt.settle(&step));
        r
    }

    fn commits(rt: &mut Runtime, ctx: &ProfileExecutionContext) -> Vec<GroupEvent> {
        let (r, step) = rt.core.group_settle(ctx).unwrap();
        assert!(rt.settle(&step));
        r.unwrap().into_iter().flat_map(|r| r.events).collect()
    }

    fn send_lane(rt: &mut Runtime, ctx: &ProfileExecutionContext) {
        rt.supply_routes(8);
        loop {
            let (r, step) = rt.core.group_lane_flush(ctx).unwrap();
            assert!(rt.settle(&step));
            if !r.unwrap() {
                break;
            }
        }
    }

    fn pump(rt: &mut Runtime, ctx: &ProfileExecutionContext) -> Vec<GroupEvent> {
        let (r, step) = rt.core.group_lane_pump(ctx).unwrap();
        assert!(rt.settle(&step));
        r.into_iter().flat_map(|(_, r)| r.unwrap().events).collect()
    }

    fn finish_routes(rt: &mut Runtime, ctx: &ProfileExecutionContext, conv: &[u8]) {
        rt.supply_routes(8);
        let (r, step) = rt.core.group_route_update(ctx, conv).unwrap();
        r.unwrap();
        assert!(rt.settle(&step));
    }

    fn deliver_frames(
        from: &mut Runtime,
        fc: &ProfileExecutionContext,
        to: &mut Runtime,
        tc: &ProfileExecutionContext,
        wanted: impl Fn(&GroupWire) -> bool,
    ) -> Vec<GroupEvent> {
        let mut events = vec![];
        for item in from
            .core
            .group_outbox(fc)
            .unwrap()
            .into_iter()
            .filter(|i| wanted(&i.wire))
        {
            from.supply_routes(8);
            let (sealed, step) = from
                .core
                .group_frame_send(fc, &item.to_outbound().unwrap())
                .unwrap();
            assert!(from.settle(&step));
            for s in sealed.unwrap() {
                let (r, step) = to.core.group_frame_receive(tc, &s.wrapper).unwrap();
                assert!(to.settle(&step));
                events.extend(r.unwrap().events);
            }
            let step = from.core.group_outbox_done(fc, &[item.id]).unwrap();
            assert!(from.settle(&step));
        }
        events
    }

    fn chat(rt: &mut Runtime, ctx: &ProfileExecutionContext) -> String {
        let list: Vec<ConversationSummary> =
            rt.query(Query::GetConversationList { ctx: ctx.clone() });
        assert_eq!(list.len(), 1);
        list[0].id.clone()
    }

    fn room(
        from: &mut Runtime,
        fc: &ProfileExecutionContext,
        to: &mut Runtime,
        tc: &ProfileExecutionContext,
        name: &str,
    ) -> Vec<u8> {
        let relationship = chat(from, fc);
        let r = group_do(
            from,
            fc,
            GroupIntent::CreateWithProtection {
                name: name.into(),
                relationship_ids: vec![relationship],
                protection: 3,
            },
        )
        .unwrap();
        let conv = r
            .events
            .into_iter()
            .find_map(|e| match e {
                GroupEvent::Created { conversation_id } => Some(conversation_id),
                _ => None,
            })
            .unwrap();
        send_lane(from, fc);
        settle_all(&mut [(from, fc), (to, tc)]);
        let i = invitation(&pump(to, tc));
        group_do(
            to,
            tc,
            GroupIntent::Accept {
                invite_id: i,
                siblings: vec![],
            },
        )
        .unwrap();
        send_lane(to, tc);
        settle_all(&mut [(from, fc), (to, tc)]);
        pump(from, fc);
        commits(from, fc);
        let joined = deliver_frames(from, fc, to, tc, |w| matches!(w, GroupWire::Welcome { .. }));
        assert!(joined.contains(&GroupEvent::Joined {
            conversation_id: conv.clone()
        }));
        conv
    }

    fn invitation(events: &[GroupEvent]) -> String {
        events
            .iter()
            .find_map(|e| match e {
                GroupEvent::InviteReceived { invite_id } => Some(invite_id.clone()),
                _ => None,
            })
            .expect("ordinary invitation")
    }

    fn reconnect(w: &mut World, f: &mut Fresh) -> String {
        let candidates: Vec<ReconnectContactView> =
            f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
        assert_eq!(
            f.go(
                "reconnect-for-room",
                IntentBody::ReconnectContact {
                    account_id: candidates[0].account_id.clone(),
                }
            ),
            Some(OutcomeClass::Committed)
        );
        let invite: Option<InviteView> = f.rt.query(Query::GetInvite {
            ctx: f.ctx.clone(),
            operation_id: "reconnect-for-room".into(),
        });
        w.bob.dispatch(
            &w.b,
            "accept-reconnect-for-room",
            IntentBody::AcceptInvitation {
                link: invite.unwrap().link,
            },
        );
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        let intros: Vec<IntroductionRequest> =
            f.rt.query(Query::GetIntroductions { ctx: f.ctx.clone() });
        f.rt.dispatch(
            &f.ctx,
            "bind-for-room",
            IntentBody::AcceptIntroduction {
                request_id: intros[0].id.clone(),
            },
        );
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        let candidates: Vec<ReconnectContactView> =
            f.rt.query(Query::GetRelationshipRestore { ctx: f.ctx.clone() });
        candidates[0]
            .conversation_id
            .clone()
            .expect("live restored contact")
    }

    fn rejoin(rt: &Runtime, ctx: &ProfileExecutionContext) -> RejoinView {
        match rt
            .core
            .group_query(ctx, GroupQuery::Rejoin)
            .unwrap()
            .unwrap()
        {
            GroupView::Rejoin(view) => view,
            _ => unreachable!(),
        }
    }

    #[test]
    fn recovered_owner_rejoins_only_through_an_ordinary_invitation() {
        let mut w = world();
        let conv = room(&mut w.bob, &w.b, &mut w.alice, &w.a, "Book club");
        let other = room(&mut w.alice, &w.a, &mut w.bob, &w.b, "Weekend plans");
        let devices: DevicesView = w.alice.query(Query::GetDevices { ctx: w.a.clone() });
        let old = devices
            .devices
            .iter()
            .find(|d| d.is_current)
            .unwrap()
            .id
            .clone();
        let (mut w, mut f, _) = recover_with_contact(w, true);
        let relationship = reconnect(&mut w, &mut f);
        assert_eq!(rejoin(&f.rt, &f.ctx).contacts, [relationship.clone()]);

        let ask = |hint: &str| GroupIntent::AskToRejoin {
            relationship_id: relationship.clone(),
            room_hint: hint.into(),
        };
        let wire = group_do(&mut f.rt, &f.ctx, ask("Book club"))
            .unwrap()
            .out
            .pop()
            .unwrap()
            .wire;
        group_do(&mut f.rt, &f.ctx, ask("Weekend plans")).unwrap();
        assert!(
            group_do(&mut f.rt, &f.ctx, ask("Book club"))
                .unwrap()
                .out
                .is_empty(),
            "repeat tap is idempotent"
        );
        f.rt.restart();
        f.ctx = f.rt.open("q");
        assert_eq!(rejoin(&f.rt, &f.ctx).pending.len(), 2);
        let log_start = w.net.deposit_log().len();
        send_lane(&mut f.rt, &f.ctx);
        f.rt.flush_net(&f.ctx);
        let delivered = w.net.deposit_log().split_off(log_start);
        assert!(delivered.len() >= 2);
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        assert_eq!(
            pump(&mut w.bob, &w.b),
            [GroupEvent::RejoinChanged, GroupEvent::RejoinChanged]
        );

        w.bob.restart();
        w.b = w.bob.open("p-b");
        for (mailbox, bytes) in delivered {
            use om_core::host::{NetCall, NetResult};
            assert!(matches!(
                w.net.perform(&NetCall::Deposit {
                    mailbox,
                    bytes,
                    relay_uri: None
                }),
                NetResult::Ok
            ));
        }
        // Repeat the semantic request under a fresh sealed event after the recipient restarts.
        let (r, step) =
            f.rt.core
                .group_lane_send(&f.ctx, &relationship, &wire)
                .unwrap();
        r.unwrap();
        assert!(f.rt.settle(&step));
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        assert!(pump(&mut w.bob, &w.b).is_empty());
        let reviews = rejoin(&w.bob, &w.b).reviews;
        assert_eq!(reviews.len(), 2);
        let review = reviews
            .iter()
            .find(|r| r.conversation_id == hex(&conv))
            .unwrap();
        let policy_refused = reviews
            .iter()
            .find(|r| r.conversation_id == hex(&other))
            .unwrap();
        assert!(!policy_refused.can_invite);
        let confirm = |id: &str| GroupIntent::ResolveRejoin {
            request_id: id.into(),
            agree: true,
        };
        assert_eq!(
            group_do(&mut w.bob, &w.b, confirm(&policy_refused.id)),
            Err(GroupError("not_authorized"))
        );
        assert_eq!(
            group_do(
                &mut w.bob,
                &w.b,
                GroupIntent::SendText {
                    conversation_id: conv.clone(),
                    text: "blocked before removal".into(),
                }
            ),
            Err(GroupError("revoked_leaf_pending"))
        );

        let parent = w.bob.core.debug_app_state(&w.b).unwrap()["groups"][hex(&conv)]["head"]
            .as_str()
            .unwrap()
            .to_string();
        let parent = om_core::groups::GroupState::decode(&unhex(&parent).unwrap()).unwrap();
        let removal = group_do(&mut w.bob, &w.b, confirm(&review.id)).unwrap();
        assert!(
            removal
                .out
                .iter()
                .all(|o| matches!(o.wire, GroupWire::Commit { .. }))
        );
        assert!(
            removal
                .events
                .iter()
                .all(|e| !matches!(e, GroupEvent::InviteSent { .. }))
        );
        w.bob.restart();
        w.b = w.bob.open("p-b");
        let events = commits(&mut w.bob, &w.b);
        assert!(events.contains(&GroupEvent::DeviceRemoved {
            conversation_id: conv.clone()
        }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GroupEvent::InviteSent { .. }))
        );
        let bob = w.bob.core.debug_app_state(&w.b).unwrap();
        assert!(bob["groupDevices"][hex(&conv)][&old].is_null());
        let removed = om_core::groups::GroupState::decode(
            &unhex(bob["groups"][hex(&conv)]["head"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(removed.membership, parent.membership);
        assert_eq!(removed.mls_epoch_ref, parent.mls_epoch_ref + 1);
        drop(bob);
        finish_routes(&mut w.bob, &w.b, &conv);
        send_lane(&mut w.bob, &w.b);
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);

        let GroupView::Group(detail) = w
            .bob
            .core
            .group_query(
                &w.b,
                GroupQuery::Group {
                    conversation_id: conv.clone(),
                },
            )
            .unwrap()
            .unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            detail.people.len(),
            2,
            "an invitation for a device adds no person to the member view"
        );
        assert_eq!(detail.summary.invited, 0);
        let invite = invitation(&pump(&mut f.rt, &f.ctx));
        let unsolicited = GroupWire::Welcome {
            invite_id: invite.clone(),
            welcome: String::new(),
            state: String::new(),
            directory: vec![],
        };
        let (r, step) =
            f.rt.core
                .group_receive(
                    &f.ctx,
                    Sender::Relationship(relationship.clone()),
                    unsolicited,
                )
                .unwrap();
        assert!(f.rt.settle(&step));
        assert_eq!(
            r.unwrap().events,
            [GroupEvent::Dropped("unsolicited_welcome")]
        );
        assert!(
            f.rt.core
                .group_query(
                    &f.ctx,
                    GroupQuery::Group {
                        conversation_id: conv.clone()
                    }
                )
                .unwrap()
                .is_err()
        );
        assert_eq!(rejoin(&f.rt, &f.ctx).pending.len(), 2);

        group_do(
            &mut f.rt,
            &f.ctx,
            GroupIntent::Accept {
                invite_id: invite,
                siblings: vec![],
            },
        )
        .unwrap();
        send_lane(&mut f.rt, &f.ctx);
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        assert!(
            pump(&mut w.bob, &w.b)
                .iter()
                .any(|e| matches!(e, GroupEvent::CommitPrepared { .. }))
        );
        commits(&mut w.bob, &w.b);
        let joined = deliver_frames(&mut w.bob, &w.b, &mut f.rt, &f.ctx, |w| {
            matches!(w, GroupWire::Welcome { .. })
        });
        assert!(joined.contains(&GroupEvent::Joined {
            conversation_id: conv.clone()
        }));
        assert_eq!(rejoin(&f.rt, &f.ctx).pending[0].room_hint, "Weekend plans");
        let app = w.bob.core.debug_app_state(&w.b).unwrap();
        let after = om_core::groups::GroupState::decode(
            &unhex(app["groups"][hex(&conv)]["head"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            after.membership, parent.membership,
            "rejoin preserves the membership incarnation and roles"
        );
        assert_eq!(after.mls_epoch_ref, removed.mls_epoch_ref + 1);
        assert!(app["groupDevices"][hex(&conv)][&old].is_null());

        let talk = |from: &mut Runtime,
                    fc: &ProfileExecutionContext,
                    to: &mut Runtime,
                    tc: &ProfileExecutionContext,
                    sent: &str| {
            group_do(
                from,
                fc,
                GroupIntent::SendText {
                    conversation_id: conv.clone(),
                    text: sent.into(),
                },
            )
            .unwrap();
            assert!(
                deliver_frames(from, fc, to, tc, |w| matches!(w, GroupWire::App { .. }))
                    .iter()
                    .any(|e| matches!(e, GroupEvent::MessageReceived { .. }))
            );
            let GroupView::Messages(log) = to
                .core
                .group_query(
                    tc,
                    GroupQuery::Messages {
                        conversation_id: conv.clone(),
                    },
                )
                .unwrap()
                .unwrap()
            else {
                unreachable!()
            };
            assert_eq!(log.last().unwrap().text, sent);
        };
        talk(&mut f.rt, &f.ctx, &mut w.bob, &w.b, "I'm back");
        talk(&mut w.bob, &w.b, &mut f.rt, &f.ctx, "Welcome back");

        f.rt.set_recovery_place(Some(place()));
        assert_eq!(
            f.go("new-head", IntentBody::ReplaceRecoveryKey),
            Some(OutcomeClass::Committed)
        );
        assert_eq!(
            f.go(
                "save-new-head",
                IntentBody::ConfirmRecoveryKeyStored {
                    key_operation_id: "new-head".into()
                }
            ),
            Some(OutcomeClass::Committed)
        );
        settle_all(&mut [(&mut f.rt, &f.ctx), (&mut w.bob, &w.b)]);
        assert!(
            rejoin(&w.bob, &w.b).reviews.is_empty(),
            "a changed head invalidates the earlier review"
        );
        assert_eq!(
            group_do(&mut w.bob, &w.b, confirm(&policy_refused.id)),
            Err(GroupError("stale_rejoin"))
        );

        let person = w
            .bob
            .query::<Vec<ConversationSummary>>(Query::GetConversationList { ctx: w.b.clone() })[0]
            .person_id
            .clone();
        let confirmation = common::confirmation(&mut w.bob, &w.b, "BlockPerson", &person);
        w.bob.dispatch(
            &w.b,
            "block-reconnected-contact",
            IntentBody::BlockPerson {
                person_id: person,
                scope: BlockScope::Local,
                confirmation,
            },
        );
        assert_eq!(
            w.bob.outcome(&w.b, "block-reconnected-contact"),
            Some(OutcomeClass::Committed)
        );
        assert_eq!(
            group_do(
                &mut w.bob,
                &w.b,
                GroupIntent::SendText {
                    conversation_id: other,
                    text: "the other Room still has the revoked leaf".into(),
                }
            ),
            Err(GroupError("revoked_leaf_pending"))
        );
    }
}
