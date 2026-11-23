// SPDX-License-Identifier: MPL-2.0
//! `LanDeliveryProfile-v1` rendezvous vectors (ADR-0158 sections 1 and 2) replayed against
//! `om_core::crypto::lan_rendezvous`. The vectors come from the standard-library Python construction in
//! `verification/prototypes/lan-rendezvous/`, so the two implementations agree byte for byte.
use om_core::crypto::lan_rendezvous as lr;
use serde_json::Value;

fn vectors() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/vectors/lan-rendezvous-vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn a<const N: usize>(v: &Value) -> [u8; N] {
    let s = v.as_str().unwrap();
    let b: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
    b.try_into().unwrap()
}

#[test]
fn seeds_and_tags_match_the_independent_construction() {
    let v = vectors();
    let p = &v["pair"];
    let (sk, ini, resp) = (
        a::<32>(&p["sk"]),
        a::<16>(&p["initiator"]),
        a::<16>(&p["responder"]),
    );
    let prof = p["crypto_profile"].as_u64().unwrap();
    let seed = lr::pair_seed(&sk, prof, &ini, &resp);
    assert_eq!(seed.expose_for_test(), a::<32>(&p["seed"]));
    // The roles and the profile are in the info: swapping them, or another profile, gives another seed.
    assert_eq!(
        lr::pair_seed(&sk, prof, &resp, &ini).expose_for_test(),
        a::<32>(&v["pair_swapped_roles_seed"])
    );
    assert_eq!(
        lr::pair_seed(&sk, prof + 1, &ini, &resp).expose_for_test(),
        a::<32>(&v["pair_other_profile_seed"])
    );
    assert_ne!(
        seed.expose_for_test(),
        a::<32>(&v["pair_swapped_roles_seed"])
    );
    for t in p["tags"].as_array().unwrap() {
        let posix = t["posix"].as_u64().unwrap();
        assert_eq!(lr::epoch_of(posix), t["epoch"].as_u64().unwrap());
        assert_eq!(
            lr::tag(&seed, lr::epoch_of(posix)),
            a::<16>(&t["tag"]),
            "pair tag at {posix}"
        );
    }
    let i = &v["invite"];
    let iseed = lr::invite_seed(&a::<32>(&i["secret"]), &a::<16>(&i["invite_id"]));
    assert_eq!(iseed.expose_for_test(), a::<32>(&i["seed"]));
    for t in i["tags"].as_array().unwrap() {
        let e = t["epoch"].as_u64().unwrap();
        assert_eq!(
            lr::tag(&iseed, e),
            a::<16>(&t["tag"]),
            "invite tag in epoch {e}"
        );
    }
}

#[test]
fn the_accepted_window_is_adjacent_epochs_only() {
    assert_eq!(
        lr::accepted_epochs(1_795_000_000).collect::<Vec<_>>(),
        vec![1_994_443, 1_994_444, 1_994_445]
    );
    assert_eq!(lr::accepted_epochs(0).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(lr::epoch_of(899), 0);
    assert_eq!(lr::epoch_of(900), 1);
}
