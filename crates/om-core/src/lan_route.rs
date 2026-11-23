// SPDX-License-Identifier: MPL-2.0
//! The ephemeral local route of `LanDeliveryProfile-v1` (ADR-0158 section 5). After a channel is established the receiver
//! creates a `RelayEntry` used only inside the current private relationship or bootstrap context:
//! `relay_uri = "om+lan://v1/" + base64url(route_token)` (16 random bytes, no padding), a 32-byte random capability,
//! `expires_at` no later than the end of the current rendezvous epoch plus one skew epoch, and `transports =
//! ["webrtc-datachannel-v1"]`. The URI names a route in this table only; it is never globally resolvable and never goes
//! into a public RelaySet. A `SealedDeliveryKeyRecord` for the route uses the existing W0 binding
//! (`RelayEntry::binding`), so 1:1 traffic is an ordinary `SealedDeliveryEnvelope` and a LAN route never authenticates a
//! sender. Sans-IO: the host passes `now` and a byte filler. Routes die at an epoch change or a link-layer address change.

use crate::crypto::cbor::V;
use crate::crypto::lan_rendezvous::{self as lr, EPOCH_SECONDS};
use crate::flows::b64url;
use crate::route::RelayEntry;
use zeroize::Zeroize;

pub const URI_PREFIX: &str = "om+lan://v1/";
pub const TRANSPORT: &str = "webrtc-datachannel-v1";
pub const TOKEN_LEN: usize = 16;
pub const CAPABILITY_LEN: usize = 32;

struct Route {
    entry: RelayEntry,
    epoch: u64,
    link: u64,
}

/// Local-session table of live LAN routes.
#[derive(Default)]
pub struct LanRouteTable {
    routes: Vec<Route>,
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// `relay-entry` CBOR including the `transports` set (key 5), which `RelayEntry::to_v` does not carry.
pub fn entry_v(e: &RelayEntry) -> V {
    V::M(vec![
        (0, V::T(e.relay_uri.clone())),
        (1, V::B(e.capability.clone())),
        (2, V::U(u64::from(e.priority))),
        (4, V::U(e.expires_at)),
        (5, V::A(vec![V::T(TRANSPORT.into())])),
    ])
}

impl LanRouteTable {
    /// Creates a fresh route bound to the epoch of `now` and the current link identity `link` (any host value that
    /// changes when the link-layer address does). Expires at the end of the next epoch (current epoch plus one skew epoch).
    pub fn open(&mut self, now: u64, link: u64, fill: &mut dyn FnMut(&mut [u8])) -> RelayEntry {
        let epoch = lr::epoch_of(now);
        let mut token = [0u8; TOKEN_LEN];
        let mut cap = [0u8; CAPABILITY_LEN];
        fill(&mut token);
        fill(&mut cap);
        let uri = format!("{URI_PREFIX}{}", b64url(&token));
        let expires = (epoch + 2) * EPOCH_SECONDS;
        let entry = RelayEntry::new(&uri, &cap, expires).expect("a 16-byte token and 32-byte capability form a valid entry");
        token.zeroize();
        cap.zeroize();
        self.routes.push(Route {
            entry: entry.clone(),
            epoch,
            link,
        });
        entry
    }

    /// Drops every route from an earlier epoch or another link, zeroizing the capabilities. Call at each epoch boundary and
    /// whenever the link-layer address changes. Returns how many were destroyed.
    pub fn rotate(&mut self, now: u64, link: u64) -> usize {
        let epoch = lr::epoch_of(now);
        let before = self.routes.len();
        self.routes.retain_mut(|r| {
            let live = r.epoch == epoch && r.link == link && r.entry.expires_at > now;
            if !live {
                r.entry.capability.zeroize();
            }
            live
        });
        before - self.routes.len()
    }

    /// Whether `(relay_uri, capability)` names a live route: the deposit check a LAN receiver makes before accepting a
    /// sealed envelope. Constant-time on the capability. It says nothing about who is sending.
    pub fn accepts(&self, now: u64, relay_uri: &str, capability: &[u8]) -> bool {
        self.routes
            .iter()
            .any(|r| r.entry.relay_uri == relay_uri && ct_eq(&r.entry.capability, capability) && r.entry.expires_at > now)
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// Destroys every route (relationship end, block, or the facility being turned off).
    pub fn clear(&mut self) {
        for r in &mut self.routes {
            r.entry.capability.zeroize();
        }
        self.routes.clear();
    }
}
