// SPDX-License-Identifier: MPL-2.0
//! TEST ONLY: an in-memory loopback provider (key material, introduction service, relay mailboxes), an in-memory
//! encrypted-store stand-in and a synchronous driver for the sans-IO [`Core`]. Compiled only with the
//! `test-loopback` feature, which cannot be combined with `production` (R-072). Real hosts implement the same
//! `NetCall` / `Commit` contract over the PlatformAdapter.

use crate::api::*;
use crate::crypto::rng::insecure_test_rng::InsecureTestRng;
use crate::host::*;
use crate::{Core, crypto::hex};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

pub const TEST_ONLY_MARKER: &str = "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION";

#[derive(Default)]
struct Published {
    bundle: Vec<u8>,
    opks: Vec<Vec<u8>>,
    pqopks: Vec<Vec<u8>>,
}

#[derive(Default)]
struct Mailbox {
    next_seq: u64,
    items: Vec<(u64, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Reject the call before doing anything (definitely not performed).
    Unavailable,
    /// Perform the call but report a timeout (the bytes were delivered; outcome unknown to the caller).
    TimeoutAfterEffect,
    /// Report a timeout without performing it.
    TimeoutBeforeEffect,
}

#[derive(Default)]
struct NetState {
    keys: BTreeMap<Vec<u8>, Published>,
    invites: BTreeMap<Vec<u8>, Vec<u8>>,
    mailboxes: BTreeMap<Vec<u8>, Mailbox>,
    faults: Vec<Fault>,
    deposits: u64,
    /// Every deposit a relay ever received, in order (the relay-visible byte stream; mailboxes are drained by acks).
    log: Vec<(Vec<u8>, Vec<u8>)>,
}

/// One loopback provider shared by every client in a test (cheap to clone).
#[derive(Clone, Default)]
pub struct LoopbackNetwork(Rc<RefCell<NetState>>);

impl LoopbackNetwork {
    /// Queue a fault for the next network call of any kind.
    pub fn inject(&self, f: Fault) {
        self.0.borrow_mut().faults.push(f);
    }
    pub fn deposit_count(&self) -> u64 {
        self.0.borrow().deposits
    }
    /// Everything a relay could see for a mailbox (for ciphertext-only assertions).
    pub fn mailbox_bytes(&self, mailbox: &[u8]) -> Vec<Vec<u8>> {
        self.0
            .borrow()
            .mailboxes
            .get(mailbox)
            .map(|m| m.items.iter().map(|(_, b)| b.clone()).collect())
            .unwrap_or_default()
    }
    /// Every byte string any relay mailbox ever received, in deposit order (acks free relay storage, not this log).
    pub fn all_deposited_bytes(&self) -> Vec<Vec<u8>> {
        self.0.borrow().log.iter().map(|(_, b)| b.clone()).collect()
    }
    /// The deposit log with its destination mailbox.
    pub fn deposit_log(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.0.borrow().log.clone()
    }
    /// One-time prekeys the key-material provider still holds, per published device: (device id, EC items, PQ items).
    pub fn key_inventory(&self) -> Vec<(Vec<u8>, usize, usize)> {
        self.0
            .borrow()
            .keys
            .iter()
            .map(|(d, p)| (d.clone(), p.opks.len(), p.pqopks.len()))
            .collect()
    }
    /// Hostile introduction service: exchange the descriptors stored under two invite ids.
    pub fn swap_invites(&self, a: &[u8], b: &[u8]) {
        let mut s = self.0.borrow_mut();
        if let (Some(x), Some(y)) = (s.invites.get(a).cloned(), s.invites.get(b).cloned()) {
            s.invites.insert(a.to_vec(), y);
            s.invites.insert(b.to_vec(), x);
        }
    }
    pub fn revoke_all_invites(&self) {
        self.0.borrow_mut().invites.clear();
    }

    pub fn perform(&self, call: &NetCall) -> NetResult {
        let mut s = self.0.borrow_mut();
        let fault = if s.faults.is_empty() {
            None
        } else {
            Some(s.faults.remove(0))
        };
        match fault {
            Some(Fault::Unavailable) => {
                return NetResult::Err {
                    error: NetError::Unavailable,
                };
            }
            Some(Fault::TimeoutBeforeEffect) => {
                return NetResult::Err {
                    error: NetError::Timeout,
                };
            }
            _ => {}
        }
        let r = match call {
            NetCall::PublishKeyMaterial {
                device_id,
                bundle,
                opks,
                pqopks,
            } => {
                let p = s.keys.entry(device_id.clone()).or_default();
                p.bundle = bundle.clone();
                p.opks.extend(opks.iter().cloned());
                p.pqopks.extend(pqopks.iter().cloned());
                NetResult::Ok
            }
            NetCall::FetchKeyMaterial { device_id } => match s.keys.get_mut(device_id) {
                None => NetResult::Err {
                    error: NetError::NotFound,
                },
                Some(p) => {
                    let mut out = vec![p.bundle.clone()];
                    if !p.opks.is_empty() {
                        out.push(p.opks.remove(0));
                    }
                    if !p.pqopks.is_empty() {
                        out.push(p.pqopks.remove(0));
                    }
                    NetResult::Many { items: out }
                }
            },
            NetCall::PutInvite {
                invite_id,
                descriptor,
                ..
            } => {
                s.invites.insert(invite_id.clone(), descriptor.clone());
                NetResult::Ok
            }
            NetCall::GetInvite { invite_id } => match s.invites.get(invite_id) {
                Some(d) => NetResult::Bytes { bytes: d.clone() },
                None => NetResult::Err {
                    error: NetError::NotFound,
                },
            },
            NetCall::RevokeInvite { invite_id } => {
                s.invites.remove(invite_id);
                NetResult::Ok
            }
            NetCall::Deposit { mailbox, bytes } => {
                s.deposits += 1;
                s.log.push((mailbox.clone(), bytes.clone()));
                let m = s.mailboxes.entry(mailbox.clone()).or_default();
                m.next_seq += 1;
                let seq = m.next_seq;
                m.items.push((seq, bytes.clone()));
                NetResult::Ok
            }
            NetCall::Ack { mailbox, seq } => {
                if let Some(m) = s.mailboxes.get_mut(mailbox) {
                    m.items.retain(|(q, _)| q > seq);
                }
                NetResult::Ok
            }
        };
        if fault == Some(Fault::TimeoutAfterEffect) {
            return NetResult::Err {
                error: NetError::Timeout,
            };
        }
        r
    }

    pub fn poll(&self, mailbox: &[u8], after: u64) -> Vec<Inbound> {
        self.0
            .borrow()
            .mailboxes
            .get(mailbox)
            .map(|m| {
                m.items
                    .iter()
                    .filter(|(q, _)| *q > after)
                    .map(|(q, b)| Inbound {
                        seq: *q,
                        bytes: b.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Per-profile durable records, standing in for the host's EncryptedKv. Tests can make the next commit fail.
#[derive(Clone, Default)]
pub struct MemStore(Rc<RefCell<MemStoreInner>>);

#[derive(Default)]
struct MemStoreInner {
    data: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    fail_commits: u32,
}

impl MemStore {
    pub fn fail_next_commit(&self) {
        self.0.borrow_mut().fail_commits += 1;
    }
    pub fn records(&self, profile: &str) -> Vec<Record> {
        self.0
            .borrow()
            .data
            .get(profile)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| Record {
                        key: k.clone(),
                        value: v.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// All stored bytes of a profile (for "no plaintext/secret at rest outside the sealed record" style checks).
    pub fn raw(&self, profile: &str) -> Vec<Vec<u8>> {
        self.0
            .borrow()
            .data
            .get(profile)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
    fn commit(&self, c: &Commit) -> bool {
        let mut s = self.0.borrow_mut();
        if s.fail_commits > 0 {
            s.fail_commits -= 1;
            return false;
        }
        let m = s.data.entry(c.profile_id.clone()).or_default();
        for w in &c.writes {
            match &w.value {
                Some(v) => m.insert(w.key.clone(), v.clone()),
                None => m.remove(&w.key),
            };
        }
        true
    }
}

pub fn home() -> HomeSummary {
    HomeSummary {
        tier: "T1".into(),
        observers: vec![
            HomeObserverCode::HostSeesMetadata,
            HomeObserverCode::HostSeesCiphertext,
        ],
        migration_available: true,
        client_origin_choice: vec![ClientOriginChoiceCode::DefaultOrigin],
    }
}

/// A synchronous host around [`Core`]: commits through the store, performs the outbox against the loopback
/// provider and polls mailboxes. `restart` throws the Core away and rebuilds it from the store alone.
pub struct Runtime {
    pub core: Core<InsecureTestRng>,
    pub store: MemStore,
    pub net: LoopbackNetwork,
    pub clock: Rc<Cell<u64>>,
    label: String,
    boots: u32,
    open: Vec<String>,
}

impl Runtime {
    pub fn new(label: &str, net: &LoopbackNetwork) -> Self {
        let clock = Rc::new(Cell::new(1_790_000_000_000u64));
        let c2 = clock.clone();
        Runtime {
            core: Core::new(
                InsecureTestRng::from_label(format!("{label}/0").as_bytes()),
                move || c2.get(),
                home(),
            ),
            store: MemStore::default(),
            net: net.clone(),
            clock,
            label: label.to_string(),
            boots: 0,
            open: vec![],
        }
    }

    pub fn tick(&self, ms: u64) {
        self.clock.set(self.clock.get() + ms);
    }

    pub fn open(&mut self, profile: &str) -> ProfileExecutionContext {
        let recs = self.store.records(profile);
        let ctx = self
            .core
            .open_profile(profile, &recs)
            .expect("open profile");
        if !self.open.iter().any(|p| p == profile) {
            self.open.push(profile.to_string());
        }
        ctx
    }

    /// Process restart: a new Core (new RNG stream) over the same durable records.
    pub fn restart(&mut self) -> Vec<ProfileExecutionContext> {
        self.boots += 1;
        let c2 = self.clock.clone();
        self.core = Core::new(
            InsecureTestRng::from_label(format!("{}/{}", self.label, self.boots).as_bytes()),
            move || c2.get(),
            home(),
        );
        let open = self.open.clone();
        open.iter().map(|p| self.open(p)).collect()
    }

    /// Commit a step through the store. Returns false when the host could not persist it (nothing released).
    pub fn settle(&mut self, step: &Step) -> bool {
        match &step.commit {
            None => true,
            Some(c) => {
                let ok = self.store.commit(c);
                self.core.finish_commit(c.id, ok).expect("finish commit");
                ok
            }
        }
    }

    pub fn dispatch(
        &mut self,
        ctx: &ProfileExecutionContext,
        op: &str,
        body: IntentBody,
    ) -> OperationHandle {
        let step = self
            .core
            .dispatch(&Intent {
                ctx: ctx.clone(),
                operation_id: op.to_string(),
                body,
            })
            .expect("dispatch");
        self.settle(&step);
        step.handle.expect("handle")
    }

    pub fn outcome(&self, ctx: &ProfileExecutionContext, op: &str) -> Option<OutcomeClass> {
        self.core.outcome(ctx, op).expect("outcome")
    }

    pub fn query<T: serde::de::DeserializeOwned>(&self, q: Query) -> T {
        serde_json::from_value(self.core.query(&q).expect("query")).expect("query result shape")
    }

    /// Perform every owed network call once (in outbox order) and feed the results back.
    pub fn flush_net(&mut self, ctx: &ProfileExecutionContext) -> usize {
        let ops = self.core.net_ops(ctx).expect("net ops");
        let n = ops.len();
        for op in ops {
            let result = self.net.perform(&op.call);
            let step = self
                .core
                .net_result(ctx, op.id, result)
                .expect("net result");
            self.settle(&step);
        }
        n
    }

    /// Poll every mailbox once.
    pub fn poll(&mut self, ctx: &ProfileExecutionContext) -> usize {
        let mut n = 0;
        for t in self.core.poll_targets(ctx).expect("poll targets") {
            let items = self.net.poll(&t.mailbox, t.after);
            if items.is_empty() {
                continue;
            }
            n += items.len();
            let step = self.core.inbound(ctx, &t.mailbox, &items).expect("inbound");
            self.settle(&step);
        }
        n
    }

    /// One full cycle: owed network work, then polling.
    pub fn pump(&mut self, ctx: &ProfileExecutionContext) -> usize {
        self.flush_net(ctx) + self.poll(ctx)
    }
}

/// Alternate `pump` over several (runtime, context) pairs until nothing moves.
pub fn settle_all(parties: &mut [(&mut Runtime, &ProfileExecutionContext)]) {
    for _ in 0..40 {
        let mut moved = 0;
        for (rt, ctx) in parties.iter_mut() {
            moved += rt.pump(ctx);
        }
        if moved == 0 {
            return;
        }
    }
    panic!("network did not settle");
}

pub fn mailbox_hex(b: &[u8]) -> String {
    hex(b)
}
