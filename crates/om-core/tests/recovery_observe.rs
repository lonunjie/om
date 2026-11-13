// SPDX-License-Identifier: MPL-2.0
//! Recovery, controller side (work group E, slice 3): observing a request and its vetoes, the verifier-local interval,
//! `VetoRecovery`, the controller co-sign ("That was me") and applying a completion. Core is driven as a host would
//! drive it: observations and monotonic readings are handed in; nothing is fetched.
//! Conformance ID-03: a verifier that was withheld the request waits its OWN full interval.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::control;
use om_core::crypto::binding::Acs;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::unhex;
use om_core::host::{MonotonicSample, NetCall};
use om_core::recovery::{
    self, RecoveryPolicy, RecoveryRequest, RecoveryRoot, RecoveryVeto, SuccessorBundle,
    sign_request, sign_veto,
};
use om_core::runtime::{LoopbackNetwork, Runtime};
use std::collections::BTreeSet;

const H: u64 = 3_600_000;
const DELAY_MS: u64 = 72 * H;

fn mono(session: u8, ms: u64) -> MonotonicSample {
    MonotonicSample {
        boot_session: vec![session; 8],
        elapsed_ms: ms,
    }
}

/// The recovery key as shown to the person (56 base32 characters in groups of 4, 32 bytes + a 3 byte check).
fn root_from_groups(groups: &[String]) -> RecoveryRoot {
    const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let s: String = groups.concat();
    let mut bits = String::new();
    for c in s.chars() {
        bits.push_str(&format!("{:05b}", A.find(c).unwrap()));
    }
    let bytes: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap())
        .collect();
    RecoveryRoot::from_bytes(bytes.try_into().unwrap())
}

fn w_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

struct World {
    rt: Runtime,
    ctx: ProfileExecutionContext,
    root: RecoveryRoot,
    rng: InsecureTestRng,
    /// The device that is recovering (it is not part of the account).
    new_dev: Device,
}

fn world(label: &str) -> World {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new(label, &net);
    let ctx = rt.open("p");
    rt.dispatch(
        &ctx,
        "create",
        IntentBody::CreateIdentityProfile { label: "Me".into() },
    );
    rt.flush_net(&ctx);
    let key: Option<RecoveryKeyView> = rt.query(Query::GetRecoveryKey {
        ctx: ctx.clone(),
        key_operation_id: "create".into(),
    });
    let root = root_from_groups(&key.expect("recovery key").groups);
    let mut w = World {
        rt,
        ctx,
        root,
        rng: InsecureTestRng::from_label(label.as_bytes()),
        new_dev: Device::new(&mut InsecureTestRng::from_label(b"x")),
    };
    let account = w.head().account_id;
    let mut d = Device::new(&mut w.rng);
    d.account_id = account;
    w.new_dev = d;
    w
}

impl World {
    fn app(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()
    }
    fn hexes(&self, k: &str) -> Vec<Vec<u8>> {
        self.app()[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect()
    }
    fn links(&self) -> Vec<control::RecoveryLinkEvidence> {
        let h = |v: &serde_json::Value, k: &str| unhex(v[k].as_str().unwrap()).unwrap();
        self.app()["recoveryLinks"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|l| control::RecoveryLinkEvidence {
                child_hash: h(l, "childHash").try_into().unwrap(),
                policy: h(l, "policy"),
                next_policy: h(l, "nextPolicy"),
                request: h(l, "request"),
                elapsed_seconds: l["elapsedSeconds"].as_u64().unwrap(),
            })
            .collect()
    }
    fn head(&self) -> Acs {
        control::verify_chain_with(
            &self.hexes("acsChain"),
            &self.hexes("acsCredentials"),
            &self.links(),
        )
        .unwrap()
    }
    /// Rewrite the stored app state with `f`, then reopen the profile (a restart over the edited record).
    fn rewrite(&mut self, f: impl FnOnce(&mut serde_json::Value)) {
        let rec = self
            .rt
            .store
            .records("p")
            .into_iter()
            .find(|r| r.key == "state")
            .unwrap();
        let v = om_core::crypto::cbor::decode(&rec.value).unwrap();
        let mut app: serde_json::Value =
            serde_json::from_slice(v.get(1).unwrap().b().unwrap()).unwrap();
        f(&mut app);
        let new = om_core::crypto::cbor::encode(&om_core::crypto::cbor::V::M(vec![
            (0, v.get(0).unwrap().clone()),
            (
                1,
                om_core::crypto::cbor::V::B(serde_json::to_vec(&app).unwrap()),
            ),
        ]));
        self.rt.store.replace_record("p", "state", new);
        self.ctx = self.rt.restart().remove(0);
    }
    fn policy(&self) -> RecoveryPolicy {
        RecoveryPolicy::from_body(&unhex(self.app()["recoveryPolicy"].as_str().unwrap()).unwrap())
            .unwrap()
    }
    fn request_for(&mut self, parent: [u8; 32], nonce: u8) -> Vec<u8> {
        let req = RecoveryRequest {
            account_id: self.head().account_id,
            recovery_epoch: 0,
            new_device_id: self.new_dev.device_id,
            new_device_credential: self.new_dev.credential().hash(),
            parent_state_hash: parent,
            request_nonce: [nonce; 16],
        };
        sign_request(&req, &self.root.auth_signer(0), &mut self.rng)
    }
    fn request(&mut self) -> Vec<u8> {
        let h = self.head().hash();
        self.request_for(h, 9)
    }
    fn digest(req: &[u8], policy: &RecoveryPolicy) -> String {
        om_core::crypto::hex(
            &recovery::open_request(req, policy.recovery_key().unwrap())
                .unwrap()
                .digest(),
        )
    }
    fn next_policy(&self) -> RecoveryPolicy {
        let mut p = RecoveryPolicy::default_for(self.root.auth_public_key(1), 1);
        p.delay_seconds = self.policy().delay_seconds;
        p
    }
    /// The recovering device's recovery-signed successor of the current head (no controller co-signature).
    fn bundle(&mut self) -> SuccessorBundle {
        let p = self.head();
        let mut c = p.clone();
        c.generation = p.generation + 1;
        c.previous = Some(p.hash());
        let mut revoked = p.revoked.clone();
        revoked.extend(p.control.iter().chain(p.messaging.iter()));
        c.control = BTreeSet::from([self.new_dev.device_id]);
        c.quarantined.clear();
        c.messaging.clear();
        c.revoked = revoked;
        c.credentials = vec![(self.new_dev.device_id, self.new_dev.credential().hash())];
        c.key_epochs.clear();
        c.recovery_commitment = Some(self.next_policy().hash());
        let child = control::sign_acs_recovery(
            &c.body(),
            &c.account_id,
            &self.root.auth_signer(0),
            &[],
            &mut self.rng,
        );
        SuccessorBundle {
            child,
            next_policy: self.next_policy().body(),
            credential: self.new_dev.credential().body(),
        }
    }
    fn observe(&mut self, req: &[u8], m: &MonotonicSample) {
        self.rt.recovery_observe_request(&self.ctx, req, m);
    }
    fn tick(&mut self, m: &MonotonicSample) {
        self.rt.recovery_tick(&self.ctx, m);
    }
    fn status(&self) -> RecoveryStatusView {
        self.rt.query(Query::GetRecoveryStatus {
            ctx: self.ctx.clone(),
        })
    }
    fn tracked(&self) -> usize {
        self.app()["recoveryRequests"]
            .as_object()
            .map_or(0, |m| m.len())
    }
    fn rec(&self, digest: &str) -> serde_json::Value {
        self.app()["recoveryRequests"][digest].clone()
    }
    fn intent(&mut self, op: &str, body: IntentBody) -> Option<OutcomeClass> {
        self.rt.dispatch(&self.ctx, op, body);
        self.rt.outcome(&self.ctx, op)
    }
    fn veto(&mut self, id: &str) -> Option<OutcomeClass> {
        let c = nonce(&mut self.rt, &self.ctx, "VetoRecovery", id);
        self.intent(
            &format!("veto-{id}-{}", w_seq()),
            IntentBody::VetoRecovery {
                request_id: id.into(),
                confirmation: c,
            },
        )
    }
    fn confirm(&mut self, id: &str) -> Option<OutcomeClass> {
        let c = nonce(&mut self.rt, &self.ctx, "ConfirmRecovery", id);
        self.intent(
            &format!("confirm-{id}-{}", w_seq()),
            IntentBody::ConfirmRecovery {
                request_id: id.into(),
                confirmation: c,
            },
        )
    }
    /// `n` more controllers the test keeps (so it can sign vetoes as them), linked in one transition.
    fn more_controllers(&mut self, n: usize) -> Vec<Device> {
        let devs: Vec<Device> = (0..n)
            .map(|_| {
                let mut d = Device::new(&mut self.rng);
                d.account_id = self.head().account_id;
                d
            })
            .collect();
        let creds: Vec<Vec<u8>> = devs.iter().map(|d| d.credential().body()).collect();
        let step = self
            .rt
            .core
            .debug_link_controllers(&self.ctx, &creds)
            .unwrap();
        assert!(self.rt.settle(&step));
        devs
    }
    fn second_controller(&mut self) -> Device {
        self.more_controllers(1).remove(0)
    }
    fn veto_by(&mut self, who: &Device, req: &[u8]) -> Vec<u8> {
        let r = recovery::open_request(req, self.policy().recovery_key().unwrap()).unwrap();
        sign_veto(
            &RecoveryVeto {
                account_id: r.account_id,
                recovery_epoch: r.recovery_epoch,
                request_digest: r.digest(),
            },
            &who.device_id,
            &who.signer,
            &mut self.rng,
        )
    }
}

fn state_of(v: &RecoveryStatusView, id: &str) -> RecoveryRequestStateCode {
    v.requests
        .iter()
        .find(|r| r.request_id == id)
        .expect("request listed")
        .state
}

fn rejected(r: RejectReason) -> Option<OutcomeClass> {
    Some(OutcomeClass::Rejected { reason: r })
}

#[test]
fn first_observation_is_persisted_and_a_duplicate_never_resets_the_interval() {
    let mut w = world("obs-dup");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    assert!(w.status().requests.is_empty());
    w.observe(&req, &mono(1, 1_000));
    let first = w.rec(&id);
    assert_eq!(first["creditedMs"], 0);
    assert_eq!(first["state"], "waiting");
    w.tick(&mono(1, 1_000 + 5 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 5 * H);
    // The same request seen again (a different route, a replay): nothing changes, least of all the interval.
    w.rt.tick(10_000);
    w.observe(&req, &mono(1, 1_000 + 6 * H));
    let again = w.rec(&id);
    assert_eq!(again["creditedMs"], 5 * H);
    assert_eq!(again["firstSeenMs"], first["firstSeenMs"]);
    assert_eq!(w.tracked(), 1);
    let v = w.status();
    assert_eq!(v.requests.len(), 1);
    assert_eq!(v.requests[0].state, RecoveryRequestStateCode::Waiting);
    assert_eq!(v.requests[0].direction, RecoveryRequestDirection::Incoming);
    assert!(v.requests[0].can_stop);
}

#[test]
fn activates_at_comes_only_from_this_verifiers_own_interval() {
    let mut w = world("obs-activates");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let now = |w: &World| {
        let v = w.status();
        (v.requests[0].activates_at.unwrap(), v.activation_delay_ms)
    };
    let (a0, delay) = now(&w);
    assert_eq!(delay, DELAY_MS);
    // Wall time passing without monotonic credit shortens nothing: the estimate just moves with `now`.
    let wall0 = a0 - DELAY_MS;
    w.rt.tick(100 * H);
    w.tick(&mono(1, 0));
    assert_eq!(now(&w).0, wall0 + 100 * H + DELAY_MS);
    assert_eq!(w.rec(&id)["creditedMs"], 0);
    // Credited monotonic time does.
    w.tick(&mono(1, 10 * H));
    assert_eq!(now(&w).0, wall0 + 100 * H + DELAY_MS - 10 * H);
    assert!(!w.status().requests[0].ready);
    w.tick(&mono(1, DELAY_MS));
    let v = w.status();
    assert!(v.requests[0].ready);
    assert_eq!(v.requests[0].state, RecoveryRequestStateCode::Waiting);
    assert_eq!(v.requests[0].activates_at, Some(wall0 + 100 * H));
}

#[test]
fn id_03_a_verifier_that_was_withheld_the_request_waits_its_own_full_interval() {
    let mut w = world("obs-id03");
    let req = w.request();
    // The request has been "out there" for ten days by wall clock before this verifier first sees it.
    w.rt.tick(240 * H);
    w.observe(&req, &mono(1, 7 * 24 * H));
    let v = w.status();
    assert_eq!(v.requests[0].state, RecoveryRequestStateCode::Waiting);
    assert!(!v.requests[0].ready);
    assert_eq!(
        v.requests[0].activates_at,
        Some(w.rt.clock.get() + DELAY_MS)
    );
    // Even a recovery-signed successor cannot be applied before ITS interval has run.
    let b = w.bundle();
    let id = World::digest(&req, &w.policy());
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 7 * 24 * H + 71 * H));
    assert_eq!(w.rec(&id)["state"], "waiting");
    w.tick(&mono(1, 7 * 24 * H + DELAY_MS));
    assert_eq!(w.rec(&id)["state"], "completed");
}

#[test]
fn a_restart_keeps_the_interval_and_downtime_is_never_credited() {
    let mut w = world("obs-restart");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    w.tick(&mono(1, 20 * H));
    let ctxs = w.rt.restart();
    w.ctx = ctxs[0].clone();
    assert_eq!(w.rec(&id)["creditedMs"], 20 * H);
    // A new boot of the host clock: nothing is credited for the time the app was not running (or for the new
    // session's own first reading), only for growth after it.
    w.tick(&mono(2, 500 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 20 * H);
    w.tick(&mono(2, 500 * H + 3 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 23 * H);
    // A reading that goes backwards (a clock fault) credits nothing and is not trusted as the new baseline.
    w.tick(&mono(2, 499 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 23 * H);
    w.tick(&mono(2, 500 * H + 4 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 24 * H);
}

#[test]
fn lost_or_rolled_back_timer_state_lengthens_the_interval_never_shortens_it() {
    let mut w = world("obs-rollback");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let snapshot = w.rt.store.records("p");
    w.tick(&mono(1, 40 * H));
    assert_eq!(w.rec(&id)["creditedMs"], 40 * H);
    // The durable record is rolled back to before the credit.
    for r in &snapshot {
        w.rt.store.replace_record("p", &r.key, r.value.clone());
    }
    let ctxs = w.rt.restart();
    w.ctx = ctxs[0].clone();
    assert_eq!(w.rec(&id)["creditedMs"], 0);
    // The record is gone entirely: the next observation starts a brand-new interval.
    let mut w2 = world("obs-lost");
    let req2 = w2.request();
    let id2 = World::digest(&req2, &w2.policy());
    assert_eq!(w2.tracked(), 0);
    w2.rt.tick(50 * H);
    w2.observe(&req2, &mono(1, 90 * H));
    assert_eq!(w2.rec(&id2)["creditedMs"], 0);
    assert!(!w2.status().requests[0].ready);
}

#[test]
fn a_veto_observed_before_activation_blocks_it_for_good() {
    let mut w = world("obs-veto");
    let other = w.second_controller();
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 0));
    assert_eq!(w.rec(&id)["state"], "waiting");
    let v = w.veto_by(&other, &req);
    w.rt.recovery_observe_veto(&w.ctx, &v, &mono(1, 10 * H));
    assert_eq!(w.rec(&id)["state"], "vetoed");
    assert_eq!(state_of(&w.status(), &id), RecoveryRequestStateCode::Vetoed);
    // The full delay running afterwards, or a co-sign, completes nothing.
    w.tick(&mono(1, 200 * H));
    assert_eq!(w.rec(&id)["state"], "vetoed");
    assert_eq!(w.confirm(&id), rejected(RejectReason::Conflict));
    // The same veto again is not a second entry.
    w.rt.recovery_observe_veto(&w.ctx, &v, &mono(1, 201 * H));
    assert_eq!(w.rec(&id)["vetoes"].as_array().unwrap().len(), 1);
}

#[test]
fn a_veto_by_an_unknown_or_quarantined_signer_or_for_another_request_is_ignored() {
    let mut w = world("obs-veto-ignored");
    // Three controllers, so suspending one does not freeze the account.
    let other = w.more_controllers(2).remove(0);
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    // A stranger (not a controller of the account), even with a perfectly formed veto.
    let mut stranger = Device::new(&mut w.rng);
    stranger.account_id = w.head().account_id;
    let sv = w.veto_by(&stranger, &req);
    w.rt.recovery_observe_veto(&w.ctx, &sv, &mono(1, H));
    assert_eq!(w.rec(&id)["state"], "waiting");
    // A messaging device is not a controller either.
    let (msg_id, step) = w.rt.core.debug_link_messaging_device(&w.ctx).unwrap();
    assert!(w.rt.settle(&step));
    let _ = msg_id;
    // A veto naming some other request digest is dropped (nothing tracked under it).
    let other_req = {
        let h = w.head().hash();
        w.request_for(h, 77)
    };
    let ov = w.veto_by(&other, &other_req);
    w.rt.recovery_observe_veto(&w.ctx, &ov, &mono(1, H));
    assert_eq!(w.tracked(), 1);
    // A veto by a quarantined controller does not count: quarantine the second controller first.
    let od = om_core::crypto::hex(&other.device_id);
    let c = nonce(&mut w.rt, &w.ctx, "SuspendDevice", &od);
    let outcome = w.intent(
        "suspend",
        IntentBody::SuspendDevice {
            device_id: od,
            confirmation: c,
        },
    );
    assert_eq!(outcome, Some(OutcomeClass::Committed), "quarantine setup");
    {
        // The head moved: the pending request was bound to the old parent and is stale, so re-bind a new one.
        let h = w.head().hash();
        let req2 = w.request_for(h, 5);
        let id2 = World::digest(&req2, &w.policy());
        w.observe(&req2, &mono(1, H));
        let qv = w.veto_by(&other, &req2);
        w.rt.recovery_observe_veto(&w.ctx, &qv, &mono(1, 2 * H));
        assert_eq!(w.rec(&id2)["state"], "waiting");
    }
}

#[test]
fn invalid_stale_and_foreign_requests_are_ignored_with_no_state_change() {
    let mut w = world("obs-ignored");
    let before = w.app();
    let good = w.request();
    // Signed by some other recovery key.
    let other_root = RecoveryRoot::from_bytes([5; 32]);
    let forged = {
        let r = recovery::open_request(&good, w.policy().recovery_key().unwrap()).unwrap();
        sign_request(&r, &other_root.auth_signer(0), &mut w.rng)
    };
    // Bound to another parent state (stale).
    let stale_parent = w.request_for([3; 32], 9);
    // Another account.
    let foreign = {
        let mut r = recovery::open_request(&good, w.policy().recovery_key().unwrap()).unwrap();
        r.account_id = [8; 32];
        sign_request(&r, &w.root.auth_signer(0), &mut w.rng)
    };
    // Another epoch.
    let wrong_epoch = {
        let mut r = recovery::open_request(&good, w.policy().recovery_key().unwrap()).unwrap();
        r.recovery_epoch = 1;
        sign_request(&r, &w.root.auth_signer(0), &mut w.rng)
    };
    // Aimed at a device that already belongs to the account.
    let enrolled = {
        let mut r = recovery::open_request(&good, w.policy().recovery_key().unwrap()).unwrap();
        r.new_device_id = w.head().control.iter().next().copied().unwrap();
        sign_request(&r, &w.root.auth_signer(0), &mut w.rng)
    };
    for bad in [
        forged,
        stale_parent,
        foreign,
        wrong_epoch,
        enrolled,
        vec![1, 2, 3],
        vec![],
    ] {
        let step =
            w.rt.core
                .recovery_observe_request(&w.ctx, &bad, &mono(1, 0))
                .unwrap();
        assert!(step.commit.is_none(), "an ignored request must not commit");
    }
    assert_eq!(w.app(), before);
    assert!(w.status().requests.is_empty());
    // A malformed monotonic sample is an error, not an observation.
    assert!(
        w.rt.core
            .recovery_observe_request(&w.ctx, &good, &mono(1, 0).clone_with_empty_session())
            .is_err()
    );
}

trait EmptySession {
    fn clone_with_empty_session(&self) -> MonotonicSample;
}
impl EmptySession for MonotonicSample {
    fn clone_with_empty_session(&self) -> MonotonicSample {
        MonotonicSample {
            boot_session: vec![],
            elapsed_ms: self.elapsed_ms,
        }
    }
}

#[test]
fn a_request_goes_stale_when_the_account_moves_on() {
    let mut w = world("obs-stale");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    assert_eq!(
        state_of(&w.status(), &id),
        RecoveryRequestStateCode::Waiting
    );
    // A device is enrolled meanwhile: the head moves, the request's parent is gone.
    let (_, step) = w.rt.core.debug_link_messaging_device(&w.ctx).unwrap();
    assert!(w.rt.settle(&step));
    let v = w.status();
    assert_eq!(v.requests[0].state, RecoveryRequestStateCode::Stale);
    assert!(!v.requests[0].can_stop && v.requests[0].activates_at.is_none());
    w.tick(&mono(1, 10 * H));
    assert_eq!(w.rec(&id)["state"], "stale");
    assert_eq!(w.veto(&id), rejected(RejectReason::Conflict));
}

#[test]
fn tracked_requests_are_bounded() {
    let mut w = world("obs-bound");
    let h = w.head().hash();
    for n in 0..12u8 {
        let r = w.request_for(h, n);
        w.observe(&r, &mono(1, 0));
    }
    assert_eq!(w.tracked(), 8);
}

#[test]
fn veto_recovery_signs_records_and_queues_the_veto_only_for_a_live_controller() {
    let mut w = world("obs-vetoer");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    // Unknown request, and no confirmation, are refused.
    assert_eq!(w.veto(&"00".repeat(32)), rejected(RejectReason::NotFound));
    assert_eq!(
        w.intent(
            "bare",
            IntentBody::VetoRecovery {
                request_id: id.clone(),
                confirmation: serde_json::json!({"nonce": "nope"}),
            }
        ),
        rejected(RejectReason::Unauthorized)
    );
    assert_eq!(w.rec(&id)["state"], "waiting");
    // The only controller (this device) stops it.
    assert_eq!(w.veto(&id), Some(OutcomeClass::Committed));
    let rec = w.rec(&id);
    assert_eq!(rec["state"], "vetoed");
    assert_eq!(rec["ownVeto"], true);
    // The signed veto verifies against this device's credential and is queued for the (stub) transport.
    let raw = unhex(rec["vetoes"][0].as_str().unwrap()).unwrap();
    let me = w.head().control.iter().next().copied().unwrap();
    let pk = om_core::crypto::binding::Credential::from_body(&w.hexes("acsCredentials")[0])
        .unwrap()
        .signing_pk;
    let v = recovery::open_veto(&raw, &me, &pk).unwrap();
    assert_eq!(om_core::crypto::hex(&v.request_digest), id);
    let queued: Vec<_> =
        w.rt.core
            .net_ops(&w.ctx)
            .unwrap()
            .into_iter()
            .filter(|o| matches!(&o.call, NetCall::RecoveryVetoFanout { bytes } if *bytes == raw))
            .collect();
    assert_eq!(queued.len(), 1);
    // The stub transport refuses and Core drops the entry: nothing waits on it.
    w.rt.flush_net(&w.ctx);
    assert!(w.rt.core.net_ops(&w.ctx).unwrap().is_empty());
    // A second veto on a request already vetoed conflicts.
    assert_eq!(w.veto(&id), rejected(RejectReason::Conflict));
    assert_eq!(
        w.status().requests[0].state,
        RecoveryRequestStateCode::Vetoed
    );
}

#[test]
fn a_controller_cosign_completes_immediately() {
    let mut w = world("obs-cosign");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    // Nothing to co-sign until the recovering device's successor has arrived.
    assert_eq!(w.confirm(&id), rejected(RejectReason::Unavailable));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 60));
    // Interval barely started: the successor is held, not applied.
    assert_eq!(w.rec(&id)["state"], "waiting");
    assert!(w.rec(&id)["successor"].is_object());
    assert_eq!(w.confirm(&id), Some(OutcomeClass::Committed));
    let rec = w.rec(&id);
    assert_eq!(rec["state"], "completed");
    // The kept successor carries the recovery signature and this controller's.
    let child = unhex(rec["successor"]["child"].as_str().unwrap()).unwrap();
    let sigs = om_core::crypto::cbor::decode(&child).unwrap();
    assert_eq!(sigs.get(2).unwrap().a().unwrap().len(), 2);
    assert_eq!(
        state_of(&w.status(), &id),
        RecoveryRequestStateCode::Completed
    );
    let v = w.status();
    assert!(!v.requests[0].can_stop && v.requests[0].activates_at.is_none());
}

#[test]
fn a_cosign_with_two_controllers_completes_and_a_veto_still_wins() {
    let mut w = world("obs-cosign2");
    let other = w.second_controller();
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 0));
    // The other controller says "stop" before this one says "that was me": the veto wins.
    let v = w.veto_by(&other, &req);
    w.rt.recovery_observe_veto(&w.ctx, &v, &mono(1, 1));
    assert_eq!(w.confirm(&id), rejected(RejectReason::Conflict));
    assert_eq!(w.rec(&id)["state"], "vetoed");
}

#[test]
fn a_cosign_by_one_of_two_controllers_completes_immediately() {
    let mut w = world("obs-cosign-of-two");
    let _other = w.second_controller();
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 0));
    assert_eq!(w.rec(&id)["state"], "waiting");
    assert_eq!(w.confirm(&id), Some(OutcomeClass::Committed));
    assert_eq!(w.rec(&id)["state"], "completed");
}

#[test]
fn recovery_alone_completes_after_the_full_delay_when_the_successor_is_held() {
    let mut w = world("obs-alone");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    // Without the successor the delay only makes it ready.
    w.tick(&mono(1, DELAY_MS));
    let v = w.status();
    assert!(v.requests[0].ready);
    assert_eq!(v.requests[0].state, RecoveryRequestStateCode::Waiting);
    // The successor arrives after the interval: verified and applied at once.
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, DELAY_MS + 1));
    assert_eq!(w.rec(&id)["state"], "completed");
}

#[test]
fn a_bad_successor_is_ignored() {
    let mut w = world("obs-bad-successor");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let mut b = w.bundle();
    b.next_policy = w.policy().body(); // does not rotate the commitment
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, DELAY_MS));
    assert!(w.rec(&id)["successor"].is_null());
    assert_eq!(w.rec(&id)["state"], "waiting");
    let mut b = w.bundle();
    b.child[10] ^= 1; // corrupted signature
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, DELAY_MS));
    assert!(w.rec(&id)["successor"].is_null());
}

// ------------------------------------------------------------------------------- slice 4: chain, ruling, tests

#[test]
fn a_cosigned_completion_enters_the_chain_rotates_the_policy_and_replays() {
    let mut w = world("obs-chain-cosign");
    let before = w.head();
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    let next = b.next_policy.clone();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 60));
    assert_eq!(w.confirm(&id), Some(OutcomeClass::Committed));
    // The successor is the new head of the chain; the new device is the sole controller; the policy rotated.
    assert_eq!(w.hexes("acsChain").len(), 2);
    let head = w.head();
    assert_eq!(head.previous, Some(before.hash()));
    assert_eq!(head.control, BTreeSet::from([w.new_dev.device_id]));
    assert!(head.revoked.is_superset(&before.control));
    assert_eq!(
        unhex(w.app()["recoveryPolicy"].as_str().unwrap()).unwrap(),
        next
    );
    assert_eq!(head.recovery_commitment, Some(w.policy().hash()));
    assert_eq!(w.policy().epoch(), 1);
    // The link is only replayable with this verifier's own evidence.
    assert!(control::verify_chain(&w.hexes("acsChain"), &w.hexes("acsCredentials")).is_err());
    assert_eq!(w.app()["recoveryLinks"].as_array().unwrap().len(), 1);
    // It survives a restart (the status query re-verifies the stored chain).
    w.ctx = w.rt.restart().remove(0);
    assert_eq!(w.head().hash(), head.hash());
    assert_eq!(
        state_of(&w.status(), &id),
        RecoveryRequestStateCode::Completed
    );
    // This device is no longer a controller: it can neither stop nor confirm anything (nothing is waiting either).
    assert_eq!(w.veto(&id), rejected(RejectReason::Unauthorized));
}

#[test]
fn a_completion_after_the_delay_also_enters_the_chain() {
    let mut w = world("obs-chain-delay");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    w.tick(&mono(1, DELAY_MS));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, DELAY_MS + 1));
    assert_eq!(w.rec(&id)["state"], "completed");
    assert_eq!(w.hexes("acsChain").len(), 2);
    assert_eq!(w.head().control, BTreeSet::from([w.new_dev.device_id]));
    assert_eq!(w.links()[0].elapsed_seconds, 72 * 3600);
}

#[test]
fn a_tampered_link_evidence_makes_the_chain_unverifiable_not_wrongly_verified() {
    let mut w = world("obs-chain-tamper");
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 60));
    assert_eq!(w.confirm(&id), Some(OutcomeClass::Committed));
    let mut ev = w.links();
    ev[0].request[3] ^= 1;
    assert!(
        control::verify_chain_with(&w.hexes("acsChain"), &w.hexes("acsCredentials"), &ev).is_err()
    );
    // An empty evidence list (a peer's view) refuses the recovery link too.
    assert!(
        control::verify_chain_with(&w.hexes("acsChain"), &w.hexes("acsCredentials"), &[]).is_err()
    );
}

#[test]
fn n4_controllers_one_cosign_is_below_the_majority_and_is_refused_ruling_a() {
    // RECOVERY 10.3 "one co-sign completes immediately" and 10.4 "one vote toward floor(N/2)+1" cannot both hold for
    // N = 4 (recovery + one co-signer = 2 < 3). Coordinating ruling (a): keep the majority rule, reject, record it.
    let mut w = world("obs-n4");
    let _others = w.more_controllers(3);
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    let b = w.bundle();
    w.rt.recovery_observe_successor(&w.ctx, &id, &b, &mono(1, 0));
    assert_eq!(w.confirm(&id), rejected(RejectReason::Conflict));
    assert_eq!(w.rec(&id)["state"], "waiting");
    assert_eq!(w.hexes("acsChain").len(), 2); // genesis + the controller link only: nothing recovered
    // It falls to the delay path: after the full verifier-local interval, with no veto, it completes (RECOVERY 10.3/10.7).
    w.tick(&mono(1, DELAY_MS));
    assert_eq!(w.rec(&id)["state"], "completed");
    assert_eq!(w.hexes("acsChain").len(), 3);
}

#[test]
fn a_veto_by_a_quarantined_self_is_refused() {
    let mut w = world("obs-quarantined-self");
    let devs = w.more_controllers(2);
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    w.observe(&req, &mono(1, 0));
    // The two other controllers (a majority of three) quarantine this device.
    let head = w.head();
    let me = *head
        .control
        .iter()
        .find(|c| devs.iter().all(|d| d.device_id != **c))
        .unwrap();
    let mut child = head.clone();
    child.generation += 1;
    child.previous = Some(head.hash());
    child.quarantined.insert(me);
    let signed = control::sign_acs(
        &child.body(),
        &[
            (&devs[0].device_id, &devs[0].signer),
            (&devs[1].device_id, &devs[1].signer),
        ],
        &mut w.rng,
    );
    assert!(control::verify_successor(&head, &signed, &w.hexes("acsCredentials")).is_ok());
    w.rewrite(|app| {
        app["acsChain"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(om_core::crypto::hex(&signed)));
    });
    assert_eq!(w.veto(&id), rejected(RejectReason::Unauthorized));
    assert_eq!(w.confirm(&id), rejected(RejectReason::Unauthorized));
}

#[test]
fn a_veto_before_its_request_is_dropped_and_counts_when_the_host_redelivers_it() {
    // Decision (recorded in the plan report): a veto naming a request this verifier has not tracked is ignored with no
    // state; the Home redelivers (RecoveryVetoFanout is idempotent), so the veto counts once the request is observed.
    let mut w = world("obs-veto-first");
    let other = w.second_controller();
    let req = w.request();
    let id = World::digest(&req, &w.policy());
    let v = w.veto_by(&other, &req);
    w.rt.recovery_observe_veto(&w.ctx, &v, &mono(1, 0));
    assert_eq!(w.tracked(), 0);
    w.observe(&req, &mono(1, 1));
    assert_eq!(w.rec(&id)["state"], "waiting");
    w.rt.recovery_observe_veto(&w.ctx, &v, &mono(1, 2)); // redelivery
    assert_eq!(w.rec(&id)["state"], "vetoed");
}

#[test]
fn a_profile_without_a_matching_stored_policy_reports_pending() {
    let mut w = world("obs-legacy");
    let ok: RecoveryStatusView = w.status();
    // The recovery key is not yet confirmed stored, so even a fresh profile is pending until it is.
    assert_eq!(ok.policy, RecoveryPolicyState::Pending);
    let op = "stored";
    let out = w.intent(
        op,
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "create".into(),
        },
    );
    assert_eq!(out, Some(OutcomeClass::Committed));
    assert_eq!(w.status().policy, RecoveryPolicyState::On);
    // A profile made before recovery policies existed has no stored policy body: pending, however saved the key is.
    w.rewrite(|app| {
        app.as_object_mut().unwrap().remove("recoveryPolicy");
    });
    let legacy = w.status();
    assert_eq!(legacy.policy, RecoveryPolicyState::Pending);
    assert!(legacy.recovery_key_saved);
    // A stored policy that the head does not commit to (another epoch) is pending as well.
    let other = RecoveryPolicy::default_for(w.root.auth_public_key(1), 1).body();
    w.rewrite(|app| app["recoveryPolicy"] = serde_json::json!(om_core::crypto::hex(&other)));
    assert_eq!(w.status().policy, RecoveryPolicyState::Pending);
}

// ------------------------------------------------------------------------------- recovering device (BeginRecovery)

#[test]
fn begin_recovery_keeps_the_checked_key_in_a_restricted_state_with_no_authority() {
    // Person A made an identity and saved the recovery key; on a new device they type it back (lower case, spaces).
    let net = LoopbackNetwork::default();
    let mut a = Runtime::new("begin-a", &net);
    let actx = a.open("p");
    a.dispatch(
        &actx,
        "create",
        IntentBody::CreateIdentityProfile { label: "Me".into() },
    );
    let key: RecoveryKeyView = a
        .query::<Option<RecoveryKeyView>>(Query::GetRecoveryKey {
            ctx: actx.clone(),
            key_operation_id: "create".into(),
        })
        .unwrap();
    let typed = key.groups.join(" ").to_lowercase();

    let mut b = Runtime::new("begin-b", &net);
    let ctx = b.open("q");
    let begin = |b: &mut Runtime, op: &str, k: &str, home: &str| {
        b.dispatch(
            &ctx,
            op,
            IntentBody::BeginRecovery {
                recovery_key: k.into(),
                home: home.into(),
            },
        );
        b.outcome(&ctx, op)
    };
    let bad = Some(OutcomeClass::Rejected {
        reason: RejectReason::Invalid,
    });
    // A wrong check, a short key and a non-URL are refused with no state.
    let mut wrong = key.groups.clone();
    wrong[0] = if wrong[0].starts_with('A') {
        "BBBB".into()
    } else {
        "AAAA".into()
    };
    assert_eq!(
        begin(&mut b, "o1", &wrong.join(" "), "https://home.example"),
        bad
    );
    assert_eq!(
        begin(
            &mut b,
            "o2",
            &key.groups[..13].join(" "),
            "https://home.example"
        ),
        bad
    );
    assert_eq!(begin(&mut b, "o3", &typed, "not a url"), bad);
    let st: RecoveryStatusView = b.query(Query::GetRecoveryStatus { ctx: ctx.clone() });
    assert!(st.recovering.is_none());
    // The right key and a Home address: the restricted recovery state, no authority, still pending.
    assert_eq!(
        begin(&mut b, "o4", &typed, "https://home.example"),
        Some(OutcomeClass::Committed)
    );
    let st: RecoveryStatusView = b.query(Query::GetRecoveryStatus { ctx: ctx.clone() });
    assert_eq!(
        st.recovering,
        Some(RecoveringView {
            home: "https://home.example".into(),
            restricted: true
        })
    );
    assert_eq!(st.policy, RecoveryPolicyState::Pending);
    // The root is kept for the request to come; a second attempt conflicts.
    assert_eq!(
        begin(&mut b, "o5", &typed, "https://home.example"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Conflict
        })
    );
    // It survives a restart.
    let ctx2 = b.restart().remove(0);
    let st: RecoveryStatusView = b.query(Query::GetRecoveryStatus { ctx: ctx2 });
    assert!(st.recovering.is_some());
}
