// SPDX-License-Identifier: MPL-2.0
//! Portability slice 4: a stale archive cannot lower a floor, resurrect a revoked device or override a newer control state;
//! only an already authorized device rehydrates; ViewOnce and expiry rules hold.
use om_archive_reader::import::*;
use om_archive_reader::rehydrate::*;

struct Live;
impl LiveControl for Live {
    fn live_version(&self, s: &str) -> Option<u64> {
        (s == "account").then_some(7)
    }
}
fn view() -> ImportView {
    ImportView {
        objects: vec![
            (
                "objects/old.cbor".into(),
                ObjectStatus::StaleControl {
                    archived: 3,
                    live: 7,
                },
            ),
            (
                "objects/new.cbor".into(),
                ObjectStatus::ControlNeedsLiveValidation { archived: 9 },
            ),
            ("objects/msg.cbor".into(), ObjectStatus::VerifiedHistory),
        ],
        ..Default::default()
    }
}
fn item(id: &str) -> Item {
    Item {
        id: id.into(),
        view_once: false,
        expiry: None,
        device: None,
    }
}
fn cx<'a>(live: &'a Live, revoked: &'a [String]) -> Context<'a> {
    Context {
        authorized_device: true,
        account_matches: true,
        now: 1000,
        can_enforce_expiry: true,
        live,
        revoked_devices: revoked,
    }
}

#[test]
fn only_an_authorized_device_for_the_same_account_rehydrates() {
    let l = Live;
    let mut c = cx(&l, &[]);
    c.authorized_device = false;
    assert_eq!(
        plan(&view(), &[item("a")], &c),
        Err(Refusal::NotAuthorizedDevice)
    );
    let mut c = cx(&l, &[]);
    c.account_matches = false;
    assert_eq!(plan(&view(), &[item("a")], &c), Err(Refusal::WrongAccount));
}

#[test]
fn control_objects_are_history_or_live_validation_never_applied() {
    let l = Live;
    let p = plan(&view(), &[item("a")], &cx(&l, &[])).unwrap();
    assert_eq!(p.stale_control, vec!["objects/old.cbor"]);
    assert_eq!(p.needs_live_validation, vec!["objects/new.cbor"]);
    assert!(p.fresh_sessions_required);
    assert_eq!(p.import, vec!["a"]);
}

#[test]
fn a_floor_always_comes_from_live_state() {
    assert_eq!(
        floor_after(Some(7), 3),
        Some(7),
        "an older archive cannot lower it"
    );
    assert_eq!(
        floor_after(Some(7), 99),
        Some(7),
        "a newer archive cannot raise it"
    );
    assert_eq!(floor_after(None, 5), None);
}

#[test]
fn history_from_a_revoked_device_is_kept_as_history_only() {
    let l = Live;
    let revoked = vec!["dev-old".to_string()];
    let mut it = item("m1");
    it.device = Some("dev-old".into());
    let p = plan(&view(), &[it], &cx(&l, &revoked)).unwrap();
    assert_eq!(p.import, vec!["m1"]);
    assert_eq!(p.history_only_revoked_device, vec!["m1"]);
}

#[test]
fn view_once_is_never_rehydrated_and_disappearing_items_follow_their_expiry() {
    let l = Live;
    let mut v = item("once");
    v.view_once = true;
    let mut gone = item("gone");
    gone.expiry = Some(1000);
    let mut soon = item("soon");
    soon.expiry = Some(5000);
    let p = plan(&view(), &[v, gone, soon, item("plain")], &cx(&l, &[])).unwrap();
    assert_eq!(p.refused_view_once, vec!["once"]);
    assert_eq!(p.dropped_expired, vec!["gone"]);
    assert_eq!(p.schedule_deletion, vec![("soon".to_string(), 5000)]);
    assert_eq!(p.import, vec!["soon", "plain"]);
}

#[test]
fn an_importer_that_cannot_enforce_expiry_skips_disappearing_items() {
    let l = Live;
    let mut c = cx(&l, &[]);
    c.can_enforce_expiry = false;
    let mut soon = item("soon");
    soon.expiry = Some(5000);
    let p = plan(&view(), &[soon, item("plain")], &c).unwrap();
    assert_eq!(p.skipped_unenforceable, vec!["soon"]);
    assert_eq!(p.import, vec!["plain"]);
    assert!(p.schedule_deletion.is_empty());
}
