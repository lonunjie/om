// SPDX-License-Identifier: MPL-2.0
//! Shared world for the owner-recovery tests: Alice (a surviving controller) with a recovery place and Bob as a contact,
//! and a fresh device that lost everything.
#![allow(dead_code, unused_imports)]

use super::*;
use om_core::api::*;
use om_core::host::MonotonicSample;
use om_core::recovery_record::{RecordKeys, RecoveryPlace};
use om_core::recovery_sheet::Sheet;
use om_core::runtime::{LoopbackNetwork, Runtime, settle_all};

pub const SCOPE: [u8; 32] = [7; 32];
pub const H: u64 = 3_600_000;
pub const DELAY_MS: u64 = 72 * H;

pub fn place() -> RecoveryPlace {
    RecoveryPlace {
        host: "home.example".into(),
        port: None,
        scope: SCOPE,
    }
}

pub fn mono(session: u8, ms: u64) -> MonotonicSample {
    MonotonicSample {
        boot_session: vec![session; 8],
        elapsed_ms: ms,
    }
}

pub struct World {
    pub net: LoopbackNetwork,
    pub alice: Runtime,
    pub a: ProfileExecutionContext,
    pub bob: Runtime,
    pub b: ProfileExecutionContext,
    pub sheet: String,
    pub keys: RecordKeys,
}

/// Alice with a recovery place and Bob as an active contact; the key is confirmed saved, so the record (with the
/// snapshot of Bob) is on the Home.
pub fn world() -> World {
    world_with(true)
}

/// `with_place: false` is an account made while the Home offered no recovery place: it has no kit and no sheet.
pub fn world_with(with_place: bool) -> World {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    if with_place {
        alice.set_recovery_place(Some(place()));
    }
    let a = super::create_profile(&mut alice, "p-a", "Alice");
    let key: RecoveryKeyView = alice
        .query::<Option<RecoveryKeyView>>(Query::GetRecoveryKey {
            ctx: a.clone(),
            key_operation_id: "create-1".into(),
        })
        .expect("key");
    let mut bob = Runtime::new("bob", &net);
    let b = super::create_profile(&mut bob, "p-b", "Bob");
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
    assert_eq!(net.recovery_puts().len(), usize::from(with_place), "the record is written only with a place");
    let root = root_of(&key.groups);
    World {
        net,
        alice,
        a,
        bob,
        b,
        sheet: if with_place { key.sheet.expect("sheet") } else { String::new() },
        keys: RecordKeys::derive(&root, &SCOPE),
    }
}

/// The RecoveryRoot a displayed key encodes (the first 32 bytes of its groups).
pub fn root_of(groups: &[String]) -> [u8; 32] {
    const A: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = String::new();
    for c in groups.concat().chars() {
        bits.push_str(&format!("{:05b}", A.find(c).unwrap()));
    }
    let v: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap())
        .collect();
    <[u8; 32]>::try_from(v).unwrap()
}

pub struct Fresh {
    pub rt: Runtime,
    pub ctx: ProfileExecutionContext,
}

pub fn fresh(net: &LoopbackNetwork) -> Fresh {
    let mut rt = Runtime::new("fresh", net);
    let ctx = rt.open("q");
    Fresh { rt, ctx }
}

impl Fresh {
    pub fn go(&mut self, op: &str, body: IntentBody) -> Option<OutcomeClass> {
        self.rt.dispatch(&self.ctx, op, body);
        self.rt.flush_net(&self.ctx);
        self.rt.outcome(&self.ctx, op)
    }
    pub fn begin(&mut self, sheet: &str) {
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
    pub fn view(&self) -> Option<RecoveringView> {
        self.rt
            .query::<RecoveryStatusView>(Query::GetRecoveryStatus {
                ctx: self.ctx.clone(),
            })
            .recovering
    }
    pub fn tick(&mut self, m: &MonotonicSample) {
        self.rt.recovery_tick(&self.ctx, m);
        // a round of reads, then whatever it leads to
        for _ in 0..4 {
            self.rt.flush_net(&self.ctx);
        }
    }
    pub fn app(&self) -> serde_json::Value {
        self.rt.core.debug_app_state(&self.ctx).unwrap()
    }
}

/// The whole approval ceremony, start to adoption: the restricted device asks, Alice's controller offers, the device
/// joins, the person confirms, and every party settles.
pub fn adopt_via_approval(w: &mut World, f: &mut Fresh) {
    let settle = |w: &mut World, f: &mut Fresh| {
        settle_all(&mut [
            (&mut w.alice, &w.a),
            (&mut w.bob, &w.b),
            (&mut f.rt, &f.ctx),
        ]);
    };
    assert_eq!(
        f.go("req", IntentBody::RequestRecovery),
        Some(OutcomeClass::Committed)
    );
    w.alice
        .dispatch(&w.a, "off", IntentBody::OfferRecoveryApproval);
    w.alice.flush_net(&w.a);
    let view = |w: &World| -> RecoveryApprovalOfferView {
        w.alice
            .query::<Option<RecoveryApprovalOfferView>>(Query::GetRecoveryApprovalOffer {
                ctx: w.a.clone(),
                operation_id: "off".into(),
            })
            .expect("offer")
    };
    let link = view(w).link.expect("link");
    f.go("join", IntentBody::JoinRecoveryApproval { link });
    settle(w, f);
    let rid = view(w).request_id.expect("request");
    let c = super::confirmation(&mut w.alice, &w.a, "ConfirmRecovery", &rid);
    w.alice.dispatch(
        &w.a,
        "confirm",
        IntentBody::ConfirmRecovery {
            request_id: rid,
            confirmation: c,
        },
    );
    for _ in 0..4 {
        settle(w, f);
        f.rt.flush_net(&f.ctx);
    }
}
