// SPDX-License-Identifier: MPL-2.0
//! First-contact resource and lifetime limits on the inviter side (audit 2026-10-28: M4, H2 admission cost; ADR-0181
//! sections 5 and 6).

mod common;

use common::{BindOpts, FakeRequester, app_state, create_profile, invite};
use om_core::api::*;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::runtime::{LoopbackNetwork, Runtime};
use om_core::{FIRST_CONTACT_PER_DAY, FIRST_CONTACT_PER_HOUR, HOUR_MS};

const DAY: u64 = 24 * 60 * 60;

fn pending(alice: &mut Runtime, a: &ProfileExecutionContext) -> usize {
    alice.poll(a);
    alice
        .query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })
        .len()
}

#[test]
fn a_requesters_bootstrap_key_cannot_outlive_the_bootstrap_window() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let now_ms = alice.clock.get();
    let mut rng = InsecureTestRng::from_label(b"m4");
    let (link, record) = invite(&mut alice, &a, "inv-1");
    // 90 days is the lifetime of an established delivery key; a request-scoped bootstrap key may live only 7 days
    // (profile section 7: the request deadline is min(route, key)).
    for ttl in [90 * DAY, 30 * DAY, 7 * DAY + 2 * 3600] {
        let mut r = FakeRequester::new(&mut rng);
        r.request(&net, &mut rng, &link, &record, now_ms, ttl, "Bob");
        assert_eq!(
            pending(&mut alice, &a),
            0,
            "a bootstrap key valid for {ttl}s must be refused"
        );
    }
    // Exactly 7 days (what an honest Core mints) is accepted, with one hour of clock skew allowed.
    let mut r = FakeRequester::new(&mut rng);
    r.request(
        &net,
        &mut rng,
        &link,
        &record,
        now_ms,
        7 * DAY + 1800,
        "Bob",
    );
    assert_eq!(
        pending(&mut alice, &a),
        1,
        "the 7-day bootstrap key is accepted"
    );
}

/// A request that is well-formed for the invite but cannot be opened (the ciphertext is random): it costs the
/// limiter a slot and the device a decapsulation attempt, nothing else.
fn junk_request(net: &LoopbackNetwork, rng: &mut InsecureTestRng, link: &[u8], record: &[u8]) {
    let rec = om_core::crypto::cbor::decode(record).unwrap();
    let d = om_core::intro::open_intro(
        rec.get(1).unwrap().b().unwrap(),
        om_core::intro::LBL_DESCRIPTOR_SIG,
        rec.get(3).unwrap().b().unwrap(),
    )
    .unwrap();
    let desc = om_core::intro::read_descriptor(&d.body).unwrap();
    let pk = desc.request_key.unwrap();
    let inbox = om_core::intro::parse_service_uri(&desc.intro_service).unwrap();
    let rk = om_core::request_key_digest(&pk);
    let mut enc = vec![0u8; 1120];
    let mut ct = vec![0u8; 600];
    rand_core::RngCore::fill_bytes(rng, &mut enc);
    rand_core::RngCore::fill_bytes(rng, &mut ct);
    common::deposit_request(net, &inbox, link[..16].try_into().unwrap(), &rk, enc, ct);
}

#[test]
fn the_limiter_runs_before_any_decapsulation_and_drops_the_excess_silently() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"flood");
    let (link, record) = invite(&mut alice, &a, "inv-1");
    for _ in 0..(FIRST_CONTACT_PER_HOUR + 7) {
        junk_request(&net, &mut rng, &link, &record);
    }
    alice.poll(&a);
    let l = &app_state(&alice, &a)["firstContact"];
    assert_eq!(
        l["admitted"].as_array().unwrap().len(),
        FIRST_CONTACT_PER_HOUR,
        "only the budget reaches the expensive path"
    );
    assert_eq!(
        l["dropped"], 7,
        "the rest are dropped and counted, nothing more"
    );
    let load: IntroductionLoadView = alice.query(Query::GetIntroductionLoad { ctx: a.clone() });
    assert_eq!(
        load,
        IntroductionLoadView {
            throttled: true,
            dropped: 7
        }
    );
    // A refused request leaves no pending request, no relationship and no reply on the wire.
    assert_eq!(pending(&mut alice, &a), 0);
    assert_eq!(net.deposit_count(), FIRST_CONTACT_PER_HOUR as u64 + 7);

    // The notice is one aggregated flag that lapses with the window; the budget returns after an hour.
    alice.tick(HOUR_MS + 1000);
    assert!(
        !alice
            .query::<IntroductionLoadView>(Query::GetIntroductionLoad { ctx: a.clone() })
            .throttled
    );
    let mut r = FakeRequester::new(&mut rng);
    r.request(
        &net,
        &mut rng,
        &link,
        &record,
        alice.clock.get(),
        3600,
        "Bob",
    );
    assert_eq!(
        pending(&mut alice, &a),
        1,
        "the invite-bound request is admitted again"
    );
}

#[test]
fn the_limiter_is_persisted_and_a_clock_set_backwards_does_not_reopen_the_window() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"persist");
    let (link, record) = invite(&mut alice, &a, "inv-1");
    for _ in 0..FIRST_CONTACT_PER_HOUR {
        junk_request(&net, &mut rng, &link, &record);
    }
    alice.poll(&a);
    // Restart: the budget is durable.
    let a = alice.restart().remove(0);
    junk_request(&net, &mut rng, &link, &record);
    alice.poll(&a);
    assert_eq!(app_state(&alice, &a)["firstContact"]["dropped"], 1);
    // The wall clock jumps back an hour: the high-water mark keeps the window closed.
    alice.clock.set(alice.clock.get() - HOUR_MS);
    junk_request(&net, &mut rng, &link, &record);
    alice.poll(&a);
    assert_eq!(
        app_state(&alice, &a)["firstContact"]["dropped"],
        2,
        "a clock set backwards must not reopen the hourly window"
    );
}

#[test]
fn the_daily_budget_holds_when_the_hourly_one_keeps_refilling() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"daily");
    let (link, record) = invite(&mut alice, &a, "inv-1");
    let mut sent = 0;
    // 20 per hour for 6 hours is 120 attempts; only 100 are admitted inside the day.
    for _hour in 0..6 {
        for _ in 0..FIRST_CONTACT_PER_HOUR {
            junk_request(&net, &mut rng, &link, &record);
            sent += 1;
        }
        alice.poll(&a);
        alice.tick(HOUR_MS + 1000);
    }
    assert_eq!(sent, 120);
    let l = &app_state(&alice, &a)["firstContact"];
    assert_eq!(
        l["admitted"].as_array().unwrap().len(),
        FIRST_CONTACT_PER_DAY
    );
}

#[test]
fn a_first_contact_chain_above_the_cap_is_refused_before_any_signature_is_checked() {
    // Alice (real Core) accepts a requester whose Bind carries 17 chain entries: refused cleanly, nothing pinned.
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"cap");
    let now_ms = alice.clock.get();
    let (link, record) = invite(&mut alice, &a, "inv-1");
    let mut r = FakeRequester::new(&mut rng);
    r.request(&net, &mut rng, &link, &record, now_ms, 3600, "Bob");
    alice.poll(&a);
    let id = alice.query::<Vec<IntroductionRequest>>(Query::GetIntroductions { ctx: a.clone() })[0]
        .id
        .clone();
    alice.dispatch(&a, "acc", IntentBody::AcceptIntroduction { request_id: id });
    alice.flush_net(&a);
    let acc = r.accept(&net, now_ms / 1000).unwrap();
    // Seventeen entries: the cap is checked first, so the junk entries are never parsed or verified.
    let junk = vec![vec![0u8; 8]; om_core::FIRST_CONTACT_MAX_CHAIN + 1];
    let bind = r.bind_payload(
        &mut rng,
        now_ms / 1000,
        BindOpts {
            chain: Some(junk),
            ..BindOpts::default()
        },
    );
    r.deposit_to_inviter(&net, &mut rng, &acc, &bind, now_ms / 1000);
    alice.poll(&a);
    let rel = app_state(&alice, &a)["relationships"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(rel["state"], "accepted_unbound");
    assert_eq!(rel["peerAccountId"], "", "no account was pinned");
    assert_eq!(rel["integrityFailures"], 1);
}

#[test]
fn cheap_checks_run_first_and_spend_no_budget() {
    // Requests that fail an invite-state, size, shape or ticket/digest check never reach the limiter, so a flood of
    // them neither burns the budget nor triggers the notice (ADR-0181 section 5, order of checks).
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let mut rng = InsecureTestRng::from_label(b"cheap");
    let (link, record) = invite(&mut alice, &a, "inv-1");
    let rec = om_core::crypto::cbor::decode(&record).unwrap();
    let d = om_core::intro::open_intro(
        rec.get(1).unwrap().b().unwrap(),
        om_core::intro::LBL_DESCRIPTOR_SIG,
        rec.get(3).unwrap().b().unwrap(),
    )
    .unwrap();
    let desc = om_core::intro::read_descriptor(&d.body).unwrap();
    let inbox = om_core::intro::parse_service_uri(&desc.intro_service).unwrap();
    let rk = om_core::request_key_digest(&desc.request_key.unwrap());
    let id: [u8; 16] = link[..16].try_into().unwrap();
    for _ in 0..(3 * FIRST_CONTACT_PER_HOUR) {
        // wrong ticket id, wrong request-key digest, oversized ciphertext, not a MessageRequest at all
        common::deposit_request(&net, &inbox, &[1; 16], &rk, vec![0; 1120], vec![0; 600]);
        common::deposit_request(&net, &inbox, &id, &[2; 32], vec![0; 1120], vec![0; 600]);
        common::deposit_request(&net, &inbox, &id, &rk, vec![0; 1120], vec![0; 40_000]);
        net.perform(&om_core::host::NetCall::Deposit {
            mailbox: inbox.to_vec(),
            relay_uri: None,
            bytes: vec![0xff; 100],
        });
        let _ = &mut rng;
    }
    alice.poll(&a);
    let l = &app_state(&alice, &a)["firstContact"];
    assert_eq!(
        l["admitted"].as_array().unwrap().len(),
        0,
        "no budget was spent"
    );
    assert_eq!(
        l["dropped"], 0,
        "and nothing was counted as a stranger flood"
    );
}
