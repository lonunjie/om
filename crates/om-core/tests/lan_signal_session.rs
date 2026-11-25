// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 4: the LAN rendezvous seed is exported from the PQXDH secret at session establishment on both
//! sides (ADR-0158 section 2), is persisted, is destroyed on retirement, and seals and opens the signalling envelope
//! (section 3) without ever being handed to a caller.

use om_core::crypto::binding::RelationshipContext;
use om_core::crypto::lan_signal::{Kind, SignalError, SignalGuard};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::{Device, NOW, Provider};

fn pair(label: &str) -> (Device, Device, InsecureTestRng) {
    let mut r = InsecureTestRng::from_label(label.as_bytes());
    let mut bob = Device::new(&mut r);
    let mut prov = Provider::default();
    let up = bob.generate_one_time(3, 3, &mut r);
    prov.publish(&bob, up);
    let mut alice = Device::new(&mut r);
    bob.trust_device(&alice).unwrap();
    let did = bob.device_id;
    let (acs, cred) = prov.view(&did);
    let fetch = prov.fetch(&did).unwrap();
    alice
        .start_session(&fetch, &Device::peer_view(&acs, &cred), &mut r)
        .unwrap();
    let env = alice.encrypt(&bob.ik(), b"hello").unwrap().0;
    bob.decrypt(&RelationshipContext::new(alice.account_id), &env, &mut r)
        .unwrap();
    (alice, bob, r)
}

const SID: [u8; 16] = [7; 16];
const NONCE: [u8; 12] = [3; 12];
const SDP: &str = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\n";

#[test]
fn both_ends_derive_the_same_seed_and_open_each_others_signals() {
    let (alice, bob, _) = pair("lan-signal-pair");
    assert!(alice.has_lan_seed(&bob.ik()) && bob.has_lan_seed(&alice.ik()));
    let (mut ga, mut gb) = (SignalGuard::default(), SignalGuard::default());
    let offer = alice
        .lan_signal_seal(&mut ga, &bob.ik(), NOW, Kind::Offer, SID, NONCE, SDP)
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&offer).contains("IN IP4"),
        "the SDP is not in clear"
    );
    let (k, sid, sdp) = bob
        .lan_signal_open(&mut gb, &alice.ik(), NOW, &offer)
        .unwrap();
    assert_eq!((k, sid, sdp.as_str()), (Kind::Offer, SID, SDP));
    // Replay of the same envelope is refused.
    assert_eq!(
        bob.lan_signal_open(&mut gb, &alice.ik(), NOW, &offer),
        Err(SignalError::Replay)
    );
    // The answer travels the other way under the same seed.
    let answer = bob
        .lan_signal_seal(
            &mut gb,
            &alice.ik(),
            NOW,
            Kind::Answer,
            [8; 16],
            [4; 12],
            "a",
        )
        .unwrap();
    assert_eq!(
        alice
            .lan_signal_open(&mut ga, &bob.ik(), NOW, &answer)
            .unwrap()
            .2,
        "a"
    );
}

#[test]
fn a_third_party_and_an_unknown_peer_get_no_seed_or_no_decrypt() {
    let (alice, bob, mut r) = pair("lan-signal-third");
    let carol = Device::new(&mut r);
    let mut g = SignalGuard::default();
    assert_eq!(
        alice.lan_signal_seal(&mut g, &carol.ik(), NOW, Kind::Offer, SID, NONCE, SDP),
        Err(SignalError::NoSeed)
    );
    // A different relationship's seed cannot open this pair's envelope.
    let (alice2, bob2, _) = pair("lan-signal-other");
    let env = alice
        .lan_signal_seal(&mut g, &bob.ik(), NOW, Kind::Offer, SID, NONCE, SDP)
        .unwrap();
    let mut g2 = SignalGuard::default();
    assert_eq!(
        bob2.lan_signal_open(&mut g2, &alice2.ik(), NOW, &env),
        Err(SignalError::Decrypt)
    );
}

#[test]
fn the_seed_survives_persistence_and_retirement_destroys_it() {
    let (alice, bob, _) = pair("lan-signal-persist");
    let mut g = SignalGuard::default();
    let env = alice
        .lan_signal_seal(&mut g, &bob.ik(), NOW, Kind::Offer, SID, NONCE, SDP)
        .unwrap();
    let mut bob2 = Device::from_bytes(&bob.to_bytes()).unwrap();
    assert_eq!(
        bob2.lan_signal_open(&mut SignalGuard::default(), &alice.ik(), NOW, &env)
            .unwrap()
            .2,
        SDP
    );
    assert!(bob2.retire_lan_seed(&alice.ik()));
    assert!(!bob2.retire_lan_seed(&alice.ik()));
    assert_eq!(
        bob2.lan_signal_open(&mut SignalGuard::default(), &alice.ik(), NOW, &env),
        Err(SignalError::NoSeed)
    );
    // Retired state does not write the key back.
    assert!(
        !Device::from_bytes(&bob2.to_bytes())
            .unwrap()
            .has_lan_seed(&alice.ik())
    );
}

#[test]
fn state_written_before_the_seed_existed_loads_with_no_seed() {
    let (alice, bob, _) = pair("lan-signal-legacy");
    let mut v = bob.to_cbor();
    if let om_core::crypto::cbor::V::M(m) = &mut v {
        m.retain(|(k, _)| *k != 24);
    }
    let back = Device::from_cbor(&v).unwrap();
    assert!(!back.has_lan_seed(&alice.ik()));
}
