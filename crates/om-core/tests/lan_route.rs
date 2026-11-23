// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 3 (route half): the ephemeral `om+lan://v1` RelayEntry (ADR-0158 section 5).
use om_core::crypto::cbor::{V, encode};
use om_core::crypto::delivery;
use om_core::lan_route::{self, LanRouteTable, URI_PREFIX};
use om_core::route::RelayEntry;

const NOW: u64 = 1_795_000_000;

fn rng() -> impl FnMut(&mut [u8]) {
    let mut x = 7u32;
    move |b: &mut [u8]| {
        for v in b.iter_mut() {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (x >> 24) as u8;
        }
    }
}

#[test]
fn the_entry_has_the_profile_shape_and_the_frozen_w0_binding() {
    let mut t = LanRouteTable::default();
    let e = t.open(NOW, 1, &mut rng());
    assert!(e.relay_uri.starts_with(URI_PREFIX));
    assert_eq!(e.relay_uri.len(), URI_PREFIX.len() + 22, "16 bytes in base64url without padding");
    assert_eq!(e.capability.len(), 32);
    assert!(e.is_valid());
    // Expires no later than the end of the current epoch plus one skew epoch.
    let epoch_end = (NOW / 900 + 1) * 900;
    assert!(e.expires_at > NOW && e.expires_at <= epoch_end + 900);
    // The binding is the existing OM-SEALED-DELIVERY-ROUTE-v1 construction, nothing LAN-specific.
    assert_eq!(e.binding(), delivery::route_binding(&e.relay_uri, &e.capability));
    // The transports set rides in key 5 and the plain entry codec still round-trips the rest.
    let v = lan_route::entry_v(&e);
    assert_eq!(v.get(5).unwrap(), &V::A(vec![V::T("webrtc-datachannel-v1".into())]));
    assert_eq!(RelayEntry::from_v(&v).unwrap(), e);
    assert!(encode(&v).len() < 200);
}

#[test]
fn routes_are_unique_and_only_the_exact_pair_is_accepted() {
    let mut t = LanRouteTable::default();
    let mut f = rng();
    let a = t.open(NOW, 1, &mut f);
    let b = t.open(NOW, 1, &mut f);
    assert_ne!(a.relay_uri, b.relay_uri);
    assert_ne!(a.capability, b.capability);
    assert!(t.accepts(NOW, &a.relay_uri, &a.capability));
    assert!(!t.accepts(NOW, &a.relay_uri, &b.capability), "another route's capability does not open this one");
    assert!(!t.accepts(NOW, &b.relay_uri, &a.capability));
    assert!(!t.accepts(NOW, "om+lan://v1/unknown", &a.capability));
    let mut short = a.capability.clone();
    short.pop();
    assert!(!t.accepts(NOW, &a.relay_uri, &short));
}

#[test]
fn routes_die_at_the_epoch_and_at_a_link_change() {
    let mut t = LanRouteTable::default();
    let a = t.open(NOW, 1, &mut rng());
    assert_eq!(t.rotate(NOW + 1, 1), 0, "same epoch, same link: kept");
    assert_eq!(t.rotate(NOW + 1, 2), 1, "the link-layer address changed: destroyed");
    assert!(!t.accepts(NOW, &a.relay_uri, &a.capability));
    let b = t.open(NOW, 1, &mut rng());
    assert_eq!(t.rotate(NOW + 900, 1), 1, "the next epoch: destroyed even if unexpired");
    assert!(!t.accepts(NOW, &b.relay_uri, &b.capability));
    assert!(t.is_empty());
}

#[test]
fn an_expired_route_is_refused_and_clear_destroys_everything() {
    let mut t = LanRouteTable::default();
    let a = t.open(NOW, 1, &mut rng());
    assert!(!t.accepts(a.expires_at, &a.relay_uri, &a.capability), "at expiry it is gone");
    t.open(NOW, 1, &mut rng());
    t.clear();
    assert!(t.is_empty());
}
