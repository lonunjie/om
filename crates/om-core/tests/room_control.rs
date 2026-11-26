// SPDX-License-Identifier: MPL-2.0
//! ADR-0269: a Room trusts a signed service or bridge control state only after every signature verifies against an
//! account's own control head. The body is the frozen ADR-0262 `ServiceControlState` vector, re-signed by test devices.
use om_core::crypto::binding::{Acs, Credential};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::sig::{self, DeviceSigner, ROLE_CONTROLLER, ds_sig};
use om_core::crypto::{PROFILE_V1, cbor};
use om_core::room_control::*;
use om_core::service_control::{ControlOutcome, ServiceChain};
use std::collections::BTreeSet;

/// The account the frozen vector names as its only controller.
const CONTROLLER: [u8; 32] = [0x31; 32];
const DEVICE: [u8; 16] = [7; 16];

fn vector_body() -> Vec<u8> {
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../verification/prototypes/services-wire/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let v = doc["positive"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "p02-service-control-genesis")
        .unwrap();
    let wire = om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap());
    let env = cbor::decode(&wire).unwrap();
    env.get(1).unwrap().b().unwrap().to_vec()
}

struct Fixture {
    signer: DeviceSigner,
    account: ControllerAccount,
}

fn fixture(control: bool) -> Fixture {
    let mut rng = InsecureTestRng::from_label(b"room-control");
    let signer = DeviceSigner::generate(&mut rng);
    let cred = Credential {
        device_id: DEVICE,
        account_id: CONTROLLER,
        signing_pk: signer.public(),
        ik: [9; 32],
        capabilities: vec![],
    };
    let head = Acs {
        account_id: CONTROLLER,
        generation: 1,
        control: if control {
            BTreeSet::from([DEVICE])
        } else {
            BTreeSet::new()
        },
        quarantined: BTreeSet::new(),
        messaging: BTreeSet::from([DEVICE]),
        revoked: BTreeSet::new(),
        credentials: vec![(DEVICE, cred.hash())],
        previous: None,
        constitutional: (1, None),
        recovery_commitment: None,
        min_profile: 1,
        key_epochs: vec![],
        transparency: None,
    };
    Fixture {
        signer,
        account: ControllerAccount {
            head,
            credentials: vec![cred.body()],
        },
    }
}

fn signed(label: &str, body: &[u8], signer: &DeviceSigner, who: &[u8; 16]) -> Vec<u8> {
    let sg = signer.sign_with(
        &ds_sig(label, PROFILE_V1, ROLE_CONTROLLER, who, body),
        [3; 32],
    );
    sig::envelope(
        body.to_vec(),
        vec![sig::signature_entry(who, PROFILE_V1, ROLE_CONTROLLER, sg)],
    )
}

#[test]
fn a_state_signed_by_an_active_control_device_opens_and_feeds_the_chain() {
    let f = fixture(true);
    let body = vector_body();
    let env = signed(LBL_SERVICE_SIG, &body, &f.signer, &DEVICE);
    let opened =
        open_signed_state(ObjectKind::Service, &env, std::slice::from_ref(&f.account)).unwrap();
    assert_eq!(opened.approvers, vec![(KIND_ACCOUNT, CONTROLLER.to_vec())]);
    let mut chain = ServiceChain::new(opened.state.service_id);
    assert_eq!(
        chain.accept(opened.state, &opened.approvers),
        Ok(ControlOutcome::Accepted)
    );
}

#[test]
fn refusals_are_named_and_nothing_is_half_trusted() {
    let f = fixture(true);
    let body = vector_body();
    let good = signed(LBL_SERVICE_SIG, &body, &f.signer, &DEVICE);
    let accounts = [f.account.clone()];

    // The bridge label is a different signature: a service state signed for it does not verify as a bridge object,
    // and a service state signed under the wrong label never verifies as a service.
    let wrong_label = signed(LBL_BRIDGE_SIG, &body, &f.signer, &DEVICE);
    assert_eq!(
        open_signed_state(ObjectKind::Service, &wrong_label, &accounts),
        Err(OpenError::BadSignature)
    );
    // Tampered body.
    let mut tampered = body.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    let env = signed(LBL_SERVICE_SIG, &body, &f.signer, &DEVICE);
    let swapped = {
        let v = cbor::decode(&env).unwrap();
        sig::envelope(tampered, vec![v.get(2).unwrap().a().unwrap()[0].clone()])
    };
    assert!(open_signed_state(ObjectKind::Service, &swapped, &accounts).is_err());
    // A signer that no known account lists as an active control device.
    assert_eq!(
        open_signed_state(ObjectKind::Service, &good, &[]),
        Err(OpenError::UnknownSigner)
    );
    let messaging_only = fixture(false);
    assert_eq!(
        open_signed_state(ObjectKind::Service, &good, &[messaging_only.account]),
        Err(OpenError::UnknownSigner)
    );
    // Not an envelope at all.
    assert_eq!(
        open_signed_state(ObjectKind::Service, &[0xff], &accounts),
        Err(OpenError::Malformed)
    );
}
