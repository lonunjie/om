// SPDX-License-Identifier: MPL-2.0
//! The owner side of lost-all-devices recovery, first part (ADR-0199 section 4, ADR-0200 section 8): a fresh device
//! enters the key sheet, asks the Home for the record, opens and checks it. Against the loopback RecoveryStore.

mod common;

use om_core::api::*;
use om_core::recovery_record::{RecordKeys, RecoveryPlace};
use om_core::recovery_sheet::Sheet;
use om_core::runtime::{LoopbackNetwork, Runtime};
use om_core::state::JoinStage;

const SCOPE: [u8; 32] = [7; 32];

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

/// An account whose record is written to the loopback Home; its sheet text and key.
struct Owner {
    net: LoopbackNetwork,
    sheet: String,
    root: [u8; 32],
    keys: RecordKeys,
}

fn owner() -> Owner {
    let net = LoopbackNetwork::default();
    let mut rt = Runtime::new("owner", &net);
    rt.set_recovery_place(Some(place()));
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
        .expect("key");
    rt.dispatch(
        &ctx,
        "saved",
        IntentBody::ConfirmRecoveryKeyStored {
            key_operation_id: "create".into(),
        },
    );
    rt.flush_net(&ctx);
    let root = root_from_groups(&key.groups);
    assert_eq!(net.recovery_puts().len(), 1, "the record is written");
    Owner {
        net,
        sheet: key.sheet.expect("the sheet text"),
        root,
        keys: RecordKeys::derive(&root, &SCOPE),
    }
}

/// The older key-only text: groups of four of the 35 bytes (root and its 3-byte check), as shown at creation.
fn key_text(root: &[u8; 32]) -> String {
    const A: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let chk = om_core::crypto::sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", root]);
    let mut raw = root.to_vec();
    raw.extend_from_slice(&chk[..3]);
    let mut bits = String::new();
    for b in raw {
        bits.push_str(&format!("{b:08b}"));
    }
    let chars: Vec<char> = bits
        .as_bytes()
        .chunks(5)
        .map(|c| A[usize::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()] as char)
        .collect();
    chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
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
    fn begin(&mut self, key: &str, home: &str, place: Option<&str>) -> Option<OutcomeClass> {
        self.rt.dispatch(
            &self.ctx,
            "begin",
            IntentBody::BeginRecovery {
                recovery_key: key.into(),
                home: home.into(),
                place: place.map(str::to_string),
            },
        );
        self.rt.flush_net(&self.ctx);
        self.rt.outcome(&self.ctx, "begin")
    }
    fn stage(&self) -> Option<JoinStage> {
        self.rt
            .query::<RecoveryStatusView>(Query::GetRecoveryStatus {
                ctx: self.ctx.clone(),
            })
            .recovering
            .map(|r| r.stage)
    }
}

#[test]
fn a_sheet_finds_the_record_and_keeps_the_restricted_state() {
    let o = owner();
    let mut f = fresh(&o.net);
    assert_eq!(f.begin(&o.sheet, "", None), Some(OutcomeClass::Committed));
    assert_eq!(f.stage(), Some(JoinStage::Found));
    let v =
        f.rt.query::<RecoveryStatusView>(Query::GetRecoveryStatus { ctx: f.ctx.clone() });
    assert_eq!(v.recovering.unwrap().home, "https://home.example");
    // The restricted state grants nothing: nothing more is pending and there is no account to list.
    assert_eq!(f.rt.flush_net(&f.ctx), 0);
}

#[test]
fn a_restart_keeps_the_result_and_asks_nothing_again() {
    let o = owner();
    let mut f = fresh(&o.net);
    f.begin(&o.sheet, "", None);
    let ctxs = f.rt.restart();
    f.ctx = ctxs.into_iter().next().unwrap_or_else(|| f.rt.open("q"));
    assert_eq!(f.stage(), Some(JoinStage::Found));
}

#[test]
fn the_text_forms_and_the_key_alone_with_a_place_string_all_find_it() {
    let o = owner();
    // the key alone plus a place-only string (ADR-0200 section 3)
    let place_only = Sheet::place_only(SCOPE, None, "home.example")
        .unwrap()
        .to_sheet_form();
    let legacy_key = {
        // the older key text: the groups of the shown key
        key_text(&o.root)
    };
    let mut f = fresh(&o.net);
    assert_eq!(
        f.begin(&legacy_key, "", Some(&place_only)),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.stage(), Some(JoinStage::Found));
    // the compact form of the sheet too
    let compact = Sheet::full(o.root, SCOPE, None, "home.example")
        .unwrap()
        .to_compact();
    let mut g = fresh(&o.net);
    assert_eq!(g.begin(&compact, "", None), Some(OutcomeClass::Committed));
    assert_eq!(g.stage(), Some(JoinStage::Found));
}

#[test]
fn a_key_alone_with_no_place_waits_for_one_and_asks_the_home_nothing() {
    let o = owner();
    let legacy_key = key_text(&o.root);
    let mut f = fresh(&o.net);
    assert_eq!(
        f.begin(&legacy_key, "https://home.example", None),
        Some(OutcomeClass::Committed)
    );
    assert_eq!(f.stage(), Some(JoinStage::NeedsPlace));
    assert_eq!(f.rt.flush_net(&f.ctx), 0, "nothing was asked of the Home");
}

#[test]
fn wrong_material_is_one_answer_and_leaves_nothing_usable() {
    let o = owner();
    // another key at the same place: the Home knows no such record
    let other = Sheet::full([9; 32], SCOPE, None, "home.example")
        .unwrap()
        .to_sheet_form();
    let mut f = fresh(&o.net);
    f.begin(&other, "", None);
    assert_eq!(f.stage(), Some(JoinStage::NotFound));
    // a place alone is not a key; a damaged line is refused and keeps nothing
    let place_only = Sheet::place_only(SCOPE, None, "home.example")
        .unwrap()
        .to_sheet_form();
    let mut g = fresh(&o.net);
    assert_eq!(
        g.begin(&place_only, "", None),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    assert_eq!(g.stage(), None);
    let mut damaged = o.sheet.clone();
    let i = damaged.find(['A', 'B']).unwrap_or(10);
    damaged.replace_range(i..i + 1, if &damaged[i..i + 1] == "A" { "C" } else { "A" });
    let mut h = fresh(&o.net);
    assert_eq!(
        h.begin(&damaged, "", None),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
    // no address of any kind and a bare key: nothing to look up, nothing kept
    let key_only = key_text(&o.root);
    let mut k = fresh(&o.net);
    assert_eq!(
        k.begin(&key_only, "", None),
        Some(OutcomeClass::Rejected {
            reason: RejectReason::Invalid
        })
    );
}

#[test]
fn a_tampered_record_cannot_be_read_and_is_never_used() {
    let o = owner();
    let (_, mut bytes) = o.net.recovery_record(&o.keys.locator).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 1;
    o.net.recovery_replace(&o.keys.locator, bytes);
    let mut f = fresh(&o.net);
    f.begin(&o.sheet, "", None);
    assert_eq!(f.stage(), Some(JoinStage::CantRead));
}

#[test]
fn a_missing_record_is_not_found() {
    let net = LoopbackNetwork::default();
    let sheet = Sheet::full([5; 32], SCOPE, None, "home.example")
        .unwrap()
        .to_sheet_form();
    let mut f = fresh(&net);
    f.begin(&sheet, "", None);
    assert_eq!(f.stage(), Some(JoinStage::NotFound));
}
