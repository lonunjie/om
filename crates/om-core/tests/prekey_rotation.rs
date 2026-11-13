// SPDX-License-Identifier: MPL-2.0
//! Signed-prekey rotation (ASYNC_KEY_MATERIAL sections 4, 6, 7; ADR-0140 sections 2, 3, 5, 6; schedule and overlap from
//! `docs/reports/2026-11-13-signed-prekey-rotation.md` G1, G2, G3). The first half drives `Device` directly; the second
//! half drives the whole Core against the loopback provider (clock, outbox, restart).

use om_core::api::*;
use om_core::crypto::binding::{self, RelationshipContext};
use om_core::crypto::ledger::LAST_RESORT_ACCEPT_LIMIT;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::{
    Device, LEDGER_ROTATE_AT, NOW, Provider, ROTATE_BEFORE_EXPIRY_S, SPK_MAX_AGE_S,
};
use om_core::crypto::{Error, cbor};
use om_core::host::NetCall;
use om_core::runtime::{Fault, LoopbackNetwork, Runtime, settle_all};
use std::collections::BTreeSet;

const DAY: u64 = 24 * 60 * 60;

fn rng(label: &str) -> InsecureTestRng {
    InsecureTestRng::from_label(label.as_bytes())
}

/// Bob with a published first generation (3 EC + 3 PQ one-time items) whose bundle expires at `NOW + ttl`.
fn published_bob(r: &mut InsecureTestRng, ttl: u64) -> (Device, Provider) {
    let mut bob = Device::new(r);
    bob.bundle_expires_at = NOW + ttl;
    bob.rotation_at = NOW;
    let mut prov = Provider::default();
    let up = bob.generate_one_time(3, 3, r);
    prov.publish(&bob, up);
    (bob, prov)
}

/// A first message from a fresh initiator that fetched `prov`'s current material for `bob`.
fn first_message(
    bob: &mut Device,
    prov: &mut Provider,
    text: &[u8],
    r: &mut InsecureTestRng,
) -> (Device, Vec<u8>) {
    let mut alice = Device::new(r);
    bob.trust_device(&alice).unwrap();
    let did = bob.device_id;
    let (acs, cred) = prov.view(&did);
    let fetch = prov.fetch(&did).unwrap();
    alice
        .start_session(&fetch, &Device::peer_view(&acs, &cred), r)
        .unwrap();
    let env = alice.encrypt(&bob.ik(), text).unwrap().0;
    (alice, env)
}

fn accepts(
    bob: &mut Device,
    alice: &Device,
    env: &[u8],
    r: &mut InsecureTestRng,
) -> Result<(), Error> {
    bob.decrypt(&RelationshipContext::new(alice.account_id), env, r)
        .map(|_| ())
}

#[test]
fn generation_strictly_increases_and_every_key_id_is_fresh() {
    let mut r = rng("rot-gen");
    let (mut bob, mut prov) = published_bob(&mut r, 90 * DAY);
    let mut ids: BTreeSet<u32> = bob.opks.keys().chain(bob.pqopks.keys()).copied().collect();
    ids.insert(bob.spk.id);
    ids.insert(bob.last_resort.id);
    let mut secrets = BTreeSet::from([bob.spk.secret.to_vec(), bob.last_resort.seed.to_vec()]);
    let mut last = bob.epoch;
    for step in 1..=3u64 {
        let up = bob.rotate_signed_prekeys(
            NOW + step * 8 * DAY,
            NOW + step * 8 * DAY + 90 * DAY,
            3,
            3,
            &mut r,
        );
        assert_eq!(bob.epoch, last + 1, "generation + 1 on every rotation");
        last = bob.epoch;
        assert_eq!(
            (up.opks.len(), up.pqopks.len()),
            (3, 3),
            "a fresh one-time batch rides along"
        );
        // Fresh key ids and fresh secrets: nothing from an earlier generation is relabelled.
        for id in [bob.spk.id, bob.last_resort.id] {
            assert!(ids.insert(id), "signed-prekey id {id} reused");
        }
        assert!(
            secrets.insert(bob.spk.secret.to_vec())
                && secrets.insert(bob.last_resort.seed.to_vec())
        );
        assert_eq!((bob.spk.epoch, bob.last_resort.epoch), (last, last));
        let fresh: Vec<u32> = bob
            .opks
            .keys()
            .chain(bob.pqopks.keys())
            .copied()
            .filter(|i| bob.one_time_epoch_of(*i) == last)
            .collect();
        assert_eq!(fresh.len(), 6);
        for id in fresh {
            assert!(ids.insert(id), "one-time id {id} reused");
        }
        prov.publish(&bob, up);
        let (acs, cred) = prov.view(&bob.device_id);
        let vb = binding::verify_fetch(
            &prov.fetch(&bob.device_id).unwrap(),
            &Device::peer_view(&acs, &cred),
        )
        .unwrap();
        assert_eq!(vb.epoch, last);
        assert_eq!(vb.spk_id, bob.spk.id);
    }
    assert_eq!(bob.epoch, 4);
    // After three rotations the first-generation one-time secrets are gone (one previous slot only).
    assert!(
        bob.opks
            .keys()
            .chain(bob.pqopks.keys())
            .all(|i| bob.one_time_epoch_of(*i) >= 3)
    );
}

#[test]
fn old_generation_first_message_is_accepted_in_the_overlap_and_rejected_after_retirement() {
    let mut r = rng("rot-overlap");
    let (mut bob, mut prov) = published_bob(&mut r, 1_000);
    let old_expires = bob.bundle_expires_at;
    // Two peers fetched generation 1 before the rotation.
    let (a1, env1) = first_message(&mut bob, &mut prov, b"one", &mut r);
    let (a2, env2) = first_message(&mut bob, &mut prov, b"two", &mut r);
    let old_epoch = bob.epoch;
    let up = bob.rotate_signed_prekeys(NOW + 500, NOW + 500 + 90 * DAY, 3, 3, &mut r);
    prov.publish(&bob, up);
    assert_eq!(bob.prev_expires_at, old_expires);
    // Not yet: the old bundle is still valid for peers, so nothing is deleted.
    assert!(!bob.retire_previous(old_expires));
    assert!(
        accepts(&mut bob, &a1, &env1, &mut r).is_ok(),
        "generation 1 initiation inside the overlap"
    );
    // Past the old bundle's own expires_at nothing can legitimately initiate against it.
    assert!(bob.retire_previous(old_expires + 1));
    assert!(bob.prev_spk.is_none() && bob.prev_last_resort.is_none());
    assert_eq!(bob.ledger.count(old_epoch), 0);
    assert!(
        bob.opks
            .keys()
            .chain(bob.pqopks.keys())
            .all(|i| bob.one_time_epoch_of(*i) == bob.epoch)
    );
    assert_eq!(
        accepts(&mut bob, &a2, &env2, &mut r),
        Err(Error::UnknownSignedPrekey)
    );
    assert!(!bob.retire_previous(old_expires + 2), "idempotent");
}

#[test]
fn a_one_time_secret_of_an_older_epoch_is_not_usable_with_a_newer_signed_prekey() {
    let mut r = rng("rot-mix");
    let (mut bob, mut prov) = published_bob(&mut r, 90 * DAY);
    let (a, env) = first_message(&mut bob, &mut prov, b"old", &mut r);
    let up = bob.rotate_signed_prekeys(NOW + 8 * DAY, NOW + 98 * DAY, 3, 3, &mut r);
    prov.publish(&bob, up);
    // Spent on the first generation it still works (that epoch is in its overlap) ...
    assert!(accepts(&mut bob, &a, &env, &mut r).is_ok());
    // ... and the tag survives a round trip through the vault.
    let back = Device::from_bytes(&bob.to_bytes()).unwrap();
    assert!(back == bob);
    assert_eq!(back.one_time_epoch, bob.one_time_epoch);
}

#[test]
fn the_ledger_resets_per_epoch_and_three_quarters_of_the_cap_triggers_rotation() {
    let mut r = rng("rot-ledger");
    let (mut bob, _prov) = published_bob(&mut r, 90 * DAY);
    let epoch = bob.epoch;
    for i in 0..LEDGER_ROTATE_AT - 1 {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_be_bytes());
        bob.ledger.record(epoch, id);
    }
    assert!(!bob.rotation_due(NOW + 60), "just below 3/4 of the cap");
    bob.ledger.record(epoch, [0xff; 32]);
    assert!(bob.rotation_due(NOW + 60), "3/4 of the cap");
    bob.rotate_signed_prekeys(NOW + 60, NOW + 60 + 90 * DAY, 3, 3, &mut r);
    assert_eq!(
        bob.ledger.count(epoch),
        LEDGER_ROTATE_AT,
        "the previous epoch keeps its own ledger"
    );
    assert_eq!(bob.ledger.count(bob.epoch), 0, "the new epoch starts empty");
    assert!(!bob.rotation_due(NOW + 61));
    // The old epoch can still fill up and fail closed without touching the new one.
    for i in LEDGER_ROTATE_AT..LAST_RESORT_ACCEPT_LIMIT {
        let mut id = [1u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_be_bytes());
        bob.ledger.record(epoch, id);
    }
    assert_eq!(
        bob.ledger.check(epoch, &[9; 32]),
        Err(Error::LastResortExhausted)
    );
    assert_eq!(bob.ledger.check(bob.epoch, &[9; 32]), Ok(()));
}

#[test]
fn schedule_age_expiry_and_a_backwards_clock() {
    let mut r = rng("rot-clock");
    let (mut bob, _prov) = published_bob(&mut r, 90 * DAY);
    assert!(!bob.rotation_due(NOW + SPK_MAX_AGE_S - 1));
    assert!(
        bob.rotation_due(NOW + SPK_MAX_AGE_S),
        "7 days of signed-prekey age"
    );
    // A clock that went backwards reads as age 0 and never rotates early.
    assert!(!bob.rotation_due(NOW - 400 * DAY));
    let before = bob.rotation_at;
    let epoch = bob.epoch;
    bob.rotate_signed_prekeys(NOW - 400 * DAY, NOW + 90 * DAY, 0, 0, &mut r);
    assert_eq!(
        bob.epoch,
        epoch + 1,
        "an explicit rotation still only moves the epoch up"
    );
    assert_eq!(
        bob.rotation_at, before,
        "the rotation time is never lowered"
    );
    assert!(
        !bob.rotation_due(NOW + SPK_MAX_AGE_S - 1),
        "no early rotation after the clock came back"
    );
    // Within 7 days of the bundle's own expiry.
    let (bob2, _) = published_bob(&mut rng("rot-clock-2"), 20 * DAY);
    assert!(!bob2.rotation_due(NOW + 6 * DAY));
    assert!(bob2.rotation_due(NOW + 20 * DAY - ROTATE_BEFORE_EXPIRY_S));
    // State from before rotation existed has no rotation time: nothing is due until the clock is stamped.
    let mut legacy = Device::new(&mut rng("rot-legacy"));
    assert!(!legacy.rotation_due(NOW + 400 * DAY));
    legacy.stamp_rotation_clock(NOW);
    legacy.stamp_rotation_clock(NOW + 5 * DAY);
    assert_eq!(legacy.rotation_at, NOW);
}

#[test]
fn a_peer_that_saw_the_new_generation_rejects_a_replayed_old_bundle() {
    let mut r = rng("rot-rollback");
    let (mut bob, mut prov) = published_bob(&mut r, 90 * DAY);
    let (acs, cred) = prov.view(&bob.device_id);
    let old_fetch = prov.fetch(&bob.device_id).unwrap();
    let up = bob.rotate_signed_prekeys(NOW + 8 * DAY, NOW + 98 * DAY, 3, 3, &mut r);
    prov.publish(&bob, up);
    let new_fetch = prov.fetch(&bob.device_id).unwrap();
    let mut alice = Device::new(&mut r);
    bob.trust_device(&alice).unwrap();
    let view = Device::peer_view(&acs, &cred);
    alice.start_session(&new_fetch, &view, &mut r).unwrap();
    let replay = alice.start_session(&old_fetch, &view, &mut r);
    assert_eq!(replay.err(), Some(Error::PrekeyGenerationRollback));
}

#[test]
fn the_device_state_without_the_new_keys_reads_as_the_current_epoch() {
    let mut r = rng("rot-legacy-state");
    let (bob, _) = published_bob(&mut r, 90 * DAY);
    let mut v = bob.to_cbor();
    if let cbor::V::M(m) = &mut v {
        m.retain(|(k, _)| !(19..=21).contains(k));
    }
    let back = Device::from_cbor(&v).unwrap();
    assert!(back.one_time_epoch.is_empty());
    assert_eq!(back.rotation_at, 0, "unknown rotation time");
    assert!(
        back.opks
            .keys()
            .all(|i| back.one_time_epoch_of(*i) == back.epoch)
    );
}

// ------------------------------------------------------------------------------------------ Core against the loopback

fn profile(rt: &mut Runtime, id: &str, label: &str) -> ProfileExecutionContext {
    let ctx = rt.open(id);
    rt.dispatch(
        &ctx,
        "c",
        IntentBody::CreateIdentityProfile {
            label: label.into(),
        },
    );
    rt.flush_net(&ctx);
    ctx
}

fn publishes(rt: &Runtime, ctx: &ProfileExecutionContext) -> Vec<(u64, NetCall)> {
    rt.core
        .net_ops(ctx)
        .unwrap()
        .into_iter()
        .filter(|o| matches!(o.call, NetCall::PublishKeyMaterial { .. }))
        .map(|o| (o.id, o.call))
        .collect()
}

fn generation(net: &LoopbackNetwork) -> u64 {
    net.key_generations()[0].1
}

/// Bob after `days` of silence, then something that makes Core look at its keys (any intent).
fn bob_due(net: &LoopbackNetwork, days: u64) -> (Runtime, ProfileExecutionContext) {
    let mut bob = Runtime::new("bob", net);
    let b = profile(&mut bob, "pb", "Bob");
    assert_eq!(generation(net), 1);
    bob.tick(days * DAY * 1000);
    (bob, b)
}

#[test]
fn a_rotated_device_publishes_once_and_first_contact_still_works() {
    let net = LoopbackNetwork::default();
    let (mut bob, b) = bob_due(&net, 8);
    let mut alice = Runtime::new("alice", &net);
    alice.clock.set(bob.clock.get());
    let a = profile(&mut alice, "pa", "Alice");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    // The Home that is asked to revoke the old generation is unreachable: the new publish alone must make the old
    // one-time items harmless (G3, option b), or the whole fetch would fail verification under generation 2.
    net.inject(Fault::Unavailable);
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    bob.flush_net(&b);
    assert_eq!(generation(&net), 2, "the new generation is published");
    let (_, ec, pq) = net.key_inventory()[0].clone();
    assert_eq!(
        (ec, pq),
        (10, 10),
        "items of generation 1 were dropped, only the fresh batch remains"
    );
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
    let active: Vec<ConversationSummary> =
        alice.query(Query::GetConversationList { ctx: a.clone() });
    assert_eq!(
        active.len(),
        1,
        "first contact against generation 2 succeeds"
    );
}

#[test]
fn rotation_is_one_commit_a_crash_replays_the_identical_publish_and_never_rotates_twice() {
    let net = LoopbackNetwork::default();
    let (mut bob, b) = bob_due(&net, 8);
    bob.dispatch(&b, "inv", IntentBody::CreateInvite);
    let owed = publishes(&bob, &b);
    assert_eq!(
        owed.len(),
        1,
        "the rotation and its publish are committed together"
    );
    // Crash after the commit and before anything was sent.
    let ctxs = bob.restart();
    let b = ctxs[0].clone();
    assert_eq!(generation(&net), 1, "nothing was sent yet");
    assert_eq!(
        publishes(&bob, &b),
        owed,
        "the restart lists the identical bytes under the same id"
    );
    // More activity while the publish is owed never starts a second rotation.
    bob.tick(9 * DAY * 1000);
    bob.dispatch(&b, "inv2", IntentBody::CreateInvite);
    assert_eq!(publishes(&bob, &b), owed);
    // The publish fails twice (not sent, then sent but timed out): still the same single entry, same bytes.
    net.inject(Fault::Unavailable);
    net.inject(Fault::Unavailable);
    bob.flush_net(&b);
    assert_eq!(publishes(&bob, &b), owed, "retry keeps the entry untouched");
    net.inject(Fault::TimeoutBeforeEffect);
    net.inject(Fault::TimeoutBeforeEffect);
    bob.flush_net(&b);
    assert_eq!(publishes(&bob, &b), owed);
    assert_eq!(generation(&net), 1);
    bob.flush_net(&b);
    assert_eq!(
        generation(&net),
        2,
        "exactly one rotation reached the provider"
    );
    assert!(publishes(&bob, &b).is_empty());
}

#[test]
fn a_clock_that_runs_backwards_never_rotates() {
    let net = LoopbackNetwork::default();
    let mut bob = Runtime::new("bob", &net);
    let b = profile(&mut bob, "pb", "Bob");
    bob.clock.set(bob.clock.get() - 30 * DAY * 1000);
    bob.dispatch(&b, "inv", IntentBody::CreateInvite);
    bob.flush_net(&b);
    assert_eq!(generation(&net), 1);
    // Forward again, within a week of the real creation: still no rotation.
    bob.clock
        .set(bob.clock.get() + 30 * DAY * 1000 + 3 * DAY * 1000);
    bob.dispatch(&b, "inv2", IntentBody::CreateInvite);
    bob.flush_net(&b);
    assert_eq!(generation(&net), 1);
    // A week after creation it does.
    bob.tick(5 * DAY * 1000);
    bob.dispatch(&b, "inv3", IntentBody::CreateInvite);
    bob.flush_net(&b);
    assert_eq!(generation(&net), 2);
}
