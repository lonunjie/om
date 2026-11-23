// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 2: `LanSignalEnvelopeV1` (ADR-0158 section 3). The construction is replayed against the
//! independent Python vectors; the refusals cover tamper, wrong seed, wrong epoch, replay and nonce reuse.
use om_core::crypto::lan_rendezvous as lr;
use om_core::crypto::lan_signal::{self as ls, Kind, SignalError, SignalGuard};
use serde_json::Value;

fn vectors() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/vectors/lan-rendezvous-vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
fn hx(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn a<const N: usize>(v: &Value) -> [u8; N] {
    hx(v).try_into().unwrap()
}
fn pair_seed(v: &Value) -> lr::LanSeed {
    let p = &v["pair"];
    lr::pair_seed(
        &a(&p["sk"]),
        p["crypto_profile"].as_u64().unwrap(),
        &a(&p["initiator"]),
        &a(&p["responder"]),
    )
}
fn now_in(epoch: u64) -> u64 {
    epoch * 900 + 5
}

#[test]
fn keys_aad_and_envelopes_match_the_independent_vectors() {
    let v = vectors();
    let seed = pair_seed(&v);
    for s in v["signal"].as_array().unwrap() {
        let kind = if s["kind"] == 1 {
            Kind::Offer
        } else {
            Kind::Answer
        };
        let (sid, epoch) = (a::<16>(&s["session_id"]), s["epoch"].as_u64().unwrap());
        assert_eq!(ls::signal_key(&seed, &sid, epoch), a::<32>(&s["key"]));
        assert_eq!(ls::signal_aad(kind, &sid, epoch), hx(&s["aad"]));
        let sdp = s["sdp"].as_str().unwrap();
        let env = ls::seal(
            &seed,
            &mut SignalGuard::default(),
            now_in(epoch),
            kind,
            sid,
            a(&s["nonce"]),
            sdp,
        )
        .unwrap();
        assert_eq!(
            env,
            hx(&s["envelope"]),
            "{}: byte-identical envelope",
            s["name"]
        );
        let (k, got_sid, got) =
            ls::open(&seed, &mut SignalGuard::default(), now_in(epoch), &env).unwrap();
        assert_eq!((k, got_sid, got.as_str()), (kind, sid, sdp));
    }
}

#[test]
fn refusals() {
    let v = vectors();
    let seed = pair_seed(&v);
    let s = &v["signal"][0];
    let env = hx(&s["envelope"]);
    let epoch = s["epoch"].as_u64().unwrap();
    let now = now_in(epoch);
    let open = |seed: &lr::LanSeed, now: u64, b: &[u8]| {
        ls::open(seed, &mut SignalGuard::default(), now, b)
    };

    // Wrong seed (a passive listener without LanSeed) cannot read it.
    let other = lr::pair_seed(&[9; 32], 1, &[1; 16], &[2; 16]);
    assert_eq!(open(&other, now, &env), Err(SignalError::Decrypt));
    // Epoch window: e-1 and e+1 are accepted, e+2 and e-2 are not.
    assert!(open(&seed, now_in(epoch + 1), &env).is_ok());
    assert!(open(&seed, now_in(epoch - 1), &env).is_ok());
    assert_eq!(
        open(&seed, now_in(epoch + 2), &env),
        Err(SignalError::Epoch)
    );
    assert_eq!(
        open(&seed, now_in(epoch - 2), &env),
        Err(SignalError::Epoch)
    );
    // Every single-bit flip of the ciphertext tail or the header fields is refused (never accepted).
    for i in 0..env.len() {
        let mut t = env.clone();
        t[i] ^= 1;
        assert!(
            open(&seed, now, &t).is_err(),
            "bit flip at {i} was accepted"
        );
    }
    // Truncation and trailing bytes.
    assert_eq!(
        open(&seed, now, &env[..env.len() - 1]),
        Err(SignalError::Malformed)
    );
    let mut longer = env.clone();
    longer.push(0);
    assert_eq!(open(&seed, now, &longer), Err(SignalError::Malformed));
    // Replay of a captured envelope on the same guard, and nonce reuse on seal.
    let mut g = SignalGuard::default();
    assert!(ls::open(&seed, &mut g, now, &env).is_ok());
    assert_eq!(
        ls::open(&seed, &mut g, now, &env).unwrap_err(),
        SignalError::Replay
    );
    let mut g = SignalGuard::default();
    let sid = [3u8; 16];
    assert!(ls::seal(&seed, &mut g, now, Kind::Offer, sid, [4; 12], "v=0").is_ok());
    assert_eq!(
        ls::seal(&seed, &mut g, now, Kind::Answer, sid, [4; 12], "v=0"),
        Err(SignalError::Replay)
    );
    assert!(
        ls::seal(&seed, &mut g, now, Kind::Answer, sid, [5; 12], "v=0").is_ok(),
        "a fresh nonce is fine"
    );
    // A forged envelope that fails authentication does not burn the real one.
    let mut g = SignalGuard::default();
    let mut forged = env.clone();
    let last = forged.len() - 1;
    forged[last] ^= 1;
    assert!(ls::open(&seed, &mut g, now, &forged).is_err());
    assert!(ls::open(&seed, &mut g, now, &env).is_ok());
    // Size limits.
    let mut g = SignalGuard::default();
    assert_eq!(
        ls::seal(&seed, &mut g, now, Kind::Offer, sid, [6; 12], ""),
        Err(SignalError::Payload)
    );
    let big = "a".repeat(ls::MAX_SDP + 1);
    assert_eq!(
        ls::seal(&seed, &mut g, now, Kind::Offer, sid, [7; 12], &big),
        Err(SignalError::Payload)
    );
    let max = "a".repeat(ls::MAX_SDP);
    let e = ls::seal(&seed, &mut g, now, Kind::Offer, sid, [8; 12], &max).unwrap();
    assert!(
        open(&seed, now, &e).is_ok(),
        "the largest SDP the CDDL allows round-trips"
    );
}
