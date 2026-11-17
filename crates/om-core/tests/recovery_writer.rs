// SPDX-License-Identifier: MPL-2.0
//! The RecoveryRecord writer (ADR-0199 section 6) against the loopback RecoveryStore: first write at the custodian
//! confirmation, the author's slot staged in the same transaction as the ACS change, compare-and-swap, the steward's
//! refresh slots, identical bytes on a retry after a restart, and a kit that is not in a backup.

mod common;

use om_core::api::*;
use om_core::host::NetCall;
use om_core::recovery_record::{
    self as rr, Plain, RecordKeys, RecoveryPlace, generation_acs, generation_refresh,
    record_generation,
};
use om_core::runtime::{LoopbackNetwork, Runtime};

const SCOPE: [u8; 32] = [7; 32];
const DAY: u64 = 86_400_000;

fn place() -> RecoveryPlace {
    RecoveryPlace {
        host: "home.example".into(),
        port: None,
        scope: SCOPE,
    }
}

fn root_from_groups(groups: &[String]) -> [u8; 32] {
    const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = String::new();
    for c in groups.concat().chars() {
        bits.push_str(&format!("{:05b}", A.find(c).unwrap()));
    }
    let v: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap())
        .collect();
    v.try_into().unwrap()
}

struct W {
    rt: Runtime,
    ctx: ProfileExecutionContext,
    keys: RecordKeys,
    root: [u8; 32],
}

fn world(with_place: bool) -> W {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("writer", &net);
    if with_place {
        rt.set_recovery_place(Some(place()));
    }
    let ctx = rt.open("p");
    rt.dispatch(
        &ctx,
        "create",
        IntentBody::CreateIdentityProfile { label: "Me".into() },
    );
    rt.flush_net(&ctx);
    let key: RecoveryKeyView = rt
        .query::<Option<RecoveryKeyView>>(Query::GetRecoveryKey {
            ctx: ctx.clone(),
            key_operation_id: "create".into(),
        })
        .expect("recovery key");
    let root = root_from_groups(&key.groups);
    W {
        rt,
        ctx,
        keys: RecordKeys::derive(&root, &SCOPE),
        root,
    }
}

fn confirm(w: &mut W) {
    w.rt.dispatch(
        &w.ctx,
        "saved",
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "create".into(),
        },
    );
}

fn puts(w: &W) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    w.rt.net.recovery_puts()
}

fn stored(w: &W) -> rr::RecoveryRecord {
    let (_, bytes) =
        w.rt.net
            .recovery_record(&w.keys.locator)
            .expect("a stored record");
    match rr::open(&w.keys, &SCOPE, &bytes).expect("opens") {
        Plain::Record(r) => *r,
        _ => panic!("not a record"),
    }
}

#[test]
fn no_write_before_the_key_is_confirmed_and_one_after() {
    let mut w = world(true);
    w.rt.flush_net(&w.ctx);
    assert!(
        puts(&w).is_empty(),
        "nothing before the person confirmed the key"
    );
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    assert_eq!(puts(&w).len(), 1);
    let r = stored(&w);
    assert_eq!(generation_refresh(r.generation), 0, "the author's slot");
    assert_eq!(generation_acs(r.generation), r.head.generation);
    r.verify().expect("the evidence holds together");
    assert_eq!(r.providers[0].host, "home.example");
    assert_eq!(r.providers[0].scope, Some(SCOPE));
    // the loopback Home holds ciphertext of a bucket size, never the plaintext
    let (_, bytes) = w.rt.net.recovery_record(&w.keys.locator).unwrap();
    assert!(rr::open_envelope(&w.keys, &[8; 32], &bytes).is_err());
    let _ = w.root;
}

#[test]
fn an_account_without_a_place_shows_the_key_alone_and_writes_nothing() {
    let mut w = world(false);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    assert!(puts(&w).is_empty());
    let s = common::app_state(&w.rt, &w.ctx);
    assert!(s["recoveryKit"].is_null());
}

#[test]
fn an_acs_change_stages_the_authors_write_in_the_same_transaction() {
    let mut w = world(true);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    let first = stored(&w);
    let (_, step) = w.rt.core.debug_link_messaging_device(&w.ctx).expect("link");
    assert!(step.commit.is_some());
    // before anything is flushed: the new write is already in the same committed state as the new head
    w.rt.settle(&step);
    let owed: Vec<_> = w.rt.core.net_ops(&w.ctx).unwrap();
    let put = owed
        .iter()
        .find_map(|o| match &o.call {
            NetCall::RecoveryPut {
                if_match, bytes, ..
            } => Some((if_match.clone(), bytes.clone())),
            _ => None,
        })
        .expect("a staged put");
    assert!(
        put.0.is_some(),
        "compare-and-swap against the acknowledged ETag"
    );
    w.rt.flush_net(&w.ctx);
    let second = stored(&w);
    assert!(generation_acs(second.generation) > generation_acs(first.generation));
    assert_eq!(generation_refresh(second.generation), 0);
    assert_eq!(
        second.evidence.tail.len(),
        1,
        "the previous state rides as tail"
    );
    second.verify().expect("verifies");
    assert_eq!(puts(&w).len(), 2);
}

#[test]
fn the_steward_refreshes_a_quiet_record_in_slot_one() {
    let mut w = world(true);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    let first = stored(&w);
    w.rt.core_tick_days(91);
    w.rt.pump(&w.ctx);
    let second = stored(&w);
    assert_eq!(
        generation_acs(second.generation),
        generation_acs(first.generation)
    );
    assert_eq!(generation_refresh(second.generation), 1);
}

trait Days {
    fn core_tick_days(&self, d: u64);
}
impl Days for Runtime {
    fn core_tick_days(&self, d: u64) {
        self.tick(d * DAY);
    }
}

fn foreign(w: &W, refresh: u32) -> Vec<u8> {
    let mut r = stored(w);
    r.generation = record_generation(generation_acs(r.generation), refresh).unwrap();
    let padded = r.to_padded_bytes().unwrap();
    rr::seal_envelope(&w.keys, &SCOPE, [9; 12], &padded).unwrap()
}

#[test]
fn a_newer_record_from_another_writer_is_not_overwritten() {
    let mut w = world(true);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    w.rt.core_tick_days(91);
    // Core stages refresh 1; another writer's refresh 7 of the same head lands first
    let tick = w.rt.core.notice_tick(&w.ctx).unwrap();
    w.rt.settle(&tick);
    let theirs = foreign(&w, 7);
    w.rt.net.recovery_replace(&w.keys.locator, theirs.clone());
    for _ in 0..3 {
        w.rt.flush_net(&w.ctx);
    }
    let (_, now) = w.rt.net.recovery_record(&w.keys.locator).unwrap();
    assert_eq!(now, theirs, "ours was dropped, theirs stands");
    let s = common::app_state(&w.rt, &w.ctx);
    assert!(s["recoveryWriter"]["staged"].is_null());
    assert_eq!(
        s["recoveryWriter"]["ackedGeneration"].as_u64().unwrap() & 0xF_FFFF,
        7
    );
}

#[test]
fn a_lower_record_is_replaced_against_the_current_etag() {
    let mut w = world(true);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    let (_, step) = w.rt.core.debug_link_messaging_device(&w.ctx).unwrap();
    w.rt.settle(&step);
    // another writer moved the record on (a refresh slot of the OLD head) before our put lands
    let theirs = foreign(&w, 5); // the sealed copy of the stored (old-head) record at refresh 5
    w.rt.net.recovery_replace(&w.keys.locator, theirs);
    for _ in 0..3 {
        w.rt.flush_net(&w.ctx);
    }
    let r = stored(&w);
    assert_eq!(generation_refresh(r.generation), 0);
    assert_eq!(
        r.evidence.tail.len(),
        1,
        "ours (the newer head) is what stands"
    );
}

#[test]
fn a_retry_after_a_restart_sends_the_same_bytes_and_request_id_and_stores_once() {
    let mut w = world(true);
    confirm(&mut w);
    let before: Vec<_> = w.rt.core.net_ops(&w.ctx).unwrap();
    let call1 = before
        .iter()
        .find(|o| matches!(o.call, NetCall::RecoveryPut { .. }))
        .expect("staged")
        .call
        .clone();
    // maybe-sent: the Home stored it but the answer was lost, then the process restarted
    w.rt.net.perform(&call1);
    let ctxs = w.rt.restart();
    let ctx = ctxs[0].clone();
    let after = w.rt.core.net_ops(&ctx).unwrap();
    let call2 = after
        .iter()
        .find(|o| matches!(o.call, NetCall::RecoveryPut { .. }))
        .expect("still owed")
        .call
        .clone();
    assert_eq!(call1, call2, "identical bytes, request id and precondition");
    w.rt.flush_net(&ctx);
    assert_eq!(w.rt.net.recovery_puts().len(), 1, "stored once");
}

#[test]
fn the_kit_is_not_in_a_backup() {
    let mut w = world(true);
    confirm(&mut w);
    w.rt.flush_net(&w.ctx);
    let app: om_core::state::AppState =
        serde_json::from_value(common::app_state(&w.rt, &w.ctx)).expect("app state");
    assert!(app.recovery_kit.is_some());
    let archive = om_core::backup::collect(&app, 0).expect("collect");
    let body = om_core::backup::encode_body(&archive);
    for secret in [
        &w.keys.write_cap,
        &w.keys.read_cap,
        &w.keys.record_key,
        &w.root,
    ] {
        assert!(!body.windows(32).any(|x| x == secret.as_slice()));
    }
}
