// SPDX-License-Identifier: MPL-2.0
//! `communities_publications` slice 3: bilateral links, the binding value and delegated-intent validation against the
//! ADR-0160 `delegated-reduction` vector and the V-008 cases (no unconsenting Room, no protected capability, stale head
//! fails closed, unlink ends the binding, sub-delegation rejected unless opted in).
use om_codec::cbor::unhex;
use om_core::space_binding::*;
use std::collections::BTreeSet;

const SPACE: [u8; 32] = [0xaa; 32];
const HEAD: [u8; 32] = [0xbb; 32];

fn vector_body() -> Vec<u8> {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/prototypes/v008-space-wire/python_codec.py"
    );
    let out = std::process::Command::new("python3")
        .args(["-I", "-c", "import json,sys;sys.path.insert(0,sys.argv[1]);import python_codec as c;v=json.load(open(sys.argv[1]+'/vectors.json'));print(json.dumps({x['id']:c.enc(c.norm(x['value'])).hex() for x in v['positive']}))"])
        .arg(std::path::Path::new(p).parent().unwrap())
        .output()
        .expect("python3");
    assert!(out.status.success());
    let m: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    unhex(m["delegated-reduction"].as_str().unwrap())
}
fn official() -> ChildLink {
    let mut l = ChildLink::proposed_by_space();
    l.child_accepts();
    l
}
fn binding(caps: &[&str], subdelegation: bool) -> Binding {
    Binding::new(
        &official(),
        SPACE,
        BTreeSet::from([(1, vec![0x11; 32])]),
        caps.iter().map(|c| c.to_string()).collect(),
        3600,
        subdelegation,
    )
    .unwrap()
}

#[test]
fn a_link_is_official_only_while_both_sides_record_it() {
    let mut l = ChildLink::proposed_by_space();
    assert!(
        !l.is_official(),
        "a Space cannot claim an unconsenting Room"
    );
    l.child_accepts();
    assert!(l.is_official());
    l.child_revokes();
    assert!(!l.is_official());
    let mut l = official();
    l.space_revokes();
    assert!(!l.is_official());
}

#[test]
fn a_binding_needs_an_official_link_and_never_lists_a_protected_capability() {
    let unofficial = ChildLink::proposed_by_space();
    let caps: BTreeSet<String> = ["room.remove_member".to_string()].into();
    assert_eq!(
        Binding::new(&unofficial, SPACE, BTreeSet::new(), caps.clone(), 60, false),
        Err(BindingError::LinkNotOfficial)
    );
    for p in PROTECTED {
        let c: BTreeSet<String> = [p.to_string()].into();
        assert_eq!(
            Binding::new(&official(), SPACE, BTreeSet::new(), c, 60, false),
            Err(BindingError::Protected)
        );
    }
    assert_eq!(
        Binding::new(
            &official(),
            SPACE,
            BTreeSet::new(),
            BTreeSet::new(),
            60,
            false
        ),
        Err(BindingError::EmptyCapabilities)
    );
}

#[test]
fn a_delegated_reduction_within_the_binding_validates() {
    let d = validate_delegated_intent(
        &vector_body(),
        &binding(&["room.remove_member", "room.ban_member"], false),
        (HEAD, 7),
        60,
        false,
    )
    .unwrap();
    assert!(d.reducing);
    assert_eq!(d.operation, "control.member.remove");
    assert!(d.capabilities.contains("room.remove_member"));
}

#[test]
fn it_fails_closed_on_another_space_a_stale_or_different_head() {
    let b = binding(&["room.remove_member"], false);
    let body = vector_body();
    let mut other = b.clone();
    other.space_id = [0xcc; 32];
    assert_eq!(
        validate_delegated_intent(&body, &other, (HEAD, 7), 0, false),
        Err(IntentError::WrongSpace)
    );
    // the member holds a different head or version than the one the intent was validated against
    assert_eq!(
        validate_delegated_intent(&body, &b, ([0x01; 32], 7), 0, false),
        Err(IntentError::StaleHead)
    );
    assert_eq!(
        validate_delegated_intent(&body, &b, (HEAD, 8), 0, false),
        Err(IntentError::StaleHead)
    );
    // the held head is older than the freshness bound
    assert_eq!(
        validate_delegated_intent(&body, &b, (HEAD, 7), 3601, false),
        Err(IntentError::StaleHead)
    );
}

#[test]
fn a_capability_outside_the_binding_and_sub_delegated_evidence_are_refused() {
    let body = vector_body();
    let narrow = binding(&["room.ban_member"], false);
    assert_eq!(
        validate_delegated_intent(&body, &narrow, (HEAD, 7), 0, false),
        Err(IntentError::OutsideBinding)
    );
    let b = binding(&["room.remove_member"], false);
    assert_eq!(
        validate_delegated_intent(&body, &b, (HEAD, 7), 0, true),
        Err(IntentError::SubDelegated)
    );
    let opted = binding(&["room.remove_member"], true);
    assert!(validate_delegated_intent(&body, &opted, (HEAD, 7), 0, true).is_ok());
}

#[test]
fn garbage_is_malformed_and_unlink_consumes_the_binding() {
    let b = binding(&["room.remove_member"], false);
    assert_eq!(
        validate_delegated_intent(&[0xa0], &b, (HEAD, 7), 0, false),
        Err(IntentError::Malformed)
    );
    // a Space-side unlink ends the binding: it is moved, so it cannot be used again; re-linking needs `Binding::new`
    b.space_unlinked();
    let mut relink = official();
    relink.space_revokes();
    assert!(!relink.is_official());
}
