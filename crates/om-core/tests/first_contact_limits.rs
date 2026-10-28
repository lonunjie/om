// SPDX-License-Identifier: MPL-2.0
//! First-contact resource and lifetime limits on the inviter side (audit 2026-10-28: M4, H2 admission cost).

mod common;

use common::{Parts, create_profile, intros, invite, parts, send_request};
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::runtime::{LoopbackNetwork, Runtime};

const DAY: u64 = 24 * 60 * 60;

fn honest(p: &Parts) -> Parts {
    Parts {
        acs: p.acs.clone(),
        creds: p.creds.clone(),
        chain: p.chain.clone(),
    }
}

#[test]
fn a_requesters_bootstrap_key_cannot_outlive_the_bootstrap_window() {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    let a = create_profile(&mut alice, "p-alice", "Alice");
    let b = create_profile(&mut bob, "p-bob", "Bob");
    let (_, bob_desc) = invite(&mut bob, &b, "bob-inv");
    let bob_parts = parts(&bob_desc);
    let now_ms = alice.clock.get();
    let mut rng = InsecureTestRng::from_label(b"m4");
    let (link, desc) = invite(&mut alice, &a, "inv-1");
    // 90 days is the lifetime of an established delivery key; a request-scoped bootstrap key may live only 7 days
    // (profile section 7: the request deadline is min(route, key)).
    for ttl in [90 * DAY, 30 * DAY, 7 * DAY + 2 * 3600] {
        send_request(
            &net,
            &mut rng,
            &link,
            &desc,
            honest(&bob_parts),
            now_ms,
            ttl,
        );
        assert!(
            intros(&mut alice, &a).is_empty(),
            "a bootstrap key valid for {ttl}s must be refused"
        );
    }
    // Exactly 7 days (what an honest Core mints) is accepted, with one hour of clock skew allowed.
    send_request(
        &net,
        &mut rng,
        &link,
        &desc,
        honest(&bob_parts),
        now_ms,
        7 * DAY + 1800,
    );
    assert_eq!(
        intros(&mut alice, &a).len(),
        1,
        "the 7-day bootstrap key is accepted"
    );
}
