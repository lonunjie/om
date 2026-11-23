// SPDX-License-Identifier: MPL-2.0
//! `bridges` slice 5: forged or stale links never change authorship or merge accounts; "linked" is display only.
use om_codec::{Registry, Value};
use om_core::bridge_identity::*;

const BASIS: (u64, [u8; 32]) = (1, [0x71; 32]);

fn wire(id: &str) -> Vec<u8> {
    let d: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/bridges-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let v = d["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap();
    om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap())
}
fn link(id: &str) -> Link {
    let env = Registry::load_w4()
        .check("ForeignIdentityLink", &wire(id), false)
        .unwrap();
    let Some(Value::Bytes(b)) = env.get(1) else {
        panic!("no body")
    };
    Link::from_body(b).unwrap()
}
fn base() -> Link {
    link("ok-foreignidentitylink")
}

#[test]
fn the_vector_links_decode_and_the_chain_starts_at_version_one() {
    let l = base();
    assert_eq!(
        (l.version, l.prev, l.revoked, l.method),
        (1, None, false, 1)
    );
    assert_eq!(l.basis, BASIS);
    assert_eq!(link("ok-link-bridge-attestation").method, 5);
    assert!(link("ok-link-bridge-attestation").bridge.is_some());
}

#[test]
fn unverified_proof_or_stale_basis_fails_closed() {
    let mut t = LinkTable::new();
    assert_eq!(
        t.accept(base(), false, BASIS),
        Err(LinkError::ProofNotVerified)
    );
    assert_eq!(
        t.accept(base(), true, (2, [0x72; 32])),
        Err(LinkError::StaleBasis)
    );
    t.accept(base(), true, BASIS).unwrap();
    assert_eq!(t.accept(base(), true, BASIS), Err(LinkError::Duplicate));
}

#[test]
fn method_five_needs_a_bridge_and_a_genesis_cannot_carry_prev() {
    let mut t = LinkTable::new();
    let mut l = base();
    l.method = 5;
    l.bridge = None;
    assert_eq!(t.accept(l, true, BASIS), Err(LinkError::MissingBridge));
    let mut g = base();
    g.prev = Some([1; 32]);
    assert_eq!(t.accept(g, true, BASIS), Err(LinkError::UnknownPrev));
    let mut g = base();
    g.version = 2;
    assert_eq!(t.accept(g, true, BASIS), Err(LinkError::BadVersion));
    let mut g = base();
    g.not_before = Some(g.expires_at);
    assert_eq!(t.accept(g, true, BASIS), Err(LinkError::BadWindow));
}

#[test]
fn a_revoked_link_stays_revoked_and_stops_displaying() {
    let mut t = LinkTable::new();
    let g = base();
    t.accept(g.clone(), true, BASIS).unwrap();
    let now = g.expires_at - 1;
    assert!(matches!(
        t.display(&g.account, &g.namespace, &g.foreign_ref, now),
        Display::Linked { .. }
    ));
    let mut rev = g.clone();
    rev.revoked = true;
    rev.version = 2;
    rev.prev = Some(g.hash);
    rev.hash = [0xEE; 32];
    t.accept(rev.clone(), true, BASIS).unwrap();
    assert_eq!(
        t.display(&g.account, &g.namespace, &g.foreign_ref, now),
        Display::NotLinked
    );
    let mut back = g.clone();
    back.version = 3;
    back.prev = Some(rev.hash);
    back.hash = [0xEF; 32];
    assert_eq!(t.accept(back, true, BASIS), Err(LinkError::Resurrected));
}

#[test]
fn an_update_must_chain_to_the_current_head() {
    let mut t = LinkTable::new();
    let g = base();
    t.accept(g.clone(), true, BASIS).unwrap();
    let mut up = g.clone();
    up.version = 2;
    up.prev = Some([9; 32]);
    up.hash = [0xA1; 32];
    assert_eq!(
        t.accept(up.clone(), true, BASIS),
        Err(LinkError::UnknownPrev)
    );
    up.prev = Some(g.hash);
    up.version = 3;
    assert_eq!(
        t.accept(up.clone(), true, BASIS),
        Err(LinkError::BadVersion)
    );
    up.version = 2;
    t.accept(up, true, BASIS).unwrap();
}

#[test]
fn display_respects_the_window_and_a_link_never_yields_an_author() {
    let mut t = LinkTable::new();
    let mut g = base();
    g.not_before = Some(100);
    g.expires_at = 200;
    t.accept(g.clone(), true, BASIS).unwrap();
    assert_eq!(
        t.display(&g.account, &g.namespace, &g.foreign_ref, 99),
        Display::NotLinked
    );
    assert!(matches!(
        t.display(&g.account, &g.namespace, &g.foreign_ref, 100),
        Display::Linked { .. }
    ));
    assert_eq!(
        t.display(&g.account, &g.namespace, &g.foreign_ref, 200),
        Display::NotLinked
    );
    assert_eq!(
        t.display(&g.account, &g.namespace, "someone-else", 150),
        Display::NotLinked
    );
    assert_eq!(native_author(&g), None);
}
