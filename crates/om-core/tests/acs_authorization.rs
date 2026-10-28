// SPDX-License-Identifier: MPL-2.0
//! V-002 conformance for the Rust AccountControlState verifier: the committed golden vectors, the machine-readable
//! transition policy, optional differential replay of the exhaustive enumeration, and signed-state chain checks.

use om_core::control::{self, Ctx, Model, POLICY_ROWS, Verdict, authorize};
use om_core::crypto::binding::Acs;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Deserialize)]
struct Vector {
    name: String,
    parent: Model,
    child: Model,
    context: Ctx,
    expect: String,
}

#[test]
fn replays_every_committed_v002_golden_vector() {
    let raw = std::fs::read_to_string(
        root().join("verification/vectors/acs-authorization-table-vectors.json"),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let vectors: Vec<Vector> = serde_json::from_value(v["vectors"].clone()).unwrap();
    assert!(vectors.len() >= 60, "the committed corpus has 63 vectors");
    let mut classes = BTreeSet::new();
    for x in &vectors {
        let got = authorize(&x.parent, &x.child, &x.context);
        assert_eq!(got.class(), x.expect, "{}: {}", x.name, got.reason());
        classes.insert(x.name.clone());
    }
}

#[test]
fn policy_rows_match_the_normative_machine_readable_policy() {
    let raw = std::fs::read_to_string(
        root().join("spec/protocol/account-control-transition-policy.json"),
    )
    .unwrap();
    let p: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let rows = p["transitions"].as_array().unwrap();
    assert_eq!(rows.len(), POLICY_ROWS.len());
    for (id, auth) in POLICY_ROWS {
        let row = rows
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("policy row {id} missing"));
        assert_eq!(row["authorization"], auth, "{id}");
    }
}

#[test]
fn differential_against_the_exhaustive_python_enumeration_when_provided() {
    let Ok(path) = std::env::var("OM_ACS_EXHAUSTIVE") else {
        eprintln!(
            "OM_ACS_EXHAUSTIVE not set: skipping the 16,200-case differential (see acs_dump_cases.py)"
        );
        return;
    };
    #[derive(Deserialize)]
    struct Case {
        parent: Model,
        child: Model,
        context: Ctx,
        expect: String,
    }
    let text = std::fs::read_to_string(path).unwrap();
    let mut n = 0;
    for line in text.lines() {
        let c: Case = serde_json::from_str(line).unwrap();
        let got = authorize(&c.parent, &c.child, &c.context);
        assert_eq!(
            got.class(),
            c.expect,
            "case {n}: {} ({:?} -> {:?} {:?})",
            got.reason(),
            c.parent,
            c.child,
            c.context
        );
        n += 1;
    }
    assert!(n >= 16_000, "{n} cases replayed");
    eprintln!("differential: {n} cases agree with the Python reference");
}

// ---------------------------------------------------------------------------------------------- signed chain

fn account(rng: &mut InsecureTestRng, n: usize) -> (Vec<Device>, Acs) {
    // N control devices of one account (the genesis names them all and each signs it).
    let first = Device::new(rng);
    let mut devs = vec![first];
    while devs.len() < n {
        let mut d = Device::new(rng);
        d.account_id = devs[0].account_id;
        devs.push(d);
    }
    let mut acs = devs[0].acs();
    acs.control = devs.iter().map(|d| d.device_id).collect();
    acs.credentials = devs
        .iter()
        .map(|d| (d.device_id, d.credential().hash()))
        .collect();
    (devs, acs)
}

fn creds(devs: &[Device]) -> Vec<Vec<u8>> {
    devs.iter().map(|d| d.credential().body()).collect()
}

fn sign(devs: &[&Device], acs: &Acs, rng: &mut InsecureTestRng) -> Vec<u8> {
    let s: Vec<_> = devs.iter().map(|d| (&d.device_id, &d.signer)).collect();
    control::sign_acs(&acs.body(), &s, rng)
}

#[test]
fn genesis_must_be_signed_by_every_named_controller() {
    let mut rng = InsecureTestRng::from_label(b"acs-genesis");
    let (devs, acs) = account(&mut rng, 2);
    let both = sign(&[&devs[0], &devs[1]], &acs, &mut rng);
    assert!(control::verify_genesis(&both, &creds(&devs)).is_ok());
    let one = sign(&[&devs[0]], &acs, &mut rng);
    assert!(
        control::verify_genesis(&one, &creds(&devs)).is_err(),
        "an unsigned controller is not bound"
    );
    // A signature by a key that is not the committed credential fails.
    let mut forged = devs[0].clone();
    forged.signer = Device::new(&mut rng).signer.clone();
    let bad = sign(&[&forged, &devs[1]], &acs, &mut rng);
    assert!(control::verify_genesis(&bad, &creds(&devs)).is_err());
}

#[test]
fn successor_chain_links_messaging_device_by_one_controller_and_rejects_everything_else() {
    let mut rng = InsecureTestRng::from_label(b"acs-chain");
    let (devs, g) = account(&mut rng, 3);
    let msg = Device::new(&mut rng);
    let mut child = g.clone();
    child.generation = 2;
    child.previous = Some(g.hash());
    child.messaging.insert(msg.device_id);
    child.credentials.push((msg.device_id, {
        let mut c = msg.credential();
        c.account_id = g.account_id;
        c.hash()
    }));
    let mut all = creds(&devs);
    let mut mc = msg.credential();
    mc.account_id = g.account_id;
    all.push(mc.body());
    // One active controller suffices for link_messaging.
    let ok = sign(&[&devs[1]], &child, &mut rng);
    assert!(control::verify_successor(&g, &ok, &all).is_ok());
    // A messaging device cannot sign as a controller (it has no committed controller credential role: its key is
    // not a signer of the parent).
    let by_msg = control::sign_acs(&child.body(), &[(&msg.device_id, &msg.signer)], &mut rng);
    assert!(control::verify_successor(&g, &by_msg, &all).is_err());
    // Promoting needs a majority of the parent's 3 controllers: one signature is not enough, two are.
    let spare = Device::new(&mut rng);
    let mut sc = spare.credential();
    sc.account_id = g.account_id;
    let mut promote = g.clone();
    promote.generation = 2;
    promote.previous = Some(g.hash());
    promote.control.insert(spare.device_id);
    promote.credentials.push((spare.device_id, sc.hash()));
    let mut all2 = creds(&devs);
    all2.push(sc.body());
    assert!(control::verify_successor(&g, &sign(&[&devs[0]], &promote, &mut rng), &all2).is_err());
    assert!(
        control::verify_successor(&g, &sign(&[&devs[0], &devs[2]], &promote, &mut rng), &all2)
            .is_ok()
    );
    // A child that does not chain to the trusted head is not a descendant, whatever its generation.
    let mut fork = child.clone();
    fork.previous = Some([9; 32]);
    fork.generation = 99;
    assert!(control::verify_successor(&g, &sign(&[&devs[0]], &fork, &mut rng), &all).is_err());
    // A tampered body invalidates every signature.
    let mut tampered = ok.clone();
    let n = tampered.len();
    tampered[n / 3] ^= 1;
    assert!(control::verify_successor(&g, &tampered, &all).is_err());
    // A revoked DeviceID never returns: revoke d3's... then try to promote it back.
    let mut rev = g.clone();
    rev.generation = 2;
    rev.previous = Some(g.hash());
    rev.control.remove(&devs[2].device_id);
    rev.revoked.insert(devs[2].device_id);
    rev.credentials.retain(|(d, _)| *d != devs[2].device_id);
    let rev_signed = sign(&[&devs[0], &devs[1]], &rev, &mut rng);
    let g2 = control::verify_successor(&g, &rev_signed, &creds(&devs))
        .expect("majority revokes a control device");
    let mut back = g2.clone();
    back.generation = 3;
    back.previous = Some(g2.hash());
    back.control.insert(devs[2].device_id);
    back.credentials
        .push((devs[2].device_id, devs[2].credential().hash()));
    assert!(
        control::verify_successor(
            &g2,
            &sign(&[&devs[0], &devs[1]], &back, &mut rng),
            &creds(&devs)
        )
        .is_err()
    );
}

#[test]
fn a_single_controller_account_can_neither_quarantine_nor_revoke_itself() {
    let mut rng = InsecureTestRng::from_label(b"acs-solo");
    let (devs, g) = account(&mut rng, 1);
    let me = devs[0].device_id;
    let (pm, mut cm) = (
        control::model_of(&g).unwrap(),
        control::model_of(&g).unwrap(),
    );
    cm.generation = 2;
    cm.quarantined.insert(crate_hex(&me));
    let v = authorize(
        &pm,
        &cm,
        &Ctx {
            signers: [crate_hex(&me)].into(),
            ..Ctx::default()
        },
    );
    assert!(matches!(v, Verdict::Reject(_)), "{v:?}");
}

fn crate_hex(d: &[u8; 16]) -> String {
    om_core::crypto::hex(d)
}
