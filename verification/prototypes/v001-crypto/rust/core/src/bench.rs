// SPDX-License-Identifier: Apache-2.0
//! V-022 crypto cost measurements. The clock is injected so the same code runs natively,
//! under WASI and inside a JS host (performance.now()).

use crate::rng::{CryptoRng, RngCore};
use crate::store::{Device, Provider};
use serde_json::{json, Value};

fn stats(mut v: Vec<u64>) -> Value {
    v.sort_unstable();
    let n = v.len();
    let us = |ns: u64| (ns as f64 / 100.0).round() / 10.0;
    json!({
        "n": n,
        "median_us": us(v[n / 2]),
        "p95_us": us(v[(n * 95 / 100).min(n - 1)]),
        "min_us": us(v[0]),
        "max_us": us(v[n - 1]),
    })
}

/// Returns timing statistics (microseconds). `now_ns` must be monotonic.
pub fn run<R: RngCore + CryptoRng>(rng: &mut R, now_ns: &dyn Fn() -> u64, setup_iters: usize, msg_iters: usize) -> Value {
    // Prekey generation cost (per one-time EC + PQ prekey pair: X25519 + ML-KEM-768 keygen and two
    // composite Ed25519 + ML-DSA-65 item signatures, plus re-signing the static bundle).
    let mut bob = Device::new(rng);
    let bob_ik = bob.ik();
    let did = bob.device_id;
    let mut prov = Provider::default();
    let mut gen = vec![];
    for _ in 0..setup_iters {
        let t = now_ns();
        let up = bob.generate_one_time(1, 1, rng);
        gen.push(now_ns() - t);
        prov.publish(&bob, up);
    }

    // Session establishment: initiator (verify ACS/credential/dual-signed bundle + items, PQXDH, DR init,
    // first encrypt) and responder (PQXDH incl. ML-KEM re-expansion + decaps, DR init, first decrypt).
    let (acs_b, cred_b) = prov.view(&did);
    let view = Device::peer_view(&acs_b, &cred_b);
    let (mut ta, mut tb, mut tt) = (vec![], vec![], vec![]);
    for _ in 0..setup_iters {
        let mut alice = Device::new(rng);
        let fetch = prov.fetch(&did).expect("bundle");
        let t0 = now_ns();
        alice.start_session(&fetch, &view, rng).expect("initiate");
        let env = alice.encrypt(&bob_ik, b"first message").expect("encrypt").0;
        let t1 = now_ns();
        bob.decrypt(&env, rng).expect("accept");
        let t2 = now_ns();
        ta.push(t1 - t0);
        tb.push(t2 - t1);
        tt.push(t2 - t0);
    }

    // Steady-state messaging on one session, bursts of 3 in alternating directions so the DH ratchet advances.
    bob.sessions.clear();
    let mut alice = Device::new(rng);
    let up = bob.generate_one_time(1, 1, rng);
    prov.publish(&bob, up);
    let b = prov.fetch(&did).unwrap();
    alice.start_session(&b, &view, rng).unwrap();
    let env = alice.encrypt(&bob_ik, b"hello").unwrap().0;
    bob.decrypt(&env, rng).unwrap();
    let alice_ik = alice.ik();
    let (mut enc, mut dec, mut both) = (vec![], vec![], vec![]);
    let payload = [0x42u8; 160];
    for i in 0..msg_iters {
        let a_to_b = (i / 3) % 2 == 0;
        let (s, r, peer) = if a_to_b { (&mut alice, &mut bob, bob_ik) } else { (&mut bob, &mut alice, alice_ik) };
        let t0 = now_ns();
        let env = s.encrypt(&peer, &payload).unwrap().0;
        let t1 = now_ns();
        let (_, pt, _) = r.decrypt(&env, rng).unwrap();
        let t2 = now_ns();
        assert_eq!(pt, payload);
        enc.push(t1 - t0);
        dec.push(t2 - t1);
        both.push(t2 - t0);
    }

    // State serialize + reload of the initiator device holding one active session.
    let (mut ser, mut de) = (vec![], vec![]);
    let mut size = 0;
    for _ in 0..msg_iters.clamp(10, 200) {
        let t0 = now_ns();
        let bytes = alice.to_bytes();
        let t1 = now_ns();
        let back = Device::from_bytes(&bytes).unwrap();
        let t2 = now_ns();
        assert!(back == alice);
        size = bytes.len();
        ser.push(t1 - t0);
        de.push(t2 - t1);
    }
    let sess_bytes = crate::cbor::encode(&alice.sessions.values().next().unwrap().dr.to_cbor()).len();

    json!({
        "prekey_generate_one_time_pq_and_ec": stats(gen),
        "session_setup_initiator": stats(ta),
        "session_setup_responder": stats(tb),
        "session_setup_total": stats(tt),
        "message_encrypt": stats(enc),
        "message_decrypt": stats(dec),
        "message_encrypt_plus_decrypt": stats(both),
        "state_serialize": stats(ser),
        "state_reload": stats(de),
        "device_state_bytes_one_session": size,
        "session_state_bytes": sess_bytes,
        "payload_bytes": payload.len(),
        "setup_iters": setup_iters,
        "msg_iters": msg_iters,
    })
}
