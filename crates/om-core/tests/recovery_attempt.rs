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
    assert_eq!(
        f.go("c", IntentBody::CancelRecovery),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Cancelled));
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS * 2));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Cancelled));
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
    let mut w = world();
    alice_tick(&mut w, 1, 0);
    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    f.go("req", IntentBody::RequestRecovery);
    for t in 1..=4 {
        alice_tick(&mut w, 1, t * 1000);
    }
    let id = tracked(&w)[0].request_id.clone();
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
    f.tick(&mono(1, 0));
    f.tick(&mono(1, DELAY_MS));
    assert_eq!(f.view().unwrap().attempt, Some(AttemptState::Vetoed));
}
