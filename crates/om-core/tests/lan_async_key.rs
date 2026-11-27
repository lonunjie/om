// SPDX-License-Identifier: MPL-2.0
#![cfg(feature = "scope-engines")]

use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;
use om_core::crypto::{PROFILE_V1, delivery};
use om_core::direct::ErrorCode;
use om_core::lan::ContactRef;
use om_core::lan_async_key::{
    FetchGrant, IssueError, MAX_ISSUED, RelationshipState, Reply, Responder,
};
use om_core::lan_frame::{Frame, decode, encode_frame};

const CONTACT: ContactRef = [1; 16];
const OTHER_CONTACT: ContactRef = [2; 16];
const REQUEST_ID: [u8; 16] = [3; 16];
const CAPABILITY: [u8; 32] = [4; 32];
const NOW: u64 = 100;
const EXPIRES: u64 = 200;

struct Fixture {
    device_id: [u8; 16],
    bundle: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let mut rng = InsecureTestRng::from_label(b"lan-async-key-bundle");
        let device = Device::new(&mut rng);
        Self {
            device_id: device.device_id,
            bundle: device.signed_bundle(&mut rng),
        }
    }

    fn request(&self, capability: &[u8]) -> Frame {
        let frame = Frame::FetchAsyncKey {
            request_id: REQUEST_ID,
            device_id: self.device_id,
            profile: PROFILE_V1,
            capability: capability.to_vec(),
        };
        decode(&encode_frame(&frame).unwrap()).unwrap()
    }

    fn issued(&self) -> Responder {
        let mut responder = Responder::new(self.device_id);
        responder.issue(CONTACT, &CAPABILITY, NOW, EXPIRES).unwrap();
        responder
    }

    fn active<'a>(
        &'a self,
        relationship: ContactRef,
        seal_to_requester: &'a mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
    ) -> RelationshipState<'a> {
        RelationshipState::Active {
            relationship,
            fetch_grant: Some(FetchGrant {
                relationship,
                signed_bundle: &self.bundle,
                seal_to_requester,
            }),
        }
    }
}

fn unexpected_seal(_: &[u8]) -> Option<Vec<u8>> {
    panic!("denied fetch invoked the sealer")
}

fn stub_seal(_: &[u8]) -> Option<Vec<u8>> {
    Some(vec![1])
}

fn error_bytes(reply: Option<Reply>) -> Vec<u8> {
    let Some(Reply::Error(frame)) = reply else {
        panic!("expected Unavailable");
    };
    assert_eq!(
        frame,
        Frame::Error {
            id: REQUEST_ID,
            code: ErrorCode::Unavailable as u64,
        }
    );
    encode_frame(&frame).unwrap()
}

#[test]
fn authorized_fetch_returns_the_unchanged_bundle_sealed_to_the_requester() {
    let fixture = Fixture::new();
    let mut responder = fixture.issued();
    let request = fixture.request(&CAPABILITY);
    let mut rng = InsecureTestRng::from_label(b"lan-async-key-reply");
    // Exercise the reply boundary with the existing delivery sealer. No success frame is encoded here.
    let uri = "om+lan://v1/test";
    let route_capability = [7; 32];
    let (seed, key) = delivery::new_key(
        delivery::route_binding(uri, &route_capability),
        EXPIRES,
        &mut rng,
    );
    let reply = responder.handle(
        &request,
        NOW,
        fixture.active(CONTACT, &mut |bundle| {
            assert_eq!(bundle, fixture.bundle.as_slice());
            delivery::seal(
                &key.public_key,
                &key.key_id,
                uri,
                &route_capability,
                key.expires_at,
                bundle,
                &mut rng,
            )
            .ok()
        }),
    );
    let Some(Reply::Bundle {
        request_id,
        sealed_bundle,
    }) = reply
    else {
        panic!("authorized fetch failed");
    };
    assert_eq!(request_id, REQUEST_ID);
    let opened = delivery::open(
        &sealed_bundle,
        &seed,
        &key.key_id,
        uri,
        &route_capability,
        NOW,
    )
    .expect("requester can open the reply");
    assert_eq!(opened.inner_envelope, fixture.bundle);

    error_bytes(responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal)));
}

#[test]
fn all_denied_fetches_have_identical_error_bytes() {
    let fixture = Fixture::new();
    let mut responder = fixture.issued();
    let request = fixture.request(&CAPABILITY);
    let unknown = fixture.request(&[5; 32]);
    let unknown_bytes =
        error_bytes(responder.handle(&unknown, NOW, fixture.active(CONTACT, &mut unexpected_seal)));
    assert_eq!(
        decode(&unknown_bytes).unwrap(),
        Frame::Error {
            id: REQUEST_ID,
            code: 1
        }
    );

    for (frame, now, state) in [
        (&request, NOW, RelationshipState::Inactive),
        (
            &request,
            NOW,
            RelationshipState::Active {
                relationship: CONTACT,
                fetch_grant: None,
            },
        ),
    ] {
        assert_eq!(
            error_bytes(responder.handle(frame, now, state)),
            unknown_bytes
        );
    }
    for (relationship, now) in [(OTHER_CONTACT, NOW), (CONTACT, EXPIRES)] {
        let reply = responder.handle(
            &request,
            now,
            fixture.active(relationship, &mut unexpected_seal),
        );
        assert_eq!(error_bytes(reply), unknown_bytes);
    }
    let reply = responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal));
    assert_eq!(error_bytes(reply), unknown_bytes);

    let mut responder = fixture.issued();
    let reply = responder.handle(&request, NOW, RelationshipState::Blocked(CONTACT));
    assert_eq!(error_bytes(reply), unknown_bytes);
    let reply = responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal));
    assert_eq!(error_bytes(reply), unknown_bytes);

    let mut responder = fixture.issued();
    assert!(matches!(
        responder.handle(&request, NOW, fixture.active(CONTACT, &mut stub_seal)),
        Some(Reply::Bundle { .. })
    ));
    let reply = responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal));
    assert_eq!(error_bytes(reply), unknown_bytes);
}

#[test]
fn sealing_failure_consumes_the_capability() {
    let fixture = Fixture::new();
    let mut responder = fixture.issued();
    let request = fixture.request(&CAPABILITY);
    let mut called = false;
    let failed = error_bytes(responder.handle(
        &request,
        NOW,
        fixture.active(CONTACT, &mut |_: &[u8]| {
            called = true;
            None
        }),
    ));
    assert!(called);
    let replay = responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal));
    assert_eq!(error_bytes(replay), failed);
}

#[test]
fn mismatched_target_and_profile_are_refused_without_sealing() {
    let fixture = Fixture::new();
    let mut responder = fixture.issued();
    for (device_id, profile) in [([0; 16], PROFILE_V1), (fixture.device_id, PROFILE_V1 + 1)] {
        let frame = Frame::FetchAsyncKey {
            request_id: REQUEST_ID,
            device_id,
            profile,
            capability: CAPABILITY.to_vec(),
        };
        error_bytes(responder.handle(&frame, NOW, fixture.active(CONTACT, &mut unexpected_seal)));
    }
    error_bytes(responder.handle(
        &fixture.request(&CAPABILITY),
        NOW,
        RelationshipState::Active {
            relationship: CONTACT,
            fetch_grant: Some(FetchGrant {
                relationship: OTHER_CONTACT,
                signed_bundle: &fixture.bundle,
                seal_to_requester: &mut unexpected_seal,
            }),
        },
    ));
    assert!(matches!(
        responder.handle(
            &fixture.request(&CAPABILITY),
            NOW,
            fixture.active(CONTACT, &mut stub_seal)
        ),
        Some(Reply::Bundle { .. })
    ));
}

#[test]
fn full_table_refuses_issuance_and_reclaims_oldest_expired_first() {
    let mut responder = Responder::new([0; 16]);
    assert_eq!(MAX_ISSUED, 64);
    for slot in 0..MAX_ISSUED as u8 {
        let expiry = match slot {
            0 => EXPIRES + 1,
            1 => EXPIRES,
            _ => EXPIRES + 10,
        };
        responder.issue(CONTACT, &[slot; 32], NOW, expiry).unwrap();
    }
    assert_eq!(
        responder.issue(OTHER_CONTACT, &[64; 32], NOW, EXPIRES),
        Err(IssueError::Full)
    );
    assert_eq!(
        responder.issue(OTHER_CONTACT, &[0; 32], NOW, EXPIRES),
        Err(IssueError::AlreadyIssued)
    );

    let now = EXPIRES + 1;
    responder
        .issue(OTHER_CONTACT, &[64; 32], now, EXPIRES + 10)
        .unwrap();
    // A deliberate duplicate probes which expired digest is still in the table.
    assert_eq!(
        responder.issue(CONTACT, &[1; 32], now, EXPIRES + 10),
        Err(IssueError::AlreadyIssued)
    );
    responder
        .issue(OTHER_CONTACT, &[65; 32], now, EXPIRES + 10)
        .unwrap();
    assert_eq!(
        responder.issue(OTHER_CONTACT, &[66; 32], now, EXPIRES + 10),
        Err(IssueError::Full)
    );
}

#[test]
fn retirement_and_end_destroy_only_that_relationships_capabilities() {
    let fixture = Fixture::new();
    let mut responder = fixture.issued();
    let second_capability = [6; 32];
    let other_capability = [7; 32];
    responder
        .issue(CONTACT, &second_capability, NOW, EXPIRES)
        .unwrap();
    responder
        .issue(OTHER_CONTACT, &other_capability, NOW, EXPIRES)
        .unwrap();
    responder.retire(&CONTACT);
    for cap in [CAPABILITY, second_capability] {
        let reply = responder.handle(
            &fixture.request(&cap),
            NOW,
            fixture.active(CONTACT, &mut unexpected_seal),
        );
        error_bytes(reply);
    }

    assert!(matches!(
        responder.handle(
            &fixture.request(&other_capability),
            NOW,
            fixture.active(OTHER_CONTACT, &mut stub_seal)
        ),
        Some(Reply::Bundle { .. })
    ));

    for state in [
        RelationshipState::Ended(CONTACT),
        RelationshipState::Retired(CONTACT),
    ] {
        let mut responder = fixture.issued();
        let request = fixture.request(&CAPABILITY);
        error_bytes(responder.handle(&request, NOW, state));
        error_bytes(responder.handle(&request, NOW, fixture.active(CONTACT, &mut unexpected_seal)));
    }
}
