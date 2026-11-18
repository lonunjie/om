// SPDX-License-Identifier: MPL-2.0
//! Groups load check at the Core level (ADR-0177, ADR-0187): a 32-member Room on the real MLS engine (stock OpenMLS),
//! built the way `groups_loopback.rs` builds one, then driven with bursts of application messages and membership churn.
//! Every commit goes through the Core journal and the committer-signed binding, and every other member applies it through
//! `group_receive_commit`. The test asserts the invariants (all members on one epoch, one head hash and one exporter
//! secret; every message read by every member and by no removed one; a removed member cannot read later traffic) and
//! reports time per operation and memory. Run the report with:
//!   cargo test -p om-core --release --features groups-openmls --test groups_load -- --ignored --nocapture
#![cfg(feature = "groups-openmls")]

use om_codec::{Value, encode};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::DeviceSigner;
use om_core::groups::journal::*;
use om_core::groups::mls::*;
use om_core::groups::openmls::{OpenMlsGroupCrypto as Eng, OpenMlsLoader};
use om_core::groups::*;
use om_core::state::AppState;
use std::time::{Duration, Instant};

const ADMIN: u8 = 0xa0;
const MEMBER: u8 = 0xa1;
const NOW: u64 = 1_800_000_000_000;
const CONV: [u8; 32] = [9; 32];
const GID: [u8; 24] = [0x6a; 24];

fn id16(n: u8) -> Vec<u8> {
    vec![n; 16]
}
fn acct(n: u8) -> Principal {
    Principal::account(vec![n; 32])
}
fn m(e: Vec<(u64, Value)>) -> Value {
    Value::Map(e.into_iter().map(|(k, v)| (Value::Uint(k), v)).collect())
}
fn dig(h: &[u8]) -> Value {
    m(vec![(0, Value::Uint(1)), (1, Value::Bytes(h.to_vec()))])
}
fn p_val(p: &Principal) -> Value {
    m(vec![
        (0, Value::Uint(p.kind)),
        (1, Value::Bytes(p.id.clone())),
    ])
}

/// One device on its own account: its own state, engine, signer and deterministic RNG. Device and account number are `n`.
struct Dev {
    app: AppState,
    eng: Eng,
    signer: DeviceSigner,
    rng: InsecureTestRng,
    n: u8,
}

impl Dev {
    fn new(n: u8) -> Self {
        let mut rng = InsecureTestRng::from_label(&[b'L', n]);
        Dev {
            app: AppState::default(),
            eng: Eng::new(&id16(n)).expect("engine"),
            signer: DeviceSigner::generate(&mut rng),
            rng,
            n,
        }
    }
    fn id(&self) -> Vec<u8> {
        id16(self.n)
    }
    fn pk(&self) -> Vec<u8> {
        self.signer.public()
    }
    fn kp(&mut self) -> Vec<u8> {
        let mut env = CryptoEnv {
            rng: &mut self.rng,
            now_ms: NOW,
        };
        self.eng.key_package(&mut env).unwrap().bytes
    }
}

macro_rules! env {
    ($d:expr) => {
        &mut CryptoEnv {
            rng: &mut $d.rng,
            now_ms: NOW,
        }
    };
}

/// An intent id that is unique for any number of intents (the loopback tests use one byte).
fn intent_id(n: u32) -> Vec<u8> {
    let mut v = vec![0x77; 16];
    v[..4].copy_from_slice(&n.to_be_bytes());
    v
}

fn intent_body(parent: &GroupState, n: u32, op_id: u64, args: Value) -> Vec<u8> {
    let reducing = matches!(op_id, 2 | 3 | 5 | 11 | 16);
    encode(&m(vec![
        (0, Value::Uint(29)),
        (1, Value::Uint(1)),
        (2, Value::Bytes(parent.conversation_id.clone())),
        (3, dig(&parent.hash())),
        (4, Value::Uint(parent.state_version)),
        (5, Value::Bytes(intent_id(n))),
        (6, Value::Uint(op_id)),
        (7, Value::Bool(reducing)),
        (8, args),
        (9, m(vec![(0, Value::Uint(1))])),
    ]))
}

/// Account `who` is admitted under the incarnation `inc`.
fn admit_body(parent: &GroupState, n: u32, who: u8, inc: u8) -> Vec<u8> {
    let e = vec![
        (0, p_val(&acct(who))),
        (1, Value::Bytes(id16(inc))),
        (3, Value::Uint(1)),
    ];
    intent_body(parent, n, op::MEMBER_ADMIT, m(e))
}

fn remove_body(parent: &GroupState, n: u32, who: u8, inc: u8) -> Vec<u8> {
    let a = m(vec![(0, p_val(&acct(who))), (1, Value::Bytes(id16(inc)))]);
    intent_body(parent, n, op::MEMBER_REMOVE, a)
}

fn head(d: &Dev) -> GroupState {
    d.app.group(&CONV).unwrap().unwrap()
}

fn join(joiner: &mut Dev, welcome: &[u8], state: &GroupState) {
    let n = joiner.n;
    let (gid, info) = joiner
        .eng
        .process_welcome(env!(joiner), welcome)
        .unwrap_or_else(|e| panic!("joiner {n}: {e:?}"));
    assert_eq!(gid, GID);
    assert_eq!(info.epoch, state.mls_epoch_ref);
    joiner
        .app
        .groups
        .insert(om_core::crypto::hex(&CONV), GroupRec::of(state));
    joiner.app.group_bind_mls_id(&CONV, &GID).unwrap();
    joiner.app.group_crypto_save(&joiner.eng);
}

// ---- measurement ----------------------------------------------------------------------------------------------------------

/// Resident memory of this test process in bytes (`ps`, so it works on macOS and Linux with no new dependency).
fn rss() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
}

#[derive(Default)]
struct Times(Vec<Duration>);
impl Times {
    fn add(&mut self, d: Duration) {
        self.0.push(d);
    }
    fn pct(&self, q: f64) -> f64 {
        if self.0.is_empty() {
            return 0.0;
        }
        let mut v: Vec<f64> = self.0.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[(((q * v.len() as f64).ceil() as usize).max(1) - 1).min(v.len() - 1)]
    }
    fn line(&self, name: &str) -> String {
        let total: f64 = self.0.iter().map(|d| d.as_secs_f64()).sum();
        format!(
            "  {name:<26} n={:<6} p50={:>8.2} ms  p95={:>8.2} ms  p99={:>8.2} ms  max={:>8.2} ms  total={:>7.2} s",
            self.0.len(),
            self.pct(0.5),
            self.pct(0.95),
            self.pct(0.99),
            self.pct(1.0),
            total
        )
    }
}

pub struct Report {
    pub lines: Vec<String>,
}

/// A Room of `size` members (account and device `n` for n in 1..=size; 1 is the admin).
struct Room {
    devs: Vec<Dev>,
    /// Which devices are in the Room now (index into `devs`).
    members: Vec<bool>,
    intent: u32,
    inc: Vec<u8>,
    commit_prepare: Times,
    commit_receive: Times,
}

impl Room {
    fn new(size: usize) -> (Room, GroupState, Times) {
        let mut devs: Vec<Dev> = (1..=size as u8).map(Dev::new).collect();
        {
            let a = &mut devs[0];
            let g = genesis(&GenesisParams {
                conversation_id: CONV.to_vec(),
                creator: acct(1),
                creator_incarnation: id16(0x11),
                creator_device: a.id(),
                admin_role_id: id16(ADMIN),
                member_role_id: id16(MEMBER),
                name: "Load room".into(),
                protection: 1,
            })
            .unwrap();
            a.app.group_install_genesis(&g).unwrap();
            a.app.group_bind_mls_id(&CONV, &GID).unwrap();
            a.eng.create_group(env!(a), &GID, &CONV).unwrap();
            a.app.group_crypto_save(&a.eng);
        }
        let mut members = vec![false; size];
        members[0] = true;
        let mut room = Room {
            devs,
            members,
            intent: 1,
            inc: (0..size).map(|i| 0x40 + i as u8).collect(),
            commit_prepare: Times::default(),
            commit_receive: Times::default(),
        };
        room.inc[0] = 0x11;
        let mut joins = Times::default();
        for i in 1..size {
            let d = room.add(i);
            joins.add(d);
        }
        let head = head(&room.devs[0]);
        (room, head, joins)
    }

    /// The admin prepares, publishes and accepts one Commit; returns its bytes, binding, new head state and Welcome (empty for a removal).
    fn commit(
        &mut self,
        plan: CommitPlan,
        body: Vec<u8>,
    ) -> (Vec<u8>, Vec<u8>, GroupState, Vec<u8>) {
        let t0 = Instant::now();
        let a = &mut self.devs[0];
        let e = a.app.group_journal_begin(&body, &[1; 32]).unwrap();
        let p = a
            .app
            .group_journal_prepare(
                &e.intent_id,
                &plan,
                DEFAULT_BINDING_KIND,
                &mut a.eng,
                env!(a),
                &a.signer,
            )
            .unwrap();
        a.app.group_journal_published(&e.intent_id).unwrap();
        let s = a
            .app
            .group_journal_accept(&e.intent_id, &mut a.eng)
            .unwrap();
        a.app.group_crypto_save(&a.eng);
        self.commit_prepare.add(t0.elapsed());
        (p.commit, p.signed_binding, s, p.welcome.unwrap_or_default())
    }

    fn receive(&mut self, skip: Option<usize>, commit: &[u8], binding: &[u8], body: &[u8]) {
        let pk = self.devs[0].pk();
        for i in 1..self.devs.len() {
            if !self.members[i] || Some(i) == skip {
                continue;
            }
            let t0 = Instant::now();
            let d = &mut self.devs[i];
            let out = d
                .app
                .group_receive_commit(binding, &pk, commit, body, &[1; 32], &mut d.eng, env!(d))
                .unwrap();
            assert!(matches!(out, ReceiveOutcome::Applied { .. }), "{out:?}");
            d.app.group_crypto_save(&d.eng);
            self.commit_receive.add(t0.elapsed());
        }
    }

    /// Admit device `i` (account `i + 1`): one Commit, one Welcome, every member applies the Commit. Returns the joiner's Welcome time.
    fn add(&mut self, i: usize) -> Duration {
        let who = (i + 1) as u8;
        self.inc[i] = self.inc[i].wrapping_add(0x80);
        let g = head(&self.devs[0]);
        let n = self.intent;
        self.intent += 1;
        let body = admit_body(&g, n, who, self.inc[i]);
        let kp = self.devs[i].kp();
        let (commit, binding, state, welcome) =
            self.commit(CommitPlan::Add(vec![kp]), body.clone());
        self.receive(Some(i), &commit, &binding, &body);
        let t0 = Instant::now();
        join(&mut self.devs[i], &welcome, &state);
        self.members[i] = true;
        t0.elapsed()
    }

    /// Remove device `i`: one Commit; every other member applies it; the removed device learns it is out.
    fn remove(&mut self, i: usize) {
        let who = (i + 1) as u8;
        let g = head(&self.devs[0]);
        let n = self.intent;
        self.intent += 1;
        let body = remove_body(&g, n, who, self.inc[i]);
        let (commit, binding, _s, _w) =
            self.commit(CommitPlan::Remove(vec![id16(who)]), body.clone());
        // the removed device sees the Commit too and learns it is out
        let pk = self.devs[0].pk();
        {
            let d = &mut self.devs[i];
            let out = d
                .app
                .group_receive_commit(&binding, &pk, &commit, &body, &[1; 32], &mut d.eng, env!(d))
                .unwrap();
            assert!(
                matches!(
                    out,
                    ReceiveOutcome::Applied {
                        still_member: false,
                        ..
                    }
                ),
                "{out:?}"
            );
        }
        self.members[i] = false;
        self.receive(Some(i), &commit, &binding, &body);
    }

    fn current(&self) -> Vec<usize> {
        (0..self.devs.len()).filter(|i| self.members[*i]).collect()
    }

    /// Every member on one epoch, one head hash and one exporter secret.
    fn assert_converged(&self) {
        let cur = self.current();
        let h0 = head(&self.devs[cur[0]]);
        let x0 = self.devs[cur[0]]
            .eng
            .export_secret(&GID, "om-load", b"ctx", 32)
            .unwrap();
        for &i in &cur {
            let d = &self.devs[i];
            assert_eq!(head(d).hash(), h0.hash(), "member {} head", i + 1);
            assert_eq!(d.eng.epoch(&GID).unwrap(), h0.mls_epoch_ref);
            assert_eq!(
                d.eng.export_secret(&GID, "om-load", b"ctx", 32).unwrap(),
                x0
            );
            assert_eq!(d.eng.members(&GID).unwrap().len(), cur.len());
        }
    }

    /// `count` messages from rotating senders; every other member reads each one; returns the encrypt and decrypt times.
    fn burst(&mut self, count: usize, seed: usize, enc: &mut Times, dec: &mut Times) -> usize {
        let cur = self.current();
        let mut reads = 0;
        for k in 0..count {
            let s = cur[(seed + k * 7) % cur.len()];
            let text = format!(
                "message {k} from {s} in epoch {}",
                self.devs[s].eng.epoch(&GID).unwrap()
            );
            let t0 = Instant::now();
            let ct = {
                let d = &mut self.devs[s];
                d.eng
                    .encrypt_application(env!(d), &GID, text.as_bytes())
                    .unwrap()
            };
            enc.add(t0.elapsed());
            for &r in &cur {
                if r == s {
                    continue;
                }
                let t0 = Instant::now();
                let got = self.devs[r].eng.decrypt_application(&GID, &ct).unwrap();
                dec.add(t0.elapsed());
                assert_eq!(got.plaintext, text.as_bytes());
                assert_eq!(got.sender, id16((s + 1) as u8));
                reads += 1;
            }
        }
        reads
    }
}

/// The whole scenario; returns the report lines. `size` members, `burst` messages per burst, `rounds` churn rounds of `churn` removals then re-admissions.
fn scenario(size: usize, burst: usize, rounds: usize, churn: usize) -> Report {
    let mut lines = vec![format!(
        "groups load: {size}-member Room, {burst} messages per burst, {rounds} churn rounds of {churn} removed and re-admitted"
    )];
    let rss0 = rss();
    let t_all = Instant::now();
    let (mut room, _g, joins) = Room::new(size);
    let built = t_all.elapsed();
    room.assert_converged();
    let rss_built = rss();
    lines.push(format!(
        "  build: {:.2} s for {} admissions (one Commit and one Welcome each, applied by every earlier member); epoch {}",
        built.as_secs_f64(),
        size - 1,
        room.devs[0].eng.epoch(&GID).unwrap()
    ));
    lines.push(joins.line("welcome join (joiner)"));
    lines.push(room.commit_prepare.line("commit prepare+accept (admin)"));
    lines.push(room.commit_receive.line("commit apply (receiver)"));

    // bursts at the full size
    let (mut enc, mut dec) = (Times::default(), Times::default());
    let t0 = Instant::now();
    let reads = room.burst(burst, 0, &mut enc, &mut dec);
    let secs = t0.elapsed().as_secs_f64();
    lines.push(format!("  burst of {burst} at {size} members: {reads} reads in {secs:.2} s ({:.0} messages/s sent, {:.0} reads/s)", burst as f64 / secs, reads as f64 / secs));
    lines.push(enc.line("encrypt"));
    lines.push(dec.line("decrypt"));
    let rss_burst = rss();

    // churn: remove `churn` members, send a burst (removed ones read nothing), re-admit them, send another
    let mut removed_reads_refused = 0;
    for round in 0..rounds {
        let victims: Vec<usize> = (0..churn)
            .map(|k| 1 + (round * churn + k) % (size - 1))
            .collect();
        for &v in &victims {
            room.remove(v);
        }
        room.assert_converged();
        let (mut e2, mut d2) = (Times::default(), Times::default());
        room.burst(burst / 4 + 1, round, &mut e2, &mut d2);
        // a removed member can read nothing sent after its removal
        let sender = room.current()[0];
        let ct = {
            let d = &mut room.devs[sender];
            d.eng
                .encrypt_application(env!(d), &GID, b"after the removal")
                .unwrap()
        };
        for &v in &victims {
            assert!(room.devs[v].eng.decrypt_application(&GID, &ct).is_err());
            removed_reads_refused += 1;
        }
        for &v in &victims {
            room.add(v);
        }
        room.assert_converged();
        room.burst(burst / 4 + 1, round + 3, &mut e2, &mut d2);
    }
    lines.push(format!(
        "  churn: {} removals and {} re-admissions over {rounds} rounds; {removed_reads_refused} reads by removed members refused; final epoch {}",
        rounds * churn,
        rounds * churn,
        room.devs[0].eng.epoch(&GID).unwrap()
    ));
    lines.push(room.commit_prepare.line("commit prepare+accept (all)"));
    lines.push(room.commit_receive.line("commit apply (all)"));
    let rss_end = rss();

    // storage: what each device would persist (engine snapshot and the whole app state)
    let snaps: Vec<usize> = room.devs.iter().map(|d| d.eng.snapshot().len()).collect();
    let apps: Vec<usize> = room
        .devs
        .iter()
        .map(|d| serde_json::to_string(&d.app).unwrap().len())
        .collect();
    let avg = |v: &[usize]| v.iter().sum::<usize>() / v.len();
    lines.push(format!(
        "  persisted per device: engine snapshot avg {} KiB (max {} KiB), app state avg {} KiB (max {} KiB)",
        avg(&snaps) / 1024,
        snaps.iter().max().unwrap() / 1024,
        avg(&apps) / 1024,
        apps.iter().max().unwrap() / 1024
    ));
    // a restored engine keeps working (crash and restart of the busiest device)
    let busiest = room.devs[0].eng.snapshot();
    let mut again = OpenMlsLoader::restore(&busiest).unwrap();
    let sender = 1usize;
    let ct = {
        let d = &mut room.devs[sender];
        d.eng
            .encrypt_application(env!(d), &GID, b"after restore")
            .unwrap()
    };
    assert_eq!(
        again.decrypt_application(&GID, &ct).unwrap().plaintext,
        b"after restore"
    );
    lines.push(format!(
        "  memory (resident, whole test process, {size} engines and states): start {:.1} MiB, built {:.1} MiB, after burst {:.1} MiB, end {:.1} MiB; total {:.1} s",
        rss0 as f64 / 1048576.0,
        rss_built as f64 / 1048576.0,
        rss_burst as f64 / 1048576.0,
        rss_end as f64 / 1048576.0,
        t_all.elapsed().as_secs_f64()
    ));
    Report { lines }
}

#[test]
fn a_32_member_room_converges_under_bursts_and_membership_churn() {
    // Sized to finish in a debug build; the --ignored report below runs the larger one.
    let r = scenario(32, 8, 1, 2);
    for l in &r.lines {
        eprintln!("{l}");
    }
}

#[test]
#[ignore = "load report: run with --release --ignored --nocapture"]
fn load_report_32_members() {
    let burst: usize = std::env::var("OM_GROUP_BURST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let rounds: usize = std::env::var("OM_GROUP_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let r = scenario(32, burst, rounds, 4);
    for l in &r.lines {
        println!("{l}");
    }
}
