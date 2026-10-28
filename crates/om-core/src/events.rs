// SPDX-License-Identifier: MPL-2.0
//! Canonical 1:1 `ConversationEvent`s (object type 10, spec/cddl/events.cddl, EVENT_MODEL.md). An event is not
//! separately signed: the 1:1 session AEAD authenticates it. Its canonical bytes are what the ratchet carries, what
//! dedupe hashes and what multi-device sync and archive will replay, so they are built and checked in one place.

use crate::crypto::cbor::{self, V};
use crate::crypto::sha256;

pub const PROTOCOL_VERSION: u64 = 1;
pub const KIND_VERSION: u64 = 1;
/// The inner-payload discriminator of an event is its object type (map key 0 = 10).
pub const OBJECT_TYPE: u64 = 10;

pub const K_CREATE: u64 = 1;
pub const K_EDIT: u64 = 2;
pub const K_DELETE: u64 = 3;
pub const K_REACTION: u64 = 4;
pub const K_RECEIPT: u64 = 5;

/// Principal kind 1 = account.
const PRINCIPAL_ACCOUNT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hlc {
    pub physical_ms: u64,
    pub logical: u32,
}

impl Hlc {
    /// Next clock value for a local event: strictly after `prev` and not behind the wall clock.
    pub fn next(prev: Hlc, now_ms: u64) -> Hlc {
        if now_ms > prev.physical_ms {
            Hlc {
                physical_ms: now_ms,
                logical: 0,
            }
        } else {
            Hlc {
                physical_ms: prev.physical_ms,
                logical: prev.logical.saturating_add(1),
            }
        }
    }
}

/// RichTextV1 for plain text: one paragraph block holding one text inline.
pub fn rich_text(text: &str) -> V {
    V::M(vec![
        (0, V::U(1)),
        (
            1,
            V::A(vec![V::M(vec![
                (0, V::U(1)),
                (
                    1,
                    V::A(vec![V::M(vec![(0, V::U(1)), (1, V::T(text.to_string()))])]),
                ),
            ])]),
        ),
    ])
}

/// Plain text of a RichTextV1 value. Only semantic data is read: links and mentions contribute their label text and
/// nothing is ever interpreted as markup. Blocks are joined with newlines.
pub fn plain_text(rt: &V) -> Option<String> {
    fn inlines(v: &V, out: &mut String, depth: u32) -> Option<()> {
        if depth > 6 {
            return None;
        }
        for i in v.a().ok()? {
            match i.get(0).ok()?.u().ok()? {
                1 => out.push_str(i.get(1).ok()?.t().ok()?),
                2 | 3 => inlines(i.get(4).ok()?, out, depth + 1)?,
                _ => return None,
            }
        }
        Some(())
    }
    if rt.get(0).ok()?.u().ok()? != 1 {
        return None;
    }
    let mut parts = vec![];
    for b in rt.get(1).ok()?.a().ok()? {
        let mut s = String::new();
        match b.get(0).ok()?.u().ok()? {
            1 | 2 => inlines(b.get(1).ok()?, &mut s, 0)?,
            3 => s.push_str(b.get(2).ok()?.t().ok()?),
            _ => return None,
        }
        parts.push(s);
    }
    Some(parts.join("\n"))
}

pub struct Header {
    pub event_id: [u8; 16],
    pub conversation_id: [u8; 32],
    pub author_account: [u8; 32],
    pub author_device: [u8; 16],
    pub clock: Hlc,
    pub kind: u64,
    pub body: V,
}

impl Header {
    /// Canonical W0 bytes. `endpoint_tiebreak` is the authenticated endpoint (device id), per ADR-0121.
    pub fn encode(&self) -> Vec<u8> {
        cbor::encode(&V::M(vec![
            (0, V::U(OBJECT_TYPE)),
            (1, V::U(1)),
            (2, V::U(PROTOCOL_VERSION)),
            (3, V::B(self.event_id.to_vec())),
            (4, V::B(self.conversation_id.to_vec())),
            (
                5,
                V::M(vec![
                    (0, V::U(PRINCIPAL_ACCOUNT)),
                    (1, V::B(self.author_account.to_vec())),
                ]),
            ),
            (6, V::B(self.author_device.to_vec())),
            (
                7,
                V::M(vec![
                    (0, V::U(self.clock.physical_ms)),
                    (1, V::U(self.clock.logical as u64)),
                    (2, V::B(self.author_device.to_vec())),
                ]),
            ),
            (8, V::U(self.kind)),
            (9, V::U(KIND_VERSION)),
            (11, self.body.clone()),
        ]))
    }
}

pub fn message_create_body(text: &str) -> V {
    V::M(vec![(0, rich_text(text)), (1, V::A(vec![]))])
}

/// A parsed, structurally valid inbound event. Authorization (author, conversation) is checked by the caller.
#[derive(Clone, Debug)]
pub struct Event {
    pub event_id: [u8; 16],
    pub conversation_id: [u8; 32],
    pub author_account: [u8; 32],
    pub author_device: [u8; 16],
    pub clock: Hlc,
    pub kind: u64,
    pub body: V,
    /// SHA-256 of the exact canonical bytes (equivocation detection).
    pub digest: [u8; 32],
}

/// Parse after the W0 registry check has passed. Returns `None` for anything the 1:1 profile does not accept.
pub fn parse(bytes: &[u8]) -> Option<Event> {
    let v = cbor::decode(bytes).ok()?;
    if v.get(0).ok()?.u().ok()? != OBJECT_TYPE || v.get(2).ok()?.u().ok()? != PROTOCOL_VERSION {
        return None;
    }
    // A required capability or a kind version this client does not implement is refused, never half-applied.
    if v.opt(10).is_some() || v.get(9).ok()?.u().ok()? != KIND_VERSION || v.opt(23).is_some() {
        return None;
    }
    let principal = v.get(5).ok()?;
    if principal.get(0).ok()?.u().ok()? != PRINCIPAL_ACCOUNT {
        return None;
    }
    let clock = v.get(7).ok()?;
    let device: [u8; 16] = v.get(6).ok()?.b().ok()?.try_into().ok()?;
    // The clock's tiebreak must equal the authenticated endpoint (ADR-0121); it is never authority.
    if clock.get(2).ok()?.b().ok()? != device {
        return None;
    }
    Some(Event {
        event_id: v.get(3).ok()?.b().ok()?.try_into().ok()?,
        conversation_id: v.get(4).ok()?.b32().ok()?,
        author_account: principal.get(1).ok()?.b32().ok()?,
        author_device: device,
        clock: Hlc {
            physical_ms: clock.get(0).ok()?.u().ok()?,
            logical: clock.get(1).ok()?.u32().ok()?,
        },
        kind: v.get(8).ok()?.u().ok()?,
        body: v.get(11).ok()?.clone(),
        digest: sha256(&[bytes]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_round_trips_and_event_bytes_are_w0_valid() {
        let h = Header {
            event_id: [1; 16],
            conversation_id: [2; 32],
            author_account: [3; 32],
            author_device: [4; 16],
            clock: Hlc {
                physical_ms: 1_790_000_000_000,
                logical: 7,
            },
            kind: K_CREATE,
            body: message_create_body("héllo\nworld"),
        };
        let bytes = h.encode();
        om_codec::Registry::load()
            .check("ConversationEvent", &bytes, false)
            .expect("W0-valid ConversationEvent");
        let e = parse(&bytes).unwrap();
        assert_eq!(e.clock.logical, 7);
        assert_eq!(
            plain_text(e.body.get(0).unwrap()).as_deref(),
            Some("héllo\nworld")
        );
    }

    #[test]
    fn hlc_is_monotonic_even_when_the_wall_clock_steps_back() {
        let a = Hlc::next(
            Hlc {
                physical_ms: 0,
                logical: 0,
            },
            100,
        );
        let b = Hlc::next(a, 50);
        let c = Hlc::next(b, 100);
        assert!(a < b && b < c);
    }
}
