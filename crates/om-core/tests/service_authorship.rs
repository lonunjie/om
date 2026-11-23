// SPDX-License-Identifier: MPL-2.0
//! `services_bots` slice 2: typed authorship against the ADR-0262 event and grant vectors and the representation rules
//! (SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md sections 9, 10, 15 and 16).
use om_codec::cbor::unhex;
use om_codec::{Registry, Value};
use om_core::service_authorship::*;
use om_core::service_control::*;
use std::collections::{BTreeMap, BTreeSet};

const SVC: [u8; 32] = [0x41; 32];
const ACCOUNT: [u8; 32] = [0x31; 32];
const EP: [u8; 16] = [0x10; 16];

fn doc() -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/services-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn vector(id: &str) -> Vec<u8> {
    let d = doc();
    let v = d["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap_or_else(|| panic!("no vector {id}"));
    unhex(v["wire_hex"].as_str().unwrap())
}
/// The canonical body inside a signed envelope vector.
fn body_of(wire: &[u8], object: &str) -> Vec<u8> {
    match Registry::load_w4()
        .check(object, wire, false)
        .unwrap()
        .get(1)
    {
        Some(Value::Bytes(b)) => b.clone(),
        _ => panic!("no envelope body"),
    }
}

fn chain_with(endpoints: Vec<(u8, u64)>) -> ServiceChain {
    let mut c = ServiceChain::new(SVC);
    let eps: BTreeMap<Vec<u8>, Endpoint> = endpoints
        .into_iter()
        .map(|(id, state)| {
            (
                vec![id; 16],
                Endpoint {
                    id: vec![id; 16],
                    state,
                    profile: 1,
                    key: vec![id; 40],
                    mode: 2,
                    host_ref: None,
                },
            )
        })
        .collect();
    let g = ServiceState {
        service_id: SVC,
        version: 1,
        prev: None,
        controllers: [(1u64, vec![1u8; 32])].into(),
        threshold: 1,
        endpoints: eps,
        hash: [9; 32],
    };
    c.accept(g, &[(1, vec![1; 32])]).unwrap();
    c
}
fn room() -> (u64, Vec<u8>) {
    (2, vec![0x52; 32])
}
fn installs() -> InstallationTable {
    let mut t = InstallationTable::new();
    t.accept(Installation {
        incarnation: vec![0x81; 16],
        service_id: SVC,
        scope: room(),
        mode: 1,
        revoked: false,
        version: 1,
        prev: None,
        hash: [7; 32],
    })
    .unwrap();
    t
}
fn grant() -> Grant {
    Grant::from_body(&body_of(&vector("p11-grant-active"), "RepresentationGrant")).unwrap()
}
fn grants_with(g: Grant) -> GrantTable {
    let mut t = GrantTable::new();
    let dev = g.issuer_device.clone();
    t.accept(g, (&dev, SignerRole::ControlDevice)).unwrap();
    t
}

#[test]
fn the_vector_event_names_exactly_the_vector_grant() {
    let g = grant();
    let ev = Registry::load_w4()
        .check("ConversationEvent", &vector("p16-event-represented"), false)
        .unwrap();
    let Some(Value::Bytes(h)) = ev.get(13).and_then(|r| r.get(1)).and_then(|d| d.get(1)) else {
        panic!("no digest")
    };
    assert_eq!(
        h.as_slice(),
        g.hash.as_slice(),
        "key 13 digest is the grant hash"
    );
    assert_eq!(g.account, ACCOUNT);
    assert_eq!(g.service, SVC);
}

#[test]
fn autonomous_and_represented_events_validate_and_are_always_automated() {
    let c = chain_with(vec![(0x10, ACTIVE)]);
    let ins = installs();
    let gt = grants_with(grant());
    let auto = validate_service_event(
        &vector("p17-event-autonomous-unrepresented"),
        &EP,
        &room(),
        1_900_000_000,
        &c,
        &ins,
        &gt,
    )
    .unwrap();
    assert_eq!(auto.service, SVC);
    assert_eq!(auto.on_behalf_of, None);
    assert!(auto.automated());
    let rep = validate_service_event(
        &vector("p16-event-represented"),
        &EP,
        &room(),
        1_900_000_000,
        &c,
        &ins,
        &gt,
    )
    .unwrap();
    assert_eq!(rep.on_behalf_of, Some(ACCOUNT));
    assert!(
        rep.automated(),
        "a represented event is still service-authored"
    );
}

#[test]
fn a_human_authored_event_is_not_a_service_event() {
    let c = chain_with(vec![(0x10, ACTIVE)]);
    assert_eq!(
        validate_service_event(
            &vector("p21-w0-baseline-without-representation"),
            &EP,
            &room(),
            0,
            &c,
            &installs(),
            &GrantTable::new()
        ),
        Err(AuthorError::NotServiceAuthor)
    );
}

#[test]
fn only_a_usable_endpoint_in_an_installed_room_may_author() {
    let ev = vector("p17-event-autonomous-unrepresented");
    let gt = GrantTable::new();
    // revoked or unknown endpoint
    let revoked = chain_with(vec![(0x10, REVOKED)]);
    assert_eq!(
        validate_service_event(&ev, &EP, &room(), 0, &revoked, &installs(), &gt),
        Err(AuthorError::EndpointNotUsable)
    );
    let c = chain_with(vec![(0x10, ACTIVE)]);
    assert_eq!(
        validate_service_event(&ev, &[0x77; 16], &room(), 0, &c, &installs(), &gt),
        Err(AuthorError::EndpointNotUsable)
    );
    // no installation in that Room, or the installation was revoked
    let other = (2u64, vec![0x99u8; 32]);
    assert_eq!(
        validate_service_event(&ev, &EP, &other, 0, &c, &installs(), &gt),
        Err(AuthorError::NotInstalled)
    );
    let mut t = installs();
    t.accept(Installation {
        incarnation: vec![0x81; 16],
        service_id: SVC,
        scope: room(),
        mode: 1,
        revoked: true,
        version: 2,
        prev: Some([7; 32]),
        hash: [8; 32],
    })
    .unwrap();
    assert_eq!(
        validate_service_event(&ev, &EP, &room(), 0, &c, &t, &gt),
        Err(AuthorError::NotInstalled)
    );
}

#[test]
fn a_represented_event_needs_a_live_scoped_grant() {
    let c = chain_with(vec![(0x10, ACTIVE)]);
    let ins = installs();
    let ev = vector("p16-event-represented");
    // no grant known
    assert_eq!(
        validate_service_event(
            &ev,
            &EP,
            &room(),
            1_900_000_000,
            &c,
            &ins,
            &GrantTable::new()
        ),
        Err(AuthorError::NoGrant)
    );
    // a grant whose issuing control device is later quarantined stops validating
    let mut gt = grants_with(grant());
    let dev = grant().issuer_device;
    gt.device_quarantined(&dev);
    assert_eq!(
        validate_service_event(&ev, &EP, &room(), 1_900_000_000, &c, &ins, &gt),
        Err(AuthorError::NoGrant)
    );
    // a Room outside the grant's scope
    let gt = grants_with(grant());
    let other_room = (2u64, vec![0x53u8; 32]);
    let mut ins2 = InstallationTable::new();
    ins2.accept(Installation {
        incarnation: vec![0x82; 16],
        service_id: SVC,
        scope: other_room.clone(),
        mode: 1,
        revoked: false,
        version: 1,
        prev: None,
        hash: [6; 32],
    })
    .unwrap();
    assert_eq!(
        validate_service_event(&ev, &EP, &other_room, 1_900_000_000, &c, &ins2, &gt),
        Err(AuthorError::OutOfScope)
    );
}

#[test]
fn grant_issuance_rules() {
    let g = grant();
    let dev = g.issuer_device.clone();
    // a messaging-only device cannot issue, nor can a control device other than the one named
    let mut t = GrantTable::new();
    assert_eq!(
        t.accept(g.clone(), (&dev, SignerRole::MessagingDevice)),
        Err(GrantError::Issuer)
    );
    assert_eq!(
        t.accept(g.clone(), (&[0xEE; 16], SignerRole::ControlDevice)),
        Err(GrantError::Issuer)
    );
    t.accept(g.clone(), (&dev, SignerRole::ControlDevice))
        .unwrap();
    // forbidden actions, missing counterparty notice, bad window
    for (mutate, want) in [
        (
            Box::new(|x: &mut Grant| {
                x.actions.insert("account.control".into());
            }) as Box<dyn Fn(&mut Grant)>,
            GrantError::ForbiddenAction,
        ),
        (
            Box::new(|x: &mut Grant| {
                x.actions.insert(READ_COUNTERPARTY.into());
                x.has_notice = false;
            }),
            GrantError::NoCounterpartyNotice,
        ),
        (
            Box::new(|x: &mut Grant| {
                x.not_before = Some(10);
                x.expires_at = Some(10);
            }),
            GrantError::BadWindow,
        ),
    ] {
        let mut bad = g.clone();
        mutate(&mut bad);
        bad.hash = [0xAB; 32];
        bad.incarnation = vec![0x55; 16];
        assert_eq!(
            GrantTable::new().accept(bad, (&dev, SignerRole::ControlDevice)),
            Err(want)
        );
    }
}

#[test]
fn a_messaging_device_may_revoke_and_a_revoked_grant_stays_revoked() {
    let g = grant();
    let dev = g.issuer_device.clone();
    let mut t = grants_with(g.clone());
    assert!(t.live(&g.hash, 1_900_000_000).is_some());
    let revoke = Grant {
        revoked: true,
        version: 2,
        prev: Some(g.hash),
        hash: [0xCD; 32],
        ..g.clone()
    };
    assert_eq!(
        t.accept(revoke.clone(), (&[0x01; 16], SignerRole::Other)),
        Err(GrantError::RevokeSigner)
    );
    t.accept(revoke.clone(), (&[0x01; 16], SignerRole::MessagingDevice))
        .unwrap();
    assert!(t.live(&g.hash, 1_900_000_000).is_none());
    let back = Grant {
        revoked: false,
        version: 3,
        prev: Some(revoke.hash),
        hash: [0xCE; 32],
        ..g
    };
    assert_eq!(
        t.accept(back, (&dev, SignerRole::ControlDevice)),
        Err(GrantError::Resurrected)
    );
}

#[test]
fn service_principals_and_endpoints_never_count_toward_a_human_quorum() {
    let endpoints: BTreeSet<Vec<u8>> = [vec![0x10u8; 16]].into();
    let approvers = vec![
        (1u64, vec![1u8; 32]),
        (2, SVC.to_vec()),
        (1, vec![0x10; 16]),
        (1, vec![2; 32]),
    ];
    assert_eq!(
        human_quorum(&approvers, &endpoints),
        vec![(1u64, vec![1u8; 32]), (1, vec![2; 32])]
    );
}
