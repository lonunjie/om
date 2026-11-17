// SPDX-License-Identifier: MPL-2.0
//! Contact-side recovery verification (ADR-0190): a contact that already pins an account's head learns of a
//! recovery-only request from a `RecoveryNotice`, runs its OWN interval from its own first valid observation, holds
//! what the new device sends until then, and accepts the successor only after that interval with no veto. Evidence
//! travels; time never does. Core is driven as a host would: deliveries are polled, monotonic readings are handed in.

mod common;

use common::confirmation as nonce;
use om_core::api::*;
use om_core::crypto::binding::{Acs, PeerView};
use om_core::crypto::cbor::{self, V};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::{delivery, sha256, unhex};
use om_core::host::{MonotonicSample, NetCall, NetResult, Step};
use om_core::recovery::{
    self, RecoveryPolicy, RecoveryRequest, RecoveryRoot, SuccessorBundle, sign_request,
};
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};
use om_core::{build_notice, control, events};
use std::collections::BTreeSet;

const H: u64 = 3_600_000;
const DELAY_MS: u64 = 72 * H;

fn mono(session: u8, ms: u64) -> MonotonicSample {
    MonotonicSample {
        boot_session: vec![session; 8],
        elapsed_ms: ms,
    }
}

fn s(v: &serde_json::Value) -> String {
    v.as_str().unwrap().to_string()
}

fn root_from_groups(groups: &[String]) -> RecoveryRoot {
    const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = String::new();
    for c in groups.concat().chars() {
        bits.push_str(&format!("{:05b}", A.find(c).unwrap()));
    }
    let bytes: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap())
        .collect();
    RecoveryRoot::from_bytes(bytes.try_into().unwrap())
}

/// One contact: a runtime, its profile and the conversation with Bob.
struct Contact {
    rt: Runtime,
    ctx: ProfileExecutionContext,
    id: String,
}

struct World {
    net: LoopbackNetwork,
    bob: Runtime,
    b: ProfileExecutionContext,
    root: RecoveryRoot,
    rng: InsecureTestRng,
    new_dev: Device,
    contacts: Vec<Contact>,
    seal_n: u64,
}

/// Bob (the account that will be recovered) with `n` active contacts, all started from one shared network.
fn world(n: usize) -> World {
    let net = LoopbackNetwork::default();
    let mut bob = Runtime::new("bob", &net);
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    let key: Option<RecoveryKeyView> = bob.query(Query::GetRecoveryKey {
        ctx: b.clone(),
        key_operation_id: "create-1".into(),
    });
    let root = root_from_groups(&key.expect("recovery key").groups);
    let mut contacts = vec![];
    for i in 0..n {
        let mut rt = Runtime::new(&format!("c{i}"), &net);
        let ctx = common::create_profile(&mut rt, "p-c", &format!("C{i}"));
        let op = format!("inv{i}");
        bob.dispatch(&b, &op, IntentBody::CreateInvite);
        bob.flush_net(&b);
        let link = bob
            .query::<Option<InviteView>>(Query::GetInvite {
                ctx: b.clone(),
                operation_id: op,
            })
            .unwrap()
            .link;
        rt.dispatch(&ctx, "acc", IntentBody::AcceptInvitation { link });
        settle_all(&mut [(&mut bob, &b), (&mut rt, &ctx)]);
        let intros: Vec<IntroductionRequest> =
            bob.query(Query::GetIntroductions { ctx: b.clone() });
        bob.dispatch(
            &b,
            &format!("i{i}"),
            IntentBody::AcceptIntroduction {
                request_id: intros[0].id.clone(),
            },
        );
        settle_all(&mut [(&mut bob, &b), (&mut rt, &ctx)]);
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: ctx.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        let id = l[0].id.clone();
        contacts.push(Contact { rt, ctx, id });
    }
    let mut rng = InsecureTestRng::from_label(b"recovery-peer");
    let mut new_dev = Device::new(&mut rng);
    let acct = {
        let st = bob.core.debug_app_state(&b).unwrap();
        let chain: Vec<Vec<u8>> = st["acsChain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| unhex(v.as_str().unwrap()).unwrap())
            .collect();
        let creds = hexes(&st, "acsCredentials");
        control::verify_chain(&chain, &creds).unwrap().account_id
    };
    new_dev.account_id = acct;
    World {
        net,
        bob,
        b,
        root,
        rng,
        new_dev,
        contacts,
        seal_n: 0,
    }
}

fn hexes(st: &serde_json::Value, k: &str) -> Vec<Vec<u8>> {
    st[k]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| unhex(v.as_str().unwrap()).unwrap())
        .collect()
}

impl Contact {
    fn rel(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()["relationships"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone()
    }
    fn recs(&self) -> Vec<serde_json::Value> {
        self.rel()["peerRecovery"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
    fn state(&self) -> String {
        s(&self.recs()[0]["state"])
    }
    fn credited(&self) -> u64 {
        self.recs()[0]["obs"]["creditedMs"].as_u64().unwrap()
    }
    fn head_gen(&self) -> u64 {
        Acs::from_body(&unhex(&s(&self.rel()["peerAcsBody"])).unwrap())
            .unwrap()
            .generation
    }
    fn held(&self) -> usize {
        self.rel()["held"].as_array().unwrap().len()
    }
    fn failures(&self) -> u64 {
        self.rel()["integrityFailures"].as_u64().unwrap()
    }
    fn tick(&mut self, m: &MonotonicSample) {
        self.rt.recovery_tick(&self.ctx, m);
    }
    fn timeline(&self) -> Vec<String> {
        let v: ConversationView = self.rt.query(Query::GetConversationView {
            ctx: self.ctx.clone(),
            conversation_id: self.id.clone(),
        });
        v.timeline
            .into_iter()
            .map(|t| match t {
                TimelineItem::Incoming { text, .. } | TimelineItem::Outgoing { text, .. } => text,
            })
            .collect()
    }
    fn view(&self) -> Option<PeerRecoveryView> {
        let v: ConversationView = self.rt.query(Query::GetConversationView {
            ctx: self.ctx.clone(),
            conversation_id: self.id.clone(),
        });
        v.summary.peer_recovery
    }
    /// Rewrite the stored app state with `f`, then restart over the edited record.
    fn rewrite(&mut self, f: impl FnOnce(&mut serde_json::Value)) {
        let rec = self
            .rt
            .store
            .records("p-c")
            .into_iter()
            .find(|r| r.key == "state")
            .unwrap();
        let v = cbor::decode(&rec.value).unwrap();
        let mut app: serde_json::Value =
            serde_json::from_slice(v.get(1).unwrap().b().unwrap()).unwrap();
        f(&mut app);
        let new = cbor::encode(&V::M(vec![
            (0, v.get(0).unwrap().clone()),
            (1, V::B(serde_json::to_vec(&app).unwrap())),
        ]));
        self.rt.store.replace_record("p-c", "state", new);
        self.ctx = self.rt.restart().remove(0);
    }
    fn restart(&mut self) {
        self.ctx = self.rt.restart().remove(0);
    }
    fn intent(&mut self, op: &str, body: IntentBody) -> Option<OutcomeClass> {
        self.rt.dispatch(&self.ctx, op, body);
        self.rt.outcome(&self.ctx, op)
    }
}

impl World {
    fn bob_state(&self) -> serde_json::Value {
        self.bob.core.debug_app_state(&self.b).unwrap()
    }
    fn chain(&self) -> Vec<Vec<u8>> {
        hexes(&self.bob_state(), "acsChain")
    }
    fn creds(&self) -> Vec<Vec<u8>> {
        hexes(&self.bob_state(), "acsCredentials")
    }
    fn head(&self) -> Acs {
        control::verify_chain(&self.chain(), &self.creds()).unwrap()
    }
    fn policy(&self) -> RecoveryPolicy {
        RecoveryPolicy::from_body(
            &unhex(self.bob_state()["recoveryPolicy"].as_str().unwrap()).unwrap(),
        )
        .unwrap()
    }
    fn request_with(
        &mut self,
        parent: [u8; 32],
        epoch: u64,
        nonce: u8,
        signer_epoch: u64,
    ) -> Vec<u8> {
        let req = RecoveryRequest {
            account_id: self.head().account_id,
            recovery_epoch: epoch,
            new_device_id: self.new_dev.device_id,
            new_device_credential: self.new_dev.credential().hash(),
            parent_state_hash: parent,
            request_nonce: [nonce; 16],
        };
        sign_request(&req, &self.root.auth_signer(signer_epoch), &mut self.rng)
    }
    fn request(&mut self) -> Vec<u8> {
        let h = self.head().hash();
        self.request_with(h, 0, 9, 0)
    }
    fn digest(&self, req: &[u8]) -> String {
        om_core::crypto::hex(
            &recovery::open_request(req, self.policy().recovery_key().unwrap())
                .unwrap()
                .digest(),
        )
    }
    fn next_policy(&self) -> RecoveryPolicy {
        let mut p = RecoveryPolicy::default_for(self.root.auth_public_key(1), 1);
        p.delay_seconds = self.policy().delay_seconds;
        p
    }
    /// The recovering device's recovery-signed successor of Bob's current head (no controller co-signature).
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
    fn notice(&self, req: &[u8], bundle: Option<&SuccessorBundle>) -> Vec<u8> {
        build_notice(&self.policy().body(), req, &[], &[], bundle, &[]).unwrap()
    }
    /// Seal `inner` to contact `i`'s current delivery key and first route, as any sender would, and deposit it.
    fn deposit(&mut self, i: usize, inner: &[u8]) {
        self.seal_n += 1;
        let ar = self.contacts[i].rel();
        let key = &ar["myDelivery"][0]["public"];
        let route = &ar["myBoxes"][0]["route"];
        let mut rng = InsecureTestRng::from_label(format!("sealer-{}", self.seal_n).as_bytes());
        let w = delivery::seal(
            &unhex(&s(&key["publicKey"])).unwrap(),
            &unhex(&s(&key["keyId"])).unwrap().try_into().unwrap(),
            &s(&route["relayUri"]),
            &unhex(&s(&route["capability"])).unwrap(),
            key["expiresAt"].as_u64().unwrap().min(1_790_000_000 + 3600),
            inner,
            &mut rng,
        )
        .expect("seal");
        let r = self.net.perform(&NetCall::Deposit {
            mailbox: unhex(&s(&route["capability"])).unwrap(),
            relay_uri: Some(s(&route["relayUri"])),
            bytes: w,
        });
        assert!(matches!(r, NetResult::Ok), "{r:?}");
    }
    /// Deposit a notice for contact `i` and let it poll.
    fn deliver(&mut self, i: usize, inner: &[u8]) {
        self.deposit(i, inner);
        self.poll(i);
    }
    fn poll(&mut self, i: usize) {
        let c = &mut self.contacts[i];
        c.rt.poll(&c.ctx);
    }
    /// Let Bob and contact `i` exchange everything they owe each other.
    fn settle(&mut self, i: usize) {
        let (ctx, b) = (self.contacts[i].ctx.clone(), self.b.clone());
        settle_all(&mut [(&mut self.contacts[i].rt, &ctx), (&mut self.bob, &b)]);
    }
    /// A message from the new device to contact 0, sealed as any sender would (the session starts from the contact's
    /// published key material, as in `peer_head`).
    fn new_device_message(&mut self, first: bool, text: &str) -> Vec<u8> {
        let br = self.bob.core.debug_app_state(&self.b).unwrap()["relationships"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        let ar = self.contacts[0].rel();
        let alice_dev = unhex(&s(&br["peerDeviceId"])).unwrap();
        let alice_ik: [u8; 32] = unhex(&s(&br["peerIk"])).unwrap().try_into().unwrap();
        if first {
            let cap = vec![9u8; 32];
            self.net.perform(&NetCall::GrantKeyFetch {
                device_id: alice_dev.clone(),
                capability: cap.clone(),
            });
            let NetResult::Many { items } = self.net.perform(&NetCall::FetchKeyMaterial {
                device_id: alice_dev,
                service: None,
                capability: cap,
            }) else {
                panic!("key material")
            };
            let acs = unhex(&s(&br["peerAcsBody"])).unwrap();
            let cred = unhex(&s(&br["peerCredentials"][0])).unwrap();
            let view = PeerView {
                trusted_acs_hash: om_core::crypto::sig::ds_hash(
                    om_core::crypto::binding::LBL_ACS_HASH,
                    &acs,
                ),
                acs_body: &acs,
                credential_body: &cred,
                now: 1_790_000_100,
            };
            self.new_dev
                .start_session(&items, &view, &mut self.rng)
                .expect("session");
        }
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&sha256(&[text.as_bytes()])[..4]);
        let canon: [u8; 32] = unhex(&s(&ar["canonId"])).unwrap().try_into().unwrap();
        let payload = events::Header {
            event_id: id,
            conversation_id: canon,
            author_account: self.new_dev.account_id,
            author_device: self.new_dev.device_id,
            clock: events::Hlc {
                physical_ms: 1_790_000_000_500,
                logical: 0,
            },
            kind: events::K_CREATE,
            body: events::message_create_body(text),
        }
        .encode();
        let (env, _) = self.new_dev.encrypt(&alice_ik, &payload).expect("encrypt");
        env
    }
}

#[test]
fn a_notice_starts_a_waiting_record_holds_the_new_device_and_accepts_only_after_the_full_delay() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let digest = w.digest(&req);
    // Before the notice the new device's message is held like any unknown device's: no plaintext, no failure.
    let env = w.new_device_message(true, "from the new phone");
    w.deliver(0, &env);
    assert_eq!(w.contacts[0].held(), 1);
    assert_eq!(w.contacts[0].failures(), 0);
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let c = &mut w.contacts[0];
    assert_eq!(c.state(), "waiting");
    assert_eq!(c.recs()[0]["obs"]["digest"], digest);
    assert_eq!(c.failures(), 0);
    assert_eq!(
        c.head_gen(),
        1,
        "the pinned head does not move before the interval ran"
    );
    // The interval is the contact's own: the first tick sets the baseline, later ticks credit monotonic time.
    c.tick(&mono(1, 5 * H));
    assert_eq!(
        c.credited(),
        0,
        "a delivery brings no clock: the baseline is the first reading"
    );
    c.tick(&mono(1, 5 * H + DELAY_MS - 1));
    assert_eq!(
        c.state(),
        "waiting",
        "one millisecond short is still waiting"
    );
    assert_eq!(c.head_gen(), 1);
    assert!(
        c.timeline().is_empty(),
        "nothing from the new device is shown"
    );
    assert_eq!(c.held(), 1, "and it is still held, unprocessed");
    let v = c
        .view()
        .expect("the conversation view carries the recovery");
    assert_eq!(v.state, om_core::state::PeerRecoveryState::Waiting);
    assert_eq!(v.days_left, 1);
    assert!(v.can_decline && !v.needs_confirm);
    c.tick(&mono(1, 5 * H + DELAY_MS));
    assert_eq!(c.state(), "accepted");
    assert_eq!(c.head_gen(), 2);
    assert_eq!(c.rel()["peerLinks"].as_array().unwrap().len(), 1);
    assert_eq!(
        c.rel()["peerLinks"][0]["elapsedSeconds"].as_u64().unwrap(),
        72 * 3600,
        "this verifier's own credited seconds are kept locally"
    );
    // The held message is processed only now.
    assert_eq!(c.held(), 0);
    assert_eq!(c.timeline(), vec!["from the new phone".to_string()]);
}

#[test]
fn two_contacts_each_enforce_their_own_interval_from_their_own_first_observation() {
    let mut w = world(2);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    // Both are sent the notice, over the real sealed deposit path of the account's own device.
    let queued = w.bob.recovery_send_notices(&w.b, &req, Some(&bundle));
    assert_eq!(queued, 2);
    w.bob.flush_net(&w.b);
    // Contact 0 observes it live and starts its interval; contact 1 is offline and does not poll until later.
    w.poll(0);
    assert_eq!(w.contacts[0].state(), "waiting");
    assert!(
        w.contacts[1].recs().is_empty(),
        "contact 1 has seen nothing yet"
    );
    w.contacts[0].tick(&mono(1, 0));
    w.contacts[0].tick(&mono(1, 40 * H));
    // 40 hours later contact 1 comes online and sees the same notice for the first time (a resend is also seen).
    w.poll(1);
    w.deliver(1, &n);
    assert_eq!(w.contacts[1].state(), "waiting");
    w.contacts[1].tick(&mono(7, 0));
    w.contacts[0].tick(&mono(1, DELAY_MS));
    assert_eq!(
        w.contacts[0].state(),
        "accepted",
        "contact 0 ran its own 72 hours"
    );
    // 32 hours later (72 hours after contact 0 started, 32 after contact 1 did) contact 1 still waits.
    w.contacts[1].tick(&mono(7, 32 * H));
    assert_eq!(w.contacts[1].state(), "waiting");
    assert_eq!(
        w.contacts[1].head_gen(),
        1,
        "contact 0's elapsed time is not contact 1's"
    );
    w.contacts[1].tick(&mono(7, DELAY_MS - 1));
    assert_eq!(w.contacts[1].state(), "waiting");
    w.contacts[1].tick(&mono(7, DELAY_MS));
    assert_eq!(w.contacts[1].state(), "accepted");
}

#[test]
fn the_timer_survives_a_restart_and_a_lost_record_restarts_it() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let c = &mut w.contacts[0];
    c.tick(&mono(1, 0));
    c.tick(&mono(1, 40 * H));
    assert_eq!(c.credited(), 40 * H);
    // Crash and restart: a new boot session. Downtime credits nothing, the checkpoint credit stays.
    c.restart();
    assert_eq!(c.credited(), 40 * H, "the credit was checkpointed");
    c.tick(&mono(2, 1_000_000));
    assert_eq!(
        c.credited(),
        40 * H,
        "downtime between boot sessions is not credited"
    );
    c.tick(&mono(2, 1_000_000 + 31 * H));
    assert_eq!(c.state(), "waiting");
    c.tick(&mono(2, 1_000_000 + 32 * H));
    assert_eq!(
        c.state(),
        "accepted",
        "the checkpoint plus the new session's own time"
    );
    // A lost record: nothing is credited, the next valid observation restarts the interval.
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    w.contacts[0].tick(&mono(1, 0));
    w.contacts[0].tick(&mono(1, 70 * H));
    w.contacts[0].rewrite(|app| {
        for r in app["relationships"].as_object_mut().unwrap().values_mut() {
            r["peerRecovery"] = serde_json::json!([]);
        }
    });
    assert!(w.contacts[0].recs().is_empty());
    let n2 = w.notice(&req, Some(&bundle));
    w.deliver(0, &n2);
    assert_eq!(w.contacts[0].state(), "waiting");
    w.contacts[0].tick(&mono(3, 0));
    w.contacts[0].tick(&mono(3, 71 * H));
    assert_eq!(
        w.contacts[0].state(),
        "waiting",
        "restarted from zero, never earlier"
    );
    w.contacts[0].tick(&mono(3, DELAY_MS));
    assert_eq!(w.contacts[0].state(), "accepted");
}

#[test]
fn nothing_but_the_hosts_monotonic_reading_moves_the_interval() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    w.contacts[0].tick(&mono(1, 100));
    w.contacts[0].tick(&mono(1, 100 + 10 * H));
    // A wall clock jump of a week, a Home timestamp (the notice's resend) and a second delivery of the same digest.
    w.contacts[0].rt.tick(7 * 24 * H);
    w.deliver(0, &n);
    let n_again = w.notice(&req, None);
    w.deliver(0, &n_again);
    assert_eq!(w.contacts[0].recs().len(), 1, "one digest, one record");
    assert_eq!(
        w.contacts[0].credited(),
        10 * H,
        "a redelivery never resets or advances it"
    );
    // A backward reading credits nothing; a reading in another boot session only sets a new baseline.
    w.contacts[0].tick(&mono(1, 50));
    assert_eq!(w.contacts[0].credited(), 10 * H);
    w.contacts[0].tick(&mono(9, 1));
    assert_eq!(w.contacts[0].credited(), 10 * H);
    assert_eq!(w.contacts[0].state(), "waiting");
}

#[test]
fn a_forged_elapsed_time_and_a_time_like_extension_are_refused_without_a_trace() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let base = w.notice(&req, Some(&bundle));
    let head_before = w.contacts[0].rel()["peerAcsBody"].clone();
    let with_ext = |name: &str, value: V| {
        let V::M(mut f) = cbor::decode(&base).unwrap() else {
            panic!("map")
        };
        f.push((
            23,
            V::TM(vec![(
                name.to_string(),
                V::M(vec![(0, V::Bool(false)), (1, value)]),
            )]),
        ));
        cbor::encode(&V::M(f))
    };
    for name in [
        "elapsed_seconds",
        "x-delay",
        "credited_ms",
        "SinceMs",
        "wall.time",
        "ts",
    ] {
        w.deliver(0, &with_ext(name, V::U(999_999_999)));
        assert!(w.contacts[0].recs().is_empty(), "{name}: no record");
        assert_eq!(w.contacts[0].rel()["peerAcsBody"], head_before);
    }
    assert!(w.contacts[0].failures() >= 6, "each was a counted refusal");
    // An ordinary noncritical extension is preserved and never read: the notice still works.
    w.deliver(0, &with_ext("label", V::T("hello".into())));
    assert_eq!(w.contacts[0].state(), "waiting");
    assert_eq!(
        w.contacts[0].credited(),
        0,
        "the extension credited nothing"
    );
}

#[test]
fn evidence_that_does_not_verify_creates_no_state() {
    let mut w = world(1);
    let head = w.head();
    let good = w.request();
    let bundle = w.bundle();
    let policy_body = w.policy().body();
    let head_before = w.contacts[0].rel()["peerAcsBody"].clone();
    let mut other = w.policy();
    other.delay_seconds += 3600; // a policy that does not hash to the committed one
    let signed_by_other_key = w.request_with(head.hash(), 0, 3, 1);
    let epoch_one = w.request_with(head.hash(), 1, 4, 0);
    let other_parent = w.request_with([7u8; 32], 0, 5, 0);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "policy hash",
            build_notice(&other.body(), &good, &[], &[], None, &[]).unwrap(),
        ),
        (
            "other key",
            build_notice(&policy_body, &signed_by_other_key, &[], &[], None, &[]).unwrap(),
        ),
        (
            "other epoch",
            build_notice(&policy_body, &epoch_one, &[], &[], None, &[]).unwrap(),
        ),
        (
            "other parent",
            build_notice(&policy_body, &other_parent, &[], &[], None, &[]).unwrap(),
        ),
        (
            "junk request",
            build_notice(&policy_body, &[1, 2, 3], &[], &[], None, &[]).unwrap(),
        ),
        ("successor for another request", {
            let mut b = bundle.clone();
            b.credential = vec![0u8; 8];
            build_notice(&policy_body, &good, &[], &[], Some(&b), &[]).unwrap()
        }),
    ];
    for (name, bytes) in cases {
        let skip_state = name == "successor for another request";
        w.deliver(0, &bytes);
        if skip_state {
            // A valid request with a bad successor still tracks the request, but holds no successor.
            assert_eq!(w.contacts[0].recs().len(), 1);
            assert!(w.contacts[0].recs()[0]["obs"]["successor"].is_null());
            continue;
        }
        assert!(w.contacts[0].recs().is_empty(), "{name}");
        assert_eq!(w.contacts[0].rel()["peerAcsBody"], head_before, "{name}");
    }
    // Oversize and a bridge of states of another chain are refused by the codec / the verifier.
    let big = {
        let V::M(mut f) = cbor::decode(&w.notice(&good, None)).unwrap() else {
            panic!()
        };
        f.push((5, V::A(vec![V::B(vec![0u8; 32_769])])));
        cbor::encode(&V::M(f))
    };
    let before = w.contacts[0].failures();
    w.deliver(0, &big);
    assert!(w.contacts[0].failures() > before);
}

#[test]
fn a_contact_behind_by_two_ordinary_states_is_bridged() {
    let mut w = world(1);
    // Bob links two devices; his updates stay in his outbox, so the contact still pins generation 1.
    for i in 0..2u8 {
        let mut rng = InsecureTestRng::from_label(&[b'x', i]);
        let mut d = Device::new(&mut rng);
        d.account_id = w.head().account_id;
        let (_, step) = w
            .bob
            .core
            .debug_link_credential(&w.b, &d.credential().body())
            .unwrap();
        w.bob.settle(&step);
    }
    assert_eq!(w.contacts[0].head_gen(), 1);
    assert_eq!(w.head().generation, 3);
    let req = w.request(); // bound to Bob's generation 3 head
    let bundle = w.bundle();
    let n = build_notice(
        &w.policy().body(),
        &req,
        &w.chain(),
        &w.creds(),
        Some(&bundle),
        &[],
    )
    .unwrap();
    w.deliver(0, &n);
    assert_eq!(
        w.contacts[0].head_gen(),
        3,
        "the bridge was adopted by the ordinary refresh rule"
    );
    assert_eq!(w.contacts[0].state(), "waiting");
    // A notice whose bridge stops short of the request's parent is refused and moves nothing.
    let mut w2 = world(1);
    for i in 0..2u8 {
        let mut rng = InsecureTestRng::from_label(&[b'y', i]);
        let mut d = Device::new(&mut rng);
        d.account_id = w2.head().account_id;
        let (_, step) = w2
            .bob
            .core
            .debug_link_credential(&w2.b, &d.credential().body())
            .unwrap();
        w2.bob.settle(&step);
    }
    let req = w2.request();
    let short = build_notice(
        &w2.policy().body(),
        &req,
        &w2.chain()[..2],
        &w2.creds(),
        None,
        &[],
    )
    .unwrap();
    w2.deliver(0, &short);
    assert!(w2.contacts[0].recs().is_empty());
    assert_eq!(
        w2.contacts[0].head_gen(),
        1,
        "nothing adopted when the notice did not verify"
    );
}

#[test]
fn a_veto_by_a_live_controller_ends_it_and_nobody_else_can() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let digest = w.digest(&req);
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let env = w.new_device_message(true, "stop me");
    w.deliver(0, &env);
    assert_eq!(w.contacts[0].held(), 1);
    w.contacts[0].tick(&mono(1, 0));
    // A veto signed by someone who is not a controller of the pinned head does not count.
    let head = w.head();
    let mut rng = InsecureTestRng::from_label(b"stranger");
    let stranger = Device::new(&mut rng);
    let forged = recovery::sign_veto(
        &recovery::RecoveryVeto {
            account_id: head.account_id,
            recovery_epoch: 0,
            request_digest: unhex(&digest).unwrap().try_into().unwrap(),
        },
        &stranger.device_id,
        &stranger.signer,
        &mut rng,
    );
    let n_forged = build_notice(&w.policy().body(), &req, &[], &[], None, &[forged]).unwrap();
    w.deliver(0, &n_forged);
    assert_eq!(
        w.contacts[0].state(),
        "waiting",
        "a non-controller's veto is ignored"
    );
    // Bob's own live device vetoes: the veto is pushed over the ordinary session to the contact.
    w.bob.recovery_observe_request(&w.b, &req, &mono(1, 0));
    let c = nonce(&mut w.bob, &w.b, "VetoRecovery", &digest);
    w.bob.dispatch(
        &w.b,
        "veto-1",
        IntentBody::VetoRecovery {
            request_id: digest.clone(),
            confirmation: c,
        },
    );
    assert_eq!(w.bob.outcome(&w.b, "veto-1"), Some(OutcomeClass::Committed));
    w.settle(0);
    let c = &mut w.contacts[0];
    assert_eq!(c.state(), "vetoed");
    assert_eq!(c.held(), 0, "what the new device sent is dropped");
    assert!(c.timeline().is_empty());
    c.tick(&mono(1, 2 * DELAY_MS));
    assert_eq!(
        c.state(),
        "vetoed",
        "nothing ever completes a vetoed request"
    );
    assert_eq!(c.head_gen(), 1);
    // A veto that arrives inside a notice does the same (a fresh world, no live push).
    let mut w = world(1);
    let req = w.request();
    let digest = w.digest(&req);
    w.bob.recovery_observe_request(&w.b, &req, &mono(1, 0));
    let cn = nonce(&mut w.bob, &w.b, "VetoRecovery", &digest);
    w.bob.dispatch(
        &w.b,
        "veto-2",
        IntentBody::VetoRecovery {
            request_id: digest.clone(),
            confirmation: cn,
        },
    );
    let veto = w.bob_state()["recoveryRequests"][&digest]["vetoes"][0]
        .as_str()
        .map(|v| unhex(v).unwrap())
        .unwrap();
    let n = build_notice(&w.policy().body(), &req, &[], &[], None, &[veto]).unwrap();
    w.deliver(0, &n);
    assert_eq!(w.contacts[0].state(), "vetoed");
}

#[test]
fn a_moved_on_account_makes_the_request_stale() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let env = w.new_device_message(true, "late");
    w.deliver(0, &env);
    assert_eq!(w.contacts[0].held(), 1);
    // The account moves on: Bob links a device and the contact pins the new head.
    let mut rng = InsecureTestRng::from_label(b"another");
    let mut d = Device::new(&mut rng);
    d.account_id = w.head().account_id;
    let (_, step) = w
        .bob
        .core
        .debug_link_credential(&w.b, &d.credential().body())
        .unwrap();
    w.bob.settle(&step);
    w.settle(0);
    assert_eq!(w.contacts[0].head_gen(), 2);
    w.contacts[0].tick(&mono(1, 0));
    assert_eq!(w.contacts[0].state(), "stale");
    assert_eq!(w.contacts[0].held(), 0);
    // A late redelivery of the old request creates nothing new.
    w.deliver(0, &n);
    assert_eq!(w.contacts[0].recs().len(), 1);
    assert_eq!(w.contacts[0].state(), "stale");
}

#[test]
fn hardened_and_maximum_need_the_contacts_confirmation_recommended_does_not() {
    for level in ["hardened", "maximum"] {
        let mut w = world(1);
        w.contacts[0].rewrite(|app| app["privacyLevel"] = serde_json::json!(level));
        let req = w.request();
        let bundle = w.bundle();
        let digest = w.digest(&req);
        let n = w.notice(&req, Some(&bundle));
        w.deliver(0, &n);
        let c = &mut w.contacts[0];
        c.tick(&mono(1, 0));
        // Confirming before the wait is over is refused.
        let tok = nonce(&mut c.rt, &c.ctx, "ConfirmPeerRecovery", &digest);
        assert!(matches!(
            c.intent(
                "early",
                IntentBody::ConfirmPeerRecovery {
                    conversation_id: c.id.clone(),
                    request_id: digest.clone(),
                    confirmation: tok,
                }
            ),
            Some(OutcomeClass::Rejected { .. })
        ));
        c.tick(&mono(1, DELAY_MS));
        assert_eq!(
            c.state(),
            "ready",
            "{level}: the wait is over but the contact has not confirmed"
        );
        assert_eq!(c.head_gen(), 1);
        let v = c.view().unwrap();
        assert!(v.needs_confirm);
        // An unconfirmed token does not work; a Core-issued one does.
        let bad = c.intent(
            "bad",
            IntentBody::ConfirmPeerRecovery {
                conversation_id: c.id.clone(),
                request_id: digest.clone(),
                confirmation: serde_json::json!({"nonce": "nope"}),
            },
        );
        assert_eq!(
            bad,
            Some(OutcomeClass::Rejected {
                reason: RejectReason::Unauthorized
            })
        );
        let tok = nonce(&mut c.rt, &c.ctx, "ConfirmPeerRecovery", &digest);
        assert_eq!(
            c.intent(
                "yes",
                IntentBody::ConfirmPeerRecovery {
                    conversation_id: c.id.clone(),
                    request_id: digest.clone(),
                    confirmation: tok,
                }
            ),
            Some(OutcomeClass::Committed)
        );
        assert_eq!(c.state(), "accepted");
        assert_eq!(c.head_gen(), 2);
    }
}

#[test]
fn declining_keeps_the_old_head_and_drops_what_was_held() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let digest = w.digest(&req);
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let env = w.new_device_message(true, "hello");
    w.deliver(0, &env);
    let c = &mut w.contacts[0];
    c.tick(&mono(1, 0));
    let tok = nonce(&mut c.rt, &c.ctx, "DeclinePeerRecovery", &digest);
    assert_eq!(
        c.intent(
            "no",
            IntentBody::DeclinePeerRecovery {
                conversation_id: c.id.clone(),
                request_id: digest.clone(),
                confirmation: tok,
            }
        ),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(c.state(), "declined");
    assert_eq!(c.held(), 0);
    c.tick(&mono(1, 3 * DELAY_MS));
    assert_eq!(c.state(), "declined");
    assert_eq!(c.head_gen(), 1, "the old head is kept");
    assert!(c.timeline().is_empty());
}

#[test]
fn without_the_successor_the_record_is_ready_not_accepted_and_the_dossier_leaks_no_seconds() {
    let mut w = world(1);
    let req = w.request();
    let n = w.notice(&req, None);
    w.deliver(0, &n);
    w.contacts[0].tick(&mono(1, 0));
    w.contacts[0].tick(&mono(1, DELAY_MS));
    assert_eq!(
        w.contacts[0].state(),
        "ready",
        "a step is missing: the successor"
    );
    // The successor arrives later (a resend): now it completes with this contact's own interval.
    let bundle = w.bundle();
    let n2 = w.notice(&req, Some(&bundle));
    w.deliver(0, &n2);
    assert_eq!(w.contacts[0].state(), "accepted");
    // Chain history: Bob completes his own recovery locally, so his chain has a recovery link he holds evidence for.
    let mut w = world(0);
    let req = w.request();
    let bundle = w.bundle();
    let digest = w.digest(&req);
    w.bob.recovery_observe_request(&w.b, &req, &mono(1, 0));
    w.bob
        .recovery_observe_successor(&w.b, &digest, &bundle, &mono(1, 0));
    w.bob.recovery_tick(&w.b, &mono(1, DELAY_MS));
    let (chain, creds) = (w.chain(), w.creds());
    assert_eq!(chain.len(), 2);
    let links: Vec<control::RecoveryLinkEvidence> = w.bob_state()["recoveryLinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| control::RecoveryLinkEvidence {
            child_hash: unhex(&s(&l["childHash"])).unwrap().try_into().unwrap(),
            policy: unhex(&s(&l["policy"])).unwrap(),
            next_policy: unhex(&s(&l["nextPolicy"])).unwrap(),
            request: unhex(&s(&l["request"])).unwrap(),
            elapsed_seconds: l["elapsedSeconds"].as_u64().unwrap(),
        })
        .collect();
    assert_eq!(links.len(), 1);
    assert!(
        control::verify_chain(&chain, &creds).is_err(),
        "no evidence: refused as before"
    );
    let (head, pending) = control::verify_chain_partial(&chain, &creds, &[]).unwrap();
    assert_eq!(
        head.generation, 1,
        "the walk stops before the recovery link"
    );
    assert_eq!(pending.unwrap().index, 1);
    let (head, pending) = control::verify_chain_partial(&chain, &creds, &links).unwrap();
    assert_eq!((head.generation, pending), (2, None));
    // A dossier carries the policy bodies and the request and nothing else: no seconds, no time.
    let dossier = control::dossier_of(&chain, &links);
    assert_eq!(dossier.len(), 1);
    assert_eq!(dossier[0].child_hash, links[0].child_hash);
    assert_eq!(dossier[0].request, links[0].request);
    // The same evidence rebuilt with another verifier's seconds still has to verify under its own interval.
    let mut short = links.clone();
    short[0].elapsed_seconds = 3600;
    assert!(control::verify_chain_with(&chain, &creds, &short).is_err());
    let (_, pending) = control::verify_chain_partial(&chain, &creds, &short).unwrap();
    assert!(
        pending.is_some(),
        "an unfinished interval is pending, never accepted"
    );
}

/// A `KIND_ACS_UPDATE` payload carrying `chain`, `creds` and a one-entry dossier, as a head update would.
fn update_with_dossier(
    chain: &[Vec<u8>],
    creds: &[Vec<u8>],
    child_hash: [u8; 32],
    policy: &[u8],
    next_policy: &[u8],
    request: &[u8],
) -> Vec<u8> {
    let list = |l: &[Vec<u8>]| V::A(l.iter().cloned().map(V::B).collect());
    cbor::encode(&V::M(vec![
        (0, V::U(8)),
        (1, list(chain)),
        (2, list(creds)),
        (
            5,
            V::A(vec![V::M(vec![
                (0, om_core::crypto::sig::digest(&child_hash)),
                (1, V::B(policy.to_vec())),
                (2, V::B(next_policy.to_vec())),
                (3, V::B(request.to_vec())),
            ])]),
        ),
    ]))
}

#[test]
fn a_dossier_with_a_head_update_starts_the_same_record_and_everything_that_does_not_match_is_refused()
 {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let digest = w.digest(&req);
    let child_hash = Acs::from_body(
        cbor::decode(&bundle.child)
            .unwrap()
            .get(1)
            .unwrap()
            .b()
            .unwrap(),
    )
    .unwrap()
    .hash();
    let mut chain = w.chain();
    chain.push(bundle.child.clone());
    let mut creds = w.creds();
    creds.push(bundle.credential.clone());
    let policy = w.policy().body();
    let next = bundle.next_policy.clone();
    let send = |w: &mut World, payload: &[u8]| {
        let c = &mut w.contacts[0];
        let (_, step) = c.rt.core.debug_acs_update(&c.ctx, &c.id, payload).unwrap();
        c.rt.settle(&step);
    };

    // Evidence that does not match leaves nothing behind: a child hash that names no link of the chain, a request
    // signed by another key, and a policy body the pinned head does not commit to.
    let other_req = w.request_with(w.head().hash(), 0, 7, 1);
    let bad = [
        update_with_dossier(&chain, &creds, [3u8; 32], &policy, &next, &req),
        update_with_dossier(&chain, &creds, child_hash, &policy, &next, &other_req),
        update_with_dossier(&chain, &creds, child_hash, &next, &next, &req),
    ];
    for payload in &bad {
        send(&mut w, payload);
        assert!(
            w.contacts[0].recs().is_empty(),
            "no record from bad evidence"
        );
        assert_eq!(w.contacts[0].head_gen(), 1);
        assert_eq!(w.contacts[0].failures(), 0);
    }

    // The genuine dossier: the contact tracks the request and runs its OWN interval; the update carried no time.
    let good = update_with_dossier(&chain, &creds, child_hash, &policy, &next, &req);
    send(&mut w, &good);
    let c = &mut w.contacts[0];
    assert_eq!(c.state(), "waiting");
    assert_eq!(c.recs()[0]["obs"]["digest"], digest);
    assert_eq!(c.head_gen(), 1, "the recovery link is not followed yet");
    c.tick(&mono(1, 0));
    c.tick(&mono(1, DELAY_MS - 1));
    assert_eq!(c.state(), "waiting");
    // The same dossier again (a redelivery) never restarts or advances the interval.
    send(&mut w, &good);
    let c = &mut w.contacts[0];
    assert_eq!(c.recs().len(), 1);
    assert_eq!(c.credited(), DELAY_MS - 1);
    c.tick(&mono(1, DELAY_MS));
    assert_eq!(c.state(), "accepted");
    assert_eq!(c.head_gen(), 2);
}

fn send_text(c: &mut Contact, op: &str, text: &str) -> Option<OutcomeClass> {
    let id = c.id.clone();
    c.intent(
        op,
        IntentBody::SendMessage {
            conversation_id: id,
            draft: Draft {
                text: text.into(),
                reply_to: None,
            },
        },
    )
}

/// The conversation's outgoing messages as stored: text, delivery, whether the event still waits, whether it was sealed.
fn outgoing(c: &Contact) -> Vec<(String, String, bool, bool)> {
    c.rel()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["outgoing"] == true)
        .map(|m| {
            (
                s(&m["text"]),
                s(&m["delivery"]["state"]),
                !m["queuedEvent"].is_null(),
                !m["envelope"].is_null(),
            )
        })
        .collect()
}

#[test]
fn a_text_written_while_a_contact_recovers_waits_unsealed_and_is_sent_to_the_new_device_once_accepted()
 {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    // The new device has spoken before, so a session with it exists once the head lists it.
    let env = w.new_device_message(true, "from the new phone");
    w.deliver(0, &env);
    w.deliver(0, &n);
    assert_eq!(w.contacts[0].state(), "waiting");
    let before = outbox_len(&w.contacts[0]);
    assert_eq!(
        send_text(&mut w.contacts[0], "s1", "are you there"),
        Some(OutcomeClass::PendingNetwork)
    );
    assert_eq!(
        send_text(&mut w.contacts[0], "s2", "second"),
        Some(OutcomeClass::PendingNetwork)
    );
    let c = &mut w.contacts[0];
    // Kept, in order, unsealed: no envelope, nothing in the outbox, nobody addressed.
    assert_eq!(
        outgoing(c),
        vec![
            (
                "are you there".into(),
                "waiting_for_peer_recovery".into(),
                true,
                false
            ),
            (
                "second".into(),
                "waiting_for_peer_recovery".into(),
                true,
                false
            ),
        ]
    );
    assert_eq!(outbox_len(c), before);
    assert!(
        c.rel()["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["fan"].as_array().is_none_or(Vec::is_empty)),
        "no copy is addressed to any device"
    );
    c.tick(&mono(1, 0));
    c.tick(&mono(1, DELAY_MS - 1));
    assert_eq!(c.state(), "waiting");
    assert_eq!(outgoing(c)[0].1, "waiting_for_peer_recovery");
    // Accepted: released in order, as ordinary sends, only to the devices the new head lists.
    c.tick(&mono(1, DELAY_MS));
    assert_eq!(c.state(), "accepted");
    let after = outgoing(c);
    // Released: the event is now addressed (to the new device alone), so it no longer waits as a held event.
    assert!(
        after
            .iter()
            .all(|(_, d, queued, _)| d == "pending_network" && !queued),
        "{after:?}"
    );
    let new_id = common_hex(&w.new_dev.device_id);
    let fans: Vec<String> = w.contacts[0].rel()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["outgoing"] == true)
        .flat_map(|m| {
            m["fan"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| s(&f["deviceId"]))
        })
        .collect();
    assert!(
        !fans.is_empty() && fans.iter().all(|d| *d == new_id),
        "{fans:?}"
    );
    assert_eq!(
        w.contacts[0].rt.outcome(&w.contacts[0].ctx, "s1"),
        Some(OutcomeClass::PendingNetwork)
    );
}

fn outbox_len(c: &Contact) -> usize {
    c.rt.core.debug_app_state(&c.ctx).unwrap()["outbox"]
        .as_array()
        .unwrap()
        .len()
}

fn common_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn a_hold_survives_a_restart_and_what_cannot_be_held_is_refused_with_nothing_queued() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    let env = w.new_device_message(true, "from the new phone");
    w.deliver(0, &env);
    w.deliver(0, &n);
    let c = &mut w.contacts[0];
    c.tick(&mono(1, 0));
    assert_eq!(
        send_text(c, "s1", "kept"),
        Some(OutcomeClass::PendingNetwork)
    );
    // A place is not kept for later: refused, nothing queued.
    let id = c.id.clone();
    assert_eq!(
        c.intent(
            "loc",
            IntentBody::SendLocation {
                conversation_id: id,
                lat_e7: 1,
                lon_e7: 1,
                accuracy_m: None,
                label: None,
                note: None,
            }
        ),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::PeerRecovering
        })
    );
    assert_eq!(outgoing(c).len(), 1);
    // The bound: 64 kept, the next is refused and queues nothing.
    for i in 1..64 {
        assert_eq!(
            send_text(c, &format!("m{i}"), &format!("t{i}")),
            Some(OutcomeClass::PendingNetwork),
            "{i}"
        );
    }
    assert_eq!(
        send_text(c, "over", "one too many"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::PeerRecovering
        })
    );
    assert_eq!(outgoing(c).len(), 64);
    // Crash and restart: still waiting, still unsealed, still in order.
    c.restart();
    let kept = outgoing(c);
    assert_eq!(kept.len(), 64);
    assert!(
        kept.iter()
            .all(|(_, d, q, sealed)| d == "waiting_for_peer_recovery" && *q && !*sealed)
    );
    assert_eq!(kept[0].0, "kept");
    // The wait ends after the restart: they are released.
    c.tick(&mono(2, 1_000_000));
    c.tick(&mono(2, 1_000_000 + DELAY_MS));
    assert_eq!(c.state(), "accepted");
    assert!(
        outgoing(c)
            .iter()
            .all(|(_, d, q, _)| d == "pending_network" && !*q)
    );
}

#[test]
fn when_the_recovery_ends_without_them_what_waited_is_handed_back_and_one_retry_sends_it_to_the_old_head()
 {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    send_text(&mut w.contacts[0], "s1", "wait for me");
    assert_eq!(outgoing(&w.contacts[0])[0].1, "waiting_for_peer_recovery");
    let digest = w.digest(&req);
    let id = w.contacts[0].id.clone();
    {
        let c = &mut w.contacts[0];
        let token = nonce(&mut c.rt, &c.ctx, "DeclinePeerRecovery", &digest);
        c.intent(
            "dec",
            IntentBody::DeclinePeerRecovery {
                conversation_id: id.clone(),
                request_id: digest,
                confirmation: token,
            },
        );
    }
    let c = &mut w.contacts[0];
    assert_eq!(c.state(), "declined");
    // Nothing went out on its own; the text is kept, marked not sent, and the operation says so.
    assert_eq!(
        outgoing(c),
        vec![("wait for me".into(), "rejected".into(), true, false)]
    );
    assert_eq!(
        c.rt.outcome(&c.ctx, "s1"),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Unavailable
        })
    );
    // Restart: it stays handed back (nothing is sent by restarting).
    c.restart();
    assert_eq!(outgoing(c)[0].1, "rejected");
    // One retry sends it, sealed to the pinned head, as an ordinary message.
    let msg_id = c.rel()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["outgoing"] == true)
        .map(|m| s(&m["id"]))
        .unwrap();
    assert_eq!(
        c.intent("retry", IntentBody::RetryDelivery { message_id: msg_id }),
        Some(OutcomeClass::PendingNetwork)
    );
    let sent = outgoing(c);
    assert!(
        sent[0].1 != "rejected" && !sent[0].2 && sent[0].3,
        "{sent:?}"
    );
}

#[test]
fn a_veto_and_a_moved_on_account_also_hand_back_and_a_lost_record_does_not_strand_a_text() {
    // Stale: the account moved on while a text waited.
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    send_text(&mut w.contacts[0], "s1", "late");
    let mut rng = InsecureTestRng::from_label(b"another");
    let mut d = Device::new(&mut rng);
    d.account_id = w.head().account_id;
    let (_, step) = w
        .bob
        .core
        .debug_link_credential(&w.b, &d.credential().body())
        .unwrap();
    w.bob.settle(&step);
    w.settle(0);
    w.contacts[0].tick(&mono(1, 0));
    assert_eq!(w.contacts[0].state(), "stale");
    assert_eq!(outgoing(&w.contacts[0])[0].1, "rejected");
    // A record that is gone while a text waits: the next tick hands the text back.
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    send_text(&mut w.contacts[0], "s1", "orphan");
    w.contacts[0].rewrite(|app| {
        for r in app["relationships"].as_object_mut().unwrap().values_mut() {
            r["peerRecovery"] = serde_json::json!([]);
        }
    });
    w.contacts[0].tick(&mono(1, 0));
    assert_eq!(outgoing(&w.contacts[0])[0].1, "rejected");
}

/// ADR-0190 section 6 (Restore): a `waiting` record is inert history after a restore from backup. A history backup taken
/// while a contact's interval is running restores no record, no held message and no credit, whatever the clock then says;
/// the restored profile has to observe a notice again and start its own interval.
#[test]
fn a_restored_backup_carries_no_recovery_record_and_credits_nothing() {
    let mut w = world(1);
    let req = w.request();
    let bundle = w.bundle();
    let held = w.new_device_message(true, "from the new phone");
    w.deliver(0, &held);
    let n = w.notice(&req, Some(&bundle));
    w.deliver(0, &n);
    let c = &mut w.contacts[0];
    c.tick(&mono(1, H));
    c.tick(&mono(1, 20 * H));
    assert_eq!(c.state(), "waiting");
    assert_eq!(c.credited(), 19 * H, "the interval was running");
    assert_eq!(c.held(), 1);
    // Back up this profile, then restore it into a fresh one (a new device or a wiped one).
    let plan = c.rt.core.backup_plan();
    let key = [0x42u8; 32];
    let file = c.rt.core.export_history(&c.ctx, &plan, &key).unwrap();
    let mut fresh = Runtime::new("restored", &w.net);
    let f = common::create_profile(&mut fresh, "p-r", "Restored");
    let (_, commit) = fresh.core.import_history(&f, &file, &key).unwrap();
    assert!(fresh.settle(&Step {
        commit,
        handle: None
    }));
    let st = common::app_state(&fresh, &f);
    assert!(
        st["relationships"].as_object().unwrap().is_empty(),
        "no relationship came back, so no record, head or held message came with it"
    );
    assert_eq!(
        st["restored"].as_object().unwrap().len(),
        1,
        "the history did"
    );
    let status: RecoveryStatusView = fresh.query(Query::GetRecoveryStatus { ctx: f.clone() });
    assert!(status.requests.is_empty());
    let l: Vec<ConversationSummary> = fresh.query(Query::GetConversationList { ctx: f.clone() });
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].relationship, RelationshipStateCode::Restored);
    assert!(
        l[0].peer_recovery.is_none(),
        "no recovery banner on restored history"
    );
    // Time cannot make anything of it: days of readings, in the old boot session and a new one, change nothing.
    fresh.recovery_tick(&f, &mono(1, 20 * H));
    fresh.recovery_tick(&f, &mono(1, 20 * H + 5 * DELAY_MS));
    fresh.recovery_tick(&f, &mono(2, 5 * DELAY_MS));
    let st = common::app_state(&fresh, &f);
    assert!(st["relationships"].as_object().unwrap().is_empty());
    assert!(fresh.core.poll_targets(&f).unwrap().is_empty());
    // The original profile is untouched by the restore elsewhere.
    assert_eq!(w.contacts[0].state(), "waiting");
}
