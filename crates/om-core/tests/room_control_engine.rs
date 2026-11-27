// SPDX-License-Identifier: MPL-2.0
//! ADR-0269 at the Core boundary: a Room observes a signed service control state only when every signature verifies
//! against a control head Core already holds, and the read-only view reports what was observed.
mod common;

use om_core::crypto::cbor::{self, V};
use om_core::engine::ObserveError;
use om_core::groups::flow::{GroupEvent, GroupIntent};
use om_core::room_control::{ObjectKind, OpenError};
use om_core::runtime::{LoopbackNetwork, Runtime};
use om_core::service_control::ControlOutcome;

/// The frozen ADR-0262 genesis body with its only controller replaced by `account`.
fn body_controlled_by(account: [u8; 32]) -> Vec<u8> {
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
    let env = cbor::decode(&om_codec::cbor::unhex(v["wire_hex"].as_str().unwrap())).unwrap();
    let V::M(mut fields) = cbor::decode(env.get(1).unwrap().b().unwrap()).unwrap() else {
        panic!("body is a map")
    };
    for (k, val) in &mut fields {
        if *k == 5 {
            *val = V::A(vec![V::M(vec![(0, V::U(1)), (1, V::B(account.to_vec()))])]);
        }
    }
    cbor::encode(&V::M(fields))
}

fn create_room(runtime: &mut Runtime, ctx: &om_core::api::ProfileExecutionContext) -> String {
    let (reply, step) = runtime
        .core
        .group_do(
            ctx,
            GroupIntent::Create {
                name: "Room".into(),
                relationship_ids: vec![],
            },
        )
        .unwrap();
    assert!(runtime.settle(&step));
    let reply = reply.unwrap();
    let GroupEvent::Created { conversation_id } = &reply.events[0] else {
        panic!("Room not created")
    };
    om_core::crypto::hex(conversation_id)
}

#[test]
fn a_room_observes_a_state_its_own_account_signed_and_reports_it() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let room = create_room(&mut alice, &a);
    let account = alice.core.debug_account_id(&a).unwrap();
    let body = body_controlled_by(account);
    let kind = ObjectKind::Service;
    let env = alice.core.debug_sign_room_control(&a, kind, &body).unwrap();

    let approvers = vec![(1, account.to_vec())];
    let out = alice
        .core
        .room_control_observe(&a, &room, kind, &env, approvers.clone(), 10)
        .unwrap();
    assert_eq!(out.unwrap().0, ControlOutcome::Accepted);
    let again = alice
        .core
        .room_control_observe(&a, &room, kind, &env, vec![], 40)
        .unwrap();
    assert_eq!(again, Ok((ControlOutcome::Duplicate, None)));
    alice.tick(100_000); // A wall-clock step never changes the observed age.

    let view = alice.core.room_control_view(&a, &room, 50).unwrap();
    assert_eq!(view[0].observed_age_ms, 40);
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].head_version, Some(1));
    assert!(!view[0].frozen);
    assert_eq!(view[0].endpoints.len(), 1);
    assert!(view[0].endpoints[0].2, "the active endpoint is usable");
    assert_eq!(
        view[0].reported_approvers, approvers,
        "the first report is kept, labelled as reported"
    );
    let other = common::create_profile(&mut alice, "p-other", "Other");
    assert!(
        alice
            .core
            .room_control_view(&other, &room, 50)
            .unwrap()
            .is_empty()
    );
    assert!(
        alice
            .core
            .room_control_view(&a, "room-2", 50)
            .unwrap()
            .is_empty()
    );
    alice.core.close_profile(&a.profile_id);
    assert!(alice.core.room_control_view(&a, &room, 50).is_err());
    let reopened = alice.open(&a.profile_id);
    assert!(alice.core.room_control_reader(&reopened, &room).unwrap());
    assert!(
        alice
            .core
            .room_control_view(&reopened, &room, 50)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_state_nobody_known_signed_is_refused_and_leaves_nothing() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    let room = create_room(&mut alice, &a);
    // Bob is not Alice's contact: his signature resolves to no known account.
    let body = body_controlled_by(bob.core.debug_account_id(&b).unwrap());
    let kind = ObjectKind::Service;
    let env = bob.core.debug_sign_room_control(&b, kind, &body).unwrap();
    let out = alice
        .core
        .room_control_observe(&a, &room, kind, &env, vec![], 10)
        .unwrap();
    assert_eq!(out, Err(ObserveError::Open(OpenError::UnknownSigner)));
    assert!(
        alice
            .core
            .room_control_view(&a, &room, 10)
            .unwrap()
            .is_empty()
    );

    // Alice's own signature over a state that names someone else is a valid signature of a non-controller: the chain
    // refuses it for lack of threshold, and nothing is left behind.
    let stranger = body_controlled_by([0x55; 32]);
    let env = alice
        .core
        .debug_sign_room_control(&a, kind, &stranger)
        .unwrap();
    let out = alice
        .core
        .room_control_observe(&a, &room, kind, &env, vec![], 10)
        .unwrap();
    assert!(matches!(out, Err(ObserveError::Control(_))), "{out:?}");
    assert!(
        alice
            .core
            .room_control_view(&a, &room, 10)
            .unwrap()
            .is_empty()
    );
}
