// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 1: rendezvous state (ADR-0158 sections 1, 2, 9). Off by default, K fixed tags, adjacent epochs
//! only, no match under a privacy level that turns it off, retirement deletes the seed, rate limit before crypto.
use om_core::api::PrivacyPolicyLevel as L;
use om_core::crypto::lan_rendezvous as lr;
use om_core::lan::{ContactRef, K, LanError, LanRendezvous, RateLimit};

fn seed(n: u8) -> lr::LanSeed {
    lr::pair_seed(&[n; 32], 1, &[n; 16], &[n.wrapping_add(1); 16])
}
fn c(n: u8) -> ContactRef {
    [n; 16]
}
fn rng() -> impl FnMut(&mut [u8]) {
    let mut x = 0x1234_5678u32;
    move |b: &mut [u8]| {
        for v in b.iter_mut() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (x >> 24) as u8;
        }
    }
}
const NOW: u64 = 1_795_000_000;
fn lan(level: Option<L>) -> LanRendezvous {
    LanRendezvous::new(level, RateLimit::new(100, 100))
}

#[test]
fn off_by_default_and_nothing_is_advertised() {
    let mut l = lan(None);
    l.add_seed(c(1), seed(1));
    assert!(!l.is_enabled(&c(1)));
    assert!(l.advertised_tags(NOW, &mut rng()).is_empty());
    let t = lr::tag(&seed(1), lr::epoch_of(NOW));
    assert_eq!(
        l.match_tag(NOW, &t),
        None,
        "a disabled contact never matches"
    );
}

#[test]
fn enabled_contact_advertises_exactly_k_tags_including_its_own() {
    let mut l = lan(Some(L::Recommended));
    l.add_seed(c(1), seed(1));
    l.enable(&c(1)).unwrap();
    let tags = l.advertised_tags(NOW, &mut rng());
    assert_eq!(tags.len(), K);
    assert!(tags.contains(&lr::tag(&seed(1), lr::epoch_of(NOW))));
    assert!(
        tags.windows(2).all(|w| w[0] < w[1]),
        "sorted, no duplicates"
    );
    // The tag set changes with the epoch.
    assert_ne!(tags, l.advertised_tags(NOW + 900, &mut rng()));
}

#[test]
fn more_than_k_contacts_rotate_a_deterministic_window() {
    let mut l = lan(None);
    for n in 1..=12u8 {
        l.add_seed(c(n), seed(n));
        l.enable(&c(n)).unwrap();
    }
    let a = l.advertised_tags(NOW, &mut rng());
    assert_eq!(
        a,
        l.advertised_tags(NOW, &mut rng()),
        "same epoch, same subset"
    );
    assert_eq!(a.len(), K);
    let all: Vec<_> = (1..=12u8)
        .map(|n| lr::tag(&seed(n), lr::epoch_of(NOW)))
        .collect();
    assert!(
        a.iter().all(|t| all.contains(t)),
        "no decoys needed when 12 real tags exist"
    );
    let mut seen = std::collections::BTreeSet::new();
    for e in 0..12u64 {
        seen.extend(l.advertised_tags(NOW + e * 900, &mut rng()));
    }
    assert!(
        seen.len() > 12 * 3,
        "over epochs every contact is eventually advertised"
    );
}

#[test]
fn match_accepts_adjacent_epochs_only() {
    let mut l = lan(None);
    l.add_seed(c(7), seed(7));
    l.enable(&c(7)).unwrap();
    let e = lr::epoch_of(NOW);
    for ep in [e - 1, e, e + 1] {
        assert_eq!(l.match_tag(NOW, &lr::tag(&seed(7), ep)), Some(c(7)));
    }
    for ep in [e - 2, e + 2] {
        assert_eq!(l.match_tag(NOW, &lr::tag(&seed(7), ep)), None);
    }
    assert_eq!(
        l.match_tag(NOW, &lr::tag(&seed(8), e)),
        None,
        "another seed does not match"
    );
}

#[test]
fn hardened_maximum_and_custom_turn_it_off_entirely() {
    for level in [L::Hardened, L::Maximum, L::Custom] {
        let mut l = lan(Some(level));
        l.add_seed(c(1), seed(1));
        assert_eq!(l.enable(&c(1)), Err(LanError::Unavailable));
    }
    // Tightening the level after enabling stops advertising and matching at once.
    let mut l = lan(Some(L::Recommended));
    l.add_seed(c(1), seed(1));
    l.enable(&c(1)).unwrap();
    l.set_level(Some(L::Hardened));
    assert!(l.advertised_tags(NOW, &mut rng()).is_empty());
    assert_eq!(
        l.match_tag(NOW, &lr::tag(&seed(1), lr::epoch_of(NOW))),
        None
    );
    assert!(!l.is_enabled(&c(1)));
}

#[test]
fn retirement_removes_the_contact_and_unknown_contacts_cannot_be_enabled() {
    let mut l = lan(None);
    assert_eq!(l.enable(&c(9)), Err(LanError::UnknownContact));
    l.add_seed(c(1), seed(1));
    l.enable(&c(1)).unwrap();
    assert!(l.retire(&c(1)));
    assert!(!l.retire(&c(1)));
    assert!(l.advertised_tags(NOW, &mut rng()).is_empty());
    assert_eq!(
        l.match_tag(NOW, &lr::tag(&seed(1), lr::epoch_of(NOW))),
        None
    );
}

#[test]
fn the_rate_limit_applies_before_matching() {
    let mut l = LanRendezvous::new(None, RateLimit::new(2, 1));
    l.add_seed(c(1), seed(1));
    l.enable(&c(1)).unwrap();
    let t = lr::tag(&seed(1), lr::epoch_of(NOW));
    assert_eq!(l.match_tag(NOW, &t), Some(c(1)));
    assert_eq!(l.match_tag(NOW, &t), Some(c(1)));
    assert_eq!(
        l.match_tag(NOW, &t),
        None,
        "burst exhausted: refused without computing a tag"
    );
    assert_eq!(l.match_tag(NOW + 1, &t), Some(c(1)), "refills over time");
}
